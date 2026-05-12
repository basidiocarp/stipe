use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use colored::Colorize;
use serde_json::json;

use crate::commands::host_policy;
use crate::commands::install::{self, InstallProfile};

#[derive(Debug, Clone, PartialEq, Eq)]
struct PackageBackup {
    original: PathBuf,
    backup: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RollbackFailure {
    original: PathBuf,
    backup: PathBuf,
    error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct RollbackSummary {
    restored: Vec<PathBuf>,
    skipped_existing_original: Vec<PathBuf>,
    skipped_missing_backup: Vec<PathBuf>,
    failures: Vec<RollbackFailure>,
}

impl RollbackSummary {
    fn has_issues(&self) -> bool {
        !self.failures.is_empty()
            || !self.skipped_existing_original.is_empty()
            || !self.skipped_missing_backup.is_empty()
    }
}

#[derive(Debug)]
struct BackupPreparationFailure {
    error: anyhow::Error,
    backups: Vec<PackageBackup>,
    rollback: RollbackSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PackageSurface {
    Claude,
    Codex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LamellaInvocation {
    subcommand: &'static str,
    args: &'static [&'static str],
    surfaces: &'static [PackageSurface],
}

impl LamellaInvocation {
    fn claude_install() -> Self {
        Self {
            subcommand: "install",
            args: &["--all", "--force"],
            surfaces: &[PackageSurface::Claude],
        }
    }

    fn codex_install() -> Self {
        Self {
            subcommand: "install-codex",
            args: &["--all", "--force"],
            surfaces: &[PackageSurface::Codex],
        }
    }
}

pub fn run(profile: Option<InstallProfile>, dry_run: bool) -> Result<()> {
    let profile = resolve_profile(profile);
    let lamella_invocations = lamella_invocations(profile);

    print_repair_header(&profile, &lamella_invocations);

    if lamella_invocations.is_empty() {
        println!("  - none for {}", profile.mode_label());
        println!(
            "{}",
            "No package repair surface is defined for this profile yet.".dimmed()
        );
        return Ok(());
    }

    let lamella_root = locate_lamella_root()?;
    let targets = package_state_targets(&lamella_invocations);

    print_lamella_config(&lamella_root, &lamella_invocations);

    if dry_run {
        print_dry_run_targets(&targets);
        return Ok(());
    }

    run_repair_with_backups(profile, &lamella_root, &lamella_invocations, &targets)
}

fn print_repair_header(profile: &InstallProfile, _invocations: &[LamellaInvocation]) {
    println!();
    println!("{}", "Package Repair".bold());
    println!("{}", "─".repeat(75));
    println!("Profile: {}", profile.profile_name());
    println!("Lamella invocation(s):");
}

fn print_lamella_config(lamella_root: &Path, invocations: &[LamellaInvocation]) {
    println!(
        "Lamella source: {}",
        host_policy::format_user_path(lamella_root)
    );
    for invocation in invocations {
        println!("  - {}", lamella_command_string(invocation));
    }
}

fn print_dry_run_targets(targets: &[PathBuf]) {
    for target in targets {
        println!(
            "Would back up {} before package install.",
            host_policy::format_user_path(target)
        );
    }
}

fn run_repair_with_backups(
    profile: InstallProfile,
    lamella_root: &Path,
    lamella_invocations: &[LamellaInvocation],
    targets: &[PathBuf],
) -> Result<()> {
    let backups = match prepare_backups(targets) {
        Ok(backups) => backups,
        Err(failure) => {
            handle_backup_failure(profile, lamella_root, lamella_invocations, &failure)?;
            return Err(anyhow!("backup preparation failed"));
        }
    };

    let status = run_lamella_install(lamella_root, lamella_invocations);

    match status {
        Ok(()) => handle_repair_success(profile, lamella_root, lamella_invocations, &backups),
        Err(error) => handle_repair_failure(profile, lamella_root, lamella_invocations, &backups, error),
    }
}

fn handle_backup_failure(
    profile: InstallProfile,
    lamella_root: &Path,
    lamella_invocations: &[LamellaInvocation],
    failure: &BackupPreparationFailure,
) -> Result<()> {
    if !failure.backups.is_empty() {
        for line in rollback_summary_lines(&failure.rollback) {
            println!("{line}");
        }
    }
    let failure_message = format_backup_preparation_failure_message(
        &failure.error,
        failure.backups.len(),
        &failure.rollback,
    );
    append_audit_log_best_effort(&build_audit_event(
        "failed",
        profile,
        lamella_root,
        lamella_invocations,
        &failure.backups,
        if failure.backups.is_empty() {
            None
        } else {
            Some(&failure.rollback)
        },
        Some(&failure_message),
    ));
    Ok(())
}

fn handle_repair_success(
    profile: InstallProfile,
    lamella_root: &Path,
    lamella_invocations: &[LamellaInvocation],
    backups: &[PackageBackup],
) -> Result<()> {
    let backup_paths = backups
        .iter()
        .map(|backup| host_policy::format_user_path(&backup.backup))
        .collect::<Vec<_>>();

    append_audit_log_best_effort(&build_audit_event(
        "success",
        profile,
        lamella_root,
        lamella_invocations,
        backups,
        None,
        None,
    ));

    if backup_paths.is_empty() {
        println!("No existing package state required backup.");
    } else {
        println!("Backups created:");
        for path in backup_paths {
            println!("  - {path}");
        }
        println!(
            "{}",
            "Rollback target: rename backup paths back to their original locations."
                .dimmed()
        );
    }
    println!("{}", "Package repair completed.".green());
    Ok(())
}

fn handle_repair_failure(
    profile: InstallProfile,
    lamella_root: &Path,
    lamella_invocations: &[LamellaInvocation],
    backups: &[PackageBackup],
    error: anyhow::Error,
) -> Result<()> {
    let rollback = rollback_backups(backups);
    for line in rollback_summary_lines(&rollback) {
        println!("{line}");
    }
    let error_string = error.to_string();
    append_audit_log_best_effort(&build_audit_event(
        "failed",
        profile,
        lamella_root,
        lamella_invocations,
        backups,
        Some(&rollback),
        Some(&error_string),
    ));
    let failure_message = format_failed_package_repair_message(&error, &rollback);
    Err(anyhow!(failure_message))
}

pub(crate) fn supports_profile(profile: InstallProfile) -> bool {
    !lamella_invocations(profile).is_empty()
}

fn resolve_profile(profile: Option<InstallProfile>) -> InstallProfile {
    let saved_profile = install::load_saved_profile().map(|saved| saved.profile);
    let detected_clients = crate::ecosystem::clients::detect_clients()
        .into_iter()
        .map(|client| client.name().to_string())
        .collect::<Vec<_>>();
    let preferred_profile = host_policy::preferred_install_profile(None, &detected_clients);

    resolve_profile_from_inputs(profile, saved_profile, preferred_profile)
}

fn resolve_profile_from_inputs(
    profile: Option<InstallProfile>,
    saved_profile: Option<InstallProfile>,
    preferred_profile: InstallProfile,
) -> InstallProfile {
    if let Some(profile) = profile {
        return profile;
    }
    if let Some(saved_profile) = saved_profile {
        return saved_profile;
    }
    preferred_profile
}

fn locate_lamella_root() -> Result<PathBuf> {
    let project_root = host_policy::project_root()
        .context("unable to determine project root for package repair")?;
    for candidate in lamella_root_candidates(&project_root) {
        if candidate.join("lamella").exists() && candidate.join("resources").exists() {
            return Ok(candidate);
        }
    }

    Err(anyhow!(
        "could not locate Lamella package source under {}",
        host_policy::format_user_path(&project_root)
    ))
}

fn lamella_root_candidates(project_root: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![project_root.join("lamella"), project_root.to_path_buf()];
    if let Some(parent) = project_root.parent() {
        candidates.push(parent.join("lamella"));
    }
    candidates
}

fn lamella_invocations(profile: InstallProfile) -> Vec<LamellaInvocation> {
    match profile {
        InstallProfile::Codex => vec![LamellaInvocation::codex_install()],
        InstallProfile::ClaudeCode => vec![LamellaInvocation::claude_install()],
        InstallProfile::Cursor
        | InstallProfile::Minimal
        | InstallProfile::Standard
        | InstallProfile::FullStack
        | InstallProfile::DeveloperTools => Vec::new(),
    }
}

fn lamella_command_string(invocation: &LamellaInvocation) -> String {
    if invocation.args.is_empty() {
        format!("./lamella {}", invocation.subcommand)
    } else {
        format!(
            "./lamella {} {}",
            invocation.subcommand,
            invocation.args.join(" ")
        )
    }
}

fn package_state_targets(invocations: &[LamellaInvocation]) -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    package_state_targets_with_home(&home, invocations)
}

fn package_state_targets_with_home(home: &Path, invocations: &[LamellaInvocation]) -> Vec<PathBuf> {
    let surfaces = invocations
        .iter()
        .flat_map(|invocation| invocation.surfaces.iter().copied())
        .collect::<BTreeSet<_>>();
    let mut targets = BTreeSet::new();

    for surface in surfaces {
        for target in package_state_targets_for_surface(home, surface) {
            targets.insert(target);
        }
    }

    targets.into_iter().collect()
}

fn package_state_targets_for_surface(home: &Path, surface: PackageSurface) -> Vec<PathBuf> {
    match surface {
        PackageSurface::Claude => vec![
            home.join(".claude").join("plugins"),
            home.join(".claude").join("rules"),
            home.join(".claude").join("workflows"),
            home.join(".claude").join("templates"),
        ],
        PackageSurface::Codex => vec![
            home.join(".codex").join("skills"),
            home.join(".codex").join("agents"),
        ],
    }
}

#[allow(clippy::result_large_err)]
fn prepare_backups(
    targets: &[PathBuf],
) -> std::result::Result<Vec<PackageBackup>, BackupPreparationFailure> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    prepare_backups_with_timestamp(targets, timestamp)
}

#[allow(clippy::result_large_err)]
fn prepare_backups_with_timestamp(
    targets: &[PathBuf],
    timestamp: u64,
) -> std::result::Result<Vec<PackageBackup>, BackupPreparationFailure> {
    prepare_backups_under_root(targets, timestamp, &backup_root())
}

#[allow(clippy::result_large_err)]
fn prepare_backups_under_root(
    targets: &[PathBuf],
    timestamp: u64,
    root: &Path,
) -> std::result::Result<Vec<PackageBackup>, BackupPreparationFailure> {
    let mut backups = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        if !target.exists() {
            continue;
        }

        let backup = backup_path_under(root, target, timestamp, index);
        if let Some(parent) = backup.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                let error = anyhow!(error).context(format!(
                    "failed to create backup directory {}",
                    host_policy::format_user_path(parent)
                ));
                let rollback = rollback_backups(&backups);
                return Err(BackupPreparationFailure {
                    error,
                    backups,
                    rollback,
                });
            }
        }
        if let Err(error) = fs::rename(target, &backup) {
            let error = anyhow!(error).context(format!(
                "failed to back up {} to {}",
                host_policy::format_user_path(target),
                host_policy::format_user_path(&backup)
            ));
            let rollback = rollback_backups(&backups);
            return Err(BackupPreparationFailure {
                error,
                backups,
                rollback,
            });
        }
        backups.push(PackageBackup {
            original: target.clone(),
            backup,
        });
    }

    Ok(backups)
}

