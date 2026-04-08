use colored::Colorize;
use spore::logging::{SpanContext, subprocess_span, tool_span, workflow_span};
use std::process::{Command, Output};

use crate::commands::codex_notify;
use crate::commands::host_policy::{self, HostConfigScope};

use super::EcosystemOptions;
use super::clients::{self, McpClient};
use super::status::build_ecosystem_servers;

fn configure_mcp_client(
    client: McpClient,
    label: &str,
    success_label: &str,
    hyphae_installed: bool,
    rhizome_installed: bool,
    scope: HostConfigScope,
    options: EcosystemOptions,
) -> bool {
    let span_context = ecosystem_span_context(client.name());
    let _tool_span = tool_span("configure_mcp_client", &span_context).entered();
    let servers = build_ecosystem_servers(hyphae_installed, rhizome_installed);

    if servers.is_empty() {
        return false;
    }

    if options.emit_stdout {
        println!();
        println!("{}", format!("Configuring {label}...").bold());
        println!();
    }

    match clients::register_servers(client, &servers, scope, options.verbose) {
        Ok(true) => {
            if options.emit_stdout {
                println!();
                println!(
                    "  {} MCP servers registered for {success_label}:",
                    "\u{2713}".green()
                );
                for server in &servers {
                    println!("    - {}", server.name);
                }
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
    options: EcosystemOptions,
) {
    let span_context = ecosystem_span_context("codex");
    let _workflow_span = workflow_span("configure_codex_cli", &span_context).entered();

    if !host_policy::host_scope_supported(host_policy::HostMode::Codex, scope) {
        eprintln!(
            "  {} Codex host mode does not support the '{}' scope — skipping Codex configuration.",
            "!".yellow(),
            host_policy::scope_name(scope)
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
        options,
    ) {
        return;
    }

    if hyphae_installed {
        match codex_notify::install_codex_notify(scope, options.verbose) {
            Ok(true) => {
                if options.emit_stdout {
                    println!("    - Hyphae Codex notify adapter");
                }
            }
            Ok(false) => eprintln!("  {} Codex notify installation skipped", "!".yellow()),
            Err(e) => eprintln!("  {} Codex notify installation failed: {}", "!".yellow(), e),
        }
    }
}

pub(super) fn configure_cursor_host(
    hyphae_installed: bool,
    rhizome_installed: bool,
    options: EcosystemOptions,
) {
    let _ = configure_mcp_client(
        McpClient::Cursor,
        "Cursor mode",
        "Cursor mode",
        hyphae_installed,
        rhizome_installed,
        HostConfigScope::User,
        options,
    );
}

pub(super) fn initialize_hyphae_db_if_needed(hyphae_installed: bool, emit_stdout: bool) {
    let span_context = ecosystem_span_context("hyphae");
    let _tool_span = tool_span("initialize_hyphae_db_if_needed", &span_context).entered();

    if let Some(data_dir) = hyphae_installed
        .then(dirs::data_dir)
        .flatten()
        .map(|d| d.join("hyphae"))
        .filter(|d| !d.join("hyphae.db").exists())
    {
        let _ = std::fs::create_dir_all(&data_dir);
        let _subprocess_span = subprocess_span("hyphae stats", &span_context).entered();
        match Command::new("hyphae").arg("stats").output() {
            Ok(output) if output.status.success() => {
                if emit_stdout {
                    println!("  {} Hyphae database initialized", "\u{2713}".green());
                }
            }
            Ok(output) => {
                eprintln!(
                    "  {} Hyphae database initialization failed: {}",
                    "!".yellow(),
                    describe_command_failure("hyphae stats", &output)
                );
            }
            Err(error) => {
                eprintln!(
                    "  {} Hyphae database initialization failed: {}",
                    "!".yellow(),
                    error
                );
            }
        }
    }
}

#[allow(clippy::ref_option)]
pub(super) fn configure_detected_clients(
    client_filter: Option<&str>,
    hyphae_installed: bool,
    rhizome_installed: bool,
    options: EcosystemOptions,
) {
    let span_context = ecosystem_span_context("mcp-clients");
    let _workflow_span = workflow_span("configure_detected_clients", &span_context).entered();
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

    if options.emit_stdout {
        println!();
        println!("{}", "Configuring additional MCP clients...".bold());
    }

    let mut client_configured = Vec::new();

    for target in &targets {
        if client_filter.is_none() && target.handled_separately_in_ecosystem() {
            continue;
        }

        match clients::register_servers(*target, &servers, HostConfigScope::User, options.verbose) {
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

    if options.emit_stdout && !client_configured.is_empty() {
        println!();
        println!("  {} MCP servers registered for:", "\u{2713}".green());
        for name in &client_configured {
            println!("    - {name}");
        }
    }
}

fn ecosystem_span_context(tool: &str) -> SpanContext {
    let context = SpanContext::for_app("stipe").with_tool(tool);
    match host_policy::project_root().or_else(|| std::env::current_dir().ok()) {
        Some(path) => context.with_workspace_root(path.display().to_string()),
        None => context,
    }
}

fn describe_command_failure(command: &str, output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let mut details = vec![format!("{command} exited with {}", output.status)];
    if !stdout.is_empty() {
        details.push(format!("stdout: {stdout}"));
    }
    if !stderr.is_empty() {
        details.push(format!("stderr: {stderr}"));
    }
    details.join("; ")
}
