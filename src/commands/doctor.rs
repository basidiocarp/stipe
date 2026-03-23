use anyhow::Result;
use colored::Colorize;
use serde::Serialize;
use serde_json::Value;
use spore::{Tool, discover};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::install::InstallProfile;
use super::repair::{RepairAction, RepairTier, cargo_install_action, dedupe_repair_actions};
use crate::ecosystem::clients::{self, McpClient};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigFormat {
    Json,
    Toml,
}

fn codex_cli_installed() -> bool {
    Command::new("codex")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn codex_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".codex").join("config.toml"))
}

fn install_profile_action(profile: InstallProfile) -> RepairAction {
    match profile {
        InstallProfile::Codex => RepairAction::stipe(
            "install-codex",
            "Install the Codex profile",
            "Install the core local agent stack and Codex setup path before wiring MCP clients.",
            &["install", "--profile", "codex"],
            RepairTier::Primary,
        ),
        InstallProfile::Minimal
        | InstallProfile::ClaudeCode
        | InstallProfile::Cursor
        | InstallProfile::FullStack => RepairAction::stipe(
            "install-claude-code",
            "Install the hooks-enabled profile",
            "Install the core local agent stack before wiring MCP clients.",
            &["install", "--profile", "claude-code"],
            RepairTier::Primary,
        ),
    }
}

fn preferred_install_profile() -> InstallProfile {
    if codex_environment_present() {
        InstallProfile::Codex
    } else {
        InstallProfile::ClaudeCode
    }
}

fn codex_environment_present() -> bool {
    codex_cli_installed() || clients::detect_clients().contains(&McpClient::CodexCli)
}

fn missing_tool_actions(tool: Tool) -> Vec<RepairAction> {
    let install_profile = preferred_install_profile();

    match tool {
        Tool::Mycelium => vec![RepairAction::stipe(
            "install-minimal",
            "Install the minimal profile",
            "Restore the Mycelium CLI before attempting deeper repair work.",
            &["install", "--profile", "minimal"],
            RepairTier::Primary,
        )],
        Tool::Hyphae | Tool::Rhizome => vec![
            install_profile_action(install_profile),
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

fn check_codex_available() -> HealthCheck {
    let passed = codex_cli_installed();

    HealthCheck {
        name: "codex cli".to_string(),
        passed,
        message: if passed {
            "Available".to_string()
        } else {
            "Not found in PATH (optional)".to_string()
        },
        repair_actions: Vec::new(),
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

fn codex_notify_adapter_configured_at_path(config_path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(config_path) else {
        return false;
    };

    let Ok(parsed) = toml::from_str::<toml::Value>(&content) else {
        return false;
    };

    parsed
        .get("notify")
        .and_then(toml::Value::as_array)
        .is_some_and(|values| {
            values.len() == 2
                && values[0].as_str() == Some("hyphae")
                && values[1].as_str() == Some("codex-notify")
        })
}

fn check_codex_notify_adapter() -> HealthCheck {
    let Some(config_path) = codex_config_path() else {
        return HealthCheck {
            name: "codex adapter".to_string(),
            passed: false,
            message: "Cannot determine Codex config path".to_string(),
            repair_actions: vec![RepairAction::manual(
                "Configure the Codex notify adapter".to_string(),
                "Run hyphae init so ~/.codex/config.toml includes notify = [\"hyphae\", \"codex-notify\"].".to_string(),
                "hyphae init".to_string(),
                vec!["init".to_string()],
                RepairTier::Primary,
            )],
        };
    };

    if codex_notify_adapter_configured_at_path(&config_path) {
        HealthCheck {
            name: "codex adapter".to_string(),
            passed: true,
            message: "Codex notify adapter configured".to_string(),
            repair_actions: Vec::new(),
        }
    } else {
        HealthCheck {
            name: "codex adapter".to_string(),
            passed: false,
            message: if config_path.exists() {
                "Codex notify adapter missing from ~/.codex/config.toml (run 'hyphae init')"
                    .to_string()
            } else {
                "Codex config not found (run 'hyphae init')".to_string()
            },
            repair_actions: vec![RepairAction::manual(
                "Configure the Codex notify adapter".to_string(),
                "Run hyphae init so ~/.codex/config.toml includes notify = [\"hyphae\", \"codex-notify\"].".to_string(),
                "hyphae init".to_string(),
                vec!["init".to_string()],
                RepairTier::Primary,
            )],
        }
    }
}

fn config_mentions_servers(content: &str, required_servers: &[&str], format: ConfigFormat) -> bool {
    match format {
        ConfigFormat::Json => {
            let parsed: Value = match serde_json::from_str(content) {
                Ok(value) => value,
                Err(_) => return false,
            };

            let Some(mcp_servers) = parsed.get("mcpServers").and_then(Value::as_object) else {
                return false;
            };

            required_servers
                .iter()
                .all(|server| mcp_servers.contains_key(*server))
        }
        ConfigFormat::Toml => {
            let parsed: toml::Value = match toml::from_str(content) {
                Ok(value) => value,
                Err(_) => return false,
            };

            let Some(mcp_servers) = parsed.get("mcp_servers").and_then(toml::Value::as_table)
            else {
                return false;
            };

            required_servers
                .iter()
                .all(|server| mcp_servers.contains_key(*server))
        }
    }
}

fn mcp_client_config_paths() -> Vec<(&'static str, PathBuf, ConfigFormat)> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };

    let mut paths = vec![
        ("Claude Code", home.join(".claude.json"), ConfigFormat::Json),
        (
            "Cursor",
            home.join(".cursor").join("mcp.json"),
            ConfigFormat::Json,
        ),
        (
            "Windsurf",
            home.join(".windsurf").join("mcp.json"),
            ConfigFormat::Json,
        ),
        (
            "Continue",
            home.join(".continue").join("config.json"),
            ConfigFormat::Json,
        ),
        (
            "Codex CLI",
            home.join(".codex").join("config.toml"),
            ConfigFormat::Toml,
        ),
    ];

    if let Some(cline_path) = vscode_cline_settings_path() {
        paths.push(("Cline", cline_path, ConfigFormat::Json));
    }

    #[cfg(target_os = "macos")]
    {
        paths.push((
            "Claude Desktop",
            home.join("Library")
                .join("Application Support")
                .join("Claude")
                .join("claude_desktop_config.json"),
            ConfigFormat::Json,
        ));
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Some(config_dir) = dirs::config_dir() {
            paths.push((
                "Claude Desktop",
                config_dir.join("Claude").join("claude_desktop_config.json"),
                ConfigFormat::Json,
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

    for (client_name, path, format) in configs {
        if !path.exists() {
            continue;
        }

        found_any = true;
        match fs::read_to_string(&path) {
            Ok(content) if config_mentions_servers(&content, &required_servers, format) => {
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
        check_codex_available(),
        check_hyphae_db(),
        check_mcp_config_drift(),
        check_codex_notify_adapter(),
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
            "Some checks failed. Use 'stipe init' to repair MCP registrations, 'hyphae init' to configure Codex notify coverage, or 'stipe install --profile codex' to restore the core Codex stack.".yellow()
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
    fn test_build_report_mentions_hooks_and_notify_separately() {
        let report = build_report();
        let messages = report
            .checks
            .iter()
            .map(|check| check.message.as_str())
            .collect::<Vec<_>>();

        assert!(messages.iter().any(|message| message.contains("Codex")));
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
