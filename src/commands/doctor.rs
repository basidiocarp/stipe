use std::fmt::Write as _;

use anyhow::Result;
use colored::Colorize;
use std::path::Path;

use super::claude_hooks;
use super::developer_tools;
use super::host;
use super::host_policy;
use super::install;
use super::output;
use super::repair::{RepairAction, RepairTier, dedupe_repair_actions};
use super::runtime_policy;
use super::tool_registry;
use crate::verify;

mod config_checks;
mod council_checks;
mod hook_checks;
mod instruction_checks;
mod lamella_hooks_checks;
mod lamella_presence_checks;
pub(crate) mod model;
mod package_checks;
mod plugin_inventory_checks;
pub(crate) mod provider_checks;
mod server_checks;
mod skills_checks;
mod tool_checks;
pub(crate) mod version_pins;

use config_checks::{ConfigDriftState, check_mcp_config_drift};
use council_checks::check_task_linked_council;
use hook_checks::{check_claude_hook_commands, check_codex_notify_entries};
use instruction_checks::check_instruction_files;
use lamella_hooks_checks::check_lamella_hooks;
use lamella_presence_checks::check_lamella_presence;
use model::{
    ApiKeyHealth, ApiKeyStatus, AuthFreshness, DoctorReport, DriftFinding, DriftReport,
    HealthCheck, InstallProfileSummary, McpServerHealth, McpServerStatus, PackageDrift,
    PackageInventory, PluginInventory, PluginPathStatus, ProviderHealth, VersionDriftStatus,
    WorktreeConfigDiscovery,
};
use package_checks::{
    collect_package_drift, collect_package_inventory, collect_worktree_config_discovery,
};
use plugin_inventory_checks::collect_plugin_inventory;
use provider_checks::{collect_api_key_health, collect_mcp_health, collect_provider_health};
use server_checks::collect_mcp_server_health;
use skills_checks::check_skills;
use tool_checks::{
    check_canopy_wal_mode, check_capability_registry_health, check_codex_notify,
    check_hook_command_runnability, check_hyphae_db, check_mcp_startups, check_profile_tools,
    check_rhizome_compiled_env, check_shared_storage_root, check_stipe_toml_sync, check_tool,
    init_live_versions,
};

const STIPE_DOCTOR_SCHEMA_VERSION: &str = "1.0";

#[cfg(test)]
mod tests;

fn render_check_line(check: &HealthCheck, colorize: bool, deep: bool) -> String {
    let (symbol, raw_message) = if check.passed {
        ("✓", check.message.clone())
    } else {
        ("✗", check.message.clone())
    };

    let message = if check.passed && !deep {
        // Trim verbose parenthetical from passing checks
        raw_message
            .find(" (")
            .map(|i| raw_message[..i].to_string())
            .unwrap_or(raw_message)
    } else {
        raw_message
    };

    let message = if colorize {
        if check.passed {
            message.green().to_string()
        } else {
            message.red().to_string()
        }
    } else {
        message
    };

    let name = if colorize {
        check.name.bold().to_string()
    } else {
        check.name.clone()
    };

    let line = format!("  {name:<20} {symbol} {message}");
    // Indent continuation lines so they align with the start of the message text.
    // Prefix: 2 margin + 20 name + 1 space + 1 symbol + 1 space = 25 chars.
    if line.contains('\n') {
        line.replace('\n', "\n                         ")
    } else {
        line
    }
}

fn render_drift_finding(finding: &DriftFinding, colorize: bool) -> (String, String) {
    let (symbol, headline, hint) = match finding {
        DriftFinding::MissingMcpRegistration {
            config_path, name, ..
        } => (
            "✗",
            format!(
                "MCP {name}: registration missing from {}",
                host_policy::format_user_path(config_path)
            ),
            "Run: stipe init --repair".to_string(),
        ),
        DriftFinding::MissingMcpBinary {
            binary_path, name, ..
        } => (
            "✗",
            format!(
                "MCP {name}: binary not found at registered path ({})",
                host_policy::format_user_path(binary_path)
            ),
            format!("Run: stipe install {name}"),
        ),
        DriftFinding::MissingHookRegistration {
            config_path, event, ..
        } => (
            "✗",
            format!(
                "Hook {event}: registration missing from {}",
                host_policy::format_user_path(config_path)
            ),
            "Run: stipe init --repair".to_string(),
        ),
        DriftFinding::MissingHookBinary {
            binary_path, event, ..
        } => (
            "✗",
            format!(
                "Hook {event}: registered path not found ({})",
                host_policy::format_user_path(binary_path)
            ),
            "Run: stipe install cortina".to_string(),
        ),
        DriftFinding::ConfigFileModified {
            path,
            actual_checksum,
            ..
        } => (
            "~",
            if actual_checksum.is_some() {
                format!(
                    "Config {}: modified since last init",
                    host_policy::format_user_path(path)
                )
            } else {
                format!(
                    "Config {}: missing since last init",
                    host_policy::format_user_path(path)
                )
            },
            "Run: stipe init --repair".to_string(),
        ),
    };

    let line = if colorize {
        format!("  {symbol} {}", headline.yellow())
    } else {
        format!("  {symbol} {headline}")
    };

    (line, hint)
}

fn render_drift_report(report: &DriftReport, colorize: bool) -> Vec<String> {
    if report.findings.is_empty() {
        return Vec::new();
    }

    let mut lines = vec![if colorize {
        "Config drift detected:".bold().to_string()
    } else {
        "Config drift detected:".to_string()
    }];

    for finding in &report.findings {
        let (line, hint) = render_drift_finding(finding, colorize);
        lines.push(line);
        lines.push(if colorize {
            format!("    {}", hint.dimmed())
        } else {
            format!("    {hint}")
        });
    }

    lines
}

fn render_hook_paths(hook_paths: &[claude_hooks::HookPathSnapshot], colorize: bool) -> Vec<String> {
    if hook_paths.is_empty() {
        return Vec::new();
    }

    let mut lines = vec![if colorize {
        "Hooks:".bold().to_string()
    } else {
        "Hooks:".to_string()
    }];

    for hook in hook_paths {
        let symbol = if hook.passed { "✓" } else { "✗" };
        let path = if colorize {
            if hook.passed {
                hook.path.display().to_string().green().to_string()
            } else {
                hook.path.display().to_string().red().to_string()
            }
        } else {
            hook.path.display().to_string()
        };

        let line = if hook.passed {
            format!("  {symbol} {}: {path}", hook.event)
        } else {
            format!("  {symbol} {}: {path} (not found)", hook.event)
        };
        lines.push(line);
    }

    lines
}

