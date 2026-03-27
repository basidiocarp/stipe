use anyhow::{Context, Result};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::host_policy::{self, HostMode};

#[derive(Clone, Copy)]
struct HookSpec {
    event: &'static str,
    matcher: Option<&'static str>,
    subcommand: &'static str,
    status_message: &'static str,
    timeout_secs: u64,
}

const CLAUDE_HOOK_SPECS: [HookSpec; 3] = [
    HookSpec {
        event: "PreToolUse",
        matcher: Some("Bash"),
        subcommand: "pre-tool-use",
        status_message: "Cortina rewriting bash commands",
        timeout_secs: 2,
    },
    HookSpec {
        event: "PostToolUse",
        matcher: Some("Bash|Write|Edit|MultiEdit"),
        subcommand: "post-tool-use",
        status_message: "Cortina capturing lifecycle signals",
        timeout_secs: 2,
    },
    HookSpec {
        event: "Stop",
        matcher: None,
        subcommand: "stop",
        status_message: "Cortina capturing session summary",
        timeout_secs: 2,
    },
];

pub fn cortina_installed() -> bool {
    Command::new("cortina")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn claude_settings_path() -> Option<PathBuf> {
    host_policy::host_config_path(HostMode::ClaudeCode)
}

fn hook_command(spec: HookSpec) -> String {
    format!("cortina adapter claude-code {}", spec.subcommand)
}

fn command_matches(existing: &str, expected: &str) -> bool {
    existing == expected || existing.contains(expected)
}

fn hook_entry_present(root: &serde_json::Value, spec: HookSpec, command: &str) -> bool {
    let Some(entries) = root
        .get("hooks")
        .and_then(|hooks| hooks.get(spec.event))
        .and_then(serde_json::Value::as_array)
    else {
        return false;
    };

    entries.iter().any(|entry| {
        let matcher_matches = spec.matcher.is_none_or(|matcher| {
            entry.get("matcher").and_then(serde_json::Value::as_str) == Some(matcher)
        });
        if !matcher_matches {
            return false;
        }

        entry
            .get("hooks")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|hooks| {
                hooks.iter().any(|hook| {
                    hook.get("command")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|existing| command_matches(existing, command))
                })
            })
    })
}

fn insert_hook_entry(root: &mut serde_json::Value, spec: HookSpec, command: &str) {
    let root_obj = if let Some(obj) = root.as_object_mut() {
        obj
    } else {
        *root = json!({});
        root.as_object_mut()
            .expect("fresh object must be present after initialization")
    };

    let hooks = root_obj
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .expect("hooks must be an object");

    let event_hooks = hooks
        .entry(spec.event)
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("event hook list must be an array");

    let mut entry = serde_json::Map::new();
    if let Some(matcher) = spec.matcher {
        entry.insert(
            "matcher".to_string(),
            serde_json::Value::String(matcher.to_string()),
        );
    }
    entry.insert(
        "hooks".to_string(),
        json!([{
            "type": "command",
            "command": command,
            "timeout": spec.timeout_secs,
            "statusMessage": spec.status_message,
        }]),
    );

    event_hooks.push(serde_json::Value::Object(entry));
}

fn load_or_create_settings(settings_path: &Path) -> Result<serde_json::Value> {
    if settings_path.exists() {
        let content = fs::read_to_string(settings_path)
            .with_context(|| format!("reading {}", settings_path.display()))?;
        if content.trim().is_empty() {
            Ok(json!({}))
        } else {
            serde_json::from_str(&content)
                .with_context(|| format!("parsing {}", settings_path.display()))
        }
    } else {
        Ok(json!({}))
    }
}

fn write_settings(settings_path: &Path, root: &serde_json::Value) -> Result<()> {
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }

    fs::write(
        settings_path,
        serde_json::to_string_pretty(root).context("serializing hook settings")?,
    )
    .with_context(|| format!("writing {}", settings_path.display()))?;
    Ok(())
}

fn install_claude_hooks_at_path(settings_path: &Path) -> Result<bool> {
    let mut root = load_or_create_settings(settings_path)?;
    let mut changed = false;

    for spec in CLAUDE_HOOK_SPECS {
        let command = hook_command(spec);
        if !hook_entry_present(&root, spec, &command) {
            insert_hook_entry(&mut root, spec, &command);
            changed = true;
        }
    }

    if changed {
        write_settings(settings_path, &root)?;
    }

    Ok(true)
}

fn claude_hooks_configured_at_path(settings_path: &Path) -> bool {
    let Ok(root) = load_or_create_settings(settings_path) else {
        return false;
    };

    CLAUDE_HOOK_SPECS.iter().copied().all(|spec| {
        let command = hook_command(spec);
        hook_entry_present(&root, spec, &command)
    })
}

pub fn claude_hooks_configured() -> bool {
    claude_settings_path()
        .as_deref()
        .is_some_and(claude_hooks_configured_at_path)
}

pub fn install_claude_hooks(verbose: u8) -> Result<bool> {
    let Some(settings_path) = claude_settings_path() else {
        return Ok(false);
    };

    if !cortina_installed() {
        return Ok(false);
    }

    let configured = install_claude_hooks_at_path(&settings_path)?;
    if configured && verbose > 0 {
        eprintln!(
            "  Wrote Cortina Claude hooks to {}",
            settings_path.display()
        );
    }
    Ok(configured)
}

pub fn claude_hooks_detail(configured: bool) -> String {
    let path = host_policy::host_config_display_path(HostMode::ClaudeCode);
    if configured {
        format!("Claude Code config exists and Cortina hooks are installed in {path}.")
    } else if cortina_installed() {
        format!("Run `stipe init` to install Cortina Claude hooks in {path}.")
    } else {
        "Cortina is not installed, so Claude hook registration cannot be completed yet.".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_settings_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("stipe-{name}-{unique}.json"))
    }

    #[test]
    fn test_install_claude_hooks_at_path_is_idempotent() {
        let settings_path = test_settings_path("hooks-idempotent");

        install_claude_hooks_at_path(&settings_path).unwrap();
        install_claude_hooks_at_path(&settings_path).unwrap();

        let root: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert!(hook_entry_present(
            &root,
            CLAUDE_HOOK_SPECS[0],
            &hook_command(CLAUDE_HOOK_SPECS[0]),
        ));
        assert!(hook_entry_present(
            &root,
            CLAUDE_HOOK_SPECS[1],
            &hook_command(CLAUDE_HOOK_SPECS[1]),
        ));
        assert!(hook_entry_present(
            &root,
            CLAUDE_HOOK_SPECS[2],
            &hook_command(CLAUDE_HOOK_SPECS[2]),
        ));

        let _ = fs::remove_file(settings_path);
    }

    #[test]
    fn test_claude_hooks_configured_at_path_detects_missing_hook() {
        let settings_path = test_settings_path("hooks-missing");
        fs::write(
            &settings_path,
            json!({
                "hooks": {
                    "PreToolUse": [{
                        "matcher": "Bash",
                        "hooks": [{
                            "type": "command",
                            "command": "cortina adapter claude-code pre-tool-use",
                            "timeout": 2,
                            "statusMessage": "Cortina rewriting bash commands"
                        }]
                    }]
                }
            })
            .to_string(),
        )
        .unwrap();

        assert!(!claude_hooks_configured_at_path(&settings_path));
        let _ = fs::remove_file(settings_path);
    }
}
