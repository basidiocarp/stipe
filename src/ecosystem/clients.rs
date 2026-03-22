//! Multi-client MCP detection and registration.
//!
//! Detects installed MCP clients (Cursor, Windsurf, Cline, Continue, Claude Desktop)
//! and registers hyphae/rhizome MCP servers in each client's config.

use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::{Map, Value, json};
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Known MCP clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpClient {
    ClaudeCode,
    Cursor,
    Windsurf,
    Cline,
    Continue,
    ClaudeDesktop,
}

impl McpClient {
    /// Human-readable display name.
    pub fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Cursor => "Cursor",
            Self::Windsurf => "Windsurf",
            Self::Cline => "Cline",
            Self::Continue => "Continue",
            Self::ClaudeDesktop => "Claude Desktop",
        }
    }

    /// CLI flag value (lowercase, kebab-case). Inverse of [`from_flag`].
    #[allow(dead_code)]
    pub fn flag(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Cursor => "cursor",
            Self::Windsurf => "windsurf",
            Self::Cline => "cline",
            Self::Continue => "continue",
            Self::ClaudeDesktop => "claude-desktop",
        }
    }

    /// Parse from CLI `--client` value.
    pub fn from_flag(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "claude-code" | "claude" => Some(Self::ClaudeCode),
            "cursor" => Some(Self::Cursor),
            "windsurf" => Some(Self::Windsurf),
            "cline" => Some(Self::Cline),
            "continue" => Some(Self::Continue),
            "claude-desktop" => Some(Self::ClaudeDesktop),
            _ => None,
        }
    }

    /// Config file path for this client (if applicable).
    fn config_path(self) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        match self {
            Self::ClaudeCode => Some(home.join(".claude.json")),
            Self::Cursor => Some(home.join(".cursor").join("mcp.json")),
            Self::Windsurf => Some(home.join(".windsurf").join("mcp.json")),
            Self::Cline => vscode_cline_settings_path(),
            Self::Continue => Some(home.join(".continue").join("config.json")),
            Self::ClaudeDesktop => {
                #[cfg(target_os = "macos")]
                {
                    Some(
                        home.join("Library")
                            .join("Application Support")
                            .join("Claude")
                            .join("claude_desktop_config.json"),
                    )
                }
                #[cfg(not(target_os = "macos"))]
                {
                    // Linux: ~/.config/Claude/claude_desktop_config.json
                    dirs::config_dir().map(|d| d.join("Claude").join("claude_desktop_config.json"))
                }
            }
        }
    }
}

impl fmt::Display for McpClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// All known clients in detection order.
const ALL_CLIENTS: [McpClient; 6] = [
    McpClient::ClaudeCode,
    McpClient::Cursor,
    McpClient::Windsurf,
    McpClient::Cline,
    McpClient::Continue,
    McpClient::ClaudeDesktop,
];

/// Detect which MCP clients are installed on this system.
pub fn detect_clients() -> Vec<McpClient> {
    ALL_CLIENTS
        .iter()
        .copied()
        .filter(|c| is_installed(*c))
        .collect()
}

/// Check if a client appears to be installed.
fn is_installed(client: McpClient) -> bool {
    match client {
        McpClient::ClaudeCode => {
            // Check CLI first, fall back to config file
            Command::new("claude")
                .arg("--version")
                .output()
                .is_ok_and(|o| o.status.success())
                || client.config_path().is_some_and(|p| p.exists())
        }
        McpClient::Cline => {
            // Check if VS Code extensions dir has cline
            vscode_cline_extension_exists() || client.config_path().is_some_and(|p| p.exists())
        }
        _ => client.config_path().is_some_and(|p| {
            // For Cursor/Windsurf: check parent dir exists (app installed)
            // For Continue/ClaudeDesktop: check config file or parent dir
            p.exists() || p.parent().is_some_and(|d| d.exists())
        }),
    }
}

/// MCP server definition for registration.
pub struct ServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

