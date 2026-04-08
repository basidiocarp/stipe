//! Ecosystem management: tool detection, MCP registration, database initialization.

pub mod clients;
mod configure;
mod context;
mod mcp;
mod status;
mod workflow;

use anyhow::Result;
use spore::logging::{SpanContext, workflow_span};

use crate::commands::host_policy::HostConfigScope;

use context::EcosystemContext;
use status::{build_server_configs, print_status_report};
use workflow::execute;

#[derive(Debug, Clone, Copy)]
pub struct EcosystemOptions {
    pub verbose: u8,
    pub emit_stdout: bool,
}

impl EcosystemOptions {
    pub const fn new(verbose: u8) -> Self {
        Self {
            verbose,
            emit_stdout: true,
        }
    }

    pub const fn quiet(verbose: u8) -> Self {
        Self {
            verbose,
            emit_stdout: false,
        }
    }
}

/// Main entry point for ecosystem setup.
#[allow(
    clippy::unnecessary_wraps,
    reason = "CLI command boundary stays Result-shaped"
)]
pub fn run_ecosystem(
    client: Option<&str>,
    scope: HostConfigScope,
    options: EcosystemOptions,
) -> Result<()> {
    let span_context = ecosystem_span_context();
    let _workflow_span = workflow_span("run_ecosystem", &span_context).entered();

    if client
        .as_ref()
        .is_some_and(|value| value.eq_ignore_ascii_case("generic"))
    {
        if options.emit_stdout {
            clients::print_generic_config(&build_server_configs());
        }
        return Ok(());
    }

    let context = EcosystemContext::build(client);
    if options.emit_stdout {
        print_status_report(&context);
    }
    execute(&context, client, scope, options)
}

fn ecosystem_span_context() -> SpanContext {
    let context = SpanContext::for_app("stipe");
    match crate::commands::host_policy::project_root().or_else(|| std::env::current_dir().ok()) {
        Some(path) => context.with_workspace_root(path.display().to_string()),
        None => context,
    }
}
