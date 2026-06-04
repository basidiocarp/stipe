use anyhow::{Context, Result, anyhow};
use colored::Colorize;
use spore::logging::{SpanContext, workflow_span};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::install;
use super::install::release::normalize_version;
use super::tool_registry;
use super::tool_registry::InstallProfile;
use crate::commands::github::GitHubClient;

#[cfg(test)]
mod tests;

fn get_installed_version(tool: &str) -> Result<String> {
    // Prefer spore-based resolution (PATH-independent) for registry-known tools,
    // fall back to which::which for tools installed outside the managed set.
    let resolved = tool_registry::find(tool)
        .and_then(tool_registry::resolve_binary_path)
        .or_else(|| which::which(tool).ok())
        .with_context(|| format!("{tool} not found"))?;
    let output = install::release::run_command_with_timeout(
        Command::new(resolved).arg("--version"),
        Duration::from_secs(5),
    )
    .with_context(|| format!("Failed to get version for {tool}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "Failed to get version for {tool}: {}",
            stderr.trim()
        ));
    }

    let version_output = String::from_utf8_lossy(&output.stdout);
    let version = version_output
        .split_whitespace()
        .last()
        .ok_or_else(|| anyhow!("Empty version output from {tool}"))?;

    Ok(version.to_string())
}

fn fetch_latest_version(tool: &str, client: &GitHubClient) -> Result<String> {
    crate::commands::github::fetch_release_tag(tool, client)
}

struct UpdateInfo {
    installed: String,
    latest: String,
    update_available: bool,
    needs_reinstall: bool,
}

fn check_tool_update(tool: &str, client: &GitHubClient) -> Result<UpdateInfo> {
    let (installed, needs_reinstall) = if let Some(spec) = tool_registry::find(tool) {
        match tool_registry::probe(spec) {
            tool_registry::ToolProbe::Installed(version) => (version, false),
            tool_registry::ToolProbe::Broken => ("broken".to_string(), true),
            tool_registry::ToolProbe::Missing => {
                return Err(anyhow!("{tool} is not installed"));
            }
        }
    } else {
        (get_installed_version(tool)?, false)
    };
    let latest = fetch_latest_version(tool, client)?;

    let update_available =
        needs_reinstall || normalize_version(&installed) != normalize_version(&latest);

    Ok(UpdateInfo {
        installed,
        latest,
        update_available,
        needs_reinstall,
    })
}

fn update_tool(tool: &str, client: &GitHubClient) -> Result<()> {
    println!("  {} Checking for updates...", "⏳".yellow());

    let update_info = check_tool_update(tool, client)?;

    if !update_info.update_available {
        println!(
            "  {} {} is up to date ({})",
            "✓".green(),
            tool,
            update_info.installed
        );
        return Ok(());
    }

    if update_info.needs_reinstall {
        println!(
            "  {} {} is installed but broken → reinstall {}",
            "↑".cyan(),
            tool,
            update_info.latest
        );
    } else {
        println!(
            "  {} {} {} → {} available",
            "↑".cyan(),
            tool,
            update_info.installed,
            update_info.latest
        );
    }

    println!("  {} Downloading and installing...", "⏳".yellow());

    let prefix = install::install_bin_dir()?;

    // Snapshot the current binary before replacing it, so a failed update can
    // roll back to it. `install_tool` deploys the new binary (rename-into-place)
    // *before* running its internal spec-driven smoke check (verify_functional),
    // so a smoke-check failure surfaces as an Err with the broken binary already
    // live — exactly the state this restore guards against. update_tool is only
    // reached for registry-resolved tools (handle_update_result <- run()'s loop),
    // so verify_and_report_installation's unknown-tool branch is unreachable here.
    // The reachable post-deploy Errs (the `--version` binary verify and the
    // spec-driven smoke check) all leave a live-but-broken binary that restore
    // correctly replaces. Restoring on *any* install error is therefore safe:
    // pre-deploy failures leave the original binary untouched, so the restore
    // just rewrites identical bytes.
    //
    // The per-tool backup directory is named distinctly from run()'s bulk
    // pre-update backup (which uses a bare `backup_timestamp()`). Sharing that
    // name would land in the same `base.join(timestamp)` directory and overwrite
    // the bulk all-tools manifest with this single-tool one, corrupting the full
    // snapshot `stipe rollback` restores. A distinct name keeps both intact.
    let tool_path = prefix.join(tool);
    let backup_dir = if tool_path.exists() {
        let timestamp = pre_update_backup_name(tool);
        let stipe_version = env!("CARGO_PKG_VERSION");
        let binary_paths = vec![(tool.to_string(), tool_path.clone())];
        Some(
            crate::backup::create_backup(&timestamp, stipe_version, &binary_paths, &[])
                .context("could not create pre-update backup")?,
        )
    } else {
        None
    };

    if let Err(error) = super::install::install_tool(tool, &prefix, true, client) {
        if let Some(backup_dir) = &backup_dir {
            restore_after_failed_update(backup_dir, tool);
        }
        return Err(error);
    }

    // Update succeeded: drop the per-tool pre-update snapshot. Its only purpose is
    // the failure-path auto-restore above; leaving it behind would shadow run()'s
    // bulk pre-update backup as the default `stipe rollback` target, since the
    // distinct per-tool name sorts newer than the bulk bare-timestamp name.
    // Rollback-to-previous for the whole run is already covered by the bulk
    // snapshot. Best-effort: a stale snapshot is harmless beyond the shadowing.
    if let Some(backup_dir) = &backup_dir {
        let _ = std::fs::remove_dir_all(backup_dir);
    }

    println!(
        "  {} {} updated to {}",
        "✓".green(),
        tool,
        update_info.latest
    );

    Ok(())
}

