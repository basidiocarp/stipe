use std::fs;
use std::path::{MAIN_SEPARATOR, Path, PathBuf};

use clap::ValueEnum;
use serde::Serialize;

use super::install::InstallProfile;
use super::repair::{RepairAction, RepairTier};
use crate::ecosystem::clients::McpClient;

pub const CODEX_CLIENT_FLAG: &str = "codex";
pub const CLAUDE_CODE_HOST_MODE_LABEL: &str = "Claude Code operator mode";
pub const CODEX_HOST_MODE_LABEL: &str = "Codex host mode";
pub const CURSOR_HOST_MODE_LABEL: &str = "Cursor mode";
const CODEX_NOTIFY_VALUES: [&str; 2] = ["hyphae", "codex-notify"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostMode {
    ClaudeCode,
    Codex,
    Cursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostAdapterKind {
    HooksAndMcp,
    McpAndNotify,
    Mcp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostDescriptor {
    pub adapter_kind: HostAdapterKind,
    pub client_flag: &'static str,
    pub config_label: &'static str,
    pub display_name: &'static str,
    pub install_profile: InstallProfile,
    pub mode: HostMode,
}

impl HostMode {
    pub fn descriptor(self) -> HostDescriptor {
        match self {
            Self::ClaudeCode => HostDescriptor {
                adapter_kind: HostAdapterKind::HooksAndMcp,
                client_flag: "claude-code",
                config_label: host_config_label(self),
                display_name: CLAUDE_CODE_HOST_MODE_LABEL,
                install_profile: InstallProfile::ClaudeCode,
                mode: self,
            },
            Self::Codex => HostDescriptor {
                adapter_kind: HostAdapterKind::McpAndNotify,
                client_flag: CODEX_CLIENT_FLAG,
                config_label: host_config_label(self),
                display_name: CODEX_HOST_MODE_LABEL,
                install_profile: InstallProfile::Codex,
                mode: self,
            },
            Self::Cursor => HostDescriptor {
                adapter_kind: HostAdapterKind::Mcp,
                client_flag: "cursor",
                config_label: host_config_label(self),
                display_name: CURSOR_HOST_MODE_LABEL,
                install_profile: InstallProfile::Cursor,
                mode: self,
            },
        }
    }

    pub fn client_flag(self) -> &'static str {
        self.descriptor().client_flag
    }

    pub fn install_profile(self) -> InstallProfile {
        self.descriptor().install_profile
    }

    pub fn label(self) -> &'static str {
        self.descriptor().display_name
    }

    pub fn client(self) -> McpClient {
        match self {
            Self::ClaudeCode => McpClient::ClaudeCode,
            Self::Codex => McpClient::CodexCli,
            Self::Cursor => McpClient::Cursor,
        }
    }
}

impl HostAdapterKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::HooksAndMcp => "hooks + MCP",
            Self::McpAndNotify => "MCP + notify",
            Self::Mcp => "MCP",
        }
    }
}

pub fn supported_host_modes() -> &'static [HostMode] {
    &[HostMode::ClaudeCode, HostMode::Codex, HostMode::Cursor]
}

pub fn host_config_label(mode: HostMode) -> &'static str {
    match mode {
        HostMode::ClaudeCode => "Claude Code config",
        HostMode::Codex => "Codex config",
        HostMode::Cursor => "Cursor MCP config",
    }
}

pub fn host_config_path(mode: HostMode) -> Option<PathBuf> {
    mode.client().config_path()
}

pub fn format_user_path(path: &Path) -> String {
    let Some(home) = dirs::home_dir() else {
        return path.display().to_string();
    };

    let Ok(relative) = path.strip_prefix(&home) else {
        return path.display().to_string();
    };

    if relative.as_os_str().is_empty() {
        "~".to_string()
    } else {
        format!("~{}{}", MAIN_SEPARATOR, relative.display())
    }
}

pub fn host_mode_from_client_flag(client: &str) -> Option<HostMode> {
    match client {
        "claude-code" => Some(HostMode::ClaudeCode),
        CODEX_CLIENT_FLAG => Some(HostMode::Codex),
        "cursor" => Some(HostMode::Cursor),
        _ => None,
    }
}

pub fn host_detected_with_clients(mode: HostMode, detected_clients: &[McpClient]) -> bool {
    detected_clients.contains(&mode.client())
}

pub fn host_config_display_path(mode: HostMode) -> String {
    host_config_path(mode)
        .map(|path| format_user_path(&path))
        .unwrap_or_else(|| host_config_label(mode).to_string())
}

pub fn host_setup_repair_action(mode: HostMode) -> RepairAction {
    RepairAction::manual(
        format!("Set up {}", mode.label()),
        format!(
            "Install the matching profile and initialize {} with its expected host adapters.",
            mode.label()
        ),
        format!("stipe host setup {}", mode.client_flag()),
        vec![
            "host".to_string(),
            "setup".to_string(),
            mode.client_flag().to_string(),
        ],
        RepairTier::Primary,
    )
}

pub fn codex_config_path() -> Option<PathBuf> {
    HostMode::Codex.client().config_path()
}

pub fn codex_target_requested(client: Option<&str>) -> bool {
    client.is_some_and(|value| value.eq_ignore_ascii_case(CODEX_CLIENT_FLAG))
}

