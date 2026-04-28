use anyhow::{Context, Result, anyhow};
use std::process::Command;

/// Creates a manual backup of the Hyphae database and binary.
#[allow(clippy::unnecessary_wraps)]
pub fn backup_hyphae() -> Result<()> {
    // Get the current hyphae version
    let hyphae_version = get_hyphae_version().unwrap_or_else(|_| "unknown".to_string());

    // Get a timestamp
    let timestamp = crate::backup::backup_timestamp();

    // Create the hyphae-specific pre-upgrade backup
    if let Some(backup_path) = crate::backup::pre_upgrade_backup_hyphae(&hyphae_version, &timestamp)
    {
        println!("✓ Hyphae backup created at: {}", backup_path.display());
    } else {
        // Backup creation returned None, which means it encountered issues
        // but didn't want to hard-error
        println!("⚠ Hyphae backup could not be fully created. Check logs for details.");
    }
    Ok(())
}

/// Attempts to get the current hyphae version by running `hyphae --version`
fn get_hyphae_version() -> Result<String> {
    let output = Command::new("hyphae")
        .arg("--version")
        .output()
        .context("Failed to get hyphae version")?;

    if !output.status.success() {
        return Err(anyhow!("hyphae --version returned non-zero exit code"));
    }

    let version_output = String::from_utf8_lossy(&output.stdout);
    let version = version_output
        .split_whitespace()
        .last()
        .ok_or_else(|| anyhow!("Empty version output from hyphae"))?
        .to_string();

    Ok(version)
}
