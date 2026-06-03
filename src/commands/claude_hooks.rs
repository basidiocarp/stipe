use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::json;
use spore::atomic_write_bytes;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::host_policy::{self, HostConfigScope, HostMode};
use crate::commands::tool_registry::{self, ToolProbe};
use dirs;

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
    /// When set, used as the hook command verbatim instead of generating a cortina adapter command.
    command_override: Option<&'static str>,
}

const CLAUDE_HOOK_SPECS: [HookSpec; 7] = [
    HookSpec {
        event: "PreToolUse",
        matcher: Some("Bash"),
        subcommand: "pre-tool-use",
        status_message: "Cortina rewriting bash commands",
        timeout_secs: 2,
        command_override: None,
    },
    HookSpec {
        event: "PostToolUse",
        matcher: Some("Bash|Write|Edit|MultiEdit"),
        subcommand: "post-tool-use",
        status_message: "Cortina capturing lifecycle signals",
        timeout_secs: 2,
        command_override: None,
    },
    HookSpec {
        event: "Stop",
        matcher: None,
        subcommand: "stop",
        status_message: "Cortina capturing session summary",
        timeout_secs: 2,
        command_override: None,
    },
    HookSpec {
        event: "SessionEnd",
        matcher: None,
        subcommand: "session-end",
        status_message: "Cortina capturing session end",
        timeout_secs: 10,
        command_override: None,
    },
    // SessionStart: not yet registered — cortina has no SessionStart handler.
    // Track in cortina handoff: session-lifecycle-hooks follow-up.
    HookSpec {
        event: "PreCompact",
        matcher: Some("*"),
        subcommand: "pre-compact",
        status_message: "Cortina capturing compaction snapshots",
        timeout_secs: 10,
        command_override: None,
    },
    HookSpec {
        event: "UserPromptSubmit",
        matcher: Some("*"),
        subcommand: "user-prompt-submit",
        status_message: "Cortina capturing submitted prompts",
        timeout_secs: 10,
        command_override: None,
    },
    HookSpec {
        event: "PreToolUse",
        matcher: Some("Bash"),
        subcommand: "",
        status_message: "Checking for Rhizome code navigation opportunity",
        timeout_secs: 2,
        command_override: Some(
            r#"jq -r '.tool_input.command // ""' 2>/dev/null | grep -qE '\b(grep|head|cat|rg|find|sed|awk)\b.+\.(rs|tsx?)' && echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"REMINDER: Use Rhizome MCP tools for code navigation on .rs/.ts files. Load via ToolSearch (select:mcp__rhizome__search_symbols,mcp__rhizome__get_structure,mcp__rhizome__get_symbol_body,mcp__rhizome__find_references) instead of Bash."}}' || true"#,
        ),
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
    tool_registry::find("cortina")
        .is_some_and(|spec| matches!(tool_registry::probe(spec), ToolProbe::Installed(_)))
}

fn annulus_available() -> bool {
    tool_registry::find("annulus")
        .is_some_and(|spec| matches!(tool_registry::probe(spec), ToolProbe::Installed(_)))
}

fn configured_paths() -> Vec<PathBuf> {
    host_policy::claude_hook_settings_paths()
        .into_iter()
        .filter(|path| claude_hooks_configured_at_path(path))
        .collect()
}

/// Resolve a binary name to its absolute path via spore discovery (PATH-independent),
/// falling back to the bare name with a warning. Using spore discovery ensures the
/// written hook command works when Claude Code is launched from a GUI app that lacks
/// ~/.local/bin on PATH.
fn resolve_binary_path(binary_name: &str) -> String {
    tool_registry::find(binary_name)
        .and_then(tool_registry::resolve_binary_path)
        .or_else(|| {
            // Explicit fallback for the canonical stipe install location.
            // which::which misses this when ~/.local/bin is absent from PATH
            // (GUI launches, subprocess invocations).
            dirs::home_dir()
                .map(|home| home.join(".local/bin").join(binary_name))
                .filter(|p| p.exists())
        })
        .map_or_else(
            || {
                eprintln!(
                    "  warning: {binary_name} not found — hook registered with bare name; \
                     install via stipe to ensure hooks fire from GUI apps"
                );
                binary_name.to_string()
            },
            |p| p.to_string_lossy().into_owned(),
        )
}

fn hook_command(spec: HookSpec) -> String {
    if let Some(raw) = spec.command_override {
        return raw.to_string();
    }
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

    // For cortina adapter commands: extract "adapter claude-code <subcommand>" suffix
    // from both existing and expected, then compare. This handles both bare and absolute paths.
    if let Some(existing_pos) = existing.find("adapter claude-code ") {
        if let Some(expected_pos) = expected.find("adapter claude-code ") {
            let existing_suffix = &existing[existing_pos..];
            let expected_suffix = &expected[expected_pos..];
            if existing_suffix == expected_suffix {
                return true;
            }
        }
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

fn insert_hook_entry(root: &mut serde_json::Value, spec: HookSpec, command: &str) -> Result<()> {
    let root_obj = if let Some(obj) = root.as_object_mut() {
        obj
    } else {
        *root = json!({});
        root.as_object_mut().ok_or_else(|| {
            anyhow::anyhow!("settings root is not an object — unexpected type in settings.json")
        })?
    };

    let hooks = root_obj
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            anyhow::anyhow!("settings.hooks is not an object — unexpected type in settings.json")
        })?;

    let event_hooks = hooks
        .entry(spec.event)
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "settings.hooks.{} is not an array — unexpected type in settings.json",
                spec.event
            )
        })?;

    let mut entry = serde_json::Map::new();
    entry.insert(
        "_tag".to_string(),
        serde_json::Value::String("stipe-managed".to_string()),
    );
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
    Ok(())
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

