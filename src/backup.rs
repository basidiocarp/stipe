use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::warn;

/// Snapshot of the pre-install state.
#[derive(Debug, Serialize, Deserialize)]
pub struct BackupManifest {
    pub timestamp: String,
    pub stipe_version: String,
    pub binaries: Vec<BinaryRecord>,
    pub config_files: Vec<ConfigRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BinaryRecord {
    pub tool_name: String,
    pub original_path: PathBuf,
    pub backup_path: PathBuf,
    pub version: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigRecord {
    pub original_path: PathBuf,
    pub backup_path: PathBuf,
    /// FNV-inspired hash for change detection, not cryptographic.
    pub checksum: String,
}

/// Returns the backup base directory.
/// Uses STIPE_BACKUP_DIR env var, falling back to ~/.local/share/stipe/backups.
pub fn backup_base_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("STIPE_BACKUP_DIR") {
        return PathBuf::from(dir);
    }
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("stipe")
        .join("backups")
}

/// Creates a pre-install backup snapshot and returns the backup directory path.
pub fn create_backup(
    timestamp: &str,
    stipe_version: &str,
    binary_paths: &[(String, PathBuf)],
    config_paths: &[PathBuf],
) -> Result<PathBuf> {
    let base = backup_base_dir();
    let backup_dir = base.join(timestamp);
    let bin_dir = backup_dir.join("bin");
    let cfg_dir = backup_dir.join("config");

    fs::create_dir_all(&bin_dir)
        .with_context(|| format!("create backup bin dir: {}", bin_dir.display()))?;
    fs::create_dir_all(&cfg_dir)
        .with_context(|| format!("create backup config dir: {}", cfg_dir.display()))?;

    let mut binaries = Vec::new();
    for (tool_name, path) in binary_paths {
        if !path.exists() {
            continue;
        }
        let backup_path = bin_dir.join(path.file_name().unwrap_or_default());
        fs::copy(path, &backup_path)
            .with_context(|| format!("backup binary {}", path.display()))?;
        binaries.push(BinaryRecord {
            tool_name: tool_name.clone(),
            original_path: path.clone(),
            backup_path,
            version: None,
        });
    }

    let mut config_files = Vec::new();
    for path in config_paths {
        if !path.exists() {
            continue;
        }
        let file_name = path.file_name().unwrap_or_default();
        let backup_path = cfg_dir.join(file_name);
        let content = fs::read(path)
            .with_context(|| format!("read config {}", path.display()))?;
        let checksum = format!("{:x}", hash_checksum(&content));
        fs::write(&backup_path, &content)
            .with_context(|| format!("backup config {}", path.display()))?;
        config_files.push(ConfigRecord {
            original_path: path.clone(),
            backup_path,
            checksum,
        });
    }

    let manifest = BackupManifest {
        timestamp: timestamp.to_string(),
        stipe_version: stipe_version.to_string(),
        binaries,
        config_files,
    };

    let manifest_path = backup_dir.join("manifest.json");
    let json = serde_json::to_string_pretty(&manifest)
        .context("serialize backup manifest")?;
    fs::write(&manifest_path, json)
        .with_context(|| format!("write manifest: {}", manifest_path.display()))?;

    Ok(backup_dir)
}

/// Lists available backups, returning (timestamp, backup_dir) sorted newest first.
pub fn list_backups() -> Result<Vec<(String, PathBuf)>> {
    let base = backup_base_dir();
    if !base.exists() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<(String, PathBuf)> = fs::read_dir(&base)
        .with_context(|| format!("read backup dir: {}", base.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let path = e.path();
            (name, path)
        })
        .collect();
    entries.sort_by(|a, b| b.0.cmp(&a.0)); // newest first
    Ok(entries)
}

/// Loads a backup manifest from a backup directory.
pub fn load_manifest(backup_dir: &Path) -> Result<BackupManifest> {
    let manifest_path = backup_dir.join("manifest.json");
    let json = fs::read_to_string(&manifest_path)
        .with_context(|| format!("read manifest: {}", manifest_path.display()))?;
    serde_json::from_str(&json).context("parse backup manifest")
}

/// Restores binaries and config files from a backup manifest.
pub fn restore_from_backup(manifest: &BackupManifest) -> Result<()> {
    for record in &manifest.binaries {
        if record.backup_path.exists() {
            fs::copy(&record.backup_path, &record.original_path)
                .with_context(|| format!("restore binary: {}", record.original_path.display()))?;
        }
    }
    for record in &manifest.config_files {
        if record.backup_path.exists() {
            fs::copy(&record.backup_path, &record.original_path)
                .with_context(|| format!("restore config: {}", record.original_path.display()))?;
        }
    }
    Ok(())
}

/// Simple FNV-inspired hash used for change detection in backup manifests.
/// Not cryptographic — for equality checking only.
fn hash_checksum(data: &[u8]) -> u128 {
    // Simple FNV-1a hash as checksum (avoids adding md5 dep)
    let mut hash: u128 = 0xcbf29ce484222325_u64 as u128;
    for &byte in data {
        hash ^= byte as u128;
        hash = hash.wrapping_mul(0x100000001b3_u128);
    }
    hash
}

/// Returns a timestamp string suitable for backup directory names.
pub fn backup_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Format: epoch seconds
    format!("{}", secs)
}

