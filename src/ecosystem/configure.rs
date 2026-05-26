use anyhow::Context;
use colored::Colorize;
use spore::logging::{SpanContext, subprocess_span, tool_span, workflow_span};
use std::process::{Command, Output};
use std::time::Duration;

use crate::commands::codex_notify;
use crate::commands::host_policy::{self, HostConfigScope};
use crate::commands::install::release::run_command_with_timeout;
use crate::commands::tool_registry;

use super::EcosystemOptions;
use super::clients::{self, McpClient};
use super::paths;
use super::status::build_ecosystem_servers;

fn configure_mcp_client(
    client: McpClient,
    label: &str,
    success_label: &str,
    hyphae_installed: bool,
    rhizome_installed: bool,
    scope: HostConfigScope,
    options: EcosystemOptions,
) -> anyhow::Result<()> {
    let span_context = ecosystem_span_context(client.name());
    let _tool_span = tool_span("configure_mcp_client", &span_context).entered();
    let servers = build_ecosystem_servers(hyphae_installed, rhizome_installed);

    if servers.is_empty() {
        return Ok(());
    }

    if options.emit_stdout {
        eprintln!();
        eprintln!("{}", format!("Configuring {label}...").bold());
        eprintln!();
    }

    match clients::register_servers(client, &servers, scope, options.verbose) {
        Ok(()) => {
            if options.emit_stdout {
                eprintln!();
                eprintln!(
                    "  {} MCP servers registered for {success_label}:",
                    "\u{2713}".green()
                );
                for server in &servers {
                    eprintln!("    - {}", server.name);
                }
            }
            Ok(())
        }
        Err(e) => Err(e).with_context(|| format!("{success_label} registration failed")),
    }
}

pub(super) fn configure_codex_cli(
    hyphae_installed: bool,
    rhizome_installed: bool,
    scope: HostConfigScope,
    options: EcosystemOptions,
) -> anyhow::Result<()> {
    let span_context = ecosystem_span_context("codex");
    let _workflow_span = workflow_span("configure_codex_cli", &span_context).entered();

    if !host_policy::host_scope_supported(host_policy::HostMode::Codex, scope) {
        eprintln!(
            "  {} Codex host mode does not support the '{}' scope — skipping Codex configuration.",
            "!".yellow(),
            host_policy::scope_name(scope)
        );
        return Ok(());
    }

    configure_mcp_client(
        McpClient::CodexCli,
        "Codex host mode",
        "Codex host mode",
        hyphae_installed,
        rhizome_installed,
        scope,
        options,
    )?;

    if hyphae_installed {
        match codex_notify::install_codex_notify(scope, options.verbose) {
            Ok(true) => {
                if options.emit_stdout {
                    eprintln!("    - Hyphae Codex notify adapter");
                }
            }
            Ok(false) => {
                if options.emit_stdout {
                    eprintln!("  {} Codex notify installation skipped", "!".yellow());
                }
            }
            Err(e) => return Err(e).context("Codex notify installation failed"),
        }
    }

    match run_lamella_codex_install(options.verbose) {
        Ok(true) => {
            if options.emit_stdout {
                eprintln!("    - Lamella codex skill profiles");
            }
        }
        Ok(false) => {
            // Failure is best-effort; don't fail the overall setup
        }
        Err(e) => {
            if options.verbose > 0 {
                eprintln!("  {} Lamella codex install warning: {e}", "!".yellow());
            }
        }
    }

    Ok(())
}

fn run_lamella_codex_install(verbose: u8) -> anyhow::Result<bool> {
    const LAMELLA_CODEX_TIMEOUT: Duration = Duration::from_secs(30);

    // Prefer spore-based resolution so this works even when ~/.local/bin is not
    // on PATH (e.g. immediately after a fresh install before the shell is reloaded).
    let Some(lamella_path) = tool_registry::find("lamella")
        .and_then(tool_registry::resolve_binary_path)
        .or_else(|| which::which("lamella").ok())
    else {
        if verbose > 0 {
            eprintln!("  lamella not found — skipping skill install");
            eprintln!("  Run 'lamella install-codex' manually after installing lamella");
        }
        return Ok(false);
    };

    let mut cmd = std::process::Command::new(&lamella_path);
    cmd.args(["install-codex"]);
    let output = run_command_with_timeout(&mut cmd, LAMELLA_CODEX_TIMEOUT)
        .with_context(|| "running lamella install-codex")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if verbose > 0 {
            eprintln!("  lamella install-codex failed: {stderr}");
        }
        return Ok(false);
    }

    if verbose > 0 {
        eprintln!("  Installed lamella codex skill profiles to ~/.codex/skills/");
    }
    Ok(true)
}