/// Remove all hook entries tagged with `_tag: "stipe-managed"` from a settings JSON root.
/// Also removes legacy stipe-owned entries that lack the tag (for backward compatibility).
/// Operator-written hooks without the tag are preserved.
pub(crate) fn remove_stipe_managed_hooks(root: &mut serde_json::Value) {
    // Stipe-owned subcommands that identify legacy entries for removal
    let stipe_subcommands = [
        "adapter claude-code pre-tool-use",
        "adapter claude-code post-tool-use",
        "adapter claude-code stop",
        "adapter claude-code session-end",
        "adapter claude-code pre-compact",
        "adapter claude-code user-prompt-submit",
    ];

    let Some(hooks) = root.pointer_mut("/hooks") else {
        return;
    };
    let Some(hooks_obj) = hooks.as_object_mut() else {
        return;
    };

    for event_arr in hooks_obj.values_mut() {
        if let Some(arr) = event_arr.as_array_mut() {
            arr.retain(|entry| {
                // Remove if tagged as stipe-managed
                if entry.get("_tag").and_then(|v| v.as_str()) == Some("stipe-managed") {
                    return false;
                }

                // Remove if it's a legacy stipe entry (no tag, but contains stipe subcommand)
                if let Some(inner_hooks) = entry.get("hooks").and_then(serde_json::Value::as_array)
                {
                    if inner_hooks.iter().any(|hook| {
                        let cmd = hook
                            .get("command")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("");
                        stipe_subcommands
                            .iter()
                            .any(|pattern| cmd.contains(pattern))
                    }) {
                        return false;
                    }
                }

                true
            });
        }
    }

    // Remove empty event arrays
    hooks_obj.retain(|_, v| v.as_array().is_some_and(|a| !a.is_empty()));
}

fn ensure_auto_compact_defaults(root: &mut serde_json::Value) -> bool {
    let Some(obj) = root.as_object_mut() else {
        return false;
    };
    let mut changed = false;
    if !obj.contains_key("autoCompactEnabled") {
        obj.insert("autoCompactEnabled".to_string(), json!(true));
        changed = true;
    }
    if !obj.contains_key("autoCompactWindow") {
        // 160_000 ≈ 80% of the 200k Claude context window.
        obj.insert("autoCompactWindow".to_string(), json!(160_000));
        changed = true;
    }
    changed
}

fn ensure_subprocess_env_scrub(root: &mut serde_json::Value) -> bool {
    let Some(obj) = root.as_object_mut() else {
        return false;
    };
    let env = obj.entry("env").or_insert_with(|| json!({}));
    let Some(env_obj) = env.as_object_mut() else {
        return false;
    };
    if env_obj.contains_key("CLAUDE_CODE_SUBPROCESS_ENV_SCRUB") {
        return false;
    }
    env_obj.insert("CLAUDE_CODE_SUBPROCESS_ENV_SCRUB".to_string(), json!("1"));
    true
}

