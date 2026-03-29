use crate::commands::install::InstallProfile;

use super::*;

#[test]
fn test_host_mode_mappings_are_explicit() {
    assert_eq!(HostMode::Codex.client_flag(), "codex");
    assert_eq!(HostMode::Codex.install_profile(), InstallProfile::Codex);
    assert_eq!(HostMode::ClaudeCode.client_flag(), "claude-code");
    assert_eq!(
        HostMode::ClaudeCode.install_profile(),
        InstallProfile::ClaudeCode
    );
    assert_eq!(HostMode::Cursor.client_flag(), "cursor");
    assert_eq!(HostMode::Cursor.install_profile(), InstallProfile::Cursor);
}

#[test]
fn test_codex_doctor_report_includes_notify_repair() {
    let entry = HostInventoryEntry {
        mode: HostMode::Codex,
        label: HostMode::Codex.label().to_string(),
        adapter_kind: HostAdapterKind::McpAndNotify,
        adapter_label: HostAdapterKind::McpAndNotify.label().to_string(),
        detected: true,
        configured: false,
        config_path: Some("/Users/test/.codex/config.toml".to_string()),
        detail: "Run `stipe init --client codex` to install the Codex notify adapter.".to_string(),
    };

    let checks = doctor_checks_for_entry(&entry);
    let repair_actions = crate::commands::repair::dedupe_repair_actions(
        checks
            .iter()
            .flat_map(|check| check.repair_actions.clone())
            .collect(),
    );

    assert!(repair_actions.iter().any(|action| {
        action.command.contains("stipe init --client codex")
            || action.command.contains("stipe host setup codex")
    }));
}

#[test]
fn test_doctor_checks_reflect_inventory_entry() {
    let entry = HostInventoryEntry {
        mode: HostMode::Cursor,
        label: HostMode::Cursor.label().to_string(),
        adapter_kind: HostAdapterKind::Mcp,
        adapter_label: HostAdapterKind::Mcp.label().to_string(),
        detected: false,
        configured: false,
        config_path: Some("/Users/test/.cursor/mcp.json".to_string()),
        detail: "Cursor is not detected on this machine yet.".to_string(),
    };

    let checks = doctor_checks_for_entry(&entry);

    assert_eq!(checks.len(), 2);
    assert!(!checks[0].passed);
    assert!(
        checks[0]
            .repair_actions
            .iter()
            .any(|action| action.command == "stipe host setup cursor")
    );
    assert_eq!(checks[1].message, entry.detail);
}

#[test]
fn test_inventory_entry_uses_shared_host_descriptor_metadata() {
    let entry = inventory_entry(HostMode::Codex, &[]);

    assert_eq!(entry.label, HostMode::Codex.label());
    assert_eq!(entry.adapter_kind, HostAdapterKind::McpAndNotify);
    assert_eq!(entry.adapter_label, "MCP + notify");
}
