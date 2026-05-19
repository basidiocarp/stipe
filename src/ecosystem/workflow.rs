use colored::Colorize;
use spore::logging::{SpanContext, tool_span, workflow_span};

use crate::commands::claude_hooks;
use crate::commands::host_policy::{HostConfigScope, HostMode};
use crate::commands::install::release::verify_mcp_handshake;
use crate::commands::tool_registry::{self, ToolProbe};

use super::EcosystemOptions;
use super::configure::{
    configure_codex_cli, configure_cursor_host, configure_detected_clients,
    initialize_hyphae_db_if_needed,
};
use super::context::EcosystemContext;
use super::mcp::{RegistrationStatus, register_mcp};

pub(super) fn execute(
    context: &EcosystemContext,
    client: Option<&str>,
    scope: HostConfigScope,
    options: EcosystemOptions,
) -> anyhow::Result<()> {
    let span_context = ecosystem_span_context("ecosystem");
    let _workflow_span = workflow_span("execute_ecosystem", &span_context).entered();
    let mut failures = Vec::new();

    match context.target_host {
        Some(HostMode::ClaudeCode) => {
            if let Err(err) = configure_claude_code(context, scope, options, true) {
                failures.push(format!("Claude Code configuration failed: {err}"));
            }
            if let Err(err) = initialize_hyphae_db_if_needed(
                context.hyphae_probe.is_installed(),
                options.emit_stdout,
            ) {
                failures.push(format!("Hyphae database initialization failed: {err}"));
            }
        }
        Some(HostMode::Codex) => {
            if let Err(err) = configure_codex_host(context, scope, options) {
                failures.push(format!("Codex host mode configuration failed: {err}"));
            }
            if let Err(err) = initialize_hyphae_db_if_needed(
                context.hyphae_probe.is_installed(),
                options.emit_stdout,
            ) {
                failures.push(format!("Hyphae database initialization failed: {err}"));
            }
        }
        Some(HostMode::Cursor) => {
            if let Err(err) = configure_cursor_host(
                context.hyphae_probe.is_installed(),
                context.rhizome_probe.is_installed(),
                options,
            ) {
                failures.push(format!("Cursor mode configuration failed: {err}"));
            }
            if let Err(err) = initialize_hyphae_db_if_needed(
                context.hyphae_probe.is_installed(),
                options.emit_stdout,
            ) {
                failures.push(format!("Hyphae database initialization failed: {err}"));
            }
        }
        None => {
            if let Err(err) = configure_claude_code(context, scope, options, false) {
                failures.push(format!("Claude Code configuration failed: {err}"));
            }
            if let Err(err) = configure_other_hosts_and_clients(context, client, scope, options) {
                failures.push(err.to_string());
            }
        }
    }
    verify_registered_mcp_servers(context, options.emit_stdout);
    if options.emit_stdout {
        print_repair_hints(context);
        println!();
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(failures.join("; ")))
    }
}

