use std::fs;
use std::path::{Path, PathBuf};

use crate::commands::host_policy;
use crate::commands::install::{SavedInstallProfile, expected_profile_tools, manual_member};
use crate::commands::package_repair;
use crate::commands::repair::{RepairAction, RepairTier};
use crate::commands::tool_registry;

use super::model::{HealthCheck, PackageDrift, PackageInventory, WorktreeConfigDiscovery};

pub(super) fn collect_package_inventory() -> PackageInventory {
    let project_root = host_policy::project_root();
    let metadata_sources = project_root
        .as_deref()
        .map_or_else(Vec::new, collect_lamella_metadata_sources);
    let discovered_packages = project_root
        .as_deref()
        .map_or_else(Vec::new, collect_lamella_manifest_packages);

    PackageInventory {
        package_metadata_available: !metadata_sources.is_empty(),
        metadata_sources,
        discovered_packages,
        discovered_plugins: collect_discovered_plugins(project_root.as_deref()),
    }
}

pub(super) fn collect_worktree_config_discovery() -> WorktreeConfigDiscovery {
    let project_root = host_policy::project_root();
    let discovered_configs = project_root
        .as_deref()
        .map_or_else(Vec::new, discover_worktree_configs);

    WorktreeConfigDiscovery {
        detected: project_root.is_some(),
        project_root,
        discovered_configs,
    }
}

pub(super) fn collect_package_drift(
    saved_profile: Option<&SavedInstallProfile>,
) -> (PackageDrift, HealthCheck) {
    let Some(saved_profile) = saved_profile else {
        return (
            PackageDrift {
                metadata_available: false,
                expected_packages: Vec::new(),
                installed_packages: Vec::new(),
                missing_packages: Vec::new(),
            },
            HealthCheck {
                name: "package drift".to_string(),
                passed: true,
                message: "No saved install profile found; skipping package drift checks."
                    .to_string(),
                repair_actions: Vec::new(),
                suppressed: false,
            },
        );
    };

    let expected_packages = expected_profile_tools(saved_profile.profile);
    let installed_packages = expected_packages
        .iter()
        .filter(|package| package_installed(package))
        .cloned()
        .collect::<Vec<_>>();
    let missing_packages = expected_packages
        .iter()
        .filter(|package| !installed_packages.contains(*package))
        .cloned()
        .collect::<Vec<_>>();
    let missing_count = missing_packages.len();

    let repair_actions =
        if missing_count > 0 && package_repair::supports_profile(saved_profile.profile) {
            vec![RepairAction::stipe(
                "package-repair",
                "Repair packaged skills and plugins",
                "Run Lamella package install with backup and rollback targets managed by Stipe.",
                &["package", "--profile", saved_profile.profile.profile_name()],
                RepairTier::Primary,
            )]
        } else {
            Vec::new()
        };

    (
        PackageDrift {
            metadata_available: true,
            expected_packages,
            installed_packages,
            missing_packages,
        },
        HealthCheck {
            name: "package drift".to_string(),
            passed: missing_count == 0,
            message: if missing_count == 0 {
                "Installed packages match saved profile metadata.".to_string()
            } else if repair_actions.is_empty() {
                format!(
                    "{missing_count} expected packages are missing for saved profile; no automated package repair surface is defined"
                )
            } else {
                format!("{missing_count} expected packages are missing for saved profile")
            },
            repair_actions,
            suppressed: false,
        },
    )
}

fn collect_lamella_metadata_sources(project_root: &Path) -> Vec<PathBuf> {
    lamella_roots(project_root)
        .into_iter()
        .flat_map(|root| [root.join("manifests"), root.join("resources")])
        .filter(|path| path.exists())
        .collect()
}

