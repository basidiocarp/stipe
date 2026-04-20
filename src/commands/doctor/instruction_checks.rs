use std::path::{Path, PathBuf};

use super::model::HealthCheck;

/// List of active subprojects that should have CLAUDE.md files at L2.
const ACTIVE_SUBPROJECTS: &[&str] = &[
    "canopy",
    "cap",
    "cortina",
    "annulus",
    "hyphae",
    "stipe",
    "mycelium",
    "lamella",
    "spore",
    "volva",
    "hymenium",
    "rhizome",
];

/// Find the workspace root (basidiocarp directory).
///
/// Tries the current directory and its parents first, then falls back to
/// ~/.projects/basidiocarp, and finally returns None if not found.
fn find_workspace_root() -> Option<PathBuf> {
    // Try current directory and parents
    if let Ok(cwd) = std::env::current_dir() {
        let project_root = spore::paths::find_project_root(&cwd).unwrap_or(cwd.clone());
        if project_root.file_name().map_or(false, |name| name == "basidiocarp") {
            return Some(project_root);
        }
        if let Some(parent) = project_root.parent() {
            if parent.file_name().map_or(false, |name| name == "basidiocarp") {
                return Some(parent.to_path_buf());
            }
        }
    }

    // Try default location
    dirs::home_dir().map(|home| home.join("projects").join("basidiocarp"))
}

/// Check that instruction files exist at expected ecosystem locations.
///
/// Returns a Vec of HealthCheck items (all warnings, not errors) that verify:
/// - L0: ~/.claude/rules/ directory exists
/// - L1: workspace root CLAUDE.md and AGENTS.md exist
/// - L2: project CLAUDE.md files exist for active subprojects
///
/// All findings are warnings since missing layers degrade guidance but do not break operation.
pub(super) fn check_instruction_files() -> Vec<HealthCheck> {
    let workspace_root = match find_workspace_root() {
        Some(root) => root,
        None => {
            // If we can't find the workspace root, return a warning check
            return vec![HealthCheck {
                name: "instruction file checks".to_string(),
                passed: false,
                message: "Could not find workspace root (basidiocarp); skipping instruction file checks"
                    .to_string(),
                repair_actions: Vec::new(),
            }];
        }
    };

    check_instruction_files_at_path(&workspace_root)
}

/// Check that instruction files exist at expected ecosystem locations (internal, with path provided).
///
/// Returns a Vec of HealthCheck items (all warnings, not errors) that verify:
/// - L0: ~/.claude/rules/ directory exists
/// - L1: workspace root CLAUDE.md and AGENTS.md exist
/// - L2: project CLAUDE.md files exist for active subprojects
///
/// All findings are warnings since missing layers degrade guidance but do not break operation.
fn check_instruction_files_at_path(workspace_root: &Path) -> Vec<HealthCheck> {
    let mut checks = Vec::new();

    // L0: Check global user rules directory
    if let Some(home) = dirs::home_dir() {
        let l0_path = home.join(".claude").join("rules");
        checks.push(HealthCheck {
            name: "L0: global user rules".to_string(),
            passed: l0_path.exists(),
            message: if l0_path.exists() {
                "Global user rules directory found at ~/.claude/rules/".to_string()
            } else {
                "Global user rules directory not found at ~/.claude/rules/".to_string()
            },
            repair_actions: Vec::new(),
        });
    }

    // L1: Check workspace root CLAUDE.md
    let l1_claude = workspace_root.join("CLAUDE.md");
    checks.push(HealthCheck {
        name: "L1: workspace root CLAUDE.md".to_string(),
        passed: l1_claude.exists(),
        message: if l1_claude.exists() {
            "Workspace root CLAUDE.md found".to_string()
        } else {
            "Workspace root CLAUDE.md not found".to_string()
        },
        repair_actions: Vec::new(),
    });

    // L1: Check workspace root AGENTS.md
    let l1_agents = workspace_root.join("AGENTS.md");
    checks.push(HealthCheck {
        name: "L1: workspace root AGENTS.md".to_string(),
        passed: l1_agents.exists(),
        message: if l1_agents.exists() {
            "Workspace root AGENTS.md found".to_string()
        } else {
            "Workspace root AGENTS.md not found".to_string()
        },
        repair_actions: Vec::new(),
    });

    // L2: Check project CLAUDE.md files for active subprojects
    for project in ACTIVE_SUBPROJECTS {
        let project_path = workspace_root.join(project);
        if project_path.exists() && project_path.is_dir() {
            let l2_path = project_path.join("CLAUDE.md");
            checks.push(HealthCheck {
                name: format!("L2: {}/CLAUDE.md", project),
                passed: l2_path.exists(),
                message: if l2_path.exists() {
                    format!("{}/CLAUDE.md found", project)
                } else {
                    format!("{}/CLAUDE.md not found", project)
                },
                repair_actions: Vec::new(),
            });
        }
    }

    checks
}
