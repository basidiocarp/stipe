use anyhow::{Context, Result};
use spore::atomic_write_bytes;
use std::fs;
use std::path::{Path, PathBuf};

use super::host_policy::{self, HostConfigScope};
use super::repair::{RepairAction, RepairTier};
use crate::commands::tool_registry::{self, ToolProbe};

fn required_notify_values() -> Vec<String> {
    let mut values = Vec::new();

    // Resolve via spore discovery (PATH-independent) so GUI launchers that lack
    // ~/.local/bin on PATH still find the binary at MCP startup.
    let hyphae_value = tool_registry::find("hyphae")
        .and_then(tool_registry::resolve_binary_path)
        .map_or_else(
            || {
                tracing::warn!("hyphae not found — codex notify entry may fail in GUI launches");
                "hyphae".to_string()
            },
            |p| p.to_string_lossy().into_owned(),
        );
    values.push(hyphae_value);

    values
}

fn optional_cortina_adapter() -> Option<String> {
    // Resolve via spore discovery so the absolute path is used even when
    // ~/.local/bin is absent from the GUI launcher's PATH.
    let cortina_path =
        tool_registry::find("cortina").and_then(tool_registry::resolve_binary_path)?;
    Some(format!(
        "{} adapter codex turn-complete",
        cortina_path.display()
    ))
}

fn hyphae_installed() -> bool {
    tool_registry::find("hyphae")
        .is_some_and(|spec| matches!(tool_registry::probe(spec), ToolProbe::Installed(_)))
}

fn configured_paths() -> Vec<PathBuf> {
    host_policy::codex_notify_config_paths()
        .into_iter()
        .filter(|path| codex_notify_configured_at_path(path))
        .collect()
}

fn load_or_create_config(config_path: &Path) -> Result<toml::Value> {
    if config_path.exists() {
        let content = fs::read_to_string(config_path)
            .with_context(|| format!("reading {}", config_path.display()))?;
        if content.trim().is_empty() {
            Ok(toml::Value::Table(toml::map::Map::new()))
        } else {
            toml::from_str(&content).with_context(|| format!("parsing {}", config_path.display()))
        }
    } else {
        Ok(toml::Value::Table(toml::map::Map::new()))
    }
}

fn write_config(config_path: &Path, root: &toml::Value) -> Result<()> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }

    let content = toml::to_string_pretty(root).context("serializing Codex config")?;
    atomic_write_bytes(config_path, content.as_bytes())
        .with_context(|| format!("writing {}", config_path.display()))?;
    Ok(())
}

/// Check if a notify value is present in the array, handling both bare and absolute path forms.
fn notify_value_present(values: &[toml::Value], required: &str) -> bool {
    for entry in values {
        if let Some(entry_str) = entry.as_str() {
            // Exact match
            if entry_str == required {
                return true;
            }
            // For hyphae, also match bare "hyphae" against any absolute path ending with "/hyphae"
            if required == "hyphae" && entry_str.ends_with("/hyphae") {
                return true;
            }
            if entry_str == "hyphae" && required.ends_with("/hyphae") {
                return true;
            }
            // For other tools, exact match is required (codex-notify is always bare)
        }
    }
    false
}

pub fn codex_notify_configured_at_path(config_path: &Path) -> bool {
    let Ok(root) = load_or_create_config(config_path) else {
        return false;
    };

    let required = required_notify_values();

    root.get("notify")
        .and_then(toml::Value::as_array)
        .is_some_and(|values| {
            required
                .iter()
                .all(|required_val| notify_value_present(values, required_val))
        })
}

pub fn codex_notify_configured() -> bool {
    !configured_paths().is_empty()
}

