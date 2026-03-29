use anyhow::Result;

use super::host_policy::{HostConfigScope, HostMode};

mod doctor_report;
mod inventory;
mod model;
mod render;
#[cfg(test)]
mod tests;

#[cfg(test)]
use crate::commands::host_policy::HostAdapterKind;
pub(crate) use doctor_report::build_host_doctor_report;
#[cfg(test)]
use doctor_report::doctor_checks_for_entry;
#[cfg(test)]
use inventory::inventory_entry;
pub(crate) use model::HostCommand;
#[cfg(test)]
use model::HostInventoryEntry;
#[cfg(test)]
use render::{render_doctor, render_list};

pub fn run(command: HostCommand) -> Result<()> {
    match command {
        HostCommand::List => {
            render::run_list();
            Ok(())
        }
        HostCommand::Setup {
            mode,
            scope,
            dry_run,
        } => render::run_setup(mode, scope, dry_run),
        HostCommand::Doctor { mode, json } => render::run_doctor(mode, json),
        HostCommand::LegacyClaudeCode { dry_run } => {
            render::run_setup(HostMode::ClaudeCode, HostConfigScope::User, dry_run)
        }
        HostCommand::LegacyCodex { dry_run } => {
            render::run_setup(HostMode::Codex, HostConfigScope::User, dry_run)
        }
        HostCommand::LegacyCursor { dry_run } => {
            render::run_setup(HostMode::Cursor, HostConfigScope::User, dry_run)
        }
    }
}
