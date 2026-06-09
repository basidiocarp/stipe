use super::model::HealthCheck;
use crate::commands::doctor::version_pins::pinned_ecosystem_versions;
use crate::commands::install::{SkillPackManifest, SkillVerifyStatus};
use std::path::PathBuf;

/// Check the health of installed skills from a skill pack.
pub(super) fn check_skills() -> HealthCheck {
    let mut checks = vec![check_skills_at(&get_installed_manifest_path())];
    checks.push(check_codex_skills_installed());

    let all_passed = checks.iter().all(|c| c.passed);

    if all_passed {
        HealthCheck {
            name: "installed skills".to_string(),
            passed: true,
            message: "All skill checks passed".to_string(),
            repair_actions: Vec::new(),
        }
    } else {
        let failed_messages: Vec<&str> = checks
            .iter()
            .filter(|c| !c.passed)
            .map(|c| c.message.as_str())
            .collect();
        HealthCheck {
            name: "installed skills".to_string(),
            passed: false,
            message: failed_messages.join("; "),
            repair_actions: checks
                .iter()
                .flat_map(|c| c.repair_actions.clone())
                .collect(),
        }
    }
}

fn check_skills_at(installed_manifest_path: &std::path::Path) -> HealthCheck {
    // If no manifest exists, this is OK (no skill pack installed)
    if !installed_manifest_path.exists() {
        return HealthCheck {
            name: "installed skills".to_string(),
            passed: true,
            message: "No skill pack installed".to_string(),
            repair_actions: Vec::new(),
        };
    }

    // Try to load and verify the manifest
    match load_and_verify_manifest(installed_manifest_path) {
        Ok((passed, message)) => HealthCheck {
            name: "installed skills".to_string(),
            passed,
            message,
            repair_actions: Vec::new(),
        },
        Err(e) => HealthCheck {
            name: "installed skills".to_string(),
            passed: false,
            message: format!("Error reading installed skill manifest: {e}"),
            repair_actions: Vec::new(),
        },
    }
}

/// Get the path to the installed skills manifest.
fn get_installed_manifest_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("basidiocarp")
        .join("skills")
        .join(".installed-manifest.json")
}

/// Resolve the expected skill pack version from the single source of truth.
///
/// The skill pack is lamella's product, so the expected pack version is the
/// pinned `lamella` version from the workspace `ecosystem-versions.toml [tools]`
/// table (compiled in via `version_pins.rs`). Resolution order:
///   1. `LAMELLA_SKILL_PACK_VERSION` env var, when set and non-empty (runtime override).
///   2. The `"lamella"` key in `pinned_ecosystem_versions()`.
///
/// Returns `None` when neither source provides a version — for example when the
/// pins table has no `lamella` entry. A `None` result means the version check is
/// skipped entirely (it is not an error and not a panic).
fn canonical_skill_pack_version() -> Option<String> {
    if let Ok(env_version) = std::env::var("LAMELLA_SKILL_PACK_VERSION") {
        if !env_version.is_empty() {
            return Some(env_version);
        }
    }

    pinned_ecosystem_versions()
        .get("lamella")
        .map(|v| (*v).to_string())
}

