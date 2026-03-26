//! Multi-client MCP detection and registration.
//!
//! Detects installed MCP clients (Cursor, Windsurf, Cline, Continue, Claude Desktop)
//! and registers hyphae/rhizome MCP servers in each client's config.

use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::{Map, Value, json};
use spore::editors::{self, Editor, McpServer as SporeMcpServer};
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
    CodexCli,
    GeminiCli,
    CopilotCli,
}

const SHARED_EDITOR_CLIENTS: &[(McpClient, Editor)] = &[
    (McpClient::ClaudeCode, Editor::ClaudeCode),
    (McpClient::Cursor, Editor::Cursor),
    (McpClient::Windsurf, Editor::Windsurf),
    (McpClient::ClaudeDesktop, Editor::ClaudeDesktop),
    (McpClient::CodexCli, Editor::CodexCli),
    (McpClient::GeminiCli, Editor::GeminiCli),
    (McpClient::CopilotCli, Editor::CopilotCli),
];

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
            Self::CodexCli => "Codex CLI",
            Self::GeminiCli => "Gemini CLI",
            Self::CopilotCli => "Copilot CLI",
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
            Self::CodexCli => "codex",
            Self::GeminiCli => "gemini",
            Self::CopilotCli => "copilot",
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
            "codex" | "codex-cli" => Some(Self::CodexCli),
            "gemini" | "gemini-cli" => Some(Self::GeminiCli),
            "copilot" | "copilot-cli" => Some(Self::CopilotCli),
            _ => None,
        }
    }

    /// Config file path for this client (if applicable).
    pub(crate) fn config_path(self) -> Option<PathBuf> {
        if let Some(editor) = self.shared_editor() {
            return editors::config_path(editor).ok();
        }

        let home = dirs::home_dir()?;
        match self {
            Self::Cline => vscode_cline_settings_path(),
            Self::Continue => Some(home.join(".continue").join("config.json")),
            Self::ClaudeCode
            | Self::Cursor
            | Self::Windsurf
            | Self::ClaudeDesktop
            | Self::CodexCli
            | Self::GeminiCli
            | Self::CopilotCli => None,
        }
    }

    fn shared_editor(self) -> Option<Editor> {
        SHARED_EDITOR_CLIENTS
            .iter()
            .find_map(|(client, editor)| (*client == self).then_some(*editor))
    }

    pub fn handled_separately_in_ecosystem(self) -> bool {
        matches!(self, Self::ClaudeCode | Self::CodexCli)
    }
}

impl fmt::Display for McpClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// All known clients in detection order.
const ALL_CLIENTS: [McpClient; 9] = [
    McpClient::ClaudeCode,
    McpClient::Cursor,
    McpClient::Windsurf,
    McpClient::Cline,
    McpClient::Continue,
    McpClient::ClaudeDesktop,
    McpClient::CodexCli,
    McpClient::GeminiCli,
    McpClient::CopilotCli,
];

/// Detect which MCP clients are installed on this system.
pub fn detect_clients() -> Vec<McpClient> {
    let detected_editors = editors::detect();
    collect_detected_clients(
        &detected_editors,
        claude_cli_installed(),
        cline_installed(),
        continue_installed(),
    )
}

fn collect_detected_clients(
    detected_editors: &[Editor],
    claude_cli_available: bool,
    cline_detected: bool,
    continue_detected: bool,
) -> Vec<McpClient> {
    ALL_CLIENTS
        .iter()
        .copied()
        .filter(|client| {
            shared_client_detected(*client, detected_editors)
                || (*client == McpClient::ClaudeCode && claude_cli_available)
                || (*client == McpClient::Cline && cline_detected)
                || (*client == McpClient::Continue && continue_detected)
        })
        .collect()
}

fn shared_client_detected(client: McpClient, detected_editors: &[Editor]) -> bool {
    client
        .shared_editor()
        .is_some_and(|editor| detected_editors.contains(&editor))
}