pub fn install_codex_notify(scope: HostConfigScope, verbose: u8) -> Result<bool> {
    let Some(config_path) = host_policy::codex_notify_config_path(scope) else {
        return Ok(false);
    };

    if !hyphae_installed() {
        return Ok(false);
    }

    let existing = load_or_create_config(&config_path)?;
    let mut root = match existing {
        toml::Value::Table(map) => map,
        _ => toml::map::Map::new(),
    };

    let mut notify = match root.remove("notify") {
        Some(toml::Value::Array(values)) => values,
        Some(toml::Value::String(value)) => vec![toml::Value::String(value)],
        Some(_) | None => Vec::new(),
    };

    let required = required_notify_values();
    let mut changed = false;
    for value in required {
        if !notify_value_present(&notify, &value) {
            notify.push(toml::Value::String(value));
            changed = true;
        }
    }

    // Add cortina adapter if available and not already present
    if let Some(cortina_adapter) = optional_cortina_adapter() {
        if !notify.iter().any(|entry| {
            entry
                .as_str()
                .is_some_and(|s| s.contains("cortina") && s.contains("adapter"))
        }) {
            notify.push(toml::Value::String(cortina_adapter));
            changed = true;
        }
    }

    root.insert("notify".to_string(), toml::Value::Array(notify));

    if changed {
        write_config(&config_path, &toml::Value::Table(root))?;
        if verbose > 0 {
            eprintln!("  Wrote Codex notify adapter to {}", config_path.display());
        }
    }

    Ok(changed)
}

pub fn codex_notify_detail(_configured: bool) -> String {
    let configured = configured_paths();
    let candidate_paths = host_policy::codex_notify_config_paths();
    if !configured.is_empty() {
        format!(
            "Codex host mode already points at Hyphae via notify in {}.",
            host_policy::format_config_path_list(&configured)
        )
    } else if hyphae_installed() {
        format!(
            "Run `stipe host setup codex --scope <{}>` to install Hyphae notify for Codex host mode in {}.",
            host_policy::supported_scope_hint(crate::commands::host_policy::HostMode::Codex),
            host_policy::format_config_path_list(&candidate_paths)
        )
    } else {
        "Hyphae is not installed, so Codex notify registration cannot be completed yet.".to_string()
    }
}

pub fn codex_notify_repair_action() -> RepairAction {
    RepairAction::manual(
        "codex_notify_adapter_setup".to_string(),
        "Configure the Codex notify adapter".to_string(),
        format!(
            "Write notify = [\"hyphae\"] to one of {} and complete Codex host mode.",
            host_policy::format_config_path_list(&host_policy::codex_notify_config_paths())
        ),
        "stipe host setup codex".to_string(),
        vec!["host".to_string(), "setup".to_string(), "codex".to_string()],
        RepairTier::Primary,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_config_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("stipe-{name}-{unique}.toml"))
    }

    #[test]
    fn test_codex_notify_configured_at_path_detects_required_entries_with_extras() {
        let config_path = test_config_path("codex-notify-detect");
        // The check uses codex_notify_configured_at_path which verifies that hyphae is
        // present. It doesn't matter if hyphae is the bare name or an absolute path —
        // the check matches on the string content.

        // Case 1: with bare "hyphae" name
        fs::write(&config_path, r#"notify = ["existing-hook", "hyphae"]"#).unwrap();
        assert!(codex_notify_configured_at_path(&config_path));

        // Case 2: with absolute path (if hyphae is on PATH in test env)
        if let Ok(hyphae_path) = which::which("hyphae") {
            let hyphae_str = hyphae_path.to_string_lossy().to_string();
            fs::write(
                &config_path,
                format!(r#"notify = ["existing-hook", "{hyphae_str}"]"#),
            )
            .unwrap();
            assert!(codex_notify_configured_at_path(&config_path));
        }
    }

    #[test]
    fn test_install_codex_notify_preserves_existing_notify_entries() {
        let config_path = test_config_path("codex-notify-install");
        fs::write(&config_path, r#"notify = ["existing-hook"]"#).unwrap();

        let existing = load_or_create_config(&config_path).unwrap();
        let mut root = match existing {
            toml::Value::Table(map) => map,
            _ => toml::map::Map::new(),
        };
        let mut notify = match root.remove("notify") {
            Some(toml::Value::Array(values)) => values,
            _ => Vec::new(),
        };
        let required = required_notify_values();
        for value in required {
            if !notify
                .iter()
                .any(|entry| entry.as_str() == Some(value.as_str()))
            {
                notify.push(toml::Value::String(value));
            }
        }
        root.insert("notify".to_string(), toml::Value::Array(notify));
        write_config(&config_path, &toml::Value::Table(root)).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("existing-hook"));
        assert!(content.contains("hyphae"));
        assert!(codex_notify_configured_at_path(&config_path));
    }

    #[test]
    fn test_codex_notify_repair_action_points_at_host_setup() {
        let action = codex_notify_repair_action();

        assert_eq!(action.command, "stipe host setup codex");
        assert_eq!(action.args, vec!["host", "setup", "codex"]);
    }
}
