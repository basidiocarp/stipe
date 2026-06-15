use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::commands::install::InstallProfile;

const BASIDIOCARP_CONFIG_DIR: &str = "basidiocarp";
const PROFILE_CONFIG_FILE: &str = "profile.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SavedInstallProfile {
    pub(crate) profile: InstallProfile,
    pub(crate) path: PathBuf,
    pub(crate) suppressed_checks: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct DoctorConfig {
    #[serde(default)]
    suppress: BTreeMap<String, bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProfileConfigFile {
    profile: String,
    #[serde(default)]
    doctor: DoctorConfig,
}

fn profile_config_path() -> Option<PathBuf> {
    current_config_dir().map(|dir| dir.join(BASIDIOCARP_CONFIG_DIR).join(PROFILE_CONFIG_FILE))
}

fn current_config_dir() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(path) = test_config_dir_override() {
        return Some(path);
    }

    dirs::config_dir()
}

#[cfg(test)]
thread_local! {
    static TEST_CONFIG_DIR_OVERRIDE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn test_config_dir_override() -> Option<PathBuf> {
    TEST_CONFIG_DIR_OVERRIDE.with(|path| path.borrow().clone())
}

#[cfg(test)]
pub(crate) fn with_config_dir_override<T>(root: PathBuf, f: impl FnOnce() -> T) -> T {
    TEST_CONFIG_DIR_OVERRIDE.with(|path| {
        let previous = path.replace(Some(root));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        path.replace(previous);
        match result {
            Ok(value) => value,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    })
}

fn save_profile(path: &Path, profile: InstallProfile) -> Result<()> {
    let parent = path
        .parent()
        .context("profile config path should have a parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;

    // Read existing file to preserve [doctor] section
    let existing_doctor = if path.exists() {
        let content = fs::read_to_string(path).ok();
        content
            .and_then(|c| toml::from_str::<ProfileConfigFile>(&c).ok())
            .map(|f| f.doctor)
            .unwrap_or_default()
    } else {
        DoctorConfig::default()
    };

    let content = toml::to_string_pretty(&ProfileConfigFile {
        profile: profile.profile_name().to_string(),
        doctor: existing_doctor,
    })
    .context("serializing install profile")?;

    fs::write(path, content).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn load_profile(path: &Path) -> Result<Option<InstallProfile>> {
    if !path.exists() {
        return Ok(None);
    }

    let content =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let parsed: ProfileConfigFile =
        toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;

    InstallProfile::from_profile_name(parsed.profile.trim())
        .map(Some)
        .with_context(|| {
            format!(
                "unknown install profile '{}' in {}",
                parsed.profile,
                path.display()
            )
        })
}

pub(crate) fn save_selected_profile(profile: InstallProfile) -> Result<Option<PathBuf>> {
    let Some(path) = profile_config_path() else {
        return Ok(None);
    };

    save_profile(&path, profile)?;
    Ok(Some(path))
}

pub(crate) fn load_saved_profile() -> Option<SavedInstallProfile> {
    let path = profile_config_path()?;
    match load_profile(&path) {
        Ok(Some(profile)) => {
            let suppressed_checks = load_suppressed_checks(&path).unwrap_or_default();
            Some(SavedInstallProfile {
                profile,
                path,
                suppressed_checks,
            })
        }
        Ok(None) | Err(_) => None,
    }
}

pub(crate) fn load_suppressed_checks(path: &Path) -> Result<Vec<String>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let parsed: ProfileConfigFile =
        toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;

    Ok(parsed
        .doctor
        .suppress
        .into_iter()
        .filter(|(_, v)| *v)
        .map(|(k, _)| k)
        .collect())
}

pub(crate) fn set_doctor_suppression(path: &Path, slug: &str, value: bool) -> Result<()> {
    let parent = path
        .parent()
        .context("profile config path should have a parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;

    // Load existing file or start with defaults
    let mut config = if path.exists() {
        let content =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        toml::from_str::<ProfileConfigFile>(&content)
            .with_context(|| format!("parsing {}", path.display()))?
    } else {
        return Err(anyhow::anyhow!(
            "no profile config at {}; run `stipe setup` first before suppressing checks",
            path.display()
        ));
    };

    // Update the suppression
    config.doctor.suppress.insert(slug.to_string(), value);

    let content = toml::to_string_pretty(&config).context("serializing config")?;
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn save_profile_to_path(path: &Path, profile: InstallProfile) -> Result<()> {
    save_profile(path, profile)
}

#[cfg(test)]
pub(crate) fn load_profile_from_path(path: &Path) -> Result<Option<InstallProfile>> {
    load_profile(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("stipe-profile-config-{label}-{nonce}"))
    }

    #[test]
    fn test_suppression_round_trip() {
        let temp_dir = unique_test_dir("suppression-round-trip");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let profile_path = temp_dir.join("profile.toml");

        // Save initial profile
        save_profile_to_path(&profile_path, InstallProfile::Standard).expect("Should save profile");

        // Set a suppression
        set_doctor_suppression(&profile_path, "test-check", true).expect("Should set suppression");

        // Load and verify the profile still has the correct name
        let loaded = load_profile_from_path(&profile_path)
            .expect("Should load profile")
            .expect("Profile should exist");
        assert_eq!(
            loaded.profile_name(),
            "standard",
            "Profile name should survive round-trip"
        );

        // Load and verify suppression survived
        let suppressed =
            load_suppressed_checks(&profile_path).expect("Should load suppressed checks");
        assert!(
            suppressed.contains(&"test-check".to_string()),
            "Suppression should survive round-trip"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_backward_compatible_no_doctor_section() {
        let temp_dir = unique_test_dir("backward-compat");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let profile_path = temp_dir.join("profile.toml");
        // Create a profile.toml with no [doctor] section
        fs::write(&profile_path, "profile = \"standard\"\n").unwrap();

        // Load suppressed checks from the profile
        let suppressed = load_suppressed_checks(&profile_path)
            .expect("Should not error when [doctor] section is missing");

        assert!(
            suppressed.is_empty(),
            "Should have empty suppression list for profile without [doctor] section"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
