//! Plugin and hook inventory checks.
//!
//! Lists installed lamella skills, hooks, and commands.  For each:
//! - Path validity: `valid` / `stale` / `missing`
//! - Installed version vs the pin in `ecosystem-versions.toml`
//!
//! When `annulus validate-hooks` is available its exit code is consumed.
//! Otherwise path stat checks are used as the fallback.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::model::{PluginInventory, PluginInventoryItem, PluginPathStatus, VersionDriftStatus};

// ---------------------------------------------------------------------------
// Ecosystem version pins
// ---------------------------------------------------------------------------

/// Pinned tool versions from `ecosystem-versions.toml` (the `[tools]` table).
///
/// We embed the versions at compile time from the source-of-truth file.
/// This avoids runtime file I/O for a file that lives outside the stipe repo
/// boundary and may not be present on end-user machines.  When this crate is
/// built from the workspace the values are current.  When it is installed as
/// a release binary the values reflect the ecosystem state at release time,
/// which is still the correct reference for version drift.
///
/// These versions must stay synchronized with ecosystem-versions.toml [tools] table.
/// During `stipe doctor`, version drift for installed binaries is checked against these pins.
fn pinned_tool_versions() -> HashMap<&'static str, &'static str> {
    let mut pins = HashMap::new();
    pins.insert("mycelium", "0.11.0");
    pins.insert("hyphae", "0.14.0");
    pins.insert("rhizome", "0.8.0");
    pins.insert("canopy", "0.8.0");
    pins.insert("cortina", "0.5.0");
    pins.insert("stipe", "0.8.1");
    pins.insert("volva", "0.3.1");
    pins.insert("hymenium", "0.8.0");
    pins.insert("annulus", "0.7.0");
    pins.insert("cap", "0.13.0");
    pins.insert("lamella", "0.5.15");
    pins.insert("spore", "0.6.0");
    pins
}

// ---------------------------------------------------------------------------
// Annulus hook validation
// ---------------------------------------------------------------------------

/// Probe whether `annulus validate-hooks` is available and run it.
///
/// Returns `(used, items)` where `used` is `true` when the annulus path ran
/// successfully.  When annulus is absent the caller falls back to stat checks.
fn try_annulus_validate_hooks() -> (bool, Vec<PluginInventoryItem>) {
    let Ok(annulus_path) = which::which("annulus") else {
        return (false, Vec::new());
    };

    let output = Command::new(&annulus_path)
        .args(["validate-hooks", "--json"])
        .output();

    let Ok(output) = output else {
        return (false, Vec::new());
    };

    if !output.status.success() {
        // annulus is available but returned a non-zero exit code — treat hooks as stale.
        // stderr is intentionally not surfaced in output to avoid leaking internal details.
        return (
            true,
            vec![PluginInventoryItem {
                name: "hooks".to_string(),
                category: "hook".to_string(),
                path_status: PluginPathStatus::Stale,
                installed_version: None,
                version_drift: VersionDriftStatus::Unknown,
                pinned_version: None,
            }],
        );
    }

    // annulus returned 0 — all hooks valid.
    (
        true,
        vec![PluginInventoryItem {
            name: "hooks".to_string(),
            category: "hook".to_string(),
            path_status: PluginPathStatus::Valid,
            installed_version: None,
            version_drift: VersionDriftStatus::Unknown,
            pinned_version: None,
        }],
    )
}

// ---------------------------------------------------------------------------
// Lamella skill discovery
// ---------------------------------------------------------------------------

/// Candidate roots where lamella resources live.
///
/// Returns roots in priority order.  When installed Claude plugins are found
/// at `~/.claude/plugins/lamella`, that root is returned alone so the source
/// tree is not also counted (which would double-report every skill).
fn lamella_resource_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(home) = std::env::var("LAMELLA_HOME") {
        let path = PathBuf::from(home);
        if path.exists() {
            roots.push(path);
        }
    }

    if let Some(home) = dirs::home_dir() {
        // Installed Claude plugins take priority: skills live here after
        // `lamella install` runs.
        let claude_plugins = home.join(".claude").join("plugins").join("lamella");
        if claude_plugins.exists() {
            if !roots.iter().any(|r| r == &claude_plugins) {
                roots.push(claude_plugins);
            }
            // Found installed plugins — do not also count the source tree.
            return roots;
        }

        for candidate in [
            home.join(".lamella"),
            home.join(".local").join("share").join("lamella"),
            home.join(".config").join("lamella"),
        ] {
            if candidate.exists() && !roots.iter().any(|r| r == &candidate) {
                roots.push(candidate);
            }
        }
    }

    // Workspace sibling fallback when nothing else is present.
    if let Some(home) = dirs::home_dir() {
        let workspace_lamella = home.join("projects").join("basidiocarp").join("lamella");
        if workspace_lamella.exists() && !roots.iter().any(|r| r == &workspace_lamella) {
            roots.push(workspace_lamella);
        }
    }

    roots
}