fn render_report(report: &DoctorReport, colorize: bool, deep: bool) -> Vec<String> {
    let mut lines = vec![
        String::new(),
        if colorize {
            "Basidiocarp Ecosystem Health Check".bold().to_string()
        } else {
            "Basidiocarp Ecosystem Health Check".to_string()
        },
        "─".repeat(75),
        String::new(),
    ];

    render_report_header(&mut lines, report, colorize);
    render_report_overview(&mut lines, report, colorize, deep);
    render_report_providers(&mut lines, report, colorize, deep);
    render_report_mcp(&mut lines, report, colorize, deep);
    render_report_hosts(&mut lines, report, colorize);
    if deep {
        render_report_runtime_policy(&mut lines, report, colorize, deep);
    }
    if deep {
        render_report_worktree(&mut lines, report, colorize, deep);
    }
    if deep {
        render_report_packages(&mut lines, report, colorize, deep);
    }
    render_report_drift(&mut lines, report, colorize);
    render_report_hooks(&mut lines, report, colorize);
    render_report_mcp_servers(&mut lines, report, colorize);
    render_report_api_keys(&mut lines, report, colorize);
    render_report_plugins(&mut lines, report, colorize, deep);
    render_report_footer(&mut lines, report, colorize, deep);
    render_report_developer_tools(&mut lines, report);

    lines
}

fn render_report_header(lines: &mut Vec<String>, report: &DoctorReport, _colorize: bool) {
    if let Some(profile) = &report.install_profile {
        lines.push(format!(
            "Install profile: {} ({})",
            profile.profile,
            host_policy::format_user_path(&profile.config_path)
        ));
        lines.push(String::new());
    }
}

fn render_report_overview(
    lines: &mut Vec<String>,
    report: &DoctorReport,
    colorize: bool,
    deep: bool,
) {
    lines.extend(render_overview(report, colorize, deep));
    lines.push(String::new());
}

fn render_report_providers(
    lines: &mut Vec<String>,
    report: &DoctorReport,
    colorize: bool,
    deep: bool,
) {
    lines.extend(render_provider_health(
        &report.provider_health,
        colorize,
        deep,
    ));
    if !report.provider_health.is_empty() {
        lines.push(String::new());
    }
}

fn render_report_mcp(lines: &mut Vec<String>, report: &DoctorReport, colorize: bool, deep: bool) {
    lines.extend(render_mcp_health(&report.mcp_health, colorize, deep));
    if !report.mcp_health.is_empty() {
        lines.push(String::new());
    }
}

fn render_report_hosts(lines: &mut Vec<String>, report: &DoctorReport, colorize: bool) {
    let host_checks = report
        .checks
        .iter()
        .filter(|check| is_host_check(check))
        .collect::<Vec<_>>();
    if !host_checks.is_empty() {
        lines.extend(render_host_status(&host_checks, colorize));
        lines.push(String::new());
    }
}

fn render_report_runtime_policy(
    lines: &mut Vec<String>,
    report: &DoctorReport,
    colorize: bool,
    deep: bool,
) {
    if let Some(runtime_policy) = &report.runtime_policy {
        lines.extend(render_runtime_policy(runtime_policy, colorize, deep));
        lines.push(String::new());
    }
}

fn render_report_worktree(
    lines: &mut Vec<String>,
    report: &DoctorReport,
    colorize: bool,
    deep: bool,
) {
    if let Some(worktree) = &report.worktree_config {
        lines.extend(render_worktree_config(worktree, colorize, deep));
        lines.push(String::new());
    }
}

fn render_report_packages(
    lines: &mut Vec<String>,
    report: &DoctorReport,
    colorize: bool,
    deep: bool,
) {
    if let Some(inventory) = &report.package_inventory {
        lines.extend(render_package_inventory(inventory, colorize, deep));
        lines.push(String::new());
    }

    if let Some(drift) = &report.package_drift {
        lines.extend(render_package_drift(drift, colorize, deep));
        lines.push(String::new());
    }
}

fn render_report_drift(lines: &mut Vec<String>, report: &DoctorReport, colorize: bool) {
    if let Some(drift) = &report.drift
        && !drift.findings.is_empty()
    {
        lines.extend(render_drift_report(drift, colorize));
        lines.push(String::new());
    }
}

fn render_report_hooks(lines: &mut Vec<String>, report: &DoctorReport, colorize: bool) {
    lines.extend(render_hook_paths(&report.hook_paths, colorize));
    if !report.hook_paths.is_empty() {
        lines.push(String::new());
    }
}

fn render_report_mcp_servers(lines: &mut Vec<String>, report: &DoctorReport, colorize: bool) {
    lines.extend(render_mcp_server_health(
        &report.mcp_server_health,
        colorize,
    ));
    if !report.mcp_server_health.is_empty() {
        lines.push(String::new());
    }
}

fn render_report_api_keys(lines: &mut Vec<String>, report: &DoctorReport, colorize: bool) {
    lines.extend(render_api_key_health(&report.api_key_health, colorize));
    if !report.api_key_health.is_empty() {
        lines.push(String::new());
    }
}

fn render_report_plugins(
    lines: &mut Vec<String>,
    report: &DoctorReport,
    colorize: bool,
    deep: bool,
) {
    if let Some(plugin_inventory) = &report.plugin_inventory {
        lines.extend(render_plugin_inventory(plugin_inventory, colorize, deep));
        lines.push(String::new());
    }
}

fn render_report_footer(
    lines: &mut Vec<String>,
    report: &DoctorReport,
    colorize: bool,
    deep: bool,
) {
    if report.healthy {
        lines.extend(render_footer_lines(
            &report.summary,
            "stay on the current ecosystem configuration; no repair action is needed",
            (!deep).then_some(
                "run `stipe doctor --deep` for the expanded operator report".to_string(),
            ),
            colorize,
        ));
    } else if report.repair_actions.is_empty() {
        lines.extend(render_footer_lines(
            &report.summary,
            "review the failing sections above",
            None,
            colorize,
        ));
    } else {
        match build_repair_plan(&report.repair_actions) {
            Ok(repair_plan) => {
                lines.extend(render_footer_lines(
                    &report.summary,
                    &format!("run `{}`", repair_plan.primary.command),
                    repair_plan
                        .follow_up
                        .as_ref()
                        .map(|action| format!("run `{}`", action.command)),
                    colorize,
                ));

                let additional_actions = render_additional_repair_actions(&repair_plan, colorize);
                if !additional_actions.is_empty() {
                    lines.push(String::new());
                    lines.extend(additional_actions);
                }
            }
            Err(e) => {
                lines.extend(render_footer_lines(
                    &report.summary,
                    "review the failing sections above",
                    None,
                    colorize,
                ));
                tracing::error!("failed to build repair plan: {e}");
            }
        }
    }

    lines.push(String::new());
}

