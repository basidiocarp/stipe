use std::fs;
use std::path::{Path, PathBuf};

use super::install::InstallProfile;
use super::repair::{RepairAction, RepairTier};

pub const CODEX_CLIENT_FLAG: &str = "codex";
pub const CLAUDE_CODE_HOST_MODE_LABEL: &str = "Claude Code operator mode";
pub const CODEX_HOST_MODE_LABEL: &str = "Codex host mode";
pub const CURSOR_HOST_MODE_LABEL: &str = "Cursor mode";
const CODEX_NOTIFY_VALUES: [&str; 2] = ["hyphae", "codex-notify"];

pub fn codex_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".codex").join("config.toml"))
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
        "Run `hyphae init` to add the Codex notify adapter to ~/.codex/config.toml and complete Codex host mode.".to_string()
    }
}

pub fn codex_notify_repair_action() -> RepairAction {
    RepairAction::manual(
        "Configure the Codex notify adapter".to_string(),
        "Run hyphae init so ~/.codex/config.toml includes notify = [\"hyphae\", \"codex-notify\"] and completes Codex host mode."
            .to_string(),
        "hyphae init".to_string(),
        vec!["init".to_string()],
        RepairTier::Primary,
    )
}
