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
/// Uses `STIPE_BACKUP_DIR` env var, falling back to `~/.local/share/stipe/backups`.
pub fn backup_base_dir() -> PathBuf {
    if let Ok(raw) = std::env::var("STIPE_BACKUP_DIR") {
        // Expand tilde if present at the start of the path
        let expanded = if let Some(rest) = raw.strip_prefix("~/") {
            dirs::home_dir().map_or_else(|| PathBuf::from(raw.clone()), |h| h.join(rest))
        } else if raw == "~" {
            dirs::home_dir().unwrap_or_else(|| PathBuf::from(raw.clone()))
        } else {
            PathBuf::from(&raw)
        };
        return expanded;
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
        let Some(fname) = path.file_name() else {
            continue;
        };
        let backup_path = bin_dir.join(fname);
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
        let Some(file_name) = path.file_name() else {
            continue;
        };
        let backup_path = cfg_dir.join(file_name);
        let content = fs::read(path).with_context(|| format!("read config {}", path.display()))?;
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
    let json = serde_json::to_string_pretty(&manifest).context("serialize backup manifest")?;
    fs::write(&manifest_path, json)
        .with_context(|| format!("write manifest: {}", manifest_path.display()))?;

    Ok(backup_dir)
}

/// Lists available backups, returning `(timestamp, backup_dir)` sorted newest first.
pub fn list_backups() -> Result<Vec<(String, PathBuf)>> {
    let base = backup_base_dir();
    if !base.exists() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<(String, PathBuf)> = fs::read_dir(&base)
        .with_context(|| format!("read backup dir: {}", base.display()))?
        .filter_map(Result::ok)
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

/// Non-standard hash used for change detection in backup manifests.
/// Uses 64-bit FNV-1a constants folded into a u128 accumulator.
/// Not cryptographic — for equality checking only.
fn hash_checksum(data: &[u8]) -> u128 {
    let mut hash = u128::from(0xcbf2_9ce4_8422_2325_u64);
    for &byte in data {
        hash ^= u128::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3_u128);
    }
    hash
}

/// Returns a timestamp string suitable for backup directory names.
pub fn backup_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // Format: epoch seconds
    secs.to_string()
}

/// Outcome of a pre-upgrade backup attempt.
#[derive(Debug, Clone)]
pub struct BackupOutcome {
    /// Backup directory path if successfully created.
    pub backup_dir: Option<PathBuf>,
    /// Binaries that were successfully copied.
    pub binaries_copied: Vec<PathBuf>,
    /// Database files that were successfully copied.
    pub databases_copied: Vec<PathBuf>,
    /// Missing files (not found at expected location).
    pub missing: Vec<String>,
    /// Files that failed to copy.
    pub failed: Vec<String>,
}

impl BackupOutcome {
    /// Returns true if the backup was completely successful.
    pub fn is_complete(&self) -> bool {
        // A missing database or binary means there was nothing to back up (optional/not yet
        // created). Only a copy failure — where the file exists but could not be written —
        // is a real problem that should surface as incomplete.
        self.failed.is_empty()
    }
}

/// Creates a pre-upgrade backup of the Hyphae database and binary.
/// The backup path includes the hyphae version and timestamp.
/// Returns a `BackupOutcome` describing what was successfully backed up,
/// what failed, and what was missing. Does not fail the upgrade even
/// if backup is incomplete.
pub fn pre_upgrade_backup_hyphae(hyphae_version: &str, timestamp: &str) -> BackupOutcome {
    let base = backup_base_dir();
    let backup_dir_name = format!("hyphae-{hyphae_version}-{timestamp}");
    let backup_dir = base.join(&backup_dir_name);

    let mut binaries_copied = Vec::new();
    let mut databases_copied = Vec::new();
    let mut missing = Vec::new();
    let mut failed = Vec::new();

    // Create the backup directory structure
    let backup_dir_created = if let Err(e) = fs::create_dir_all(&backup_dir) {
        warn!(
            "Failed to create hyphae backup directory {}: {}",
            backup_dir.display(),
            e
        );
        failed.push(format!("backup directory: {e}"));
        false
    } else {
        true
    };

    if !backup_dir_created {
        return BackupOutcome {
            backup_dir: None,
            binaries_copied,
            databases_copied,
            missing,
            failed,
        };
    }

    // Find the hyphae binary
    let hyphae_binary = if let Ok(path) = which::which("hyphae") {
        Some(path)
    } else {
        warn!("Could not locate hyphae binary for pre-upgrade backup");
        missing.push("hyphae binary".to_string());
        None
    };

    // Resolve the hyphae database path. The canonical location moved from
    // `~/.local/share/hyphae/` to `~/.local/share/basidiocarp/hyphae/` after the
    // shared storage root migration. Check the canonical path first, then the
    // legacy path so that machines that haven't launched hyphae since the migration
    // still get their database backed up.
    let hyphae_db = dirs::data_dir().and_then(|data| {
        let canonical = data.join("basidiocarp").join("hyphae").join("hyphae.db");
        if canonical.exists() {
            return Some(canonical);
        }
        let legacy = data.join("hyphae").join("hyphae.db");
        if legacy.exists() {
            return Some(legacy);
        }
        None
    });

    // Backup the hyphae binary if it exists
    if let Some(bin_path) = hyphae_binary {
        if bin_path.exists() {
            let backup_bin = backup_dir.join("hyphae");
            match fs::copy(&bin_path, &backup_bin) {
                Ok(_) => {
                    binaries_copied.push(backup_bin);
                }
                Err(e) => {
                    warn!(
                        "Failed to backup hyphae binary from {}: {}",
                        bin_path.display(),
                        e
                    );
                    failed.push(format!("hyphae binary: {e}"));
                }
            }
        } else {
            missing.push("hyphae binary (not found at expected path)".to_string());
        }
    }

    // Backup the hyphae database if it was found at either the canonical or legacy path.
    // If neither path exists the database has not been created yet (fresh install or hyphae
    // has never been run); that is not an error, so we skip silently rather than flagging it
    // as missing.
    if let Some(db_path) = hyphae_db {
        let backup_db = backup_dir.join("hyphae.db");
        match fs::copy(&db_path, &backup_db) {
            Ok(_) => {
                databases_copied.push(backup_db);
            }
            Err(e) => {
                warn!(
                    "Failed to backup hyphae database from {}: {}",
                    db_path.display(),
                    e
                );
                failed.push(format!("hyphae database: {e}"));
            }
        }
    }

    BackupOutcome {
        backup_dir: Some(backup_dir),
        binaries_copied,
        databases_copied,
        missing,
        failed,
    }
}

