use super::model::{HealthCheck, PackageInventory, WorktreeConfigDiscovery};
use crate::commands::install::SavedInstallProfile;
use crate::commands::repair::{RepairAction, RepairTier};
use crate::commands::tool_registry::{self, ToolProbe};

pub(super) fn check_task_linked_council(
    saved_profile: Option<&SavedInstallProfile>,
    package_inventory: &PackageInventory,
    worktree_config: &WorktreeConfigDiscovery,
) -> HealthCheck {
    let has_worktree_context = worktree_config.detected && worktree_config.project_root.is_some();
    let hyphae_ready = tool_ready("hyphae");
    let canopy_ready = tool_ready("canopy");
    let council_packages_present = package_inventory
        .discovered_packages
        .iter()
        .any(|package| package.contains("council"));

    let mut missing = Vec::new();
    if !has_worktree_context {
        missing.push("worktree config");
    }
    if !hyphae_ready {
        missing.push("hyphae retrieval");
    }
    if !canopy_ready {
        missing.push("canopy runtime");
    }
    if !package_inventory.package_metadata_available {
        missing.push("Lamella package metadata");
    } else if !council_packages_present {
        missing.push("council role bundles");
    }

    let passed = missing.is_empty();
    HealthCheck {
        name: "task-linked council".to_string(),
        passed,
        message: if passed {
            "Task-linked council summon prerequisites look ready.".to_string()
        } else {
            format!("Missing {}", missing.join(", "))
        },
        repair_actions: if passed {
            Vec::new()
        } else {
            repair_actions(saved_profile, has_worktree_context, hyphae_ready, canopy_ready)
        },
    }
}

fn tool_ready(name: &str) -> bool {
    tool_registry::find(name)
        .is_some_and(|spec| matches!(tool_registry::probe(spec), ToolProbe::Installed(_)))
}

fn repair_actions(
    saved_profile: Option<&SavedInstallProfile>,
    has_worktree_context: bool,
    hyphae_ready: bool,
    canopy_ready: bool,
) -> Vec<RepairAction> {
    let mut actions = Vec::new();

    if !has_worktree_context {
        actions.push(RepairAction::stipe(
            "repair-init",
            "Repair shared workspace config",
            "Reapply workspace-scoped config from the project root before using task-linked council summon.",
            &["init", "--repair"],
            RepairTier::Primary,
        ));
    }
    if !hyphae_ready {
        actions.push(RepairAction::stipe(
            "install-hyphae",
            "Install Hyphae",
            "Install the retrieval surface used for council artifact storage and lookup.",
            &["install", "hyphae"],
            RepairTier::Primary,
        ));
    }
    if !canopy_ready {
        actions.push(RepairAction::stipe(
            "install-canopy",
            "Install Canopy",
            "Install the task runtime used to attach council sessions to tracked work.",
            &["install", "canopy"],
            RepairTier::Primary,
        ));
    }

    match saved_profile {
        Some(saved_profile) => actions.push(RepairAction::stipe(
            "package-council",
            "Repair packaged council bundles",
            "Refresh Lamella-packaged role bundles for the saved install profile.",
            &["package", "--profile", saved_profile.profile.profile_name()],
            RepairTier::Secondary,
        )),
        None => actions.push(RepairAction::stipe(
            "package-council",
            "Repair packaged council bundles",
            "Refresh Lamella-packaged role bundles so council summon can discover packaged roles.",
            &["package"],
            RepairTier::Secondary,
        )),
    }

    actions
}
