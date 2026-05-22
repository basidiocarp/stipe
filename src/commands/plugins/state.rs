
//! Enable/disable state for lamella plugins.
//!
//! State is persisted at `~/.config/lamella/disabled-plugins.json` as a JSON
//! object with a `disabled` array of plugin name strings.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use spore::atomic_write_bytes;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
struct DisabledState {
    disabled: Vec<String>,
}

fn state_path() -> Result<PathBuf> {
    dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine config directory"))
        .map(|dir| dir.join("lamella").join("disabled-plugins.json"))
}

/// Return the list of disabled plugin names from the state file.
///
/// Returns an empty list if the file does not exist or cannot be parsed.
pub fn load_disabled() -> Vec<String> {
    state_path()
        .ok()
        .and_then(|path| std::fs::read_to_string(&path).ok())
        .and_then(|json| serde_json::from_str::<DisabledState>(&json).ok())
        .map(|state| state.disabled)
        .unwrap_or_default()
}

fn save_disabled(disabled: Vec<String>) -> Result<()> {
    let path = state_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create config dir: {}", parent.display()))?;
    }
    let state = DisabledState { disabled };
    let json = serde_json::to_string_pretty(&state).context("serialize disabled-plugins state")?;
    atomic_write_bytes(&path, json.as_bytes())
        .with_context(|| format!("write disabled-plugins state: {}", path.display()))
}

/// Remove `name` from the disabled list.
pub fn enable(name: &str) -> Result<()> {
    let mut disabled = load_disabled();
    let before = disabled.len();
    disabled.retain(|n| n != name);
    if disabled.len() == before {
        println!("Plugin '{name}' was not disabled.");
    } else {
        save_disabled(disabled)?;
        println!("Plugin '{name}' enabled.");
    }
    Ok(())
}

/// Add `name` to the disabled list.
pub fn disable(name: &str) -> Result<()> {
    let mut disabled = load_disabled();
    if disabled.iter().any(|n| n == name) {
        println!("Plugin '{name}' is already disabled.");
    } else {
        disabled.push(name.to_string());
        save_disabled(disabled)?;
        println!("Plugin '{name}' disabled.");
    }
    Ok(())
}
