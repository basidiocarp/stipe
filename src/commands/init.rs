use crate::ecosystem;
use anyhow::{Result, anyhow};
use colored::Colorize;
use serde::Serialize;
use spore::{Tool, discover};

use super::repair::{RepairAction, RepairTier, cargo_install_action, dedupe_repair_actions};
use crate::ecosystem::clients::{self, McpClient};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct InitSnapshot {
    target_client: Option<String>,
    detected_clients: Vec<String>,
    hyphae_installed: bool,
    rhizome_installed: bool,
    hyphae_db_exists: bool,
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
    detected_clients: Vec<String>,
    steps: Vec<InitStep>,
    repair_actions: Vec<RepairAction>,
}

fn build_snapshot(client: Option<&str>) -> Result<InitSnapshot> {
    if let Some(target) = client
        && McpClient::from_flag(target).is_none()
    {
        return Err(anyhow!(
            "Unknown client '{target}'. Known: claude-code, cursor, windsurf, cline, continue, claude-desktop, codex, gemini, copilot"
        ));
    }

    let detected_clients = clients::detect_clients()
        .into_iter()
        .filter(|client| *client != McpClient::ClaudeCode)
        .map(|client| client.name().to_string())
        .collect();

    let hyphae_installed = discover(Tool::Hyphae).is_some();
    let rhizome_installed = discover(Tool::Rhizome).is_some();
    let hyphae_db_exists = dirs::data_dir()
        .map(|dir| dir.join("hyphae").join("hyphae.db"))
        .is_some_and(|db_path| db_path.exists());

    Ok(InitSnapshot {
        target_client: client.map(ToOwned::to_owned),
        detected_clients,
        hyphae_installed,
        rhizome_installed,
        hyphae_db_exists,
    })
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

    if let Some(client) = &snapshot.target_client {
        steps.push(InitStep {
            status: InitStepStatus::Planned,
            title: format!("target {client}"),
            detail: "Use the selected MCP client for registration.".to_string(),
        });
    } else if snapshot.detected_clients.is_empty() {
        steps.push(InitStep {
            status: InitStepStatus::Skipped,
            title: "configure MCP clients".to_string(),
            detail: "No supported MCP clients were detected on this machine.".to_string(),
        });
    } else {
        steps.push(InitStep {
            status: InitStepStatus::Planned,
            title: "configure detected MCP clients".to_string(),
            detail: format!("Detected: {}", snapshot.detected_clients.join(", ")),
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

    steps.push(InitStep {
        status: InitStepStatus::Planned,
        title: "patch CLAUDE.md with ecosystem instructions".to_string(),
        detail: "Keep the local Claude Code instructions aligned with the installed ecosystem."
            .to_string(),
    });

    steps
}

fn build_repair_actions(snapshot: &InitSnapshot) -> Vec<RepairAction> {
    let mut actions = Vec::new();

    if !snapshot.hyphae_installed || !snapshot.rhizome_installed {
        actions.push(RepairAction::stipe(
            "install-claude-code",
            "Install the Claude Code profile",
            "Install the core local agent stack before wiring MCP clients.",
            &["install", "--profile", "claude-code"],
            RepairTier::Primary,
        ));
    }

    if !snapshot.hyphae_db_exists
        || snapshot.target_client.is_some()
        || !snapshot.detected_clients.is_empty()
    {
        actions.push(RepairAction::stipe(
            "init",
            "Initialize the ecosystem",
            "Apply MCP registrations, hook wiring, and Hyphae bootstrap work.",
            &["init"],
            RepairTier::Primary,
        ));
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
            detected_clients: vec!["Cursor".to_string(), "Continue".to_string()],
            hyphae_installed: true,
            rhizome_installed: false,
            hyphae_db_exists: false,
        };

        let lines = render_preview(&snapshot);
        assert!(lines.iter().any(|line| line.contains("target cursor")));
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
            detected_clients: vec!["Cursor".to_string(), "Continue".to_string()],
            hyphae_installed: false,
            rhizome_installed: false,
            hyphae_db_exists: true,
        };

        let lines = render_preview(&snapshot);
        assert!(
            lines
                .iter()
                .any(|line| line.contains("configure detected MCP clients"))
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
            detected_clients: vec!["Cursor".to_string()],
            hyphae_installed: false,
            rhizome_installed: true,
            hyphae_db_exists: false,
        };

        let plan = build_plan(&snapshot, true);
        let commands = plan
            .repair_actions
            .iter()
            .map(|action| action.command.as_str())
            .collect::<Vec<_>>();

        assert!(commands.contains(&"stipe install --profile claude-code"));
        assert!(commands.contains(&"stipe init"));
        assert!(commands.contains(&"cargo install hyphae"));
    }
}
