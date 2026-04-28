use anyhow::Result;
use clap::{Parser, Subcommand};
use spore::logging::{LogOutput, LoggingConfig, SpanContext, SpanEvents, root_span, workflow_span};
use tracing::Level;

mod backup;
mod banner;
mod commands;
mod ecosystem;
mod lockfile;
pub(crate) mod verify;

#[derive(Parser)]
#[command(name = "stipe", version, about = "Basidiocarp ecosystem manager")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Install ecosystem tools and profile surfaces
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

        /// Build from local source instead of downloading release binary
        #[arg(long)]
        from_source: bool,

        /// Path to local source directory (default: ~/projects/basidiocarp/{tool})
        #[arg(long)]
        source_dir: Option<std::path::PathBuf>,

        /// Override any existing install lock
        #[arg(long)]
        force: bool,

        /// Specific tools to install
        tools: Vec<String>,
    },

    /// Update installed tools to latest versions
    Update {
        /// Update tools from a predefined profile
        #[arg(long, value_enum)]
        profile: Option<commands::install::InstallProfile>,

        /// Update all installed tools
        #[arg(long)]
        all: bool,

        /// Check for updates without installing
        #[arg(long)]
        check: bool,

        /// Override any existing install lock
        #[arg(long)]
        force: bool,

        /// Specific tools to update
        tools: Vec<String>,
    },

    /// Inspect or update the stipe binary itself
    #[command(name = "self")]
    SelfCmd {
        #[command(subcommand)]
        command: commands::self_update::SelfCommand,
    },

    /// Configure MCP clients, Codex notify adapters, hooks, databases, and repair drift
    Init {
        /// Target a specific MCP client
        #[arg(long)]
        client: Option<String>,

        /// Scope for host-specific adapter configuration
        #[arg(long, value_enum, default_value_t = commands::host_policy::HostConfigScope::User)]
        scope: commands::host_policy::HostConfigScope,

        /// Show what would change without mutating the machine
        #[arg(long)]
        dry_run: bool,

        /// Emit structured JSON instead of human-readable text
        #[arg(long)]
        json: bool,

        /// Reapply shared ecosystem configuration and refresh the baseline
        #[arg(long, alias = "force", alias = "repair-hooks")]
        repair: bool,

        /// Prompt for optional project context to seed into hyphae
        #[arg(long, default_value_t = false)]
        interactive: bool,
    },

    /// Inspect and configure supported hosts
    Host {
        #[command(subcommand)]
        command: commands::host::HostCommand,
    },

    /// Check ecosystem health
    Doctor {
        /// Emit structured JSON instead of human-readable text
        #[arg(long)]
        json: bool,

        /// Include advisory developer tool checks
        #[arg(long)]
        developer: bool,

        /// Run deep verification, including functional smoke tests and MCP handshakes
        #[arg(long)]
        deep: bool,
    },

    /// Repair packaged skill and plugin state using Lamella with backup and rollback targets
    Package {
        /// Optional install profile label used for package drift and audit metadata
        #[arg(long, value_enum)]
        profile: Option<commands::install::InstallProfile>,

        /// Show what would change without mutating packaged state
        #[arg(long)]
        dry_run: bool,
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

    /// Inspect and configure ecosystem providers
    Provider {
        #[command(subcommand)]
        command: commands::provider::ProviderCommand,
    },

    /// Show ecosystem status
    Status,

    /// Restore from a previous backup
    Rollback {
        #[command(flatten)]
        args: commands::rollback::RollbackArgs,
    },

    /// Create manual backups of ecosystem tools and data
    Backup {
        #[command(subcommand)]
        command: BackupCommand,
    },
}

#[derive(Debug, Subcommand)]
enum BackupCommand {
    /// Backup the Hyphae database and binary
    Hyphae,
}

fn main() -> Result<()> {
    spore::logging::init_with_config(
        LoggingConfig::for_app("stipe", Level::WARN)
            .with_output(LogOutput::Stderr)
            .with_span_events(SpanEvents::Lifecycle),
    );
    let span_context = current_span_context();
    let _root_span = root_span(&span_context).entered();
    let cli = Cli::parse();
    let _workflow_span = workflow_span(command_name(&cli.command), &span_context).entered();

    match cli.command {
        Commands::Install {
            profile,
            all,
            dry_run,
            from_source,
            source_dir,
            force,
            tools,
        } => commands::install::run(
            all,
            profile,
            dry_run,
            from_source,
            source_dir,
            force,
            &tools,
        ),
        Commands::Update {
            profile,
            all,
            check,
            force,
            tools,
        } => commands::update::run(all, profile, check, force, &tools),
        Commands::SelfCmd { command } => commands::self_update::run(command),
        Commands::Init {
            client,
            scope,
            dry_run,
            json,
            repair,
            interactive,
        } => commands::init::run(client.as_deref(), scope, dry_run, json, repair, interactive),
        Commands::Host { command } => commands::host::run(command),
        Commands::Doctor {
            json,
            developer,
            deep,
        } => commands::doctor::run(json, developer, deep),
        Commands::Package { profile, dry_run } => commands::package_repair::run(profile, dry_run),
        Commands::Uninstall {
            all,
            dry_run,
            tools,
        } => commands::uninstall::run(all, dry_run, &tools),
        Commands::Provider { command } => commands::provider::run(command),
        Commands::Status => commands::status::run(),
        Commands::Rollback { args } => commands::rollback::run(&args),
        Commands::Backup { command } => match command {
            BackupCommand::Hyphae => commands::backup::backup_hyphae(),
        },
    }
}