/// Returns the backup root directory for `package_repair` backups.
/// Reads `STIPE_BACKUP_DIR` (matching the convention in `stipe::backup`), falling
/// back to `~/.local/share/stipe/backups`.
fn backup_root() -> PathBuf {
    backup_root_from(std::env::var("STIPE_BACKUP_DIR").ok().as_deref())
}

/// Pure decision function for the backup root, taking the env override as a
/// parameter so tests can call it without mutating process-global state.
fn backup_root_from(env_override: Option<&str>) -> PathBuf {
    if let Some(override_path) = env_override {
        return PathBuf::from(override_path);
    }
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("stipe")
        .join("backups")
}

/// Converts a path to a flattened name suitable for storage in the backup root.
/// E.g. `/Users/me/.claude/rules` → `Users-me-.claude-rules`.
/// Strips `..` segments so a flattened name cannot escape the backup root via
/// `Path::join`. Callers should also avoid passing untrusted paths here, but
/// this guard means even a `..` slip can't construct an escaping destination.
fn flatten_path_for_storage(path: &Path) -> String {
    use std::path::Component;
    let mut parts: Vec<String> = Vec::new();
    for component in path.components() {
        if let Component::Normal(s) = component {
            let segment: String = s
                .to_string_lossy()
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '.' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            if !segment.is_empty() && segment != "." {
                parts.push(segment);
            }
            // RootDir, Prefix (Windows), CurDir, and ParentDir (`..`) are skipped —
            // only Normal segments matter; `..` is never preserved so a malformed
            // input cannot escape the bucket via Path::join.
        }
    }
    if parts.is_empty() {
        // Fallback for empty/edge-case paths.
        "state".to_string()
    } else {
        parts.join("-")
    }
}

