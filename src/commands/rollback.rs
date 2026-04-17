use crate::backup;
use anyhow::Result;
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

    backup::restore_from_backup(&manifest)?;

    println!("Restore complete.");
    println!("Running stipe doctor to verify restored state...");

    // Run doctor after restore
    let status = std::process::Command::new("stipe")
        .arg("doctor")
        .status();

    match status {
        Ok(s) if s.success() => println!("Doctor: all checks passed."),
        Ok(_) => eprintln!("Doctor reported issues. Check 'stipe doctor' for details."),
        Err(e) => eprintln!("Could not run doctor: {}", e),
    }

    Ok(())
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
