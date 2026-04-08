use crate::ecosystem;
use anyhow::Result;
use spore::logging::{SpanContext, workflow_span};

use super::host_policy::HostConfigScope;

pub(crate) mod baseline;
mod model;
mod plan;
mod render;
mod snapshot;

use baseline::record_current_baseline;
use plan::build_plan;
use render::print_preview;
use snapshot::build_snapshot;

#[cfg(test)]
mod tests;

pub fn run(
    client: Option<&str>,
    scope: HostConfigScope,
    dry_run: bool,
    json: bool,
    repair: bool,
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
    record_current_baseline(&snapshot, scope)
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
