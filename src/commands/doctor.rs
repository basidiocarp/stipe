use anyhow::Result;
use colored::Colorize;

use super::claude_hooks;
use super::developer_tools;
use super::host;
use super::host_policy;
use super::install;
use super::repair::{RepairAction, RepairTier, dedupe_repair_actions};
use super::runtime_policy;
use super::tool_registry;

mod config_checks;
mod council_checks;
mod model;
mod package_checks;
mod provider_checks;
mod tool_checks;

use config_checks::check_mcp_config_drift;
use council_checks::check_task_linked_council;
use model::{
    DoctorReport, DriftFinding, DriftReport, HealthCheck, InstallProfileSummary, PackageDrift,
    PackageInventory, ProviderHealth, WorktreeConfigDiscovery,
};
use package_checks::{
    collect_package_drift, collect_package_inventory, collect_worktree_config_discovery,
};
use provider_checks::{collect_mcp_health, collect_provider_health};
use tool_checks::{check_hyphae_db, check_mcp_startups, check_profile_tools, check_tool};

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

fn render_report(report: &DoctorReport, colorize: bool) -> Vec<String> {
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

    lines.extend(
        report
            .checks
            .iter()
            .map(|check| render_check_line(check, colorize)),
    );
    lines.push(String::new());

    lines.extend(render_provider_health(&report.provider_health, colorize));
    if !report.provider_health.is_empty() {
        lines.push(String::new());
    }

    lines.extend(render_mcp_health(&report.mcp_health, colorize));
    if !report.mcp_health.is_empty() {
        lines.push(String::new());
    }

    if let Some(runtime_policy) = &report.runtime_policy {
        lines.extend(render_runtime_policy(runtime_policy, colorize));
        lines.push(String::new());
    }

    if let Some(worktree) = &report.worktree_config {
        lines.extend(render_worktree_config(worktree, colorize));
        lines.push(String::new());
    }

    if let Some(inventory) = &report.package_inventory {
        lines.extend(render_package_inventory(inventory, colorize));
        lines.push(String::new());
    }

    if let Some(drift) = &report.package_drift {
        lines.extend(render_package_drift(drift, colorize));
        lines.push(String::new());
    }

    lines.extend(render_hook_paths(&report.hook_paths, colorize));
    if !report.hook_paths.is_empty() {
        lines.push(String::new());
    }

    if let Some(drift) = &report.drift
        && !drift.findings.is_empty()
    {
        lines.extend(render_drift_report(drift, colorize));
        lines.push(String::new());
    }

    if report.healthy {
        lines.push(if colorize {
            "All checks passed.".green().to_string()
        } else {
            "All checks passed.".to_string()
        });
    } else {
        lines.push(if colorize {
            "Some checks failed. Use 'stipe init --repair' to repair shared MCP state, 'stipe host doctor' to inspect per-host state, or 'stipe host setup <host>' to restore a specific host.".yellow().to_string()
        } else {
            "Some checks failed. Use 'stipe init --repair' to repair shared MCP state, 'stipe host doctor' to inspect per-host state, or 'stipe host setup <host>' to restore a specific host.".to_string()
        });
        if !report.repair_actions.is_empty() {
            lines.push(String::new());
            lines.push(if colorize {
                "Recommended repair actions:".bold().to_string()
            } else {
                "Recommended repair actions:".to_string()
            });
            lines.extend(
                report
                    .repair_actions
                    .iter()
                    .map(|action| format!("  - {}", action.command)),
            );
        }
    }

    lines.push(String::new());

    if let Some(developer_tools) = &report.developer_tools {
        lines.extend(developer_tools::render_report(developer_tools));
    }

    lines
}

fn render_provider_health(provider_health: &[ProviderHealth], colorize: bool) -> Vec<String> {
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
        let status = if colorize {
            if provider.healthy {
                provider.status.green().to_string()
            } else {
                provider.status.yellow().to_string()
            }
        } else {
            provider.status.clone()
        };
        lines.push(format!(
            "  {symbol} {:<12} {:<18} {}",
            provider.host.client_flag(),
            provider.provider,
            status
        ));
        if let Some(auth_detail) = &provider.auth_detail {
            lines.push(format!("    auth: {:?}", provider.auth_freshness).to_lowercase());
            lines.push(format!("    detail: {auth_detail}"));
        } else {
            lines.push(format!("    auth: {:?}", provider.auth_freshness).to_lowercase());
        }
    }

    lines
}

