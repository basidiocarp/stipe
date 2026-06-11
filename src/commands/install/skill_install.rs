use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spore::atomic_write_bytes;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// A skill pack manifest describing the skills to install.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPackManifest {
    pub pack_name: String,
    pub version: String,
    pub skills: Vec<SkillEntry>,
}

/// A single skill file to be installed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    pub name: String,
    /// Relative path inside pack to source file
    pub source_path: String,
    /// Absolute or ~-prefixed path on host
    pub target_path: String,
    /// Lowercase hex SHA-256 of source file content
    pub sha256: String,
}

/// Result of verifying a skill file.
#[derive(Debug, Clone)]
#[allow(dead_code)] // public API surface for skill verification; not yet consumed by a CLI command
pub struct SkillVerifyResult {
    pub entry: SkillEntry,
    pub status: SkillVerifyStatus,
}

/// Status of a verified skill file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillVerifyStatus {
    Ok,
    Missing,
    ChecksumMismatch { actual: String },
}

impl SkillVerifyStatus {
    /// Verify a skill entry and return its status.
    pub fn from_entry(entry: &SkillEntry) -> Result<Self> {
        verify_skill(entry)
    }
}

/// Load and parse a skills.json manifest from a pack directory.
pub fn load_manifest(pack_dir: &Path) -> Result<SkillPackManifest> {
    let manifest_path = pack_dir.join("skills.json");
    let json = fs::read_to_string(&manifest_path)
        .with_context(|| format!("read skills.json from {}", manifest_path.display()))?;
    serde_json::from_str(&json).context("parse skills.json manifest")
}

/// Expand ~-prefixed paths to absolute paths.
fn expand_home(path: &str) -> Result<PathBuf> {
    if let Some(rest) = path.strip_prefix("~/") {
        dirs::home_dir()
            .map(|h| h.join(rest))
            .ok_or_else(|| anyhow!("Could not determine home directory"))
    } else if path == "~" {
        dirs::home_dir().ok_or_else(|| anyhow!("Could not determine home directory"))
    } else {
        Ok(PathBuf::from(path))
    }
}

/// Compute SHA-256 hex of file contents.
fn file_sha256(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("open file: {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Verify a single skill file against its manifest entry.
pub fn verify_skill(entry: &SkillEntry) -> Result<SkillVerifyStatus> {
    let target_path = expand_home(&entry.target_path)?;
    if !target_path.exists() {
        return Ok(SkillVerifyStatus::Missing);
    }
    let actual = file_sha256(&target_path)?;
    if actual == entry.sha256 {
        Ok(SkillVerifyStatus::Ok)
    } else {
        Ok(SkillVerifyStatus::ChecksumMismatch { actual })
    }
}

/// Extract a .tar.gz file to a temporary directory.
fn extract_skill_pack_archive(archive_path: &Path) -> Result<tempfile::TempDir> {
    let data = fs::read(archive_path)
        .with_context(|| format!("read archive: {}", archive_path.display()))?;
    let tar_gz = std::io::Cursor::new(data);
    let tar = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(tar);
    let temp_dir = tempfile::TempDir::new().context("create temp directory for extraction")?;
    archive.unpack(temp_dir.path()).context("extract archive")?;
    Ok(temp_dir)
}

/// Create a snapshot of pre-install state for rollback.
pub fn create_skill_snapshot(manifest: &SkillPackManifest) -> Result<SkillSnapshot> {
    let snapshot_dir = tempfile::TempDir::new().context("create skill snapshot directory")?;
    let mut files = Vec::new();

    for entry in &manifest.skills {
        let target_path = expand_home(&entry.target_path)?;
        let state = if target_path.exists() {
            SkillFileState::Exists {
                content: fs::read(&target_path)
                    .with_context(|| format!("read existing file: {}", target_path.display()))?,
            }
        } else {
            SkillFileState::Absent
        };

        files.push((entry.target_path.clone(), state));
    }

    Ok(SkillSnapshot {
        snapshot_dir,
        files,
    })
}

/// Pre-install state of skill files for rollback.
pub struct SkillSnapshot {
    #[allow(dead_code)] // TempDir is kept alive for RAII cleanup; dropped when SkillSnapshot drops
    pub snapshot_dir: tempfile::TempDir,
    pub files: Vec<(String, SkillFileState)>,
}

/// State of a single skill file before install.
#[derive(Clone)]
pub enum SkillFileState {
    Exists { content: Vec<u8> },
    Absent,
}

/// Restore files from a snapshot.
pub fn restore_skill_snapshot(snapshot: SkillSnapshot) -> Result<()> {
    for (target_path_str, state) in snapshot.files {
        let target_path = expand_home(&target_path_str)?;
        match state {
            SkillFileState::Exists { content } => {
                // File existed before install; restore it
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("create parent directories: {}", parent.display())
                    })?;
                }
                fs::write(&target_path, content)
                    .with_context(|| format!("restore file: {}", target_path.display()))?;
            }
            SkillFileState::Absent => {
                // File didn't exist before install; delete it
                if target_path.exists() {
                    fs::remove_file(&target_path).with_context(|| {
                        format!("delete installed file: {}", target_path.display())
                    })?;
                }
            }
        }
    }
    Ok(())
}

