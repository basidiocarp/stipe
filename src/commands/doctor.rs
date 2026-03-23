use anyhow::Result;
use colored::Colorize;
use serde::Serialize;
use serde_json::Value;
use spore::{Tool, discover};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use super::repair::{RepairAction, RepairTier, cargo_install_action, dedupe_repair_actions};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct HealthCheck {
    name: String,
    passed: bool,
    message: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    repair_actions: Vec<RepairAction>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct DoctorReport {
    healthy: bool,
    summary: String,
    checks: Vec<HealthCheck>,
    repair_actions: Vec<RepairAction>,
}

fn missing_tool_actions(tool: Tool) -> Vec<RepairAction> {
    match tool {
        Tool::Mycelium => vec![RepairAction::stipe(
            "install-minimal",
            "Install the minimal profile",
            "Restore the Mycelium CLI before attempting deeper repair work.",
            &["install", "--profile", "minimal"],
            RepairTier::Primary,
        )],
        Tool::Hyphae | Tool::Rhizome => vec![
            RepairAction::stipe(
                "install-claude-code",
                "Install the Claude Code profile",
                "Restore the core local agent stack and MCP servers.",
                &["install", "--profile", "claude-code"],
                RepairTier::Primary,
            ),
            RepairAction::stipe(
                "install-full-stack",
                "Install the full stack",
                "Install every supported ecosystem tool when you want the broadest local setup.",
                &["install", "--profile", "full-stack"],
                RepairTier::Secondary,
            ),
            match tool {
                Tool::Hyphae => cargo_install_action("hyphae"),
                Tool::Rhizome => cargo_install_action("rhizome"),
                Tool::Mycelium | Tool::Cap => unreachable!(),
            },
        ],
        Tool::Cap => Vec::new(),
    }
}

fn check_tool(tool: Tool) -> HealthCheck {
    let tool_name = format!("{tool:?}").to_lowercase();

    match discover(tool) {
        Some(info) => {
            let cmd_name = match tool {
                Tool::Mycelium => "mycelium",
                Tool::Hyphae => "hyphae",
                Tool::Rhizome => "rhizome",
                Tool::Cap => "cap",
            };

            match Command::new(cmd_name).arg("--version").output() {
                Ok(output) => {
                    let _version = String::from_utf8_lossy(&output.stdout);
                    HealthCheck {
                        name: tool_name,
                        passed: true,
                        message: format!("v{} installed and working", info.version),
                        repair_actions: Vec::new(),
                    }
                }
                Err(e) => HealthCheck {
                    name: tool_name,
                    passed: false,
                    message: format!("Binary found but failed to run: {e}"),
                    repair_actions: missing_tool_actions(tool),
                },
            }
        }
        None => HealthCheck {
            name: tool_name,
            passed: false,
            message: "Not installed".to_string(),
            repair_actions: missing_tool_actions(tool),
        },
    }
}

fn check_hyphae_db() -> HealthCheck {
    if let Some(data_dir) = dirs::data_dir() {
        check_hyphae_db_at_path(&data_dir.join("hyphae").join("hyphae.db"))
    } else {
        HealthCheck {
            name: "hyphae database".to_string(),
            passed: false,
            message: "Cannot determine data directory".to_string(),
            repair_actions: vec![RepairAction::stipe(
                "init",
                "Initialize the ecosystem",
                "Bootstrap Hyphae and MCP client state on this machine.",
                &["init"],
                RepairTier::Primary,
            )],
        }
    }
}

fn check_hyphae_db_at_path(db_path: &std::path::Path) -> HealthCheck {
    if db_path.exists() {
        HealthCheck {
            name: "hyphae database".to_string(),
            passed: true,
            message: "Database initialized".to_string(),
            repair_actions: Vec::new(),
        }
    } else {
        HealthCheck {
            name: "hyphae database".to_string(),
            passed: false,
            message: "Database not found (run 'stipe init' to initialize)".to_string(),
            repair_actions: vec![RepairAction::stipe(
                "init",
                "Initialize the ecosystem",
                "Create the Hyphae database and wire the local ecosystem together.",
                &["init"],
                RepairTier::Primary,
            )],
        }
    }
}

fn installed_mcp_servers() -> Vec<&'static str> {
    let mut servers = Vec::new();

    if discover(Tool::Hyphae).is_some() {
        servers.push("hyphae");
    }
    if discover(Tool::Rhizome).is_some() {
        servers.push("rhizome");
    }

    servers
}