/// Backup-directory name for `update_tool`'s per-tool pre-update snapshot.
///
/// MUST stay distinct from `run()`'s bulk pre-update backup, which uses a bare
/// `backup_timestamp()`. A bare name collides into the same `base.join(name)`
/// directory and overwrites the bulk all-tools manifest with this single-tool
/// one, so `stipe rollback` would restore only one tool instead of the full
/// pre-update snapshot. The `-{tool}-preupdate` suffix guarantees no collision.
fn pre_update_backup_name(tool: &str) -> String {
    format!("{}-{tool}-preupdate", crate::backup::backup_timestamp())
}

/// Best-effort restore of a tool's pre-update binary from a backup snapshot
/// created earlier in `update_tool`. Logs the outcome and never propagates a
/// restore error: the caller is already returning the original update failure,
/// and masking it with a restore error would hide the root cause.
fn restore_after_failed_update(backup_dir: &Path, tool: &str) {
    match crate::backup::load_manifest(backup_dir)
        .and_then(|manifest| crate::backup::restore_from_backup(&manifest))
    {
        Ok(()) => println!(
            "  {} Restored previous {} after failed update",
            "↩".yellow(),
            tool
        ),
        Err(restore_error) => eprintln!(
            "  {} Failed to restore {} after failed update: {}",
            "!".red(),
            tool,
            restore_error
        ),
    }
}

fn unique_tools(base: Vec<String>, extras: &[String]) -> Vec<String> {
    let mut ordered = base;
    for tool in extras {
        if !ordered.iter().any(|existing| existing == tool) {
            ordered.push(tool.clone());
        }
    }
    ordered
}

fn installed_profile_tools_with<F>(profile: InstallProfile, mut probe: F) -> Vec<String>
where
    F: FnMut(&tool_registry::ToolSpec) -> tool_registry::ToolProbe,
{
    tool_registry::specs_for_profile(profile)
        .into_iter()
        .filter_map(|spec| {
            probe(spec)
                .is_repairable_presence()
                .then_some(spec.name.to_string())
        })
        .collect()
}

fn resolve_requested_tools(
    all: bool,
    profile: Option<InstallProfile>,
    tools: &[String],
) -> Option<Vec<String>> {
    if all {
        let installed = tool_registry::update_all_specs()
            .into_iter()
            .filter_map(|spec| {
                tool_registry::probe(spec)
                    .is_repairable_presence()
                    .then_some(spec.name.to_string())
            })
            .collect::<Vec<_>>();
        return Some(unique_tools(installed, tools));
    }

    if let Some(profile) = profile {
        let installed = installed_profile_tools_with(profile, tool_registry::probe);
        return Some(unique_tools(installed, tools));
    }

    if !tools.is_empty() {
        return Some(tools.to_vec());
    }

    None
}

fn profile_flag_name(profile: InstallProfile) -> &'static str {
    match profile {
        InstallProfile::Minimal => "minimal",
        InstallProfile::Standard => "standard",
        InstallProfile::ClaudeCode => "claude-code",
        InstallProfile::Codex => "codex",
        InstallProfile::Cursor => "cursor",
        InstallProfile::FullStack => "full",
        InstallProfile::DeveloperTools => "developer-tools",
    }
}

fn print_update_header() {
    println!();
    println!("{}", "Basidiocarp Ecosystem Update".bold());
    println!("{}", "─".repeat(75));
    println!();
}

fn print_update_usage(all: bool) {
    if all {
        println!("No installed tools found. Run 'stipe install --all' first.");
    } else {
        println!("Specify tools to update:");
        println!("  {} stipe update mycelium", "→".dimmed());
        println!("  {} stipe update hyphae rhizome canopy", "→".dimmed());
        println!("  {} stipe update --profile claude-code", "→".dimmed());
        println!("  {} stipe update --all", "→".dimmed());
        println!();
        println!("Check without installing:");
        println!("  {} stipe update --check --profile codex", "→".dimmed());
        println!("  {} stipe update --check --all", "→".dimmed());
    }
    println!();
}

