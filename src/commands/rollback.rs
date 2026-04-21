use crate::backup;
use anyhow::{Context, Result};
use std::io;
use std::path::PathBuf;

#[derive(Debug, clap::Args)]
pub struct RollbackArgs {
    /// List available backups without restoring.
    #[arg(long)]
    pub list: bool,

    /// Restore from this specific backup timestamp.
    #[arg(long)]
    pub to: Option<String>,

    /// Skip confirmation prompts.
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: &RollbackArgs) -> Result<()> {
    if args.list {
        return list_backups();
    }

    let backups = backup::list_backups()?;
    if backups.is_empty() {
        eprintln!("No backups found. Run stipe install first to create a backup.");
        return Ok(());
    }

    let backup_dir: PathBuf = if let Some(ts) = &args.to {
        let found = backups.iter().find(|(name, _)| name == ts);
        match found {
            Some((_, dir)) => dir.clone(),
            None => anyhow::bail!("No backup found with timestamp: {}", ts),
        }
    } else {
        backups[0].1.clone() // most recent
    };

    let manifest = backup::load_manifest(&backup_dir)?;

    println!(
        "Restoring from backup: {} (stipe v{})",
        manifest.timestamp, manifest.stipe_version
    );
    println!(
        "  Binaries: {}, Config files: {}",
        manifest.binaries.len(),
        manifest.config_files.len()
    );

    if !args.force {
        eprintln!(
            "This will restore {} binaries and {} config files.",
            manifest.binaries.len(),
            manifest.config_files.len()
        );
        eprint!("Type 'y' or 'yes' to confirm, anything else to abort: ");
        use std::io::Write;
        let _ = io::stderr().flush();

        let stdin = io::stdin();
        let mut line = String::new();
        stdin.read_line(&mut line)
            .context("Failed to read confirmation from stdin")?;

        let response = line.trim().to_lowercase();
        if response != "y" && response != "yes" {
            eprintln!("Rollback cancelled.");
            return Ok(());
        }
    }

    backup::restore_from_backup(&manifest)?;

    println!("Restore complete.");
    println!("Running stipe doctor to verify restored state...");

    // Run doctor after restore and propagate failure so the operator knows if the
    // restored state is unhealthy.
    let status = std::process::Command::new("stipe")
        .arg("doctor")
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("Doctor: all checks passed.");
            Ok(())
        }
        Ok(s) => Err(anyhow::anyhow!(
            "Rollback complete but 'stipe doctor' reported issues (exit {s}). \
             Run 'stipe doctor' for details."
        )),
        Err(e) => Err(anyhow::anyhow!(
            "Rollback complete but could not run 'stipe doctor': {e}"
        )),
    }
}

fn list_backups() -> Result<()> {
    let backups = backup::list_backups()?;
    if backups.is_empty() {
        println!("No backups available.");
        return Ok(());
    }
    println!("Available backups (newest first):");
    for (ts, dir) in &backups {
        match backup::load_manifest(dir) {
            Ok(manifest) => println!(
                "  {} — stipe v{}, {} binaries, {} config files",
                ts,
                manifest.stipe_version,
                manifest.binaries.len(),
                manifest.config_files.len()
            ),
            Err(_) => println!("  {} — (manifest unreadable)", ts),
        }
    }
    Ok(())
}