/// Pure path computation taking an explicit root, so tests can call it with a
/// temp directory without mutating process-global state.
fn backup_path_under(root: &Path, path: &Path, timestamp: u64, index: usize) -> PathBuf {
    // Index is included in the bucket name to guarantee uniqueness within a
    // single prepare_backups_with_timestamp call: even if two targets flatten
    // to the same name (e.g. via different special-character mappings), they
    // land in different buckets and cannot overwrite each other.
    let bucket = format!("{timestamp}-{index}-pre-package-repair");
    let flattened = flatten_path_for_storage(path);
    root.join(&bucket).join(&flattened)
}

fn rollback_backups(backups: &[PackageBackup]) -> RollbackSummary {
    let mut summary = RollbackSummary::default();
    for backup in backups.iter().rev() {
        if !backup.backup.exists() {
            summary.skipped_missing_backup.push(backup.backup.clone());
            continue;
        }
        if backup.original.exists() {
            summary
                .skipped_existing_original
                .push(backup.original.clone());
            continue;
        }
        match fs::rename(&backup.backup, &backup.original) {
            Ok(()) => summary.restored.push(backup.original.clone()),
            Err(err) => summary.failures.push(RollbackFailure {
                original: backup.original.clone(),
                backup: backup.backup.clone(),
                error: err.to_string(),
            }),
        }
    }
    summary
}