/// Allowed target path prefixes for skill install (security boundary).
const ALLOWED_TARGET_PREFIXES: &[&str] = &[
    "~/.config/basidiocarp/",
    "~/.config/claude/",
    "~/.claude/",
    "~/.local/share/basidiocarp/",
];

/// Validate and resolve a `source_path` against the pack root, rejecting traversal attempts.
fn validated_source_path(pack_root: &Path, source_path: &str) -> Result<PathBuf> {
    let joined = pack_root.join(source_path);
    let canonical_root = pack_root
        .canonicalize()
        .with_context(|| format!("canonicalize pack root: {}", pack_root.display()))?;
    let canonical_joined = joined
        .canonicalize()
        .with_context(|| format!("source file not found: {}", joined.display()))?;
    if !canonical_joined.starts_with(&canonical_root) {
        return Err(anyhow!(
            "source_path '{source_path}' escapes the pack directory — rejected"
        ));
    }
    Ok(canonical_joined)
}

/// Validate and expand a `target_path`, rejecting paths outside the allowed skill directories.
fn validated_target_path(target_path: &str) -> Result<PathBuf> {
    let allowed = ALLOWED_TARGET_PREFIXES
        .iter()
        .any(|prefix| target_path.starts_with(prefix));
    if !allowed {
        let allowed_list = ALLOWED_TARGET_PREFIXES.join(", ");
        return Err(anyhow!(
            "target_path '{target_path}' is outside allowed directories. Allowed prefixes: {allowed_list}"
        ));
    }
    expand_home(target_path)
}

/// Get the path to the installed skills manifest file.
/// This path is used for both reading and writing the manifest.
fn installed_manifest_path() -> Result<PathBuf> {
    #[cfg(test)]
    if let Some(path) = test_manifest_path_override() {
        return Ok(path);
    }

    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow!("Could not determine config directory"))?
        .join("basidiocarp")
        .join("skills");
    Ok(config_dir.join(".installed-manifest.json"))
}

