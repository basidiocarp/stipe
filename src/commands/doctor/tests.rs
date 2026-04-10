use super::*;
use crate::commands::codex_notify;
use crate::commands::developer_tools::DeveloperToolTier;
use crate::commands::repair::{RepairAction, RepairTier};
use crate::commands::runtime_policy::{
    DecisionSource, PolicyDecision, PolicyScope, RememberedDecision, RuntimePolicyReport,
};
use crate::commands::tool_registry::ToolProbe;
use std::fs;

use super::config_checks::{codex_notify_adapter_configured_at_path, config_mentions_servers};
use super::council_checks::check_task_linked_council;
use super::model::ConfigFormat;
use super::tool_checks::check_hyphae_db_at_path;
use super::{host_policy, tool_registry};

#[test]
fn test_check_hyphae_db_exists() {
    let temp_dir = std::env::temp_dir().join("stipe-test-hyphae-exists");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let db_path = temp_dir.join("hyphae.db");
    fs::write(&db_path, "").unwrap();

    let check = check_hyphae_db_at_path(&db_path);
    assert!(check.passed, "Should pass when database exists");
    assert_eq!(check.name, "hyphae database");
    assert!(check.message.contains("initialized"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_check_hyphae_db_missing() {
    let temp_dir = std::env::temp_dir().join("stipe-test-hyphae-missing");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let db_path = temp_dir.join("nonexistent.db");

    let check = check_hyphae_db_at_path(&db_path);
    assert!(!check.passed, "Should fail when database does not exist");
    assert_eq!(check.name, "hyphae database");
    assert!(check.message.contains("not found"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_config_mentions_servers_detects_required_names() {
    let content =
        r#"{"mcpServers":{"hyphae":{"command":"hyphae"},"rhizome":{"command":"rhizome"}}}"#;

    assert!(config_mentions_servers(
        content,
        &["hyphae", "rhizome"],
        ConfigFormat::Json
    ));
    assert!(!config_mentions_servers(
        content,
        &["hyphae", "cortina"],
        ConfigFormat::Json
    ));
}

#[test]
fn test_config_mentions_servers_detects_codex_toml() {
    let content = r#"
            [mcp_servers.hyphae]
            command = "hyphae"
            args = ["serve"]

            [mcp_servers.rhizome]
            command = "rhizome"
            args = ["serve", "--expanded"]
        "#;

    assert!(config_mentions_servers(
        content,
        &["hyphae", "rhizome"],
        ConfigFormat::Toml
    ));
    assert!(!config_mentions_servers(
        content,
        &["hyphae", "cortina"],
        ConfigFormat::Toml
    ));
}

#[test]
fn test_codex_notify_adapter_configured_at_path_detects_notify_entry() {
    let temp_dir = std::env::temp_dir().join("stipe-test-codex-notify");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let config_path = temp_dir.join("config.toml");
    fs::write(&config_path, r#"notify = ["hyphae", "codex-notify"]"#).unwrap();

    assert!(codex_notify_adapter_configured_at_path(&config_path));

    fs::write(&config_path, r#"notify = ["hyphae", "something-else"]"#).unwrap();
    assert!(!codex_notify_adapter_configured_at_path(&config_path));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_build_report_includes_host_inventory_checks() {
    let report = build_report_with_saved_profile(None, false, false);
    let names = report
        .checks
        .iter()
        .map(|check| check.name.as_str())
        .collect::<Vec<_>>();

    assert!(names.iter().any(|name| name.contains("host: claude-code")));
    assert!(names.iter().any(|name| name.contains("host: codex")));
    assert!(
        names
            .iter()
            .any(|name| name.contains("task-linked council"))
    );
    assert!(names.iter().any(|name| name.contains("runtime policy")));
}

#[test]
fn test_task_linked_council_check_passes_when_all_prereqs_exist() {
    let check = check_task_linked_council(
        None,
        &PackageInventory {
            package_metadata_available: true,
            metadata_sources: Vec::new(),
            discovered_packages: vec!["codex:council-reviewer".to_string()],
            discovered_plugins: Vec::new(),
        },
        &WorktreeConfigDiscovery {
            detected: true,
            project_root: Some(std::path::PathBuf::from("/tmp/workspace")),
            discovered_configs: vec![std::path::PathBuf::from("/tmp/workspace/.mcp.json")],
        },
    );

    if tool_registry::find("hyphae")
        .is_some_and(|spec| !matches!(tool_registry::probe(spec), ToolProbe::Installed(_)))
        || tool_registry::find("canopy")
            .is_some_and(|spec| !matches!(tool_registry::probe(spec), ToolProbe::Installed(_)))
    {
        return;
    }

    assert!(check.passed);
    assert_eq!(
        check.message,
        "Task-linked council summon prerequisites look ready."
    );
}

#[test]
fn test_task_linked_council_check_reports_missing_prereqs() {
    let check = check_task_linked_council(
        None,
        &PackageInventory {
            package_metadata_available: false,
            metadata_sources: Vec::new(),
            discovered_packages: Vec::new(),
            discovered_plugins: Vec::new(),
        },
        &WorktreeConfigDiscovery {
            detected: false,
            project_root: None,
            discovered_configs: Vec::new(),
        },
    );

    assert!(!check.passed);
    assert!(check.message.contains("worktree config"));
    assert!(check.message.contains("Lamella package metadata"));
    assert!(
        check
            .repair_actions
            .iter()
            .any(|action| action.command == "stipe init --repair")
    );
    assert!(
        check
            .repair_actions
            .iter()
            .any(|action| action.command == "stipe package")
    );
}

#[test]
fn test_codex_notify_helpers_are_shared() {
    let detail = codex_notify::codex_notify_detail(false);
    assert!(detail.contains("Codex"));
    assert!(host_policy::codex_target_requested(Some("codex")));
}

#[test]
fn test_health_check_struct() {
    let check = HealthCheck {
        name: "test".to_string(),
        passed: true,
        message: "Test passed".to_string(),
        repair_actions: Vec::new(),
    };

    assert_eq!(check.name, "test");
    assert!(check.passed);
    assert_eq!(check.message, "Test passed");
}

#[test]
fn test_optional_canopy_missing_is_not_a_failure() {
    let canopy = tool_registry::find("canopy").expect("canopy spec should exist");
    let check = check_tool(canopy, false);

    if !matches!(tool_registry::probe(canopy), ToolProbe::Missing) {
        return;
    }

    assert!(check.passed);
    assert!(check.message.contains("Optional coordination runtime"));
}

#[test]
fn test_build_report_includes_repair_actions_for_failures() {
    let report = DoctorReport {
        schema_version: STIPE_DOCTOR_SCHEMA_VERSION.to_string(),
        healthy: false,
        summary: "1 checks need attention.".to_string(),
        install_profile: None,
        checks: vec![HealthCheck {
            name: "hyphae database".to_string(),
            passed: false,
            message: "missing".to_string(),
            repair_actions: vec![RepairAction::stipe(
                "init",
                "Initialize the ecosystem",
                "Create the database.",
                &["init"],
                RepairTier::Primary,
            )],
        }],
        hook_paths: vec![],
        repair_actions: vec![RepairAction::stipe(
            "init",
            "Initialize the ecosystem",
            "Create the database.",
            &["init"],
            RepairTier::Primary,
        )],
        drift: None,
        developer_tools: None,
        provider_health: Vec::new(),
        mcp_health: Vec::new(),
        runtime_policy: None,
        package_inventory: None,
        worktree_config: None,
        package_drift: None,
    };

    assert!(!report.healthy);
    assert_eq!(report.repair_actions.len(), 1);
    assert_eq!(report.repair_actions[0].command, "stipe init");
}

#[test]
fn test_render_report_snapshot_for_failure() {
    let report = DoctorReport {
        schema_version: STIPE_DOCTOR_SCHEMA_VERSION.to_string(),
        healthy: false,
        summary: "1 checks need attention.".to_string(),
        install_profile: None,
        checks: vec![HealthCheck {
            name: "hyphae".to_string(),
            passed: false,
            message: "not found in PATH".to_string(),
            repair_actions: vec![],
        }],
        hook_paths: vec![],
        repair_actions: vec![RepairAction::stipe(
            "install hyphae",
            "Install hyphae",
            "Add hyphae to PATH.",
            &["install", "hyphae"],
            RepairTier::Primary,
        )],
        drift: None,
        developer_tools: None,
        provider_health: Vec::new(),
        mcp_health: Vec::new(),
        runtime_policy: None,
        package_inventory: None,
        worktree_config: None,
        package_drift: None,
    };

    assert_eq!(
        render_report(&report, false),
        vec![
            "",
            "Basidiocarp Ecosystem Health Check",
            &"─".repeat(75),
            "",
            "  hyphae               ✗ not found in PATH",
            "",
            "Some checks failed. Use 'stipe init --repair' to repair shared MCP state, 'stipe host doctor' to inspect per-host state, or 'stipe host setup <host>' to restore a specific host.",
            "",
            "Recommended repair actions:",
            "  - stipe install hyphae",
            "",
        ]
    );
}

#[test]
fn test_render_report_includes_hook_paths_section() {
    let report = DoctorReport {
        schema_version: STIPE_DOCTOR_SCHEMA_VERSION.to_string(),
        healthy: false,
        summary: "1 checks need attention.".to_string(),
        install_profile: None,
        checks: vec![HealthCheck {
            name: "hyphae".to_string(),
            passed: true,
            message: "installed".to_string(),
            repair_actions: vec![],
        }],
        hook_paths: vec![claude_hooks::HookPathSnapshot {
            event: "PostToolUse".to_string(),
            path: std::path::PathBuf::from("/tmp/missing-hook.js"),
            passed: false,
        }],
        repair_actions: Vec::new(),
        drift: None,
        developer_tools: None,
        provider_health: Vec::new(),
        mcp_health: Vec::new(),
        runtime_policy: None,
        package_inventory: None,
        worktree_config: None,
        package_drift: None,
    };

    let lines = render_report(&report, false);
    assert!(lines.iter().any(|line| line == "Hooks:"));
    assert!(
        lines
            .iter()
            .any(|line| line.contains("PostToolUse: /tmp/missing-hook.js (not found)"))
    );
}

#[test]
fn test_render_report_includes_drift_section() {
    let report = DoctorReport {
        schema_version: STIPE_DOCTOR_SCHEMA_VERSION.to_string(),
        healthy: false,
        summary: "1 checks need attention.".to_string(),
        install_profile: None,
        checks: vec![HealthCheck {
            name: "config drift".to_string(),
            passed: false,
            message: "1 config drift issue(s) detected".to_string(),
            repair_actions: vec![RepairAction::stipe(
                "repair-init",
                "Repair the init baseline",
                "Reapply shared ecosystem configuration and refresh the baseline manifest.",
                &["init", "--repair"],
                RepairTier::Primary,
            )],
        }],
        hook_paths: vec![],
        repair_actions: vec![RepairAction::stipe(
            "repair-init",
            "Repair the init baseline",
            "Reapply shared ecosystem configuration and refresh the baseline manifest.",
            &["init", "--repair"],
            RepairTier::Primary,
        )],
        drift: Some(DriftReport {
            baseline_path: std::path::PathBuf::from("/tmp/init-baseline.json"),
            generated_at_unix_nanos: 1,
            findings: vec![DriftFinding::ConfigFileModified {
                path: std::path::PathBuf::from("/tmp/config.json"),
                expected_checksum: "deadbeef".to_string(),
                actual_checksum: Some("cafebabe".to_string()),
            }],
        }),
        developer_tools: None,
        provider_health: Vec::new(),
        mcp_health: Vec::new(),
        runtime_policy: None,
        package_inventory: None,
        worktree_config: None,
        package_drift: None,
    };

    let lines = render_report(&report, false);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Config drift detected:"))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("modified since last init"))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("stipe init --repair"))
    );
}

#[test]
fn test_render_report_includes_runtime_policy_section() {
    let report = DoctorReport {
        schema_version: STIPE_DOCTOR_SCHEMA_VERSION.to_string(),
        healthy: true,
        summary: "All checks passed.".to_string(),
        install_profile: None,
        checks: vec![HealthCheck {
            name: "runtime policy".to_string(),
            passed: true,
            message: "No remembered approvals or deny decisions are currently stored.".to_string(),
            repair_actions: Vec::new(),
        }],
        hook_paths: Vec::new(),
        repair_actions: Vec::new(),
        drift: None,
        developer_tools: None,
        provider_health: Vec::new(),
        mcp_health: Vec::new(),
        runtime_policy: Some(RuntimePolicyReport {
            configured: true,
            config_paths: vec![std::path::PathBuf::from("/tmp/runtime-policy.toml")],
            precedence: vec![PolicyScope::Project, PolicyScope::User],
            load_error: None,
            remembered_decisions: vec![RememberedDecision {
                subject: "install-profile:codex".to_string(),
                scope: PolicyScope::User,
                decision: PolicyDecision::Allow,
                source: DecisionSource::OperatorProfile,
                updated_at_unix: 42,
                note: Some("Remembered approval".to_string()),
            }],
            active_install_profile: Some(RememberedDecision {
                subject: "install-profile:codex".to_string(),
                scope: PolicyScope::User,
                decision: PolicyDecision::Allow,
                source: DecisionSource::OperatorProfile,
                updated_at_unix: 42,
                note: Some("Remembered approval".to_string()),
            }),
        }),
        package_inventory: None,
        worktree_config: None,
        package_drift: None,
    };

    let lines = render_report(&report, false);
    assert!(lines.iter().any(|line| line == "Runtime policy:"));
    assert!(
        lines
            .iter()
            .any(|line| line.contains("policy scope precedence: project -> user"))
    );
    assert!(lines.iter().any(|line| line.contains("approval memory")));
    assert!(
        lines
            .iter()
            .any(|line| line.contains("active install profile decision: allow"))
    );
}

#[test]
fn test_build_report_can_include_developer_tools() {
    let report = build_report_with_saved_profile(None, true, false);
    let developer_tools = report
        .developer_tools
        .expect("developer tools section should be present");
    assert!(
        developer_tools
            .checks
            .iter()
            .any(|check| check.tier == DeveloperToolTier::Tier1)
    );
}
