use crate::ecosystem;
use anyhow::Result;
use spore::logging::{SpanContext, workflow_span};

use super::host_policy::HostConfigScope;
use super::tool_registry;

pub(crate) mod baseline;
mod model;
mod plan;
mod render;
mod seed;
mod snapshot;

use baseline::record_current_baseline;
use plan::build_plan;
use render::{print_embedded_preview, print_preview};
use seed::seed_first_run;
use snapshot::build_snapshot;

#[cfg(test)]
mod tests;

/// Configuration options for the init flow.
/// These bools represent distinct, related configuration options that are only meaningful together.
#[allow(clippy::struct_excessive_bools)]
pub struct InitOptions {
    /// Client name if specified.
    pub client: Option<String>,
    /// Scope for host configuration.
    pub scope: HostConfigScope,
    /// Print preview and exit without applying changes.
    pub dry_run: bool,
    /// Output plan as JSON instead of applying changes.
    pub json: bool,
    /// Reapply configuration in repair mode.
    pub repair: bool,
    /// Prompt for user input interactively.
    pub interactive: bool,
}

pub fn run(opts: &InitOptions) -> Result<()> {
    let client_ref = opts.client.as_deref();
    let span_context = init_span_context(client_ref);
    let _workflow_span = workflow_span("init", &span_context).entered();
    let snapshot = build_snapshot(client_ref, opts.scope)?;
    let plan = build_plan(&snapshot, opts.dry_run);

    if opts.json {
        if !opts.dry_run {
            ecosystem::run_ecosystem(client_ref, opts.scope, ecosystem::EcosystemOptions::quiet(0))?;
            record_current_baseline(&snapshot, opts.scope)?;
        }
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }

    if opts.dry_run {
        print_preview(&snapshot);
        return Ok(());
    }

    if opts.repair {
        println!("Repair mode: reapplying shared ecosystem configuration.");
        println!();
    }

    ecosystem::run_ecosystem(client_ref, opts.scope, ecosystem::EcosystemOptions::new(0))?;
    record_current_baseline(&snapshot, opts.scope)?;

    // Write capability registry snapshot (non-fatal)
    if let Err(e) =
        tool_registry::write_capability_registry(&tool_registry::default_registry_path())
    {
        tracing::warn!(error = %e, "capability registry write failed (non-fatal)");
    }

    // Seed initial project context into hyphae if available
    if let Some(project) = get_project_name() {
        let result = if opts.interactive {
            seed::seed_first_run_interactive(&project, false)
        } else {
            seed_first_run(&project, false)
        };

        if let Err(e) = result {
            tracing::warn!(error = %e, "first-run seeding failed (non-fatal)");
        }
    }

    // Prompt for volva operating mode (non-fatal if unavailable or non-interactive)
    if opts.interactive {
        if let Err(e) = prompt_and_write_volva_mode() {
            tracing::warn!(error = %e, "volva mode configuration failed (non-fatal)");
        }
    }

    Ok(())
}

pub(crate) fn run_embedded_preview(opts: &InitOptions) -> Result<()> {
    let client_ref = opts.client.as_deref();
    let snapshot = build_snapshot(client_ref, opts.scope)?;
    print_embedded_preview(&snapshot);
    Ok(())
}

fn init_span_context(client: Option<&str>) -> SpanContext {
    let context = SpanContext::for_app("stipe");
    let context = match client {
        Some(client) => context.with_tool(client),
        None => context,
    };

    match crate::commands::host_policy::project_root().or_else(|| std::env::current_dir().ok()) {
        Some(path) => context.with_workspace_root(path.display().to_string()),
        None => context,
    }
}

/// Extract the project name from the current working directory basename.
fn get_project_name() -> Option<String> {
    std::env::current_dir()
        .ok()?
        .file_name()?
        .to_str()
        .map(std::borrow::ToOwned::to_owned)
}

/// Prompt the user to choose a volva operating mode and write it to `~/.config/volva/config.toml`.
/// Defaults to `baseline` if stdin is not a terminal or the user skips the prompt.
fn prompt_and_write_volva_mode() -> Result<()> {
    use std::io::{BufRead, IsTerminal, Write};

    if !std::io::stdin().is_terminal() {
        return write_volva_mode_config("baseline");
    }

    println!();
    println!("Which mode do you want for volva?");
    println!("  [1] baseline      — hyphae, mycelium, rhizome (recommended default)");
    println!("  [2] orchestration — full coordination with canopy and hymenium");
    print!("Choose [1/2] (default: 1): ");
    std::io::stdout().flush()?;

    let mut input = String::new();
    std::io::stdin().lock().read_line(&mut input)?;
    let choice = input.trim();

    let mode = match choice {
        "2" | "orchestration" => "orchestration",
        _ => "baseline",
    };

    write_volva_mode_config(mode)?;
    println!("volva mode set to: {mode}");
    Ok(())
}

/// Write `mode = "<mode>"` to `~/.config/volva/config.toml`, creating the directory if needed.
fn write_volva_mode_config(mode: &str) -> Result<()> {
    use std::io::Write;

    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine config directory"))?
        .join("volva");

    std::fs::create_dir_all(&config_dir)?;

    let config_path = config_dir.join("config.toml");
    let mut file = std::fs::File::create(&config_path)?;
    writeln!(file, "# Volva global configuration")?;
    writeln!(
        file,
        "# Managed by stipe. Edit manually or re-run stipe init to change."
    )?;
    writeln!(file, "mode = \"{mode}\"")?;

    Ok(())
}
