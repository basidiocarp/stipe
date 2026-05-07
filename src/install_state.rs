use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Status of a recorded installed item, determined by checking the filesystem.
#[derive(Debug, Clone, Copy)]
pub enum ItemStatus {
    /// File exists and checksum matches (or no checksum was recorded).
    Ok,
    /// File path was recorded but the file no longer exists.
    Missing,
    /// File exists but its checksum does not match the recorded value.
    Drift,
    /// No path was recorded, so status cannot be determined.
    Unknown,
}

impl ItemStatus {
    /// Returns a short label suitable for display.
    pub fn label(self) -> &'static str {
        match self {
            ItemStatus::Ok => "OK",
            ItemStatus::Missing => "MISSING",
            ItemStatus::Drift => "DRIFT",
            ItemStatus::Unknown => "UNKNOWN",
        }
    }
}

/// Checks the on-disk status of a recorded installed item.
pub fn check_item_status(item: &InstalledItem) -> ItemStatus {
    // If no path recorded, status is UNKNOWN
    let Some(path_str) = &item.path else {
        return ItemStatus::Unknown;
    };

    let path = Path::new(path_str);

    // If path doesn't exist, status is MISSING
    if !path.exists() {
        return ItemStatus::Missing;
    }

    // If checksum is set, verify it
    if let Some(expected_checksum) = &item.checksum {
        match compute_checksum(path) {
            Ok(actual_checksum) => {
                if actual_checksum == *expected_checksum {
                    ItemStatus::Ok
                } else {
                    ItemStatus::Drift
                }
            }
            Err(_) => ItemStatus::Unknown,
        }
    } else {
        // File exists but no checksum to verify
        ItemStatus::Ok
    }
}

/// Returns the path to the install state database.
pub fn db_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("stipe").join("install-state.db"))
}

/// Opens or creates the install state database.
pub fn open() -> Result<Connection> {
    let path = db_path().context("Could not determine data directory")?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .context("Failed to create stipe data directory")?;
    }

    let conn = Connection::open(&path)
        .with_context(|| format!("Failed to open install state database at {}", path.display()))?;

    // Initialize schema
    initialize_schema(&conn)?;

    Ok(conn)
}

fn initialize_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS installed_items (
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            path TEXT,
            version TEXT,
            installed_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            source TEXT,
            checksum TEXT
        );

        CREATE TABLE IF NOT EXISTS install_state_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    ).context("Failed to initialize install state schema")?;

    // Initialize schema version if not present
    let has_version: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM install_state_meta WHERE key = 'schema_version')",
        [],
        |row| row.get(0),
    ).context("Failed to check schema version")?;

    if !has_version {
        conn.execute(
            "INSERT INTO install_state_meta (key, value) VALUES (?, ?)",
            params!["schema_version", "1"],
        ).context("Failed to initialize schema version")?;
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub struct InstalledItem {
    pub id: String,
    pub kind: String,
    pub path: Option<String>,
    #[allow(dead_code)]
    pub version: Option<String>,
    #[allow(dead_code)]
    pub installed_at: i64,
    #[allow(dead_code)]
    pub updated_at: i64,
    pub source: Option<String>,
    pub checksum: Option<String>,
}

/// Records the installation of an item.
pub fn record_install(
    conn: &Connection,
    id: &str,
    kind: &str,
    path: Option<&str>,
    version: Option<&str>,
    source: Option<&str>,
    checksum: Option<&str>,
) -> Result<()> {
    // Unix timestamps fit comfortably in i64 for centuries; the cast won't wrap.
    #[allow(clippy::cast_possible_wrap)]
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("Failed to get current time")?
        .as_secs() as i64;

    // Use ON CONFLICT DO UPDATE so that installed_at is preserved on re-installs;
    // only updated_at, path, version, source, and checksum are refreshed.
    conn.execute(
        "INSERT INTO installed_items (id, kind, path, version, installed_at, updated_at, source, checksum)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
             kind       = excluded.kind,
             path       = excluded.path,
             version    = excluded.version,
             updated_at = excluded.updated_at,
             source     = excluded.source,
             checksum   = excluded.checksum",
        params![id, kind, path, version, now, now, source, checksum],
    ).with_context(|| format!("Failed to record install for {id}"))?;

    Ok(())
}

/// Lists all installed items.
pub fn list_all(conn: &Connection) -> Result<Vec<InstalledItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, path, version, installed_at, updated_at, source, checksum
         FROM installed_items ORDER BY id"
    ).context("Failed to prepare list query")?;

    let items = stmt.query_map([], |row| {
        Ok(InstalledItem {
            id: row.get(0)?,
            kind: row.get(1)?,
            path: row.get(2)?,
            version: row.get(3)?,
            installed_at: row.get(4)?,
            updated_at: row.get(5)?,
            source: row.get(6)?,
            checksum: row.get(7)?,
        })
    }).context("Failed to query installed items")?;

    let mut result = Vec::new();
    for item in items {
        result.push(item.context("Failed to parse installed item")?);
    }

    Ok(result)
}

/// Computes SHA256 checksum of a file.
pub fn compute_checksum(path: &Path) -> Result<String> {
    use sha2::{Sha256, Digest};
    use std::fs::File;
    use std::io::Read;

    let mut file = File::open(path)
        .with_context(|| format!("Failed to open file for checksum: {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)
            .context("Failed to read file for checksum")?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_record_and_list() -> Result<()> {
        let tmpdir = TempDir::new()?;
        let db_path = tmpdir.path().join("test.db");

        let conn = Connection::open(&db_path)?;
        initialize_schema(&conn)?;

        record_install(&conn, "test-item", "hook", Some("/path/to/item"), Some("1.0.0"), Some("test"), None)?;

        let items = list_all(&conn)?;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "test-item");
        assert_eq!(items[0].kind, "hook");
        assert_eq!(items[0].path, Some("/path/to/item".to_string()));
        assert_eq!(items[0].version, Some("1.0.0".to_string()));

        Ok(())
    }

    #[test]
    fn test_replace_on_duplicate_id() -> Result<()> {
        let tmpdir = TempDir::new()?;
        let db_path = tmpdir.path().join("test.db");

        let conn = Connection::open(&db_path)?;
        initialize_schema(&conn)?;

        record_install(&conn, "item", "hook", Some("/path1"), Some("1.0.0"), None, None)?;
        record_install(&conn, "item", "hook", Some("/path2"), Some("2.0.0"), None, None)?;

        let items = list_all(&conn)?;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].path, Some("/path2".to_string()));
        assert_eq!(items[0].version, Some("2.0.0".to_string()));

        Ok(())
    }

    #[test]
    fn test_schema_creates_cleanly() -> Result<()> {
        let tmpdir = TempDir::new()?;
        let db_path = tmpdir.path().join("test.db");

        let conn = Connection::open(&db_path)?;
        initialize_schema(&conn)?;

        // Verify tables exist by querying them
        let items = list_all(&conn)?;
        assert_eq!(items.len(), 0);

        Ok(())
    }
}
