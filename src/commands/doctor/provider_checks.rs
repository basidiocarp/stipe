use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::Value;

use crate::commands::host;
use crate::commands::host_policy;
use crate::commands::host_policy::HostMode;

use super::model::{ApiKeyHealth, ApiKeyStatus, AuthFreshness, McpHealth, ProviderHealth};

const REQUIRED_MCP_SERVERS: &[&str] = &["hyphae", "rhizome"];
const AUTH_STALE_AFTER_DAYS: u64 = 30;

pub(super) fn collect_provider_health() -> Vec<ProviderHealth> {
    host::build_inventory()
        .into_iter()
        .map(|entry| {
            let config_paths = host_config_paths(entry.mode);
            let auth_freshness = auth_freshness_for_paths(&config_paths);
            let auth_detail = auth_detail_for_paths(&config_paths, auth_freshness);
            let healthy = entry.detected && entry.configured;
            ProviderHealth {
                host: entry.mode,
                provider: entry.label,
                available: entry.detected,
                healthy,
                status: if healthy {
                    "provider ready".to_string()
                } else if entry.detected {
                    "provider detected but not fully configured".to_string()
                } else {
                    "provider not detected".to_string()
                },
                auth_freshness,
                auth_detail,
            }
        })
        .collect()
}

pub(super) fn collect_mcp_health() -> Vec<McpHealth> {
    host_policy::supported_host_modes()
        .iter()
        .copied()
        .map(build_mcp_health_for_mode)
        .collect()
}

fn build_mcp_health_for_mode(mode: HostMode) -> McpHealth {
    let mut registered = BTreeSet::new();
    let config_paths = host_config_paths(mode)
        .into_iter()
        .filter(|path| path.exists())
        .collect::<Vec<_>>();

    for path in &config_paths {
        for server in read_registered_servers(path, mode) {
            registered.insert(server);
        }
    }

    let required_servers = REQUIRED_MCP_SERVERS
        .iter()
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();
    let registered_servers = registered.into_iter().collect::<Vec<_>>();
    let missing_servers = required_servers
        .iter()
        .filter(|required| !registered_servers.contains(*required))
        .cloned()
        .collect::<Vec<_>>();
    let healthy = !config_paths.is_empty() && missing_servers.is_empty();
    let status = if config_paths.is_empty() {
        "no MCP registration config discovered".to_string()
    } else if missing_servers.is_empty() {
        "required MCP servers are registered".to_string()
    } else {
        format!(
            "missing MCP registration for {}",
            missing_servers.join(", ")
        )
    };

    McpHealth {
        host: mode,
        config_paths,
        required_servers,
        registered_servers,
        missing_servers,
        healthy,
        status,
        auth_freshness: auth_freshness_for_paths(&host_config_paths(mode)),
    }
}

fn read_registered_servers(path: &Path, mode: HostMode) -> Vec<String> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };

    match mode {
        HostMode::Codex => parse_toml_servers(&content),
        HostMode::ClaudeCode => {
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == ".mcp.json")
            {
                parse_json_servers(&content)
            } else {
                parse_claude_root_servers(&content)
            }
        }
        HostMode::Cursor => parse_json_servers(&content),
    }
}

