//! Check for lamella security hooks registration.
//!
//! Verifies that at least one `PreToolUse` hook is registered with a lamella
//! security script (file-guard, sandbox-validation, dangerous-actions-blocker,
//! or output-secrets-scanner).

use std::fs;

use super::model::HealthCheck;
use crate::commands::repair::{RepairAction, RepairTier};

fn check_lamella_hooks_at(paths: &[std::path::PathBuf]) -> HealthCheck {
    let lamella_security_markers = [
        "file-guard",
        "sandbox-validation",
        "dangerous-actions-blocker",
        "output-secrets-scanner",
    ];

    let mut found = false;

    for settings_path in paths {
        if let Ok(content) = fs::read_to_string(settings_path) {
            if let Ok(root) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(hooks) = root
                    .get("hooks")
                    .and_then(|h| h.get("PreToolUse"))
                    .and_then(serde_json::Value::as_array)
                {
                    for entry in hooks {
                        if let Some(hook_list) =
                            entry.get("hooks").and_then(serde_json::Value::as_array)
                        {
                            for hook in hook_list {
                                if let Some(command) =
                                    hook.get("command").and_then(serde_json::Value::as_str)
                                {
                                    if lamella_security_markers
                                        .iter()
                                        .any(|marker| command.contains(marker))
                                    {
                                        found = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    HealthCheck {
        name: "lamella security hooks".to_string(),
        passed: found,
        message: if found {
            "Security hooks registered".to_string()
        } else {
            "No lamella PreToolUse hooks found in .claude/settings.json — run 'lamella install' or check plugin setup".to_string()
        },
        repair_actions: if found {
            Vec::new()
        } else {
            vec![RepairAction::stipe(
                "install-lamella-hooks",
                "Install lamella security hooks",
                "Run 'lamella install' to register security hooks in .claude/settings.json.",
                &["init", "--repair"],
                RepairTier::Primary,
            )]
        },
    }
}

/// Check if lamella security hooks are registered in .claude/settings.json.
pub(super) fn check_lamella_hooks() -> HealthCheck {
    let settings_paths = crate::commands::host_policy::claude_hook_settings_paths();
    check_lamella_hooks_at(&settings_paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_settings_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("stipe-{name}-{unique}.json"))
    }

    #[test]
    fn check_lamella_hooks_passes_when_file_guard_found() {
        let settings_path = test_settings_path("lamella-hooks-guard");
        let content = serde_json::json!({
            "hooks": {
                "PreToolUse": [{
                    "matcher": "*",
                    "hooks": [{
                        "type": "command",
                        "command": "node ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/file-guard.js",
                        "timeout": 5,
                        "statusMessage": "Checking file access"
                    }]
                }]
            }
        });
        fs::write(&settings_path, content.to_string()).unwrap();

        let check = check_lamella_hooks_at(std::slice::from_ref(&settings_path));
        assert!(check.passed);
        assert!(check.message.contains("Security hooks registered"));

        let _ = fs::remove_file(settings_path);
    }

    #[test]
    fn check_lamella_hooks_fails_when_no_security_hooks() {
        let settings_path = test_settings_path("lamella-hooks-none");
        let content = serde_json::json!({
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
        });
        fs::write(&settings_path, content.to_string()).unwrap();

        let check = check_lamella_hooks_at(std::slice::from_ref(&settings_path));
        assert!(!check.passed);
        assert!(check.message.contains("lamella"));
        assert!(!check.repair_actions.is_empty());

        let _ = fs::remove_file(settings_path);
    }
}
