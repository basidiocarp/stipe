use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
        let checksum = format!("{:x}", md5_checksum(&content));
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

fn md5_checksum(data: &[u8]) -> u128 {
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
}