/// Load manifest and verify all skills.
fn load_and_verify_manifest(manifest_path: &std::path::Path) -> Result<(bool, String), String> {
    let json = std::fs::read_to_string(manifest_path).map_err(|e| format!("read manifest: {e}"))?;

    let manifest: SkillPackManifest =
        serde_json::from_str(&json).map_err(|e| format!("parse manifest: {e}"))?;

    // Verify each skill
    let mut failed_skills = Vec::new();
    for entry in &manifest.skills {
        match SkillVerifyStatus::from_entry(entry) {
            Ok(SkillVerifyStatus::Ok) => {}
            Ok(SkillVerifyStatus::Missing) => {
                failed_skills.push(format!("{}: file not found", entry.name));
            }
            Ok(SkillVerifyStatus::ChecksumMismatch { actual }) => {
                failed_skills.push(format!(
                    "{}: checksum mismatch (expected {}, got {})",
                    entry.name, entry.sha256, actual
                ));
            }
            Err(e) => {
                failed_skills.push(format!("{}: verification error ({})", entry.name, e));
            }
        }
    }

    // Compare the installed skill pack version against the canonical expected
    // version. When no canonical version resolves (no `lamella` pin and no env
    // override), the check is skipped and the note stays empty — this is the
    // reachable `None` arm, not dead code. The mismatch note is advisory only:
    // it never changes the success/fail bool.
    let mut staleness_note = String::new();
    if let Some(canonical_version) = canonical_skill_pack_version() {
        if manifest.version != canonical_version {
            staleness_note = format!(
                " [WARNING: skill pack version mismatch — expected v{}, got v{}]",
                canonical_version, manifest.version
            );
        }
    }

    if failed_skills.is_empty() {
        let message = format!(
            "Skill pack '{}' v{} ({} skills) installed and verified{}",
            manifest.pack_name,
            manifest.version,
            manifest.skills.len(),
            staleness_note
        );
        Ok((true, message))
    } else {
        let message = format!(
            "Skill pack '{}' v{}: {} skills failed verification:\n  {}{}",
            manifest.pack_name,
            manifest.version,
            failed_skills.len(),
            failed_skills.join("\n  "),
            staleness_note
        );
        Ok((false, message))
    }
}

/// Check if codex skills are installed.
fn check_codex_skills_installed() -> HealthCheck {
    let skills_dir = dirs::home_dir()
        .map(|h| h.join(".codex/skills"))
        .unwrap_or_default();
    check_codex_skills_at(&skills_dir)
}

fn check_codex_skills_at(skills_dir: &std::path::Path) -> HealthCheck {
    if !skills_dir.exists() {
        return HealthCheck {
            name: "codex skills".to_string(),
            passed: false,
            message: "No codex skills installed — run 'stipe host setup codex' or 'lamella install-codex'".to_string(),
            repair_actions: Vec::new(),
        };
    }

    let has_profiles = std::fs::read_dir(skills_dir).is_ok_and(|rd| rd.count() > 0);

    if !has_profiles {
        return HealthCheck {
            name: "codex skills".to_string(),
            passed: false,
            message: "~/.codex/skills/ exists but is empty — run 'lamella install-codex'"
                .to_string(),
            repair_actions: Vec::new(),
        };
    }

    HealthCheck {
        name: "codex skills".to_string(),
        passed: true,
        message: "Codex skills are installed".to_string(),
        repair_actions: Vec::new(),
    }
}