fn claude_cli_installed() -> bool {
    Command::new("claude")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn cline_installed() -> bool {
    vscode_cline_extension_exists() || McpClient::Cline.config_path().is_some_and(|p| p.exists())
}

fn continue_installed() -> bool {
    McpClient::Continue
        .config_path()
        .is_some_and(|p| p.exists() || p.parent().is_some_and(std::path::Path::exists))
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
    if let Some(editor) = client.shared_editor() {
        return register_spore_editor(editor, servers, verbose);
    }

    match client {
        McpClient::ClaudeCode => register_claude_code(servers, verbose),
        McpClient::Continue => register_continue(servers, verbose),
        McpClient::Cline => {
            print_cline_snippet(servers);
            Ok(true)
        }
        McpClient::Cursor
        | McpClient::Windsurf
        | McpClient::ClaudeDesktop
        | McpClient::CodexCli
        | McpClient::GeminiCli
        | McpClient::CopilotCli => unreachable!("shared editors handled above"),
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
// Shared spore-backed clients
// ─────────────────────────────────────────────────────────────────────────────

fn register_spore_editor(editor: Editor, servers: &[ServerConfig], verbose: u8) -> Result<bool> {
    let config_path =
        editors::config_path(editor).map_err(|err| anyhow::anyhow!(err.to_string()))?;

    let arg_slices: Vec<Vec<&str>> = servers
        .iter()
        .map(|server| server.args.iter().map(String::as_str).collect())
        .collect();
    let spore_servers: Vec<SporeMcpServer<'_>> = servers
        .iter()
        .zip(arg_slices.iter())
        .map(|(server, args)| SporeMcpServer {
            name: &server.name,
            command: &server.command,
            args: args.as_slice(),
        })
        .collect();

    editors::register_mcp_servers(editor, &spore_servers)
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;

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
    fn test_shared_editor_mapping_covers_supported_shared_hosts() {
        assert_eq!(McpClient::Cursor.shared_editor(), Some(Editor::Cursor));
        assert_eq!(McpClient::Windsurf.shared_editor(), Some(Editor::Windsurf));
        assert_eq!(McpClient::CodexCli.shared_editor(), Some(Editor::CodexCli));
        assert_eq!(McpClient::Continue.shared_editor(), None);
        assert_eq!(McpClient::Cline.shared_editor(), None);
    }

    #[test]
    fn test_ecosystem_special_case_clients_stay_explicit() {
        assert!(McpClient::ClaudeCode.handled_separately_in_ecosystem());
        assert!(McpClient::CodexCli.handled_separately_in_ecosystem());
        assert!(!McpClient::Cursor.handled_separately_in_ecosystem());
    }

    #[test]
    fn test_collect_detected_clients_preserves_inventory_order() {
        let detected =
            collect_detected_clients(&[Editor::CodexCli, Editor::Cursor], true, true, false);

        assert_eq!(
            detected,
            vec![
                McpClient::ClaudeCode,
                McpClient::Cursor,
                McpClient::Cline,
                McpClient::CodexCli,
            ]
        );
    }

    #[test]
    fn test_collect_detected_clients_keeps_claude_hybrid_detection() {
        let detected = collect_detected_clients(&[], true, false, false);

        assert_eq!(detected, vec![McpClient::ClaudeCode]);
    }

    #[test]
    fn test_collect_detected_clients_does_not_map_vscode_to_cline() {
        let detected = collect_detected_clients(&[Editor::VsCode], false, false, false);

        assert!(detected.is_empty());
    }

    #[test]
    fn test_collect_detected_clients_keeps_continue_outside_shared_overlap() {
        let detected = collect_detected_clients(&[Editor::Cursor], false, false, true);

        assert_eq!(detected, vec![McpClient::Cursor, McpClient::Continue]);
    }

    #[test]
    fn test_shared_host_config_paths_resolve_via_spore() {
        assert_eq!(
            McpClient::Cursor.config_path(),
            editors::config_path(Editor::Cursor).ok()
        );
        assert_eq!(
            McpClient::ClaudeDesktop.config_path(),
            editors::config_path(Editor::ClaudeDesktop).ok()
        );
        assert_eq!(
            McpClient::CodexCli.config_path(),
            editors::config_path(Editor::CodexCli).ok()
        );
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
