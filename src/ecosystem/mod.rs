//! Ecosystem management: tool detection, MCP registration, database initialization.

pub mod clients;

use anyhow::Result;
use colored::Colorize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::commands::claude_hooks;
use crate::commands::codex_notify;
use crate::commands::host_policy::{self, HostConfigScope};
use crate::commands::tool_registry::{self, ToolProbe};
use clients::{McpClient, ServerConfig};

fn tool_probe(tool_name: &str) -> ToolProbe {
    tool_registry::find(tool_name).map_or(ToolProbe::Missing, tool_registry::probe)
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

fn current_project_root() -> Option<PathBuf> {
    host_policy::project_root()
}

fn claude_mcp_project_path() -> Option<PathBuf> {
    current_project_root().map(|root| root.join(".mcp.json"))
}

fn load_json(path: &Path) -> Option<Value> {
    let content = fs::read_to_string(path).ok()?;
    if content.trim().is_empty() {
        Some(serde_json::json!({}))
    } else {
        serde_json::from_str(&content).ok()
    }
}

fn path_scoped_mcp_exists(root: &Value, project_root: &Path, name: &str) -> bool {
    let project_key = project_root.to_string_lossy();
    root.get("projects")
        .and_then(Value::as_object)
        .and_then(|projects| projects.get(project_key.as_ref()))
        .and_then(|project| project.get("mcpServers"))
        .and_then(Value::as_object)
        .is_some_and(|servers| servers.contains_key(name))
}

/// Check if an MCP server is already registered with Claude Code for the selected scope.
fn mcp_exists(name: &str, scope: HostConfigScope) -> bool {
    match scope {
        HostConfigScope::User => host_policy::host_config_path(host_policy::HostMode::ClaudeCode)
            .as_deref()
            .and_then(load_json)
            .and_then(|root| root.get("mcpServers").and_then(Value::as_object).cloned())
            .is_some_and(|servers| servers.contains_key(name)),
        HostConfigScope::Project => claude_mcp_project_path()
            .as_deref()
            .and_then(load_json)
            .and_then(|root| root.get("mcpServers").and_then(Value::as_object).cloned())
            .is_some_and(|servers| servers.contains_key(name)),
        HostConfigScope::Local => host_policy::host_config_path(host_policy::HostMode::ClaudeCode)
            .as_deref()
            .and_then(load_json)
            .zip(current_project_root())
            .is_some_and(|(root, project_root)| path_scoped_mcp_exists(&root, &project_root, name)),
    }
}

/// Register an MCP server with Claude Code. Returns:
/// - `Ok(Some("registered"))` if newly registered
/// - `Ok(Some("already registered"))` if already present
/// - `Ok(None)` if registration failed
fn register_mcp(
    name: &str,
    args: &[&str],
    scope: HostConfigScope,
    verbose: u8,
) -> Result<Option<&'static str>> {
    if mcp_exists(name, scope) {
        if verbose > 0 {
            eprintln!("  {name} MCP already registered");
        }
        return Ok(Some("already registered"));
    }

    let mut cmd = Command::new("claude");
    cmd.arg("mcp")
        .arg("add")
        .arg("--scope")
        .arg(match scope {
            HostConfigScope::User => "user",
            HostConfigScope::Project => "project",
            HostConfigScope::Local => "local",
        })
        .arg(name);
    cmd.arg("--");
    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!(
            "  Running: claude mcp add --scope {} {} -- {}",
            match scope {
                HostConfigScope::User => "user",
                HostConfigScope::Project => "project",
                HostConfigScope::Local => "local",
            },
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
pub fn run_ecosystem(client: Option<&str>, scope: HostConfigScope, verbose: u8) -> Result<()> {
    // Handle --client generic: just print JSON snippet and exit
    if client
        .as_ref()
        .is_some_and(|c| c.eq_ignore_ascii_case("generic"))
    {
        let servers = build_server_configs();
        clients::print_generic_config(&servers);
        return Ok(());
    }

    let target_host = client.and_then(host_policy::host_mode_from_client_flag);
    let detected_clients = clients::detect_clients();
    let claude_runtime_relevant = target_host == Some(host_policy::HostMode::ClaudeCode)
        || (target_host.is_none()
            && host_policy::host_detected_with_clients(
                host_policy::HostMode::ClaudeCode,
                &detected_clients,
            ));

    // ─────────────────────────────────────────────────────────────────────
    // 1. Discover tools
    // ─────────────────────────────────────────────────────────────────────
    let mycelium_probe = tool_probe("mycelium");
    let hyphae_probe = tool_probe("hyphae");
    let rhizome_probe = tool_probe("rhizome");
    let canopy_probe = tool_probe("canopy");
    let cortina_probe = tool_probe("cortina");
    let cap_probe = tool_probe("cap");
    let codex_version = discover_codex_version();

    // ─────────────────────────────────────────────────────────────────────
    // 2. Print ecosystem status
    // ─────────────────────────────────────────────────────────────────────
    println!();
    println!("{}", "Basidiocarp Ecosystem Status".bold());
    println!("{}", "\u{2500}".repeat(75));
    println!();

    for spec in tool_registry::ecosystem_specs() {
        let version = match spec.name {
            "mycelium" => mycelium_probe.version(),
            "hyphae" => hyphae_probe.version(),
            "rhizome" => rhizome_probe.version(),
            "canopy" => canopy_probe.version(),
            "cortina" => cortina_probe.version(),
            "cap" => cap_probe.version(),
            _ => None,
        };
        let probe = match spec.name {
            "mycelium" => &mycelium_probe,
            "hyphae" => &hyphae_probe,
            "rhizome" => &rhizome_probe,
            "canopy" => &canopy_probe,
            "cortina" => &cortina_probe,
            "cap" => &cap_probe,
            _ => unreachable!(),
        };
        print_tool_status(spec.name, version, probe);
    }

    let codex_probe = if let Some(version) = codex_version.as_ref() {
        ToolProbe::Installed(version.clone())
    } else {
        ToolProbe::Missing
    };
    print_tool_status("codex", codex_version.as_deref(), &codex_probe);

    println!();

    // ─────────────────────────────────────────────────────────────────────
    // 3. Configure Claude Code (if available)
    // ─────────────────────────────────────────────────────────────────────
    if target_host.is_none_or(|mode| mode == host_policy::HostMode::ClaudeCode)
        && claude_is_available()
    {
        println!("{}", "Configuring Claude Code...".bold());
        println!();

        let mut configured = Vec::new();

        // Register hyphae MCP if installed
        if hyphae_probe.is_installed() {
            match register_mcp("hyphae", &["hyphae", "serve"], scope, verbose) {
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
        if rhizome_probe.is_installed() {
            match register_mcp(
                "rhizome",
                &["rhizome", "serve", "--expanded"],
                scope,
                verbose,
            ) {
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

        if cortina_probe.is_installed() {
            match claude_hooks::install_claude_hooks(scope, verbose) {
                Ok(true) => {
                    println!("    - Cortina Claude hooks");
                }
                Ok(false) => {
                    eprintln!("  {} Claude hook installation skipped", "!".yellow());
                }
                Err(e) => {
                    eprintln!("  {} Cortina hook registration failed: {}", "!".yellow(), e);
                }
            }
        } else if matches!(cortina_probe, ToolProbe::Broken) {
            eprintln!(
                "  {} {} is installed but broken — repair it before retrying Claude hook registration.",
                "!".yellow(),
                "cortina".bold()
            );
            eprintln!("    Run: stipe install cortina");
        } else {
            eprintln!(
                "  {} {} not found in PATH — skipping Claude hook registration.",
                "!".yellow(),
                "cortina".bold()
            );
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
    if target_host == Some(host_policy::HostMode::Codex) {
        if codex_version.is_some() {
            configure_codex_cli(
                hyphae_probe.is_installed(),
                rhizome_probe.is_installed(),
                scope,
                verbose,
            );
        } else {
            println!(
                "  {} {} not found in PATH — skipping Codex host mode configuration.",
                "!".yellow(),
                "codex".bold()
            );
            println!("    Install Codex first, then re-run: stipe init");
        }
    } else {
        if target_host.is_none() && codex_version.is_some() {
            configure_codex_cli(
                hyphae_probe.is_installed(),
                rhizome_probe.is_installed(),
                scope,
                verbose,
            );
        }

        // ─────────────────────────────────────────────────────────────────────
        // 3c. Initialize the Hyphae database if needed
        // ─────────────────────────────────────────────────────────────────────
        if let Some(data_dir) = hyphae_probe
            .is_installed()
            .then(dirs::data_dir)
            .flatten()
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
        configure_detected_clients(
            client,
            hyphae_probe.is_installed(),
            rhizome_probe.is_installed(),
            verbose,
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // 4. Print missing tool instructions
    // ─────────────────────────────────────────────────────────────────────
    let mut missing: Vec<(&str, &str)> = Vec::new();
    let mut broken: Vec<(&str, &str)> = Vec::new();
    for spec in tool_registry::ecosystem_specs() {
        let probe = match spec.name {
            "mycelium" => &mycelium_probe,
            "hyphae" => &hyphae_probe,
            "rhizome" => &rhizome_probe,
            "canopy" => &canopy_probe,
            "cortina" => &cortina_probe,
            "cap" => &cap_probe,
            _ => continue,
        };

        if spec.name == "cortina" && !claude_runtime_relevant {
            continue;
        }

        if matches!(probe, ToolProbe::Broken)
            && let Some(hint) = spec.missing_hint
        {
            broken.push((spec.name, hint));
        } else if matches!(probe, ToolProbe::Missing)
            && let Some(hint) = spec.missing_hint
        {
            missing.push((spec.name, hint));
        }
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

    if !broken.is_empty() {
        println!();
        println!("{}", "Broken tools:".bold());
        for (name, cmd) in &broken {
            println!("  {:<10}{} {}", name, "\u{2192}".dimmed(), cmd.dimmed());
        }
    }

    println!();
    Ok(())
}

/// Print a single tool's status line.
fn print_tool_status(name: &str, version: Option<&str>, probe: &ToolProbe) {
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

/// Build MCP server configurations from discovered tools.
fn build_server_configs() -> Vec<ServerConfig> {
    build_ecosystem_servers(
        tool_probe("hyphae").is_installed(),
        tool_probe("rhizome").is_installed(),
    )
}

fn build_ecosystem_servers(hyphae_installed: bool, rhizome_installed: bool) -> Vec<ServerConfig> {
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

fn configure_codex_cli(
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

    let servers = build_ecosystem_servers(hyphae_installed, rhizome_installed);

    if servers.is_empty() {
        return;
    }

    println!();
    println!("{}", "Configuring Codex host mode...".bold());
    println!();

    match clients::register_servers(McpClient::CodexCli, &servers, scope, verbose) {
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

    if hyphae_installed {
        match codex_notify::install_codex_notify(scope, verbose) {
            Ok(true) => println!("    - Hyphae Codex notify adapter"),
            Ok(false) => eprintln!("  {} Codex notify installation skipped", "!".yellow()),
            Err(e) => eprintln!("  {} Codex notify installation failed: {}", "!".yellow(), e),
        }
    }
}

/// Configure detected MCP clients that are not already handled by dedicated ecosystem flows.
#[allow(clippy::ref_option)]
fn configure_detected_clients(
    client_filter: Option<&str>,
    hyphae_installed: bool,
    rhizome_installed: bool,
    verbose: u8,
) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_tool_status_installed_does_not_panic() {
        // Just verify no panics with various inputs
        print_tool_status(
            "mycelium",
            Some("0.2.0"),
            &ToolProbe::Installed("0.2.0".to_string()),
        );
        print_tool_status(
            "hyphae",
            Some("0.6.0"),
            &ToolProbe::Installed("0.6.0".to_string()),
        );
    }

    #[test]
    fn test_print_tool_status_missing_does_not_panic() {
        print_tool_status("cap", None, &ToolProbe::Missing);
        print_tool_status("rhizome", None, &ToolProbe::Missing);
        print_tool_status("codex", None, &ToolProbe::Missing);
    }

    #[test]
    fn test_installed_version_does_not_panic_for_optional_tool() {
        // Cap is often absent in test env — just verify probing stays safe.
        let _result = tool_probe("cap");
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