#[cfg(test)]
#[allow(unsafe_code)] // set_var is unsafe in Rust 2024; serialized via ENV_LOCK
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;

    // Global lock to serialize tests that modify environment variables.
    // Ensures that multiple tests modifying LAMELLA_SKILL_PACK_VERSION don't interfere.
    // Note: this lock only serializes tests within this module; it does not guard
    // against tests in other modules reading the same env var concurrently.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_check_skills_no_manifest() {
        let temp = tempfile::TempDir::new().unwrap();
        let missing = temp.path().join("nonexistent.json");
        let check = check_skills_at(&missing);
        assert!(check.passed);
        assert_eq!(check.name, "installed skills");
        assert_eq!(check.message, "No skill pack installed");
    }

    #[test]
    fn test_check_skills_with_empty_manifest() {
        let temp = tempfile::TempDir::new().unwrap();
        let manifest_file = temp.path().join(".installed-manifest.json");

        let manifest_json = r#"{
            "pack_name": "test-pack",
            "version": "1.0.0",
            "skills": []
        }"#;
        fs::write(&manifest_file, manifest_json).unwrap();

        let check = check_skills_at(&manifest_file);
        assert!(check.passed);
        assert_eq!(check.name, "installed skills");
        assert!(check.message.contains("test-pack"));
    }

    #[test]
    fn test_check_codex_skills_no_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let skills_dir = temp.path().join(".codex/skills");
        // Directory does not exist yet
        let check = check_codex_skills_at(&skills_dir);
        assert_eq!(check.name, "codex skills");
        assert!(!check.passed);
        assert!(check.message.contains("No codex skills installed"));
    }

    #[test]
    fn test_check_codex_skills_empty_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let skills_dir = temp.path().join(".codex/skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let check = check_codex_skills_at(&skills_dir);
        assert_eq!(check.name, "codex skills");
        assert!(!check.passed);
        assert!(check.message.contains("empty"));
    }

    #[test]
    fn test_check_codex_skills_populated_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let skills_dir = temp.path().join(".codex/skills");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::write(skills_dir.join("some-skill.md"), "# skill").unwrap();
        let check = check_codex_skills_at(&skills_dir);
        assert_eq!(check.name, "codex skills");
        assert!(check.passed);
    }

    #[test]
    fn test_canonical_skill_pack_version_uses_env_when_set() {
        let _guard = ENV_LOCK.lock().unwrap();
        // A set, non-empty env var takes precedence over the pinned value.
        // SAFETY: test-only; env access for LAMELLA_SKILL_PACK_VERSION is serialized via ENV_LOCK.
        unsafe {
            std::env::set_var("LAMELLA_SKILL_PACK_VERSION", "2.0.0");
        }
        let version = canonical_skill_pack_version();
        // SAFETY: test-only; serialized via ENV_LOCK.
        unsafe {
            std::env::remove_var("LAMELLA_SKILL_PACK_VERSION");
        }
        assert_eq!(version, Some("2.0.0".to_string()));
    }

    #[test]
    fn test_canonical_skill_pack_version_falls_back_to_pin() {
        let _guard = ENV_LOCK.lock().unwrap();
        // With no env override, the version resolves from the pinned `lamella`
        // entry in the generated ecosystem-versions map. This documents the real
        // single source of truth: whichever outcome the generated map produces is
        // the expected behavior — present means a value, absent means skipped.
        // SAFETY: test-only; serialized via ENV_LOCK.
        unsafe {
            std::env::remove_var("LAMELLA_SKILL_PACK_VERSION");
        }
        let resolved = canonical_skill_pack_version();
        let pinned = pinned_ecosystem_versions()
            .get("lamella")
            .map(|v| (*v).to_string());
        assert_eq!(resolved, pinned);
    }

    #[test]
    fn test_canonical_skill_pack_version_ignores_empty_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        // An empty env var is treated as unset and falls back to the pin.
        // SAFETY: test-only; serialized via ENV_LOCK.
        unsafe {
            std::env::set_var("LAMELLA_SKILL_PACK_VERSION", "");
        }
        let resolved = canonical_skill_pack_version();
        // SAFETY: test-only; serialized via ENV_LOCK.
        unsafe {
            std::env::remove_var("LAMELLA_SKILL_PACK_VERSION");
        }
        let pinned = pinned_ecosystem_versions()
            .get("lamella")
            .map(|v| (*v).to_string());
        assert_eq!(resolved, pinned);
    }

    #[test]
    fn test_load_and_verify_manifest_matching_version() {
        let _guard = ENV_LOCK.lock().unwrap();
        // Drive the canonical version deterministically via the env override so
        // the manifest can be authored to match it regardless of the pin value.
        // SAFETY: test-only; serialized via ENV_LOCK.
        unsafe {
            std::env::set_var("LAMELLA_SKILL_PACK_VERSION", "3.1.4");
        }
        let temp = tempfile::TempDir::new().unwrap();
        let manifest_file = temp.path().join(".installed-manifest.json");
        let manifest_json = r#"{
            "pack_name": "test-pack",
            "version": "3.1.4",
            "skills": []
        }"#;
        fs::write(&manifest_file, manifest_json).unwrap();

        let result = load_and_verify_manifest(&manifest_file);
        // SAFETY: test-only; serialized via ENV_LOCK.
        unsafe {
            std::env::remove_var("LAMELLA_SKILL_PACK_VERSION");
        }

        assert!(result.is_ok());
        let (passed, message) = result.unwrap();
        assert!(passed);
        // When versions match, there is no advisory note.
        assert!(!message.contains("WARNING"));
        assert!(!message.contains("version mismatch"));
    }

    #[test]
    fn test_load_and_verify_manifest_mismatched_version() {
        let _guard = ENV_LOCK.lock().unwrap();
        // Force a known canonical version, then install a different one.
        // SAFETY: test-only; serialized via ENV_LOCK.
        unsafe {
            std::env::set_var("LAMELLA_SKILL_PACK_VERSION", "3.1.4");
        }
        let temp = tempfile::TempDir::new().unwrap();
        let manifest_file = temp.path().join(".installed-manifest.json");
        let manifest_json = r#"{
            "pack_name": "test-pack",
            "version": "0.7.0",
            "skills": []
        }"#;
        fs::write(&manifest_file, manifest_json).unwrap();

        let result = load_and_verify_manifest(&manifest_file);
        // SAFETY: test-only; serialized via ENV_LOCK.
        unsafe {
            std::env::remove_var("LAMELLA_SKILL_PACK_VERSION");
        }

        assert!(result.is_ok());
        let (passed, message) = result.unwrap();
        // The mismatch is advisory: success bool is unchanged.
        assert!(passed);
        assert!(message.contains("WARNING"));
        assert!(message.contains("version mismatch"));
        assert!(message.contains("expected v3.1.4"));
        assert!(message.contains("got v0.7.0"));
    }

    #[test]
    fn test_load_and_verify_manifest_no_note_when_version_unresolvable() {
        let _guard = ENV_LOCK.lock().unwrap();
        // Exercise the reachable `None` arm. We can only reach it here when the
        // pins map has no `lamella` entry; if it does, the env override cannot
        // simulate absence, so this test asserts the contract conditionally:
        //   - pin absent  -> no note regardless of installed version (None arm)
        //   - pin present -> the matching-version case yields no note
        // SAFETY: test-only; serialized via ENV_LOCK.
        unsafe {
            std::env::remove_var("LAMELLA_SKILL_PACK_VERSION");
        }
        let canonical = canonical_skill_pack_version();
        let temp = tempfile::TempDir::new().unwrap();
        let manifest_file = temp.path().join(".installed-manifest.json");

        // Choose an installed version equal to the canonical one when it exists,
        // so the expected outcome is "no note" in both branches.
        let installed = canonical.clone().unwrap_or_else(|| "0.0.1".to_string());
        let manifest_json = format!(
            r#"{{
            "pack_name": "test-pack",
            "version": "{installed}",
            "skills": []
        }}"#
        );
        fs::write(&manifest_file, manifest_json).unwrap();

        let result = load_and_verify_manifest(&manifest_file);
        assert!(result.is_ok());
        let (passed, message) = result.unwrap();
        assert!(passed);
        assert!(!message.contains("WARNING"));
        assert!(!message.contains("version mismatch"));
    }

    #[test]
    fn test_load_and_verify_manifest_with_failed_skills_and_version_mismatch() {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: test-only; serialized via ENV_LOCK.
        unsafe {
            std::env::set_var("LAMELLA_SKILL_PACK_VERSION", "3.1.4");
        }
        let temp = tempfile::TempDir::new().unwrap();
        let manifest_file = temp.path().join(".installed-manifest.json");
        let manifest_json = r#"{
            "pack_name": "test-pack",
            "version": "0.6.0",
            "skills": [
                {
                    "name": "test-skill",
                    "source_path": "test.md",
                    "target_path": "~/.codex/skills/test.md",
                    "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                }
            ]
        }"#;
        fs::write(&manifest_file, manifest_json).unwrap();

        let result = load_and_verify_manifest(&manifest_file);
        // SAFETY: test-only; serialized via ENV_LOCK.
        unsafe {
            std::env::remove_var("LAMELLA_SKILL_PACK_VERSION");
        }

        assert!(result.is_ok());
        let (passed, message) = result.unwrap();
        // Skill verification fails (ChecksumMismatch/Missing path stays reachable).
        assert!(!passed);
        // The advisory note is still appended alongside the failure detail.
        assert!(message.contains("WARNING"));
        assert!(message.contains("version mismatch"));
    }
}