fn render_report_developer_tools(lines: &mut Vec<String>, report: &DoctorReport) {
    if let Some(developer_tools) = &report.developer_tools {
        lines.extend(developer_tools::render_report(developer_tools));
    }
}

fn is_instruction_check(check: &HealthCheck) -> bool {
    check.name.starts_with("L0:") || check.name.starts_with("L1:") || check.name.starts_with("L2:")
}

fn render_instruction_summary(checks: &[&HealthCheck], colorize: bool) -> String {
    let total = checks.len();
    let failed: Vec<_> = checks.iter().filter(|c| !c.passed).collect();
    let name = "instruction files";
    let name_fmt = if colorize {
        name.bold().to_string()
    } else {
        name.to_string()
    };
    if failed.is_empty() {
        let msg = format!("{total} files found");
        let msg = if colorize {
            msg.green().to_string()
        } else {
            msg
        };
        format!("  {name_fmt:<20} ✓ {msg}")
    } else {
        let passed = total - failed.len();
        let missing: Vec<String> = failed
            .iter()
            .map(|c| {
                c.name
                    .split_once(": ")
                    .map_or_else(|| c.name.clone(), |(_, n)| n.to_string())
            })
            .collect();
        let msg = format!(
            "{passed} passed, {} missing: {}",
            failed.len(),
            missing.join(", ")
        );
        let msg = if colorize { msg.red().to_string() } else { msg };
        format!("  {name_fmt:<20} ✗ {msg}")
    }
}

fn render_overview(report: &DoctorReport, colorize: bool, deep: bool) -> Vec<String> {
    let mut lines = vec![if colorize {
        "Overview:".bold().to_string()
    } else {
        "Overview:".to_string()
    }];

    let regular_checks: Vec<&HealthCheck> = report
        .checks
        .iter()
        .filter(|c| !is_host_check(c) && !is_instruction_check(c))
        .collect();

    let instruction_checks: Vec<&HealthCheck> = report
        .checks
        .iter()
        .filter(|c| is_instruction_check(c))
        .collect();

    if deep {
        // Deep: show all regular checks expanded
        for check in &regular_checks {
            lines.push(render_check_line(check, colorize, deep));
        }
    } else {
        // Compact: show only failing regular checks
        let failing: Vec<_> = regular_checks.iter().filter(|c| !c.passed).collect();
        let passed_count = regular_checks.iter().filter(|c| c.passed).count();

        for check in &failing {
            lines.push(render_check_line(check, colorize, deep));
        }

        if passed_count > 0 {
            let summary = format!("  ({passed_count} checks passed)");
            let summary = if colorize {
                summary.dimmed().to_string()
            } else {
                summary
            };
            lines.push(summary);
        }
    }

    // Instruction checks: always collapsed in compact, expanded in deep
    if !instruction_checks.is_empty() {
        if deep {
            for check in &instruction_checks {
                lines.push(render_check_line(check, colorize, deep));
            }
        } else {
            lines.push(render_instruction_summary(&instruction_checks, colorize));
        }
    }

    // Host summary (unchanged)
    let host_checks: Vec<&HealthCheck> =
        report.checks.iter().filter(|c| is_host_check(c)).collect();
    if !host_checks.is_empty() {
        lines.push(render_check_line(
            &host_summary_check(&host_checks),
            colorize,
            deep,
        ));
    }

    lines
}

fn is_host_check(check: &HealthCheck) -> bool {
    check.name.starts_with("host: ")
}

fn host_label(check: &HealthCheck) -> &str {
    check.name.strip_prefix("host: ").unwrap_or(&check.name)
}

fn host_groups<'a>(checks: &'a [&'a HealthCheck]) -> Vec<(&'a str, Vec<&'a HealthCheck>)> {
    let mut groups: Vec<(&str, Vec<&HealthCheck>)> = Vec::new();
    for check in checks {
        let label = host_label(check);
        if let Some((_, grouped)) = groups.iter_mut().find(|(existing, _)| *existing == label) {
            grouped.push(*check);
        } else {
            groups.push((label, vec![*check]));
        }
    }
    groups
}

fn pluralize(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        singular.to_string()
    } else {
        plural.to_string()
    }
}

fn host_summary_check(host_checks: &[&HealthCheck]) -> HealthCheck {
    let groups = host_groups(host_checks);
    let total = groups.len();
    let ready = groups
        .iter()
        .filter(|(_, grouped)| grouped.iter().all(|check| check.passed))
        .count();
    let attention = total.saturating_sub(ready);

    let message = if attention == 0 {
        format!(
            "{total} {} look ready",
            pluralize(total, "host mode", "host modes")
        )
    } else if ready == 0 {
        format!(
            "{attention} {} {} attention",
            pluralize(attention, "host mode", "host modes"),
            if attention == 1 { "needs" } else { "need" }
        )
    } else {
        format!(
            "{ready} ready, {attention} {} {} attention",
            pluralize(attention, "host mode", "host modes"),
            if attention == 1 { "needs" } else { "need" }
        )
    };

    HealthCheck {
        name: "host status".to_string(),
        passed: attention == 0,
        message,
        repair_actions: Vec::new(),
    }
}

fn render_host_status(host_checks: &[&HealthCheck], colorize: bool) -> Vec<String> {
    let mut lines = vec![if colorize {
        "Host status:".bold().to_string()
    } else {
        "Host status:".to_string()
    }];

    for (label, grouped) in host_groups(host_checks) {
        let healthy = grouped.iter().all(|check| check.passed);
        let symbol = if healthy { "✓" } else { "✗" };
        let message = grouped
            .iter()
            .copied()
            .find(|check| !check.passed)
            .or_else(|| grouped.last().copied())
            .map(|check| check.message.clone())
            .unwrap_or_default();
        let message = if colorize {
            if healthy {
                message.green().to_string()
            } else {
                message.yellow().to_string()
            }
        } else {
            message
        };
        let label = if colorize {
            label.bold().to_string()
        } else {
            label.to_string()
        };

        lines.push(format!("  {symbol} {label:<12} {message}"));
    }

    lines
}

