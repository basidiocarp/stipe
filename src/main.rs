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
        /// Install a predefined profile
        #[arg(long, value_enum)]
        profile: Option<commands::install::InstallProfile>,

        /// Install all tools
        #[arg(long)]
        all: bool,

        /// Show what would change without mutating the machine
        #[arg(long)]
        dry_run: bool,

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

        /// Show what would change without mutating the machine
        #[arg(long)]
        dry_run: bool,

        /// Emit structured JSON instead of human-readable text
        #[arg(long)]
        json: bool,
    },

    /// Check ecosystem health
    Doctor {
        /// Emit structured JSON instead of human-readable text
        #[arg(long)]
        json: bool,
    },

    /// Remove ecosystem tools and configuration
    Uninstall {
        /// Remove all tools and configuration
        #[arg(long)]
        all: bool,

        /// Show what would change without mutating the machine
        #[arg(long)]
        dry_run: bool,

        /// Specific tools to remove
        tools: Vec<String>,
    },

    /// Show ecosystem status
    Status,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Install {
            profile,
            all,
            dry_run,
            tools,
        } => commands::install::run(all, profile, dry_run, &tools),
        Commands::Update { all, check, tools } => commands::update::run(all, check, &tools),
        Commands::Init {
            client,
            dry_run,
            json,
        } => commands::init::run(client.as_deref(), dry_run, json),
        Commands::Doctor { json } => commands::doctor::run(json),
        Commands::Uninstall {
            all,
            dry_run,
            tools,
        } => commands::uninstall::run(all, dry_run, &tools),
        Commands::Status => commands::status::run(),
    }
}
