use crate::ecosystem;
use anyhow::Result;
use spore::logging::{SpanContext, workflow_span};

use super::host_policy::HostConfigScope;

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

pub fn run(
    client: Option<&str>,
    scope: HostConfigScope,
    dry_run: bool,
    json: bool,
    repair: bool,
    interactive: bool,
) -> Result<()> {
    let span_context = init_span_context(client);
    let _workflow_span = workflow_span("init", &span_context).entered();
    let snapshot = build_snapshot(client, scope)?;
    let plan = build_plan(&snapshot, dry_run);

    if json {
        if !dry_run {
            ecosystem::run_ecosystem(client, scope, ecosystem::EcosystemOptions::quiet(0))?;
            record_current_baseline(&snapshot, scope)?;
        }
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }

    if dry_run {
        print_preview(&snapshot);
        return Ok(());
    }

    if repair {
        println!("Repair mode: reapplying shared ecosystem configuration.");
        println!();
    }

    ecosystem::run_ecosystem(client, scope, ecosystem::EcosystemOptions::new(0))?;
    record_current_baseline(&snapshot, scope)?;

    // Seed initial project context into hyphae if available
    if let Some(project) = get_project_name() {
        let result = if interactive {
            seed::seed_first_run_interactive(&project, false)
        } else {
            seed_first_run(&project, false)
        };

        if let Err(e) = result {
            tracing::warn!(error = %e, "first-run seeding failed (non-fatal)");
        }
    }

    Ok(())
}

pub(crate) fn run_embedded_preview(client: Option<&str>, scope: HostConfigScope) -> Result<()> {
    let snapshot = build_snapshot(client, scope)?;
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
        .map(|s| s.to_string())
}
