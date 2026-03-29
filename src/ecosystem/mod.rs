//! Ecosystem management: tool detection, MCP registration, database initialization.

pub mod clients;
mod configure;
mod mcp;
mod status;

use anyhow::Result;
use colored::Colorize;

use crate::commands::claude_hooks;
use crate::commands::host_policy::{self, HostConfigScope};
use crate::commands::tool_registry::{self, ToolProbe};
use configure::{configure_codex_cli, configure_detected_clients, initialize_hyphae_db_if_needed};
use mcp::register_mcp;
use status::{
    build_server_configs, claude_is_available, discover_codex_version, print_tool_status,
    tool_probe,
};

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
        initialize_hyphae_db_if_needed(hyphae_probe.is_installed());

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
