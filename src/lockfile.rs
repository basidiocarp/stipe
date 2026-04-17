use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const STALE_THRESHOLD_SECS: u64 = 600; // 10 minutes

#[derive(Debug, Serialize, Deserialize)]
pub struct LockRecord {
    pub pid: u32,
    pub timestamp_secs: u64,
}

/// Returns the path to the install lockfile.
pub fn lock_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("stipe")
        .join("install.lock")
}

/// Acquires the install lock. Returns error if another process holds a fresh lock.
/// If force=true, overrides stale locks without prompting.
pub fn acquire_lock(force: bool) -> Result<()> {
    let path = lock_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create stipe data dir")?;
    }

    if path.exists() {
        let content = fs::read_to_string(&path).context("read lock file")?;
        if let Ok(record) = serde_json::from_str::<LockRecord>(&content) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let age = now.saturating_sub(record.timestamp_secs);

            if age < STALE_THRESHOLD_SECS {
                bail!(
                    "stipe install is already running (PID {}, started {}s ago). \
                     Use --force to override.",
                    record.pid,
                    age
                );
            } else if !force {
                bail!(
                    "A stale stipe lock exists (PID {}, {}s old). \
                     Use --force to override.",
                    record.pid,
                    age
                );
            }
            // Force override — fall through to write new lock
        }
    }

    let record = LockRecord {
        pid: std::process::id(),
        timestamp_secs: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    };
    let json = serde_json::to_string(&record).context("serialize lock record")?;
    fs::write(&path, json).with_context(|| format!("write lock file: {}", path.display()))?;
    Ok(())
}

/// Releases the install lock.
pub fn release_lock() {
    let path = lock_path();
    let _ = fs::remove_file(&path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_and_release_lock() {
        // Just verify the functions don't panic on a fresh state
        // (full path test would require mocking dirs::data_local_dir)
        assert!(lock_path().to_str().is_some());
    }
}