fn render_provider_health(
    provider_health: &[ProviderHealth],
    colorize: bool,
    deep: bool,
) -> Vec<String> {
    if provider_health.is_empty() {
        return Vec::new();
    }

    let mut lines = vec![if colorize {
        "Providers:".bold().to_string()
    } else {
        "Providers:".to_string()
    }];

    for provider in provider_health {
        let symbol = if provider.healthy { "✓" } else { "✗" };
        let status = format!(
            "{} (auth: {})",
            provider.status,
            auth_freshness_label(provider.auth_freshness)
        );
        let status = if colorize {
            if provider.healthy {
                status.green().to_string()
            } else {
                status.yellow().to_string()
            }
        } else {
            status
        };
        lines.push(format!(
            "  {symbol} {:<12} {}",
            provider.host.client_flag(),
            status
        ));
        if (!provider.healthy || deep)
            && let Some(auth_detail) = &provider.auth_detail
        {
            lines.push(format!("    detail: {auth_detail}"));
        }
    }

    lines
}

fn render_mcp_health(mcp_health: &[model::McpHealth], colorize: bool, deep: bool) -> Vec<String> {
    if mcp_health.is_empty() {
        return Vec::new();
    }

    let mut lines = vec![if colorize {
        "MCP status:".bold().to_string()
    } else {
        "MCP status:".to_string()
    }];

    for mcp in mcp_health {
        let symbol = if mcp.healthy { "✓" } else { "✗" };
        let status = format!(
            "{} (auth: {})",
            mcp.status,
            auth_freshness_label(mcp.auth_freshness)
        );
        let status = if colorize {
            if mcp.healthy {
                status.green().to_string()
            } else {
                status.yellow().to_string()
            }
        } else {
            status
        };
        lines.push(format!(
            "  {symbol} {:<12} {}",
            mcp.host.client_flag(),
            status
        ));
        if (!mcp.healthy || deep) && !mcp.config_paths.is_empty() {
            let paths = mcp
                .config_paths
                .iter()
                .map(|path| host_policy::format_user_path(path))
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("    config: {paths}"));
        }
        if !mcp.missing_servers.is_empty() {
            lines.push(format!("    missing: {}", mcp.missing_servers.join(", ")));
        }
        if deep && !mcp.registered_servers.is_empty() {
            lines.push(format!(
                "    registered: {}",
                mcp.registered_servers.join(", ")
            ));
        }
    }

    lines
}

fn render_worktree_config(
    report: &WorktreeConfigDiscovery,
    colorize: bool,
    deep: bool,
) -> Vec<String> {
    let mut lines = vec![if colorize {
        "Worktree config discovery:".bold().to_string()
    } else {
        "Worktree config discovery:".to_string()
    }];

    if let Some(project_root) = &report.project_root {
        lines.push(format!(
            "  root: {}",
            host_policy::format_user_path(project_root)
        ));
    } else {
        lines.push("  root: not detected".to_string());
    }

    if report.discovered_configs.is_empty() {
        lines.push("  configs: none discovered".to_string());
    } else {
        lines.push(format!(
            "  configs: {} discovered",
            report.discovered_configs.len()
        ));
        if deep {
            lines.extend(
                report
                    .discovered_configs
                    .iter()
                    .map(|path| format!("  config: {}", host_policy::format_user_path(path))),
            );
        }
    }

    lines
}

fn render_runtime_policy(
    report: &runtime_policy::RuntimePolicyReport,
    colorize: bool,
    deep: bool,
) -> Vec<String> {
    let mut lines = vec![if colorize {
        "Runtime policy:".bold().to_string()
    } else {
        "Runtime policy:".to_string()
    }];

    lines.push(format!(
        "  configured: {}",
        if report.configured { "yes" } else { "no" }
    ));
    lines.push(format!(
        "  policy scope precedence: {}",
        report
            .precedence
            .iter()
            .map(|scope| format!("{scope:?}").to_lowercase())
            .collect::<Vec<_>>()
            .join(" -> ")
    ));

    if report.config_paths.is_empty() {
        lines.push("  config: none discovered".to_string());
    } else {
        lines.push(format!(
            "  config: {} file(s) discovered",
            report.config_paths.len()
        ));
        if deep {
            lines.extend(
                report
                    .config_paths
                    .iter()
                    .map(|path| format!("  config path: {}", host_policy::format_user_path(path))),
            );
        }
    }

    if let Some(load_error) = &report.load_error {
        lines.push(format!("  load error: {load_error}"));
    }

    if report.remembered_decisions.is_empty() {
        lines.push("  approval memory: none recorded".to_string());
    } else {
        let allow_count = report
            .remembered_decisions
            .iter()
            .filter(|decision| decision.decision == runtime_policy::PolicyDecision::Allow)
            .count();
        let deny_count = report
            .remembered_decisions
            .iter()
            .filter(|decision| decision.decision == runtime_policy::PolicyDecision::Deny)
            .count();
        lines.push(format!(
            "  approval memory: {allow_count} allow, {deny_count} deny"
        ));
        if deep {
            lines.extend(report.remembered_decisions.iter().map(|decision| {
                let mut line = format!(
                    "    - {} {} ({}, source: {}, updated: {})",
                    match decision.decision {
                        runtime_policy::PolicyDecision::Allow => "allow",
                        runtime_policy::PolicyDecision::Deny => "deny",
                    },
                    decision.subject,
                    match decision.scope {
                        runtime_policy::PolicyScope::Project => "project",
                        runtime_policy::PolicyScope::User => "user",
                    },
                    match decision.source {
                        runtime_policy::DecisionSource::OperatorProfile => "operator-profile",
                        runtime_policy::DecisionSource::OperatorPolicyFile =>
                            "operator-policy-file",
                        runtime_policy::DecisionSource::ImportedConfig => "imported-config",
                    },
                    decision.updated_at_unix
                );
                if let Some(note) = &decision.note {
                    let _ = write!(line, "; note: {note}");
                }
                line
            }));
        }
    }

    if let Some(active) = &report.active_install_profile {
        lines.push(format!(
            "  active install profile decision: {} ({}, source: {})",
            match active.decision {
                runtime_policy::PolicyDecision::Allow => "allow",
                runtime_policy::PolicyDecision::Deny => "deny",
            },
            match active.scope {
                runtime_policy::PolicyScope::Project => "project",
                runtime_policy::PolicyScope::User => "user",
            },
            match active.source {
                runtime_policy::DecisionSource::OperatorProfile => "operator-profile",
                runtime_policy::DecisionSource::OperatorPolicyFile => "operator-policy-file",
                runtime_policy::DecisionSource::ImportedConfig => "imported-config",
            }
        ));
    }

    lines
}

