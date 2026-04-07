use std::path::{Path, PathBuf};

use super::model::HealthCheck;
use super::tool_registry::{self, DoctorCoverage, ToolProbe, ToolSpec};
use crate::commands::host_policy;
use crate::commands::install::release::{probe_mcp_server, verify_functional};
use crate::commands::install::{
    InstallProfile, ManualProfileMember, expected_profile_tools, manual_member,
};
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

fn check_expected_tool(spec: &ToolSpec, profile: InstallProfile, deep: bool) -> HealthCheck {
    match tool_registry::probe_with_level(spec, tool_registry::VerifyLevel::Version) {
        ToolProbe::Installed(version) => {
            if deep
                && let Some(binary_path) = tool_registry::resolve_binary_path(spec)
                && let Err(error) = verify_functional(&binary_path, spec)
            {
                return HealthCheck {
                    name: spec.name.to_string(),
                    passed: false,
                    message: format!(
                        "v{version} installed but functional check failed: {error} (expected by {})",
                        profile.mode_label()
                    ),
                    repair_actions: missing_tool_actions(spec),
                };
            }

            HealthCheck {
                name: spec.name.to_string(),
                passed: true,
                message: format!(
                    "v{version} installed (expected by {})",
                    profile.mode_label()
                ),
                repair_actions: Vec::new(),
            }
        }
        ToolProbe::Broken => HealthCheck {
            name: spec.name.to_string(),
            passed: false,
            message: format!(
                "Binary found but failed to run (expected by {})",
                profile.mode_label()
            ),
            repair_actions: missing_tool_actions(spec),
        },
        ToolProbe::Missing => HealthCheck {
            name: spec.name.to_string(),
            passed: false,
            message: format!("Not installed (expected by {})", profile.mode_label()),
            repair_actions: missing_tool_actions(spec),
        },
    }
}

fn push_candidate_root(roots: &mut Vec<PathBuf>, candidate: Option<PathBuf>) {
    if let Some(candidate) = candidate
        && !roots.iter().any(|existing| existing == &candidate)
    {
        roots.push(candidate);
    }
}

fn candidate_workspace_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(cwd) = std::env::current_dir() {
        let project_root = spore::paths::find_project_root(&cwd).unwrap_or(cwd.clone());
        push_candidate_root(&mut roots, Some(project_root.clone()));
        push_candidate_root(&mut roots, project_root.parent().map(Path::to_path_buf));
    }

    push_candidate_root(
        &mut roots,
        dirs::home_dir().map(|home| home.join("projects").join("claude-mycelium")),
    );

    roots
}

fn lamella_root_installed(path: &Path) -> bool {
    path.join("lamella").exists() && path.join("resources").exists()
}

fn cap_root_installed(path: &Path) -> bool {
    path.join("package.json").exists()
}

fn manual_tool_installed_in_roots(member: ManualProfileMember, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| match member.name {
        "lamella" => lamella_root_installed(root) || lamella_root_installed(&root.join("lamella")),
        "cap" => cap_root_installed(root) || cap_root_installed(&root.join("cap")),
        _ => false,
    })
}

fn manual_tool_installed(member: ManualProfileMember) -> bool {
    manual_tool_installed_in_roots(member, &candidate_workspace_roots())
}

fn manual_tool_action(member: ManualProfileMember) -> RepairAction {
    RepairAction::manual(
        format!("Install {}", member.name),
        format!("Install {} for the selected profile.", member.name),
        member.install_hint.to_string(),
        vec![member.install_hint.to_string()],
        RepairTier::Manual,
    )
}

fn check_manual_profile_tool(member: ManualProfileMember, profile: InstallProfile) -> HealthCheck {
    if manual_tool_installed(member) {
        HealthCheck {
            name: member.name.to_string(),
            passed: true,
            message: format!("installed (expected by {})", profile.mode_label()),
            repair_actions: Vec::new(),
        }
    } else {
        HealthCheck {
            name: member.name.to_string(),
            passed: false,
            message: format!("Not installed (expected by {})", profile.mode_label()),
            repair_actions: vec![manual_tool_action(member)],
        }
    }
}

pub(super) fn check_profile_tools(profile: InstallProfile, deep: bool) -> Vec<HealthCheck> {
    expected_profile_tools(profile)
        .into_iter()
        .filter_map(|tool_name| {
            if let Some(member) = manual_member(&tool_name) {
                Some(check_manual_profile_tool(member, profile))
            } else {
                tool_registry::find(&tool_name).map(|spec| check_expected_tool(spec, profile, deep))
            }
        })
        .collect()
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

#[allow(dead_code)]
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

    #[test]
    fn manual_tools_detect_standalone_repo_roots() {
        let temp_dir = std::env::temp_dir().join(format!(
            "stipe-manual-tool-standalone-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);

        let lamella_root = temp_dir.join("lamella");
        fs::create_dir_all(lamella_root.join("resources")).unwrap();
        fs::write(lamella_root.join("lamella"), "").unwrap();

        let cap_root = temp_dir.join("cap");
        fs::create_dir_all(&cap_root).unwrap();
        fs::write(cap_root.join("package.json"), "{}").unwrap();

        assert!(manual_tool_installed_in_roots(
            manual_member("lamella").expect("lamella member"),
            std::slice::from_ref(&lamella_root)
        ));
        assert!(manual_tool_installed_in_roots(
            manual_member("cap").expect("cap member"),
            std::slice::from_ref(&cap_root)
        ));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn manual_tools_detect_workspace_sibling_repos() {
        let temp_dir = std::env::temp_dir().join(format!(
            "stipe-manual-tool-workspace-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);

        let workspace_root = temp_dir.join("claude-mycelium");
        let stipe_root = workspace_root.join("stipe");
        fs::create_dir_all(&stipe_root).unwrap();
        fs::write(stipe_root.join("Cargo.toml"), "[package]\nname = \"stipe\"\n").unwrap();

        let lamella_root = workspace_root.join("lamella");
        fs::create_dir_all(lamella_root.join("resources")).unwrap();
        fs::write(lamella_root.join("lamella"), "").unwrap();

        assert!(manual_tool_installed_in_roots(
            manual_member("lamella").expect("lamella member"),
            &[stipe_root, workspace_root]
        ));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
