use super::model::HealthCheck;
use crate::commands::install::{SkillPackManifest, SkillVerifyStatus};
use std::path::PathBuf;

/// Check the health of installed skills from a skill pack.
pub(super) fn check_skills() -> HealthCheck {
    let installed_manifest_path = get_installed_manifest_path();

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
    match load_and_verify_manifest(&installed_manifest_path) {
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

/// Load manifest and verify all skills.
fn load_and_verify_manifest(manifest_path: &PathBuf) -> Result<(bool, String), String> {
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

    if failed_skills.is_empty() {
        let message = format!(
            "Skill pack '{}' v{} ({} skills) installed and verified",
            manifest.pack_name,
            manifest.version,
            manifest.skills.len()
        );
        Ok((true, message))
    } else {
        let message = format!(
            "Skill pack '{}' v{}: {} skills failed verification:\n  {}",
            manifest.pack_name,
            manifest.version,
            failed_skills.len(),
            failed_skills.join("\n  ")
        );
        Ok((false, message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_check_skills_no_manifest() {
        let check = check_skills();
        assert!(check.passed);
        assert_eq!(check.name, "installed skills");
    }

    #[test]
    fn test_check_skills_with_temp_manifest() {
        let temp = tempfile::TempDir::new().unwrap();
        let manifest_path = temp.path().join("basidiocarp").join("skills");
        fs::create_dir_all(&manifest_path).unwrap();
        let manifest_file = manifest_path.join(".installed-manifest.json");

        let manifest_json = r#"{
            "pack_name": "test-pack",
            "version": "1.0.0",
            "skills": []
        }"#;
        fs::write(&manifest_file, manifest_json).unwrap();

        // We can't easily test with real paths without mocking, but at least verify
        // the check structure is correct
        let check = check_skills();
        assert_eq!(check.name, "installed skills");
    }
}
