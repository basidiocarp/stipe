use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use super::model::HealthCheck;
use super::tool_registry::{self, DoctorCoverage, ToolProbe, ToolSpec};
use super::version_pins;
use crate::commands::claude_hooks;
use crate::commands::host_policy;
use crate::commands::install::release::{
    normalize_version, probe_mcp_server, run_command_with_timeout, verify_functional,
};
use crate::commands::install::{
    InstallProfile, ManualProfileMember, expected_profile_tools, manual_member,
};
use crate::commands::repair::{RepairAction, RepairTier, cargo_install_action};
use crate::ecosystem::clients::{self, McpClient};

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

// Live versions fetched from GitHub at doctor startup. When set, these take
// precedence over the compiled-in pins in `check_version_drift`.
static LIVE_VERSIONS: OnceLock<HashMap<String, String>> = OnceLock::new();

/// Pre-load live GitHub versions before running doctor checks.
/// Called once per doctor invocation; no-op if already initialized.
pub(super) fn init_live_versions(live: HashMap<String, String>) {
    let _ = LIVE_VERSIONS.set(live);
}

// ---------------------------------------------------------------------------
// Version drift detection
// ---------------------------------------------------------------------------

/// Parse a semantic version string into (major, minor, patch).
/// Handles leading 'v', pre-release suffixes (e.g., "1.2.3-alpha"), and missing patch.
/// Returns None if the string cannot be parsed.
fn parse_semver(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim().trim_start_matches('v');
    // Strip pre-release suffix (everything after the first dash)
    let s = s.split('-').next().unwrap_or(s);
    let mut parts = s.splitn(3, '.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

/// Check if an installed version is behind the latest known version.
/// Prefers live GitHub versions (when pre-fetched) over compiled-in pins.
/// Returns (`is_behind`, `latest_version`, `message_override`) where:
/// - `is_behind` = true only when installed < latest (semver comparison)
/// - `latest_version` = the latest version string or None if unknown
/// - `message_override` = Some(msg) if the version is ahead of latest (newer install),
///   otherwise None (use default message)
fn check_version_drift(tool_name: &str, installed: &str) -> (bool, Option<String>, Option<String>) {
    // Prefer live GitHub versions; fall back to compiled-in pins when unavailable.
    let latest: Option<String> = LIVE_VERSIONS
        .get()
        .and_then(|live| live.get(tool_name).cloned())
        .or_else(|| {
            version_pins::pinned_ecosystem_versions()
                .get(tool_name)
                .map(ToString::to_string)
        });

    match latest {
        Some(pinned) => {
            // Try semver comparison first.
            if let (Some(inst_semver), Some(pin_semver)) =
                (parse_semver(installed), parse_semver(&pinned))
            {
                let is_behind = inst_semver < pin_semver;
                let message_override = if inst_semver > pin_semver {
                    Some(format!(
                        "v{} installed (ahead of latest v{}; no action needed)",
                        installed, pinned
                    ))
                } else {
                    None
                };
                return (is_behind, Some(pinned), message_override);
            }

            // Fall back to string comparison if semver parsing fails.
            let installed_norm = normalize_version(installed);
            let pinned_norm = normalize_version(&pinned);
            (installed_norm != pinned_norm, Some(pinned), None)
        }
        None => (false, None, None),
    }
}

fn codex_cli_installed() -> bool {
    let mut cmd = Command::new("codex");
    cmd.arg("--version");
    match run_command_with_timeout(&mut cmd, PROBE_TIMEOUT) {
        Ok(o) => {
            if o.status.success() {
                true
            } else {
                tracing::debug!("codex --version returned non-zero exit code");
                false
            }
        }
        Err(e) if e.kind() == io::ErrorKind::TimedOut => {
            tracing::debug!("codex --version timed out");
            false
        }
        Err(_) => {
            tracing::debug!("codex --version failed to run");
            false
        }
    }
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
        "hymenium" => vec![RepairAction::stipe(
            "install-hymenium",
            "Install Hymenium",
            "Install the workflow orchestration engine.",
            &["install", "hymenium"],
            RepairTier::Primary,
        )],
        "cortina" => vec![RepairAction::stipe(
            "install-cortina",
            "Install Cortina",
            "Install the hook runner and session tracking tool.",
            &["install", "cortina"],
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

    let action_key = format!("update-{tool_name}");
    let action_title = format!("Update {tool_name}");
    let command = format!("stipe update {tool_name}");
    actions.push(RepairAction::manual(
        action_key,
        action_title,
        "Replace the installed binary with the latest managed release.".to_string(),
        command,
        vec!["update".to_string(), tool_name.to_string()],
        RepairTier::Secondary,
    ));

    actions
}

fn check_mcp_startup(spec: &ToolSpec) -> Option<HealthCheck> {
    let Some(args) = spec.mcp_serve_args else {
        return Some(HealthCheck {
            name: format!("{} MCP startup", spec.name),
            passed: true,
            message: "MCP server args not configured; probe skipped".to_string(),
            repair_actions: Vec::new(),
            suppressed: false,
        });
    };
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
                suppressed: false,
            },
            Err(message) => HealthCheck {
                name: format!("{} MCP startup", spec.name),
                passed: false,
                message,
                repair_actions: mcp_startup_actions(spec.name),
                suppressed: false,
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
                    suppressed: false,
                };
            }

            // Check for version drift.
            let (is_behind, pinned, msg_override) = check_version_drift(spec.name, &version);
            let (message, repair_actions) = if let Some(msg) = msg_override {
                // Version is ahead of pin.
                (msg, Vec::new())
            } else if is_behind {
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
                        "v{version} installed (latest: v{pinned_str} — run 'stipe update {name}' to update)",
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
                suppressed: false,
            }
        }
        (DoctorCoverage::Optional, ToolProbe::Missing) => HealthCheck {
            name: spec.name.to_string(),
            passed: true,
            message: format!("{} not installed (optional)", spec.description),
            repair_actions: missing_tool_actions(spec),
            suppressed: false,
        },
        (_, ToolProbe::Broken) => HealthCheck {
            name: spec.name.to_string(),
            passed: false,
            message: "Binary found but failed to run".to_string(),
            repair_actions: missing_tool_actions(spec),
            suppressed: false,
        },
        (DoctorCoverage::Required, ToolProbe::Missing) => HealthCheck {
            name: spec.name.to_string(),
            passed: false,
            message: "Not installed".to_string(),
            repair_actions: missing_tool_actions(spec),
            suppressed: false,
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
                    suppressed: false,
                };
            }

            // Check for version drift.
            let (is_behind, pinned, msg_override) = check_version_drift(spec.name, &version);
            let (message, repair_actions) = if let Some(msg) = msg_override {
                // Version is ahead of pin.
                (msg, Vec::new())
            } else if is_behind {
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
                        "v{version} installed (latest: v{pinned_str} — expected by {})",
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
                suppressed: false,
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
            suppressed: false,
        },
        ToolProbe::Missing => HealthCheck {
            name: spec.name.to_string(),
            passed: false,
            message: format!("Not installed (expected by {})", profile.mode_label()),
            repair_actions: missing_tool_actions(spec),
            suppressed: false,
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
            suppressed: false,
        }
    } else {
        HealthCheck {
            name: member.name.to_string(),
            passed: false,
            message: format!("Not installed (expected by {})", profile.mode_label()),
            repair_actions: vec![manual_tool_action(member)],
            suppressed: false,
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

pub(super) fn check_codex_notify() -> Option<HealthCheck> {
    if !codex_cli_installed() {
        return None;
    }

    let hyphae_installed = tool_registry::find("hyphae")
        .is_some_and(|spec| matches!(tool_registry::probe(spec), ToolProbe::Installed(_)));

    if !hyphae_installed {
        return Some(HealthCheck {
            name: "codex notify adapter".to_string(),
            passed: false,
            message: "Hyphae is not installed — Codex notify adapter cannot be registered"
                .to_string(),
            repair_actions: vec![RepairAction::stipe(
                "install-hyphae",
                "Install Hyphae",
                "Install the Hyphae memory server.",
                &["install", "hyphae"],
                RepairTier::Primary,
            )],
            suppressed: false,
        });
    }

    let configured = crate::commands::codex_notify::codex_notify_configured();
    let detail = crate::commands::codex_notify::codex_notify_detail(configured);
    Some(HealthCheck {
        name: "codex notify adapter".to_string(),
        passed: configured,
        message: detail,
        repair_actions: if configured {
            Vec::new()
        } else {
            vec![crate::commands::codex_notify::codex_notify_repair_action()]
        },
        suppressed: false,
    })
}

pub(super) fn check_hyphae_db() -> HealthCheck {
    // Skip the DB check when hyphae itself is not installed — the tool check already
    // surfaces the missing binary, and a "database not found" message would be misleading.
    let hyphae_installed = tool_registry::find("hyphae")
        .is_some_and(|spec| matches!(tool_registry::probe(spec), ToolProbe::Installed(_)));
    if !hyphae_installed {
        return HealthCheck {
            name: "hyphae database".to_string(),
            passed: true,
            message: "Hyphae not installed (database check skipped)".to_string(),
            repair_actions: Vec::new(),
            suppressed: false,
        };
    }

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
            suppressed: false,
        };
    };

    // New canonical path under the shared basidiocarp root.
    let new_path = crate::ecosystem::paths::hyphae_db_path(&data_dir);
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
            suppressed: false,
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
            suppressed: false,
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
            // For hyphae, check both the new canonical path and the legacy path.
            // During migration, the DB may exist in either location.
            let status = if *name == "hyphae" {
                let new_path = root.join(name).join(db_file);
                let legacy_path = data_dir.join("hyphae").join("hyphae.db");
                if new_path.exists() || legacy_path.exists() {
                    "✓"
                } else {
                    "—"
                }
            } else if root.join(name).join(db_file).exists() {
                "✓"
            } else {
                "—"
            };
            format!("{name} {status}")
        })
        .collect();

    let message = format!("{}/  {}", root.display(), parts.join("  "));

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
        suppressed: false,
    }
}