/// Walk `root` recursively and collect every `SKILL.md` as one skill entry.
///
/// Handles two layouts:
/// - **Installed plugins** (`~/.claude/plugins/lamella`): skills live at
///   `<plugin>/skills/<name>/SKILL.md`.
/// - **Source tree** (`lamella/`): skills live at
///   `resources/skills/<category>/<name>/SKILL.md`.
///
/// In both cases the skill name is the directory that directly contains
/// `SKILL.md`.
fn discover_lamella_skills(root: &Path) -> Vec<PluginInventoryItem> {
    // Iterative DFS to avoid recursion depth issues on deep trees.
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    let mut items: Vec<PluginInventoryItem> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|n| n.to_str()) == Some("SKILL.md") {
                // Skill name is the parent directory name.
                let Some(name) = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                else {
                    continue;
                };
                if seen.insert(name.to_string()) {
                    items.push(PluginInventoryItem {
                        name: name.to_string(),
                        category: "skill".to_string(),
                        path_status: PluginPathStatus::Valid,
                        installed_version: None,
                        version_drift: VersionDriftStatus::Unknown,
                        pinned_version: None,
                    });
                }
            }
        }
    }

    items.sort_by(|a, b| a.name.cmp(&b.name));
    items
}

// ---------------------------------------------------------------------------
// Hook path stat checks (fallback when annulus is absent)
// ---------------------------------------------------------------------------

fn discover_hooks_via_stat() -> Vec<PluginInventoryItem> {
    let mut items = Vec::new();

    // Check cortina, which provides the primary hooks.
    let cortina_path = which::which("cortina").ok();
    items.push(PluginInventoryItem {
        name: "cortina-hooks".to_string(),
        category: "hook".to_string(),
        path_status: match cortina_path {
            Some(ref p) if p.exists() => PluginPathStatus::Valid,
            Some(_) => PluginPathStatus::Stale,
            None => PluginPathStatus::Missing,
        },
        installed_version: cortina_path
            .as_deref()
            .and_then(installed_version_for_binary),
        version_drift: VersionDriftStatus::Unknown,
        pinned_version: None,
    });

    // Check annulus hooks binary.
    let annulus_path = which::which("annulus").ok();
    items.push(PluginInventoryItem {
        name: "annulus-hooks".to_string(),
        category: "hook".to_string(),
        path_status: match annulus_path {
            Some(ref p) if p.exists() => PluginPathStatus::Valid,
            Some(_) => PluginPathStatus::Stale,
            None => PluginPathStatus::Missing,
        },
        installed_version: annulus_path
            .as_deref()
            .and_then(installed_version_for_binary),
        version_drift: VersionDriftStatus::Unknown,
        pinned_version: None,
    });

    items
}

/// Run `<binary> --version` and return the version string.
fn installed_version_for_binary(path: &Path) -> Option<String> {
    let output = Command::new(path).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().last())
        .filter(|v| v.contains('.'))
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// Version drift resolution
// ---------------------------------------------------------------------------