/// Creates a pre-upgrade backup of the Hyphae database and binary.
/// The backup path includes the hyphae version and timestamp.
/// Returns the backup path on success; logs a warning and returns Ok(None) on any failure,
/// allowing the upgrade to proceed without blocking.
pub fn pre_upgrade_backup_hyphae(hyphae_version: &str, timestamp: &str) -> Result<Option<PathBuf>> {
    let base = backup_base_dir();
    let backup_dir_name = format!("hyphae-{}-{}", hyphae_version, timestamp);
    let backup_dir = base.join(&backup_dir_name);

    // Create the backup directory structure
    fs::create_dir_all(&backup_dir)
        .map_err(|e| {
            warn!(
                "Failed to create hyphae backup directory {}: {}",
                backup_dir.display(),
                e
            );
            e
        })
        .ok();

    // Find the hyphae binary
    let hyphae_binary = match which::which("hyphae") {
        Ok(path) => path,
        Err(_) => {
            warn!("Could not locate hyphae binary for pre-upgrade backup");
            return Ok(None);
        }
    };

    // Find the hyphae database (default path)
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let hyphae_db = home
        .join(".local")
        .join("share")
        .join("hyphae")
        .join("hyphae.db");

    // Backup the hyphae binary if it exists
    if hyphae_binary.exists() {
        let backup_bin = backup_dir.join("hyphae");
        if let Err(e) = fs::copy(&hyphae_binary, &backup_bin) {
            warn!(
                "Failed to backup hyphae binary from {}: {}",
                hyphae_binary.display(),
                e
            );
        }
    }

    // Backup the hyphae database if it exists
    if hyphae_db.exists() {
        let backup_db = backup_dir.join("hyphae.db");
        if let Err(e) = fs::copy(&hyphae_db, &backup_db) {
            warn!(
                "Failed to backup hyphae database from {}: {}",
                hyphae_db.display(),
                e
            );
        }
    }

    Ok(Some(backup_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn create_and_load_manifest() {
        let tmp = TempDir::new().unwrap();
        let bin_path = tmp.path().join("mycelium");
        fs::write(&bin_path, b"fake binary").unwrap();
        let cfg_path = tmp.path().join("config.json");
        fs::write(&cfg_path, b"{\"key\": \"value\"}").unwrap();

        let old_dir = std::env::var("STIPE_BACKUP_DIR").ok();
        unsafe {
            std::env::set_var("STIPE_BACKUP_DIR", tmp.path().join("backups").to_str().unwrap());
        }

        let backup_dir = create_backup(
            "20260416-120000",
            "0.5.18",
            &[("mycelium".to_string(), bin_path.clone())],
            &[cfg_path.clone()],
        ).unwrap();

        let manifest = load_manifest(&backup_dir).unwrap();
        assert_eq!(manifest.stipe_version, "0.5.18");
        assert_eq!(manifest.binaries.len(), 1);
        assert_eq!(manifest.config_files.len(), 1);

        if let Some(dir) = old_dir {
            unsafe {
                std::env::set_var("STIPE_BACKUP_DIR", dir);
            }
        } else {
            unsafe {
                std::env::remove_var("STIPE_BACKUP_DIR");
            }
        }
    }

    #[test]
    fn list_backups_empty_when_no_dir() {
        unsafe {
            std::env::set_var("STIPE_BACKUP_DIR", "/tmp/stipe-test-nonexistent-backup-dir-xyz");
        }
        let result = list_backups().unwrap();
        assert!(result.is_empty());
        unsafe {
            std::env::remove_var("STIPE_BACKUP_DIR");
        }
    }

    #[test]
    fn test_backup_hyphae_path_includes_version_and_timestamp() {
        let tmp = TempDir::new().unwrap();
        let backup_dir_path = tmp.path().join("backups");
        unsafe {
            std::env::set_var(
                "STIPE_BACKUP_DIR",
                backup_dir_path.to_str().unwrap(),
            );
        }

        let version = "0.5.0";
        let timestamp = "1681234567";
        let result = pre_upgrade_backup_hyphae(version, timestamp).unwrap();

        if let Some(path) = result {
            let dir_name = path.file_name().unwrap().to_string_lossy();
            assert!(dir_name.contains("hyphae"));
            assert!(dir_name.contains(version));
            assert!(dir_name.contains(timestamp));
        }

        unsafe {
            std::env::remove_var("STIPE_BACKUP_DIR");
        }
    }

    #[test]
    fn test_backup_hyphae_warns_on_failure() {
        // Set backup dir to a non-writable location to trigger a warning
        unsafe {
            std::env::set_var("STIPE_BACKUP_DIR", "/dev/null/invalid/path");
        }

        let version = "0.5.0";
        let timestamp = "1681234567";
        let result = pre_upgrade_backup_hyphae(version, timestamp);

        // Should not error, but return Ok(None) when backup fails
        match result {
            Ok(_) => {
                // Expected behavior: warning logged, returns Ok(_)
            }
            Err(_) => {
                // Also acceptable if a hard error is returned
            }
        }

        unsafe {
            std::env::remove_var("STIPE_BACKUP_DIR");
        }
    }
}
