use crate::commands::claude_hooks;
use crate::commands::codex_notify;
use crate::commands::host_policy;
use crate::commands::host_policy::HostMode;
use crate::ecosystem::clients::{self, McpClient};

use super::model::HostInventoryEntry;

pub fn build_inventory() -> Vec<HostInventoryEntry> {
    let detected_clients = clients::detect_clients();

    host_policy::supported_host_modes()
        .iter()
        .copied()
        .map(|mode| inventory_entry(mode, &detected_clients))
        .collect()
}

pub fn inventory_entry(mode: HostMode, detected_clients: &[McpClient]) -> HostInventoryEntry {
    let descriptor = mode.descriptor();
    let config_paths = host_adapter_paths(mode);
    let config_exists = config_paths.iter().any(|path| path.exists());
    let detected = host_policy::host_detected_with_clients(mode, detected_clients) || config_exists;
    let configured = host_configured(mode, config_exists);

    HostInventoryEntry {
        mode,
        label: descriptor.display_name.to_string(),
        adapter_kind: descriptor.adapter_kind,
        adapter_label: descriptor.adapter_kind.label().to_string(),
        detected,
        configured,
        config_path: (!config_paths.is_empty())
            .then(|| host_policy::format_config_path_list(&config_paths)),
        detail: host_detail(mode, detected, configured, config_exists),
    }
}

fn host_adapter_paths(mode: HostMode) -> Vec<std::path::PathBuf> {
    match mode {
        HostMode::ClaudeCode => host_policy::claude_hook_settings_paths(),
        HostMode::Codex => host_policy::codex_notify_config_paths(),
        HostMode::Cursor => host_policy::host_config_path(mode).into_iter().collect(),
    }
}

fn host_configured(mode: HostMode, config_exists: bool) -> bool {
    match mode {
        HostMode::Codex => config_exists && codex_notify::codex_notify_configured(),
        HostMode::ClaudeCode => config_exists && claude_hooks::claude_hooks_configured(),
        HostMode::Cursor => config_exists,
    }
}

fn host_detail(mode: HostMode, detected: bool, configured: bool, config_exists: bool) -> String {
    match mode {
        HostMode::Codex => {
            if detected {
                codex_notify::codex_notify_detail(configured)
            } else {
                "Codex is not detected on this machine yet.".to_string()
            }
        }
        HostMode::ClaudeCode => {
            if configured {
                claude_hooks::claude_hooks_detail(true)
            } else if detected && config_exists {
                claude_hooks::claude_hooks_detail(false)
            } else if detected {
                format!(
                    "Claude Code is detected, but no {} was found yet.",
                    host_policy::host_config_display_path(mode)
                )
            } else {
                "Claude Code is not detected on this machine yet.".to_string()
            }
        }
        HostMode::Cursor => {
            if configured {
                "Cursor MCP config is present and ready for per-host setup.".to_string()
            } else if detected || config_exists {
                format!(
                    "Cursor is detected, but no {} was found yet.",
                    host_policy::host_config_display_path(mode)
                )
            } else {
                "Cursor is not detected on this machine yet.".to_string()
            }
        }
    }
}
