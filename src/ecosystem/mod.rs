//! Ecosystem management: tool detection, MCP registration, database initialization.

pub mod clients;

use anyhow::Result;
use colored::Colorize;
use spore::{Tool, discover};
use std::process::Command;

use crate::commands::host_policy;
use clients::{McpClient, ServerConfig};

/// Cap is not in the spore `Tool` enum — detect it separately.
fn discover_cap() -> Option<String> {
    let output = Command::new("cap").arg("--version").output().ok()?;
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

/// Detect the Codex CLI version, if available.
pub fn discover_codex_version() -> Option<String> {
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

/// Check if `claude` binary is in PATH.
fn claude_is_available() -> bool {
    Command::new("claude")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Check if an MCP server is already registered with Claude Code.
fn mcp_exists(name: &str) -> bool {
    Command::new("claude")
        .args(["mcp", "get", name])
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Register an MCP server with Claude Code. Returns:
/// - `Ok(Some("registered"))` if newly registered
/// - `Ok(Some("already registered"))` if already present
/// - `Ok(None)` if registration failed
fn register_mcp(name: &str, args: &[&str], verbose: u8) -> Result<Option<&'static str>> {
    if mcp_exists(name) {
        if verbose > 0 {
            eprintln!("  {name} MCP already registered");
        }
        return Ok(Some("already registered"));
    }

    let mut cmd = Command::new("claude");
    cmd.arg("mcp")
        .arg("add")
        .arg("--scope")
        .arg("user")
        .arg(name);
    cmd.arg("--");
    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!(
            "  Running: claude mcp add --scope user {} -- {}",
            name,
            args.join(" ")
        );
    }

    let output = cmd.output()?;
    if output.status.success() {
        Ok(Some("registered"))
    } else {
        Ok(None)
    }
}

/// Main entry point for ecosystem setup.
#[allow(clippy::too_many_lines, clippy::unnecessary_wraps)]
pub fn run_ecosystem(client: Option<&str>, verbose: u8) -> Result<()> {
    // Handle --client generic: just print JSON snippet and exit
    if client
        .as_ref()
        .is_some_and(|c| c.eq_ignore_ascii_case("generic"))
    {
        let servers = build_server_configs();
        clients::print_generic_config(&servers);
        return Ok(());
    }

    // ─────────────────────────────────────────────────────────────────────
    // 1. Discover tools
    // ─────────────────────────────────────────────────────────────────────
    let cap_version = discover_cap();
    let codex_version = discover_codex_version();

    // ─────────────────────────────────────────────────────────────────────
    // 2. Print ecosystem status
    // ─────────────────────────────────────────────────────────────────────
    println!();
    println!("{}", "Basidiocarp Ecosystem Status".bold());
    println!("{}", "\u{2500}".repeat(75));
    println!();

    // Always show mycelium first (we know it's installed — we're running via stipe)
    let mycelium_info = discover(Tool::Mycelium);
    print_tool_status(
        "mycelium",
        mycelium_info.as_ref().map(|i| i.version.as_str()),
    );

    let hyphae_info = discover(Tool::Hyphae);
    print_tool_status("hyphae", hyphae_info.as_ref().map(|i| i.version.as_str()));

    let rhizome_info = discover(Tool::Rhizome);
    print_tool_status("rhizome", rhizome_info.as_ref().map(|i| i.version.as_str()));

    print_tool_status("codex", codex_version.as_deref());
    print_tool_status("cap", cap_version.as_deref());

    println!();

    // ─────────────────────────────────────────────────────────────────────
    // 3. Configure Claude Code (if available)
    // ─────────────────────────────────────────────────────────────────────
    if claude_is_available() {
        println!("{}", "Configuring Claude Code...".bold());
        println!();

        let mut configured = Vec::new();

        // Register hyphae MCP if installed
        if hyphae_info.is_some() {
            match register_mcp("hyphae", &["hyphae", "serve"], verbose) {
                Ok(Some(status)) => configured.push(if status == "already registered" {
                    "hyphae MCP (already registered)"
                } else {
                    "hyphae MCP"
                }),
                Ok(None) => eprintln!("  {} Failed to register hyphae MCP", "!".yellow()),
                Err(e) => eprintln!("  {} hyphae MCP registration error: {}", "!".yellow(), e),
            }
        }

        // Register rhizome MCP if installed
        if rhizome_info.is_some() {
            match register_mcp("rhizome", &["rhizome", "serve", "--expanded"], verbose) {
                Ok(Some(status)) => configured.push(if status == "already registered" {
                    "rhizome MCP (already registered)"
                } else {
                    "rhizome MCP"
                }),
                Ok(None) => eprintln!("  {} Failed to register rhizome MCP", "!".yellow()),
                Err(e) => eprintln!("  {} rhizome MCP registration error: {}", "!".yellow(), e),
            }
        }

        if !configured.is_empty() {
            println!();
            println!("  {} Configured:", "\u{2713}".green());
            for item in &configured {
                println!("    - {item}");
            }
        }
    } else {
        println!(
            "  {} {} not found in PATH — skipping Claude Code configuration.",
            "!".yellow(),
            "claude".bold()
        );
        println!("    Install Claude Code first, then re-run: stipe init");
    }

    // ─────────────────────────────────────────────────────────────────────
    // 3b. Configure Codex host mode (if available)
    // ─────────────────────────────────────────────────────────────────────
    if host_policy::codex_target_requested(client) {
        if codex_version.is_some() {
            configure_codex_cli(&hyphae_info, &rhizome_info, verbose);
        } else {
            println!(
                "  {} {} not found in PATH — skipping Codex host mode configuration.",
                "!".yellow(),
                "codex".bold()
            );
            println!("    Install Codex first, then re-run: stipe init");
        }
    } else {
        if client.is_none() && codex_version.is_some() {
            configure_codex_cli(&hyphae_info, &rhizome_info, verbose);
        }

        // ─────────────────────────────────────────────────────────────────────
        // 3c. Initialize the Hyphae database if needed
        // ─────────────────────────────────────────────────────────────────────
        if let Some(data_dir) = hyphae_info
            .as_ref()
            .and(dirs::data_dir())
            .map(|d| d.join("hyphae"))
            .filter(|d| !d.join("hyphae.db").exists())
        {
            let _ = std::fs::create_dir_all(&data_dir);
            let _ = Command::new("hyphae").arg("stats").output();
            println!("  {} Hyphae database initialized", "\u{2713}".green());
        }

        // ─────────────────────────────────────────────────────────────────────
        // 3d. Configure additional MCP clients
        // ─────────────────────────────────────────────────────────────────────
        configure_detected_clients(client, &hyphae_info, &rhizome_info, verbose);
    }

    // ─────────────────────────────────────────────────────────────────────
    // 4. Print missing tool instructions
    // ─────────────────────────────────────────────────────────────────────
    let mut missing: Vec<(&str, &str)> = Vec::new();

    if hyphae_info.is_none() {
        missing.push((
            "hyphae",
            "cargo install --git https://github.com/basidiocarp/hyphae hyphae-cli --no-default-features",
        ));
    }
    if rhizome_info.is_none() {
        missing.push((
            "rhizome",
            "cargo install --git https://github.com/basidiocarp/rhizome rhizome-cli",
        ));
    }
    if cap_version.is_none() {
        missing.push((
            "cap",
            "git clone https://github.com/basidiocarp/cap && cd cap && npm i && npm run dev:all",
        ));
    }

    if !missing.is_empty() {
        println!();
        println!("{}", "Missing tools:".bold());
        for (name, cmd) in &missing {
            println!("  {:<10}{} {}", name, "\u{2192}".dimmed(), cmd.dimmed());
        }
        println!();
        println!(
            "Or install all: {}",
            "curl -sSfL https://raw.githubusercontent.com/basidiocarp/.github/main/install.sh | sh"
                .dimmed()
        );
    }

    println!();
    Ok(())
}

/// Print a single tool's status line.
fn print_tool_status(name: &str, version: Option<&str>) {
    if let Some(v) = version {
        println!(
            "  {:<10}v{:<8}{}",
            name.bold(),
            v,
            "\u{2713} installed".green()
        );
    } else {
        let hint = match name {
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

/// Build MCP server configurations from discovered tools.
fn build_server_configs() -> Vec<ServerConfig> {
    build_ecosystem_servers(&discover(Tool::Hyphae), &discover(Tool::Rhizome))
}

fn build_ecosystem_servers(
    hyphae_info: &Option<spore::ToolInfo>,
    rhizome_info: &Option<spore::ToolInfo>,
) -> Vec<ServerConfig> {
    let mut servers = Vec::new();
    if hyphae_info.is_some() {
        servers.push(ServerConfig {
            name: "hyphae".to_string(),
            command: "hyphae".to_string(),
            args: vec!["serve".to_string()],
        });
    }
    if rhizome_info.is_some() {
        servers.push(ServerConfig {
            name: "rhizome".to_string(),
            command: "rhizome".to_string(),
            args: vec!["serve".to_string(), "--expanded".to_string()],
        });
    }
    servers
}

fn configure_codex_cli(
    hyphae_info: &Option<spore::ToolInfo>,
    rhizome_info: &Option<spore::ToolInfo>,
    verbose: u8,
) {
    let servers = build_ecosystem_servers(hyphae_info, rhizome_info);

    if servers.is_empty() {
        return;
    }

    println!();
    println!("{}", "Configuring Codex host mode...".bold());
    println!();

    match clients::register_servers(McpClient::CodexCli, &servers, verbose) {
        Ok(true) => {
            println!();
            println!(
                "  {} MCP servers registered for Codex host mode:",
                "\u{2713}".green()
            );
            for server in &servers {
                println!("    - {}", server.name);
            }
        }
        Ok(false) => {
            eprintln!(
                "  {} Codex host mode registration returned false",
                "!".yellow()
            );
        }
        Err(e) => {
            eprintln!(
                "  {} Codex host mode registration failed: {}",
                "!".yellow(),
                e
            );
        }
    }
}

/// Configure detected MCP clients that are not already handled by dedicated ecosystem flows.
#[allow(clippy::ref_option)]
fn configure_detected_clients(
    client_filter: Option<&str>,
    hyphae_info: &Option<spore::ToolInfo>,
    rhizome_info: &Option<spore::ToolInfo>,
    verbose: u8,
) {
    let mut servers = Vec::new();
    if hyphae_info.is_some() {
        servers.push(ServerConfig {
            name: "hyphae".to_string(),
            command: "hyphae".to_string(),
            args: vec!["serve".to_string()],
        });
    }
    if rhizome_info.is_some() {
        servers.push(ServerConfig {
            name: "rhizome".to_string(),
            command: "rhizome".to_string(),
            args: vec!["serve".to_string(), "--expanded".to_string()],
        });
    }

    if servers.is_empty() {
        return;
    }

    // Determine which clients to configure
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
        // No filter: detect all installed, skipping clients already covered above.
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

        match clients::register_servers(*target, &servers, verbose) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_tool_status_installed_does_not_panic() {
        // Just verify no panics with various inputs
        print_tool_status("mycelium", Some("0.2.0"));
        print_tool_status("hyphae", Some("0.6.0"));
    }

    #[test]
    fn test_print_tool_status_missing_does_not_panic() {
        print_tool_status("cap", None);
        print_tool_status("rhizome", None);
        print_tool_status("codex", None);
    }

    #[test]
    fn test_discover_cap_does_not_panic() {
        // Cap likely not installed in test env — just verify no panic
        let _result = discover_cap();
    }

    #[test]
    fn test_discover_codex_version_does_not_panic() {
        let _result = discover_codex_version();
    }

    #[test]
    fn test_claude_is_available_does_not_panic() {
        let _result = claude_is_available();
    }
}