fn install_statusline(root: &mut serde_json::Value, command: &str) -> Result<()> {
    let root_obj = if let Some(obj) = root.as_object_mut() {
        obj
    } else {
        *root = json!({});
        root.as_object_mut().ok_or_else(|| {
            anyhow::anyhow!("settings root is not an object — unexpected type in settings.json")
        })?
    };

    root_obj.insert(
        "statusLine".to_string(),
        json!({
            "type": "command",
            "command": command,
        }),
    );
    Ok(())
}

pub(crate) fn load_or_create_settings(settings_path: &Path) -> Result<serde_json::Value> {
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

pub(crate) fn write_settings(settings_path: &Path, root: &serde_json::Value) -> Result<()> {
    let content = serde_json::to_string_pretty(root).context("serializing hook settings")?;
    // atomic_write_bytes creates parent directories and renames into place,
    // preventing corruption if the process is interrupted mid-write.
    atomic_write_bytes(settings_path, content.as_bytes())
        .with_context(|| format!("writing {}", settings_path.display()))
}

fn install_claude_hooks_at_path(settings_path: &Path) -> Result<bool> {
    let mut root = load_or_create_settings(settings_path)?;

    // Snapshot hooks before modification to accurately detect real changes.
    let hooks_before = root.get("hooks").cloned();

    // Strip all stipe-managed hooks, then re-insert fresh. This is idempotent:
    // a hooks section that already contains the correct tagged entries will
    // be stripped and re-written identically, resulting in changed = false.
    remove_stipe_managed_hooks(&mut root);
    for spec in CLAUDE_HOOK_SPECS {
        let command = hook_command(spec);
        insert_hook_entry(&mut root, spec, &command)?;
    }

    let mut changed = hooks_before != root.get("hooks").cloned();

    let mut wrote_annulus = false;
    if !statusline_configured(&root) {
        let cmd = statusline_command();
        if annulus_available() {
            wrote_annulus = true;
        }
        install_statusline(&mut root, &cmd)?;
        changed = true;
    } else if !annulus_statusline_configured(&root) && annulus_available() {
        // Upgrade: cortina statusline → annulus statusline
        let cmd = resolve_binary_path("annulus") + " statusline";
        install_statusline(&mut root, &cmd)?;
        wrote_annulus = true;
        changed = true;
    }

    if ensure_auto_compact_defaults(&mut root) {
        changed = true;
    }

    if ensure_subprocess_env_scrub(&mut root) {
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
                match resolve_within_plugin_root(path, root) {
                    Some(resolved) => return Some(resolved),
                    None => continue,
                }
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

/// Returns `path` only if it stays within `root` once both are canonicalized.
/// If `path` does not yet exist on disk (canonicalize fails), it is returned
/// unchanged so the caller's existing `path.exists()` check still reports it.
/// If `path` resolves outside `root`, returns `None` (escape rejected).
fn resolve_within_plugin_root(path: PathBuf, root: &Path) -> Option<PathBuf> {
    let Ok(canonical_path) = std::fs::canonicalize(&path) else {
        return Some(path); // not on disk yet — let path.exists() report it
    };
    let Ok(canonical_root) = std::fs::canonicalize(root) else {
        return Some(path); // unresolvable root — don't reject
    };
    if canonical_path.starts_with(&canonical_root) {
        Some(canonical_path)
    } else {
        None
    }
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
    atomic_write_bytes(&config_path, DEFAULT_ANNULUS_CONFIG.as_bytes())
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
    install_statusline(&mut root, &cmd)?;
    write_settings(&settings_path, &root)?;
    ensure_annulus_config()?;

    if verbose > 0 {
        eprintln!("  Wrote annulus statusline to {}", settings_path.display());
    }

    Ok(true)
}

/// Locate the lamella validator script, trying `LAMELLA_HOME`, known install
/// locations, and finally $PATH (in that order).
fn find_lamella_validator() -> Option<PathBuf> {
    if let Ok(lamella_home) = std::env::var("LAMELLA_HOME") {
        let path = PathBuf::from(&lamella_home).join("scripts/validate-hooks.js");
        if path.exists() {
            return Some(path);
        }
    }

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

    // Use the resolved absolute path from which::which so the home-directory guard
    // checks the actual location rather than a bare filename canonicalized against cwd.
    which::which("lamella-validate-hooks").ok()
}

/// Check lamella hook path staleness by running the lamella validator script
pub(crate) fn lamella_hook_path_snapshots() -> Vec<HookPathSnapshot> {
    let mut snapshots = Vec::new();

    let validator_script = find_lamella_validator();

    // Run the validator if found
    if let Some(validator_path) = validator_script {
        // Tighter prefix: prefer lamella content root over broad home dir
        let safe_prefix = std::env::var("LAMELLA_CONTENT_ROOT")
            .ok()
            .map(PathBuf::from)
            .or_else(|| std::env::var("LAMELLA_HOME").ok().map(PathBuf::from))
            .or_else(|| {
                // Try known install locations
                let candidates = ["~/.lamella", "~/.local/share/lamella", "~/.config/lamella"];
                for candidate in &candidates {
                    let path = if let Some(stripped) = candidate.strip_prefix("~/") {
                        dirs::home_dir()?.join(stripped)
                    } else {
                        PathBuf::from(candidate)
                    };
                    if path.exists() {
                        return Some(path);
                    }
                }
                None
            }); // if no tighter prefix resolves, refuse to run the validator

        let Some(safe_prefix) = safe_prefix else {
            return snapshots;
        };

        // Canonicalize both sides so symlinks don't bypass the prefix guard.
        let canonical_prefix = std::fs::canonicalize(&safe_prefix).unwrap_or(safe_prefix);
        let canonical =
            std::fs::canonicalize(&validator_path).unwrap_or_else(|_| validator_path.clone());
        if !canonical.starts_with(&canonical_prefix) {
            return snapshots;
        }

        // Verify node is available before attempting to run the validator
        let Ok(node_bin) = which::which("node") else {
            tracing::warn!(
                "node not found on PATH — lamella hook validator will not run; \
                 hook paths cannot be validated for staleness"
            );
            return snapshots;
        };

        let output = crate::commands::install::release::run_command_with_timeout(
            Command::new(&node_bin).arg(&validator_path),
            std::time::Duration::from_secs(10),
        );
        if let Ok(output) = output {
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

// ---------------------------------------------------------------------------
// TOML-driven sync helpers (used by `stipe sync`)
// ---------------------------------------------------------------------------

/// A hook entry sourced from `stipe.toml` — the caller converts TOML fields
/// to this struct before calling `sync_toml_hooks` or `toml_sync_diverged`.
pub(crate) struct TomlHookEntry {
    pub event: String,
    pub matcher: Option<String>,
    pub command: String,
    /// Timeout in seconds; derived from stipe.toml's `timeout_ms / 1000`.
    pub timeout_secs: u64,
}

/// Write a single raw hook entry tagged `"stipe-managed"` into the settings JSON root.
fn insert_raw_hook_entry(
    root: &mut serde_json::Value,
    event: &str,
    matcher: Option<&str>,
    command: &str,
    timeout_secs: u64,
) -> Result<()> {
    let root_obj = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings root is not a JSON object"))?;

    let hooks = root_obj
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.hooks is not a JSON object"))?;

    let event_hooks = hooks
        .entry(event)
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.hooks.{event} is not a JSON array"))?;

    let mut entry = serde_json::Map::new();
    entry.insert(
        "_tag".to_string(),
        serde_json::Value::String("stipe-managed".to_string()),
    );
    if let Some(m) = matcher {
        entry.insert(
            "matcher".to_string(),
            serde_json::Value::String(m.to_string()),
        );
    }
    entry.insert(
        "hooks".to_string(),
        json!([{
            "type": "command",
            "command": command,
            "timeout": timeout_secs,
            "statusMessage": "",
        }]),
    );

    event_hooks.push(serde_json::Value::Object(entry));
    Ok(())
}

/// Strip stipe-managed hooks, re-insert from `hooks`, write permissions/network.
/// Returns `true` if settings were changed and written.
pub(crate) fn sync_toml_hooks(
    settings_path: &Path,
    hooks: &[TomlHookEntry],
    allow_tools: &[String],
    denied_domains: &[String],
) -> Result<bool> {
    let mut root = load_or_create_settings(settings_path)?;
    let before = root.clone();

    remove_stipe_managed_hooks(&mut root);
    for entry in hooks {
        insert_raw_hook_entry(
            &mut root,
            &entry.event,
            entry.matcher.as_deref(),
            &entry.command,
            entry.timeout_secs,
        )?;
    }

    apply_toml_permissions_network(&mut root, allow_tools, denied_domains)?;

    if root == before {
        return Ok(false);
    }
    write_settings(settings_path, &root)?;
    Ok(true)
}

/// Write or clear `permissions.allow` and `network.denyDomains` based on stipe.toml state.
///
/// When non-empty: sets the key. When empty: removes the key so that a previously-written
/// value does not linger after the user removes it from stipe.toml.
fn apply_toml_permissions_network(
    root: &mut serde_json::Value,
    allow_tools: &[String],
    denied_domains: &[String],
) -> Result<()> {
    if !allow_tools.is_empty() {
        root["permissions"]["allow"] =
            serde_json::to_value(allow_tools).context("serializing allow_tools")?;
    } else if let Some(perms) = root.get_mut("permissions").and_then(|p| p.as_object_mut()) {
        perms.remove("allow");
    }

    if !denied_domains.is_empty() {
        root["network"]["denyDomains"] =
            serde_json::to_value(denied_domains).context("serializing denied_domains")?;
    } else if let Some(net) = root.get_mut("network").and_then(|n| n.as_object_mut()) {
        net.remove("denyDomains");
    }

    Ok(())
}

/// Returns `true` if the full sync state in `settings_path` differs from what
/// `sync_toml_hooks` would produce from the given inputs.
/// Returns `false` (in sync) when the file does not exist yet.
pub(crate) fn toml_sync_diverged(
    settings_path: &Path,
    hooks: &[TomlHookEntry],
    allow_tools: &[String],
    denied_domains: &[String],
) -> Result<bool> {
    let current = load_or_create_settings(settings_path)?;
    let mut expected = current.clone();
    remove_stipe_managed_hooks(&mut expected);
    for entry in hooks {
        insert_raw_hook_entry(
            &mut expected,
            &entry.event,
            entry.matcher.as_deref(),
            &entry.command,
            entry.timeout_secs,
        )?;
    }
    apply_toml_permissions_network(&mut expected, allow_tools, denied_domains)?;
    Ok(current != expected)
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
        // The plugin-root guard returns the canonicalized path when the file exists, and the
        // constructed path unchanged when it does not (INV3). Canonicalize the expected value
        // with the same fallback so the assertion holds regardless of symlinks in $HOME /
        // LAMELLA_HOME (e.g. /tmp -> /private/tmp on macOS).
        let joined = plugin_root.join("scripts/hooks/pre-tool.js");
        let expected = std::fs::canonicalize(&joined).unwrap_or(joined);
        assert_eq!(resolved, Some(expected));
    }

    #[test]
    fn test_bare_hook_entry_stripped_and_reinserted_as_absolute_path() {
        // Only testable when cortina is installed at an absolute path; without it
        // resolve_binary_path returns the bare name.
        if which::which("cortina").is_err() {
            return;
        }

        let settings_path = test_settings_path("hooks-upgrade");
        // Create a settings file with a bare-name hook entry (legacy format, no _tag).
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

        // Install strips the legacy bare-name entry and re-inserts with absolute path + _tag.
        install_claude_hooks_at_path(&settings_path).unwrap();

        // Verify the entry was upgraded to absolute path (not duplicated).
        let root: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();

        let entries = root
            .get("hooks")
            .and_then(|hooks| hooks.get("PreToolUse"))
            .and_then(serde_json::Value::as_array)
            .expect("PreToolUse hooks array");

        // Two PreToolUse entries: cortina + Rhizome navigation reminder.
        assert_eq!(entries.len(), 2);

        // Find the cortina entry specifically and verify it was upgraded to absolute path.
        let cortina_entry = entries
            .iter()
            .find(|e| {
                e.get("hooks")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|h| {
                        h.iter().any(|hook| {
                            hook.get("command")
                                .and_then(serde_json::Value::as_str)
                                .is_some_and(|cmd| cmd.contains("adapter claude-code pre-tool-use"))
                        })
                    })
            })
            .expect("cortina pre-tool-use entry");

        let hook_list = cortina_entry
            .get("hooks")
            .and_then(serde_json::Value::as_array)
            .expect("hooks array");
        assert_eq!(hook_list.len(), 1);

        let cmd = hook_list[0]
            .get("command")
            .and_then(serde_json::Value::as_str)
            .expect("command string");

        // Should now be absolute path (not bare "cortina")
        assert!(!cmd.eq("cortina adapter claude-code pre-tool-use"));
        assert!(cmd.contains("cortina") && cmd.contains("adapter claude-code pre-tool-use"));

        let _ = fs::remove_file(settings_path);
    }

    #[test]
    fn test_ensure_subprocess_env_scrub_empty_object() {
        let mut root = json!({});
        let changed = ensure_subprocess_env_scrub(&mut root);
        assert!(changed);
        assert_eq!(
            root.get("env")
                .and_then(|env| env.get("CLAUDE_CODE_SUBPROCESS_ENV_SCRUB"))
                .and_then(serde_json::Value::as_str),
            Some("1")
        );
    }

    #[test]
    fn test_ensure_subprocess_env_scrub_idempotent() {
        let mut root = json!({});
        let first_change = ensure_subprocess_env_scrub(&mut root);
        assert!(first_change);

        let second_change = ensure_subprocess_env_scrub(&mut root);
        assert!(!second_change);
    }

    #[test]
    fn test_ensure_subprocess_env_scrub_preserves_existing_value() {
        let mut root = json!({ "env": { "CLAUDE_CODE_SUBPROCESS_ENV_SCRUB": "0" } });
        let changed = ensure_subprocess_env_scrub(&mut root);
        assert!(!changed);
        assert_eq!(
            root.get("env")
                .and_then(|env| env.get("CLAUDE_CODE_SUBPROCESS_ENV_SCRUB"))
                .and_then(serde_json::Value::as_str),
            Some("0")
        );
    }

    #[test]
    fn test_ensure_subprocess_env_scrub_preserves_other_env_keys() {
        let mut root = json!({ "env": { "FOO": "bar" } });
        let changed = ensure_subprocess_env_scrub(&mut root);
        assert!(changed);
        assert_eq!(
            root.get("env")
                .and_then(|env| env.get("FOO"))
                .and_then(serde_json::Value::as_str),
            Some("bar")
        );
        assert_eq!(
            root.get("env")
                .and_then(|env| env.get("CLAUDE_CODE_SUBPROCESS_ENV_SCRUB"))
                .and_then(serde_json::Value::as_str),
            Some("1")
        );
    }

    #[test]
    fn test_ensure_subprocess_env_scrub_leaves_non_object_env_untouched() {
        // A malformed (non-object) `env` value is occupied, so `entry().or_insert_with`
        // never fires: the helper returns false and leaves the existing value intact
        // rather than silently discarding it.
        let mut root = json!({ "env": "not-an-object" });
        let changed = ensure_subprocess_env_scrub(&mut root);
        assert!(!changed);
        assert_eq!(
            root.get("env").and_then(serde_json::Value::as_str),
            Some("not-an-object")
        );
    }

    #[test]
    fn test_resolve_within_plugin_root_returns_unchanged_nonexistent_path() {
        // INV3: a non-existent ${CLAUDE_PLUGIN_ROOT} path is returned unchanged.
        let temp_root = std::env::temp_dir().join(format!(
            "stipe-test-root-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_root).unwrap();

        let nonexistent = temp_root.join("nonexistent-hook.js");
        let result = resolve_within_plugin_root(nonexistent.clone(), &temp_root);

        assert_eq!(
            result,
            Some(nonexistent),
            "non-existent path should be returned unchanged"
        );

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn test_resolve_within_plugin_root_accepts_in_root_file() {
        // INV1: a ${CLAUDE_PLUGIN_ROOT}-derived path that canonicalizes within the canonical
        // plugin root is returned.
        let temp_root = std::env::temp_dir().join(format!(
            "stipe-test-root-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_root).unwrap();

        // Create an actual file inside the root
        let hook_file = temp_root.join("hook.js");
        fs::write(&hook_file, "// hook content").unwrap();

        let result = resolve_within_plugin_root(hook_file.clone(), &temp_root);

        // Should return the canonicalized path
        assert!(result.is_some(), "in-root file should be accepted");
        let returned = result.unwrap();
        // The returned path should be the canonical form
        let expected = std::fs::canonicalize(&hook_file).unwrap();
        assert_eq!(returned, expected);

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn test_resolve_within_plugin_root_rejects_escape() {
        // INV2: a ${CLAUDE_PLUGIN_ROOT}/../... escape to an EXISTING out-of-root file is rejected.
        let temp_base = std::env::temp_dir().join(format!(
            "stipe-test-base-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_base).unwrap();

        let root = temp_base.join("root");
        fs::create_dir_all(&root).unwrap();

        // Create a file outside the root
        let outside_file = temp_base.join("outside.js");
        fs::write(&outside_file, "// outside").unwrap();

        // Construct a path that escapes the root via ..
        let escape_path = root.join("../outside.js");

        let result = resolve_within_plugin_root(escape_path, &root);

        assert_eq!(result, None, "path escaping root should be rejected");

        let _ = fs::remove_dir_all(&temp_base);
    }
}