fn verify_registered_mcp_servers(context: &EcosystemContext, emit_stdout: bool) {
    let span_context = ecosystem_span_context("mcp-verification");
    let _workflow_span = workflow_span("verify_registered_mcp_servers", &span_context).entered();

    for tool_name in ["hyphae", "rhizome"] {
        let Some(probe) = context.probe_for_tool(tool_name) else {
            continue;
        };
        if !probe.is_installed() {
            continue;
        }

        let Some(spec) = tool_registry::find(tool_name) else {
            continue;
        };
        let Some(binary_path) = tool_registry::resolve_binary_path(spec) else {
            continue;
        };

        let tool_context = ecosystem_span_context(tool_name);
        let _tool_span = tool_span("verify_registered_mcp_server", &tool_context).entered();
        match verify_mcp_handshake(&binary_path, spec) {
            Ok(()) => {
                if emit_stdout {
                    println!("  {} {} MCP handshake verified", "✓".green(), tool_name);
                }
            }
            Err(error) => {
                eprintln!(
                    "  {} {} MCP handshake failed: {}",
                    "!".yellow(),
                    tool_name,
                    error
                );
                eprintln!("    Reinstall with: stipe install {tool_name} --force");
            }
        }
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "sequential host-setup steps are clearest in one function"
)]
fn configure_claude_code(
    context: &EcosystemContext,
    scope: HostConfigScope,
    options: EcosystemOptions,
    targeted: bool,
) -> anyhow::Result<()> {
    let span_context = ecosystem_span_context("claude");
    let _workflow_span = workflow_span("configure_claude_code", &span_context).entered();

    if !context.claude_runtime_relevant {
        return Ok(());
    }

    if !super::status::claude_is_available() {
        if options.emit_stdout {
            println!(
                "  {} {} not found in PATH — skipping Claude Code configuration.",
                "!".yellow(),
                "claude".bold()
            );
            println!(
                "    Install Claude Code first, then re-run: {}",
                if targeted {
                    "stipe host setup claude-code"
                } else {
                    "stipe init"
                }
            );
        }
        return Ok(());
    }

    if options.emit_stdout {
        println!("{}", "Configuring Claude Code...".bold());
        println!();
    }

    let mut configured = Vec::new();
    let mut failures = Vec::new();

    if context.hyphae_probe.is_installed() {
        let span_context = ecosystem_span_context("hyphae");
        let _tool_span = tool_span("register_hyphae_mcp", &span_context).entered();
        // Resolve absolute path: Claude Code launched from GUI may not have ~/.local/bin on PATH.
        let hyphae_bin = tool_registry::find("hyphae")
            .and_then(tool_registry::resolve_binary_path)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "hyphae".to_string());
        match register_mcp("hyphae", &[&hyphae_bin, "serve"], scope, options.verbose) {
            Ok(status) => configured.push(match status {
                RegistrationStatus::AlreadyRegistered => "hyphae MCP (already registered)",
                RegistrationStatus::Registered => "hyphae MCP",
            }),
            Err(err) => failures.push(format!("hyphae MCP registration error: {err}")),
        }
    }

    if context.rhizome_probe.is_installed() {
        let span_context = ecosystem_span_context("rhizome");
        let _tool_span = tool_span("register_rhizome_mcp", &span_context).entered();
        let rhizome_bin = tool_registry::find("rhizome")
            .and_then(tool_registry::resolve_binary_path)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "rhizome".to_string());
        match register_mcp(
            "rhizome",
            &[&rhizome_bin, "serve", "--expanded"],
            scope,
            options.verbose,
        ) {
            Ok(status) => configured.push(match status {
                RegistrationStatus::AlreadyRegistered => "rhizome MCP (already registered)",
                RegistrationStatus::Registered => "rhizome MCP",
            }),
            Err(err) => failures.push(format!("rhizome MCP registration error: {err}")),
        }
    }

    if options.emit_stdout && !configured.is_empty() {
        println!();
        println!("  {} Configured:", "✓".green());
        for item in &configured {
            println!("    - {item}");
        }
    }

    if context.cortina_probe.is_installed() {
        match claude_hooks::install_claude_hooks(scope, options.verbose) {
            Ok(true) => {
                if options.emit_stdout {
                    println!("    - Cortina Claude hooks");
                }
            }
            Ok(false) => eprintln!("  {} Claude hook installation skipped", "!".yellow()),
            Err(err) => failures.push(format!("Cortina hook registration failed: {err}")),
        }
    } else if matches!(context.cortina_probe, ToolProbe::Broken) {
        failures.push(
            "cortina is installed but broken — repair it before retrying Claude hook registration. Run: stipe install cortina"
                .to_string(),
        );
    } else {
        eprintln!(
            "  {} {} not found in PATH — skipping Claude hook registration.",
            "!".yellow(),
            "cortina".bold()
        );
    }

    // Statusline: prefer annulus over cortina when annulus is installed
    if context.annulus_probe.is_installed() {
        match claude_hooks::install_annulus_statusline(scope, options.verbose) {
            Ok(true) => {
                if options.emit_stdout {
                    println!("    - Annulus statusline");
                }
            }
            Ok(false) => { /* already configured */ }
            Err(err) => failures.push(format!("Annulus statusline configuration failed: {err}")),
        }
    } else if !context.cortina_probe.is_installed()
        && !context.annulus_probe.is_installed()
        && options.emit_stdout
    {
        eprintln!(
            "  {} Neither annulus nor cortina found — skipping statusline.",
            "!".yellow()
        );
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(failures.join("; ")))
    }
}

fn configure_codex_host(
    context: &EcosystemContext,
    scope: HostConfigScope,
    options: EcosystemOptions,
) -> anyhow::Result<()> {
    if context.codex_version.is_some() {
        configure_codex_cli(
            context.hyphae_probe.is_installed(),
            context.rhizome_probe.is_installed(),
            scope,
            options,
        )?;
    } else if options.emit_stdout {
        println!(
            "  {} {} not found in PATH — skipping Codex host mode configuration.",
            "!".yellow(),
            "codex".bold()
        );
        println!("    Install Codex first, then re-run: stipe host setup codex");
    }

    Ok(())
}

fn configure_other_hosts_and_clients(
    context: &EcosystemContext,
    client: Option<&str>,
    scope: HostConfigScope,
    options: EcosystemOptions,
) -> anyhow::Result<()> {
    let mut failures = Vec::new();
    if context.codex_version.is_some() {
        if let Err(err) = configure_codex_cli(
            context.hyphae_probe.is_installed(),
            context.rhizome_probe.is_installed(),
            scope,
            options,
        ) {
            failures.push(format!("Codex host mode configuration failed: {err}"));
        }
    }

    if let Err(err) =
        initialize_hyphae_db_if_needed(context.hyphae_probe.is_installed(), options.emit_stdout)
    {
        failures.push(format!("Hyphae database initialization failed: {err}"));
    }

    if let Err(err) = configure_detected_clients(
        client,
        context.hyphae_probe.is_installed(),
        context.rhizome_probe.is_installed(),
        options,
    ) {
        failures.push(format!("Detected client registration failed: {err}"));
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(failures.join("; ")))
    }
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

fn ecosystem_span_context(tool: &str) -> SpanContext {
    let context = SpanContext::for_app("stipe").with_tool(tool);
    match crate::commands::host_policy::project_root().or_else(|| std::env::current_dir().ok()) {
        Some(path) => context.with_workspace_root(path.display().to_string()),
        None => context,
    }
}
