use anyhow::Result;
use clap::{Parser, Subcommand};

mod banner;
mod commands;
mod ecosystem;

#[derive(Parser)]
#[command(name = "stipe", version, about = "Basidiocarp ecosystem manager")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Install ecosystem tools (mycelium, hyphae, rhizome, cortina)
    Install {
        /// Install all tools
        #[arg(long)]
        all: bool,

        /// Specific tools to install
        tools: Vec<String>,
    },

    /// Update installed tools to latest versions
    Update {
        /// Update all installed tools
        #[arg(long)]
        all: bool,

        /// Check for updates without installing
        #[arg(long)]
        check: bool,

        /// Specific tools to update
        tools: Vec<String>,
    },

    /// Configure MCP clients, hooks, and databases
    Init {
        /// Target a specific MCP client
        #[arg(long)]
        client: Option<String>,
    },

    /// Check ecosystem health
    Doctor,

    /// Remove ecosystem tools and configuration
    Uninstall {
        /// Remove all tools and configuration
        #[arg(long)]
        all: bool,

        /// Specific tools to remove
        tools: Vec<String>,
    },

    /// Show ecosystem status
    Status,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Install { all, tools } => commands::install::run(all, &tools),
        Commands::Update { all, check, tools } => commands::update::run(all, check, &tools),
        Commands::Init { client } => commands::init::run(client.as_deref()),
        Commands::Doctor => commands::doctor::run(),
        Commands::Uninstall { all, tools } => commands::uninstall::run(all, &tools),
        Commands::Status => commands::status::run(),
    }
}
