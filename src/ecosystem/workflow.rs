use colored::Colorize;

use crate::commands::claude_hooks;
use crate::commands::host_policy::{HostConfigScope, HostMode};
use crate::commands::tool_registry::{self, ToolProbe};

use super::configure::{
    configure_codex_cli, configure_detected_clients, initialize_hyphae_db_if_needed,
};
use super::context::EcosystemContext;
use super::mcp::register_mcp;

pub(super) fn execute(
    context: &EcosystemContext,
    client: Option<&str>,
    scope: HostConfigScope,
    verbose: u8,
) {
    configure_claude_code(context, scope, verbose);
    configure_other_hosts_and_clients(context, client, scope, verbose);
    print_repair_hints(context);
    println!();
}

fn configure_claude_code(context: &EcosystemContext, scope: HostConfigScope, verbose: u8) {
    if !(context
        .target_host
        .is_none_or(|mode| mode == HostMode::ClaudeCode)
        && super::status::claude_is_available())
    {
        println!(
            "  {} {} not found in PATH — skipping Claude Code configuration.",
            "!".yellow(),
            "claude".bold()
        );
        println!("    Install Claude Code first, then re-run: stipe init");
        return;
    }

    println!("{}", "Configuring Claude Code...".bold());
    println!();

    let mut configured = Vec::new();

    if context.hyphae_probe.is_installed() {
        match register_mcp("hyphae", &["hyphae", "serve"], scope, verbose) {
            Ok(Some(status)) => configured.push(if status == "already registered" {
                "hyphae MCP (already registered)"
            } else {
                "hyphae MCP"
            }),
            Ok(None) => eprintln!("  {} Failed to register hyphae MCP", "!".yellow()),
            Err(err) => eprintln!("  {} hyphae MCP registration error: {}", "!".yellow(), err),
        }
    }

    if context.rhizome_probe.is_installed() {
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
            Err(err) => eprintln!("  {} rhizome MCP registration error: {}", "!".yellow(), err),
        }
    }

    if !configured.is_empty() {
        println!();
        println!("  {} Configured:", "✓".green());
        for item in &configured {
            println!("    - {item}");
        }
    }

    if context.cortina_probe.is_installed() {
        match claude_hooks::install_claude_hooks(scope, verbose) {
            Ok(true) => println!("    - Cortina Claude hooks"),
            Ok(false) => eprintln!("  {} Claude hook installation skipped", "!".yellow()),
            Err(err) => eprintln!(
                "  {} Cortina hook registration failed: {}",
                "!".yellow(),
                err
            ),
        }
    } else if matches!(context.cortina_probe, ToolProbe::Broken) {
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
}

fn configure_other_hosts_and_clients(
    context: &EcosystemContext,
    client: Option<&str>,
    scope: HostConfigScope,
    verbose: u8,
) {
    if context.target_host == Some(HostMode::Codex) {
        if context.codex_version.is_some() {
            configure_codex_cli(
                context.hyphae_probe.is_installed(),
                context.rhizome_probe.is_installed(),
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
        return;
    }

    if context.target_host.is_none() && context.codex_version.is_some() {
        configure_codex_cli(
            context.hyphae_probe.is_installed(),
            context.rhizome_probe.is_installed(),
            scope,
            verbose,
        );
    }

    initialize_hyphae_db_if_needed(context.hyphae_probe.is_installed());

    configure_detected_clients(
        client,
        context.hyphae_probe.is_installed(),
        context.rhizome_probe.is_installed(),
        verbose,
    );
}

fn print_repair_hints(context: &EcosystemContext) {
    let mut missing: Vec<(&str, &str)> = Vec::new();
    let mut broken: Vec<(&str, &str)> = Vec::new();

    for spec in tool_registry::ecosystem_specs() {
        let Some(probe) = context.probe_for_tool(spec.name) else {
            continue;
        };

        if spec.name == "cortina" && !context.claude_runtime_relevant {
            continue;
        }

        if matches!(probe, ToolProbe::Broken) {
            if let Some(hint) = spec.missing_hint {
                broken.push((spec.name, hint));
            }
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
            println!("  {:<10}{} {}", name, "→".dimmed(), cmd.dimmed());
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
            println!("  {:<10}{} {}", name, "→".dimmed(), cmd.dimmed());
        }
    }
}