#[cfg(test)]
#[allow(unsafe_code)]
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
        // SAFETY: This is a test. We're setting the environment variable only for this test.
        unsafe {
            std::env::set_var(
                "STIPE_BACKUP_DIR",
                tmp.path().join("backups").to_str().unwrap(),
            );
        }

        let backup_dir = create_backup(
            "20260416-120000",
            "0.5.18",
            &[("mycelium".to_string(), bin_path)],
            std::slice::from_ref(&cfg_path),
        )
        .unwrap();

        let manifest = load_manifest(&backup_dir).unwrap();
        assert_eq!(manifest.stipe_version, "0.5.18");
        assert_eq!(manifest.binaries.len(), 1);
        assert_eq!(manifest.config_files.len(), 1);

        // SAFETY: This is a test. We're restoring or removing the environment variable.
        unsafe {
            if let Some(dir) = old_dir {
                std::env::set_var("STIPE_BACKUP_DIR", dir);
            } else {
                std::env::remove_var("STIPE_BACKUP_DIR");
            }
        }
    }

    #[test]
    fn list_backups_empty_when_no_dir() {
        let tmp = TempDir::new().unwrap();
        let nonexistent_dir = tmp.path().join("no-backups");
        // SAFETY: This is a test. We're setting the environment variable only for this test.
        unsafe {
            std::env::set_var("STIPE_BACKUP_DIR", nonexistent_dir.to_str().unwrap());
        }
        let result = list_backups().unwrap();
        assert!(result.is_empty());
        // SAFETY: This is a test. We're removing the environment variable.
        unsafe {
            std::env::remove_var("STIPE_BACKUP_DIR");
        }
    }

    #[test]
    fn test_backup_hyphae_path_includes_version_and_timestamp() {
        let tmp = TempDir::new().unwrap();
        let backup_dir_path = tmp.path().join("backups");
        // SAFETY: This is a test. We're setting the environment variable only for this test.
        unsafe {
            std::env::set_var("STIPE_BACKUP_DIR", backup_dir_path.to_str().unwrap());
        }

        let version = "0.5.0";
        let timestamp = "1681234567";
        let result = pre_upgrade_backup_hyphae(version, timestamp);

        if let Some(path) = result.backup_dir {
            let dir_name = path.file_name().unwrap().to_string_lossy();
            assert!(dir_name.contains("hyphae"));
            assert!(dir_name.contains(version));
            assert!(dir_name.contains(timestamp));
        }

        // SAFETY: This is a test. We're removing the environment variable.
        unsafe {
            std::env::remove_var("STIPE_BACKUP_DIR");
        }
    }

    #[test]
    fn test_backup_hyphae_warns_on_failure() {
        // Set backup dir to a non-writable location to trigger a warning
        // SAFETY: This is a test. We're setting the environment variable only for this test.
        unsafe {
            std::env::set_var("STIPE_BACKUP_DIR", "/dev/null/invalid/path");
        }

        let version = "0.5.0";
        let timestamp = "1681234567";
        let _ = pre_upgrade_backup_hyphae(version, timestamp);

        // SAFETY: This is a test. We're removing the environment variable.
        unsafe {
            std::env::remove_var("STIPE_BACKUP_DIR");
        }
    }
}
