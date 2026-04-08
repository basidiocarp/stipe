use anyhow::Result;
use colored::Colorize;

use super::claude_hooks;
use super::developer_tools;
use super::host;
use super::host_policy;
use super::install;
use super::repair::dedupe_repair_actions;
use super::tool_registry;

mod config_checks;
mod model;
mod tool_checks;

use config_checks::check_mcp_config_drift;
use model::{DoctorReport, DriftFinding, DriftReport, HealthCheck, InstallProfileSummary};
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
    checks.extend([check_hyphae_db(), drift_state.check.clone()]);
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