fn rollback_summary_lines(summary: &RollbackSummary) -> Vec<String> {
    let mut lines = vec!["Rollback summary:".to_string()];

    if summary.restored.is_empty() {
        lines.push("  restored: none".to_string());
    } else {
        lines.push("  restored:".to_string());
        lines.extend(
            summary
                .restored
                .iter()
                .map(|path| format!("    - {}", host_policy::format_user_path(path))),
        );
    }

    if !summary.skipped_existing_original.is_empty() {
        lines.push("  skipped (original path already exists):".to_string());
        lines.extend(
            summary
                .skipped_existing_original
                .iter()
                .map(|path| format!("    - {}", host_policy::format_user_path(path))),
        );
    }

    if !summary.failures.is_empty() {
        lines.push("  restore failures:".to_string());
        lines.extend(summary.failures.iter().map(|failure| {
            format!(
                "    - {} (backup: {}) -> {}",
                host_policy::format_user_path(&failure.original),
                host_policy::format_user_path(&failure.backup),
                failure.error
            )
        }));
    }

    if !summary.skipped_missing_backup.is_empty() {
        lines.push("  skipped (backup artifact missing):".to_string());
        lines.extend(
            summary
                .skipped_missing_backup
                .iter()
                .map(|path| format!("    - {}", host_policy::format_user_path(path))),
        );
    }

    if summary.has_issues() {
        lines.push(
            "Manual inspection recommended: verify package state and keep backup artifacts until confirmed."
                .to_string(),
        );
    } else {
        lines.push("Rollback completed without restore conflicts.".to_string());
    }

    lines
}

fn format_failed_package_repair_message(
    error: &anyhow::Error,
    rollback: &RollbackSummary,
) -> String {
    if rollback.has_issues() {
        format!(
            "Lamella install failed: {error}. Rollback encountered conflicts; inspect rollback summary and backup artifacts."
        )
    } else {
        format!("Lamella install failed: {error}. Rollback completed.")
    }
}

fn format_backup_preparation_failure_message(
    error: &anyhow::Error,
    backup_count: usize,
    rollback: &RollbackSummary,
) -> String {
    if backup_count == 0 {
        return format!(
            "Package backup step failed before Lamella install started: {error}. No package state was moved."
        );
    }
    if rollback.has_issues() {
        format!(
            "Package backup step failed before Lamella install started: {error}. Attempted rollback encountered conflicts; inspect rollback summary and backup artifacts."
        )
    } else {
        format!(
            "Package backup step failed before Lamella install started: {error}. Attempted rollback restored moved package state."
        )
    }
}

