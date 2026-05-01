use anyhow::{Context, Result, anyhow};
use std::process::Command;

/// Creates a manual backup of the Hyphae database and binary.
pub fn backup_hyphae() -> Result<()> {
    // Get the current hyphae version
    let hyphae_version = get_hyphae_version().unwrap_or_else(|_| "unknown".to_string());

    // Get a timestamp
    let timestamp = crate::backup::backup_timestamp();

    // Create the hyphae-specific pre-upgrade backup
    let outcome = crate::backup::pre_upgrade_backup_hyphae(&hyphae_version, &timestamp);

    if let Some(backup_path) = &outcome.backup_dir {
        println!("✓ Hyphae backup created at: {}", backup_path.display());
    } else {
        println!("✗ Hyphae backup directory could not be created.");
        for err in &outcome.failed {
            println!("  Failed: {err}");
        }
        return Err(anyhow!(
            "Hyphae backup failed: directory creation unsuccessful"
        ));
    }

    // Report what was backed up
    if !outcome.binaries_copied.is_empty() {
        println!("  ✓ Binary backup successful");
    } else if outcome.failed.iter().any(|e| e.contains("binary")) {
        println!("  ✗ Binary backup failed");
        for err in outcome.failed.iter().filter(|e| e.contains("binary")) {
            println!("    {err}");
        }
    }

    if !outcome.databases_copied.is_empty() {
        println!("  ✓ Database backup successful");
    } else if outcome.failed.iter().any(|e| e.contains("database")) {
        println!("  ✗ Database backup failed");
        for err in outcome.failed.iter().filter(|e| e.contains("database")) {
            println!("    {err}");
        }
    }

    // Report what was missing
    if !outcome.missing.is_empty() {
        println!("  ⚠ Missing files:");
        for miss in &outcome.missing {
            println!("    {miss}");
        }
    }

    // Fail if critical components failed to backup
    if !outcome.failed.is_empty() {
        return Err(anyhow!(
            "Hyphae backup incomplete: {} file(s) failed to copy",
            outcome.failed.len()
        ));
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
