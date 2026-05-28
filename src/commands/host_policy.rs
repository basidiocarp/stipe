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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum HostConfigScope {
    #[default]
    User,
    Project,
    Local,
}

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
    &[HostMode::ClaudeCode, HostMode::Codex]
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

pub fn project_root() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(root) = test_project_root_override() {
        return Some(root);
    }

    let cwd = std::env::current_dir().ok()?;
    Some(spore::paths::find_project_root(&cwd).unwrap_or(cwd))
}

#[cfg(test)]
thread_local! {
    static TEST_PROJECT_ROOT_OVERRIDE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn test_project_root_override() -> Option<PathBuf> {
    TEST_PROJECT_ROOT_OVERRIDE.with(|path| path.borrow().clone())
}

#[cfg(test)]
pub(crate) fn with_project_root_override<T>(root: PathBuf, f: impl FnOnce() -> T) -> T {
    TEST_PROJECT_ROOT_OVERRIDE.with(|path| {
        let previous = path.replace(Some(root));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        path.replace(previous);
        match result {
            Ok(value) => value,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    })
}

pub fn supported_host_config_scopes(mode: HostMode) -> &'static [HostConfigScope] {
    const CLAUDE_SCOPES: &[HostConfigScope] = &[
        HostConfigScope::User,
        HostConfigScope::Project,
        HostConfigScope::Local,
    ];
    const CODEX_SCOPES: &[HostConfigScope] = &[HostConfigScope::User, HostConfigScope::Project];
    const CURSOR_SCOPES: &[HostConfigScope] = &[HostConfigScope::User];

    match mode {
        HostMode::ClaudeCode => CLAUDE_SCOPES,
        HostMode::Codex => CODEX_SCOPES,
        HostMode::Cursor => CURSOR_SCOPES,
    }
}

pub fn host_scope_supported(mode: HostMode, scope: HostConfigScope) -> bool {
    supported_host_config_scopes(mode).contains(&scope)
}

pub fn scope_name(scope: HostConfigScope) -> &'static str {
    match scope {
        HostConfigScope::User => "user",
        HostConfigScope::Project => "project",
        HostConfigScope::Local => "local",
    }
}

pub fn supported_scope_hint(mode: HostMode) -> String {
    supported_host_config_scopes(mode)
        .iter()
        .map(|scope| scope_name(*scope))
        .collect::<Vec<_>>()
        .join("|")
}

pub fn claude_hook_settings_path(scope: HostConfigScope) -> Option<PathBuf> {
    match scope {
        HostConfigScope::User => dirs::home_dir().map(|home| home.join(".claude/settings.json")),
        HostConfigScope::Project => project_root().map(|root| root.join(".claude/settings.json")),
        HostConfigScope::Local => {
            project_root().map(|root| root.join(".claude/settings.local.json"))
        }
    }
}

pub fn claude_hook_settings_paths() -> Vec<PathBuf> {
    supported_host_config_scopes(HostMode::ClaudeCode)
        .iter()
        .copied()
        .filter_map(claude_hook_settings_path)
        .collect()
}

pub fn codex_notify_config_path(scope: HostConfigScope) -> Option<PathBuf> {
    match scope {
        HostConfigScope::User => dirs::home_dir().map(|home| home.join(".codex/config.toml")),
        HostConfigScope::Project => project_root().map(|root| root.join(".codex/config.toml")),
        HostConfigScope::Local => None,
    }
}

pub fn codex_notify_config_paths() -> Vec<PathBuf> {
    supported_host_config_scopes(HostMode::Codex)
        .iter()
        .copied()
        .filter_map(codex_notify_config_path)
        .collect()
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
    host_config_path(mode).map_or_else(
        || host_config_label(mode).to_string(),
        |path| format_user_path(&path),
    )
}

pub fn format_config_path_list(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| format_user_path(path))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn host_setup_repair_action(mode: HostMode) -> RepairAction {
    // mode.client_flag() returns kebab-case (e.g. "claude-code"); action_keys are snake_case.
    let action_key = format!("host_setup_{}", mode.client_flag().replace('-', "_"));
    RepairAction::manual(
        action_key,
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
        InstallProfile::Minimal => RepairAction::stipe(
            "install-minimal",
            "Install the minimal profile",
            "Install only the baseline local agent stack.",
            &["install", "--profile", "minimal"],
            RepairTier::Primary,
        ),
        InstallProfile::Standard => RepairAction::stipe(
            "install-standard",
            "Install the standard profile",
            "Install the default local agent stack and packaging helpers.",
            &["install", "--profile", "standard"],
            RepairTier::Primary,
        ),
        InstallProfile::FullStack => RepairAction::stipe(
            "install-full-stack",
            "Install the full stack",
            "Install every supported ecosystem tool when you want the broadest local setup.",
            &["install", "--profile", "full"],
            RepairTier::Primary,
        ),
        InstallProfile::DeveloperTools => RepairAction::manual(
            "install_developer_tools".to_string(),
            "Review developer tool recommendations".to_string(),
            "Inspect the advisory developer tool profile and install missing tools with your package manager."
                .to_string(),
            "stipe install --profile developer-tools".to_string(),
            vec![
                "install".to_string(),
                "--profile".to_string(),
                "developer-tools".to_string(),
            ],
            RepairTier::Secondary,
        ),
    }
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

        assert_eq!(descriptors.len(), 2);
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
    fn test_claude_hook_settings_paths_follow_scope() {
        let root = project_root().unwrap();
        let user_path = claude_hook_settings_path(HostConfigScope::User).unwrap();
        assert!(user_path.ends_with(".claude/settings.json"));

        let project_path = claude_hook_settings_path(HostConfigScope::Project).unwrap();
        assert!(project_path.ends_with(".claude/settings.json"));
        assert!(project_path.starts_with(&root));

        let local_path = claude_hook_settings_path(HostConfigScope::Local).unwrap();
        assert!(local_path.ends_with(".claude/settings.local.json"));
        assert!(local_path.starts_with(&root));
    }

    #[test]
    fn test_codex_notify_paths_follow_scope() {
        let root = project_root().unwrap();
        let user_path = codex_notify_config_path(HostConfigScope::User).unwrap();
        assert!(user_path.ends_with(".codex/config.toml"));

        let project_path = codex_notify_config_path(HostConfigScope::Project).unwrap();
        assert!(project_path.ends_with(".codex/config.toml"));
        assert!(project_path.starts_with(&root));
    }

    #[test]
    fn test_local_scope_is_not_supported_for_codex_notify() {
        assert!(codex_notify_config_path(HostConfigScope::Local).is_none());
    }

    #[test]
    fn test_install_profile_repair_action_keeps_cursor_distinct() {
        let action = install_profile_repair_action(InstallProfile::Cursor);

        assert_eq!(action.command, "stipe install --profile cursor");
        assert!(action.label.contains("Cursor"));
    }

    #[test]
    fn test_supported_scope_hint_is_stable_for_host_modes() {
        assert_eq!(
            supported_scope_hint(HostMode::ClaudeCode),
            "user|project|local"
        );
        assert_eq!(supported_scope_hint(HostMode::Codex), "user|project");
        assert_eq!(supported_scope_hint(HostMode::Cursor), "user");
    }
}
