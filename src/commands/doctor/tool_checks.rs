use std::collections::HashMap;
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

// ---------------------------------------------------------------------------
// Version drift detection
// ---------------------------------------------------------------------------

/// Pinned tool versions from ecosystem-versions.toml. Must be kept in sync.
fn pinned_ecosystem_versions() -> HashMap<&'static str, &'static str> {
    let mut pins = HashMap::new();
    pins.insert("mycelium", "0.9.0");
    pins.insert("hyphae", "0.12.0");
    pins.insert("rhizome", "0.8.0");
    pins.insert("canopy", "0.6.0");
    pins.insert("cortina", "0.3.0");
    pins.insert("stipe", "0.6.0");
    pins.insert("volva", "0.2.4");
    pins.insert("hymenium", "0.6.0");
    pins.insert("annulus", "0.5.5");
    pins.insert("cap", "0.11.9");
    pins.insert("lamella", "0.5.15");
    pins.insert("spore", "0.4.11");
    pins
}

/// Check if an installed version is behind the pinned version.
/// Returns (`is_behind`, `pinned_version`) or (false, None) if tool not in pins.
fn check_version_drift(tool_name: &str, installed: &str) -> (bool, Option<String>) {
    let pins = pinned_ecosystem_versions();
    match pins.get(tool_name) {
        Some(&pinned) => (installed != pinned, Some(pinned.to_string())),
        None => (false, None),
    }
}

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
        "annulus" => vec![RepairAction::stipe(
            "install-annulus",
            "Install Annulus",
            "Install the operator utilities CLI.",
            &["install", "annulus"],
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

            // Check for version drift.
            let (is_behind, pinned) = check_version_drift(spec.name, &version);
            let (message, repair_actions) = if is_behind {
                let pinned_str = pinned.as_deref().unwrap_or("unknown");
                let update_action = RepairAction::manual(
                    format!("update_{}", spec.name),
                    format!("Update {}", spec.name),
                    format!(
                        "Replace the installed {} with the pinned version {}.",
                        spec.name, pinned_str
                    ),
                    format!("stipe update {}", spec.name),
                    vec!["update".to_string(), spec.name.to_string()],
                    RepairTier::Primary,
                );
                (
                    format!(
                        "v{version} installed (pinned: v{pinned_str} — run 'stipe update {name}' to update)",
                        name = spec.name
                    ),
                    vec![update_action],
                )
            } else {
                (format!("v{version} installed and working"), Vec::new())
            };

            HealthCheck {
                name: spec.name.to_string(),
                passed: true,
                message,
                repair_actions,
            }
        }
        (DoctorCoverage::Optional, ToolProbe::Missing) => HealthCheck {
            name: spec.name.to_string(),
            passed: true,
            message: match spec.name {
                "volva" => "Optional backend operations CLI not installed".to_string(),
                "annulus" => "Optional operator utilities CLI not installed".to_string(),
                _ => "Optional coordination runtime not installed".to_string(),
            },
            repair_actions: if matches!(spec.name, "volva" | "annulus") {
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

            // Check for version drift.
            let (is_behind, pinned) = check_version_drift(spec.name, &version);
            let (message, repair_actions) = if is_behind {
                let pinned_str = pinned.as_deref().unwrap_or("unknown");
                let update_action = RepairAction::manual(
                    format!("update_{}", spec.name),
                    format!("Update {}", spec.name),
                    format!(
                        "Replace the installed {} with the pinned version {}.",
                        spec.name, pinned_str
                    ),
                    format!("stipe update {}", spec.name),
                    vec!["update".to_string(), spec.name.to_string()],
                    RepairTier::Primary,
                );
                (
                    format!(
                        "v{version} installed (pinned: v{pinned_str} — expected by {})",
                        profile.mode_label()
                    ),
                    vec![update_action],
                )
            } else {
                (
                    format!(
                        "v{version} installed (expected by {})",
                        profile.mode_label()
                    ),
                    Vec::new(),
                )
            };

            HealthCheck {
                name: spec.name.to_string(),
                passed: true,
                message,
                repair_actions,
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
        dirs::home_dir().map(|home| home.join("projects").join("basidiocarp")),
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
    let action_key = format!(
        "install_manual_{}",
        member.name.to_lowercase().replace('-', "_")
    );
    RepairAction::manual(
        action_key,
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
    let Some(data_dir) = dirs::data_dir() else {
        return HealthCheck {
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
        };
    };

    // New canonical path under the shared basidiocarp root.
    let new_path = data_dir
        .join("basidiocarp")
        .join("hyphae")
        .join("hyphae.db");
    if new_path.exists() {
        return check_hyphae_db_at_path(&new_path);
    }

    // Legacy path — present if hyphae hasn't launched since the path migration.
    // The migration runs automatically on next hyphae startup.
    let legacy_path = data_dir.join("hyphae").join("hyphae.db");
    if legacy_path.exists() {
        return HealthCheck {
            name: "hyphae database".to_string(),
            passed: true,
            message: "Database at legacy path (will migrate on next hyphae launch)".to_string(),
            repair_actions: Vec::new(),
        };
    }

    check_hyphae_db_at_path(&new_path)
}

/// Check the shared `~/.local/share/basidiocarp/` storage root and each tool subdirectory.
pub(super) fn check_shared_storage_root() -> HealthCheck {
    let Some(data_dir) = dirs::data_dir() else {
        return HealthCheck {
            name: "shared storage root".to_string(),
            passed: false,
            message: "Cannot determine data directory".to_string(),
            repair_actions: Vec::new(),
        };
    };

    let root = data_dir.join("basidiocarp");

    // (subdirectory, db filename)
    let tools: &[(&str, &str)] = &[
        ("hyphae", "hyphae.db"),
        ("canopy", "canopy.db"),
        ("cortina", "cortina-sessions.db"),
    ];

    let parts: Vec<String> = tools
        .iter()
        .map(|(name, db_file)| {
            let status = if root.join(name).join(db_file).exists() {
                "✓"
            } else {
                "—"
            };
            format!("{name} {status}")
        })
        .collect();

    let message = format!("~/.local/share/basidiocarp/  {}", parts.join("  "));

    HealthCheck {
        name: "shared storage root".to_string(),
        passed: root.exists(),
        message,
        repair_actions: if root.exists() {
            Vec::new()
        } else {
            vec![RepairAction::stipe(
                "init",
                "Initialize the ecosystem",
                "Create the shared basidiocarp storage root.",
                &["init"],
                RepairTier::Primary,
            )]
        },
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

/// Check that Canopy's SQLite database is running in WAL mode.
///
/// Queries via the `sqlite3` CLI if available. Passes as advisory if sqlite3
/// is not installed, since WAL mode is unconditionally set at Canopy startup.
pub(super) fn check_canopy_wal_mode() -> HealthCheck {
    let Some(data_dir) = dirs::data_dir() else {
        return HealthCheck {
            name: "canopy WAL mode".to_string(),
            passed: true,
            message: "Cannot determine data directory (advisory)".to_string(),
            repair_actions: Vec::new(),
        };
    };

    let db_path = data_dir
        .join("basidiocarp")
        .join("canopy")
        .join("canopy.db");

    if !db_path.exists() {
        return HealthCheck {
            name: "canopy WAL mode".to_string(),
            passed: true,
            message: "Canopy database not initialized (WAL mode set on first run)".to_string(),
            repair_actions: Vec::new(),
        };
    }

    match std::process::Command::new("sqlite3")
        .arg(&db_path)
        .arg("PRAGMA journal_mode;")
        .output()
    {
        Ok(output) if output.status.success() => {
            let mode = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_string();
            if mode == "wal" {
                HealthCheck {
                    name: "canopy WAL mode".to_string(),
                    passed: true,
                    message: "WAL mode active".to_string(),
                    repair_actions: Vec::new(),
                }
            } else {
                HealthCheck {
                    name: "canopy WAL mode".to_string(),
                    passed: false,
                    message: format!("journal_mode is '{mode}', expected 'wal' — restart Canopy to apply"),
                    repair_actions: Vec::new(),
                }
            }
        }
        Ok(_) => HealthCheck {
            name: "canopy WAL mode".to_string(),
            passed: true,
            message: "sqlite3 query failed (database may be locked); WAL mode set at Canopy startup (advisory)".to_string(),
            repair_actions: Vec::new(),
        },
        Err(_) => HealthCheck {
            name: "canopy WAL mode".to_string(),
            passed: true,
            message: "sqlite3 not available; WAL mode set at Canopy startup (advisory)".to_string(),
            repair_actions: Vec::new(),
        },
    }
}

pub(super) fn check_rhizome_compiled_env() -> HealthCheck {
    // Check if rhizome is available
    let rhizome_available = std::process::Command::new("rhizome")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());

    if !rhizome_available {
        return HealthCheck {
            name: "rhizome compiled environment".to_string(),
            passed: true,
            message: "Rhizome not installed (skipped)".to_string(),
            repair_actions: Vec::new(),
        };
    }

    // Check if a compiled-env memoir exists in hyphae
    let artifact_exists = std::process::Command::new("hyphae")
        .args(["memoir", "show", "--name", "compiled-env:*"])
        .output()
        .is_ok_and(|o| o.status.success());

    if artifact_exists {
        HealthCheck {
            name: "rhizome compiled environment".to_string(),
            passed: true,
            message: "Compiled environment artifact exists in Hyphae".to_string(),
            repair_actions: Vec::new(),
        }
    } else {
        HealthCheck {
            name: "rhizome compiled environment".to_string(),
            passed: true,
            message: "No compiled environment artifact; run 'rhizome compile-env' to generate one (optional)".to_string(),
            repair_actions: Vec::new(),
        }
    }
}

pub(super) fn check_mcp_startups() -> Vec<HealthCheck> {
    tool_registry::doctor_specs()
        .into_iter()
        .filter_map(check_mcp_startup)
        .collect()
}

/// Check whether the capability registry file exists and is current.
///
/// Returns a failing check if the registry is absent or stale (older than 30
/// days), prompting the operator to run `stipe init` to refresh it.
pub(super) fn check_capability_registry_health(registry_path: &Path) -> HealthCheck {
    if !registry_path.exists() {
        return HealthCheck {
            name: "capability registry".to_string(),
            passed: false,
            message: "Capability registry not found; run `stipe init` to generate it".to_string(),
            repair_actions: vec![RepairAction::stipe(
                "init",
                "Initialize the ecosystem",
                "Generate the capability registry and wire the local ecosystem together.",
                &["init"],
                RepairTier::Primary,
            )],
        };
    }

    let stale = std::fs::metadata(registry_path)
        .and_then(|m| m.modified())
        .is_ok_and(|modified| {
            modified
                .elapsed()
                .is_ok_and(|age| age > std::time::Duration::from_secs(30 * 24 * 60 * 60))
        });

    if stale {
        HealthCheck {
            name: "capability registry".to_string(),
            passed: false,
            message: "Capability registry is stale; run `stipe init` to refresh it".to_string(),
            repair_actions: vec![RepairAction::stipe(
                "init",
                "Refresh ecosystem state",
                "Regenerate the stale capability registry with the current tool set.",
                &["init"],
                RepairTier::Primary,
            )],
        }
    } else {
        HealthCheck {
            name: "capability registry".to_string(),
            passed: true,
            message: "Capability registry present and current".to_string(),
            repair_actions: Vec::new(),
        }
    }
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

        // Verify the probe returns an error for a hanging server.
        // The exact message varies with scheduling (timeout vs. early-close),
        // so we only assert that an error is returned.
        let _error = probe_mcp_server(
            &script,
            &[],
            "hyphae",
            std::time::Duration::from_millis(100),
        )
        .unwrap_err();

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

        let workspace_root = temp_dir.join("basidiocarp");
        let stipe_root = workspace_root.join("stipe");
        fs::create_dir_all(&stipe_root).unwrap();
        fs::write(
            stipe_root.join("Cargo.toml"),
            "[package]\nname = \"stipe\"\n",
        )
        .unwrap();

        let lamella_root = workspace_root.join("lamella");
        fs::create_dir_all(lamella_root.join("resources")).unwrap();
        fs::write(lamella_root.join("lamella"), "").unwrap();

        assert!(manual_tool_installed_in_roots(
            manual_member("lamella").expect("lamella member"),
            &[stipe_root, workspace_root]
        ));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    // -- version drift detection ------------------------------------------------

    #[test]
    fn check_version_drift_detects_stale_binary() {
        let (is_behind, pinned) = check_version_drift("hyphae", "0.11.0");
        assert!(
            is_behind,
            "older version should be reported as behind the pin"
        );
        assert_eq!(
            pinned.as_deref(),
            Some("0.12.0"),
            "pinned version should be returned"
        );
    }

    #[test]
    fn check_version_drift_accepts_current_binary() {
        let (is_behind, pinned) = check_version_drift("hyphae", "0.12.0");
        assert!(
            !is_behind,
            "matching version should not be reported as behind"
        );
        assert_eq!(pinned.as_deref(), Some("0.12.0"));
    }

    #[test]
    fn check_version_drift_unknown_tool_never_reports_behind() {
        let (is_behind, pinned) = check_version_drift("not-a-real-tool", "9.9.9");
        assert!(
            !is_behind,
            "unknown tool should never be reported as behind"
        );
        assert!(
            pinned.is_none(),
            "unknown tool should return no pinned version"
        );
    }

    #[test]
    fn check_version_drift_covers_hymenium_and_canopy() {
        // Verify the two tools most relevant to dogfood freshness are in the pin table.
        let (_, hymenium_pin) = check_version_drift("hymenium", "0.0.0");
        let (_, canopy_pin) = check_version_drift("canopy", "0.0.0");
        assert!(hymenium_pin.is_some(), "hymenium must have a version pin");
        assert!(canopy_pin.is_some(), "canopy must have a version pin");
    }
}
