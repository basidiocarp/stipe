use super::model::{InitPlan, InitSnapshot, InitStep, InitStepStatus};
use crate::commands::claude_hooks;
use crate::commands::codex_notify;
use crate::commands::host_policy;
use crate::commands::repair::{RepairAction, RepairTier, dedupe_repair_actions};

fn selected_mode_label(snapshot: &InitSnapshot) -> String {
    if !snapshot.selected_hosts.is_empty() {
        snapshot
            .selected_hosts
            .iter()
            .map(|mode| mode.label().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    } else if snapshot.target_is_codex() {
        "Codex host mode".to_string()
    } else {
        "detected host inventory".to_string()
    }
}

fn host_inventory_step(snapshot: &InitSnapshot) -> InitStep {
    if !snapshot.selected_hosts.is_empty() {
        InitStep {
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
        }
    } else if let Some(client) = &snapshot.target_client {
        InitStep {
            status: InitStepStatus::Planned,
            title: format!("target {client}"),
            detail: "Use the selected host inventory for registration.".to_string(),
        }
    } else if snapshot.detected_clients.is_empty() {
        InitStep {
            status: InitStepStatus::Skipped,
            title: "configure host inventory".to_string(),
            detail: "No supported host inventory was detected on this machine.".to_string(),
        }
    } else {
        InitStep {
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
        }
    }
}

fn mcp_registration_step(installed: bool, broken: bool, title: &str, tool_name: &str) -> InitStep {
    InitStep {
        status: if installed {
            InitStepStatus::Planned
        } else {
            InitStepStatus::Skipped
        },
        title: title.to_string(),
        detail: if installed {
            format!("{tool_name} is installed and can be wired into supported clients.")
        } else if broken {
            format!(
                "{tool_name} is installed but broken, so MCP registration is skipped. Run 'stipe install {}' first.",
                tool_name.to_ascii_lowercase()
            )
        } else {
            format!("{tool_name} is not installed yet.")
        },
    }
}

fn hyphae_database_step(snapshot: &InitSnapshot) -> InitStep {
    InitStep {
        status: if snapshot.tools.hyphae_db_exists {
            InitStepStatus::AlreadyOk
        } else {
            InitStepStatus::Planned
        },
        title: "initialize the Hyphae database".to_string(),
        detail: if snapshot.tools.hyphae_db_exists {
            "The Hyphae database already exists.".to_string()
        } else {
            "Hyphae will create its local database on first access.".to_string()
        },
    }
}

fn codex_notify_step(snapshot: &InitSnapshot) -> Option<InitStep> {
    snapshot
        .codex_host_selected_or_detected()
        .then(|| InitStep {
            status: if snapshot.codex.notify_configured {
                InitStepStatus::AlreadyOk
            } else {
                InitStepStatus::Planned
            },
            title: "configure the Codex notify adapter".to_string(),
            detail: codex_notify::codex_notify_detail(snapshot.codex.notify_configured),
        })
}

fn claude_hooks_step(snapshot: &InitSnapshot) -> Option<InitStep> {
    snapshot
        .claude_host_selected_or_detected()
        .then(|| InitStep {
            status: if snapshot.tools.cortina_broken || !snapshot.tools.cortina_installed {
                InitStepStatus::Skipped
            } else if snapshot.claude.hooks_configured {
                InitStepStatus::AlreadyOk
            } else {
                InitStepStatus::Planned
            },
            title: "install the Cortina Claude hooks".to_string(),
            detail: if snapshot.tools.cortina_broken {
                "Cortina is installed but broken, so Claude hook registration is skipped. Run 'stipe install cortina' first.".to_string()
            } else if snapshot.tools.cortina_installed {
                claude_hooks::claude_hooks_detail(snapshot.claude.hooks_configured)
            } else {
                "Cortina is not installed yet, so Claude hook registration is skipped.".to_string()
            },
        })
}

fn build_steps(snapshot: &InitSnapshot) -> Vec<InitStep> {
    let mut steps = vec![
        host_inventory_step(snapshot),
        mcp_registration_step(
            snapshot.tools.hyphae_installed,
            snapshot.tools.hyphae_broken,
            "register the hyphae MCP server",
            "Hyphae",
        ),
        mcp_registration_step(
            snapshot.tools.rhizome_installed,
            snapshot.tools.rhizome_broken,
            "register the rhizome MCP server",
            "Rhizome",
        ),
        hyphae_database_step(snapshot),
    ];

    if let Some(step) = codex_notify_step(snapshot) {
        steps.push(step);
    }
    if let Some(step) = claude_hooks_step(snapshot) {
        steps.push(step);
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

    if (!snapshot.tools.hyphae_installed || !snapshot.tools.rhizome_installed)
        && snapshot.selected_hosts.is_empty()
    {
        actions.push(host_policy::install_profile_repair_action(install_profile));
    }

    if !snapshot.tools.hyphae_db_exists
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

    if snapshot.codex_host_selected_or_detected() && !snapshot.codex.notify_configured {
        actions.push(codex_notify::codex_notify_repair_action());
    }

    if snapshot.claude_host_selected_or_detected() && snapshot.tools.cortina_broken {
        actions.push(RepairAction::stipe(
            "install-cortina",
            "Repair Cortina",
            "Reinstall Cortina before attempting Claude hook registration.",
            &["install", "cortina"],
            RepairTier::Primary,
        ));
    }

    if !snapshot.tools.hyphae_installed {
        actions.push(RepairAction::stipe(
            "install-hyphae",
            "Install Hyphae",
            "Install Hyphae through the managed stipe release path.",
            &["install", "hyphae"],
            RepairTier::Manual,
        ));
    }

    if !snapshot.tools.rhizome_installed {
        actions.push(RepairAction::stipe(
            "install-rhizome",
            "Install Rhizome",
            "Install Rhizome through the managed stipe release path.",
            &["install", "rhizome"],
            RepairTier::Manual,
        ));
    }

    dedupe_repair_actions(actions)
}

pub(super) fn build_plan(snapshot: &InitSnapshot, dry_run: bool) -> InitPlan {
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