pub(super) fn check_hyphae_db_at_path(db_path: &Path) -> HealthCheck {
    if db_path.exists() {
        HealthCheck {
            name: "hyphae database".to_string(),
            passed: true,
            message: "Database initialized".to_string(),
            repair_actions: Vec::new(),
            suppressed: false,
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
            suppressed: false,
        }
    }
}

/// Check that Canopy's `SQLite` database is running in WAL mode.
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
            suppressed: false,
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
            suppressed: false,
        };
    }

    let output = crate::commands::install::release::run_command_with_timeout(
        std::process::Command::new("sqlite3")
            .arg(&db_path)
            .arg("PRAGMA journal_mode;"),
        Duration::from_secs(3),
    );
    match output {
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
                    suppressed: false,
                }
            } else {
                HealthCheck {
                    name: "canopy WAL mode".to_string(),
                    passed: false,
                    message: format!("journal_mode is '{mode}', expected 'wal' — restart Canopy to apply"),
                    repair_actions: Vec::new(),
                    suppressed: false,
                }
            }
        }
        Ok(_) => HealthCheck {
            name: "canopy WAL mode".to_string(),
            passed: true,
            message: "sqlite3 query failed (database may be locked); WAL mode set at Canopy startup (advisory)".to_string(),
            repair_actions: Vec::new(),
            suppressed: false,
        },
        Err(_) => HealthCheck {
            name: "canopy WAL mode".to_string(),
            passed: true,
            message: "sqlite3 not available; WAL mode set at Canopy startup (advisory)".to_string(),
            repair_actions: Vec::new(),
            suppressed: false,
        },
    }
}

