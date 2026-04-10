use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::commands::install::InstallProfile;

const BASIDIOCARP_CONFIG_DIR: &str = "basidiocarp";
const PROFILE_CONFIG_FILE: &str = "profile.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SavedInstallProfile {
    pub(crate) profile: InstallProfile,
    pub(crate) path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProfileConfigFile {
    profile: String,
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

    let content = toml::to_string_pretty(&ProfileConfigFile {
        profile: profile.profile_name().to_string(),
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
        Ok(Some(profile)) => Some(SavedInstallProfile { profile, path }),
        Ok(None) | Err(_) => None,
    }
}

#[cfg(test)]
pub(super) fn save_profile_to_path(path: &Path, profile: InstallProfile) -> Result<()> {
    save_profile(path, profile)
}

#[cfg(test)]
pub(super) fn load_profile_from_path(path: &Path) -> Result<Option<InstallProfile>> {
    load_profile(path)
}
