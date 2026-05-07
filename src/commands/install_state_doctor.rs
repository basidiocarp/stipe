use anyhow::Result;

use crate::install_state;
use crate::install_state::{ItemStatus, check_item_status};

pub fn run() -> Result<()> {
    let conn = install_state::open()?;
    let items = install_state::list_all(&conn)?;

    if items.is_empty() {
        println!("No installed items recorded.");
        return Ok(());
    }

    let mut ok_count = 0;
    let mut missing_count = 0;
    let mut drift_count = 0;
    let mut unknown_count = 0;

    for item in &items {
        let status = check_item_status(item);
        let label = status.label();
        match status {
            ItemStatus::Ok => ok_count += 1,
            ItemStatus::Missing => missing_count += 1,
            ItemStatus::Drift => drift_count += 1,
            ItemStatus::Unknown => unknown_count += 1,
        }

        let path_display = item.path.as_deref().unwrap_or("(no path)");
        println!("[{label}] {} at {}", item.id, path_display);
    }

    let total = items.len();
    println!(
        "\nSummary: {total} items: {ok_count} OK, {missing_count} missing, {drift_count} drift, {unknown_count} unknown"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_item_status_unknown_when_no_path() {
        let item = install_state::InstalledItem {
            id: "test".to_string(),
            kind: "hook".to_string(),
            path: None,
            version: Some("1.0.0".to_string()),
            installed_at: 0,
            updated_at: 0,
            source: None,
            checksum: None,
        };

        assert!(matches!(check_item_status(&item), ItemStatus::Unknown));
    }

    #[test]
    fn test_item_status_missing_when_path_not_exists() {
        let item = install_state::InstalledItem {
            id: "test".to_string(),
            kind: "hook".to_string(),
            path: Some("/nonexistent/path/to/file".to_string()),
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

    #[test]
    fn test_item_status_ok_when_checksum_matches() -> Result<()> {
        let tmpdir = TempDir::new()?;
        let file_path = tmpdir.path().join("test_file");
        std::fs::write(&file_path, b"test content")?;

        let checksum = install_state::compute_checksum(&file_path)?;

        let item = install_state::InstalledItem {
            id: "test".to_string(),
            kind: "hook".to_string(),
            path: Some(file_path.to_string_lossy().to_string()),
            version: Some("1.0.0".to_string()),
            installed_at: 0,
            updated_at: 0,
            source: None,
            checksum: Some(checksum),
        };

        assert!(matches!(check_item_status(&item), ItemStatus::Ok));

        Ok(())
    }

    #[test]
    fn test_item_status_drift_when_checksum_mismatch() -> Result<()> {
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
            checksum: Some("wrongchecksum".to_string()),
        };

        assert!(matches!(check_item_status(&item), ItemStatus::Drift));

        Ok(())
    }
}