fn resolve_tools_to_check(
    all: bool,
    profile: Option<InstallProfile>,
    tools: &[String],
) -> Option<Vec<String>> {
    let requested = resolve_requested_tools(all, profile, tools)?;

    if (all || profile.is_some()) && requested.is_empty() {
        if let Some(profile) = profile {
            println!(
                "No installed tools found for {}. Run 'stipe install --profile {}' first.",
                profile.mode_label(),
                profile_flag_name(profile)
            );
        } else {
            println!("No installed tools found. Run 'stipe install --all' first.");
        }
        println!();
        return None;
    }

    Some(requested)
}

fn handle_update_result(
    tool: &str,
    info: &UpdateInfo,
    check: bool,
    client: &GitHubClient,
) -> Option<String> {
    if check {
        if info.needs_reinstall {
            println!(
                "  {} {} is installed but broken → reinstall {}",
                "!".yellow(),
                tool,
                info.latest
            );
        } else if info.update_available {
            println!(
                "  {} {} {} → {}",
                "↑".cyan(),
                tool,
                info.installed,
                info.latest
            );
        } else {
            println!(
                "  {} {} is up to date ({})",
                "✓".green(),
                tool,
                info.installed
            );
        }
        return None;
    }

    if info.update_available {
        if let Err(error) = update_tool(tool, client) {
            eprintln!("  {} Failed to update {}: {}", "!".red(), tool, error);
            return Some(format!("{tool}: {error}"));
        }
    } else {
        println!(
            "  {} {} is up to date ({})",
            "✓".green(),
            tool,
            info.installed
        );
    }

    None
}

#[allow(clippy::unnecessary_wraps)]
pub fn run(
    all: bool,
    profile: Option<InstallProfile>,
    check: bool,
    force: bool,
    tools: &[String],
) -> Result<()> {
    let _lock = crate::lockfile::acquire_lock(force).context("could not acquire install lock")?;
    let span_context = update_span_context();
    let _workflow_span = workflow_span("update", &span_context).entered();
    if profile == Some(InstallProfile::DeveloperTools) {
        print_update_header();
        println!("Developer tools are not managed by stipe.");
        println!(
            "Use your package manager to update them, and run 'stipe doctor --developer' to audit what is installed."
        );
        println!();
        return Ok(());
    }

    print_update_header();

    let Some(tools_to_check) = resolve_tools_to_check(all, profile, tools) else {
        if resolve_requested_tools(all, profile, tools).is_none() {
            print_update_usage(all);
        }
        return Ok(());
    };

    // Create a backup before proceeding with any updates (unless we're just checking).
    if !check {
        let mut binary_paths: Vec<(String, PathBuf)> = Vec::new();
        let prefix = install::install_bin_dir()?;
        for tool in &tools_to_check {
            let tool_path = prefix.join(tool);
            if tool_path.exists() {
                binary_paths.push((tool.clone(), tool_path));
            }
        }
        let timestamp = crate::backup::backup_timestamp();
        let stipe_version = env!("CARGO_PKG_VERSION");
        crate::backup::create_backup(&timestamp, stipe_version, &binary_paths, &[])
            .context("could not create pre-update backup")?;
    }

    let client = crate::commands::github::github_client();
    let mut failures = Vec::new();

    for tool in &tools_to_check {
        // Create hyphae-specific pre-upgrade backup before updating hyphae
        if tool == "hyphae" {
            let hyphae_version = match get_installed_version("hyphae") {
                Ok(v) => v,
                Err(e) => {
                    eprintln!(
                        "  {} Warning: could not determine installed hyphae version for backup label: {}",
                        "⚠".yellow(),
                        e
                    );
                    "unknown".to_string()
                }
            };
            let timestamp = crate::backup::backup_timestamp();
            let outcome = crate::backup::pre_upgrade_backup_hyphae(&hyphae_version, &timestamp);
            if outcome.backup_dir.is_none() {
                eprintln!(
                    "  {} Warning: pre-upgrade backup of hyphae failed; continuing upgrade without backup",
                    "⚠".yellow()
                );
            } else if !outcome.is_complete() {
                eprintln!(
                    "  {} Warning: pre-upgrade backup of hyphae partially failed; {} file(s) missing or failed",
                    "⚠".yellow(),
                    outcome.missing.len() + outcome.failed.len()
                );
            }
        }

        match check_tool_update(tool, &client) {
            Ok(info) => {
                if let Some(error) = handle_update_result(tool, &info, check, &client) {
                    failures.push(error);
                }
            }
            Err(error) => {
                eprintln!(
                    "  {} Failed to check {} for updates: {}",
                    "!".red(),
                    tool,
                    error
                );
                failures.push(format!("{tool}: {error}"));
            }
        }
    }

    println!();

    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "update failed for {} tool(s): {}",
            failures.len(),
            failures.join("; ")
        ))
    }
}

fn update_span_context() -> SpanContext {
    let context = SpanContext::for_app("stipe");
    match crate::commands::host_policy::project_root().or_else(|| std::env::current_dir().ok()) {
        Some(path) => context.with_workspace_root(path.display().to_string()),
        None => context,
    }
}