pub(super) fn check_rhizome_compiled_env() -> HealthCheck {
    // Resolve via tool registry (spore-based, PATH-independent) so this check
    // works when rhizome is installed at ~/.local/bin but not on PATH.
    let Some(rhizome_bin) =
        tool_registry::find("rhizome").and_then(tool_registry::resolve_binary_path)
    else {
        return HealthCheck {
            name: "rhizome compiled environment".to_string(),
            passed: true,
            message: "Rhizome not installed (skipped)".to_string(),
            repair_actions: Vec::new(),
            suppressed: false,
        };
    };

    let rhizome_available = run_command_with_timeout(
        Command::new(&rhizome_bin).arg("--version"),
        Duration::from_secs(5),
    )
    .is_ok_and(|o| o.status.success());

    if !rhizome_available {
        return HealthCheck {
            name: "rhizome compiled environment".to_string(),
            passed: true,
            message: "Rhizome not installed (skipped)".to_string(),
            repair_actions: Vec::new(),
            suppressed: false,
        };
    }

    // Check if a compiled-env memoir exists in hyphae
    let Some(hyphae_bin) =
        tool_registry::find("hyphae").and_then(tool_registry::resolve_binary_path)
    else {
        return HealthCheck {
            name: "rhizome compiled environment".to_string(),
            passed: true,
            message: "Hyphae not installed (skipped)".to_string(),
            repair_actions: Vec::new(),
            suppressed: false,
        };
    };

    let artifact_exists = run_command_with_timeout(
        Command::new(&hyphae_bin).args(["memoir", "show", "--name", "compiled-env:*"]),
        Duration::from_secs(5),
    )
    .is_ok_and(|o| o.status.success());

    if artifact_exists {
        HealthCheck {
            name: "rhizome compiled environment".to_string(),
            passed: true,
            message: "Compiled environment artifact exists in Hyphae".to_string(),
            repair_actions: Vec::new(),
            suppressed: false,
        }
    } else {
        HealthCheck {
            name: "rhizome compiled environment".to_string(),
            passed: true,
            message: "No compiled environment artifact; run 'rhizome compile-env' to generate one (optional)".to_string(),
            repair_actions: Vec::new(),
            suppressed: false,
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
            suppressed: false,
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
            suppressed: false,
        }
    } else {
        HealthCheck {
            name: "capability registry".to_string(),
            passed: true,
            message: "Capability registry present and current".to_string(),
            repair_actions: Vec::new(),
            suppressed: false,
        }
    }
}

