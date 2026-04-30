use crate::commands::install::InstallProfile;
use crate::commands::repair::{RepairAction, RepairTier};

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
        detail: "Run `stipe host setup codex` to install the Codex notify adapter.".to_string(),
    };

    let checks = doctor_checks_for_entry(&entry);
    let repair_actions = crate::commands::repair::dedupe_repair_actions(
        checks
            .iter()
            .flat_map(|check| check.repair_actions.clone())
            .collect(),
    );

    assert!(
        repair_actions
            .iter()
            .all(|action| action.command == "stipe host setup codex")
    );
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

#[test]
fn test_render_list_snapshot_includes_known_sections() {
    let lines = render_list();

    assert_eq!(lines[0], "");
    assert_eq!(lines[1], "Basidiocarp Host Inventory");
    assert_eq!(lines[2], "─".repeat(75));
    assert!(lines.iter().any(|line| line.contains("claude-code")));
    assert!(lines.iter().any(|line| line.contains("codex")));
    // Cursor is only included if enabled (not gated by default)
    // so we check that either cursor is present or claude-code and codex are both present
}

#[test]
fn test_render_doctor_snapshot_for_failure() {
    let report = crate::commands::host::model::HostDoctorReport {
        healthy: false,
        summary: "1 host needs attention.".to_string(),
        checks: vec![crate::commands::host::model::HostDoctorCheck {
            host: HostMode::Cursor,
            passed: false,
            message: "Cursor is not configured".to_string(),
            repair_actions: vec![],
        }],
        repair_actions: vec![RepairAction::stipe(
            "host setup cursor",
            "Set up Cursor",
            "Restore the Cursor MCP config.",
            &["host", "setup", "cursor"],
            RepairTier::Primary,
        )],
    };

    assert_eq!(
        render_doctor(&report, false),
        vec![
            "",
            "Basidiocarp Host Health",
            &"─".repeat(75),
            "",
            "  cursor         ✗ Cursor is not configured",
            "",
            "State: 1 host needs attention.",
            "Next step: run `stipe host setup cursor`",
            "",
        ]
    );
}

#[test]
fn test_render_doctor_skips_duplicate_optional_follow_up_in_additional_actions() {
    let report = crate::commands::host::model::HostDoctorReport {
        healthy: false,
        summary: "2 hosts need attention.".to_string(),
        checks: vec![crate::commands::host::model::HostDoctorCheck {
            host: HostMode::Cursor,
            passed: false,
            message: "Cursor is not configured".to_string(),
            repair_actions: vec![],
        }],
        repair_actions: vec![
            RepairAction::stipe(
                "host setup cursor",
                "Set up Cursor",
                "Restore the Cursor MCP config.",
                &["host", "setup", "cursor"],
                RepairTier::Primary,
            ),
            RepairAction::stipe(
                "install cursor",
                "Install Cursor",
                "Install Cursor before setup.",
                &["install", "cursor"],
                RepairTier::Secondary,
            ),
            RepairAction::stipe(
                "host doctor",
                "Inspect host health",
                "Inspect per-host state.",
                &["host", "doctor"],
                RepairTier::Secondary,
            ),
        ],
    };

    let lines = render_doctor(&report, false);
    let install_cursor = "  - stipe install cursor".to_string();

    assert!(
        lines
            .iter()
            .any(|line| line == "Optional follow-up: run `stipe install cursor`")
    );
    assert_eq!(
        lines.iter().filter(|line| **line == install_cursor).count(),
        0
    );
    assert!(
        lines
            .iter()
            .any(|line| line == "Additional repair actions:")
    );
    assert!(lines.iter().any(|line| line == "  - stipe host doctor"));
}

#[test]
fn test_render_setup_preview_keeps_host_level_next_step() {
    assert_eq!(
        render_setup_preview(HostMode::Cursor),
        vec![
            "Host setup preview | Cursor mode".to_string(),
            "State: preview only; the host rollout is staged but not applied".to_string(),
            "Next step: review the embedded install and init previews below, then rerun `stipe host setup cursor` without `--dry-run` to apply the host flow".to_string(),
            "Optional follow-up: run `stipe install --profile cursor --dry-run` to inspect the install surface on its own".to_string(),
        ]
    );
}

#[test]
fn test_cursor_host_disabled_when_env_unset_and_binary_absent() {
    // Negative path: env var unset AND cursor binary not detected → gated off.
    let enabled = super::inventory::cursor_host_enabled_with(None, || false);
    assert!(!enabled, "Cursor must be gated off when neither env var nor binary detection signals it");
}

#[test]
fn test_cursor_host_enabled_via_env_var_one() {
    // Positive path: STIPE_CURSOR_HOST=1 → enabled regardless of binary detection.
    let enabled = super::inventory::cursor_host_enabled_with(Some("1".to_string()), || false);
    assert!(enabled, "STIPE_CURSOR_HOST=1 must enable Cursor host checks even without the binary");
}

#[test]
fn test_cursor_host_enabled_via_env_var_true_case_insensitive() {
    // Positive path: STIPE_CURSOR_HOST=true (any case) → enabled.
    for value in &["true", "TRUE", "True"] {
        let enabled = super::inventory::cursor_host_enabled_with(Some((*value).to_string()), || false);
        assert!(enabled, "STIPE_CURSOR_HOST={value} must enable Cursor host checks");
    }
}

#[test]
fn test_cursor_host_enabled_via_binary_detection() {
    // Positive path via PATH: env var unset, binary probe returns true → enabled.
    let enabled = super::inventory::cursor_host_enabled_with(None, || true);
    assert!(enabled, "Cursor binary on PATH must enable Cursor host checks");
}

#[test]
fn test_cursor_host_disabled_for_non_truthy_env_value() {
    // Env var set to a non-truthy string → falls through to binary detection.
    // Confirms we don't accidentally treat any non-empty value as truthy.
    for value in &["0", "false", "no", "", "yes"] {
        let enabled = super::inventory::cursor_host_enabled_with(Some((*value).to_string()), || false);
        assert!(!enabled, "STIPE_CURSOR_HOST={value:?} must not enable Cursor when binary is absent");
    }
}
