//! Multi-client MCP detection and registration.
//!
//! Detects installed MCP clients (Cursor, Windsurf, Cline, Continue, Claude Desktop)
//! and registers hyphae/rhizome MCP servers in each client's config.
//!
//! Boundary note: `spore` owns editor primitives such as detection, config paths,
//! MCP config writing, and editor-specific capabilities. `stipe` owns ecosystem
//! policy such as managed tool inventory, install profiles, repair semantics,
//! and orchestration across those editors.

use anyhow::Result;
use spore::editors::Editor;
use std::fmt;
use std::path::PathBuf;

use crate::commands::host_policy::HostConfigScope;

mod detection;
mod registration;
#[cfg(test)]
mod tests;

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

pub(super) const SHARED_EDITOR_CLIENTS: &[(McpClient, Editor)] = &[
    (McpClient::ClaudeCode, Editor::ClaudeCode),
    (McpClient::Cursor, Editor::Cursor),
    (McpClient::Windsurf, Editor::Windsurf),
    (McpClient::ClaudeDesktop, Editor::ClaudeDesktop),
    (McpClient::CodexCli, Editor::CodexCli),
    (McpClient::GeminiCli, Editor::GeminiCli),
    (McpClient::CopilotCli, Editor::CopilotCli),
];

/// All known clients in detection order.
pub(super) const ALL_CLIENTS: [McpClient; 9] = [
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
            return editor
                .descriptor()
                .ok()
                .map(|descriptor| descriptor.config_path);
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

    pub(super) fn shared_editor(self) -> Option<Editor> {
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

/// MCP server definition for registration.
pub struct ServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

pub fn detect_clients() -> Vec<McpClient> {
    detection::detect_clients()
}

pub fn register_servers(
    client: McpClient,
    servers: &[ServerConfig],
    scope: HostConfigScope,
    verbose: u8,
) -> Result<bool> {
    registration::register_servers(client, servers, scope, verbose)
}

pub fn print_generic_config(servers: &[ServerConfig]) {
    registration::print_generic_config(servers);
}

/// Get the VS Code settings path where Cline stores MCP config.
pub(crate) fn vscode_cline_settings_path() -> Option<PathBuf> {
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
        dirs::config_dir().map(|dir| dir.join("Code").join("User").join("settings.json"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}
