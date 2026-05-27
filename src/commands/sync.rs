use anyhow::{Context, Result};
use colored::Colorize;
use serde::Deserialize;
use spore::atomic_write_bytes;
use std::path::Path;
use tracing::warn;

use super::claude_hooks::{self, TomlHookEntry};
use super::host_policy::{self, HostConfigScope};

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ProjectSection {
    pub name: Option<String>,
    // Parsed for completeness; not yet used in output logic.
    #[allow(dead_code)]
    pub version: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct HookEntry {
    pub event: String,
    #[serde(rename = "match")]
    pub matcher: Option<String>,
    pub command: String,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct PermissionsSection {
    #[serde(default)]
    pub allow_tools: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct NetworkSection {
    #[serde(default)]
    pub denied_domains: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StipeToml {
    #[serde(default)]
    pub project: ProjectSection,
    #[serde(default)]
    pub hooks: Vec<HookEntry>,
    #[serde(default)]
    pub permissions: PermissionsSection,
    #[serde(default)]
    pub network: NetworkSection,
}

const KNOWN_EVENTS: &[&str] = &[
    "PreToolUse",
    "PostToolUse",
    "Stop",
    "SessionEnd",
    "PreCompact",
    "UserPromptSubmit",
];

const STIPE_TOML_TEMPLATE: &str = r#"# stipe.toml — project-local stipe configuration
# Run `stipe sync` to apply this file to .claude/settings.json.
# Run `stipe sync --scaffold` to regenerate this template.

[project]
name = "my-project"

# Hook entries: each [[hooks]] block maps to one entry in .claude/settings.json.
# Supported events: PreToolUse, PostToolUse, Stop, SessionEnd, PreCompact, UserPromptSubmit

[[hooks]]
event = "PreToolUse"
match = "Bash"
command = "/usr/local/bin/cortina hook"
timeout_ms = 5000

[[hooks]]
event = "PostToolUse"
match = "Bash|Write|Edit|MultiEdit"
command = "/usr/local/bin/cortina hook"
timeout_ms = 5000

[[hooks]]
event = "Stop"
command = "/usr/local/bin/cortina hook"
timeout_ms = 5000

[[hooks]]
event = "SessionEnd"
command = "/usr/local/bin/cortina hook"
timeout_ms = 10000

# [permissions]
# allow_tools = ["Bash", "Read", "Write", "Edit"]

# [network]
# denied_domains = ["example-blocked.com"]
"#;

fn load(path: &Path) -> Result<StipeToml> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))
}

fn validate(config: &StipeToml) -> Result<()> {
    for hook in &config.hooks {
        if !KNOWN_EVENTS.contains(&hook.event.as_str()) {
            anyhow::bail!(
                "unknown hook event {:?} in stipe.toml — supported: {}",
                hook.event,
                KNOWN_EVENTS.join(", ")
            );
        }
        if hook.command.trim().is_empty() {
            anyhow::bail!("hook for event {:?} has an empty command", hook.event);
        }
        if let Some(ms) = hook.timeout_ms {
            if !(1000..=300_000).contains(&ms) {
                anyhow::bail!(
                    "timeout_ms {} for event {:?} is out of range (1000–300000)",
                    ms,
                    hook.event
                );
            }
        }
    }
    Ok(())
}

/// Validates a hook command string for shell injection risks.
///
/// Claude Code executes hook commands via a shell (`/bin/sh -c <command>` on
/// Unix), so shell metacharacters in the command string are a real injection
/// surface. This function rejects commands containing characters that enable
/// command chaining, subshell execution, or directory traversal.
///
/// Returns `true` if the command is safe to install, `false` if it should be skipped.
fn validate_hook_command(event: &str, command: &str) -> bool {
    let dangerous_patterns = [";", "&&", "||", "|", "`", "$", "\n", "\r"];

    for pattern in &dangerous_patterns {
        if command.contains(pattern) {
            eprintln!(
                "warning: skipping hook for '{}': command rejected — contains shell injection characters: {}",
                event, command
            );
            warn!(
                "hook for event {}: rejected command containing dangerous pattern {:?}: {}",
                event, pattern, command
            );
            return false;
        }
    }

    // Reject relative paths: commands starting with ./ or ../
    let trimmed = command.trim();
    if trimmed.starts_with("./") || trimmed.starts_with("../") {
        eprintln!(
            "warning: skipping hook for '{}': command rejected — relative paths not allowed: {}",
            event, command
        );
        warn!(
            "hook for event {}: rejected command with relative path: {}",
            event, command
        );
        return false;
    }

    true
}

fn to_hook_entries(hooks: &[HookEntry]) -> Vec<TomlHookEntry> {
    hooks
        .iter()
        .filter_map(|h| {
            if validate_hook_command(&h.event, &h.command) {
                Some(TomlHookEntry {
                    event: h.event.clone(),
                    matcher: h.matcher.clone(),
                    command: h.command.clone(),
                    timeout_secs: h.timeout_ms.unwrap_or(5000) / 1000,
                })
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn run(scaffold: bool) -> Result<()> {
    if scaffold {
        return write_scaffold();
    }

    let project_root = host_policy::project_root().context("cannot determine project root")?;
    let toml_path = project_root.join("stipe.toml");

    if !toml_path.exists() {
        println!(
            "No stipe.toml found — run {} to create one.",
            "`stipe sync --scaffold`".bold()
        );
        return Ok(());
    }

    let config = load(&toml_path)?;
    validate(&config)?;

    let Some(settings_path) = host_policy::claude_hook_settings_path(HostConfigScope::Project)
    else {
        anyhow::bail!("cannot determine project .claude/settings.json path");
    };

    let project_label = config.project.name.as_deref().unwrap_or("project");
    let entries = to_hook_entries(&config.hooks);
    let changed = claude_hooks::sync_toml_hooks(
        &settings_path,
        &entries,
        &config.permissions.allow_tools,
        &config.network.denied_domains,
    )?;

    if changed {
        println!(
            "{} {} synced — {} updated.",
            "✓".green(),
            project_label,
            settings_path.display()
        );
    } else {
        println!(
            "{} {} already in sync — no changes needed.",
            "✓".green(),
            project_label
        );
    }
    Ok(())
}

fn write_scaffold() -> Result<()> {
    let project_root = host_policy::project_root().context("cannot determine project root")?;
    let toml_path = project_root.join("stipe.toml");

    if toml_path.exists() {
        println!(
            "{} stipe.toml already exists at {} — skipping.",
            "→".yellow(),
            toml_path.display()
        );
        return Ok(());
    }

    atomic_write_bytes(&toml_path, STIPE_TOML_TEMPLATE.as_bytes())
        .with_context(|| format!("writing {}", toml_path.display()))?;
    println!(
        "{} Created stipe.toml at {}",
        "✓".green(),
        toml_path.display()
    );
    Ok(())
}

/// Returns `Some(warning)` when the installed config is out of sync with
/// `stipe.toml`. Returns `None` when `stipe.toml` does not exist (no check
/// needed) or when the file cannot be parsed (errors are surfaced by `stipe sync`).
pub(crate) fn check_sync_state() -> Option<String> {
    let project_root = host_policy::project_root()?;
    let toml_path = project_root.join("stipe.toml");
    if !toml_path.exists() {
        return None;
    }

    let config = load(&toml_path).ok()?;
    if validate(&config).is_err() {
        return None;
    }

    let settings_path = host_policy::claude_hook_settings_path(HostConfigScope::Project)?;
    let entries = to_hook_entries(&config.hooks);

    match claude_hooks::toml_sync_diverged(
        &settings_path,
        &entries,
        &config.permissions.allow_tools,
        &config.network.denied_domains,
    ) {
        Ok(true) => {
            Some("stipe.toml and installed config are out of sync — run `stipe sync`".to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_toml(dir: &TempDir, content: &str) {
        fs::write(dir.path().join("stipe.toml"), content).unwrap();
    }

    #[test]
    fn test_validate_rejects_unknown_event() {
        let config = StipeToml {
            project: ProjectSection::default(),
            hooks: vec![HookEntry {
                event: "UnknownEvent".to_string(),
                matcher: None,
                command: "/bin/true".to_string(),
                timeout_ms: None,
            }],
            permissions: PermissionsSection::default(),
            network: NetworkSection::default(),
        };
        assert!(validate(&config).is_err());
    }

    #[test]
    fn test_validate_rejects_empty_command() {
        let config = StipeToml {
            project: ProjectSection::default(),
            hooks: vec![HookEntry {
                event: "PreToolUse".to_string(),
                matcher: None,
                command: "   ".to_string(),
                timeout_ms: None,
            }],
            permissions: PermissionsSection::default(),
            network: NetworkSection::default(),
        };
        assert!(validate(&config).is_err());
    }

    #[test]
    fn test_validate_rejects_out_of_range_timeout() {
        let config = StipeToml {
            project: ProjectSection::default(),
            hooks: vec![HookEntry {
                event: "PreToolUse".to_string(),
                matcher: None,
                command: "/bin/true".to_string(),
                timeout_ms: Some(400_000),
            }],
            permissions: PermissionsSection::default(),
            network: NetworkSection::default(),
        };
        assert!(validate(&config).is_err());
    }

    #[test]
    fn test_validate_rejects_sub_second_timeout() {
        let config = StipeToml {
            project: ProjectSection::default(),
            hooks: vec![HookEntry {
                event: "PreToolUse".to_string(),
                matcher: None,
                command: "/bin/true".to_string(),
                timeout_ms: Some(500),
            }],
            permissions: PermissionsSection::default(),
            network: NetworkSection::default(),
        };
        assert!(
            validate(&config).is_err(),
            "timeout_ms 500 < 1000 should be rejected"
        );
    }

    #[test]
    fn test_load_valid_toml() {
        let dir = TempDir::new().unwrap();
        write_toml(
            &dir,
            r#"
[project]
name = "test"

[[hooks]]
event = "PreToolUse"
match = "Bash"
command = "/usr/bin/true"
timeout_ms = 3000
"#,
        );
        let config = load(&dir.path().join("stipe.toml")).unwrap();
        assert_eq!(config.project.name.as_deref(), Some("test"));
        assert_eq!(config.hooks.len(), 1);
        assert_eq!(config.hooks[0].event, "PreToolUse");
        assert_eq!(config.hooks[0].matcher.as_deref(), Some("Bash"));
        assert_eq!(config.hooks[0].timeout_ms, Some(3000));
    }

    #[test]
    fn test_sync_writes_hook_with_tag() {
        let dir = TempDir::new().unwrap();
        let settings_path = dir.path().join("settings.json");

        let hooks = vec![TomlHookEntry {
            event: "PreToolUse".to_string(),
            matcher: Some("Bash".to_string()),
            command: "/usr/bin/true".to_string(),
            timeout_secs: 5,
        }];

        let changed = claude_hooks::sync_toml_hooks(&settings_path, &hooks, &[], &[]).unwrap();
        assert!(changed, "first sync should write a change");

        let content = fs::read_to_string(&settings_path).unwrap();
        let root: serde_json::Value = serde_json::from_str(&content).unwrap();
        let entries = root["hooks"]["PreToolUse"]
            .as_array()
            .expect("PreToolUse array");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["_tag"], "stipe-managed");
        assert_eq!(entries[0]["matcher"], "Bash");
    }

    #[test]
    fn test_sync_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let settings_path = dir.path().join("settings.json");

        let hooks = vec![TomlHookEntry {
            event: "Stop".to_string(),
            matcher: None,
            command: "/usr/bin/true".to_string(),
            timeout_secs: 5,
        }];

        claude_hooks::sync_toml_hooks(&settings_path, &hooks, &[], &[]).unwrap();
        let after_first = fs::read_to_string(&settings_path).unwrap();

        let changed = claude_hooks::sync_toml_hooks(&settings_path, &hooks, &[], &[]).unwrap();
        let after_second = fs::read_to_string(&settings_path).unwrap();

        assert!(!changed, "second sync should be a no-op");
        assert_eq!(after_first, after_second, "settings must be identical");
    }

    #[test]
    fn test_missing_stipe_toml_exits_cleanly() {
        // check_sync_state must return None when stipe.toml doesn't exist
        host_policy::with_project_root_override(std::env::temp_dir(), || {
            assert!(check_sync_state().is_none());
        });
    }

    #[test]
    fn test_validate_hook_command_accepts_absolute_paths() {
        assert!(validate_hook_command("PreToolUse", "/usr/bin/true"));
        assert!(validate_hook_command(
            "PreToolUse",
            "/usr/local/bin/cortina"
        ));
    }

    #[test]
    fn test_validate_hook_command_rejects_semicolon() {
        assert!(!validate_hook_command("PreToolUse", "echo hello; rm -rf /"));
    }

    #[test]
    fn test_validate_hook_command_rejects_and() {
        assert!(!validate_hook_command("PreToolUse", "cmd1 && cmd2"));
    }

    #[test]
    fn test_validate_hook_command_rejects_or() {
        assert!(!validate_hook_command("PreToolUse", "cmd1 || cmd2"));
    }

    #[test]
    fn test_validate_hook_command_rejects_pipe() {
        assert!(!validate_hook_command("PreToolUse", "cmd1 | cmd2"));
    }

    #[test]
    fn test_validate_hook_command_rejects_backtick() {
        assert!(!validate_hook_command("PreToolUse", "`echo hello`"));
    }

    #[test]
    fn test_validate_hook_command_rejects_dollar_var() {
        assert!(!validate_hook_command("PreToolUse", "$HOME/.local/bin"));
    }

    #[test]
    fn test_validate_hook_command_rejects_dollar_paren() {
        assert!(!validate_hook_command("PreToolUse", "$(rm -rf /)"));
    }

    #[test]
    fn test_validate_hook_command_rejects_dollar_brace() {
        assert!(!validate_hook_command("PreToolUse", "${HOME}/.local/bin"));
    }

    #[test]
    fn test_validate_hook_command_rejects_newline() {
        assert!(!validate_hook_command("PreToolUse", "echo hello\nrm -rf /"));
    }

    #[test]
    fn test_validate_hook_command_rejects_carriage_return() {
        assert!(!validate_hook_command("PreToolUse", "echo hello\rrm -rf /"));
    }

    #[test]
    fn test_validate_hook_command_rejects_relative_path_dot_slash() {
        assert!(!validate_hook_command("PreToolUse", "./cortina"));
    }

    #[test]
    fn test_validate_hook_command_rejects_relative_path_dot_dot_slash() {
        assert!(!validate_hook_command("PreToolUse", "../bin/cortina"));
    }

    #[test]
    fn test_to_hook_entries_filters_invalid_commands() {
        let hooks = vec![
            HookEntry {
                event: "PreToolUse".to_string(),
                matcher: Some("Bash".to_string()),
                command: "/usr/bin/true".to_string(),
                timeout_ms: Some(3000),
            },
            HookEntry {
                event: "PostToolUse".to_string(),
                matcher: None,
                command: "evil; rm -rf /".to_string(),
                timeout_ms: None,
            },
            HookEntry {
                event: "Stop".to_string(),
                matcher: None,
                command: "/bin/false".to_string(),
                timeout_ms: Some(5000),
            },
        ];

        let entries = to_hook_entries(&hooks);
        assert_eq!(entries.len(), 2, "should filter out the malicious command");
        assert_eq!(entries[0].command, "/usr/bin/true");
        assert_eq!(entries[1].command, "/bin/false");
    }
}