/// Check whether a hook command's leading binary token is actually runnable.
///
/// For absolute paths, verifies the file exists and the execute bit is set.
/// For bare names, checks that `which` can locate the binary on PATH and
/// warns when it cannot (hooks launched from GUI apps often have a restricted
/// PATH that omits `~/.local/bin`).
///
/// Returns `None` when the command string is empty or unparseable.
fn check_hook_runtime_path(cmd: &str) -> Option<(bool, String)> {
    let binary_token = cmd.split_whitespace().next()?;

    if binary_token.starts_with('/') {
        // Absolute path: check existence and execute permission.
        let path = Path::new(binary_token);
        if !path.exists() {
            return Some((false, format!("hook binary not found: {binary_token}")));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(path).ok()?.permissions().mode();
            if mode & 0o111 == 0 {
                return Some((
                    false,
                    format!("hook binary not executable (mode {mode:o}): {binary_token}"),
                ));
            }
        }

        Some((true, format!("hook binary found: {binary_token}")))
    } else {
        // Bare name: check via tool_registry first (spore-based, PATH-independent),
        // then fall back to PATH. A bare-name hook passes here but will fail when
        // Claude Code fires it from a GUI launcher without ~/.local/bin on PATH.
        // Flag it as a concern even when the binary is findable.
        let resolved = tool_registry::find(binary_token)
            .and_then(tool_registry::resolve_binary_path)
            .or_else(|| which::which(binary_token).ok());
        match resolved {
            Some(_) => Some((
                false,
                format!(
                    "{binary_token} is registered as a bare name — re-run \
                     'stipe host setup claude-code' to upgrade to an absolute path"
                ),
            )),
            None => Some((
                false,
                format!(
                    "{binary_token} not found — hooks will not fire; \
                     install {binary_token} and run 'stipe host setup claude-code'"
                ),
            )),
        }
    }
}