/// Install a skill pack from a directory or .tar.gz archive.
pub fn install_skills(pack_path: &Path) -> Result<()> {
    // Keep _temp_dir alive for the full duration of this function.
    // If pack_path is a .tar.gz, the extracted directory must not be cleaned up
    // until we have finished reading from it.
    let (_temp_dir, pack_root) = if pack_path.to_string_lossy().ends_with(".tar.gz") {
        let temp = extract_skill_pack_archive(pack_path)?;
        let root = temp.path().to_path_buf();
        (Some(temp), root)
    } else {
        (None, pack_path.to_path_buf())
    };

    // Load manifest
    let manifest = load_manifest(&pack_root)?;

    // Validate all source files exist before installing (also catches traversal early)
    for entry in &manifest.skills {
        validated_source_path(&pack_root, &entry.source_path)?;
    }

    // Validate all target paths are within allowed directories
    for entry in &manifest.skills {
        validated_target_path(&entry.target_path)?;
    }

    // Create snapshot for rollback
    let snapshot = create_skill_snapshot(&manifest)?;

    // Install each skill
    for entry in &manifest.skills {
        let source_path = validated_source_path(&pack_root, &entry.source_path)?;
        let target_path = validated_target_path(&entry.target_path)?;

        // Create parent directories
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create directories for: {}", target_path.display()))?;
        }

        // Copy file
        fs::copy(&source_path, &target_path).with_context(|| {
            format!(
                "copy skill {} from {} to {}",
                entry.name,
                source_path.display(),
                target_path.display()
            )
        })?;
    }

    // Verify all installed files
    let mut verify_failures: Vec<(String, String)> = Vec::new();
    for entry in &manifest.skills {
        match verify_skill(entry) {
            Ok(SkillVerifyStatus::Ok) => {}
            Ok(SkillVerifyStatus::Missing) => {
                verify_failures.push((entry.name.clone(), "file not found after copy".to_string()));
            }
            Ok(SkillVerifyStatus::ChecksumMismatch { actual }) => {
                verify_failures.push((
                    entry.name.clone(),
                    format!(
                        "checksum mismatch (expected {}, got {actual})",
                        entry.sha256
                    ),
                ));
            }
            Err(e) => {
                verify_failures.push((entry.name.clone(), format!("verification error: {e}")));
            }
        }
    }

    if !verify_failures.is_empty() {
        // Rollback on verification failure
        restore_skill_snapshot(snapshot)?;
        let mut error_lines =
            vec!["Skill verification failed after install; rolled back:".to_string()];
        for (name, reason) in verify_failures {
            error_lines.push(format!("  {name}: {reason}"));
        }
        return Err(anyhow!(error_lines.join("\n")));
    }

    // Write the installed manifest
    let manifest_path = installed_manifest_path()?;
    let parent = manifest_path.parent().ok_or_else(|| {
        anyhow!(
            "installed manifest path has no parent directory: {}",
            manifest_path.display()
        )
    })?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create skills config directory: {}", parent.display()))?;

    let json = serde_json::to_string_pretty(&manifest).context("serialize installed manifest")?;
    atomic_write_bytes(&manifest_path, json.as_bytes())
        .with_context(|| format!("write installed manifest: {}", manifest_path.display()))?;

    println!(
        "Successfully installed skill pack '{}' (version {})",
        manifest.pack_name, manifest.version
    );

    Ok(())
}

/// Verify the integrity of installed skills against the persisted manifest.
/// This is a read-only operation that performs no filesystem mutations.
pub fn verify_installed_skills() -> Result<()> {
    let manifest_path = installed_manifest_path()?;
    if !manifest_path.exists() {
        return Err(anyhow!(
            "no installed skill manifest found at {}; nothing to verify",
            manifest_path.display()
        ));
    }

    let json = fs::read_to_string(&manifest_path)
        .with_context(|| format!("read installed manifest: {}", manifest_path.display()))?;
    let manifest: SkillPackManifest =
        serde_json::from_str(&json).context("parse installed manifest")?;

    // Verify all installed files
    let mut verify_failures: Vec<(String, String)> = Vec::new();
    for entry in &manifest.skills {
        match verify_skill(entry) {
            Ok(SkillVerifyStatus::Ok) => {}
            Ok(SkillVerifyStatus::Missing) => {
                verify_failures.push((entry.name.clone(), "file not found".to_string()));
            }
            Ok(SkillVerifyStatus::ChecksumMismatch { actual }) => {
                verify_failures.push((
                    entry.name.clone(),
                    format!(
                        "checksum mismatch (expected {}, got {actual})",
                        entry.sha256
                    ),
                ));
            }
            Err(e) => {
                verify_failures.push((entry.name.clone(), format!("verification error: {e}")));
            }
        }
    }

    if !verify_failures.is_empty() {
        let mut error_lines = vec!["Installed skill verification failed:".to_string()];
        for (name, reason) in verify_failures {
            error_lines.push(format!("  {name}: {reason}"));
        }
        return Err(anyhow!(error_lines.join("\n")));
    }

    println!(
        "Verified {} installed skills (pack '{}' version {})",
        manifest.skills.len(),
        manifest.pack_name,
        manifest.version
    );

    Ok(())
}