pub(super) fn configure_cursor_host(
    hyphae_installed: bool,
    rhizome_installed: bool,
    options: EcosystemOptions,
) -> anyhow::Result<()> {
    configure_mcp_client(
        McpClient::Cursor,
        "Cursor mode",
        "Cursor mode",
        hyphae_installed,
        rhizome_installed,
        HostConfigScope::User,
        options,
    )
}

pub(super) fn initialize_hyphae_db_if_needed(
    hyphae_installed: bool,
    emit_stdout: bool,
) -> anyhow::Result<()> {
    const HYPHAE_STATS_TIMEOUT: Duration = Duration::from_secs(10);

    let span_context = ecosystem_span_context("hyphae");
    let _tool_span = tool_span("initialize_hyphae_db_if_needed", &span_context).entered();

    let Some(base) = hyphae_installed.then(dirs::data_dir).flatten() else {
        return Ok(());
    };
    // Check both the canonical path and the legacy path. Doctor uses the same
    // priority order; keeping them aligned prevents spurious re-initialization.
    let new_db = paths::hyphae_db_path(&base);
    let legacy_db = base.join("hyphae").join("hyphae.db");
    if new_db.exists() || legacy_db.exists() {
        return Ok(());
    }
    {
        let data_dir = base.join("basidiocarp").join("hyphae");
        std::fs::create_dir_all(&data_dir)
            .with_context(|| format!("creating {}", data_dir.display()))?;
        // Resolve the binary through the tool registry rather than relying on PATH.
        // PATH may not yet include the install directory if hyphae was just installed
        // and the shell session has not been refreshed.
        let hyphae_spec = tool_registry::find("hyphae")
            .ok_or_else(|| anyhow::anyhow!("hyphae is not listed in the tool registry"))?;
        let hyphae_bin = tool_registry::resolve_binary_path(hyphae_spec).ok_or_else(|| {
            anyhow::anyhow!("hyphae binary not found; ensure it is installed and accessible")
        })?;
        let _subprocess_span = subprocess_span("hyphae stats", &span_context).entered();
        let mut hyphae_cmd = Command::new(&hyphae_bin);
        hyphae_cmd.arg("stats");
        match run_command_with_timeout(&mut hyphae_cmd, HYPHAE_STATS_TIMEOUT) {
            Ok(output) if output.status.success() => {
                if emit_stdout {
                    eprintln!("  {} Hyphae database initialized", "\u{2713}".green());
                }
            }
            Ok(output) => {
                return Err(anyhow::anyhow!(
                    "Hyphae database initialization failed: {}",
                    describe_command_failure("hyphae stats", &output)
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
                tracing::warn!(
                    "hyphae stats timed out after 10 seconds; continuing without initialization"
                );
                return Ok(());
            }
            Err(error) => {
                return Err(error).context("Hyphae database initialization failed");
            }
        }
    }

    Ok(())
}

#[allow(clippy::ref_option)]
pub(super) fn configure_detected_clients(
    client_filter: Option<&str>,
    hyphae_installed: bool,
    rhizome_installed: bool,
    options: EcosystemOptions,
) -> anyhow::Result<()> {
    let span_context = ecosystem_span_context("mcp-clients");
    let _workflow_span = workflow_span("configure_detected_clients", &span_context).entered();
    let servers = build_ecosystem_servers(hyphae_installed, rhizome_installed);

    if servers.is_empty() {
        return Ok(());
    }

    let targets: Vec<McpClient> = if let Some(name) = client_filter {
        if let Some(c) = McpClient::from_flag(name) {
            vec![c]
        } else {
            eprintln!(
                "  {} Unknown client '{name}'. Known: claude-code, cursor, windsurf, continue, claude-desktop, codex, gemini, copilot",
                "!".yellow(),
            );
            return Ok(());
        }
    } else {
        clients::detect_clients()
            .into_iter()
            .filter(|client| !client.handled_separately_in_ecosystem())
            .collect()
    };

    if targets.is_empty() {
        return Ok(());
    }

    if options.emit_stdout {
        eprintln!();
        eprintln!("{}", "Configuring additional MCP clients...".bold());
    }

    let mut client_configured = Vec::new();
    let mut failures = Vec::new();

    for target in &targets {
        if client_filter.is_none() && target.handled_separately_in_ecosystem() {
            continue;
        }

        match clients::register_servers(*target, &servers, HostConfigScope::User, options.verbose) {
            Ok(()) => {
                client_configured.push(target.name());
            }
            Err(e) => failures.push(format!("{} registration failed: {e}", target.name())),
        }
    }

    if options.emit_stdout && !client_configured.is_empty() {
        eprintln!();
        eprintln!("  {} MCP servers registered for:", "\u{2713}".green());
        for name in &client_configured {
            eprintln!("    - {name}");
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(failures.join("; ")))
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
