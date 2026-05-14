use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::json;
use spore::atomic_write_bytes;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::host_policy::{self, HostConfigScope, HostMode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HookEntrySnapshot {
    pub(crate) event: String,
    pub(crate) matcher: Option<String>,
    pub(crate) hook_type: String,
    pub(crate) command: String,
    pub(crate) timeout: u64,
    pub(crate) status_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct HookPathSnapshot {
    pub(crate) event: String,
    pub(crate) path: PathBuf,
    pub(crate) passed: bool,
}

#[derive(Clone, Copy)]
struct HookSpec {
    event: &'static str,
    matcher: Option<&'static str>,
    subcommand: &'static str,
    status_message: &'static str,
    timeout_secs: u64,
}

const CLAUDE_HOOK_SPECS: [HookSpec; 6] = [
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
    HookSpec {
        event: "SessionEnd",
        matcher: None,
        subcommand: "session-end",
        status_message: "Cortina capturing session end",
        timeout_secs: 10,
    },
    // SessionStart: not yet registered — cortina has no SessionStart handler.
    // Track in cortina handoff: session-lifecycle-hooks follow-up.
    HookSpec {
        event: "PreCompact",
        matcher: Some("*"),
        subcommand: "pre-compact",
        status_message: "Cortina capturing compaction snapshots",
        timeout_secs: 10,
    },
    HookSpec {
        event: "UserPromptSubmit",
        matcher: Some("*"),
        subcommand: "user-prompt-submit",
        status_message: "Cortina capturing submitted prompts",
        timeout_secs: 10,
    },
];

const CORTINA_STATUSLINE_COMMAND: &str = "cortina statusline";
const ANNULUS_STATUSLINE_COMMAND: &str = "annulus statusline";

const DEFAULT_ANNULUS_CONFIG: &str = "\
# Annulus statusline configuration
# Run `annulus statusline --help` for available options.

# Provider auto-detection is the default.
# Uncomment to lock to a specific provider:
# provider = \"claude\"
";

pub fn cortina_installed() -> bool {
    Command::new("cortina")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn annulus_available() -> bool {
    Command::new("annulus")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn configured_paths() -> Vec<PathBuf> {
    host_policy::claude_hook_settings_paths()
        .into_iter()
        .filter(|path| claude_hooks_configured_at_path(path))
        .collect()
}

/// Resolve a binary name to its absolute path if available on PATH, with a
/// fallback to the bare name. Emits a warning when the binary is not found so
/// the operator knows the hook may not fire from GUI apps that lack ~/.local/bin.
fn resolve_binary_path(binary_name: &str) -> String {
    if let Ok(path) = which::which(binary_name) {
        path.to_string_lossy().into_owned()
    } else {
        eprintln!(
            "  warning: {binary_name} not found on PATH — hook registered with bare name; \
             add ~/.local/bin to PATH to ensure hooks fire from GUI apps"
        );
        binary_name.to_string()
    }
}

fn hook_command(spec: HookSpec) -> String {
    let cortina = resolve_binary_path("cortina");
    format!("{cortina} adapter claude-code {}", spec.subcommand)
}

fn statusline_command() -> String {
    if annulus_available() {
        resolve_binary_path("annulus") + " statusline"
    } else {
        resolve_binary_path("cortina") + " statusline"
    }
}

/// Match a hook command entry against an expected command.
///
/// Handles two forms:
/// - Absolute-path form:  `/path/to/cortina adapter claude-code pre-tool-use`
/// - Legacy bare form:    `cortina adapter claude-code pre-tool-use`
///
/// Matching on the subcommand suffix rather than a plain substring prevents
/// false positives (one subcommand accidentally matching another) and false
/// negatives (absolute-path entries not matching a bare expected string).
fn command_matches(existing: &str, expected: &str) -> bool {
    if existing == expected {
        return true;
    }

    // Extract the subcommand suffix after "adapter claude-code " from the
    // expected command, then verify the existing command ends with that suffix.
    // This handles both bare names and absolute-path forms uniformly.
    if let Some(suffix) = expected.strip_prefix("cortina adapter claude-code ") {
        let target = format!("adapter claude-code {suffix}");
        return existing.ends_with(&target);
    }

    // For statusline-style commands, match on the trailing keyword phrase so
    // that "/usr/local/bin/annulus statusline" matches "annulus statusline".
    for marker in &["cortina statusline", "annulus statusline"] {
        if expected.contains(marker) && existing.contains(marker) {
            return true;
        }
    }

    false
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

fn statusline_configured(root: &serde_json::Value) -> bool {
    root.get("statusLine").is_some_and(|status_line| {
        status_line.get("type").and_then(serde_json::Value::as_str) == Some("command")
            && status_line
                .get("command")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|existing| {
                    command_matches(existing, CORTINA_STATUSLINE_COMMAND)
                        || command_matches(existing, ANNULUS_STATUSLINE_COMMAND)
                })
    })
}

fn annulus_statusline_configured(root: &serde_json::Value) -> bool {
    root.get("statusLine").is_some_and(|status_line| {
        status_line.get("type").and_then(serde_json::Value::as_str) == Some("command")
            && status_line
                .get("command")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|existing| command_matches(existing, ANNULUS_STATUSLINE_COMMAND))
    })
}

fn install_statusline(root: &mut serde_json::Value, command: &str) {
    let root_obj = if let Some(obj) = root.as_object_mut() {
        obj
    } else {
        *root = json!({});
        root.as_object_mut()
            .expect("fresh object must be present after initialization")
    };

    root_obj.insert(
        "statusLine".to_string(),
        json!({
            "type": "command",
            "command": command,
        }),
    );
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
    let content = serde_json::to_string_pretty(root).context("serializing hook settings")?;
    // atomic_write_bytes creates parent directories and renames into place,
    // preventing corruption if the process is interrupted mid-write.
    atomic_write_bytes(settings_path, content.as_bytes())
        .with_context(|| format!("writing {}", settings_path.display()))
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

    let mut wrote_annulus = false;
    if !statusline_configured(&root) {
        let cmd = statusline_command();
        if annulus_available() {
            wrote_annulus = true;
        }
        install_statusline(&mut root, &cmd);
        changed = true;
    } else if !annulus_statusline_configured(&root) && annulus_available() {
        // Upgrade: cortina statusline → annulus statusline
        let cmd = resolve_binary_path("annulus") + " statusline";
        install_statusline(&mut root, &cmd);
        wrote_annulus = true;
        changed = true;
    }

    if changed {
        write_settings(settings_path, &root)?;
    }
    if wrote_annulus {
        ensure_annulus_config()?;
    }

    Ok(changed)
}

fn claude_hooks_configured_at_path(settings_path: &Path) -> bool {
    let Ok(root) = load_or_create_settings(settings_path) else {
        return false;
    };

    CLAUDE_HOOK_SPECS.iter().copied().all(|spec| {
        let command = hook_command(spec);
        hook_entry_present(&root, spec, &command)
    }) && statusline_configured(&root)
}

pub(crate) fn hook_entries_at_path(settings_path: &Path) -> Result<Vec<HookEntrySnapshot>> {
    let root = load_or_create_settings(settings_path)?;
    let mut entries = Vec::new();

    let Some(hooks) = root.get("hooks").and_then(serde_json::Value::as_object) else {
        return Ok(entries);
    };

    for (event, event_entries) in hooks {
        let Some(event_entries) = event_entries.as_array() else {
            continue;
        };

        for entry in event_entries {
            let matcher = entry
                .get("matcher")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned);
            let Some(command_entries) = entry.get("hooks").and_then(serde_json::Value::as_array)
            else {
                continue;
            };

            for command_entry in command_entries {
                let Some(command) = command_entry
                    .get("command")
                    .and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                let hook_type = command_entry
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("command");
                let timeout = command_entry
                    .get("timeout")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or_default();
                let status_message = command_entry
                    .get("statusMessage")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();

                entries.push(HookEntrySnapshot {
                    event: event.clone(),
                    matcher: matcher.clone(),
                    hook_type: hook_type.to_string(),
                    command: command.to_string(),
                    timeout,
                    status_message: status_message.to_string(),
                });
            }
        }
    }

    Ok(entries)
}

/// Canonical path where Claude Code resolves `${CLAUDE_PLUGIN_ROOT}` for lamella hooks.
fn lamella_plugin_root() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("LAMELLA_HOME") {
        let p = PathBuf::from(home);
        if p.exists() {
            return Some(p);
        }
    }
    dirs::home_dir().map(|h| h.join(".claude").join("plugins").join("lamella"))
}

fn extract_hook_path(command: &str) -> Option<PathBuf> {
    let home = dirs::home_dir();
    let plugin_root = lamella_plugin_root();

    for token in command.split_whitespace() {
        let candidate = token.trim_matches(|ch| matches!(ch, '"' | '\''));

        // Resolve ${CLAUDE_PLUGIN_ROOT} — Claude Code substitutes this at hook run-time.
        // After substitution the result is already absolute, so return it directly to avoid
        // the Unix-only starts_with('/') check below failing on Windows.
        if candidate.contains("${CLAUDE_PLUGIN_ROOT}") {
            let Some(ref root) = plugin_root else {
                continue;
            };
            let resolved = candidate.replace("${CLAUDE_PLUGIN_ROOT}", &root.to_string_lossy());
            let path = PathBuf::from(resolved);
            if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| matches!(ext, "js" | "sh" | "py"))
            {
                return Some(path);
            }
            continue;
        }

        let path = if let Some(suffix) = candidate.strip_prefix("$HOME/") {
            home.as_ref().map(|home| home.join(suffix))
        } else if let Some(suffix) = candidate.strip_prefix("~/") {
            home.as_ref().map(|home| home.join(suffix))
        } else if candidate.starts_with('/') {
            Some(PathBuf::from(candidate))
        } else {
            None
        };

        let Some(path) = path else {
            continue;
        };

        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext, "js" | "sh" | "py"))
        {
            return Some(path);
        }
    }

    None
}

