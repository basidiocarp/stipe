use super::*;
use crate::commands::codex_notify;
use crate::commands::developer_tools::DeveloperToolTier;
use crate::commands::host_policy::HostMode;
use crate::commands::repair::{RepairAction, RepairTier};
use crate::commands::runtime_policy::{
    DecisionSource, PolicyDecision, PolicyScope, RememberedDecision, RuntimePolicyReport,
};
use crate::commands::tool_registry::ToolProbe;
use std::fs;

use super::config_checks::{codex_notify_adapter_configured_at_path, config_mentions_servers};
use super::council_checks::check_task_linked_council;
use super::model::{ConfigFormat, McpHealth};
use super::tool_checks::check_hyphae_db_at_path;
use super::{host_policy, tool_registry};
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_test_dir(label: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("stipe-{label}-{nonce}"))
}

#[test]
fn test_check_hyphae_db_exists() {
    let temp_dir = unique_test_dir("test-hyphae-exists");
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
    let temp_dir = unique_test_dir("test-hyphae-missing");
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
        mcp_server_health: Vec::new(),
        api_key_health: Vec::new(),
        plugin_inventory: None,
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
        mcp_server_health: Vec::new(),
        api_key_health: Vec::new(),
        plugin_inventory: None,
    };

    assert_eq!(
        render_report(&report, false, false),
        vec![
            "",
            "Basidiocarp Ecosystem Health Check",
            &"─".repeat(75),
            "",
            "Overview:",
            "  hyphae               ✗ not found in PATH",
            "",
            "State: 1 checks need attention.",
            "Next step: run `stipe install hyphae`",
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
        mcp_server_health: Vec::new(),
        api_key_health: Vec::new(),
        plugin_inventory: None,
    };

    let lines = render_report(&report, false, false);
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
        mcp_server_health: Vec::new(),
        api_key_health: Vec::new(),
        plugin_inventory: None,
    };

    let lines = render_report(&report, false, false);
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
        mcp_server_health: Vec::new(),
        api_key_health: Vec::new(),
        plugin_inventory: None,
    };

    let lines = render_report(&report, false, false);
    assert!(lines.iter().any(|line| line == "Runtime policy:"));
    assert!(
        lines
            .iter()
            .any(|line| line.contains("policy scope precedence: project -> user"))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("approval memory: 1 allow, 0 deny"))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("active install profile decision: allow"))
    );
    assert!(lines.iter().any(|line| line == "State: All checks passed."));
    assert!(lines.iter().any(|line| line
        == "Next step: stay on the current ecosystem configuration; no repair action is needed"));
    assert!(lines.iter().any(|line| line
        == "Optional follow-up: run `stipe doctor --deep` for the expanded operator report"));
}

#[test]
fn test_render_provider_health_compacts_healthy_entries() {
    let lines = render_provider_health(
        &[ProviderHealth {
            host: HostMode::Codex,
            provider: "Codex host mode".to_string(),
            available: true,
            healthy: true,
            status: "provider ready".to_string(),
            auth_freshness: AuthFreshness::Fresh,
            auth_detail: Some("auth config appears fresh (~/.codex/config.toml)".to_string()),
        }],
        false,
        false,
    );

    assert_eq!(
        lines,
        vec![
            "Providers:".to_string(),
            "  ✓ codex        provider ready (auth: fresh)".to_string(),
        ]
    );
}

#[test]
fn test_render_mcp_health_keeps_detail_for_unhealthy_entries_only() {
    let lines = render_mcp_health(
        &[
            McpHealth {
                host: HostMode::Codex,
                config_paths: vec![std::path::PathBuf::from("/tmp/codex.toml")],
                required_servers: vec!["hyphae".to_string(), "rhizome".to_string()],
                registered_servers: vec!["hyphae".to_string(), "rhizome".to_string()],
                missing_servers: Vec::new(),
                healthy: true,
                status: "required MCP servers are registered".to_string(),
                auth_freshness: AuthFreshness::Fresh,
            },
            McpHealth {
                host: HostMode::ClaudeCode,
                config_paths: vec![std::path::PathBuf::from("/tmp/claude.json")],
                required_servers: vec!["hyphae".to_string(), "rhizome".to_string()],
                registered_servers: Vec::new(),
                missing_servers: vec!["hyphae".to_string(), "rhizome".to_string()],
                healthy: false,
                status: "missing MCP registration for hyphae, rhizome".to_string(),
                auth_freshness: AuthFreshness::Fresh,
            },
        ],
        false,
        false,
    );

    assert_eq!(
        lines,
        vec![
            "MCP status:".to_string(),
            "  ✓ codex        required MCP servers are registered (auth: fresh)".to_string(),
            "  ✗ claude-code  missing MCP registration for hyphae, rhizome (auth: fresh)"
                .to_string(),
            "    config: /tmp/claude.json".to_string(),
            "    missing: hyphae, rhizome".to_string(),
        ]
    );
}