fn render_package_inventory(report: &PackageInventory, colorize: bool, deep: bool) -> Vec<String> {
    let mut lines = vec![if colorize {
        "Package and plugin inventory:".bold().to_string()
    } else {
        "Package and plugin inventory:".to_string()
    }];

    lines.push(format!(
        "  metadata available: {}",
        if report.package_metadata_available {
            "yes"
        } else {
            "no"
        }
    ));
    if report.metadata_sources.is_empty() {
        lines.push("  metadata sources: none discovered".to_string());
    } else {
        lines.push(format!(
            "  metadata sources: {} discovered",
            report.metadata_sources.len()
        ));
        if deep {
            lines.extend(
                report
                    .metadata_sources
                    .iter()
                    .map(|path| format!("  metadata: {}", host_policy::format_user_path(path))),
            );
        }
    }
    if report.discovered_packages.is_empty() {
        lines.push("  packages: none discovered".to_string());
    } else {
        lines.push(format!(
            "  packages: {} discovered",
            report.discovered_packages.len()
        ));
        lines.push(format!(
            "  families: {}",
            summarize_package_families(&report.discovered_packages)
        ));
        if deep {
            lines.push(format!(
                "  package detail: {}",
                report.discovered_packages.join(", ")
            ));
        }
    }
    if report.discovered_plugins.is_empty() {
        lines.push("  plugins: none discovered".to_string());
    } else {
        lines.push(format!(
            "  plugins: {} discovered ({})",
            report.discovered_plugins.len(),
            summarize_plugin_roots(&report.discovered_plugins)
        ));
        if deep {
            lines.push(format!(
                "  plugin detail: {}",
                report.discovered_plugins.join(", ")
            ));
        }
    }

    lines
}