fn config_mentions_servers(content: &str, required_servers: &[&str]) -> bool {
    let parsed: Value = match serde_json::from_str(content) {
        Ok(value) => value,
        Err(_) => return false,
    };

    let rendered = parsed.to_string();
    required_servers
        .iter()
        .all(|server| rendered.contains(&format!("\"{server}\"")))
}

fn mcp_client_config_paths() -> Vec<(&'static str, PathBuf)> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };

    let mut paths = vec![
        ("Claude Code", home.join(".claude.json")),
        ("Cursor", home.join(".cursor").join("mcp.json")),
        ("Windsurf", home.join(".windsurf").join("mcp.json")),
        ("Continue", home.join(".continue").join("config.json")),
    ];

    if let Some(cline_path) = vscode_cline_settings_path() {
        paths.push(("Cline", cline_path));
    }

    #[cfg(target_os = "macos")]
    {
        paths.push((
            "Claude Desktop",
            home.join("Library")
                .join("Application Support")
                .join("Claude")
                .join("claude_desktop_config.json"),
        ));
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Some(config_dir) = dirs::config_dir() {
            paths.push((
                "Claude Desktop",
                config_dir.join("Claude").join("claude_desktop_config.json"),
            ));
        }
    }

    paths
}

fn vscode_cline_settings_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;

    #[cfg(target_os = "macos")]
    {
        Some(
            home.join("Library")
                .join("Application Support")
                .join("Code")
                .join("User")
                .join("settings.json"),
        )
    }
    #[cfg(target_os = "linux")]
    {
        Some(
            home.join(".config")
                .join("Code")
                .join("User")
                .join("settings.json"),
        )
    }
    #[cfg(target_os = "windows")]
    {
        dirs::config_dir().map(|d| d.join("Code").join("User").join("settings.json"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

fn check_mcp_config_drift() -> HealthCheck {
    let required_servers = installed_mcp_servers();
    if required_servers.is_empty() {
        return HealthCheck {
            name: "mcp config".to_string(),
            passed: true,
            message: "No MCP-backed tools installed yet".to_string(),
            repair_actions: Vec::new(),
        };
    }

    let configs = mcp_client_config_paths();
    let mut found_any = false;
    let mut matching_clients = Vec::new();

    for (client_name, path) in configs {
        if !path.exists() {
            continue;
        }

        found_any = true;
        match fs::read_to_string(&path) {
            Ok(content) if config_mentions_servers(&content, &required_servers) => {
                matching_clients.push(client_name.to_string());
            }
            Ok(_) | Err(_) => {}
        }
    }

    if matching_clients.is_empty() {
        return HealthCheck {
            name: "mcp config".to_string(),
            passed: false,
            message: if found_any {
                format!(
                    "MCP client config exists but is missing registrations for {} (run 'stipe init')",
                    required_servers.join(", ")
                )
            } else {
                "No supported MCP client config found (run 'stipe init')".to_string()
            },
            repair_actions: vec![RepairAction::stipe(
                "init",
                "Repair MCP registrations",
                "Re-register Hyphae and Rhizome in supported MCP clients.",
                &["init"],
                RepairTier::Primary,
            )],
        };
    }

    HealthCheck {
        name: "mcp config".to_string(),
        passed: true,
        message: format!(
            "MCP registrations present in {}",
            matching_clients.join(", ")
        ),
        repair_actions: Vec::new(),
    }
}

fn build_report() -> DoctorReport {
    let checks = vec![
        check_tool(Tool::Mycelium),
        check_tool(Tool::Hyphae),
        check_tool(Tool::Rhizome),
        check_hyphae_db(),
        check_mcp_config_drift(),
        check_claude_available(),
    ];

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
            "Some checks failed. Use 'stipe init' to repair config drift or 'stipe install --profile full-stack' to restore missing tools.".yellow()
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

fn check_claude_available() -> HealthCheck {
    let passed = Command::new("claude")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());

    HealthCheck {
        name: "claude code".to_string(),
        passed,
        message: if passed {
            "Available".to_string()
        } else {
            "Not found in PATH (optional)".to_string()
        },
        repair_actions: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

        assert!(config_mentions_servers(content, &["hyphae", "rhizome"]));
        assert!(!config_mentions_servers(content, &["hyphae", "cortina"]));
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
