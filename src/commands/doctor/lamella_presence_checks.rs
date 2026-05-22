
//! Doctor checks for lamella binary presence, lamella-skills availability,
//! installed plugin count, and ecosystem plugin status.

use std::path::PathBuf;

use super::model::HealthCheck;
use crate::commands::repair::{RepairAction, RepairTier};
use crate::ecosystem::lamella::find_lamella;

fn lamella_skills_present(lamella_path: Option<&PathBuf>) -> bool {
    if let Ok(val) = std::env::var("LAMELLA_CONTENT_ROOT") {
        return std::path::Path::new(&val).join("skills").exists();
    }
    // Walk up from the lamella binary: binary → lamella-repo/ → parent → lamella-skills/
    if let Some(path) = lamella_path {
        if let Some(repo_dir) = path.parent() {
            let sibling = repo_dir.parent().map(|p| p.join("lamella-skills"));
            if let Some(sibling) = sibling {
                return sibling.join("skills").exists();
            }
        }
    }
    false
}

fn installed_plugin_count() -> usize {
    dirs::home_dir()
        .map(|home| home.join(".claude/plugins/lamella"))
        .filter(|dir| dir.exists())
        .and_then(|dir| std::fs::read_dir(&dir).ok())
        .map_or(0, |entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().is_dir())
                .count()
        })
}

fn ecosystem_installed() -> bool {
    dirs::home_dir()
        .map(|home| home.join(".claude/plugins/lamella/ecosystem"))
        .is_some_and(|p| p.exists())
}

/// Check lamella presence: binary on PATH, lamella-skills available,
/// plugin count, and ecosystem plugin status.
pub(super) fn check_lamella_presence() -> HealthCheck {
    let lamella_path = find_lamella();
    let binary_present = lamella_path.is_some();
    let skills_present = lamella_skills_present(lamella_path.as_ref());
    let plugin_count = installed_plugin_count();
    let ecosystem_ok = ecosystem_installed();

    let passed = binary_present && ecosystem_ok;

    let message = if !binary_present {
        "lamella not found — locate or install lamella, then run 'stipe plugins install --ecosystem'".to_string()
    } else if !skills_present {
        "lamella found but lamella-skills not present; set LAMELLA_CONTENT_ROOT or clone lamella-skills as a sibling of the lamella repo".to_string()
    } else if !ecosystem_ok {
        format!(
            "lamella present, {plugin_count} plugin(s) installed — run 'stipe plugins install --ecosystem' to install the ecosystem plugin set"
        )
    } else {
        format!("lamella present, {plugin_count} plugin(s) installed, ecosystem plugin active")
    };

    HealthCheck {
        name: "lamella plugins".to_string(),
        passed,
        message,
        repair_actions: if passed {
            Vec::new()
        } else {
            vec![RepairAction::stipe(
                "install-ecosystem-plugin",
                "Install ecosystem plugin",
                "Run 'stipe plugins install --ecosystem' to install the curated plugin set.",
                &["plugins", "install", "--ecosystem"],
                RepairTier::Secondary,
            )]
        },
    }
}
