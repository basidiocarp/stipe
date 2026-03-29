use colored::Colorize;
use std::process::Command;

use crate::commands::tool_registry::{self, ToolProbe};

use super::clients::ServerConfig;

pub(super) fn tool_probe(tool_name: &str) -> ToolProbe {
    tool_registry::find(tool_name).map_or(ToolProbe::Missing, tool_registry::probe)
}

pub(super) fn discover_codex_version() -> Option<String> {
    let output = Command::new("codex").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().last())
        .filter(|v| v.contains('.'))
        .unwrap_or("unknown")
        .to_string();
    Some(version)
}

pub(super) fn claude_is_available() -> bool {
    Command::new("claude")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

pub(super) fn print_tool_status(name: &str, version: Option<&str>, probe: &ToolProbe) {
    if let Some(v) = version {
        println!(
            "  {:<10}v{:<8}{}",
            name.bold(),
            v,
            "\u{2713} installed".green()
        );
    } else if matches!(probe, ToolProbe::Broken) {
        println!(
            "  {:<10}{:<8} {}",
            name.bold(),
            "!",
            "\u{2717} installed but broken".red()
        );
    } else {
        let hint = match name {
            "canopy" => " (optional outside the coordination runtime path: stipe install canopy)",
            "cap" => {
                " (optional: git clone https://github.com/basidiocarp/cap && cd cap && npm i && npm run dev:all)"
            }
            _ => "",
        };
        println!(
            "  {:<10}{:<8} {}{}",
            name.bold(),
            "\u{2014}",
            "\u{2717} not installed".red(),
            hint.dimmed()
        );
    }
}

pub(super) fn build_server_configs() -> Vec<ServerConfig> {
    build_ecosystem_servers(
        tool_probe("hyphae").is_installed(),
        tool_probe("rhizome").is_installed(),
    )
}

pub(super) fn build_ecosystem_servers(
    hyphae_installed: bool,
    rhizome_installed: bool,
) -> Vec<ServerConfig> {
    let mut servers = Vec::new();
    if hyphae_installed {
        servers.push(ServerConfig {
            name: "hyphae".to_string(),
            command: "hyphae".to_string(),
            args: vec!["serve".to_string()],
        });
    }
    if rhizome_installed {
        servers.push(ServerConfig {
            name: "rhizome".to_string(),
            command: "rhizome".to_string(),
            args: vec!["serve".to_string(), "--expanded".to_string()],
        });
    }
    servers
}
