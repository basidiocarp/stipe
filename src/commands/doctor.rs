use anyhow::Result;
use colored::Colorize;

use super::host;
use super::host_policy;
use super::repair::dedupe_repair_actions;
use super::tool_registry;

mod config_checks;
mod model;
mod tool_checks;

use config_checks::check_mcp_config_drift;
use model::{DoctorReport, HealthCheck};
use tool_checks::{check_hyphae_db, check_tool};

#[cfg(test)]
use config_checks::{codex_notify_adapter_configured_at_path, config_mentions_servers};
#[cfg(test)]
use model::ConfigFormat;
#[cfg(test)]
use tool_checks::check_hyphae_db_at_path;

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

fn build_report() -> DoctorReport {
    let mut checks = tool_registry::doctor_specs()
        .into_iter()
        .map(check_tool)
        .collect::<Vec<_>>();
    checks.extend([check_hyphae_db(), check_mcp_config_drift()]);
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
        healthy,
        summary: if healthy {
            "All checks passed.".to_string()
        } else {
            format!("{failing} checks need attention.")
        },
        checks,
        repair_actions,
    }
}

pub fn run(json: bool) -> Result<()> {
    let report = build_report();

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!();
    println!("{}", "Basidiocarp Ecosystem Health Check".bold());
    println!("{}", "─".repeat(75));
    println!();

    for check in &report.checks {
        let status = if check.passed {
            format!("{} {}", "✓".green(), check.message.green())
        } else {
            format!("{} {}", "✗".red(), check.message.red())
        };

        println!("  {:<20} {}", check.name.bold(), status);
    }

    println!();

    if report.healthy {
        crate::banner::print_banner();
        println!("{}", "All checks passed.".green());
    } else {
        println!(
            "{}",
            "Some checks failed. Use 'stipe init' to repair shared MCP state, 'stipe host doctor' to inspect per-host state, or 'stipe host setup <host>' to restore a specific host.".yellow()
        );
        if !report.repair_actions.is_empty() {
            println!();
            println!("{}", "Recommended repair actions:".bold());
            for action in &report.repair_actions {
                println!("  - {}", action.command);
            }
        }
    }

    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::codex_notify;
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
        let report = build_report();
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
        let check = check_tool(canopy);

        if !matches!(tool_registry::probe(canopy), ToolProbe::Missing) {
            return;
        }

        assert!(check.passed);
        assert!(check.message.contains("Optional coordination runtime"));
    }

    #[test]
    fn test_build_report_includes_repair_actions_for_failures() {
        let report = DoctorReport {
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
        };

        assert!(!report.healthy);
        assert_eq!(report.repair_actions.len(), 1);
        assert_eq!(report.repair_actions[0].command, "stipe init");
    }
}