fn parse_json_servers(content: &str) -> Vec<String> {
    let Ok(parsed) = serde_json::from_str::<Value>(content) else {
        return Vec::new();
    };

    parsed
        .get("mcpServers")
        .and_then(Value::as_object)
        .map(|servers| servers.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default()
}

fn parse_claude_root_servers(content: &str) -> Vec<String> {
    let Ok(parsed) = serde_json::from_str::<Value>(content) else {
        return Vec::new();
    };

    let mut servers = BTreeSet::new();
    if let Some(root_servers) = parsed.get("mcpServers").and_then(Value::as_object) {
        servers.extend(root_servers.keys().cloned());
    }

    if let Some(project_root) = host_policy::project_root() {
        let project_key = project_root.to_string_lossy();
        if let Some(project_servers) = parsed
            .get("projects")
            .and_then(Value::as_object)
            .and_then(|projects| projects.get(project_key.as_ref()))
            .and_then(|project| project.get("mcpServers"))
            .and_then(Value::as_object)
        {
            servers.extend(project_servers.keys().cloned());
        }
    }

    servers.into_iter().collect()
}

fn parse_toml_servers(content: &str) -> Vec<String> {
    let Ok(parsed) = toml::from_str::<toml::Value>(content) else {
        return Vec::new();
    };

    parsed
        .get("mcp_servers")
        .and_then(toml::Value::as_table)
        .map(|servers| servers.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default()
}

fn host_config_paths(mode: HostMode) -> Vec<PathBuf> {
    match mode {
        HostMode::ClaudeCode => {
            let mut paths = host_policy::claude_hook_settings_paths();
            if let Some(project_root) = host_policy::project_root() {
                paths.push(project_root.join(".mcp.json"));
            }
            paths
        }
        HostMode::Codex => host_policy::codex_notify_config_paths(),
        HostMode::Cursor => host_policy::host_config_path(mode).into_iter().collect(),
    }
}

fn auth_freshness_for_paths(paths: &[PathBuf]) -> AuthFreshness {
    let existing = paths
        .iter()
        .filter(|path| path.exists())
        .collect::<Vec<_>>();

    if existing.is_empty() {
        return AuthFreshness::Missing;
    }

    let now = SystemTime::now();
    let mut newest: Option<SystemTime> = None;
    for path in &existing {
        let Ok(metadata) = fs::metadata(path) else {
            return AuthFreshness::Unknown;
        };
        let Ok(modified) = metadata.modified() else {
            return AuthFreshness::Unknown;
        };
        newest = Some(newest.map_or(modified, |current| current.max(modified)));
    }

    let Some(newest) = newest else {
        return AuthFreshness::Unknown;
    };

    match now.duration_since(newest) {
        Ok(age) if age <= Duration::from_secs(60 * 60 * 24 * AUTH_STALE_AFTER_DAYS) => {
            AuthFreshness::Fresh
        }
        Ok(_) => AuthFreshness::Stale,
        Err(_) => AuthFreshness::Unknown,
    }
}

fn auth_detail_for_paths(paths: &[PathBuf], freshness: AuthFreshness) -> Option<String> {
    let newest_path = paths
        .iter()
        .filter(|path| path.exists())
        .max_by_key(|path| {
            fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .ok()
        })?;

    let detail = match freshness {
        AuthFreshness::Fresh => "auth config appears fresh",
        AuthFreshness::Stale => "auth config appears stale",
        AuthFreshness::Missing => "no auth config discovered",
        AuthFreshness::Unknown => "auth freshness could not be determined",
    };

    Some(format!(
        "{} ({})",
        detail,
        host_policy::format_user_path(newest_path)
    ))
}

// ---------------------------------------------------------------------------
// Provider / API key presence checks
// ---------------------------------------------------------------------------

/// Expected key prefix for Anthropic API keys.
const ANTHROPIC_KEY_PREFIX: &str = "sk-ant-";

/// Collect API key and backend config health entries.
///
/// Checks performed:
/// - `ANTHROPIC_API_KEY` env var: present and non-empty, warn if format is unexpected.
/// - Volva backend config file: `~/.volva/auth/anthropic.json` existence and JSON validity.
///
/// Keys are **never** logged.  Missing keys produce warnings, not errors.
#[must_use]
pub(super) fn collect_api_key_health() -> Vec<ApiKeyHealth> {
    vec![check_anthropic_api_key(), check_volva_backend_config()]
}

fn check_anthropic_api_key() -> ApiKeyHealth {
    let key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
    check_anthropic_api_key_value(&key)
}

/// Core key-format logic, split out for deterministic testing without env mutation.
fn check_anthropic_api_key_value(key: &str) -> ApiKeyHealth {
    if key.is_empty() {
        return ApiKeyHealth {
            provider: "anthropic".to_string(),
            status: ApiKeyStatus::Missing,
            note: "ANTHROPIC_API_KEY is not set; some hosts use managed auth instead".to_string(),
        };
    }
    if !key.starts_with(ANTHROPIC_KEY_PREFIX) {
        return ApiKeyHealth {
            provider: "anthropic".to_string(),
            status: ApiKeyStatus::UnexpectedFormat,
            // Key value is never echoed.
            note: format!(
                "ANTHROPIC_API_KEY is set but does not start with `{ANTHROPIC_KEY_PREFIX}`"
            ),
        };
    }
    ApiKeyHealth {
        provider: "anthropic".to_string(),
        status: ApiKeyStatus::Configured,
        note: "ANTHROPIC_API_KEY is set with expected format".to_string(),
    }
}

fn volva_auth_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".volva").join("auth").join("anthropic.json"))
}

