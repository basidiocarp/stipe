use super::model::HealthCheck;
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
mod tests {
    use super::*;
    use std::fs;

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
}