fn resolve_version_drift(
    name: &str,
    installed: Option<&str>,
    pins: &HashMap<&str, &str>,
) -> (VersionDriftStatus, Option<String>) {
    let Some(&pinned) = pins.get(name) else {
        return (VersionDriftStatus::Unknown, None);
    };

    let Some(installed_ver) = installed else {
        return (VersionDriftStatus::Unknown, Some(pinned.to_string()));
    };

    if installed_ver == pinned {
        (VersionDriftStatus::UpToDate, Some(pinned.to_string()))
    } else {
        (VersionDriftStatus::Behind, Some(pinned.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Top-level collection
// ---------------------------------------------------------------------------

/// Collect the full plugin and hook inventory.
#[must_use]
pub(super) fn collect_plugin_inventory() -> PluginInventory {
    let pins = pinned_tool_versions();

    // Try annulus first; fall back to direct stat checks.
    let (annulus_used, annulus_items) = try_annulus_validate_hooks();
    let mut items: Vec<PluginInventoryItem> = if annulus_used {
        annulus_items
    } else {
        discover_hooks_via_stat()
    };

    // Add tool entries for tracked ecosystem binaries with version drift.
    let ecosystem_tools = [
        "cortina", "annulus", "mycelium", "hyphae", "rhizome", "canopy", "volva", "hymenium",
    ];
    for name in &ecosystem_tools {
        let path = which::which(name).ok();
        let installed_version = path.as_deref().and_then(installed_version_for_binary);
        let (drift, pinned_version) =
            resolve_version_drift(name, installed_version.as_deref(), &pins);
        let path_status = match path {
            Some(ref p) if p.exists() => PluginPathStatus::Valid,
            Some(_) => PluginPathStatus::Stale,
            None => PluginPathStatus::Missing,
        };

        items.push(PluginInventoryItem {
            name: (*name).to_string(),
            category: "command".to_string(),
            path_status,
            installed_version,
            version_drift: drift,
            pinned_version,
        });
    }

    // Add discovered lamella skills.
    for root in lamella_resource_roots() {
        let skills = discover_lamella_skills(&root);
        items.extend(skills);
    }

    let skills_count = items.iter().filter(|i| i.category == "skill").count();
    let hooks_count = items.iter().filter(|i| i.category == "hook").count();
    let stale_count = items
        .iter()
        .filter(|i| i.path_status == PluginPathStatus::Stale)
        .count();
    let missing_count = items
        .iter()
        .filter(|i| i.path_status == PluginPathStatus::Missing)
        .count();

    PluginInventory {
        annulus_validator_used: annulus_used,
        items,
        skills_count,
        hooks_count,
        stale_count,
        missing_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_plugin_inventory_returns_without_panicking() {
        // Just exercises the happy path; we do not assert exact counts because
        // the test machine may or may not have tools installed.
        let inventory = collect_plugin_inventory();
        // skills + hooks + commands are always non-negative.
        assert!(inventory.stale_count <= inventory.items.len());
        assert!(inventory.missing_count <= inventory.items.len());
    }

    #[test]
    fn pinned_tool_versions_table_is_non_empty() {
        let pins = pinned_tool_versions();
        assert!(pins.contains_key("cortina"));
        assert!(pins.contains_key("annulus"));
        assert!(pins.contains_key("hyphae"));
    }

    #[test]
    fn version_drift_up_to_date_when_versions_match() {
        let mut pins = HashMap::new();
        pins.insert("cortina", "0.2.16");

        let (drift, pinned) = resolve_version_drift("cortina", Some("0.2.16"), &pins);
        assert_eq!(drift, VersionDriftStatus::UpToDate);
        assert_eq!(pinned.as_deref(), Some("0.2.16"));
    }

    #[test]
    fn version_drift_behind_when_versions_differ() {
        let mut pins = HashMap::new();
        pins.insert("cortina", "0.2.16");

        let (drift, _pinned) = resolve_version_drift("cortina", Some("0.2.15"), &pins);
        assert_eq!(drift, VersionDriftStatus::Behind);
    }

    #[test]
    fn version_drift_unknown_when_tool_not_in_pins() {
        let pins = HashMap::new();
        let (drift, pinned) = resolve_version_drift("unknown-tool", Some("1.0.0"), &pins);
        assert_eq!(drift, VersionDriftStatus::Unknown);
        assert!(pinned.is_none());
    }

    #[test]
    fn discover_lamella_skills_finds_skill_md_recursively() {
        use std::fs;
        let dir = std::env::temp_dir().join(format!("stipe-plugin-inv-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        // Simulate installed-plugins layout: <plugin>/skills/<name>/SKILL.md
        let skill_dir = dir.join("core-base").join("skills").join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# skill").unwrap();

        // A non-SKILL.md file should not be counted.
        fs::write(skill_dir.join("references.md"), "# refs").unwrap();

        let skills = discover_lamella_skills(&dir);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "my-skill");
        assert_eq!(skills[0].category, "skill");
        assert_eq!(skills[0].path_status, PluginPathStatus::Valid);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn discover_lamella_skills_deduplicates_by_name() {
        use std::fs;
        let dir =
            std::env::temp_dir().join(format!("stipe-plugin-inv-dedup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        // Two plugins each containing the same skill name.
        for plugin in ["plugin-a", "plugin-b"] {
            let skill_dir = dir.join(plugin).join("skills").join("shared-skill");
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(skill_dir.join("SKILL.md"), "# skill").unwrap();
        }

        let skills = discover_lamella_skills(&dir);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "shared-skill");

        let _ = fs::remove_dir_all(&dir);
    }
}
