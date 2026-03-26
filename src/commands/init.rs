use crate::ecosystem;
use anyhow::{Result, anyhow};
use colored::Colorize;
use serde::Serialize;
use spore::{Tool, discover};

use super::host_policy;
use super::host_policy::HostMode;
use super::repair::{RepairAction, RepairTier, cargo_install_action, dedupe_repair_actions};
use crate::ecosystem::clients::{self, McpClient};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct InitSnapshot {
    target_client: Option<String>,
    target_is_codex: bool,
    selected_hosts: Vec<HostMode>,
    detected_hosts: Vec<HostMode>,
    detected_clients: Vec<String>,
    hyphae_installed: bool,
    rhizome_installed: bool,
    hyphae_db_exists: bool,
    codex_notify_configured: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum InitStepStatus {
    Planned,
    AlreadyOk,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct InitStep {
    status: InitStepStatus,
    title: String,
    detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct InitPlan {
    dry_run: bool,
    target_client: Option<String>,
    selected_hosts: Vec<String>,
    detected_hosts: Vec<String>,
    detected_clients: Vec<String>,
    steps: Vec<InitStep>,
    repair_actions: Vec<RepairAction>,
}

fn build_snapshot(client: Option<&str>) -> Result<InitSnapshot> {
    let target_client = client.map(ToOwned::to_owned);

    if let Some(target) = client {
        if McpClient::from_flag(target).is_none() {
            return Err(anyhow!(
                "Unknown client '{target}'. Known: claude-code, cursor, windsurf, cline, continue, claude-desktop, codex, gemini, copilot"
            ));
        }
    }
    let target_is_codex = host_policy::codex_target_requested(client);
    let detected_clients_raw = clients::detect_clients();
    let detected_hosts = host_policy::supported_host_modes()
        .iter()
        .copied()
        .filter(|mode| host_policy::host_detected_with_clients(*mode, &detected_clients_raw))
        .collect::<Vec<_>>();
    let selected_hosts = client
        .and_then(host_policy::host_mode_from_client_flag)
        .map(|mode| vec![mode])
        .unwrap_or_else(|| detected_hosts.clone());

    let detected_clients = detected_clients_raw
        .into_iter()
        .filter(|client| *client != McpClient::ClaudeCode)
        .map(|client| client.name().to_string())
        .collect();

    let hyphae_installed = discover(Tool::Hyphae).is_some();
    let rhizome_installed = discover(Tool::Rhizome).is_some();
    let hyphae_db_exists = dirs::data_dir()
        .map(|dir| dir.join("hyphae").join("hyphae.db"))
        .is_some_and(|db_path| db_path.exists());

    let codex_notify_configured = host_policy::codex_notify_configured();

    Ok(InitSnapshot {
        target_client,
        target_is_codex,
        selected_hosts,
        detected_hosts,
        detected_clients,
        hyphae_installed,
        rhizome_installed,
        hyphae_db_exists,
        codex_notify_configured,
    })
}

fn selected_mode_label(snapshot: &InitSnapshot) -> String {
    if !snapshot.selected_hosts.is_empty() {
        snapshot
            .selected_hosts
            .iter()
            .map(|mode| mode.label().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    } else if snapshot.target_is_codex {
        "Codex host mode".to_string()
    } else {
        "detected host inventory".to_string()
    }
}

fn render_preview(snapshot: &InitSnapshot) -> Vec<String> {
    let plan = build_plan(snapshot, true);
    let mut lines = Vec::new();

    for step in plan.steps {
        let line = match step.status {
            InitStepStatus::Planned => format!("Would {}. {}", step.title, step.detail),
            InitStepStatus::AlreadyOk => format!("Already OK: {}. {}", step.title, step.detail),
            InitStepStatus::Skipped => format!("Would skip: {}. {}", step.title, step.detail),
        };
        lines.push(line);
    }

    lines
}

fn print_preview(snapshot: &InitSnapshot) {
    println!("{}", "Dry run: no changes will be made.".yellow());
    println!();

    for line in render_preview(snapshot) {
        println!("  {line}");
    }

    println!();
}

fn build_steps(snapshot: &InitSnapshot) -> Vec<InitStep> {
    let mut steps = Vec::new();

    if !snapshot.selected_hosts.is_empty() {
        steps.push(InitStep {
            status: InitStepStatus::Planned,
            title: if snapshot.target_client.is_some() {
                format!("target {}", selected_mode_label(snapshot))
            } else {
                "configure detected host inventory".to_string()
            },
            detail: if snapshot.target_client.is_some() {
                "Use the selected host inventory for registration instead of inferring setup from unrelated clients."
                    .to_string()
            } else {
                format!("Detected host inventory: {}", selected_mode_label(snapshot))
            },
        });
    } else if let Some(client) = &snapshot.target_client {
        steps.push(InitStep {
            status: InitStepStatus::Planned,
            title: format!("target {client}"),
            detail: "Use the selected host inventory for registration.".to_string(),
        });
    } else if snapshot.detected_clients.is_empty() {
        steps.push(InitStep {
            status: InitStepStatus::Skipped,
            title: "configure host inventory".to_string(),
            detail: "No supported host inventory was detected on this machine.".to_string(),
        });
    } else {
        steps.push(InitStep {
            status: InitStepStatus::Planned,
            title: "configure detected host inventory".to_string(),
            detail: format!(
                "Detected hosts: {}",
                snapshot
                    .detected_hosts
                    .iter()
                    .map(|mode| mode.label())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }

    steps.push(InitStep {
        status: if snapshot.hyphae_installed {
            InitStepStatus::Planned
        } else {
            InitStepStatus::Skipped
        },
        title: "register the hyphae MCP server".to_string(),
        detail: if snapshot.hyphae_installed {
            "Hyphae is installed and can be wired into supported clients.".to_string()
        } else {
            "Hyphae is not installed yet.".to_string()
        },
    });

    steps.push(InitStep {
        status: if snapshot.rhizome_installed {
            InitStepStatus::Planned
        } else {
            InitStepStatus::Skipped
        },
        title: "register the rhizome MCP server".to_string(),
        detail: if snapshot.rhizome_installed {
            "Rhizome is installed and can be wired into supported clients.".to_string()
        } else {
            "Rhizome is not installed yet.".to_string()
        },
    });

    steps.push(InitStep {
        status: if snapshot.hyphae_db_exists {
            InitStepStatus::AlreadyOk
        } else {
            InitStepStatus::Planned
        },
        title: "initialize the Hyphae database".to_string(),
        detail: if snapshot.hyphae_db_exists {
            "The Hyphae database already exists.".to_string()
        } else {
            "Hyphae will create its local database on first access.".to_string()
        },
    });

    if snapshot.selected_hosts.contains(&HostMode::Codex)
        || snapshot.detected_hosts.contains(&HostMode::Codex)
    {
        steps.push(InitStep {
            status: if snapshot.codex_notify_configured {
                InitStepStatus::AlreadyOk
            } else {
                InitStepStatus::Planned
            },
            title: "configure the Codex notify adapter".to_string(),
            detail: host_policy::codex_notify_detail(snapshot.codex_notify_configured),
        });
    }

    steps.push(InitStep {
        status: InitStepStatus::Planned,
        title: "patch the local instruction file with ecosystem guidance".to_string(),
        detail: "Keep the workspace instructions aligned with the installed ecosystem.".to_string(),
    });

    steps
}

fn build_repair_actions(snapshot: &InitSnapshot) -> Vec<RepairAction> {
    let mut actions = snapshot
        .selected_hosts
        .iter()
        .copied()
        .map(host_policy::host_setup_repair_action)
        .collect::<Vec<_>>();
    let install_profile = host_policy::preferred_install_profile(
        snapshot.target_client.as_deref(),
        &snapshot.detected_clients,
    );

    if !snapshot.hyphae_installed || !snapshot.rhizome_installed {
        if snapshot.selected_hosts.is_empty() {
            actions.push(host_policy::install_profile_repair_action(install_profile));
        }
    }

    if !snapshot.hyphae_db_exists
        || snapshot.target_client.is_some()
        || !snapshot.detected_clients.is_empty()
    {
        actions.push(RepairAction::stipe(
            "init",
            "Initialize the ecosystem",
            "Apply MCP registrations, Codex host mode guidance, and Hyphae bootstrap work.",
            &["init"],
            RepairTier::Primary,
        ));
    }

    if snapshot.selected_hosts.contains(&HostMode::Codex)
        || snapshot.detected_hosts.contains(&HostMode::Codex)
    {
        if !snapshot.codex_notify_configured {
            actions.push(host_policy::codex_notify_repair_action());
        }
    }

    if !snapshot.hyphae_installed {
        actions.push(cargo_install_action("hyphae"));
    }

    if !snapshot.rhizome_installed {
        actions.push(cargo_install_action("rhizome"));
    }

    dedupe_repair_actions(actions)
}

fn build_plan(snapshot: &InitSnapshot, dry_run: bool) -> InitPlan {
    InitPlan {
        dry_run,
        target_client: snapshot.target_client.clone(),
        selected_hosts: snapshot
            .selected_hosts
            .iter()
            .map(|mode| mode.label().to_string())
            .collect(),
        detected_hosts: snapshot
            .detected_hosts
            .iter()
            .map(|mode| mode.label().to_string())
            .collect(),
        detected_clients: snapshot.detected_clients.clone(),
        steps: build_steps(snapshot),
        repair_actions: build_repair_actions(snapshot),
    }
}

pub fn run(client: Option<&str>, dry_run: bool, json: bool) -> Result<()> {
    let snapshot = build_snapshot(client)?;
    let plan = build_plan(&snapshot, dry_run);

    if json {
        if !dry_run {
            ecosystem::run_ecosystem(client, 0)?;
        }
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }

    if dry_run {
        print_preview(&snapshot);
        return Ok(());
    }

    ecosystem::run_ecosystem(client, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_preview_mentions_target_client_and_actions() {
        let snapshot = InitSnapshot {
            target_client: Some("cursor".to_string()),
            target_is_codex: false,
            selected_hosts: vec![HostMode::Cursor],
            detected_hosts: vec![HostMode::Cursor],
            detected_clients: vec!["Cursor".to_string(), "Continue".to_string()],
            hyphae_installed: true,
            rhizome_installed: false,
            hyphae_db_exists: false,
            codex_notify_configured: false,
        };

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
    fn test_render_preview_lists_detected_clients_when_unfiltered() {
        let snapshot = InitSnapshot {
            target_client: None,
            target_is_codex: false,
            selected_hosts: vec![HostMode::Cursor],
            detected_hosts: vec![HostMode::Cursor],
            detected_clients: vec!["Cursor".to_string(), "Continue".to_string()],
            hyphae_installed: false,
            rhizome_installed: false,
            hyphae_db_exists: true,
            codex_notify_configured: false,
        };

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
        let snapshot = InitSnapshot {
            target_client: None,
            target_is_codex: false,
            selected_hosts: vec![HostMode::Cursor],
            detected_hosts: vec![HostMode::Cursor],
            detected_clients: vec!["Cursor".to_string()],
            hyphae_installed: false,
            rhizome_installed: true,
            hyphae_db_exists: false,
            codex_notify_configured: false,
        };

        let plan = build_plan(&snapshot, true);
        let commands = plan
            .repair_actions
            .iter()
            .map(|action| action.command.as_str())
            .collect::<Vec<_>>();

        assert!(commands.contains(&"stipe host setup cursor"));
        assert!(commands.contains(&"stipe init"));
        assert!(commands.contains(&"cargo install hyphae"));
    }

    #[test]
    fn test_render_preview_mentions_codex_notify_adapter() {
        let snapshot = InitSnapshot {
            target_client: Some("codex".to_string()),
            target_is_codex: true,
            selected_hosts: vec![HostMode::Codex],
            detected_hosts: vec![HostMode::Codex],
            detected_clients: vec!["Codex CLI".to_string()],
            hyphae_installed: true,
            rhizome_installed: true,
            hyphae_db_exists: true,
            codex_notify_configured: false,
        };

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
        let snapshot = InitSnapshot {
            target_client: Some("codex".to_string()),
            target_is_codex: true,
            selected_hosts: vec![HostMode::Codex],
            detected_hosts: vec![HostMode::Codex],
            detected_clients: vec!["Codex CLI".to_string()],
            hyphae_installed: false,
            rhizome_installed: false,
            hyphae_db_exists: false,
            codex_notify_configured: false,
        };

        let plan = build_plan(&snapshot, true);
        let commands = plan
            .repair_actions
            .iter()
            .map(|action| action.command.as_str())
            .collect::<Vec<_>>();

        assert!(commands.contains(&"stipe host setup codex"));
        assert!(commands.contains(&"hyphae init"));
    }

    #[test]
    fn test_build_plan_does_not_switch_to_codex_profile_for_non_codex_targets() {
        let snapshot = InitSnapshot {
            target_client: Some("cursor".to_string()),
            target_is_codex: false,
            selected_hosts: vec![HostMode::Cursor],
            detected_hosts: vec![HostMode::Cursor, HostMode::Codex],
            detected_clients: vec!["Codex CLI".to_string(), "Cursor".to_string()],
            hyphae_installed: false,
            rhizome_installed: false,
            hyphae_db_exists: false,
            codex_notify_configured: false,
        };

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
        let snapshot = InitSnapshot {
            target_client: Some("claude-code".to_string()),
            target_is_codex: false,
            selected_hosts: vec![HostMode::ClaudeCode],
            detected_hosts: vec![HostMode::ClaudeCode],
            detected_clients: vec!["Claude Code".to_string()],
            hyphae_installed: false,
            rhizome_installed: false,
            hyphae_db_exists: true,
            codex_notify_configured: false,
        };

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
    fn test_build_plan_prefers_codex_profile_when_codex_is_detected_by_default() {
        let snapshot = InitSnapshot {
            target_client: None,
            target_is_codex: false,
            selected_hosts: vec![HostMode::Codex],
            detected_hosts: vec![HostMode::Codex],
            detected_clients: vec!["Codex CLI".to_string()],
            hyphae_installed: false,
            rhizome_installed: false,
            hyphae_db_exists: false,
            codex_notify_configured: false,
        };

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
        let snapshot = InitSnapshot {
            target_client: None,
            target_is_codex: false,
            selected_hosts: vec![HostMode::ClaudeCode, HostMode::Codex],
            detected_hosts: vec![HostMode::ClaudeCode, HostMode::Codex],
            detected_clients: vec!["Claude Code".to_string(), "Codex CLI".to_string()],
            hyphae_installed: true,
            rhizome_installed: true,
            hyphae_db_exists: true,
            codex_notify_configured: false,
        };

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
        assert_eq!(
            host_policy::codex_notify_detail(true),
            "Codex host mode already points at Hyphae via its notify adapter."
        );
    }
}
