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

#[cfg(test)]
thread_local! {
    static TEST_LOCK_PATH_OVERRIDE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Returns the path to the install lockfile.
pub fn lock_path() -> PathBuf {
    #[cfg(test)]
    if let Some(path) = test_lock_path_override() {
        return path;
    }

    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("stipe")
        .join("install.lock")
}

#[cfg(test)]
fn test_lock_path_override() -> Option<PathBuf> {
    TEST_LOCK_PATH_OVERRIDE.with(|path| path.borrow().clone())
}

#[cfg(test)]
pub(crate) fn with_lock_path_override<T>(path: PathBuf, f: impl FnOnce() -> T) -> T {
    TEST_LOCK_PATH_OVERRIDE.with(|override_path| {
        let previous = override_path.replace(Some(path));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        override_path.replace(previous);
        match result {
            Ok(value) => value,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    })
}

/// RAII guard that releases the install lock when dropped.
pub struct LockGuard;

impl Drop for LockGuard {
    fn drop(&mut self) {
        release_lock();
    }
}

/// Acquires the install lock and returns a guard that releases it on drop.
/// Returns error if a fresh (< 10 min) lock is held by another process.
/// Stale locks (>= 10 min) are automatically reclaimed.
/// If force=true, overrides even fresh locks.
pub fn acquire_lock(force: bool) -> Result<LockGuard> {
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

            if age < STALE_THRESHOLD_SECS && !force {
                bail!(
                    "stipe install is already running (PID {}, started {}s ago). \
                     Use --force to override.",
                    record.pid,
                    age
                );
            }
            // Stale lock or force — reclaim it silently
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
    Ok(LockGuard)
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
        // Test acquire/release round-trip with a test-specific lock path
        let tmp_lock = std::env::temp_dir().join("stipe-test-install.lock");

        // Clean up any previous test lock file
        let _ = fs::remove_file(&tmp_lock);

        with_lock_path_override(tmp_lock.clone(), || {
            // Acquire the lock
            let guard = acquire_lock(false).expect("should acquire lock");
            assert!(tmp_lock.exists());

            // Drop the guard to release
            drop(guard);

            // Lock file should be cleaned up
            assert!(!tmp_lock.exists());
        });
    }

    #[test]
    fn stale_lock_can_be_reclaimed() {
        let tmp_lock = std::env::temp_dir().join("stipe-test-stale.lock");
        let _ = fs::remove_file(&tmp_lock);

        with_lock_path_override(tmp_lock.clone(), || {
            // Create a stale lock (timestamp = 0, effectively very old)
            let old_record = LockRecord {
                pid: 9999,
                timestamp_secs: 0,
            };
            let json = serde_json::to_string(&old_record).unwrap();
            fs::write(&tmp_lock, json).unwrap();

            // Should be able to acquire even without force (it's stale)
            let guard = acquire_lock(false).expect("should reclaim stale lock");
            drop(guard);
            assert!(!tmp_lock.exists());
        });
    }
}