fn run_lamella_install(lamella_root: &Path, invocations: &[LamellaInvocation]) -> Result<()> {
    let lamella_bin = lamella_root.join("lamella");
    if !lamella_bin.exists() {
        return Err(anyhow!(
            "Lamella executable not found at {}",
            host_policy::format_user_path(&lamella_bin)
        ));
    }

    for invocation in invocations {
        let output = Command::new(&lamella_bin)
            .arg(invocation.subcommand)
            .args(invocation.args)
            .current_dir(lamella_root)
            .output()
            .with_context(|| {
                format!(
                    "failed to run {}",
                    lamella_command_string(invocation)
                        .replace("./lamella", &host_policy::format_user_path(&lamella_bin))
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return Err(anyhow!(
                "{} failed: {} {}",
                lamella_command_string(invocation),
                stdout,
                stderr
            ));
        }
    }

    Ok(())
}

fn build_audit_event(
    status: &str,
    profile: InstallProfile,
    lamella_root: &Path,
    lamella_invocations: &[LamellaInvocation],
    backups: &[PackageBackup],
    rollback: Option<&RollbackSummary>,
    error: Option<&str>,
) -> serde_json::Value {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup_items = backups
        .iter()
        .map(|backup| {
            json!({
                "original": backup.original,
                "backup": backup.backup,
            })
        })
        .collect::<Vec<_>>();

    let rollback_value = rollback.map(|summary| {
        json!({
            "restored": summary.restored,
            "skipped_existing_original": summary.skipped_existing_original,
            "skipped_missing_backup": summary.skipped_missing_backup,
            "failures": summary.failures.iter().map(|failure| {
                json!({
                    "original": failure.original,
                    "backup": failure.backup,
                    "error": failure.error,
                })
            }).collect::<Vec<_>>(),
        })
    });

    json!({
        "timestamp_unix": timestamp,
        "action": "package-repair",
        "status": status,
        "profile": profile.profile_name(),
        "lamella_root": lamella_root,
        "lamella_invocations": lamella_invocations
            .iter()
            .map(lamella_command_string)
            .collect::<Vec<_>>(),
        "backups": backup_items,
        "rollback_target": "rename backup paths back to their original package state paths",
        "rollback": rollback_value,
        "error": error,
    })
}

fn append_audit_log(entry: &serde_json::Value) -> Result<()> {
    let log_path = package_audit_log_path();
    append_audit_log_with_path(entry, &log_path)
}

fn append_audit_log_best_effort(entry: &serde_json::Value) {
    if let Err(error) = append_audit_log(entry) {
        eprintln!(
            "{}",
            format!("Warning: failed to write package audit log: {error}").yellow()
        );
    }
}

#[cfg(test)]
fn append_audit_log_best_effort_with_path(
    entry: &serde_json::Value,
    log_path: &Path,
) -> Option<String> {
    append_audit_log_with_path(entry, log_path)
        .err()
        .map(|error| format!("Warning: failed to write package audit log: {error}"))
}

fn package_audit_log_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("stipe")
        .join("package-audit.log")
}

fn append_audit_log_with_path(entry: &serde_json::Value, log_path: &Path) -> Result<()> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .with_context(|| {
            format!(
                "failed to open audit log at {}",
                host_policy::format_user_path(log_path)
            )
        })?;
    writeln!(file, "{}", serde_json::to_string(entry)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_lamella_invocations_for_codex_profile() {
        let invocations = lamella_invocations(InstallProfile::Codex);
        assert_eq!(invocations, vec![LamellaInvocation::codex_install()]);
        assert_eq!(
            lamella_command_string(&invocations[0]),
            "./lamella install-codex --all --force"
        );
    }

    #[test]
    fn test_lamella_invocations_for_full_profile_include_both_surfaces() {
        let invocations = lamella_invocations(InstallProfile::FullStack);
        assert!(invocations.is_empty());
    }

    #[test]
    fn test_package_targets_for_codex_profile_stay_on_codex_surface() {
        let home = PathBuf::from("/tmp/home");
        let targets =
            package_state_targets_with_home(&home, &lamella_invocations(InstallProfile::Codex));
        assert_eq!(
            targets,
            vec![
                PathBuf::from("/tmp/home/.codex/agents"),
                PathBuf::from("/tmp/home/.codex/skills"),
            ]
        );
    }

    #[test]
    fn test_package_targets_for_claude_profile_stay_on_claude_surface() {
        let home = PathBuf::from("/tmp/home");
        let targets = package_state_targets_with_home(
            &home,
            &lamella_invocations(InstallProfile::ClaudeCode),
        );
        assert_eq!(
            targets,
            vec![
                PathBuf::from("/tmp/home/.claude/plugins"),
                PathBuf::from("/tmp/home/.claude/rules"),
                PathBuf::from("/tmp/home/.claude/templates"),
                PathBuf::from("/tmp/home/.claude/workflows"),
            ]
        );
    }

    #[test]
    fn test_cursor_profile_has_no_package_repair_surface() {
        assert!(lamella_invocations(InstallProfile::Cursor).is_empty());
    }

    #[test]
    fn test_supports_profile_only_for_host_package_surfaces() {
        assert!(supports_profile(InstallProfile::ClaudeCode));
        assert!(supports_profile(InstallProfile::Codex));
        assert!(!supports_profile(InstallProfile::FullStack));
        assert!(!supports_profile(InstallProfile::Standard));
    }

    #[test]
    fn test_resolve_profile_prefers_explicit_then_saved_then_detected_default() {
        assert_eq!(
            resolve_profile_from_inputs(
                Some(InstallProfile::ClaudeCode),
                Some(InstallProfile::Codex),
                InstallProfile::Cursor,
            ),
            InstallProfile::ClaudeCode
        );
        assert_eq!(
            resolve_profile_from_inputs(None, Some(InstallProfile::Codex), InstallProfile::Cursor),
            InstallProfile::Codex
        );
        assert_eq!(
            resolve_profile_from_inputs(None, None, InstallProfile::Cursor),
            InstallProfile::Cursor
        );
    }

    #[test]
    fn test_lamella_root_candidates_include_workspace_sibling() {
        let project_root = PathBuf::from("/tmp/basidiocarp/stipe");
        let candidates = lamella_root_candidates(&project_root);

        assert_eq!(
            candidates,
            vec![
                PathBuf::from("/tmp/basidiocarp/stipe/lamella"),
                PathBuf::from("/tmp/basidiocarp/stipe"),
                PathBuf::from("/tmp/basidiocarp/lamella"),
            ]
        );
    }

    #[test]
    fn test_flatten_path_for_storage_converts_slashes_and_special_chars() {
        let path = PathBuf::from("/Users/me/.claude/rules");
        let flattened = flatten_path_for_storage(&path);
        assert_eq!(flattened, "Users-me-.claude-rules");

        let path2 = PathBuf::from("/home/user/.config@v2/file");
        let flattened2 = flatten_path_for_storage(&path2);
        assert_eq!(flattened2, "home-user-.config_v2-file");

        // Dots are preserved
        let path3 = PathBuf::from("/Users/test.user/.config");
        let flattened3 = flatten_path_for_storage(&path3);
        assert_eq!(flattened3, "Users-test.user-.config");
    }

    #[test]
    fn test_backup_path_under_places_backup_under_explicit_root() {
        // Use explicit root rather than backup_path_in_root() / backup_root() so this
        // test doesn't race with sibling tests that set STIPE_BACKUP_DIR.
        let root = PathBuf::from("/var/lib/stipe-test/backups");
        let path = PathBuf::from("/tmp/example");
        let backup = backup_path_under(&root, &path, 1234, 2);
        assert!(
            backup.starts_with(&root),
            "backup path should be under the explicit root: {}",
            backup.display()
        );
        assert!(
            backup
                .to_string_lossy()
                .contains("1234-2-pre-package-repair"),
            "backup path should include timestamp+index bucket: {}",
            backup.display()
        );
        assert!(
            backup.to_string_lossy().contains("tmp-example"),
            "backup path should include flattened original: {}",
            backup.display()
        );
        // Confirm it's NOT a sibling of /tmp/example
        assert!(
            backup.parent().is_none_or(|p| p != Path::new("/tmp")),
            "backup should not be a sibling of the original target"
        );
    }

    #[test]
    fn test_rollback_decisions_restore_and_preserve_expected_paths() {
        let base = temp_test_dir("rollback-decisions");
        fs::create_dir_all(&base).expect("create test dir");

        let restored_original = base.join("restore-target");
        let restored_backup = base.join("restore-target.bak");
        fs::write(&restored_backup, "backup").expect("write backup");

        let existing_original = base.join("existing-target");
        let existing_backup = base.join("existing-target.bak");
        fs::write(&existing_original, "original").expect("write original");
        fs::write(&existing_backup, "backup").expect("write existing backup");

        let missing_original = base.join("missing-target");
        let missing_backup = base.join("missing-target.bak");

        let backups = vec![
            PackageBackup {
                original: restored_original.clone(),
                backup: restored_backup.clone(),
            },
            PackageBackup {
                original: existing_original.clone(),
                backup: existing_backup.clone(),
            },
            PackageBackup {
                original: missing_original,
                backup: missing_backup.clone(),
            },
        ];

        let summary = rollback_backups(&backups);

        assert_eq!(summary.restored, vec![restored_original.clone()]);
        assert_eq!(
            summary.skipped_existing_original,
            vec![existing_original.clone()]
        );
        assert_eq!(summary.skipped_missing_backup, vec![missing_backup]);
        assert!(summary.failures.is_empty());

        assert!(restored_original.exists());
        assert!(!restored_backup.exists());
        assert!(existing_original.exists());
        assert!(existing_backup.exists());

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn test_prepare_backups_creates_backups_under_backup_root_not_sibling_of_original() {
        let base = temp_test_dir("backup-root-not-sibling");
        let backup_root_dir = temp_test_dir("backup-root-custom");
        fs::create_dir_all(&base).expect("create test dir");
        fs::create_dir_all(&backup_root_dir).expect("create backup root dir");

        let target = base.join("rules");
        fs::create_dir_all(&target).expect("create target");
        fs::write(target.join("file.txt"), "content").expect("write test file");

        // Use the explicit-root API so we don't have to mutate process-global env state.
        let backups =
            prepare_backups_under_root(std::slice::from_ref(&target), 12345, &backup_root_dir)
                .expect("backup should succeed");

        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].original, target);

        // Original should now be gone (moved to backup)
        assert!(!target.exists());

        // Backup should exist under the backup root, not as a sibling
        let backup_path = &backups[0].backup;
        assert!(backup_path.exists());
        assert!(
            backup_path.starts_with(&backup_root_dir),
            "backup {} should be under backup root {}",
            backup_path.display(),
            backup_root_dir.display()
        );
        assert!(
            backup_path
                .to_string_lossy()
                .contains("12345-0-pre-package-repair"),
            "backup path should include timestamp + index bucket: {}",
            backup_path.display()
        );

        // Confirm no *.stipe-backup-* siblings in the original parent dir
        if let Ok(entries) = fs::read_dir(&base) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                assert!(
                    !name_str.contains(".stipe-backup-"),
                    "found unexpected sibling backup: {name_str}",
                );
            }
        }

        let _ = fs::remove_dir_all(base);
        let _ = fs::remove_dir_all(backup_root_dir);
    }

    #[cfg(not(windows))]
    #[test]
    fn test_prepare_backups_index_separates_targets_with_colliding_flattened_names() {
        // Two targets that flatten to the same name (after special-character normalization)
        // must still produce distinct backup paths via the per-index bucket.
        let base = temp_test_dir("backup-index-collision");
        let backup_root_dir = temp_test_dir("backup-root-collision");
        fs::create_dir_all(&base).expect("create test dir");
        fs::create_dir_all(&backup_root_dir).expect("create backup root dir");

        // Names that flatten identically: "rules!" and "rules?" both → "rules_"
        let first = base.join("rules!");
        let second = base.join("rules?");
        fs::create_dir_all(&first).expect("create first");
        fs::create_dir_all(&second).expect("create second");
        fs::write(first.join("a"), "a").expect("write a");
        fs::write(second.join("b"), "b").expect("write b");

        let backups =
            prepare_backups_under_root(&[first.clone(), second.clone()], 999, &backup_root_dir)
                .expect("backup should succeed for both");

        assert_eq!(backups.len(), 2);
        assert_ne!(
            backups[0].backup, backups[1].backup,
            "colliding flattened names must land in distinct buckets via index"
        );
        assert!(backups[0].backup.exists());
        assert!(backups[1].backup.exists());
        // Both files preserved under their respective backups
        assert!(backups[0].backup.join("a").exists() || backups[0].backup.join("b").exists());
        assert!(backups[1].backup.join("a").exists() || backups[1].backup.join("b").exists());

        let _ = fs::remove_dir_all(base);
        let _ = fs::remove_dir_all(backup_root_dir);
    }

    #[test]
    fn test_flatten_path_strips_parent_dir_segments() {
        use std::path::PathBuf;
        // ../etc/passwd must NOT round-trip via Path::join into something that escapes the bucket.
        let result = flatten_path_for_storage(&PathBuf::from("../etc/passwd"));
        assert!(
            !result.contains(".."),
            "flattened name must not contain '..': got {result}"
        );
        // The result should still be deterministic and non-empty.
        assert!(!result.is_empty());
    }

    #[test]
    fn test_backup_root_from_uses_override_when_provided() {
        let pb = backup_root_from(Some("/tmp/my-backup-root"));
        assert_eq!(pb, std::path::PathBuf::from("/tmp/my-backup-root"));
    }

    #[test]
    fn test_backup_root_from_falls_back_when_override_absent() {
        let pb = backup_root_from(None);
        // Default ends with "stipe/backups" regardless of platform.
        let s = pb.to_string_lossy();
        assert!(
            s.ends_with("stipe/backups") || s.ends_with("stipe\\backups"),
            "got {s}"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn test_prepare_backups_roll_back_partial_state_when_later_backup_fails() {
        let base = temp_test_dir("backup-rollback-on-failure");
        let backup_root_dir = temp_test_dir("backup-root-custom");
        fs::create_dir_all(&base).expect("create test dir");
        fs::create_dir_all(&backup_root_dir).expect("create backup root dir");

        let first = base.join("first");
        let second = base.join("second");
        fs::create_dir_all(&first).expect("create first target");
        fs::create_dir_all(&second).expect("create second target");

        // Seed the second backup location to cause a conflict (dir-where-file-expected).
        let second_backup = backup_path_under(&backup_root_dir, &second, 42, 1);
        if let Some(parent) = second_backup.parent() {
            fs::create_dir_all(parent).expect("create second backup parent");
        }
        fs::write(&second_backup, "occupied").expect("seed conflicting backup path");

        let err =
            prepare_backups_under_root(&[first.clone(), second.clone()], 42, &backup_root_dir)
                .expect_err("backup preparation should fail on conflicting destination");

        assert_eq!(err.backups.len(), 1);
        assert_eq!(err.backups[0].original, first);
        // First backup should have been moved to backup root (under custom dir), not sibling
        assert!(
            err.backups[0].backup.starts_with(&backup_root_dir),
            "backup should be under custom root: {}",
            err.backups[0].backup.display()
        );
        assert!(err.rollback.restored.contains(&first));
        assert!(err.rollback.failures.is_empty());
        // Original should be restored
        assert!(first.exists());
        // Second should still exist (was never backed up due to conflict)
        assert!(second.exists());

        let _ = fs::remove_dir_all(base);
        let _ = fs::remove_dir_all(backup_root_dir);
    }

    #[test]
    fn test_rollback_summary_lines_call_out_manual_inspection_when_needed() {
        let summary = RollbackSummary {
            restored: vec![PathBuf::from("/tmp/restored")],
            skipped_existing_original: vec![PathBuf::from("/tmp/existing")],
            skipped_missing_backup: vec![PathBuf::from("/tmp/missing.bak")],
            failures: Vec::new(),
        };

        let lines = rollback_summary_lines(&summary);
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Manual inspection recommended"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("skipped (original path already exists)"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("skipped (backup artifact missing)"))
        );
    }

    #[test]
    fn test_missing_backup_is_treated_as_issue_in_failure_messaging() {
        let summary = RollbackSummary {
            restored: Vec::new(),
            skipped_existing_original: Vec::new(),
            skipped_missing_backup: vec![PathBuf::from("/tmp/missing.bak")],
            failures: Vec::new(),
        };

        assert!(summary.has_issues());

        let message = format_failed_package_repair_message(&anyhow!("lamella failed"), &summary);
        assert!(message.contains("Rollback encountered conflicts"));
    }

    #[test]
    fn test_audit_log_best_effort_returns_warning_when_write_fails() {
        let base = temp_test_dir("audit-warning");
        fs::create_dir_all(&base).expect("create test dir");
        let parent_file = base.join("not-a-dir");
        fs::write(&parent_file, "occupied").expect("create blocking parent file");
        let log_path = parent_file.join("package-audit.log");

        let warning =
            append_audit_log_best_effort_with_path(&serde_json::json!({"ok": true}), &log_path);
        assert!(warning.is_some());
        assert!(
            warning
                .as_deref()
                .is_some_and(|value| value.contains("failed to write package audit log"))
        );

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn test_audit_event_includes_rollback_details_on_failure() {
        let summary = RollbackSummary {
            restored: vec![PathBuf::from("/tmp/restored")],
            skipped_existing_original: Vec::new(),
            skipped_missing_backup: Vec::new(),
            failures: vec![RollbackFailure {
                original: PathBuf::from("/tmp/original"),
                backup: PathBuf::from("/tmp/backup"),
                error: "rename failed".to_string(),
            }],
        };

        let event = build_audit_event(
            "failed",
            InstallProfile::Codex,
            Path::new("/tmp/lamella"),
            &[LamellaInvocation::codex_install()],
            &[],
            Some(&summary),
            Some("lamella failed"),
        );

        let rollback = event
            .get("rollback")
            .and_then(serde_json::Value::as_object)
            .expect("rollback details should exist");
        assert_eq!(
            rollback
                .get("restored")
                .and_then(serde_json::Value::as_array)
                .map(std::vec::Vec::len),
            Some(1)
        );
        assert_eq!(
            rollback
                .get("failures")
                .and_then(serde_json::Value::as_array)
                .map(std::vec::Vec::len),
            Some(1)
        );
    }

    fn temp_test_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("stipe-package-repair-{label}-{nanos}"))
    }
}
