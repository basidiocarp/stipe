//! Ecosystem management: tool detection, MCP registration, database initialization.

pub mod clients;
mod configure;
mod context;
mod mcp;
mod status;
mod workflow;

use anyhow::Result;

use crate::commands::host_policy::HostConfigScope;

use context::EcosystemContext;
use status::{build_server_configs, print_status_report};
use workflow::execute;

/// Main entry point for ecosystem setup.
#[allow(clippy::unnecessary_wraps, reason = "CLI command boundary stays Result-shaped")]
pub fn run_ecosystem(client: Option<&str>, scope: HostConfigScope, verbose: u8) -> Result<()> {
    if client
        .as_ref()
        .is_some_and(|value| value.eq_ignore_ascii_case("generic"))
    {
        clients::print_generic_config(&build_server_configs());
        return Ok(());
    }

    let context = EcosystemContext::build(client);
    print_status_report(&context);
    execute(&context, client, scope, verbose);
    Ok(())
}