fn check_volva_backend_config() -> ApiKeyHealth {
    let Some(path) = volva_auth_config_path() else {
        return ApiKeyHealth {
            provider: "volva-backend".to_string(),
            status: ApiKeyStatus::Missing,
            note: "cannot determine home directory for volva auth config".to_string(),
        };
    };

    if !path.exists() {
        return ApiKeyHealth {
            provider: "volva-backend".to_string(),
            status: ApiKeyStatus::Missing,
            note: "~/.volva/auth/anthropic.json not found; run `volva auth login anthropic` or set ANTHROPIC_API_KEY".to_string(),
        };
    }

    // Verify the file is parseable JSON; do not log any content.
    match fs::read_to_string(&path) {
        Err(err) => ApiKeyHealth {
            provider: "volva-backend".to_string(),
            status: ApiKeyStatus::UnexpectedFormat,
            note: format!("volva auth config exists but could not be read: {err}"),
        },
        Ok(content) => {
            if serde_json::from_str::<Value>(&content).is_ok() {
                ApiKeyHealth {
                    provider: "volva-backend".to_string(),
                    status: ApiKeyStatus::Configured,
                    note: "~/.volva/auth/anthropic.json present and valid JSON".to_string(),
                }
            } else {
                ApiKeyHealth {
                    provider: "volva-backend".to_string(),
                    status: ApiKeyStatus::UnexpectedFormat,
                    note: "~/.volva/auth/anthropic.json exists but is not valid JSON".to_string(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests call `check_anthropic_api_key_value` directly to avoid env mutation
    /// and the associated race conditions in parallel test runs.

    #[test]
    fn anthropic_key_missing_when_value_is_empty() {
        let check = check_anthropic_api_key_value("");
        assert_eq!(check.status, ApiKeyStatus::Missing);
        assert!(!check.note.is_empty());
    }

    #[test]
    fn anthropic_key_unexpected_format_when_prefix_wrong() {
        let check = check_anthropic_api_key_value("wrong-format-key");
        assert_eq!(check.status, ApiKeyStatus::UnexpectedFormat);
        // Key value must not appear in output.
        assert!(!check.note.contains("wrong-format-key"));
    }

    #[test]
    fn anthropic_key_configured_when_prefix_correct() {
        let check = check_anthropic_api_key_value("sk-ant-testkey123");
        assert_eq!(check.status, ApiKeyStatus::Configured);
        // Key value must not appear in output.
        assert!(!check.note.contains("testkey123"));
    }

    #[test]
    fn anthropic_key_output_never_contains_key_value() {
        // A well-formatted key must never appear in the output note.
        let check = check_anthropic_api_key_value("sk-ant-supersecret999");
        assert!(!check.note.contains("supersecret999"), "key leaked in note");

        // A malformed key must also not appear verbatim.
        let check = check_anthropic_api_key_value("bad-secret-key-xyz");
        assert!(
            !check.note.contains("bad-secret-key-xyz"),
            "key leaked in note"
        );
    }

    #[test]
    fn volva_backend_config_check_does_not_panic() {
        // We cannot inject the home dir, so just verify the function returns a sensible
        // result on this machine (present or missing — both are valid).
        let check = check_volva_backend_config();
        let _ = match check.status {
            ApiKeyStatus::Configured => "configured",
            ApiKeyStatus::Missing => "missing",
            ApiKeyStatus::UnexpectedFormat => "unexpected-format",
        };
    }

    #[test]
    fn collect_api_key_health_returns_two_entries() {
        let health = collect_api_key_health();
        assert_eq!(health.len(), 2);
        assert!(health.iter().any(|h| h.provider == "anthropic"));
        assert!(health.iter().any(|h| h.provider == "volva-backend"));
    }
}
