use anyhow::Result;

use crate::install_state;
use crate::install_state::{ItemStatus, check_item_status};

#[derive(Debug, Clone, Copy)]
enum RepairScope {
    All,
    Hooks,
    Skills,
    Config,
}

impl RepairScope {
    fn matches(self, kind: &str) -> bool {
        match self {
            RepairScope::All => true,
            RepairScope::Hooks => kind == "hook",
            RepairScope::Skills => kind == "skill",
            RepairScope::Config => kind == "config",
        }
    }
}

pub fn run(dry_run: bool, scope: Option<&str>, force: bool) -> Result<()> {
    let scope = match scope {
        Some("hooks") => RepairScope::Hooks,
        Some("skills") => RepairScope::Skills,
        Some("config") => RepairScope::Config,
        _ => RepairScope::All,
    };

    let conn = install_state::open()?;
    let items = install_state::list_all(&conn)?;

    if items.is_empty() {
        println!("No installed items recorded.");
        return Ok(());
    }

    let mut preview_count = 0;
    let mut skipped_drift_count = 0;
    // Tracks items that need repair but have no implementation yet.
    let mut unhandled_count = 0;

    for item in &items {
        // Skip if not in scope
        if !scope.matches(&item.kind) {
            continue;
        }

        let status = check_item_status(item);

        match status {
            ItemStatus::Missing => {
                if dry_run {
                    println!(
                        "Would re-install [MISSING] {} (source: {:?})",
                        item.id, item.source
                    );
                    preview_count += 1;
                } else {
                    // Re-install logic is not yet implemented; report honestly rather
                    // than claiming work was done when nothing happened.
                    println!(
                        "MISSING `{}`: repair not yet implemented (use --dry-run to preview)",
                        item.id
                    );
                    unhandled_count += 1;
                }
            }
            ItemStatus::Drift => {
                if force {
                    if dry_run {
                        println!("Would repair [DRIFT] {} (with --force)", item.id);
                        preview_count += 1;
                    } else {
                        // Repair logic is not yet implemented; report honestly.
                        println!(
                            "DRIFT `{}`: repair not yet implemented (use --dry-run to preview)",
                            item.id
                        );
                        unhandled_count += 1;
                    }
                } else {
                    println!("Skipping [DRIFT] {} (use --force to repair)", item.id);
                    skipped_drift_count += 1;
                }
            }
            ItemStatus::Ok | ItemStatus::Unknown => {
                // Nothing to repair
            }
        }
    }

    if dry_run {
        println!(
            "\nDry run: {preview_count} items would need repair, {skipped_drift_count} drift items skipped"
        );
    } else if unhandled_count > 0 {
        // Do not claim success when items were not actually repaired.
        println!(
            "\n{unhandled_count} item(s) need repair but have no automated fix yet. \
             Re-install affected tools manually, then re-run `stipe doctor` to verify."
        );
        anyhow::bail!("{unhandled_count} item(s) could not be repaired");
    } else {
        println!("\nRepair complete: {skipped_drift_count} drift items skipped");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_scope_matches() {
        assert!(RepairScope::All.matches("hook"));
        assert!(RepairScope::All.matches("skill"));
        assert!(RepairScope::Hooks.matches("hook"));
        assert!(!RepairScope::Hooks.matches("skill"));
        assert!(RepairScope::Skills.matches("skill"));
        assert!(!RepairScope::Skills.matches("hook"));
    }

    #[test]
    fn test_item_status_missing() {
        let item = install_state::InstalledItem {
            id: "test".to_string(),
            kind: "hook".to_string(),
            path: Some("/nonexistent/path".to_string()),
            version: Some("1.0.0".to_string()),
            installed_at: 0,
            updated_at: 0,
            source: None,
            checksum: None,
        };

        assert!(matches!(check_item_status(&item), ItemStatus::Missing));
    }

    #[test]
    fn test_item_status_ok_when_path_exists_no_checksum() -> Result<()> {
        let tmpdir = TempDir::new()?;
        let file_path = tmpdir.path().join("test_file");
        std::fs::write(&file_path, b"test content")?;

        let item = install_state::InstalledItem {
            id: "test".to_string(),
            kind: "hook".to_string(),
            path: Some(file_path.to_string_lossy().to_string()),
            version: Some("1.0.0".to_string()),
            installed_at: 0,
            updated_at: 0,
            source: None,
            checksum: None,
        };

        assert!(matches!(check_item_status(&item), ItemStatus::Ok));

        Ok(())
    }
}
