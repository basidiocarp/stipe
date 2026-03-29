use crate::ecosystem;
use anyhow::Result;

use super::host_policy::HostConfigScope;

mod model;
mod plan;
mod render;
mod snapshot;

use plan::build_plan;
use render::print_preview;
use snapshot::build_snapshot;

#[cfg(test)]
use super::host_policy::HostMode;
#[cfg(test)]
use model::{ClaudeSnapshot, CodexSnapshot, InitSnapshot, InitStepStatus, ToolSnapshot};
#[cfg(test)]
use render::render_preview;

pub fn run(client: Option<&str>, scope: HostConfigScope, dry_run: bool, json: bool) -> Result<()> {
    let snapshot = build_snapshot(client, scope)?;
    let plan = build_plan(&snapshot, dry_run);

    if json {
        if !dry_run {
            ecosystem::run_ecosystem(client, scope, 0)?;
        }
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }

    if dry_run {
        print_preview(&snapshot);
        return Ok(());
    }

    ecosystem::run_ecosystem(client, scope, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{codex_notify, host_policy};

    struct SnapshotFixture {
        target_client: Option<&'static str>,
        selected_hosts: Vec<HostMode>,
        detected_hosts: Vec<HostMode>,
        detected_clients: Vec<&'static str>,
        tools: ToolSnapshot,
        codex: CodexSnapshot,
        claude: ClaudeSnapshot,
    }

    impl Default for SnapshotFixture {
        fn default() -> Self {
            Self {
                target_client: None,
                selected_hosts: Vec::new(),
                detected_hosts: Vec::new(),
                detected_clients: Vec::new(),
                tools: ToolSnapshot {
                    hyphae_installed: false,
                    hyphae_broken: false,
                    rhizome_installed: false,
                    rhizome_broken: false,
                    cortina_installed: false,
                    cortina_broken: false,
                    hyphae_db_exists: false,
                },
                codex: CodexSnapshot {
                    notify_configured: false,
                },
                claude: ClaudeSnapshot {
                    hooks_configured: false,
                },
            }
        }
    }

    fn snapshot(fixture: SnapshotFixture) -> InitSnapshot {
        InitSnapshot {
            target_client: fixture.target_client.map(ToOwned::to_owned),
            selected_hosts: fixture.selected_hosts,
            detected_hosts: fixture.detected_hosts,
            detected_clients: fixture
                .detected_clients
                .into_iter()
                .map(ToOwned::to_owned)
                .collect(),
            tools: fixture.tools,
            codex: fixture.codex,
            claude: fixture.claude,
        }
    }

    #[test]
    fn test_render_preview_mentions_target_client_and_actions() {
        let snapshot = snapshot(SnapshotFixture {
            target_client: Some("cursor"),
            selected_hosts: vec![HostMode::Cursor],
            detected_hosts: vec![HostMode::Cursor],
            detected_clients: vec!["Cursor", "Continue"],
            tools: ToolSnapshot {
                hyphae_installed: true,
                ..Default::default()
            },
            ..Default::default()
        });

        let lines = render_preview(&snapshot);
        assert!(lines.iter().any(|line| line.contains("target Cursor mode")));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("register the hyphae MCP server"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Would skip: register the rhizome MCP server"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("initialize the Hyphae database"))
        );
    }

    #[test]
    fn test_render_preview_snapshot_for_cursor_target() {
        let snapshot = snapshot(SnapshotFixture {
            target_client: Some("cursor"),
            selected_hosts: vec![HostMode::Cursor],
            detected_hosts: vec![HostMode::Cursor],
            detected_clients: vec!["Cursor"],
            tools: ToolSnapshot {
                hyphae_installed: true,
                hyphae_db_exists: true,
                ..Default::default()
            },
            ..Default::default()
        });

        assert_eq!(
            render_preview(&snapshot),
            vec![
                "Would target Cursor mode. Use the selected host inventory for registration instead of inferring setup from unrelated clients.",
                "Would register the hyphae MCP server. Hyphae is installed and can be wired into supported clients.",
                "Would skip: register the rhizome MCP server. Rhizome is not installed yet.",
                "Already OK: initialize the Hyphae database. The Hyphae database already exists.",
                "Would patch the local instruction file with ecosystem guidance. Keep the workspace instructions aligned with the installed ecosystem.",
            ]
        );
    }

    #[test]
    fn test_render_preview_lists_detected_clients_when_unfiltered() {
        let snapshot = snapshot(SnapshotFixture {
            selected_hosts: vec![HostMode::Cursor],
            detected_hosts: vec![HostMode::Cursor],
            detected_clients: vec!["Cursor", "Continue"],
            tools: ToolSnapshot {
                hyphae_db_exists: true,
                ..Default::default()
            },
            ..Default::default()
        });

        let lines = render_preview(&snapshot);
        assert!(
            lines
                .iter()
                .any(|line| line.contains("configure detected host inventory"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("The Hyphae database already exists"))
        );
    }

    #[test]
    fn test_build_plan_contains_repair_actions() {
        let snapshot = snapshot(SnapshotFixture {
            selected_hosts: vec![HostMode::Cursor],
            detected_hosts: vec![HostMode::Cursor],
            detected_clients: vec!["Cursor"],
            tools: ToolSnapshot {
                rhizome_installed: true,
                ..Default::default()
            },
            ..Default::default()
        });

        let plan = build_plan(&snapshot, true);
        let commands = plan
            .repair_actions
            .iter()
            .map(|action| action.command.as_str())
            .collect::<Vec<_>>();

        assert!(commands.contains(&"stipe host setup cursor"));
        assert!(commands.contains(&"stipe init"));
        assert!(commands.contains(&"stipe install hyphae"));
    }

    #[test]
    fn test_render_preview_mentions_codex_notify_adapter() {
        let snapshot = snapshot(SnapshotFixture {
            target_client: Some("codex"),
            selected_hosts: vec![HostMode::Codex],
            detected_hosts: vec![HostMode::Codex],
            detected_clients: vec!["Codex CLI"],
            tools: ToolSnapshot {
                hyphae_installed: true,
                rhizome_installed: true,
                hyphae_db_exists: true,
                ..Default::default()
            },
            ..Default::default()
        });

        let lines = render_preview(&snapshot);
        assert!(
            lines
                .iter()
                .any(|line| line.contains("target Codex host mode"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("configure the Codex notify adapter"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Codex notify adapter"))
        );
    }

    #[test]
    fn test_build_plan_prefers_codex_profile_for_codex_targets() {
        let snapshot = snapshot(SnapshotFixture {
            target_client: Some("codex"),
            selected_hosts: vec![HostMode::Codex],
            detected_hosts: vec![HostMode::Codex],
            detected_clients: vec!["Codex CLI"],
            ..Default::default()
        });

        let plan = build_plan(&snapshot, true);
        let commands = plan
            .repair_actions
            .iter()
            .map(|action| action.command.as_str())
            .collect::<Vec<_>>();

        assert!(commands.contains(&"stipe host setup codex"));
        assert!(
            commands
                .iter()
                .any(|command| command.starts_with("stipe init --client codex"))
        );
    }

    #[test]
    fn test_build_plan_does_not_switch_to_codex_profile_for_non_codex_targets() {
        let snapshot = snapshot(SnapshotFixture {
            target_client: Some("cursor"),
            selected_hosts: vec![HostMode::Cursor],
            detected_hosts: vec![HostMode::Cursor, HostMode::Codex],
            detected_clients: vec!["Codex CLI", "Cursor"],
            ..Default::default()
        });

        let plan = build_plan(&snapshot, true);
        let commands = plan
            .repair_actions
            .iter()
            .map(|action| action.command.as_str())
            .collect::<Vec<_>>();

        assert!(commands.contains(&"stipe host setup cursor"));
        assert!(!commands.contains(&"stipe install --profile codex"));
    }

    #[test]
    fn test_build_plan_uses_host_setup_for_supported_target_hosts() {
        let snapshot = snapshot(SnapshotFixture {
            target_client: Some("claude-code"),
            selected_hosts: vec![HostMode::ClaudeCode],
            detected_hosts: vec![HostMode::ClaudeCode],
            detected_clients: vec!["Claude Code"],
            tools: ToolSnapshot {
                cortina_installed: true,
                hyphae_db_exists: true,
                ..Default::default()
            },
            ..Default::default()
        });

        let plan = build_plan(&snapshot, true);
        let commands = plan
            .repair_actions
            .iter()
            .map(|action| action.command.as_str())
            .collect::<Vec<_>>();

        assert!(commands.contains(&"stipe host setup claude-code"));
        assert!(!commands.contains(&"stipe install --profile claude-code"));
    }

    #[test]
    fn test_render_preview_mentions_claude_hooks_when_cortina_is_available() {
        let snapshot = snapshot(SnapshotFixture {
            target_client: Some("claude-code"),
            selected_hosts: vec![HostMode::ClaudeCode],
            detected_hosts: vec![HostMode::ClaudeCode],
            detected_clients: vec!["Claude Code"],
            tools: ToolSnapshot {
                hyphae_installed: true,
                rhizome_installed: true,
                cortina_installed: true,
                hyphae_broken: false,
                rhizome_broken: false,
                cortina_broken: false,
                hyphae_db_exists: true,
            },
            ..Default::default()
        });

        let lines = render_preview(&snapshot);
        assert!(
            lines
                .iter()
                .any(|line| line.contains("install the Cortina Claude hooks"))
        );
    }

    #[test]
    fn test_build_plan_prefers_codex_profile_when_codex_is_detected_by_default() {
        let snapshot = snapshot(SnapshotFixture {
            selected_hosts: vec![HostMode::Codex],
            detected_hosts: vec![HostMode::Codex],
            detected_clients: vec!["Codex CLI"],
            ..Default::default()
        });

        let plan = build_plan(&snapshot, true);
        let commands = plan
            .repair_actions
            .iter()
            .map(|action| action.command.as_str())
            .collect::<Vec<_>>();

        assert!(commands.contains(&"stipe host setup codex"));
        assert!(!commands.contains(&"stipe install --profile codex"));
    }

    #[test]
    fn test_render_preview_reports_multiple_detected_hosts() {
        let snapshot = snapshot(SnapshotFixture {
            selected_hosts: vec![HostMode::ClaudeCode, HostMode::Codex],
            detected_hosts: vec![HostMode::ClaudeCode, HostMode::Codex],
            detected_clients: vec!["Claude Code", "Codex CLI"],
            tools: ToolSnapshot {
                hyphae_installed: true,
                rhizome_installed: true,
                cortina_installed: true,
                hyphae_broken: false,
                rhizome_broken: false,
                cortina_broken: false,
                hyphae_db_exists: true,
            },
            ..Default::default()
        });

        let lines = render_preview(&snapshot);

        assert!(
            lines
                .iter()
                .any(|line| line.contains("configure detected host inventory"))
        );
        assert!(
            lines
                .iter()
                .any(|line| { line.contains("Claude Code operator mode, Codex host mode") })
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("configure the Codex notify adapter"))
        );
    }

    #[test]
    fn test_codex_notify_helpers_use_expected_values() {
        assert!(host_policy::codex_target_requested(Some("codex")));
        assert!(host_policy::codex_host_mode_requested(
            None,
            &["Codex CLI".to_string()]
        ));
        assert!(codex_notify::codex_notify_detail(true).contains("Codex host mode"));
    }

    #[test]
    fn test_claude_hooks_step_skips_broken_cortina_with_repair_guidance() {
        let snapshot = snapshot(SnapshotFixture {
            target_client: Some("claude-code"),
            selected_hosts: vec![HostMode::ClaudeCode],
            detected_hosts: vec![HostMode::ClaudeCode],
            detected_clients: vec!["Claude Code"],
            tools: ToolSnapshot {
                hyphae_installed: true,
                hyphae_broken: false,
                rhizome_installed: true,
                rhizome_broken: false,
                cortina_installed: false,
                cortina_broken: true,
                hyphae_db_exists: true,
            },
            ..Default::default()
        });

        let plan = build_plan(&snapshot, true);
        let hook_step = plan
            .steps
            .iter()
            .find(|step| step.title == "install the Cortina Claude hooks")
            .expect("expected Claude hooks step");

        assert_eq!(hook_step.status, InitStepStatus::Skipped);
        assert!(hook_step.detail.contains("installed but broken"));
        assert!(
            plan.repair_actions
                .iter()
                .any(|action| action.command == "stipe install cortina")
        );
    }

    #[test]
    fn test_mcp_registration_step_skips_broken_tools_with_repair_guidance() {
        let snapshot = snapshot(SnapshotFixture {
            selected_hosts: vec![HostMode::Cursor],
            detected_hosts: vec![HostMode::Cursor],
            detected_clients: vec!["Cursor"],
            tools: ToolSnapshot {
                hyphae_installed: false,
                hyphae_broken: true,
                rhizome_installed: false,
                rhizome_broken: true,
                ..Default::default()
            },
            ..Default::default()
        });

        let plan = build_plan(&snapshot, true);

        let hyphae_step = plan
            .steps
            .iter()
            .find(|step| step.title == "register the hyphae MCP server")
            .expect("expected hyphae step");
        let rhizome_step = plan
            .steps
            .iter()
            .find(|step| step.title == "register the rhizome MCP server")
            .expect("expected rhizome step");

        assert!(hyphae_step.detail.contains("installed but broken"));
        assert!(rhizome_step.detail.contains("installed but broken"));
        assert!(
            plan.repair_actions
                .iter()
                .any(|action| action.command == "stipe install hyphae")
        );
        assert!(
            plan.repair_actions
                .iter()
                .any(|action| action.command == "stipe install rhizome")
        );
    }
}