/// Register MCP servers in a client's config.
///
/// Returns `Ok(true)` if successfully registered, `Ok(false)` if skipped.
pub fn register_servers(client: McpClient, servers: &[ServerConfig], verbose: u8) -> Result<bool> {
    match client {
        McpClient::ClaudeCode => register_claude_code(servers, verbose),
        McpClient::Cursor | McpClient::Windsurf => {
            register_json_mcp_config(client, servers, verbose)
        }
        McpClient::Continue => register_continue(servers, verbose),
        McpClient::ClaudeDesktop => register_json_mcp_config(client, servers, verbose),
        McpClient::Cline => {
            print_cline_snippet(servers);
            Ok(true)
        }
    }
}

/// Print a generic JSON config snippet for any MCP client.
pub fn print_generic_config(servers: &[ServerConfig]) {
    println!("{}", "Generic MCP Configuration".bold());
    println!("{}", "─".repeat(60));
    println!();
    println!("Add the following to your MCP client's config:\n");

    let mut mcp_servers = Map::new();
    for server in servers {
        mcp_servers.insert(
            server.name.clone(),
            json!({
                "command": server.command,
                "args": server.args,
            }),
        );
    }

    let config = json!({ "mcpServers": mcp_servers });
    println!(
        "{}",
        serde_json::to_string_pretty(&config).unwrap_or_default()
    );
    println!();
    println!(
        "  {}",
        "Paste this into your MCP client's settings file.".dimmed()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Claude Code
// ─────────────────────────────────────────────────────────────────────────────

fn register_claude_code(servers: &[ServerConfig], verbose: u8) -> Result<bool> {
    let mut all_ok = true;
    for server in servers {
        let mut cmd = Command::new("claude");
        cmd.arg("mcp")
            .arg("add")
            .arg("--scope")
            .arg("user")
            .arg(&server.name)
            .arg("--");
        cmd.arg(&server.command);
        for arg in &server.args {
            cmd.arg(arg);
        }

        if verbose > 0 {
            eprintln!(
                "  Running: claude mcp add --scope user {} -- {} {}",
                server.name,
                server.command,
                server.args.join(" ")
            );
        }

        let output = cmd.output().context("failed to run `claude mcp add`")?;
        if !output.status.success() {
            all_ok = false;
        }
    }
    Ok(all_ok)
}

// ─────────────────────────────────────────────────────────────────────────────
// JSON-based clients (Cursor, Windsurf, Claude Desktop)
// ─────────────────────────────────────────────────────────────────────────────

fn register_json_mcp_config(
    client: McpClient,
    servers: &[ServerConfig],
    verbose: u8,
) -> Result<bool> {
    let config_path = client.config_path().context("no config path for client")?;

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Read existing config or create empty object
    let mut root: Value = if config_path.exists() {
        // Backup before modifying
        let backup = config_path.with_extension("json.bak");
        fs::copy(&config_path, &backup)
            .with_context(|| format!("failed to backup {}", config_path.display()))?;
        if verbose > 0 {
            eprintln!(
                "  Backed up {} → {}",
                config_path.display(),
                backup.display()
            );
        }

        let content = fs::read_to_string(&config_path)?;
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    // Ensure mcpServers key exists
    let mcp_servers = root
        .as_object_mut()
        .context("config root is not an object")?
        .entry("mcpServers")
        .or_insert_with(|| json!({}));

    let mcp_map = mcp_servers
        .as_object_mut()
        .context("mcpServers is not an object")?;

    // Merge servers
    for server in servers {
        mcp_map.insert(
            server.name.clone(),
            json!({
                "command": server.command,
                "args": server.args,
            }),
        );
    }

    // Write back
    let json_str = serde_json::to_string_pretty(&root)?;
    fs::write(&config_path, json_str)?;

    if verbose > 0 {
        eprintln!(
            "  Wrote {} server(s) to {}",
            servers.len(),
            config_path.display()
        );
    }

    Ok(true)
}

// ─────────────────────────────────────────────────────────────────────────────
// Continue
// ─────────────────────────────────────────────────────────────────────────────

fn register_continue(servers: &[ServerConfig], verbose: u8) -> Result<bool> {
    let config_path = McpClient::Continue
        .config_path()
        .context("no Continue config path")?;

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut root: Value = if config_path.exists() {
        let backup = config_path.with_extension("json.bak");
        fs::copy(&config_path, &backup)?;
        if verbose > 0 {
            eprintln!(
                "  Backed up {} → {}",
                config_path.display(),
                backup.display()
            );
        }
        let content = fs::read_to_string(&config_path)?;
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    // Continue uses "experimental.modelContextProtocolServers" array
    let obj = root
        .as_object_mut()
        .context("config root is not an object")?;

    // Ensure experimental key
    let experimental = obj
        .entry("experimental")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("experimental is not an object")?;

    let mcp_array = experimental
        .entry("modelContextProtocolServers")
        .or_insert_with(|| json!([]));

    let arr = mcp_array
        .as_array_mut()
        .context("modelContextProtocolServers is not an array")?;

    for server in servers {
        // Remove existing entry with same name
        arr.retain(|entry| entry.get("name").and_then(Value::as_str) != Some(&server.name));
        arr.push(json!({
            "name": server.name,
            "command": server.command,
            "args": server.args,
        }));
    }

    let json_str = serde_json::to_string_pretty(&root)?;
    fs::write(&config_path, json_str)?;

    if verbose > 0 {
        eprintln!(
            "  Wrote {} server(s) to {}",
            servers.len(),
            config_path.display()
        );
    }

    Ok(true)
}

// ─────────────────────────────────────────────────────────────────────────────
// Cline (print snippet)
// ─────────────────────────────────────────────────────────────────────────────

fn print_cline_snippet(servers: &[ServerConfig]) {
    println!();
    println!(
        "  {} Cline uses VS Code settings. Add this to your VS Code settings.json:",
        "→".dimmed()
    );
    println!();

    let mut mcp_servers = Map::new();
    for server in servers {
        mcp_servers.insert(
            server.name.clone(),
            json!({
                "command": server.command,
                "args": server.args,
            }),
        );
    }

    let snippet = json!({ "cline.mcpServers": mcp_servers });
    println!(
        "{}",
        serde_json::to_string_pretty(&snippet).unwrap_or_default()
    );
    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Get the VS Code settings path where Cline stores MCP config.
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

/// Check if Cline extension directory exists in VS Code.
fn vscode_cline_extension_exists() -> bool {
    dirs::home_dir()
        .map(|h| h.join(".vscode").join("extensions"))
        .is_some_and(|ext_dir| {
            ext_dir.exists()
                && fs::read_dir(ext_dir).ok().is_some_and(|entries| {
                    entries.filter_map(Result::ok).any(|e| {
                        e.file_name()
                            .to_string_lossy()
                            .starts_with("saoudrizwan.claude-dev")
                    })
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_flag_roundtrip() {
        for client in ALL_CLIENTS {
            let flag = client.flag();
            let parsed = McpClient::from_flag(flag);
            assert_eq!(parsed, Some(client), "roundtrip failed for {flag}");
        }
    }

    #[test]
    fn test_client_name_not_empty() {
        for client in ALL_CLIENTS {
            assert!(!client.name().is_empty());
        }
    }

    #[test]
    fn test_from_flag_aliases() {
        assert_eq!(McpClient::from_flag("claude"), Some(McpClient::ClaudeCode));
        assert_eq!(McpClient::from_flag("CURSOR"), Some(McpClient::Cursor));
        assert_eq!(McpClient::from_flag("unknown"), None);
    }

    #[test]
    fn test_detect_clients_does_not_panic() {
        let _clients = detect_clients();
    }

    #[test]
    fn test_print_generic_config() {
        let servers = vec![ServerConfig {
            name: "hyphae".to_string(),
            command: "hyphae".to_string(),
            args: vec!["serve".to_string()],
        }];
        // Just verify no panic
        print_generic_config(&servers);
    }
}