#[cfg(test)]
thread_local! {
    static TEST_MANIFEST_PATH_OVERRIDE: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn test_manifest_path_override() -> Option<PathBuf> {
    TEST_MANIFEST_PATH_OVERRIDE.with(|path| path.borrow().clone())
}

#[cfg(test)]
pub(crate) fn with_manifest_path_override<T>(path: PathBuf, f: impl FnOnce() -> T) -> T {
    TEST_MANIFEST_PATH_OVERRIDE.with(|override_path| {
        let previous = override_path.replace(Some(path));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        override_path.replace(previous);
        match result {
            Ok(value) => value,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_manifest_valid() {
        let temp = tempfile::TempDir::new().unwrap();
        let manifest_content = r#"{
            "pack_name": "test-pack",
            "version": "1.0.0",
            "skills": [
                {
                    "name": "skill1",
                    "source_path": "skills/skill1.sh",
                    "target_path": "~/.config/skills/skill1.sh",
                    "sha256": "abcd1234"
                }
            ]
        }"#;
        fs::write(temp.path().join("skills.json"), manifest_content).unwrap();

        let manifest = load_manifest(temp.path()).unwrap();
        assert_eq!(manifest.pack_name, "test-pack");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.skills.len(), 1);
        assert_eq!(manifest.skills[0].name, "skill1");
    }

    #[test]
    fn test_load_manifest_missing() {
        let temp = tempfile::TempDir::new().unwrap();
        let result = load_manifest(temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_status_ok() {
        let temp = tempfile::TempDir::new().unwrap();
        let file_path = temp.path().join("test.sh");
        fs::write(&file_path, b"content").unwrap();

        let actual_sha = file_sha256(&file_path).unwrap();
        let entry = SkillEntry {
            name: "test".to_string(),
            source_path: "test.sh".to_string(),
            target_path: file_path.to_string_lossy().to_string(),
            sha256: actual_sha,
        };

        let status = verify_skill(&entry).unwrap();
        assert_eq!(status, SkillVerifyStatus::Ok);
    }

    #[test]
    fn test_verify_status_missing() {
        let entry = SkillEntry {
            name: "test".to_string(),
            source_path: "test.sh".to_string(),
            target_path: "/nonexistent/path/to/skill.sh".to_string(),
            sha256: "abc123".to_string(),
        };

        let status = verify_skill(&entry).unwrap();
        assert_eq!(status, SkillVerifyStatus::Missing);
    }

    #[test]
    fn test_verify_status_checksum_mismatch() {
        let temp = tempfile::TempDir::new().unwrap();
        let file_path = temp.path().join("test.sh");
        fs::write(&file_path, b"content").unwrap();

        let entry = SkillEntry {
            name: "test".to_string(),
            source_path: "test.sh".to_string(),
            target_path: file_path.to_string_lossy().to_string(),
            sha256: "wrong_hash".to_string(),
        };

        let status = verify_skill(&entry).unwrap();
        match status {
            SkillVerifyStatus::ChecksumMismatch { .. } => {
                // Expected
            }
            _ => panic!("Expected checksum mismatch"),
        }
    }

    #[test]
    fn test_expand_home() {
        let expanded = expand_home("~/test").unwrap();
        let home = dirs::home_dir().unwrap();
        assert_eq!(expanded, home.join("test"));
    }

    #[test]
    fn test_expand_home_tilde_only() {
        let expanded = expand_home("~").unwrap();
        let home = dirs::home_dir().unwrap();
        assert_eq!(expanded, home);
    }

    #[test]
    fn test_expand_absolute_path() {
        let expanded = expand_home("/absolute/path").unwrap();
        assert_eq!(expanded, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_verify_installed_skills_with_intact_manifest() {
        let temp = tempfile::TempDir::new().unwrap();
        let manifest_dir = temp.path().join("manifest");
        fs::create_dir_all(&manifest_dir).unwrap();

        // Create a skill file with known content
        let skill_file = temp.path().join("skill.sh");
        fs::write(&skill_file, b"#!/bin/bash\necho hello").unwrap();
        let skill_sha = file_sha256(&skill_file).unwrap();

        // Create a manifest with that skill
        let manifest = SkillPackManifest {
            pack_name: "test-pack".to_string(),
            version: "1.0.0".to_string(),
            skills: vec![SkillEntry {
                name: "test-skill".to_string(),
                source_path: "skill.sh".to_string(),
                target_path: skill_file.to_string_lossy().to_string(),
                sha256: skill_sha,
            }],
        };

        let manifest_path = manifest_dir.join(".installed-manifest.json");
        let json = serde_json::to_string_pretty(&manifest).unwrap();
        fs::write(&manifest_path, json).unwrap();

        // Snapshot the raw bytes of both the manifest and the skill file so the
        // no-mutation invariant is proven byte-for-byte, not via a semantic proxy.
        let manifest_before = fs::read(&manifest_path).unwrap();
        let skill_before = fs::read(&skill_file).unwrap();

        // Override the manifest path for this test
        with_manifest_path_override(manifest_path.clone(), || {
            let result = verify_installed_skills();
            assert!(
                result.is_ok(),
                "verify_installed_skills should succeed with intact manifest"
            );

            // No-mutation invariant: neither the manifest nor the verified file changed.
            assert_eq!(
                fs::read(&manifest_path).unwrap(),
                manifest_before,
                "manifest file must not be modified by verify"
            );
            assert_eq!(
                fs::read(&skill_file).unwrap(),
                skill_before,
                "verified skill file must not be modified by verify"
            );
        });
    }

    #[test]
    fn test_verify_installed_skills_with_tampered_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let manifest_dir = temp.path().join("manifest");
        fs::create_dir_all(&manifest_dir).unwrap();

        // Create a skill file with known content
        let skill_file = temp.path().join("skill.sh");
        fs::write(&skill_file, b"#!/bin/bash\necho hello").unwrap();
        let skill_sha = file_sha256(&skill_file).unwrap();

        // Create a manifest with that skill
        let manifest = SkillPackManifest {
            pack_name: "test-pack".to_string(),
            version: "1.0.0".to_string(),
            skills: vec![SkillEntry {
                name: "test-skill".to_string(),
                source_path: "skill.sh".to_string(),
                target_path: skill_file.to_string_lossy().to_string(),
                sha256: skill_sha,
            }],
        };

        let manifest_path = manifest_dir.join(".installed-manifest.json");
        let json = serde_json::to_string_pretty(&manifest).unwrap();
        fs::write(&manifest_path, json).unwrap();

        // Tamper with the file after manifest is written
        fs::write(&skill_file, b"#!/bin/bash\necho tampered").unwrap();

        with_manifest_path_override(manifest_path, || {
            let result = verify_installed_skills();
            assert!(
                result.is_err(),
                "verify_installed_skills should fail when skill file is tampered"
            );
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("checksum mismatch")
            );
        });
    }

    #[test]
    fn test_verify_installed_skills_with_absent_manifest() {
        let temp = tempfile::TempDir::new().unwrap();
        let manifest_path = temp.path().join(".installed-manifest.json");

        with_manifest_path_override(manifest_path, || {
            let result = verify_installed_skills();
            assert!(
                result.is_err(),
                "verify_installed_skills should fail when manifest is absent"
            );
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("no installed skill manifest found")
            );
        });
    }

    #[test]
    fn test_verify_installed_skills_with_missing_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let manifest_dir = temp.path().join("manifest");
        fs::create_dir_all(&manifest_dir).unwrap();

        // Create a manifest with a non-existent target file
        let manifest = SkillPackManifest {
            pack_name: "test-pack".to_string(),
            version: "1.0.0".to_string(),
            skills: vec![SkillEntry {
                name: "missing-skill".to_string(),
                source_path: "skill.sh".to_string(),
                target_path: "/nonexistent/skill.sh".to_string(),
                sha256: "abcd1234".to_string(),
            }],
        };

        let manifest_path = manifest_dir.join(".installed-manifest.json");
        let json = serde_json::to_string_pretty(&manifest).unwrap();
        fs::write(&manifest_path, json).unwrap();

        with_manifest_path_override(manifest_path, || {
            let result = verify_installed_skills();
            assert!(
                result.is_err(),
                "verify_installed_skills should fail when a skill file is missing"
            );
            assert!(result.unwrap_err().to_string().contains("file not found"));
        });
    }
}