fn current_span_context() -> SpanContext {
    let context = SpanContext::for_app("stipe");
    match commands::host_policy::project_root().or_else(|| std::env::current_dir().ok()) {
        Some(path) => context.with_workspace_root(path.display().to_string()),
        None => context,
    }
}

fn command_name(command: &Commands) -> &'static str {
    match command {
        Commands::Install { .. } => "install",
        Commands::Update { .. } => "update",
        Commands::SelfCmd { .. } => "self",
        Commands::Init { .. } => "init",
        Commands::Host { .. } => "host",
        Commands::Doctor { .. } => "doctor",
        Commands::Package { .. } => "package",
        Commands::Uninstall { .. } => "uninstall",
        Commands::Provider { .. } => "provider",
        Commands::Status => "status",
        Commands::Rollback { .. } => "rollback",
        Commands::Backup { .. } => "backup",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_removed_setup_shim_is_rejected() {
        let Err(err) = Cli::try_parse_from(["stipe", "setup", "codex"]) else {
            panic!("expected parse failure");
        };
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidSubcommand);
    }

    #[test]
    fn test_update_accepts_profile_flag() {
        let cli = Cli::try_parse_from(["stipe", "update", "--profile", "claude-code"])
            .expect("update should accept install profiles");

        match cli.command {
            Commands::Update { profile, .. } => {
                assert_eq!(profile, Some(commands::install::InstallProfile::ClaudeCode));
            }
            _ => panic!("expected update command"),
        }
    }

    #[test]
    fn test_self_update_check_subcommand_parses() {
        let cli = Cli::try_parse_from(["stipe", "self", "update", "--check"])
            .expect("self update check should parse");

        match cli.command {
            Commands::SelfCmd { command } => match command {
                commands::self_update::SelfCommand::Update { check } => assert!(check),
            },
            _ => panic!("expected self command"),
        }
    }

    #[test]
    fn test_doctor_accepts_developer_flag() {
        let cli = Cli::try_parse_from(["stipe", "doctor", "--developer"])
            .expect("doctor should accept developer flag");

        match cli.command {
            Commands::Doctor { developer, .. } => assert!(developer),
            _ => panic!("expected doctor command"),
        }
    }

    #[test]
    fn test_doctor_accepts_deep_flag() {
        let cli = Cli::try_parse_from(["stipe", "doctor", "--deep"])
            .expect("doctor should accept deep flag");

        match cli.command {
            Commands::Doctor { deep, .. } => assert!(deep),
            _ => panic!("expected doctor command"),
        }
    }

    #[test]
    fn test_package_accepts_profile_and_dry_run() {
        let cli = Cli::try_parse_from(["stipe", "package", "--profile", "codex", "--dry-run"])
            .expect("package command should parse with profile and dry-run");

        match cli.command {
            Commands::Package { profile, dry_run } => {
                assert_eq!(profile, Some(commands::install::InstallProfile::Codex));
                assert!(dry_run);
            }
            _ => panic!("expected package command"),
        }
    }

    #[test]
    fn test_install_accepts_developer_profile_alias() {
        let cli = Cli::try_parse_from(["stipe", "install", "--profile", "developer"])
            .expect("developer alias should parse");

        match cli.command {
            Commands::Install { profile, .. } => {
                assert_eq!(
                    profile,
                    Some(commands::install::InstallProfile::DeveloperTools)
                );
            }
            _ => panic!("expected install command"),
        }
    }

    #[test]
    fn test_install_accepts_standard_profile() {
        let cli = Cli::try_parse_from(["stipe", "install", "--profile", "standard"])
            .expect("standard profile should parse");

        match cli.command {
            Commands::Install { profile, .. } => {
                assert_eq!(profile, Some(commands::install::InstallProfile::Standard));
            }
            _ => panic!("expected install command"),
        }
    }

    #[test]
    fn test_install_accepts_full_profile_alias() {
        let cli = Cli::try_parse_from(["stipe", "install", "--profile", "full"])
            .expect("full profile should parse");

        match cli.command {
            Commands::Install { profile, .. } => {
                assert_eq!(profile, Some(commands::install::InstallProfile::FullStack));
            }
            _ => panic!("expected install command"),
        }
    }

    #[test]
    fn test_init_accepts_repair_flag() {
        let cli =
            Cli::try_parse_from(["stipe", "init", "--repair"]).expect("repair flag should parse");

        match cli.command {
            Commands::Init { repair, .. } => assert!(repair),
            _ => panic!("expected init command"),
        }
    }

    #[test]
    fn test_init_accepts_force_alias() {
        let cli =
            Cli::try_parse_from(["stipe", "init", "--force"]).expect("force alias should parse");

        match cli.command {
            Commands::Init { repair, .. } => assert!(repair),
            _ => panic!("expected init command"),
        }
    }
}