fn collect_lamella_manifest_packages(project_root: &Path) -> Vec<String> {
    let Some(manifests_root) = lamella_roots(project_root)
        .into_iter()
        .map(|root| root.join("manifests"))
        .find(|path| path.exists())
    else {
        return Vec::new();
    };

    let mut packages = Vec::new();
    let Ok(host_dirs) = fs::read_dir(manifests_root) else {
        return Vec::new();
    };

    for host_dir in host_dirs.filter_map(Result::ok) {
        let host_name = host_dir.file_name().to_string_lossy().to_string();
        let Ok(entries) = fs::read_dir(host_dir.path()) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let is_yaml = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension == "yaml" || extension == "yml");
            if !is_yaml {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
                packages.push(format!("{host_name}:{stem}"));
            }
        }
    }

    packages.sort();
    packages
}

fn lamella_roots(project_root: &Path) -> Vec<PathBuf> {
    let mut roots = vec![project_root.join("lamella")];
    if let Some(parent) = project_root.parent() {
        let sibling = parent.join("lamella");
        if !roots.iter().any(|existing| existing == &sibling) {
            roots.push(sibling);
        }
    }
    roots
}

fn collect_discovered_plugins(project_root: Option<&Path>) -> Vec<String> {
    let mut plugin_roots = Vec::new();
    if let Some(home) = dirs::home_dir() {
        plugin_roots.push(home.join(".codex").join("plugins"));
        plugin_roots.push(home.join(".claude").join("plugins"));
    }
    if let Some(root) = project_root {
        plugin_roots.push(root.join(".codex").join("plugins"));
    }

    let mut discovered = Vec::new();
    for root in plugin_roots {
        if !root.exists() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                discovered.push(format!("{}:{name}", host_policy::format_user_path(&root)));
            }
        }
    }

    discovered.sort();
    discovered
}

fn discover_worktree_configs(project_root: &Path) -> Vec<PathBuf> {
    let candidates = [
        project_root.join(".mcp.json"),
        project_root.join(".claude").join("settings.json"),
        project_root.join(".claude").join("settings.local.json"),
        project_root.join(".codex").join("config.toml"),
        project_root.join(".git").join("config"),
    ];

    candidates
        .into_iter()
        .filter(|path| path.exists())
        .collect()
}

fn package_installed(name: &str) -> bool {
    if let Some(member) = manual_member(name) {
        return manual_package_installed(member.name);
    }

    tool_registry::find(name)
        .map(tool_registry::probe)
        .is_some_and(|probe| probe.is_repairable_presence())
}

fn manual_package_installed(name: &str) -> bool {
    candidate_workspace_roots().iter().any(|root| match name {
        "lamella" => lamella_root_installed(root) || lamella_root_installed(&root.join("lamella")),
        "cap" => cap_root_installed(root) || cap_root_installed(&root.join("cap")),
        _ => false,
    })
}

fn candidate_workspace_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        let project_root = spore::paths::find_project_root(&cwd).unwrap_or(cwd.clone());
        if !roots.iter().any(|existing| existing == &project_root) {
            roots.push(project_root.clone());
        }
        if let Some(parent) = project_root.parent().map(Path::to_path_buf)
            && !roots.iter().any(|existing| existing == &parent)
        {
            roots.push(parent);
        }
    }
    if let Some(home) = dirs::home_dir().map(|home| home.join("projects").join("basidiocarp"))
        && !roots.iter().any(|existing| existing == &home)
    {
        roots.push(home);
    }
    roots
}

fn lamella_root_installed(path: &Path) -> bool {
    path.join("lamella").exists() && path.join("resources").exists()
}

fn cap_root_installed(path: &Path) -> bool {
    path.join("package.json").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lamella_roots_include_workspace_sibling() {
        let roots = lamella_roots(Path::new("/tmp/basidiocarp/stipe"));

        assert_eq!(
            roots,
            vec![
                PathBuf::from("/tmp/basidiocarp/stipe/lamella"),
                PathBuf::from("/tmp/basidiocarp/lamella"),
            ]
        );
    }
}
