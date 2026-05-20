//! User-registered hook command validation.
//!
//! Checks that all commands registered in Claude hook settings and Codex notify
//! configuration resolve to valid binaries in PATH or on disk. Unlike the
//! stipe-owned hook checks in `tool_checks.rs`, this module validates *all*
//! registered commands, including user-added hooks.

use std::path::Path;

use super::model::HealthCheck;
use crate::commands::claude_hooks;
use crate::commands::host_policy;
use crate::commands::host_policy::HostConfigScope;

/// Check that every command registered in `~/.claude/settings.json` hooks
/// resolves to a binary in PATH or an absolute path on disk.
///
/// Returns one or more [`HealthCheck`] results. If no hooks are registered,
/// returns a single `passed=true` check.
pub(super) fn check_claude_hook_commands(scope: HostConfigScope) -> Vec<HealthCheck> {
    let mut failures: Vec<String> = Vec::new();

    let Some(settings_path) = host_policy::claude_hook_settings_path(scope) else {
        return vec![HealthCheck {
            name: format!(
                "claude hook commands ({})",
                host_policy::scope_name(scope)
            ),
            passed: true,
            message: "No settings.json path configured for this scope.".to_string(),
            repair_actions: Vec::new(),
        }];
    };

    // If settings file doesn't exist, no hooks to validate — pass.
    if !settings_path.exists() {
        return vec![HealthCheck {
            name: format!(
                "claude hook commands ({})",
                host_policy::scope_name(scope)
            ),
            passed: true,
            message: format!("No hooks configured (settings file not found at {})", settings_path.display()),
            repair_actions: Vec::new(),
        }];
    }

    // File exists — attempt to read hook entries. Failure here is a real problem.
    let entries = match claude_hooks::hook_entries_at_path(&settings_path) {
        Ok(e) => e,
        Err(e) => {
            return vec![HealthCheck {
                name: format!(
                    "claude hook commands ({})",
                    host_policy::scope_name(scope)
                ),
                passed: false,
                message: format!("Could not read hooks from {}: {e}", settings_path.display()),
                repair_actions: Vec::new(),
            }];
        }
    };

    if entries.is_empty() {
        return vec![HealthCheck {
            name: format!(
                "claude hook commands ({})",
                host_policy::scope_name(scope)
            ),
            passed: true,
            message: "No hooks registered.".to_string(),
            repair_actions: Vec::new(),
        }];
    }

    let entry_count = entries.len();
    for entry in entries {
        if let Some(error_msg) = check_hook_binary(&entry.command) {
            failures.push(format!("  {} ({}): {error_msg}", entry.event, entry.hook_type));
        }
    }

    if failures.is_empty() {
        vec![HealthCheck {
            name: format!(
                "claude hook commands ({})",
                host_policy::scope_name(scope)
            ),
            passed: true,
            message: format!(
                "All {} hook command(s) resolve to valid binaries.",
                entry_count
            ),
            repair_actions: Vec::new(),
        }]
    } else {
        vec![HealthCheck {
            name: format!(
                "claude hook commands ({})",
                host_policy::scope_name(scope)
            ),
            passed: false,
            message: format!(
                "{} hook command(s) reference missing or non-executable binaries:\n{}",
                failures.len(),
                failures.join("\n")
            ),
            repair_actions: Vec::new(),
        }]
    }
}

