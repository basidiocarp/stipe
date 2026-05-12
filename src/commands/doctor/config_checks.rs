use serde_json::Value;

use super::host_policy;
use super::model::{ConfigFormat, DriftReport, HealthCheck};
use crate::commands::init::baseline;
use crate::commands::repair::{RepairAction, RepairTier};

pub(super) struct ConfigDriftState {
    pub(super) check: HealthCheck,
    pub(super) report: Option<DriftReport>,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn config_mentions_servers(
    content: &str,
    required_servers: &[&str],
    format: ConfigFormat,
) -> bool {
    match format {
        ConfigFormat::Json => {
            let parsed: Value = match serde_json::from_str(content) {
                Ok(value) => value,
                Err(_) => return false,
            };

            let Some(mcp_servers) = parsed.get("mcpServers").and_then(Value::as_object) else {
                return false;
            };

            required_servers
                .iter()
                .all(|server| mcp_servers.contains_key(*server))
        }
        ConfigFormat::ClaudeRoot => {
            let parsed: Value = match serde_json::from_str(content) {
                Ok(value) => value,
                Err(_) => return false,
            };

            let user_matches = parsed
                .get("mcpServers")
                .and_then(Value::as_object)
                .is_some_and(|mcp_servers| {
                    required_servers
                        .iter()
                        .all(|server| mcp_servers.contains_key(*server))
                });

            if user_matches {
                return true;
            }

            let Some(project_root) = host_policy::project_root() else {
                return false;
            };
            let project_key = project_root.to_string_lossy();

            parsed
                .get("projects")
                .and_then(Value::as_object)
                .and_then(|projects| projects.get(project_key.as_ref()))
                .and_then(|project| project.get("mcpServers"))
                .and_then(Value::as_object)
                .is_some_and(|mcp_servers| {
                    required_servers
                        .iter()
                        .all(|server| mcp_servers.contains_key(*server))
                })
        }
        ConfigFormat::Toml => {
            let parsed: toml::Value = match toml::from_str(content) {
                Ok(value) => value,
                Err(_) => return false,
            };

            let Some(mcp_servers) = parsed.get("mcp_servers").and_then(toml::Value::as_table)
            else {
                return false;
            };

            required_servers
                .iter()
                .all(|server| mcp_servers.contains_key(*server))
        }
    }
}

pub(super) fn check_mcp_config_drift() -> ConfigDriftState {
    match baseline::evaluate_drift() {
        Ok(Some(report)) => {
            let passed = report.findings.is_empty();
            ConfigDriftState {
                check: HealthCheck {
                    name: "config drift".to_string(),
                    passed,
                    message: if passed {
                        "Init baseline matches current config state.".to_string()
                    } else {
                        format!("{} config drift issue(s) detected", report.findings.len())
                    },
                    repair_actions: baseline::repair_actions_for_report(&report),
                },
                report: Some(report),
            }
        }
        Ok(None) => ConfigDriftState {
            check: HealthCheck {
                name: "config drift".to_string(),
                passed: true,
                message: "No init baseline found; skipping drift detection.".to_string(),
                repair_actions: Vec::new(),
            },
            report: None,
        },
        Err(error) => ConfigDriftState {
            check: HealthCheck {
                name: "config drift".to_string(),
                passed: false,
                message: format!("Unable to read init baseline: {error}"),
                repair_actions: vec![RepairAction::stipe(
                    "repair-init",
                    "Repair the init baseline",
                    "Rebuild the init baseline after fixing the drifted config.",
                    &["init", "--repair"],
                    RepairTier::Primary,
                )],
            },
            report: None,
        },
    }
}

#[cfg(test)]
pub(super) fn codex_notify_adapter_configured_at_path(config_path: &std::path::Path) -> bool {
    crate::commands::codex_notify::codex_notify_configured_at_path(config_path)
}
