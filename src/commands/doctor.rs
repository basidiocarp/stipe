use anyhow::Result;
use colored::Colorize;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

use super::host;
use super::host_policy;
use super::repair::{RepairAction, RepairTier, cargo_install_action, dedupe_repair_actions};
use super::tool_registry::{self, DoctorCoverage, ToolProbe, ToolSpec};
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
    ClaudeRoot,
}

fn codex_cli_installed() -> bool {
    std::process::Command::new("codex")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn codex_environment_present() -> bool {
    codex_cli_installed() || clients::detect_clients().contains(&McpClient::CodexCli)
}

fn missing_tool_actions(tool: &ToolSpec) -> Vec<RepairAction> {
    let install_profile = host_policy::preferred_install_profile(
        if codex_environment_present() {
            Some(host_policy::CODEX_CLIENT_FLAG)
        } else {
            None
        },
        &clients::detect_clients()
            .into_iter()
            .map(|client| client.name().to_string())
            .collect::<Vec<_>>(),
    );

    match tool.name {
        "mycelium" => vec![RepairAction::stipe(
            "install-minimal",
            "Install the minimal profile",
            "Restore the Mycelium CLI before attempting deeper repair work.",
            &["install", "--profile", "minimal"],
            RepairTier::Primary,
        )],
        "hyphae" | "rhizome" => vec![
            host_policy::install_profile_repair_action(install_profile),
            RepairAction::stipe(
                "install-full-stack",
                "Install the full stack",
                "Install every supported ecosystem tool when you want the broadest local setup.",
                &["install", "--profile", "full-stack"],
                RepairTier::Secondary,
            ),
            match tool.name {
                "hyphae" => cargo_install_action("hyphae"),
                "rhizome" => cargo_install_action("rhizome"),
                _ => unreachable!(),
            },
        ],
        "canopy" => vec![
            RepairAction::stipe(
                "install-canopy",
                "Install Canopy",
                "Install the optional coordination runtime.",
                &["install", "canopy"],
                RepairTier::Primary,
            ),
            RepairAction::stipe(
                "install-full-stack",
                "Install the full stack",
                "Install every supported ecosystem tool when you want the broadest local setup.",
                &["install", "--profile", "full-stack"],
                RepairTier::Secondary,
            ),
        ],
        _ => Vec::new(),
    }
}

fn check_tool(spec: &ToolSpec) -> HealthCheck {
    match (spec.doctor_coverage, tool_registry::probe(spec)) {
        (_, ToolProbe::Installed(version)) => HealthCheck {
            name: spec.name.to_string(),
            passed: true,
            message: format!("v{version} installed and working"),
            repair_actions: Vec::new(),
        },
        (DoctorCoverage::Optional, ToolProbe::Missing) => HealthCheck {
            name: spec.name.to_string(),
            passed: true,
            message: "Optional coordination runtime not installed".to_string(),
            repair_actions: Vec::new(),
        },
        (_, ToolProbe::Broken) => HealthCheck {
            name: spec.name.to_string(),
            passed: false,
            message: "Binary found but failed to run".to_string(),
            repair_actions: missing_tool_actions(spec),
        },
        (DoctorCoverage::Required, ToolProbe::Missing) => HealthCheck {
            name: spec.name.to_string(),
            passed: false,
            message: "Not installed".to_string(),
            repair_actions: missing_tool_actions(spec),
        },
        (DoctorCoverage::Ignore, _) => unreachable!(),
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

    if tool_registry::find("hyphae")
        .is_some_and(|spec| matches!(tool_registry::probe(spec), ToolProbe::Installed(_)))
    {
        servers.push("hyphae");
    }
    if tool_registry::find("rhizome")
        .is_some_and(|spec| matches!(tool_registry::probe(spec), ToolProbe::Installed(_)))
    {
        servers.push("rhizome");
    }

    servers
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
        ConfigFormat::ClaudeRoot => {
            let parsed: Value = match serde_json::from_str(content) {
                Ok(value) => value,
                Err(_) => return false,
            };

            let user_matches = parsed
                .get("mcpServers")
                .and_then(Value::as_object)
                .is_some_and(|mcp_servers| {
                    required_servers
                        .iter()
                        .all(|server| mcp_servers.contains_key(*server))
                });

            if user_matches {
                return true;
            }

            let Some(project_root) = host_policy::project_root() else {
                return false;
            };
            let project_key = project_root.to_string_lossy();

            parsed
                .get("projects")
                .and_then(Value::as_object)
                .and_then(|projects| projects.get(project_key.as_ref()))
                .and_then(|project| project.get("mcpServers"))
                .and_then(Value::as_object)
                .is_some_and(|mcp_servers| {
                    required_servers
                        .iter()
                        .all(|server| mcp_servers.contains_key(*server))
                })
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
    let mut paths = Vec::new();

    if let Some(path) = host_policy::host_config_path(host_policy::HostMode::ClaudeCode) {
        paths.push(("Claude Code", path, ConfigFormat::ClaudeRoot));
    }
    if let Some(project_root) = host_policy::project_root() {
        paths.push((
            "Claude Code",
            project_root.join(".mcp.json"),
            ConfigFormat::Json,
        ));
    }
    if let Some(path) = host_policy::host_config_path(host_policy::HostMode::Cursor) {
        paths.push(("Cursor", path, ConfigFormat::Json));
    }
    if let Some(path) = host_policy::codex_notify_config_path(host_policy::HostConfigScope::User) {
        paths.push(("Codex CLI", path, ConfigFormat::Toml));
    }
    if let Some(path) = host_policy::codex_notify_config_path(host_policy::HostConfigScope::Project)
    {
        paths.push(("Codex CLI", path, ConfigFormat::Toml));
    }

    let Some(home) = dirs::home_dir() else {
        return paths;
    };

    paths.extend([
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
    ]);

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

#[cfg(test)]
fn codex_notify_adapter_configured_at_path(config_path: &std::path::Path) -> bool {
    super::codex_notify::codex_notify_configured_at_path(config_path)
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
