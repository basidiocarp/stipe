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

    println!();
    println!("{}", "Package Repair".bold());
    println!("{}", "─".repeat(75));
    println!("Profile: {}", profile.profile_name());
    println!("Lamella invocation(s):");
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

    println!(
        "Lamella source: {}",
        host_policy::format_user_path(&lamella_root)
    );
    for invocation in &lamella_invocations {
        println!("  - {}", lamella_command_string(invocation));
    }

    if dry_run {
        for target in targets {
            println!(
                "Would back up {} before package install.",
                host_policy::format_user_path(&target)
            );
        }
        return Ok(());
    }

    let backups = match prepare_backups(&targets) {
        Ok(backups) => backups,
        Err(failure) => {
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
                &lamella_root,
                &lamella_invocations,
                &failure.backups,
                if failure.backups.is_empty() {
                    None
                } else {
                    Some(&failure.rollback)
                },
                Some(failure_message.clone()),
            ));
            return Err(anyhow!(failure_message));
        }
    };
    let backup_paths = backups
        .iter()
        .map(|backup| host_policy::format_user_path(&backup.backup))
        .collect::<Vec<_>>();
    let status = run_lamella_install(&lamella_root, &lamella_invocations);

    match status {
        Ok(()) => {
            append_audit_log_best_effort(&build_audit_event(
                "success",
                profile,
                &lamella_root,
                &lamella_invocations,
                &backups,
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
        Err(error) => {
            let rollback = rollback_backups(&backups);
            for line in rollback_summary_lines(&rollback) {
                println!("{line}");
            }
            append_audit_log_best_effort(&build_audit_event(
                "failed",
                profile,
                &lamella_root,
                &lamella_invocations,
                &backups,
                Some(&rollback),
                Some(error.to_string()),
            ));
            let failure_message = format_failed_package_repair_message(&error, &rollback);
            Err(anyhow!(failure_message))
        }
    }
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
    let project_root =
        host_policy::project_root().context("unable to determine project root for package repair")?;
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

fn prepare_backups(
    targets: &[PathBuf],
) -> std::result::Result<Vec<PackageBackup>, BackupPreparationFailure> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    prepare_backups_with_timestamp(targets, timestamp)
}

fn prepare_backups_with_timestamp(
    targets: &[PathBuf],
    timestamp: u64,
) -> std::result::Result<Vec<PackageBackup>, BackupPreparationFailure> {
    let mut backups = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        if !target.exists() {
            continue;
        }

        let backup = sibling_backup_path(target, timestamp, index);
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

fn sibling_backup_path(path: &Path, timestamp: u64, index: usize) -> PathBuf {
    let suffix = format!(".stipe-backup-{timestamp}-{index}");
    let file_name = path.file_name().and_then(|value| value.to_str()).unwrap_or("state");
    path.with_file_name(format!("{file_name}{suffix}"))
}

fn rollback_backups(backups: &[PackageBackup]) -> RollbackSummary {
    let mut summary = RollbackSummary::default();
    for backup in backups.iter().rev() {
        if !backup.backup.exists() {
            summary
                .skipped_missing_backup
                .push(backup.backup.clone());
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

fn format_failed_package_repair_message(error: &anyhow::Error, rollback: &RollbackSummary) -> String {
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
                    lamella_command_string(invocation).replace("./lamella", &host_policy::format_user_path(&lamella_bin))
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
    error: Option<String>,
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
    };
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
        .open(&log_path)
        .with_context(|| {
            format!(
                "failed to open audit log at {}",
                host_policy::format_user_path(&log_path)
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
    fn test_sibling_backup_path_includes_timestamp_suffix() {
        let path = PathBuf::from("/tmp/example");
        let backup = sibling_backup_path(&path, 1234, 2);
        assert!(
            backup.ends_with("example.stipe-backup-1234-2"),
            "unexpected backup path: {}",
            backup.display()
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
    fn test_prepare_backups_roll_back_partial_state_when_later_backup_fails() {
        let base = temp_test_dir("backup-rollback-on-failure");
        fs::create_dir_all(&base).expect("create test dir");

        let first = base.join("first");
        let second = base.join("second");
        fs::create_dir_all(&first).expect("create first target");
        fs::create_dir_all(&second).expect("create second target");

        let first_backup = sibling_backup_path(&first, 42, 0);
        let second_backup = sibling_backup_path(&second, 42, 1);
        fs::write(&second_backup, "occupied").expect("seed conflicting backup path");

        let err = prepare_backups_with_timestamp(&[first.clone(), second.clone()], 42)
            .expect_err("backup preparation should fail on conflicting destination");

        assert_eq!(err.backups.len(), 1);
        assert_eq!(err.backups[0].original, first);
        assert_eq!(err.backups[0].backup, first_backup);
        assert!(err.rollback.restored.contains(&first));
        assert!(err.rollback.failures.is_empty());
        assert!(first.exists());
        assert!(!first_backup.exists());
        assert!(second.exists());
        assert!(second_backup.exists());

        let _ = fs::remove_dir_all(base);
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

        let message =
            format_failed_package_repair_message(&anyhow!("lamella failed"), &summary);
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
            Some("lamella failed".to_string()),
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