fn render_mcp_health(mcp_health: &[model::McpHealth], colorize: bool) -> Vec<String> {
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
        let status = if colorize {
            if mcp.healthy {
                mcp.status.green().to_string()
            } else {
                mcp.status.yellow().to_string()
            }
        } else {
            mcp.status.clone()
        };
        lines.push(format!(
            "  {symbol} {:<12} {}",
            mcp.host.client_flag(),
            status
        ));
        if !mcp.config_paths.is_empty() {
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
        lines.push(format!("    auth: {:?}", mcp.auth_freshness).to_lowercase());
    }

    lines
}

fn render_worktree_config(report: &WorktreeConfigDiscovery, colorize: bool) -> Vec<String> {
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
        lines.extend(
            report
                .discovered_configs
                .iter()
                .map(|path| format!("  config: {}", host_policy::format_user_path(path))),
        );
    }

    lines
}

fn render_runtime_policy(
    report: &runtime_policy::RuntimePolicyReport,
    colorize: bool,
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
        lines.extend(
            report
                .config_paths
                .iter()
                .map(|path| format!("  config: {}", host_policy::format_user_path(path))),
        );
    }

    if let Some(load_error) = &report.load_error {
        lines.push(format!("  load error: {load_error}"));
    }

    if report.remembered_decisions.is_empty() {
        lines.push("  approval memory: none recorded".to_string());
    } else {
        lines.push("  approval memory:".to_string());
        lines.extend(report.remembered_decisions.iter().map(|decision| {
            format!(
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
                    runtime_policy::DecisionSource::OperatorPolicyFile => "operator-policy-file",
                    runtime_policy::DecisionSource::ImportedConfig => "imported-config",
                },
                decision.updated_at_unix
            )
        }));
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

fn render_package_inventory(report: &PackageInventory, colorize: bool) -> Vec<String> {
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
    if !report.metadata_sources.is_empty() {
        lines.extend(
            report
                .metadata_sources
                .iter()
                .map(|path| format!("  metadata: {}", host_policy::format_user_path(path))),
        );
    }
    if report.discovered_packages.is_empty() {
        lines.push("  packages: none discovered".to_string());
    } else {
        lines.push(format!(
            "  packages: {}",
            report.discovered_packages.join(", ")
        ));
    }
    if report.discovered_plugins.is_empty() {
        lines.push("  plugins: none discovered".to_string());
    } else {
        lines.push(format!(
            "  plugins: {}",
            report.discovered_plugins.join(", ")
        ));
    }

    lines
}

fn render_package_drift(report: &PackageDrift, colorize: bool) -> Vec<String> {
    let mut lines = vec![if colorize {
        "Package drift:".bold().to_string()
    } else {
        "Package drift:".to_string()
    }];
    lines.push(format!(
        "  metadata available: {}",
        if report.metadata_available {
            "yes"
        } else {
            "no"
        }
    ));
    if report.expected_packages.is_empty() {
        lines.push("  expected: none".to_string());
    } else {
        lines.push(format!(
            "  expected: {}",
            report.expected_packages.join(", ")
        ));
    }
    if report.missing_packages.is_empty() {
        lines.push("  missing: none".to_string());
    } else {
        let missing = if colorize {
            report.missing_packages.join(", ").yellow().to_string()
        } else {
            report.missing_packages.join(", ")
        };
        lines.push(format!("  missing: {missing}"));
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
    let hook_paths = claude_hooks::hook_path_snapshots();
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
        &worktree_config,
    ));
    checks.extend([
        check_hyphae_db(),
        drift_state.check.clone(),
        package_drift_check,
    ]);
    if deep {
        checks.extend(check_mcp_startups());
    }
    checks.extend(host_health_checks());

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
    }
}

fn build_report(include_developer_tools: bool, deep: bool) -> DoctorReport {
    build_report_with_saved_profile(install::load_saved_profile(), include_developer_tools, deep)
}

pub fn run(json: bool, developer: bool, deep: bool) -> Result<()> {
    let report = build_report(developer, deep);

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    for line in render_report(&report, true) {
        println!("{line}");
    }

    if report.healthy {
        crate::banner::print_banner();
    }

    Ok(())
}