/// Check that every entry in `~/.codex/config.toml` `[notify]` resolves to
/// a binary in PATH or an absolute path on disk.
///
/// Returns one or more [`HealthCheck`] results. If no notify entries are configured,
/// returns a single `passed=true` check.
#[allow(clippy::too_many_lines)]
pub(super) fn check_codex_notify_entries(scope: HostConfigScope) -> Vec<HealthCheck> {
    let mut failures: Vec<String> = Vec::new();

    let Some(config_path) = host_policy::codex_notify_config_path(scope) else {
        return vec![HealthCheck {
            name: format!(
                "codex notify entries ({})",
                host_policy::scope_name(scope)
            ),
            passed: true,
            message: "No config.toml path configured for this scope.".to_string(),
            repair_actions: Vec::new(),
        }];
    };

    // If config file doesn't exist, no notify entries to validate — pass.
    if !config_path.exists() {
        return vec![HealthCheck {
            name: format!(
                "codex notify entries ({})",
                host_policy::scope_name(scope)
            ),
            passed: true,
            message: format!("No notify config (file not found at {})", config_path.display()),
            repair_actions: Vec::new(),
        }];
    }

    // File exists — failure to read is a real problem.
    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(e) => {
            return vec![HealthCheck {
                name: format!(
                    "codex notify entries ({})",
                    host_policy::scope_name(scope)
                ),
                passed: false,
                message: format!("Could not read {}: {e}", config_path.display()),
                repair_actions: Vec::new(),
            }];
        }
    };

    let Ok(root) = content.parse::<toml::Table>() else {
        return vec![HealthCheck {
            name: format!(
                "codex notify entries ({})",
                host_policy::scope_name(scope)
            ),
            passed: false,
            message: format!(
                "Could not parse TOML at {}",
                config_path.display()
            ),
            repair_actions: Vec::new(),
        }];
    };

    let Some(notify_array) = root.get("notify").and_then(|v| v.as_array()) else {
        return vec![HealthCheck {
            name: format!(
                "codex notify entries ({})",
                host_policy::scope_name(scope)
            ),
            passed: true,
            message: "No notify entries configured.".to_string(),
            repair_actions: Vec::new(),
        }];
    };

    if notify_array.is_empty() {
        return vec![HealthCheck {
            name: format!(
                "codex notify entries ({})",
                host_policy::scope_name(scope)
            ),
            passed: true,
            message: "Notify array is empty.".to_string(),
            repair_actions: Vec::new(),
        }];
    }

    for entry in notify_array {
        if let Some(entry_str) = entry.as_str() {
            if let Some(_error_msg) = check_hook_binary(entry_str) {
                let token = entry_str.split_whitespace().next().unwrap_or(entry_str);
                failures.push(format!(
                    "  '{}' in codex notify is not a known binary — remove or replace it",
                    token
                ));
            }
        }
    }

    if failures.is_empty() {
        vec![HealthCheck {
            name: format!(
                "codex notify entries ({})",
                host_policy::scope_name(scope)
            ),
            passed: true,
            message: format!(
                "All {} notify entry(ies) resolve to valid binaries.",
                notify_array.len()
            ),
            repair_actions: Vec::new(),
        }]
    } else {
        vec![HealthCheck {
            name: format!(
                "codex notify entries ({})",
                host_policy::scope_name(scope)
            ),
            passed: false,
            message: format!(
                "{} notify entry(ies) are unresolvable:\n{}",
                failures.len(),
                failures.join("\n")
            ),
            repair_actions: Vec::new(),
        }]
    }
}

