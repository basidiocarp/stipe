use colored::Colorize;
use std::process::Command;

use crate::commands::codex_notify;
use crate::commands::host_policy::{self, HostConfigScope};

use super::clients::{self, McpClient};
use super::status::build_ecosystem_servers;

fn configure_mcp_client(
    client: McpClient,
    label: &str,
    success_label: &str,
    hyphae_installed: bool,
    rhizome_installed: bool,
    scope: HostConfigScope,
    verbose: u8,
) -> bool {
    let servers = build_ecosystem_servers(hyphae_installed, rhizome_installed);

    if servers.is_empty() {
        return false;
    }

    println!();
    println!("{}", format!("Configuring {label}...").bold());
    println!();

    match clients::register_servers(client, &servers, scope, verbose) {
        Ok(true) => {
            println!();
            println!(
                "  {} MCP servers registered for {success_label}:",
                "\u{2713}".green()
            );
            for server in &servers {
                println!("    - {}", server.name);
            }
            true
        }
        Ok(false) => {
            eprintln!(
                "  {} {success_label} registration returned false",
                "!".yellow()
            );
            false
        }
        Err(e) => {
            eprintln!(
                "  {} {success_label} registration failed: {}",
                "!".yellow(),
                e
            );
            false
        }
    }
}

pub(super) fn configure_codex_cli(
    hyphae_installed: bool,
    rhizome_installed: bool,
    scope: HostConfigScope,
    verbose: u8,
) {
    if !host_policy::host_scope_supported(host_policy::HostMode::Codex, scope) {
        eprintln!(
            "  {} Codex host mode does not support the '{}' scope — skipping Codex configuration.",
            "!".yellow(),
            match scope {
                HostConfigScope::User => "user",
                HostConfigScope::Project => "project",
                HostConfigScope::Local => "local",
            }
        );
        return;
    }

    if !configure_mcp_client(
        McpClient::CodexCli,
        "Codex host mode",
        "Codex host mode",
        hyphae_installed,
        rhizome_installed,
        scope,
        verbose,
    ) {
        return;
    }

    if hyphae_installed {
        match codex_notify::install_codex_notify(scope, verbose) {
            Ok(true) => println!("    - Hyphae Codex notify adapter"),
            Ok(false) => eprintln!("  {} Codex notify installation skipped", "!".yellow()),
            Err(e) => eprintln!("  {} Codex notify installation failed: {}", "!".yellow(), e),
        }
    }
}

pub(super) fn configure_cursor_host(hyphae_installed: bool, rhizome_installed: bool, verbose: u8) {
    let _ = configure_mcp_client(
        McpClient::Cursor,
        "Cursor mode",
        "Cursor mode",
        hyphae_installed,
        rhizome_installed,
        HostConfigScope::User,
        verbose,
    );
}

pub(super) fn initialize_hyphae_db_if_needed(hyphae_installed: bool) {
    if let Some(data_dir) = hyphae_installed
        .then(dirs::data_dir)
        .flatten()
        .map(|d| d.join("hyphae"))
        .filter(|d| !d.join("hyphae.db").exists())
    {
        let _ = std::fs::create_dir_all(&data_dir);
        let _ = Command::new("hyphae").arg("stats").output();
        println!("  {} Hyphae database initialized", "\u{2713}".green());
    }
}

#[allow(clippy::ref_option)]
pub(super) fn configure_detected_clients(
    client_filter: Option<&str>,
    hyphae_installed: bool,
    rhizome_installed: bool,
    verbose: u8,
) {
    let servers = build_ecosystem_servers(hyphae_installed, rhizome_installed);

    if servers.is_empty() {
        return;
    }

    let targets: Vec<McpClient> = if let Some(name) = client_filter {
        if let Some(c) = McpClient::from_flag(name) {
            vec![c]
        } else {
            eprintln!(
                "  {} Unknown client '{name}'. Known: claude-code, cursor, windsurf, cline, continue, claude-desktop, codex, gemini, copilot",
                "!".yellow(),
            );
            return;
        }
    } else {
        clients::detect_clients()
            .into_iter()
            .filter(|client| !client.handled_separately_in_ecosystem())
            .collect()
    };

    if targets.is_empty() {
        return;
    }

    println!();
    println!("{}", "Configuring additional MCP clients...".bold());

    let mut client_configured = Vec::new();

    for target in &targets {
        if client_filter.is_none() && target.handled_separately_in_ecosystem() {
            continue;
        }

        match clients::register_servers(*target, &servers, HostConfigScope::User, verbose) {
            Ok(true) => {
                client_configured.push(target.name());
            }
            Ok(false) => {
                eprintln!(
                    "  {} {} registration returned false",
                    "!".yellow(),
                    target.name()
                );
            }
            Err(e) => {
                eprintln!(
                    "  {} {} registration failed: {}",
                    "!".yellow(),
                    target.name(),
                    e
                );
            }
        }
    }

    if !client_configured.is_empty() {
        println!();
        println!("  {} MCP servers registered for:", "\u{2713}".green());
        for name in &client_configured {
            println!("    - {name}");
        }
    }
}