/// Audit all registered cortina/annulus hook commands across all settings paths
/// and report any whose leading binary is absent or not executable.
pub(super) fn check_hook_command_runnability() -> Option<HealthCheck> {
    let mut failures: Vec<String> = Vec::new();
    let mut total = 0usize;

    for settings_path in host_policy::claude_hook_settings_paths() {
        let Ok(entries) = claude_hooks::hook_entries_at_path(&settings_path) else {
            continue;
        };

        for entry in entries {
            // Only audit stipe-owned commands (cortina/annulus).
            if !entry.command.contains("cortina")
                && !entry.command.contains("annulus")
                && !entry.command.contains("adapter claude-code")
            {
                continue;
            }

            total += 1;
            if let Some((ok, msg)) = check_hook_runtime_path(&entry.command) {
                if !ok {
                    failures.push(format!(
                        "  {} ({}): {msg}",
                        entry.event,
                        settings_path.display()
                    ));
                }
            }
        }
    }

    if total == 0 {
        return None;
    }

    Some(HealthCheck {
        name: "hook command runnability".to_string(),
        passed: failures.is_empty(),
        message: if failures.is_empty() {
            "All hook commands point to runnable binaries.".to_string()
        } else {
            format!(
                "{} hook command(s) reference missing or non-executable binaries:\n{}",
                failures.len(),
                failures.join("\n")
            )
        },
        repair_actions: if failures.is_empty() {
            Vec::new()
        } else {
            vec![RepairAction::stipe(
                "reinstall-hooks",
                "Reinstall hooks with correct binary paths",
                "Re-run host setup to write absolute binary paths into hook commands.",
                &["host", "setup", "claude-code"],
                RepairTier::Primary,
            )]
        },
        suppressed: false,
    })
}

// ---------------------------------------------------------------------------
// stipe.toml sync check
// ---------------------------------------------------------------------------

/// Check whether `stipe.toml` and the installed `.claude/settings.json` are in
/// sync. Returns `None` when no `stipe.toml` exists (nothing to check).
pub(super) fn check_stipe_toml_sync() -> Option<HealthCheck> {
    let warning = crate::commands::sync::check_sync_state()?;
    Some(HealthCheck {
        name: "stipe.toml sync".to_string(),
        passed: false,
        message: warning,
        repair_actions: vec![RepairAction::stipe(
            "sync-toml",
            "Sync stipe.toml",
            "Apply stipe.toml to .claude/settings.json.",
            &["sync"],
            RepairTier::Primary,
        )],
        suppressed: false,
    })
}

/// Warn when `stipe.toml` contains keys that are not part of its schema. Serde
/// drops such keys silently on `stipe sync`, so a typo like `[permisions]` or
/// `network.denyed_domains` discards the user's intent without any error. This
/// is a warning, not a hard failure — `stipe sync` still applies the file.
///
/// Returns `None` when no `stipe.toml` exists or every key is recognized.
pub(super) fn check_stipe_toml_unknown_keys() -> Option<HealthCheck> {
    let unknown = crate::commands::sync::check_unknown_keys()?;
    if unknown.is_empty() {
        return None;
    }

    Some(HealthCheck {
        name: "stipe.toml keys".to_string(),
        passed: false,
        message: format!(
            "stipe.toml has {} unrecognized key(s) silently ignored by `stipe sync` (likely typos): {}",
            unknown.len(),
            unknown.join(", ")
        ),
        repair_actions: vec![RepairAction::stipe(
            "fix-stipe-toml-keys",
            "Fix unrecognized stipe.toml keys",
            "Correct or remove the unrecognized keys in stipe.toml; run `stipe sync --scaffold` to see the documented schema.",
            &["sync", "--scaffold"],
            RepairTier::Primary,
        )],
        suppressed: false,
    })
}