pub fn codex_host_mode_requested(client: Option<&str>, detected_clients: &[String]) -> bool {
    codex_target_requested(client)
        || (client.is_none()
            && detected_clients
                .iter()
                .any(|detected| detected == "Codex CLI"))
}

pub fn preferred_install_profile(
    client: Option<&str>,
    detected_clients: &[String],
) -> InstallProfile {
    if codex_host_mode_requested(client, detected_clients) {
        InstallProfile::Codex
    } else {
        InstallProfile::ClaudeCode
    }
}

pub fn install_profile_repair_action(profile: InstallProfile) -> RepairAction {
    match profile {
        InstallProfile::Codex => RepairAction::stipe(
            "install-codex",
            "Install the Codex host mode",
            "Install the core local agent stack and explicit Codex setup path before wiring MCP clients.",
            &["install", "--profile", "codex"],
            RepairTier::Primary,
        ),
        InstallProfile::Cursor => RepairAction::stipe(
            "install-cursor",
            "Install the Cursor host support",
            "Install the core local agent stack for Cursor before wiring MCP clients.",
            &["install", "--profile", "cursor"],
            RepairTier::Primary,
        ),
        InstallProfile::ClaudeCode => RepairAction::stipe(
            "install-claude-code",
            "Install the hooks-enabled profile",
            "Install the core local agent stack before wiring MCP clients.",
            &["install", "--profile", "claude-code"],
            RepairTier::Primary,
        ),
        InstallProfile::Minimal | InstallProfile::FullStack => RepairAction::stipe(
            "install-full-stack",
            "Install the full stack",
            "Install every supported ecosystem tool when you want the broadest local setup.",
            &["install", "--profile", "full-stack"],
            RepairTier::Primary,
        ),
    }
}

pub fn codex_notify_configured_at_path(config_path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(config_path) else {
        return false;
    };

    let Ok(parsed) = content.parse::<toml::Value>() else {
        return false;
    };

    parsed
        .get("notify")
        .and_then(toml::Value::as_array)
        .is_some_and(|values| {
            values.len() == CODEX_NOTIFY_VALUES.len()
                && values
                    .iter()
                    .map(toml::Value::as_str)
                    .eq(CODEX_NOTIFY_VALUES.iter().copied().map(Some))
        })
}

pub fn codex_notify_configured() -> bool {
    codex_config_path()
        .as_deref()
        .is_some_and(codex_notify_configured_at_path)
}

pub fn codex_notify_detail(configured: bool) -> String {
    if configured {
        "Codex host mode already points at Hyphae via its notify adapter.".to_string()
    } else {
        format!(
            "Run `hyphae init` to add the Codex notify adapter to {} and complete Codex host mode.",
            host_config_display_path(HostMode::Codex)
        )
    }
}

pub fn codex_notify_repair_action() -> RepairAction {
    RepairAction::manual(
        "Configure the Codex notify adapter".to_string(),
        format!(
            "Run hyphae init so {} includes notify = [\"hyphae\", \"codex-notify\"] and completes Codex host mode.",
            host_config_display_path(HostMode::Codex)
        ),
        "hyphae init".to_string(),
        vec!["init".to_string()],
        RepairTier::Primary,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supported_host_modes_have_explicit_descriptors() {
        let descriptors = supported_host_modes()
            .iter()
            .map(|mode| mode.descriptor())
            .collect::<Vec<_>>();

        assert_eq!(descriptors.len(), 3);
        assert!(
            descriptors
                .iter()
                .any(|descriptor| descriptor.client_flag == "claude-code")
        );
        assert!(
            descriptors
                .iter()
                .any(|descriptor| descriptor.client_flag == "codex")
        );
        assert!(
            descriptors
                .iter()
                .any(|descriptor| descriptor.client_flag == "cursor")
        );
        assert!(
            descriptors
                .iter()
                .any(|descriptor| descriptor.config_label == "Codex config")
        );
    }

    #[test]
    fn test_host_setup_repair_action_points_at_new_host_surface() {
        let action = host_setup_repair_action(HostMode::Codex);

        assert_eq!(action.command, "stipe host setup codex");
        assert!(action.description.contains("Codex"));
    }

    #[test]
    fn test_host_modes_resolve_config_paths_via_clients() {
        assert_eq!(
            host_config_path(HostMode::ClaudeCode),
            McpClient::ClaudeCode.config_path()
        );
        assert_eq!(
            host_config_path(HostMode::Codex),
            McpClient::CodexCli.config_path()
        );
        assert_eq!(
            host_config_path(HostMode::Cursor),
            McpClient::Cursor.config_path()
        );
    }

    #[test]
    fn test_install_profile_repair_action_keeps_cursor_distinct() {
        let action = install_profile_repair_action(InstallProfile::Cursor);

        assert_eq!(action.command, "stipe install --profile cursor");
        assert!(action.label.contains("Cursor"));
    }

    #[test]
    fn test_codex_notify_detail_mentions_resolved_config_path() {
        let detail = codex_notify_detail(false);

        assert!(detail.contains("hyphae init"));
        assert!(detail.contains("Codex"));
        assert!(detail.contains(".codex"));
    }
}
