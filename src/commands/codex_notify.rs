use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::host_policy::{self, HostConfigScope};
use super::repair::{RepairAction, RepairTier};

const CODEX_NOTIFY_VALUES: [&str; 2] = ["hyphae", "codex-notify"];

fn hyphae_installed() -> bool {
    Command::new("hyphae")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
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

    fs::write(
        config_path,
        toml::to_string_pretty(root).context("serializing Codex config")?,
    )
    .with_context(|| format!("writing {}", config_path.display()))?;
    Ok(())
}

pub fn codex_notify_configured_at_path(config_path: &Path) -> bool {
    let Ok(root) = load_or_create_config(config_path) else {
        return false;
    };

    root.get("notify")
        .and_then(toml::Value::as_array)
        .is_some_and(|values| {
            CODEX_NOTIFY_VALUES.iter().all(|required| {
                values
                    .iter()
                    .any(|entry| entry.as_str().is_some_and(|value| value == *required))
            })
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

    let mut changed = false;
    for value in CODEX_NOTIFY_VALUES {
        if !notify.iter().any(|entry| entry.as_str() == Some(value)) {
            notify.push(toml::Value::String(value.to_string()));
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

    Ok(true)
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
        "Configure the Codex notify adapter".to_string(),
        format!(
            "Write notify = [\"hyphae\", \"codex-notify\"] to one of {} and complete Codex host mode.",
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
        fs::write(
            &config_path,
            r#"notify = ["existing-hook", "hyphae", "codex-notify"]"#,
        )
        .unwrap();

        assert!(codex_notify_configured_at_path(&config_path));
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
        for value in CODEX_NOTIFY_VALUES {
            if !notify.iter().any(|entry| entry.as_str() == Some(value)) {
                notify.push(toml::Value::String(value.to_string()));
            }
        }
        root.insert("notify".to_string(), toml::Value::Array(notify));
        write_config(&config_path, &toml::Value::Table(root)).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("existing-hook"));
        assert!(content.contains("hyphae"));
        assert!(content.contains("codex-notify"));
        assert!(codex_notify_configured_at_path(&config_path));
    }

    #[test]
    fn test_codex_notify_repair_action_points_at_host_setup() {
        let action = codex_notify_repair_action();

        assert_eq!(action.command, "stipe host setup codex");
        assert_eq!(action.args, vec!["host", "setup", "codex"]);
    }
}
