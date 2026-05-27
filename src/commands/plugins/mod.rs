//! `stipe plugins` subcommand: list, install, enable, disable, status.

use anyhow::{Result, anyhow};
use clap::Subcommand;

mod list;
mod state;

#[derive(Debug, Subcommand)]
pub enum PluginsCommand {
    /// Show installed Claude plugins and their status
    List,

    /// Install a plugin via lamella
    ///
    /// Examples:
    ///   stipe plugins install --ecosystem
    ///   stipe plugins install --all
    ///   stipe plugins install core
    Install {
        /// Install the ecosystem meta-plugin (core + ai-agents + workflow + tools + rust + typescript)
        #[arg(long, conflicts_with_all = ["all", "name"])]
        ecosystem: bool,

        /// Install every available plugin
        #[arg(long, conflicts_with_all = ["ecosystem", "name"])]
        all: bool,

        /// Named plugin to install
        #[arg(conflicts_with_all = ["ecosystem", "all"])]
        name: Option<String>,
    },

    /// Re-enable a disabled plugin
    Enable {
        /// Plugin name
        name: String,
    },

    /// Disable a plugin (keeps files, removes from active set)
    Disable {
        /// Plugin name
        name: String,
    },

    /// Show a short health summary (installed, enabled, disabled counts)
    Status,
}

pub fn run(command: PluginsCommand) -> Result<()> {
    match command {
        PluginsCommand::List => list::run(),
        PluginsCommand::Install {
            ecosystem,
            all,
            name,
        } => install_plugin(ecosystem, all, name.as_deref()),
        PluginsCommand::Enable { name } => state::enable(&name),
        PluginsCommand::Disable { name } => state::disable(&name),
        PluginsCommand::Status => {
            list::status();
            Ok(())
        }
    }
}

fn install_plugin(ecosystem: bool, all: bool, name: Option<&str>) -> Result<()> {
    let args: Vec<&str> = if all {
        vec!["install", "--all"]
    } else if ecosystem {
        vec!["install", "ecosystem"]
    } else if let Some(plugin_name) = name {
        vec!["install", plugin_name]
    } else {
        return Err(anyhow!(
            "specify a plugin name, --ecosystem, or --all\n\
             Examples:\n  \
             stipe plugins install --ecosystem\n  \
             stipe plugins install --all\n  \
             stipe plugins install core"
        ));
    };

    crate::ecosystem::lamella::run_lamella(&args)
}
