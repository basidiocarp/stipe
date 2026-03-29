use serde_json::Value;
use std::fs;
use std::path::PathBuf;

use super::host_policy;
use super::model::{ConfigFormat, HealthCheck};
use super::tool_checks::installed_mcp_servers;
use crate::commands::repair::{RepairAction, RepairTier};

pub(super) fn config_mentions_servers(
    content: &str,
    required_servers: &[&str],
    format: ConfigFormat,
) -> bool {
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

pub(super) fn check_mcp_config_drift() -> HealthCheck {
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

#[cfg(test)]
pub(super) fn codex_notify_adapter_configured_at_path(config_path: &std::path::Path) -> bool {
    crate::commands::codex_notify::codex_notify_configured_at_path(config_path)
}