#[test]
fn test_render_report_summarizes_host_checks_before_detail() {
    let report = DoctorReport {
        schema_version: STIPE_DOCTOR_SCHEMA_VERSION.to_string(),
        healthy: false,
        summary: "1 checks need attention.".to_string(),
        install_profile: None,
        checks: vec![
            HealthCheck {
                name: "mycelium".to_string(),
                passed: true,
                message: "installed".to_string(),
                repair_actions: Vec::new(),
            },
            HealthCheck {
                name: "host: codex".to_string(),
                passed: true,
                message: "Codex host mode detected on this machine".to_string(),
                repair_actions: Vec::new(),
            },
            HealthCheck {
                name: "host: codex".to_string(),
                passed: true,
                message:
                    "Codex host mode already points at Hyphae via notify in ~/.codex/config.toml."
                        .to_string(),
                repair_actions: Vec::new(),
            },
            HealthCheck {
                name: "host: cursor".to_string(),
                passed: false,
                message: "Cursor mode is not detected on this machine".to_string(),
                repair_actions: Vec::new(),
            },
            HealthCheck {
                name: "host: cursor".to_string(),
                passed: false,
                message: "Cursor is not detected on this machine yet.".to_string(),
                repair_actions: Vec::new(),
            },
        ],
        hook_paths: Vec::new(),
        repair_actions: Vec::new(),
        drift: None,
        developer_tools: None,
        provider_health: Vec::new(),
        mcp_health: Vec::new(),
        runtime_policy: None,
        package_inventory: None,
        worktree_config: None,
        package_drift: None,
        mcp_server_health: Vec::new(),
        api_key_health: Vec::new(),
        plugin_inventory: None,
    };

    let lines = render_report(&report, false, false);
    assert!(lines.iter().any(|line| line == "Overview:"));
    assert!(lines.iter().any(|line| line.contains("host status")
        && line.contains("1 ready, 1 host mode needs attention")));
    assert!(lines.iter().any(|line| line == "Host status:"));
    assert!(
        lines
            .iter()
            .any(|line| line.contains("codex") && line.contains("already points at Hyphae"))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("cursor") && line.contains("not detected on this machine"))
    );
}

#[test]
fn test_render_package_inventory_prefers_counts_and_families() {
    let lines = render_package_inventory(
        &PackageInventory {
            package_metadata_available: true,
            metadata_sources: vec![std::path::PathBuf::from("/tmp/lamella/resources")],
            discovered_packages: vec![
                "codex:core".to_string(),
                "codex:workflow".to_string(),
                "claude:core".to_string(),
            ],
            discovered_plugins: vec![
                "/tmp/plugins/cache".to_string(),
                "/tmp/plugins/lamella".to_string(),
            ],
        },
        false,
        false,
    );

    assert!(
        lines
            .iter()
            .any(|line| line == "Package and plugin inventory:")
    );
    assert!(lines.iter().any(|line| line == "  packages: 3 discovered"));
    assert!(
        lines
            .iter()
            .any(|line| line == "  families: codex (2), claude (1)")
    );
    assert!(
        lines
            .iter()
            .any(|line| line == "  plugins: 2 discovered (cache, lamella)")
    );
}

#[test]
fn test_render_package_drift_skips_cleanly_without_saved_profile() {
    let lines = render_package_drift(
        &PackageDrift {
            metadata_available: false,
            expected_packages: Vec::new(),
            installed_packages: Vec::new(),
            missing_packages: Vec::new(),
        },
        false,
        false,
    );

    assert_eq!(
        lines,
        vec![
            "Package drift:".to_string(),
            "  status: no saved install profile; checks skipped".to_string(),
        ]
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn test_render_report_deep_widens_human_sections() {
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
        provider_health: vec![ProviderHealth {
            host: HostMode::Codex,
            provider: "Codex host mode".to_string(),
            available: true,
            healthy: true,
            status: "provider ready".to_string(),
            auth_freshness: AuthFreshness::Fresh,
            auth_detail: Some("auth config appears fresh (~/.codex/config.toml)".to_string()),
        }],
        mcp_health: vec![McpHealth {
            host: HostMode::Codex,
            config_paths: vec![std::path::PathBuf::from("/tmp/codex.toml")],
            required_servers: vec!["hyphae".to_string(), "rhizome".to_string()],
            registered_servers: vec!["hyphae".to_string(), "rhizome".to_string()],
            missing_servers: Vec::new(),
            healthy: true,
            status: "required MCP servers are registered".to_string(),
            auth_freshness: AuthFreshness::Fresh,
        }],
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
            active_install_profile: None,
        }),
        package_inventory: Some(PackageInventory {
            package_metadata_available: true,
            metadata_sources: vec![std::path::PathBuf::from("/tmp/lamella/resources")],
            discovered_packages: vec!["codex:core".to_string(), "codex:workflow".to_string()],
            discovered_plugins: vec!["/tmp/plugins/cache".to_string()],
        }),
        worktree_config: Some(WorktreeConfigDiscovery {
            detected: true,
            project_root: Some(std::path::PathBuf::from("/tmp/workspace")),
            discovered_configs: vec![std::path::PathBuf::from(
                "/tmp/workspace/.codex/config.toml",
            )],
        }),
        package_drift: Some(PackageDrift {
            metadata_available: true,
            expected_packages: vec!["codex:core".to_string()],
            installed_packages: vec!["codex:core".to_string()],
            missing_packages: Vec::new(),
        }),
        mcp_server_health: Vec::new(),
        api_key_health: Vec::new(),
        plugin_inventory: None,
    };

    let shallow = render_report(&report, false, false);
    let deep = render_report(&report, false, true);

    assert!(!shallow.iter().any(|line| line.contains("package detail:")));
    assert!(
        deep.iter()
            .any(|line| line.contains("package detail: codex:core, codex:workflow"))
    );
    assert!(
        !shallow
            .iter()
            .any(|line| line.contains("registered: hyphae, rhizome"))
    );
    assert!(
        deep.iter()
            .any(|line| line.contains("registered: hyphae, rhizome"))
    );
    assert!(
        !shallow
            .iter()
            .any(|line| line.contains("note: Remembered approval"))
    );
    assert!(
        deep.iter()
            .any(|line| line.contains("note: Remembered approval"))
    );
    assert!(
        !shallow
            .iter()
            .any(|line| line.contains("/tmp/workspace/.codex/config.toml"))
    );
    assert!(
        deep.iter()
            .any(|line| line.contains("/tmp/workspace/.codex/config.toml"))
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
