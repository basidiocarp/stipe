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
mod instruction_checks;
pub(crate) mod model;
mod package_checks;
mod plugin_inventory_checks;
pub(crate) mod provider_checks;
mod server_checks;
mod tool_checks;

use config_checks::check_mcp_config_drift;
use council_checks::check_task_linked_council;
use instruction_checks::check_instruction_files;
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
use tool_checks::{
    check_capability_registry_health, check_hyphae_db, check_mcp_startups, check_profile_tools,
    check_shared_storage_root, check_tool,
};

const STIPE_DOCTOR_SCHEMA_VERSION: &str = "1.0";

#[cfg(test)]
mod tests;

fn render_check_line(check: &HealthCheck, colorize: bool) -> String {
    let (symbol, message) = if check.passed {
        ("✓", check.message.clone())
    } else {
        ("✗", check.message.clone())
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

    format!("  {name:<20} {symbol} {message}")
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

#[allow(clippy::too_many_lines)]
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

    if let Some(profile) = &report.install_profile {
        lines.push(format!(
            "Install profile: {} ({})",
            profile.profile,
            host_policy::format_user_path(&profile.config_path)
        ));
        lines.push(String::new());
    }

    lines.extend(render_overview(report, colorize));
    lines.push(String::new());

    lines.extend(render_provider_health(
        &report.provider_health,
        colorize,
        deep,
    ));
    if !report.provider_health.is_empty() {
        lines.push(String::new());
    }

    lines.extend(render_mcp_health(&report.mcp_health, colorize, deep));
    if !report.mcp_health.is_empty() {
        lines.push(String::new());
    }

    let host_checks = report
        .checks
        .iter()
        .filter(|check| is_host_check(check))
        .collect::<Vec<_>>();
    if !host_checks.is_empty() {
        lines.extend(render_host_status(&host_checks, colorize));
        lines.push(String::new());
    }

    if let Some(runtime_policy) = &report.runtime_policy {
        lines.extend(render_runtime_policy(runtime_policy, colorize, deep));
        lines.push(String::new());
    }

    if let Some(worktree) = &report.worktree_config {
        lines.extend(render_worktree_config(worktree, colorize, deep));
        lines.push(String::new());
    }

    if let Some(inventory) = &report.package_inventory {
        lines.extend(render_package_inventory(inventory, colorize, deep));
        lines.push(String::new());
    }

    if let Some(drift) = &report.package_drift {
        lines.extend(render_package_drift(drift, colorize, deep));
        lines.push(String::new());
    }

    lines.extend(render_hook_paths(&report.hook_paths, colorize));
    if !report.hook_paths.is_empty() {
        lines.push(String::new());
    }

    lines.extend(render_mcp_server_health(
        &report.mcp_server_health,
        colorize,
    ));
    if !report.mcp_server_health.is_empty() {
        lines.push(String::new());
    }

    lines.extend(render_api_key_health(&report.api_key_health, colorize));
    if !report.api_key_health.is_empty() {
        lines.push(String::new());
    }

    if let Some(plugin_inventory) = &report.plugin_inventory {
        lines.extend(render_plugin_inventory(plugin_inventory, colorize, deep));
        lines.push(String::new());
    }

    if let Some(drift) = &report.drift
        && !drift.findings.is_empty()
    {
        lines.extend(render_drift_report(drift, colorize));
        lines.push(String::new());
    }

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
        let repair_plan = build_repair_plan(&report.repair_actions);
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

    lines.push(String::new());

    if let Some(developer_tools) = &report.developer_tools {
        lines.extend(developer_tools::render_report(developer_tools));
    }

    lines
}

fn render_overview(report: &DoctorReport, colorize: bool) -> Vec<String> {
    let mut lines = vec![if colorize {
        "Overview:".bold().to_string()
    } else {
        "Overview:".to_string()
    }];

    lines.extend(
        report
            .checks
            .iter()
            .filter(|check| !is_host_check(check))
            .map(|check| render_check_line(check, colorize)),
    );

    let host_checks = report
        .checks
        .iter()
        .filter(|check| is_host_check(check))
        .collect::<Vec<_>>();
    if !host_checks.is_empty() {
        lines.push(render_check_line(
            &host_summary_check(&host_checks),
            colorize,
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

fn build_repair_plan(repair_actions: &[RepairAction]) -> RepairPlan {
    let mut actions = repair_actions.to_vec();
    actions.sort_by_key(repair_action_priority);

    let primary = actions
        .iter()
        .find(|action| action.tier == RepairTier::Primary)
        .cloned()
        .or_else(|| actions.first().cloned())
        .expect("repair plan requires at least one action");

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

    RepairPlan {
        primary,
        follow_up,
        remaining_primary,
        secondary,
        manual,
    }
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

#[allow(clippy::too_many_lines)]
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

    let mut checks = if let Some(saved_profile) = &saved_profile {
        check_profile_tools(saved_profile.profile, deep)
    } else {
        tool_registry::doctor_specs()
            .into_iter()
            .map(|spec| check_tool(spec, deep))
            .collect::<Vec<_>>()
    };
    let mut hook_paths = claude_hooks::hook_path_snapshots();
    // Also check lamella hook paths if available
    hook_paths.extend(claude_hooks::lamella_hook_path_snapshots());
    let drift_state = check_mcp_config_drift();
    let provider_failures = provider_health
        .iter()
        .filter(|provider| !provider.healthy)
        .count();
    let mcp_failures = mcp_health.iter().filter(|mcp| !mcp.healthy).count();
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
    checks.push(HealthCheck {
        name: "runtime policy".to_string(),
        passed: !runtime_policy::policy_conflicts_with_active_profile(&runtime_policy),
        message: runtime_policy::describe_runtime_policy(&runtime_policy),
        repair_actions: Vec::new(),
    });
    checks.push(check_task_linked_council(
        saved_profile.as_ref(),
        &package_inventory,
        &plugin_inventory,
        &worktree_config,
    ));
    checks.extend([
        check_shared_storage_root(),
        check_hyphae_db(),
        check_capability_registry_health(&tool_registry::default_registry_path()),
        drift_state.check.clone(),
        package_drift_check,
    ]);
    if deep {
        checks.extend(check_mcp_startups());
    }

    // Additive ownership check: surface stipe-managed vs user-managed tools.
    // Does not affect the overall healthy flag for tools not in the registry.
    let ownership_check = check_install_ownership();
    if let Some(check) = ownership_check {
        checks.push(check);
    }

    checks.extend(host_health_checks());

    // Check that instruction files (CLAUDE.md, AGENTS.md) exist at expected ecosystem locations.
    checks.extend(check_instruction_files());

    let hook_failures = hook_paths.iter().filter(|hook| !hook.passed).count();
    let healthy = checks.iter().all(|check| check.passed) && hook_failures == 0;
    let failing = checks.iter().filter(|check| !check.passed).count() + hook_failures;
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

    if report.healthy {
        crate::banner::print_banner();
    }

    Ok(())
}

/// Returns the doctor report as a JSON string without printing, for use by the socket endpoint.
pub fn run_json_string(developer: bool, deep: bool) -> Result<String> {
    let report = build_report(developer, deep);
    Ok(serde_json::to_string_pretty(&report)?)
}