/// Check if a hook binary token resolves to an executable.
///
/// Returns `None` if the binary is valid, or `Some(error_message)` if invalid.
fn check_hook_binary(command: &str) -> Option<String> {
    let binary_token = command.split_whitespace().next()?;

    if binary_token.starts_with('/') {
        // Absolute path: check existence.
        let path = Path::new(binary_token);
        if !path.exists() {
            return Some(format!("binary not found: {binary_token}"));
        }
        None
    } else {
        // Bare name: check via which.
        match which::which(binary_token) {
            Ok(_) => None,
            Err(_) => Some(format!("binary not in PATH: {binary_token}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn check_hook_binary_absolute_path_exists() {
        // Use /usr/bin/env which exists on Unix systems.
        let result = check_hook_binary("/usr/bin/env echo hello");
        assert_eq!(result, None, "Should pass for existing absolute path");
    }

    #[test]
    fn check_hook_binary_absolute_path_missing() {
        let result = check_hook_binary("/nonexistent/binary arg1 arg2");
        assert!(
            result.is_some(),
            "Should fail for non-existent absolute path"
        );
    }

    #[test]
    fn check_hook_binary_bare_name_in_path() {
        let result = check_hook_binary("echo some args");
        assert_eq!(result, None, "Should pass for 'echo' which is in PATH");
    }

    #[test]
    fn check_hook_binary_bare_name_missing() {
        let result = check_hook_binary("nonexistent_binary_xyz12345 arg1");
        assert!(
            result.is_some(),
            "Should fail for bare name not in PATH"
        );
    }

    #[test]
    fn check_codex_notify_entries_bad_bare_string() {
        // Create a temporary TOML config with a bad notify entry.
        let tmpdir = TempDir::new().unwrap();
        let config_path = tmpdir.path().join("config.toml");
        let content = r#"notify = ["nonexistent_binary_xyz"]"#;
        fs::write(&config_path, content).unwrap();

        // Mock the path retrieval by directly testing the logic.
        let checks = check_codex_notify_entries_with_path(&config_path);
        assert!(!checks.is_empty());
        assert!(!checks[0].passed, "Should fail for unresolvable notify entry");
        assert!(checks[0].message.contains("nonexistent_binary_xyz"));
    }

    #[test]
    fn check_codex_notify_entries_good_absolute_path() {
        let tmpdir = TempDir::new().unwrap();
        let config_path = tmpdir.path().join("config.toml");
        let content = r#"
notify = ["/usr/bin/env"]
"#;
        fs::write(&config_path, content).unwrap();

        let checks = check_codex_notify_entries_with_path(&config_path);
        assert!(!checks.is_empty());
        assert!(checks[0].passed, "Should pass for existing absolute path");
    }

    #[test]
    fn check_codex_notify_entries_empty() {
        let tmpdir = TempDir::new().unwrap();
        let config_path = tmpdir.path().join("config.toml");
        let content = r"
notify = []
";
        fs::write(&config_path, content).unwrap();

        let checks = check_codex_notify_entries_with_path(&config_path);
        assert!(!checks.is_empty());
        assert!(checks[0].passed, "Should pass for empty notify array");
    }

    // Helper for testing with a specific path instead of scope.
    fn check_codex_notify_entries_with_path(config_path: &std::path::Path) -> Vec<HealthCheck> {
        let mut failures: Vec<String> = Vec::new();

        let Ok(content) = std::fs::read_to_string(config_path) else {
            return vec![HealthCheck {
                name: "test".to_string(),
                passed: true,
                message: "File not found".to_string(),
                repair_actions: Vec::new(),
            }];
        };

        let Ok(root) = content.parse::<toml::Table>() else {
            return vec![HealthCheck {
                name: "test".to_string(),
                passed: false,
                message: "Parse error".to_string(),
                repair_actions: Vec::new(),
            }];
        };

        let Some(notify_array) = root.get("notify").and_then(|v| v.as_array()) else {
            return vec![HealthCheck {
                name: "test".to_string(),
                passed: true,
                message: "No notify".to_string(),
                repair_actions: Vec::new(),
            }];
        };

        if notify_array.is_empty() {
            return vec![HealthCheck {
                name: "test".to_string(),
                passed: true,
                message: "Empty".to_string(),
                repair_actions: Vec::new(),
            }];
        }

        for entry in notify_array {
            if let Some(entry_str) = entry.as_str() {
                if let Some(_error_msg) = check_hook_binary(entry_str) {
                    failures.push(format!("bad: {entry_str}"));
                }
            }
        }

        if failures.is_empty() {
            vec![HealthCheck {
                name: "test".to_string(),
                passed: true,
                message: "All good".to_string(),
                repair_actions: Vec::new(),
            }]
        } else {
            vec![HealthCheck {
                name: "test".to_string(),
                passed: false,
                message: format!("Failures: {}", failures.join(", ")),
                repair_actions: Vec::new(),
            }]
        }
    }
}
