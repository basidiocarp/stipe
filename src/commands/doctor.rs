use anyhow::Result;
use colored::Colorize;

use super::developer_tools;
use super::host;
use super::host_policy;
use super::repair::dedupe_repair_actions;
use super::tool_registry;

mod config_checks;
mod model;
mod tool_checks;

use config_checks::check_mcp_config_drift;
use model::{DoctorReport, DriftFinding, DriftReport, HealthCheck};
use tool_checks::{check_hyphae_db, check_mcp_startups, check_tool};

const STIPE_DOCTOR_SCHEMA_VERSION: &str = "1.0";

#[cfg(test)]
use config_checks::{codex_notify_adapter_configured_at_path, config_mentions_servers};
#[cfg(test)]
use model::ConfigFormat;
#[cfg(test)]
use tool_checks::check_hyphae_db_at_path;

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

    let mut lines = vec![
        if colorize {
            "Config drift detected:".bold().to_string()
        } else {
            "Config drift detected:".to_string()
        },
    ];

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

    lines.extend(
        report
            .checks
            .iter()
            .map(|check| render_check_line(check, colorize)),
    );
    lines.push(String::new());

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

fn build_report(include_developer_tools: bool, deep: bool) -> DoctorReport {
    let mut checks = tool_registry::doctor_specs()
        .into_iter()
        .map(|spec| check_tool(spec, deep))
        .collect::<Vec<_>>();
    let drift_state = check_mcp_config_drift();
    checks.extend([check_hyphae_db(), drift_state.check.clone()]);
    if deep {
        checks.extend(check_mcp_startups());
    }
    checks.extend(host_health_checks());

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
        checks,
        repair_actions,
        drift: drift_state.report,
        developer_tools: include_developer_tools.then(developer_tools::doctor_report),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::codex_notify;
    use crate::commands::developer_tools::DeveloperToolTier;
    use crate::commands::repair::{RepairAction, RepairTier};
    use crate::commands::tool_registry::ToolProbe;
    use std::fs;

    #[test]
    fn test_check_hyphae_db_exists() {
        let temp_dir = std::env::temp_dir().join("stipe-test-hyphae-exists");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let db_path = temp_dir.join("hyphae.db");
        fs::write(&db_path, "").unwrap();

        let check = check_hyphae_db_at_path(&db_path);
        assert!(check.passed, "Should pass when database exists");
        assert_eq!(check.name, "hyphae database");
        assert!(check.message.contains("initialized"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_check_hyphae_db_missing() {
        let temp_dir = std::env::temp_dir().join("stipe-test-hyphae-missing");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let db_path = temp_dir.join("nonexistent.db");

        let check = check_hyphae_db_at_path(&db_path);
        assert!(!check.passed, "Should fail when database does not exist");
        assert_eq!(check.name, "hyphae database");
        assert!(check.message.contains("not found"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_config_mentions_servers_detects_required_names() {
        let content =
            r#"{"mcpServers":{"hyphae":{"command":"hyphae"},"rhizome":{"command":"rhizome"}}}"#;

        assert!(config_mentions_servers(
            content,
            &["hyphae", "rhizome"],
            ConfigFormat::Json
        ));
        assert!(!config_mentions_servers(
            content,
            &["hyphae", "cortina"],
            ConfigFormat::Json
        ));
    }

    #[test]
    fn test_config_mentions_servers_detects_codex_toml() {
        let content = r#"
            [mcp_servers.hyphae]
            command = "hyphae"
            args = ["serve"]

            [mcp_servers.rhizome]
            command = "rhizome"
            args = ["serve", "--expanded"]
        "#;

        assert!(config_mentions_servers(
            content,
            &["hyphae", "rhizome"],
            ConfigFormat::Toml
        ));
        assert!(!config_mentions_servers(
            content,
            &["hyphae", "cortina"],
            ConfigFormat::Toml
        ));
    }

    #[test]
    fn test_codex_notify_adapter_configured_at_path_detects_notify_entry() {
        let temp_dir = std::env::temp_dir().join("stipe-test-codex-notify");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let config_path = temp_dir.join("config.toml");
        fs::write(&config_path, r#"notify = ["hyphae", "codex-notify"]"#).unwrap();

        assert!(codex_notify_adapter_configured_at_path(&config_path));

        fs::write(&config_path, r#"notify = ["hyphae", "something-else"]"#).unwrap();
        assert!(!codex_notify_adapter_configured_at_path(&config_path));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_build_report_includes_host_inventory_checks() {
        let report = build_report(false, false);
        let names = report
            .checks
            .iter()
            .map(|check| check.name.as_str())
            .collect::<Vec<_>>();

        assert!(names.iter().any(|name| name.contains("host: claude-code")));
        assert!(names.iter().any(|name| name.contains("host: codex")));
    }

    #[test]
    fn test_codex_notify_helpers_are_shared() {
        let detail = codex_notify::codex_notify_detail(false);
        assert!(detail.contains("Codex"));
        assert!(host_policy::codex_target_requested(Some("codex")));
    }

    #[test]
    fn test_health_check_struct() {
        let check = HealthCheck {
            name: "test".to_string(),
            passed: true,
            message: "Test passed".to_string(),
            repair_actions: Vec::new(),
        };

        assert_eq!(check.name, "test");
        assert!(check.passed);
        assert_eq!(check.message, "Test passed");
    }

    #[test]
    fn test_optional_canopy_missing_is_not_a_failure() {
        let canopy = tool_registry::find("canopy").expect("canopy spec should exist");
        let check = check_tool(canopy, false);

        if !matches!(tool_registry::probe(canopy), ToolProbe::Missing) {
            return;
        }

        assert!(check.passed);
        assert!(check.message.contains("Optional coordination runtime"));
    }

    #[test]
    fn test_build_report_includes_repair_actions_for_failures() {
        let report = DoctorReport {
            schema_version: STIPE_DOCTOR_SCHEMA_VERSION.to_string(),
            healthy: false,
            summary: "1 checks need attention.".to_string(),
            checks: vec![HealthCheck {
                name: "hyphae database".to_string(),
                passed: false,
                message: "missing".to_string(),
                repair_actions: vec![RepairAction::stipe(
                    "init",
                    "Initialize the ecosystem",
                    "Create the database.",
                    &["init"],
                    RepairTier::Primary,
                )],
            }],
            repair_actions: vec![RepairAction::stipe(
                "init",
                "Initialize the ecosystem",
                "Create the database.",
                &["init"],
                RepairTier::Primary,
            )],
            drift: None,
            developer_tools: None,
        };

        assert!(!report.healthy);
        assert_eq!(report.repair_actions.len(), 1);
        assert_eq!(report.repair_actions[0].command, "stipe init");
    }

    #[test]
    fn test_render_report_snapshot_for_failure() {
        let report = DoctorReport {
            schema_version: STIPE_DOCTOR_SCHEMA_VERSION.to_string(),
            healthy: false,
            summary: "1 checks need attention.".to_string(),
            checks: vec![HealthCheck {
                name: "hyphae".to_string(),
                passed: false,
                message: "not found in PATH".to_string(),
                repair_actions: vec![],
            }],
            repair_actions: vec![RepairAction::stipe(
                "install hyphae",
                "Install hyphae",
                "Add hyphae to PATH.",
                &["install", "hyphae"],
                RepairTier::Primary,
            )],
            drift: None,
            developer_tools: None,
        };

        assert_eq!(
            render_report(&report, false),
            vec![
                "",
                "Basidiocarp Ecosystem Health Check",
                &"─".repeat(75),
                "",
                "  hyphae               ✗ not found in PATH",
                "",
                "Some checks failed. Use 'stipe init --repair' to repair shared MCP state, 'stipe host doctor' to inspect per-host state, or 'stipe host setup <host>' to restore a specific host.",
                "",
                "Recommended repair actions:",
                "  - stipe install hyphae",
                "",
            ]
        );
    }

    #[test]
    fn test_render_report_includes_drift_section() {
        let report = DoctorReport {
            schema_version: STIPE_DOCTOR_SCHEMA_VERSION.to_string(),
            healthy: false,
            summary: "1 checks need attention.".to_string(),
            checks: vec![HealthCheck {
                name: "config drift".to_string(),
                passed: false,
                message: "1 config drift issue(s) detected".to_string(),
                repair_actions: vec![RepairAction::stipe(
                    "repair-init",
                    "Repair the init baseline",
                    "Reapply shared ecosystem configuration and refresh the baseline manifest.",
                    &["init", "--repair"],
                    RepairTier::Primary,
                )],
            }],
            repair_actions: vec![RepairAction::stipe(
                "repair-init",
                "Repair the init baseline",
                "Reapply shared ecosystem configuration and refresh the baseline manifest.",
                &["init", "--repair"],
                RepairTier::Primary,
            )],
            drift: Some(DriftReport {
                baseline_path: std::path::PathBuf::from("/tmp/init-baseline.json"),
                generated_at_unix_nanos: 1,
                findings: vec![DriftFinding::ConfigFileModified {
                    path: std::path::PathBuf::from("/tmp/config.json"),
                    expected_checksum: "deadbeef".to_string(),
                    actual_checksum: Some("cafebabe".to_string()),
                }],
            }),
            developer_tools: None,
        };

        let lines = render_report(&report, false);
        assert!(lines.iter().any(|line| line.contains("Config drift detected:")));
        assert!(lines.iter().any(|line| line.contains("modified since last init")));
        assert!(lines.iter().any(|line| line.contains("stipe init --repair")));
    }

    #[test]
    fn test_build_report_can_include_developer_tools() {
        let report = build_report(true, false);
        let developer_tools = report
            .developer_tools
            .expect("developer tools section should be present");
        assert!(
            developer_tools
                .checks
                .iter()
                .any(|check| check.tier == DeveloperToolTier::Tier1)
        );
    }
}
