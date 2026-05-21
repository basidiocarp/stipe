use anyhow::Result;
use colored::Colorize;

use super::host_policy::HostConfigScope;
use super::init::{self, InitOptions};
use super::install::{self, InstallOptions, InstallProfile};

pub struct SetupOptions {
    pub profile: Option<InstallProfile>,
    pub client: Option<String>,
    pub dry_run: bool,
    pub interactive: bool,
}

pub fn run(opts: &SetupOptions) -> Result<()> {
    let profile = opts.profile.unwrap_or(InstallProfile::Standard);

    let install_opts = InstallOptions {
        all: false,
        profile: Some(profile),
        dry_run: opts.dry_run,
        from_source: false,
        source_dir: None,
        force: false,
    };
    install::run(&install_opts, &[])?;

    println!();
    println!("{}", "─".repeat(75));
    println!();

    let init_opts = InitOptions {
        client: opts.client.clone(),
        scope: HostConfigScope::User,
        dry_run: opts.dry_run,
        json: false,
        repair: false,
        interactive: opts.interactive,
    };
    if let Err(init_err) = init::run(&init_opts) {
        eprintln!(
            "  {} Install succeeded but host initialization failed: {init_err}",
            "!".red()
        );
        eprintln!(
            "  {} Binaries are installed. Re-run {} to complete host wiring.",
            "→".yellow(),
            "`stipe init`".bold()
        );
        return Err(init_err);
    }

    if !opts.dry_run {
        println!();
        println!("{}", "Next steps:".bold());
        println!("  Open a new terminal for PATH changes to take effect.");
        println!(
            "  Run {} inside a project to add project-level config.",
            "`stipe init --scope project`".bold()
        );
        println!("  Run {} to verify your setup.", "`stipe doctor`".bold());
        println!();
    }

    Ok(())
}
