use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::model::HealthCheck;
use super::tool_registry::{self, DoctorCoverage, ToolProbe, ToolSpec};
use crate::commands::host_policy;
use crate::commands::repair::{RepairAction, RepairTier, cargo_install_action};
use crate::ecosystem::clients::{self, McpClient};

const MCP_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const MCP_INITIALIZE_REQUEST: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"stipe-doctor","version":"1.0"}}}"#;

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

fn parse_initialize_response(line: &str, expected_server: &'static str) -> Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_str(line).map_err(|err| format!("invalid JSON-RPC response: {err}"))?;

    if let Some(error) = value.get("error") {
        return Err(format!("initialize returned error: {error}"));
    }

    let result = value
        .get("result")
        .ok_or_else(|| "initialize returned no result".to_string())?;
    let server_name = result
        .get("serverInfo")
        .and_then(|server| server.get("name"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "initialize response missing serverInfo.name".to_string())?;

    if server_name != expected_server {
        return Err(format!(
            "initialize returned server `{server_name}` instead of `{expected_server}`"
        ));
    }

    Ok(())
}

fn probe_mcp_server(
    command: &str,
    args: &[&str],
    expected_server: &'static str,
    timeout: Duration,
) -> Result<(), String> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to spawn `{command}`: {err}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(MCP_INITIALIZE_REQUEST.as_bytes())
            .map_err(|err| format!("failed to write initialize request: {err}"))?;
        stdin
            .write_all(b"\n")
            .map_err(|err| format!("failed to terminate initialize request: {err}"))?;
        stdin
            .flush()
            .map_err(|err| format!("failed to flush initialize request: {err}"))?;
    } else {
        return Err("child stdin unavailable".to_string());
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "child stdout unavailable".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "child stderr unavailable".to_string())?;

    let (stdout_tx, stdout_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let result = match reader.read_line(&mut line) {
            Ok(0) => Err("connection closed before initialize response".to_string()),
            Ok(_) => Ok(line),
            Err(err) => Err(format!("failed reading initialize response: {err}")),
        };
        let _ = stdout_tx.send(result);
    });

    let (stderr_tx, stderr_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut output = String::new();
        let _ = std::io::Read::read_to_string(&mut reader, &mut output);
        let _ = stderr_tx.send(output);
    });

    let response = match stdout_rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("initialize timed out after {}s", timeout.as_secs()));
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("initialize response channel disconnected".to_string());
        }
    };

    let _ = child.kill();
    let _ = child.wait();

    response.and_then(|line| {
        parse_initialize_response(&line, expected_server).map_err(|err| {
            let stderr_output = stderr_rx
                .recv_timeout(Duration::from_millis(200))
                .unwrap_or_default();
            if stderr_output.trim().is_empty() {
                err
            } else {
                format!("{err}; stderr: {}", stderr_output.trim())
            }
        })
    })
}

fn check_mcp_startup(tool_name: &'static str, command: &str, args: &[&str]) -> Option<HealthCheck> {
    let spec = tool_registry::find(tool_name)?;
    let ToolProbe::Installed(_) = tool_registry::probe(spec) else {
        return None;
    };

    Some(
        match probe_mcp_server(command, args, tool_name, MCP_STARTUP_TIMEOUT) {
            Ok(()) => HealthCheck {
                name: format!("{tool_name} MCP startup"),
                passed: true,
                message: format!(
                    "responds to initialize within {}s",
                    MCP_STARTUP_TIMEOUT.as_secs()
                ),
                repair_actions: Vec::new(),
            },
            Err(message) => HealthCheck {
                name: format!("{tool_name} MCP startup"),
                passed: false,
                message,
                repair_actions: mcp_startup_actions(tool_name),
            },
        },
    )
}

pub(super) fn check_tool(spec: &ToolSpec) -> HealthCheck {
    match (spec.doctor_coverage, tool_registry::probe(spec)) {
        (_, ToolProbe::Installed(version)) => HealthCheck {
            name: spec.name.to_string(),
            passed: true,
            message: format!("v{version} installed and working"),
            repair_actions: Vec::new(),
        },
        (DoctorCoverage::Optional, ToolProbe::Missing) => HealthCheck {
            name: spec.name.to_string(),
            passed: true,
            message: "Optional coordination runtime not installed".to_string(),
            repair_actions: Vec::new(),
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
    let mut checks = Vec::new();

    if let Some(check) = check_mcp_startup("hyphae", "hyphae", &["serve"]) {
        checks.push(check);
    }
    if let Some(check) = check_mcp_startup("rhizome", "rhizome", &["serve", "--expanded"]) {
        checks.push(check);
    }

    checks
}

pub(super) fn installed_mcp_servers() -> Vec<&'static str> {
    let mut servers = Vec::new();

    if tool_registry::find("hyphae")
        .is_some_and(|spec| matches!(tool_registry::probe(spec), ToolProbe::Installed(_)))
    {
        servers.push("hyphae");
    }
    if tool_registry::find("rhizome")
        .is_some_and(|spec| matches!(tool_registry::probe(spec), ToolProbe::Installed(_)))
    {
        servers.push("rhizome");
    }

    servers
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_initialize_response_accepts_expected_server() {
        let line = r#"{"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"hyphae"}}}"#;
        assert!(parse_initialize_response(line, "hyphae").is_ok());
    }

    #[test]
    fn parse_initialize_response_rejects_wrong_server() {
        let line = r#"{"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"rhizome"}}}"#;
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
            "#!/bin/sh\nread line\nprintf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"serverInfo\":{\"name\":\"hyphae\"}}}'\n",
        )
        .unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();

        assert!(
            probe_mcp_server(
                script.to_str().unwrap(),
                &[],
                "hyphae",
                Duration::from_secs(1)
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
            script.to_str().unwrap(),
            &[],
            "hyphae",
            Duration::from_millis(100),
        )
        .unwrap_err();
        assert!(error.contains("timed out"));

        let _ = fs::remove_dir_all(&dir);
    }
}