pub(crate) fn hook_path_snapshots() -> Vec<HookPathSnapshot> {
    let mut snapshots = Vec::new();

    for settings_path in host_policy::claude_hook_settings_paths() {
        let Ok(entries) = hook_entries_at_path(&settings_path) else {
            continue;
        };

        for entry in entries {
            let Some(path) = extract_hook_path(&entry.command) else {
                continue;
            };

            snapshots.push(HookPathSnapshot {
                event: entry.event,
                path: path.clone(),
                passed: path.exists(),
            });
        }
    }

    snapshots.sort_by(|left, right| {
        left.event
            .cmp(&right.event)
            .then(left.path.cmp(&right.path))
    });
    snapshots.dedup_by(|left, right| left.event == right.event && left.path == right.path);
    snapshots
}

pub fn claude_hooks_configured() -> bool {
    !configured_paths().is_empty()
}

pub fn install_claude_hooks(scope: HostConfigScope, verbose: u8) -> Result<bool> {
    let Some(settings_path) = host_policy::claude_hook_settings_path(scope) else {
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

pub fn claude_hooks_detail(_configured: bool) -> String {
    let configured = configured_paths();
    let candidate_paths = host_policy::claude_hook_settings_paths();
    let statusline_label = if annulus_statusline_is_configured() {
        "Annulus"
    } else {
        "Cortina"
    };
    if !configured.is_empty() {
        format!(
            "Claude Code hooks and {statusline_label} statusline are installed in {}.",
            host_policy::format_config_path_list(&configured)
        )
    } else if cortina_installed() {
        format!(
            "Run `stipe host setup claude-code --scope <{}>` to install Claude hooks and statusline in {}.",
            host_policy::supported_scope_hint(HostMode::ClaudeCode),
            host_policy::format_config_path_list(&candidate_paths)
        )
    } else {
        "Cortina is not installed, so Claude hook registration cannot be completed yet.".to_string()
    }
}

fn annulus_statusline_configured_at_path(settings_path: &Path) -> bool {
    let Ok(root) = load_or_create_settings(settings_path) else {
        return false;
    };
    annulus_statusline_configured(&root)
}

pub fn annulus_statusline_is_configured() -> bool {
    host_policy::claude_hook_settings_paths()
        .iter()
        .any(|path| annulus_statusline_configured_at_path(path))
}

fn ensure_annulus_config() -> Result<()> {
    let config_dir = dirs::config_dir().map(|d| d.join("annulus"));
    let Some(config_dir) = config_dir else {
        return Ok(());
    };
    let config_path = config_dir.join("statusline.toml");
    if config_path.exists() {
        return Ok(());
    }
    fs::create_dir_all(&config_dir)
        .with_context(|| format!("creating annulus config directory {}", config_dir.display()))?;
    fs::write(&config_path, DEFAULT_ANNULUS_CONFIG)
        .with_context(|| format!("writing default annulus config {}", config_path.display()))?;
    Ok(())
}

pub fn install_annulus_statusline(scope: HostConfigScope, verbose: u8) -> Result<bool> {
    let Some(settings_path) = host_policy::claude_hook_settings_path(scope) else {
        return Ok(false);
    };

    let mut root = load_or_create_settings(&settings_path)?;

    if annulus_statusline_configured(&root) {
        return Ok(false);
    }

    let cmd = resolve_binary_path("annulus") + " statusline";
    install_statusline(&mut root, &cmd);
    write_settings(&settings_path, &root)?;
    ensure_annulus_config()?;

    if verbose > 0 {
        eprintln!("  Wrote annulus statusline to {}", settings_path.display());
    }

    Ok(true)
}

/// Check lamella hook path staleness by running the lamella validator script
pub(crate) fn lamella_hook_path_snapshots() -> Vec<HookPathSnapshot> {
    let mut snapshots = Vec::new();

    // Try to find the lamella validator script in order of preference:
    // 1. LAMELLA_HOME env var (if set)
    // 2. Common install locations
    // 3. $PATH search for 'lamella-validate-hooks' wrapper (future-proofing)

    let validator_script = if let Ok(lamella_home) = std::env::var("LAMELLA_HOME") {
        // Check LAMELLA_HOME/scripts/validate-hooks.js
        let path = PathBuf::from(&lamella_home).join("scripts/validate-hooks.js");
        if path.exists() { Some(path) } else { None }
    } else {
        None
    };

    let validator_script = validator_script.or_else(|| {
        // Try common install locations
        let candidates = [
            "~/.lamella/scripts/validate-hooks.js",
            "~/.local/share/lamella/scripts/validate-hooks.js",
            "~/.config/lamella/scripts/validate-hooks.js",
        ];

        for candidate in &candidates {
            let path = if let Some(stripped) = candidate.strip_prefix("~/") {
                if let Some(home) = dirs::home_dir() {
                    home.join(stripped)
                } else {
                    continue;
                }
            } else {
                PathBuf::from(candidate)
            };

            if path.exists() {
                return Some(path);
            }
        }

        None
    });

    // If no script found in standard locations, check $PATH for 'lamella-validate-hooks'
    let validator_script = validator_script.or_else(|| {
        if Command::new("which")
            .arg("lamella-validate-hooks")
            .output()
            .is_ok_and(|output| output.status.success())
        {
            Some(PathBuf::from("lamella-validate-hooks"))
        } else {
            None
        }
    });

    // Run the validator if found
    if let Some(validator_path) = validator_script {
        // Validate the script path is within home directory before execution
        let Some(safe_prefix) = dirs::home_dir() else {
            return snapshots;
        };

        // Try to canonicalize; if that fails, compare the path components directly
        let canonical =
            std::fs::canonicalize(&validator_path).unwrap_or_else(|_| validator_path.clone());
        if !canonical.starts_with(&safe_prefix) {
            return snapshots;
        }

        if let Ok(output) = Command::new("node").arg(&validator_path).output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = format!("{stdout}{stderr}");

            // Parse output lines for [OK] and [STALE] patterns
            for line in combined.lines() {
                if let Some(rest) = line.strip_prefix("[OK]") {
                    // Extract event name and path from format: [OK]    event → /path
                    if let Some(content) = rest.trim().split(" → ").next() {
                        let event = content.trim().to_string();
                        if let Some(path_str) = rest.split(" → ").nth(1) {
                            let hook_path = PathBuf::from(path_str.trim());
                            snapshots.push(HookPathSnapshot {
                                event,
                                path: hook_path,
                                passed: true,
                            });
                        }
                    }
                } else if let Some(rest) = line.strip_prefix("[STALE]") {
                    // Extract event name and path from format: [STALE] event → /path (reason)
                    if let Some(content) = rest.trim().split(" → ").next() {
                        let event = content.trim().to_string();
                        if let Some(path_part) = rest.split(" → ").nth(1) {
                            if let Some(path_str) = path_part.split(" (").next() {
                                let hook_path = PathBuf::from(path_str.trim());
                                snapshots.push(HookPathSnapshot {
                                    event,
                                    path: hook_path,
                                    passed: false,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    snapshots
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
        for spec in CLAUDE_HOOK_SPECS {
            assert!(hook_entry_present(&root, spec, &hook_command(spec)));
        }
        assert!(statusline_configured(&root));

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

    #[test]
    fn test_claude_hooks_configured_at_path_detects_missing_statusline() {
        let settings_path = test_settings_path("hooks-missing-statusline");
        install_claude_hooks_at_path(&settings_path).unwrap();

        let mut root: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
        root.as_object_mut()
            .expect("settings root object")
            .remove("statusLine");
        fs::write(&settings_path, serde_json::to_string_pretty(&root).unwrap()).unwrap();

        assert!(!claude_hooks_configured_at_path(&settings_path));
        let _ = fs::remove_file(settings_path);
    }

    #[test]
    fn test_extract_hook_path_resolves_claude_plugin_root() {
        // Lamella hooks use ${CLAUDE_PLUGIN_ROOT} which must be resolved to a concrete path.
        let plugin_root = lamella_plugin_root().expect("lamella_plugin_root returns Some");
        let command = r#"node "${CLAUDE_PLUGIN_ROOT}/scripts/hooks/pre-tool.js""#.to_string();
        let resolved = extract_hook_path(&command);
        let expected = plugin_root.join("scripts/hooks/pre-tool.js");
        assert_eq!(resolved, Some(expected));
    }
}
