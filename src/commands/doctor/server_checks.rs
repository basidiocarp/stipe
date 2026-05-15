//! MCP server binary health checks.
//!
//! For each registered MCP server (hyphae, rhizome, canopy) this module checks:
//! 1. Binary presence and executability.
//! 2. Basic responsiveness via a 3-second JSON-RPC `initialize` handshake.
//!
//! Doctor still completes even if all servers time out.

use std::time::Duration;

use crate::commands::install::release::probe_mcp_server;
use crate::commands::tool_registry::{self, ToolProbe};

use super::model::{McpServerHealth, McpServerStatus};

/// Timeout for the MCP `initialize` handshake in binary health checks.
const MCP_SERVER_HEALTH_TIMEOUT: Duration = Duration::from_secs(3);

/// Collect a [`McpServerHealth`] entry for every registered MCP server.
///
/// This runs independently of the existing `--deep` MCP startup checks.
/// Each server is checked with a 3-second timeout so a non-responding server
/// never hangs the doctor run.
///
/// MCP servers are discovered dynamically from `TOOL_SPECS` by checking for
/// entries with `mcp_serve_args` defined.
#[must_use]
pub(super) fn collect_mcp_server_health() -> Vec<McpServerHealth> {
    tool_registry::all_specs()
        .into_iter()
        .filter(|spec| spec.mcp_serve_args.is_some())
        .map(|spec| check_mcp_server_binary(spec.name))
        .collect()
}

fn check_mcp_server_binary(name: &str) -> McpServerHealth {
    let Some(spec) = tool_registry::find(name) else {
        // Server is not in the tool registry — treat as not installed.
        return McpServerHealth {
            name: name.to_string(),
            status: McpServerStatus::NotInstalled,
            detail: Some("not registered in tool registry".to_string()),
        };
    };

    let Some(serve_args) = spec.mcp_serve_args else {
        // No serve args means we cannot do a handshake — just check binary presence.
        return match tool_registry::probe(spec) {
            ToolProbe::Installed(version) => McpServerHealth {
                name: name.to_string(),
                status: McpServerStatus::Running,
                detail: Some(format!("v{version} installed")),
            },
            ToolProbe::Broken => McpServerHealth {
                name: name.to_string(),
                status: McpServerStatus::InstalledNotResponding,
                detail: Some("binary found but failed to run".to_string()),
            },
            ToolProbe::Missing => McpServerHealth {
                name: name.to_string(),
                status: McpServerStatus::NotInstalled,
                detail: None,
            },
        };
    };

    // Check binary presence first without performing the handshake.
    let probe = tool_registry::probe(spec);
    let binary_path = match probe {
        ToolProbe::Missing => {
            return McpServerHealth {
                name: name.to_string(),
                status: McpServerStatus::NotInstalled,
                detail: None,
            };
        }
        ToolProbe::Broken => {
            return McpServerHealth {
                name: name.to_string(),
                status: McpServerStatus::InstalledNotResponding,
                detail: Some("binary found but failed to run".to_string()),
            };
        }
        ToolProbe::Installed(_) => {
            let Some(path) = tool_registry::resolve_binary_path(spec) else {
                return McpServerHealth {
                    name: name.to_string(),
                    status: McpServerStatus::InstalledNotResponding,
                    detail: Some("binary path could not be resolved".to_string()),
                };
            };
            path
        }
    };

    // Binary is present and executes — attempt the MCP initialize handshake.
    match probe_mcp_server(
        &binary_path,
        serve_args,
        spec.binary_name,
        MCP_SERVER_HEALTH_TIMEOUT,
    ) {
        Ok(()) => McpServerHealth {
            name: name.to_string(),
            status: McpServerStatus::Running,
            detail: Some(format!(
                "responds to initialize within {}s",
                MCP_SERVER_HEALTH_TIMEOUT.as_secs()
            )),
        },
        Err(err) => McpServerHealth {
            name: name.to_string(),
            status: McpServerStatus::InstalledNotResponding,
            detail: Some(err),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_mcp_server_health_returns_one_entry_per_server() {
        let health = collect_mcp_server_health();
        let expected_count = tool_registry::all_specs()
            .into_iter()
            .filter(|spec| spec.mcp_serve_args.is_some())
            .count();
        assert_eq!(health.len(), expected_count);
        assert!(health.iter().any(|h| h.name == "hyphae"));
        assert!(health.iter().any(|h| h.name == "rhizome"));
    }

    #[test]
    fn mcp_server_health_status_is_one_of_three_categories() {
        // Every entry should have a valid status — just exercise the enum.
        let health = collect_mcp_server_health();
        for entry in &health {
            let _ = match entry.status {
                McpServerStatus::NotInstalled => "not-installed",
                McpServerStatus::InstalledNotResponding => "installed-not-responding",
                McpServerStatus::Running => "running",
            };
        }
    }

    #[test]
    fn mcp_server_health_check_completes_without_hanging() {
        // If a server is installed but not responding, the call must return within
        // the timeout budget.  This exercises the non-blocking contract.
        let start = std::time::Instant::now();
        let _health = collect_mcp_server_health();
        // Each server allows at most 3 seconds; three servers = 9 seconds.
        // In practice installed-but-silent servers hit exactly the timeout, so
        // we give 30 seconds of headroom for slow CI machines.
        assert!(start.elapsed().as_secs() < 30);
    }
}
