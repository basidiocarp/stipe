use std::path::Path;

use super::model::HealthCheck;
use super::tool_registry::{self, DoctorCoverage, ToolProbe, ToolSpec};
use crate::commands::host_policy;
use crate::commands::install::release::{probe_mcp_server, verify_functional};
use crate::commands::repair::{RepairAction, RepairTier, cargo_install_action};
use crate::ecosystem::clients::{self, McpClient};

fn codex_cli_installed() -> bool {
    std::process::Command::new("codex")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn codex_environment_present() -> bool {
    codex_cli_installed() || clients::detect_clients().contains(&McpClient::CodexCli)
}

fn missing_tool_actions(tool: &ToolSpec) -> Vec<RepairAction> {
    let install_profile = host_policy::preferred_install_profile(
        if codex_environment_present() {
            Some(host_policy::CODEX_CLIENT_FLAG)
        } else {
            None
        },
        &clients::detect_clients()
            .into_iter()
            .map(|client| client.name().to_string())
            .collect::<Vec<_>>(),
    );

    match tool.name {
        "mycelium" => vec![RepairAction::stipe(
            "install-minimal",
            "Install the minimal profile",
            "Restore the Mycelium CLI before attempting deeper repair work.",
            &["install", "--profile", "minimal"],
            RepairTier::Primary,
        )],
        "hyphae" | "rhizome" => vec![
            host_policy::install_profile_repair_action(install_profile),
            RepairAction::stipe(
                "install-full-stack",
                "Install the full stack",
                "Install every supported ecosystem tool when you want the broadest local setup.",
                &["install", "--profile", "full-stack"],
                RepairTier::Secondary,
            ),
            match tool.name {
                "hyphae" => cargo_install_action("hyphae"),
                "rhizome" => cargo_install_action("rhizome"),
                _ => unreachable!(),
            },
        ],
        "canopy" => vec![
            RepairAction::stipe(
                "install-canopy",
                "Install Canopy",
                "Install the optional coordination runtime.",
                &["install", "canopy"],
                RepairTier::Primary,
            ),
            RepairAction::stipe(
                "install-full-stack",
                "Install the full stack",
                "Install every supported ecosystem tool when you want the broadest local setup.",
                &["install", "--profile", "full-stack"],
                RepairTier::Secondary,
            ),
        ],
        "volva" => vec![RepairAction::stipe(
            "install-volva",
            "Install Volva",
            "Install the backend operations CLI.",
            &["install", "volva"],
            RepairTier::Primary,
        )],
        _ => Vec::new(),
    }
}

fn mcp_startup_actions(tool_name: &'static str) -> Vec<RepairAction> {
    let mut actions = vec![
        RepairAction::stipe(
            "init",
            "Reinitialize the ecosystem",
            "Re-register MCP servers and repair shared ecosystem state.",
            &["init"],
            RepairTier::Primary,
        ),
        RepairAction::stipe(
            "host-setup-claude-code",
            "Refresh Claude Code host setup",
            "Rewrite Claude Code MCP configuration with the expected PATH-based commands.",
            &["host", "setup", "claude-code"],
            RepairTier::Secondary,
        ),
        RepairAction::stipe(
            "host-setup-codex",
            "Refresh Codex host setup",
            "Rewrite Codex MCP configuration with the expected PATH-based commands.",
            &["host", "setup", "codex"],
            RepairTier::Secondary,
        ),
    ];

    let update_args = ["update", tool_name];
    actions.push(RepairAction::stipe(
        if tool_name == "hyphae" {
            "update-hyphae"
        } else {
            "update-rhizome"
        },
        if tool_name == "hyphae" {
            "Update Hyphae"
        } else {
            "Update Rhizome"
        },
        "Replace the installed binary with the latest managed release.",
        &update_args,
        RepairTier::Secondary,
    ));

    actions
}

fn check_mcp_startup(spec: &ToolSpec) -> Option<HealthCheck> {
    let args = spec.mcp_serve_args?;
    let ToolProbe::Installed(_) =
        tool_registry::probe_with_level(spec, tool_registry::VerifyLevel::Version)
    else {
        return None;
    };
    let binary_path = tool_registry::resolve_binary_path(spec)?;

    Some(
        match probe_mcp_server(
            &binary_path,
            args,
            spec.binary_name,
            crate::commands::install::release::MCP_HANDSHAKE_TIMEOUT,
        ) {
            Ok(()) => HealthCheck {
                name: format!("{} MCP startup", spec.name),
                passed: true,
                message: format!(
                    "responds to initialize within {}s",
                    crate::commands::install::release::MCP_HANDSHAKE_TIMEOUT.as_secs()
                ),
                repair_actions: Vec::new(),
            },
            Err(message) => HealthCheck {
                name: format!("{} MCP startup", spec.name),
                passed: false,
                message,
                repair_actions: mcp_startup_actions(spec.name),
            },
        },
    )
}

pub(super) fn check_tool(spec: &ToolSpec, deep: bool) -> HealthCheck {
    match (
        spec.doctor_coverage,
        tool_registry::probe_with_level(spec, tool_registry::VerifyLevel::Version),
    ) {
        (_, ToolProbe::Installed(version)) => {
            if deep
                && let Some(binary_path) = tool_registry::resolve_binary_path(spec)
                && let Err(error) = verify_functional(&binary_path, spec)
            {
                return HealthCheck {
                    name: spec.name.to_string(),
                    passed: false,
                    message: format!("v{version} installed but functional check failed: {error}"),
                    repair_actions: missing_tool_actions(spec),
                };
            }

            HealthCheck {
                name: spec.name.to_string(),
                passed: true,
                message: format!("v{version} installed and working"),
                repair_actions: Vec::new(),
            }
        }
        (DoctorCoverage::Optional, ToolProbe::Missing) => HealthCheck {
            name: spec.name.to_string(),
            passed: true,
            message: if spec.name == "volva" {
                "Optional backend operations CLI not installed".to_string()
            } else {
                "Optional coordination runtime not installed".to_string()
            },
            repair_actions: if spec.name == "volva" {
                missing_tool_actions(spec)
            } else {
                Vec::new()
            },
        },
        (_, ToolProbe::Broken) => HealthCheck {
            name: spec.name.to_string(),
            passed: false,
            message: "Binary found but failed to run".to_string(),
            repair_actions: missing_tool_actions(spec),
        },
        (DoctorCoverage::Required, ToolProbe::Missing) => HealthCheck {
            name: spec.name.to_string(),
            passed: false,
            message: "Not installed".to_string(),
            repair_actions: missing_tool_actions(spec),
        },
        (DoctorCoverage::Ignore, _) => unreachable!(),
    }
}

pub(super) fn check_hyphae_db() -> HealthCheck {
    if let Some(data_dir) = dirs::data_dir() {
        check_hyphae_db_at_path(&data_dir.join("hyphae").join("hyphae.db"))
    } else {
        HealthCheck {
            name: "hyphae database".to_string(),
            passed: false,
            message: "Cannot determine data directory".to_string(),
            repair_actions: vec![RepairAction::stipe(
                "init",
                "Initialize the ecosystem",
                "Bootstrap Hyphae and MCP client state on this machine.",
                &["init"],
                RepairTier::Primary,
            )],
        }
    }
}

pub(super) fn check_hyphae_db_at_path(db_path: &Path) -> HealthCheck {
    if db_path.exists() {
        HealthCheck {
            name: "hyphae database".to_string(),
            passed: true,
            message: "Database initialized".to_string(),
            repair_actions: Vec::new(),
        }
    } else {
        HealthCheck {
            name: "hyphae database".to_string(),
            passed: false,
            message: "Database not found (run 'stipe init' to initialize)".to_string(),
            repair_actions: vec![RepairAction::stipe(
                "init",
                "Initialize the ecosystem",
                "Create the Hyphae database and wire the local ecosystem together.",
                &["init"],
                RepairTier::Primary,
            )],
        }
    }
}

pub(super) fn check_mcp_startups() -> Vec<HealthCheck> {
    tool_registry::doctor_specs()
        .into_iter()
        .filter_map(check_mcp_startup)
        .collect()
}

pub(super) fn installed_mcp_servers() -> Vec<&'static str> {
    tool_registry::doctor_specs()
        .into_iter()
        .filter(|spec| spec.mcp_serve_args.is_some())
        .filter(|spec| matches!(tool_registry::probe(spec), ToolProbe::Installed(_)))
        .map(|spec| spec.name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::install::release::parse_initialize_response;
    use std::fs;

    #[test]
    fn parse_initialize_response_accepts_expected_server() {
        let line = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","serverInfo":{"name":"hyphae"}}}"#;
        assert!(parse_initialize_response(line, "hyphae").is_ok());
    }

    #[test]
    fn parse_initialize_response_rejects_wrong_server() {
        let line = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","serverInfo":{"name":"rhizome"}}}"#;
        let error = parse_initialize_response(line, "hyphae").unwrap_err();
        assert!(error.contains("instead of `hyphae`"));
    }

    #[cfg(unix)]
    #[test]
    fn probe_mcp_server_accepts_initialize_response() {
        use std::os::unix::fs::PermissionsExt;

        let dir =
            std::env::temp_dir().join(format!("stipe-tool-checks-{}-{}", std::process::id(), "ok"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let script = dir.join("fake-mcp.sh");
        fs::write(
            &script,
            "#!/bin/sh\nread line\nprintf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":\"2024-11-05\",\"serverInfo\":{\"name\":\"hyphae\"}}}'\n",
        )
        .unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();

        assert!(
            probe_mcp_server(
                &script,
                &[],
                "hyphae",
                crate::commands::install::release::MCP_HANDSHAKE_TIMEOUT
            )
            .is_ok()
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn probe_mcp_server_times_out_cleanly() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!(
            "stipe-tool-checks-{}-{}",
            std::process::id(),
            "timeout"
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let script = dir.join("fake-hang.sh");
        fs::write(&script, "#!/bin/sh\nsleep 1\n").unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();

        let error = probe_mcp_server(
            &script,
            &[],
            "hyphae",
            std::time::Duration::from_millis(100),
        )
        .unwrap_err();
        assert!(error.contains("timed out"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_volva_has_an_install_repair_action() {
        let volva = tool_registry::find("volva").expect("volva spec should exist");
        let actions = missing_tool_actions(volva);

        assert!(
            actions
                .iter()
                .any(|action| action.command == "stipe install volva")
        );
    }
}