fn render_package_drift(report: &PackageDrift, colorize: bool, deep: bool) -> Vec<String> {
    let mut lines = vec![if colorize {
        "Package drift:".bold().to_string()
    } else {
        "Package drift:".to_string()
    }];
    if !report.metadata_available
        && report.expected_packages.is_empty()
        && report.missing_packages.is_empty()
    {
        lines.push("  status: no saved install profile; checks skipped".to_string());
        return lines;
    }

    lines.push(format!(
        "  metadata available: {}",
        if report.metadata_available {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  expected packages: {}",
        report.expected_packages.len()
    ));
    if deep && !report.expected_packages.is_empty() {
        lines.push(format!(
            "  expected detail: {}",
            report.expected_packages.join(", ")
        ));
    }
    if deep && !report.installed_packages.is_empty() {
        lines.push(format!(
            "  installed detail: {}",
            report.installed_packages.join(", ")
        ));
    }
    if report.missing_packages.is_empty() {
        lines.push("  missing packages: none".to_string());
    } else {
        let missing = if colorize {
            report.missing_packages.join(", ").yellow().to_string()
        } else {
            report.missing_packages.join(", ")
        };
        lines.push(format!("  missing packages: {missing}"));
    }
    lines
}

fn render_mcp_server_health(servers: &[McpServerHealth], colorize: bool) -> Vec<String> {
    if servers.is_empty() {
        return Vec::new();
    }

    let mut lines = vec![if colorize {
        "MCP server health:".bold().to_string()
    } else {
        "MCP server health:".to_string()
    }];

    for server in servers {
        let (symbol, status_str) = match server.status {
            McpServerStatus::Running => ("✓", "running"),
            McpServerStatus::InstalledNotResponding => ("~", "installed-not-responding"),
            McpServerStatus::NotInstalled => ("✗", "not-installed"),
        };
        let healthy = server.status == McpServerStatus::Running;
        let status_line = if colorize {
            if healthy {
                status_str.green().to_string()
            } else if server.status == McpServerStatus::InstalledNotResponding {
                status_str.yellow().to_string()
            } else {
                status_str.red().to_string()
            }
        } else {
            status_str.to_string()
        };
        let name = if colorize {
            server.name.bold().to_string()
        } else {
            server.name.clone()
        };
        lines.push(format!("  {symbol} {name:<12} {status_line}"));
        if let Some(detail) = &server.detail {
            lines.push(format!("    detail: {detail}"));
        }
    }

    lines
}

fn render_api_key_health(keys: &[ApiKeyHealth], colorize: bool) -> Vec<String> {
    if keys.is_empty() {
        return Vec::new();
    }

    let mut lines = vec![if colorize {
        "Provider keys:".bold().to_string()
    } else {
        "Provider keys:".to_string()
    }];

    for key in keys {
        let (symbol, status_str) = match key.status {
            ApiKeyStatus::Configured => ("✓", "configured"),
            ApiKeyStatus::Missing => ("~", "missing"),
            ApiKeyStatus::UnexpectedFormat => ("~", "unexpected-format"),
        };
        let healthy = key.status == ApiKeyStatus::Configured;
        let status_display = if colorize {
            if healthy {
                status_str.green().to_string()
            } else {
                status_str.yellow().to_string()
            }
        } else {
            status_str.to_string()
        };
        let provider = if colorize {
            key.provider.bold().to_string()
        } else {
            key.provider.clone()
        };
        lines.push(format!("  {symbol} {provider:<18} {status_display}"));
        lines.push(format!("    note: {}", key.note));
    }

    lines
}

fn render_plugin_inventory(report: &PluginInventory, colorize: bool, deep: bool) -> Vec<String> {
    let mut lines = vec![if colorize {
        "Plugin and hook inventory:".bold().to_string()
    } else {
        "Plugin and hook inventory:".to_string()
    }];

    lines.push(format!(
        "  validator: {}",
        if report.annulus_validator_used {
            "annulus validate-hooks"
        } else {
            "direct path checks"
        }
    ));
    lines.push(format!("  skills: {}", report.skills_count));
    lines.push(format!("  hooks: {}", report.hooks_count));

    let stale_str = report.stale_count.to_string();
    let missing_str = report.missing_count.to_string();
    let stale_display = if colorize && report.stale_count > 0 {
        stale_str.yellow().to_string()
    } else {
        stale_str
    };
    let missing_display = if colorize && report.missing_count > 0 {
        missing_str.red().to_string()
    } else {
        missing_str
    };
    lines.push(format!("  stale: {stale_display}"));
    lines.push(format!("  missing: {missing_display}"));

    if deep && !report.items.is_empty() {
        lines.push(String::new());
        for item in &report.items {
            let path_label = match item.path_status {
                PluginPathStatus::Valid => "valid",
                PluginPathStatus::Stale => "stale",
                PluginPathStatus::Missing => "missing",
            };
            let drift_label = match item.version_drift {
                VersionDriftStatus::UpToDate => "up-to-date",
                VersionDriftStatus::Behind => "behind",
                VersionDriftStatus::Unknown => "unknown",
            };
            let version_detail = match (&item.installed_version, &item.pinned_version) {
                (Some(installed), Some(pinned))
                    if item.version_drift == VersionDriftStatus::Behind =>
                {
                    format!(" (installed: {installed}, pinned: {pinned})")
                }
                (Some(installed), _) => format!(" (v{installed})"),
                _ => String::new(),
            };
            lines.push(format!(
                "  {} [{}] {} path={path_label} drift={drift_label}{version_detail}",
                match item.path_status {
                    PluginPathStatus::Valid => "✓",
                    PluginPathStatus::Stale | PluginPathStatus::Missing => "✗",
                },
                item.category,
                item.name,
            ));
        }
    }

    lines
}

fn summarize_package_families(packages: &[String]) -> String {
    let mut families: Vec<(String, usize)> = Vec::new();
    for package in packages {
        let family = package.split(':').next().unwrap_or(package).to_string();
        if let Some((_, count)) = families.iter_mut().find(|(name, _)| *name == family) {
            *count += 1;
        } else {
            families.push((family, 1));
        }
    }
    families
        .into_iter()
        .map(|(family, count)| format!("{family} ({count})"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn summarize_plugin_roots(plugins: &[String]) -> String {
    plugins
        .iter()
        .map(|plugin| {
            if let Some((_, tail)) = plugin.rsplit_once(':') {
                return tail.to_string();
            }
            Path::new(plugin)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(plugin)
                .to_string()
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn auth_freshness_label(auth: AuthFreshness) -> &'static str {
    match auth {
        AuthFreshness::Fresh => "fresh",
        AuthFreshness::Stale => "stale",
        AuthFreshness::Missing => "missing",
        AuthFreshness::Unknown => "unknown",
    }
}

#[derive(Clone)]
struct RepairPlan {
    primary: RepairAction,
    follow_up: Option<RepairAction>,
    remaining_primary: Vec<RepairAction>,
    secondary: Vec<RepairAction>,
    manual: Vec<RepairAction>,
}

fn build_repair_plan(repair_actions: &[RepairAction]) -> Result<RepairPlan> {
    let mut actions = repair_actions.to_vec();
    actions.sort_by_key(repair_action_priority);

    let primary = actions
        .iter()
        .find(|action| action.tier == RepairTier::Primary)
        .cloned()
        .or_else(|| actions.first().cloned())
        .ok_or_else(|| anyhow::anyhow!("repair plan produced no actions; nothing to apply"))?;

    let follow_up = actions
        .iter()
        .find(|action| is_follow_up_candidate(action, &primary, &actions))
        .cloned();

    let mut remaining_primary = Vec::new();
    let mut secondary = Vec::new();
    let mut manual = Vec::new();

    for action in actions {
        if action.command == primary.command
            || follow_up
                .as_ref()
                .is_some_and(|candidate| candidate.command == action.command)
            || should_suppress_repair_action(&action, &primary, repair_actions)
        {
            continue;
        }

        match action.tier {
            RepairTier::Primary => remaining_primary.push(action),
            RepairTier::Secondary => secondary.push(action),
            RepairTier::Manual => manual.push(action),
        }
    }

    Ok(RepairPlan {
        primary,
        follow_up,
        remaining_primary,
        secondary,
        manual,
    })
}

fn repair_action_priority(action: &RepairAction) -> (u8, u8, String) {
    let tier_rank = match action.tier {
        RepairTier::Primary => 0,
        RepairTier::Secondary => 1,
        RepairTier::Manual => 2,
    };
    let command_rank = if action.command == "stipe init --repair" {
        0
    } else if action.command.starts_with("stipe host setup ") {
        1
    } else if action.command == "stipe package" {
        2
    } else if action.command.starts_with("stipe install ") {
        3
    } else if action.command == "stipe host doctor" {
        4
    } else {
        10
    };
    (tier_rank, command_rank, action.command.clone())
}

fn is_follow_up_candidate(
    action: &RepairAction,
    primary: &RepairAction,
    all_actions: &[RepairAction],
) -> bool {
    action.command != primary.command
        && !should_suppress_repair_action(action, primary, all_actions)
}

fn should_suppress_repair_action(
    action: &RepairAction,
    primary: &RepairAction,
    all_actions: &[RepairAction],
) -> bool {
    if action.command == "stipe host doctor"
        && (primary.command == "stipe init --repair"
            || all_actions
                .iter()
                .any(|candidate| candidate.command.starts_with("stipe host setup ")))
    {
        return true;
    }

    false
}

fn render_footer_lines(
    state: &str,
    next_step: &str,
    optional_follow_up: Option<String>,
    colorize: bool,
) -> Vec<String> {
    output::render_footer(state.to_string(), next_step.to_string(), optional_follow_up)
        .into_iter()
        .map(|line| {
            if colorize {
                if line.starts_with("Next step:") {
                    line.bold().to_string()
                } else {
                    line.dimmed().to_string()
                }
            } else {
                line
            }
        })
        .collect()
}

fn render_additional_repair_actions(plan: &RepairPlan, colorize: bool) -> Vec<String> {
    let mut lines = Vec::new();
    let grouped = [
        (
            "Additional repair actions:",
            !plan.remaining_primary.is_empty(),
        ),
        ("Then consider:", !plan.secondary.is_empty()),
        ("Manual follow-up:", !plan.manual.is_empty()),
    ];

    if grouped.iter().all(|(_, present)| !present) {
        return lines;
    }

    for (heading, present) in grouped {
        if !present {
            continue;
        }

        lines.push(if colorize {
            heading.bold().to_string()
        } else {
            heading.to_string()
        });

        let actions = match heading {
            "Additional repair actions:" => &plan.remaining_primary,
            "Then consider:" => &plan.secondary,
            "Manual follow-up:" => &plan.manual,
            _ => unreachable!(),
        };
        lines.extend(
            actions
                .iter()
                .map(|action| format!("  - {}", action.command)),
        );
    }

    lines
}

fn host_health_checks() -> Vec<HealthCheck> {
    host::build_host_doctor_report(None)
        .checks
        .into_iter()
        .map(|check| HealthCheck {
            name: format!("host: {}", check.host.client_flag()),
            passed: check.passed,
            message: check.message,
            repair_actions: check.repair_actions,
        })
        .collect()
}

fn build_report_with_saved_profile(
    saved_profile: Option<install::SavedInstallProfile>,
    include_developer_tools: bool,
    deep: bool,
) -> DoctorReport {
    let provider_health = collect_provider_health();
    let mcp_health = collect_mcp_health();
    let mcp_server_health = collect_mcp_server_health();
    let api_key_health = collect_api_key_health();
    let plugin_inventory = collect_plugin_inventory();
    let runtime_policy =
        runtime_policy::collect_runtime_policy(saved_profile.as_ref().map(|saved| saved.profile));
    let package_inventory = collect_package_inventory();
    let worktree_config = collect_worktree_config_discovery();
    let (package_drift, package_drift_check) = collect_package_drift(saved_profile.as_ref());
    let drift_state = check_mcp_config_drift();

    let mut checks = build_initial_tool_checks(saved_profile.as_ref(), deep);
    let hook_paths = collect_hook_paths();

    add_provider_checks(&mut checks, &provider_health);
    add_mcp_checks(&mut checks, &mcp_health);
    add_core_checks(
        &mut checks,
        &runtime_policy,
        saved_profile.as_ref(),
        &package_inventory,
        &plugin_inventory,
        &worktree_config,
        &drift_state,
        package_drift_check,
    );

    if deep {
        checks.extend(check_mcp_startups());
    }

    add_ownership_checks(&mut checks);
    checks.extend(host_health_checks());
    checks.extend(check_instruction_files());
    checks.push(check_skills());
    add_hook_checks(&mut checks, &hook_paths);
    checks.push(check_lamella_hooks());
    checks.push(check_lamella_presence());

    let healthy = checks.iter().all(|check| check.passed);
    let failing = checks.iter().filter(|check| !check.passed).count();
    let repair_actions = dedupe_repair_actions(
        checks
            .iter()
            .flat_map(|check| check.repair_actions.clone())
            .collect(),
    );

    DoctorReport {
        schema_version: STIPE_DOCTOR_SCHEMA_VERSION.to_string(),
        healthy,
        summary: if healthy {
            "All checks passed.".to_string()
        } else {
            format!("{failing} checks need attention.")
        },
        install_profile: saved_profile.map(|saved| InstallProfileSummary {
            profile: saved.profile.mode_label().to_string(),
            config_path: saved.path,
        }),
        checks,
        hook_paths,
        repair_actions,
        drift: drift_state.report,
        developer_tools: include_developer_tools.then(developer_tools::doctor_report),
        provider_health,
        mcp_health,
        runtime_policy: Some(runtime_policy),
        package_inventory: Some(package_inventory),
        worktree_config: Some(worktree_config),
        package_drift: Some(package_drift),
        mcp_server_health,
        api_key_health,
        plugin_inventory: Some(plugin_inventory),
    }
}

/// Attempt to fetch live GitHub versions for all doctor-checked tools.
/// Times out after 5 seconds; silently returns empty on any failure.
fn fetch_live_versions_for_doctor() -> std::collections::HashMap<String, String> {
    use std::sync::mpsc;
    use std::time::Duration;

    let tool_names: Vec<String> = tool_registry::doctor_specs()
        .iter()
        .map(|spec| spec.name.to_string())
        .collect();

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let client = crate::commands::github::github_client();
        let refs: Vec<&str> = tool_names.iter().map(String::as_str).collect();
        let live = crate::commands::github::fetch_live_tool_versions(&refs, &client);
        let _ = tx.send(live);
    });

    rx.recv_timeout(Duration::from_secs(5)).unwrap_or_default()
}

fn build_initial_tool_checks(
    saved_profile: Option<&install::SavedInstallProfile>,
    deep: bool,
) -> Vec<HealthCheck> {
    init_live_versions(fetch_live_versions_for_doctor());

    if let Some(saved_profile) = saved_profile {
        check_profile_tools(saved_profile.profile, deep)
    } else {
        tool_registry::doctor_specs()
            .into_iter()
            .map(|spec| check_tool(spec, deep))
            .collect::<Vec<_>>()
    }
}

fn collect_hook_paths() -> Vec<claude_hooks::HookPathSnapshot> {
    let mut hook_paths = claude_hooks::hook_path_snapshots();
    hook_paths.extend(claude_hooks::lamella_hook_path_snapshots());
    hook_paths
}

fn add_provider_checks(checks: &mut Vec<HealthCheck>, provider_health: &[ProviderHealth]) {
    let provider_failures = provider_health
        .iter()
        .filter(|provider| !provider.healthy)
        .count();
    checks.push(HealthCheck {
        name: "provider health".to_string(),
        passed: provider_failures == 0,
        message: if provider_failures == 0 {
            "All detected providers are healthy.".to_string()
        } else {
            format!("{provider_failures} provider entries need attention")
        },
        repair_actions: if provider_failures == 0 {
            Vec::new()
        } else {
            vec![RepairAction::stipe(
                "host-doctor",
                "Inspect host health",
                "Inspect host/provider health and run targeted setup for missing provider configuration.",
                &["host", "doctor"],
                RepairTier::Primary,
            )]
        },
    });
}

fn add_mcp_checks(checks: &mut Vec<HealthCheck>, mcp_health: &[model::McpHealth]) {
    let mcp_failures = mcp_health.iter().filter(|mcp| !mcp.healthy).count();
    checks.push(HealthCheck {
        name: "mcp registration".to_string(),
        passed: mcp_failures == 0,
        message: if mcp_failures == 0 {
            "Required MCP registrations look healthy.".to_string()
        } else {
            format!("{mcp_failures} MCP registration entries need attention")
        },
        repair_actions: if mcp_failures == 0 {
            Vec::new()
        } else {
            vec![RepairAction::stipe(
                "repair-init",
                "Repair shared MCP registrations",
                "Reapply shared MCP configuration across detected hosts.",
                &["init", "--repair"],
                RepairTier::Primary,
            )]
        },
    });
}

#[allow(clippy::too_many_arguments)]
fn add_core_checks(
    checks: &mut Vec<HealthCheck>,
    runtime_policy: &runtime_policy::RuntimePolicyReport,
    saved_profile: Option<&install::SavedInstallProfile>,
    package_inventory: &PackageInventory,
    plugin_inventory: &PluginInventory,
    worktree_config: &WorktreeConfigDiscovery,
    drift_state: &ConfigDriftState,
    package_drift_check: HealthCheck,
) {
    checks.push(HealthCheck {
        name: "runtime policy".to_string(),
        passed: !runtime_policy::policy_conflicts_with_active_profile(runtime_policy),
        message: runtime_policy::describe_runtime_policy(runtime_policy),
        repair_actions: Vec::new(),
    });
    checks.push(check_task_linked_council(
        saved_profile,
        package_inventory,
        plugin_inventory,
        worktree_config,
    ));
    checks.extend([
        check_shared_storage_root(),
        check_hyphae_db(),
        check_canopy_wal_mode(),
        check_rhizome_compiled_env(),
        check_capability_registry_health(&tool_registry::default_registry_path()),
        drift_state.check.clone(),
        package_drift_check,
    ]);
    if let Some(check) = check_codex_notify() {
        checks.push(check);
    }
}

fn add_ownership_checks(checks: &mut Vec<HealthCheck>) {
    let ownership_check = check_install_ownership();
    if let Some(check) = ownership_check {
        checks.push(check);
    }
}

fn add_hook_checks(checks: &mut Vec<HealthCheck>, hook_paths: &[claude_hooks::HookPathSnapshot]) {
    let hook_failures = hook_paths.iter().filter(|hook| !hook.passed).count();
    if !hook_paths.is_empty() {
        checks.push(HealthCheck {
            name: "hook paths".to_string(),
            passed: hook_failures == 0,
            message: if hook_failures == 0 {
                "All configured hook paths are present.".to_string()
            } else {
                let stale: Vec<_> = hook_paths
                    .iter()
                    .filter(|h| !h.passed)
                    .map(|h| format!("  {} ({})", h.path.display(), h.event))
                    .collect();
                format!(
                    "{hook_failures} hook path(s) not found on disk:\n{}",
                    stale.join("\n")
                )
            },
            repair_actions: if hook_failures == 0 {
                Vec::new()
            } else {
                vec![RepairAction::stipe(
                    "repair-hooks",
                    "Repair stale hook paths",
                    "Reinstall hooks to restore missing hook scripts.",
                    &["init", "--repair"],
                    RepairTier::Primary,
                )]
            },
        });
    }

    // Verify that cortina/annulus hook commands reference runnable binaries.
    if let Some(runnability_check) = check_hook_command_runnability() {
        checks.push(runnability_check);
    }

    // Check stipe.toml sync state (only present when stipe.toml exists).
    if let Some(sync_check) = check_stipe_toml_sync() {
        checks.push(sync_check);
    }

    // Check user-registered hook commands across all scopes.
    for scope in [
        host_policy::HostConfigScope::User,
        host_policy::HostConfigScope::Project,
        host_policy::HostConfigScope::Local,
    ] {
        let user_hook_checks = check_claude_hook_commands(scope);
        checks.extend(user_hook_checks);
    }

    // Check codex notify entries across supported scopes.
    for scope in [
        host_policy::HostConfigScope::User,
        host_policy::HostConfigScope::Project,
    ] {
        let codex_checks = check_codex_notify_entries(scope);
        checks.extend(codex_checks);
    }
}

/// Check which ecosystem tools are stipe-managed vs user-added.
///
/// Returns a single informational health check. Always passes — this is additive
/// metadata and does not block a healthy doctor report.
fn check_install_ownership() -> Option<HealthCheck> {
    let specs = tool_registry::doctor_specs();
    if specs.is_empty() {
        return None;
    }

    let mut managed: Vec<&'static str> = Vec::new();
    let mut untracked: Vec<&'static str> = Vec::new();

    for spec in &specs {
        let installed = matches!(
            tool_registry::probe_with_level(spec, tool_registry::VerifyLevel::Version),
            tool_registry::ToolProbe::Installed(_) | tool_registry::ToolProbe::Broken
        );
        if !installed {
            continue;
        }

        if verify::is_stipe_managed(spec.name) {
            managed.push(spec.name);
        } else {
            untracked.push(spec.name);
        }
    }

    let message = match (managed.is_empty(), untracked.is_empty()) {
        (true, true) => "No ecosystem tools detected.".to_string(),
        (false, true) => format!(
            "All detected tools are stipe-managed: {}.",
            managed.join(", ")
        ),
        (true, false) => format!(
            "All detected tools are user-managed (not installed by stipe): {}.",
            untracked.join(", ")
        ),
        (false, false) => format!(
            "stipe-managed: {}; user-managed: {}.",
            managed.join(", "),
            untracked.join(", ")
        ),
    };

    Some(HealthCheck {
        name: "install ownership".to_string(),
        passed: true,
        message,
        repair_actions: Vec::new(),
    })
}

fn build_report(include_developer_tools: bool, deep: bool) -> DoctorReport {
    build_report_with_saved_profile(install::load_saved_profile(), include_developer_tools, deep)
}

/// Returns `true` if the current ecosystem state passes all doctor checks.
///
/// Used by other commands (e.g. rollback) that need to verify system health
/// after performing an operation, without relying on the subprocess exit code.
pub fn check_health() -> bool {
    build_report(false, false).healthy
}

pub fn run(json: bool, developer: bool, deep: bool) -> Result<()> {
    let report = build_report(developer, deep);

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    for line in render_report(&report, true, deep) {
        println!("{line}");
    }

    if !json && report.healthy {
        crate::banner::print_banner();
    }

    Ok(())
}

/// Returns the doctor report as a JSON string without printing, for use by the socket endpoint.
#[cfg(unix)]
pub fn run_json_string(developer: bool, deep: bool) -> Result<String> {
    let report = build_report(developer, deep);
    Ok(serde_json::to_string_pretty(&report)?)
}