/// Check that sensitive local config files (if they exist) are covered by .gitignore.
///
/// Returns `None` when:
/// - `project_root` is unavailable
/// - no sensitive files are found on disk
/// - all found files are safely ignored
/// - git is unavailable or not a repo (exit code not 0 or 1)
///
/// Returns `Some` with `passed=false` when at least one sensitive file exists but is NOT ignored.
pub(super) fn check_local_config_gitignore() -> Option<HealthCheck> {
    let root = host_policy::project_root()?;

    // Candidate sensitive local config filenames
    let candidates = [".claude/settings.local.json", ".mcp.json", ".env", ".envrc"];

    // Collect existing files
    let mut existing = Vec::new();
    for name in &candidates {
        let path = root.join(name);
        if path.exists() {
            existing.push(name.to_string());
        }
    }

    // If no candidates exist, nothing to check
    if existing.is_empty() {
        return None;
    }

    // Check which ones are ignored by git
    let mut unsafe_files = Vec::new();

    for file_path in existing {
        let status = Command::new("git")
            .args(["check-ignore", "-q", &file_path])
            .current_dir(&root)
            .status();

        // `git check-ignore -q` exits 1 when the path is NOT ignored (unsafe). Exit 0
        // means ignored (safe); 128 means not a repo / fatal git error; a spawn Err
        // means git is not on PATH. Treat ONLY exit code 1 as unsafe (INV2) — a blanket
        // "non-zero ⇒ unsafe" rule would misclassify the 128 not-a-repo case. Every
        // other outcome skips THIS file without aborting, so unsafe files already
        // collected from earlier candidates are preserved (no evidence-discarding early
        // return). If git is absent or this is not a repo, every candidate skips and
        // `unsafe_files` stays empty, so the function returns None below (INV1).
        if let Ok(exit_status) = status {
            if exit_status.code() == Some(1) {
                unsafe_files.push(file_path);
            }
        }
    }

    // If all files are safe, return None (no issue)
    if unsafe_files.is_empty() {
        return None;
    }

    // At least one file is unsafe
    let n = unsafe_files.len();
    let joined = unsafe_files.join(", ");
    Some(HealthCheck {
        name: "config gitignore".to_string(),
        passed: false,
        message: format!(
            "{} local config file(s) not covered by .gitignore: {}",
            n, joined
        ),
        repair_actions: vec![],
        suppressed: false,
    })
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
        let (is_behind, pinned, msg_override) = check_version_drift("hyphae", "0.11.0");
        assert!(
            is_behind,
            "older version should be reported as behind the pin"
        );
        assert_eq!(
            pinned.as_deref(),
            Some("0.19.0"),
            "latest version should be returned"
        );
        assert!(
            msg_override.is_none(),
            "should not have message override for behind"
        );
    }

    #[test]
    fn check_version_drift_accepts_current_binary() {
        let (is_behind, pinned, msg_override) = check_version_drift("hyphae", "0.19.0");
        assert!(
            !is_behind,
            "matching version should not be reported as behind"
        );
        assert_eq!(pinned.as_deref(), Some("0.19.0"));
        assert!(
            msg_override.is_none(),
            "should not have message override for matching"
        );
    }

    #[test]
    fn check_version_drift_unknown_tool_never_reports_behind() {
        let (is_behind, pinned, msg_override) = check_version_drift("not-a-real-tool", "9.9.9");
        assert!(
            !is_behind,
            "unknown tool should never be reported as behind"
        );
        assert!(
            pinned.is_none(),
            "unknown tool should return no pinned version"
        );
        assert!(
            msg_override.is_none(),
            "unknown tool should not have override"
        );
    }

    #[test]
    fn check_version_drift_covers_hymenium_and_canopy() {
        // Verify the two tools most relevant to dogfood freshness are in the pin table.
        let (_, hymenium_pin, _) = check_version_drift("hymenium", "0.0.0");
        let (_, canopy_pin, _) = check_version_drift("canopy", "0.0.0");
        assert!(hymenium_pin.is_some(), "hymenium must have a version pin");
        assert!(canopy_pin.is_some(), "canopy must have a version pin");
    }

    #[test]
    fn check_version_drift_detects_newer_binary() {
        let (is_behind, pinned, msg_override) = check_version_drift("hyphae", "0.20.0");
        assert!(!is_behind, "newer version should not be reported as behind");
        assert_eq!(pinned.as_deref(), Some("0.19.0"));
        assert!(
            msg_override.is_some(),
            "newer version should have override message"
        );
        assert!(
            msg_override
                .as_deref()
                .is_some_and(|msg| msg.contains("ahead of latest"))
        );
    }

    // -- local config gitignore check ----------------------------------------

    #[test]
    fn check_local_config_gitignore_safe_file_returns_none() {
        let test_suffix = format!("local-config-safe-{}", std::process::id());
        let temp_root = std::env::temp_dir().join(format!("stipe-gitignore-test-{}", test_suffix));
        let _ = fs::remove_dir_all(&temp_root);
        fs::create_dir_all(&temp_root).unwrap();

        // Initialize a git repo and add a .gitignore entry
        let init = Command::new("git")
            .args(["init"])
            .current_dir(&temp_root)
            .output()
            .expect("git init should spawn");
        assert!(
            init.status.success(),
            "git init failed; test would otherwise pass vacuously: {}",
            String::from_utf8_lossy(&init.stderr)
        );

        fs::write(temp_root.join(".gitignore"), ".env\n").unwrap();
        fs::write(temp_root.join(".env"), "SECRET=value\n").unwrap();

        // Test the check within the overridden project root
        let result = crate::commands::host_policy::with_project_root_override(
            temp_root.clone(),
            check_local_config_gitignore,
        );

        assert!(
            result.is_none(),
            "check should return None when .env is safely ignored"
        );

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn check_local_config_gitignore_unsafe_file_returns_some() {
        let test_suffix = format!("local-config-unsafe-{}", std::process::id());
        let temp_root = std::env::temp_dir().join(format!("stipe-gitignore-test-{}", test_suffix));
        let _ = fs::remove_dir_all(&temp_root);
        fs::create_dir_all(&temp_root).unwrap();

        // Initialize a git repo but do NOT add .env to .gitignore
        let init = Command::new("git")
            .args(["init"])
            .current_dir(&temp_root)
            .output()
            .expect("git init should spawn");
        assert!(
            init.status.success(),
            "git init failed; test would otherwise pass vacuously: {}",
            String::from_utf8_lossy(&init.stderr)
        );

        fs::write(temp_root.join(".gitignore"), "# empty\n").unwrap();
        fs::write(temp_root.join(".env"), "SECRET=value\n").unwrap();

        let result = crate::commands::host_policy::with_project_root_override(
            temp_root.clone(),
            check_local_config_gitignore,
        );

        assert!(
            result.is_some(),
            "check should return Some when .env is not ignored"
        );
        let check = result.unwrap();
        assert!(!check.passed, "check should fail when unsafe files exist");
        assert!(
            check.message.contains(".env"),
            "message should name the unsafe file"
        );

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn check_local_config_gitignore_non_git_dir_returns_none() {
        let test_suffix = format!("local-config-nongit-{}", std::process::id());
        let temp_root = std::env::temp_dir().join(format!("stipe-gitignore-test-{}", test_suffix));
        let _ = fs::remove_dir_all(&temp_root);
        fs::create_dir_all(&temp_root).unwrap();

        // Do NOT initialize git; just create a .env file
        fs::write(temp_root.join(".env"), "SECRET=value\n").unwrap();

        let result = crate::commands::host_policy::with_project_root_override(
            temp_root.clone(),
            check_local_config_gitignore,
        );

        assert!(
            result.is_none(),
            "check should return None when git is not available or not a repo (INV1)"
        );

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn check_local_config_gitignore_no_candidates_returns_none() {
        let test_suffix = format!("local-config-nocandidates-{}", std::process::id());
        let temp_root = std::env::temp_dir().join(format!("stipe-gitignore-test-{}", test_suffix));
        let _ = fs::remove_dir_all(&temp_root);
        fs::create_dir_all(&temp_root).unwrap();

        // Initialize a git repo but create no sensitive files
        let init = Command::new("git")
            .args(["init"])
            .current_dir(&temp_root)
            .output()
            .expect("git init should spawn");
        assert!(
            init.status.success(),
            "git init failed; test would otherwise pass vacuously: {}",
            String::from_utf8_lossy(&init.stderr)
        );

        let result = crate::commands::host_policy::with_project_root_override(
            temp_root.clone(),
            check_local_config_gitignore,
        );

        assert!(
            result.is_none(),
            "check should return None when no candidate files exist"
        );

        let _ = fs::remove_dir_all(&temp_root);
    }
}
