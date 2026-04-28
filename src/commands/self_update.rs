use anyhow::{Context, Result, anyhow};
use clap::Subcommand;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::path::{Path, PathBuf};

use super::github;
use super::install::release::{
    download_binary, download_sha256sums, extract_tarball, fetch_latest_release,
    find_checksum_asset, find_matching_asset, normalize_version, platform_key,
    verify_asset_checksum, verify_binary,
};

#[derive(Debug, Clone, Copy, Subcommand)]
pub enum SelfCommand {
    /// Update the stipe binary in place
    Update {
        /// Check for a newer stipe release without replacing the current binary
        #[arg(long)]
        check: bool,
    },
}

fn replacement_path(current_exe: &Path) -> Result<PathBuf> {
    let file_name = current_exe
        .file_name()
        .ok_or_else(|| anyhow!("Current executable path has no file name"))?;
    let parent = current_exe
        .parent()
        .ok_or_else(|| anyhow!("Current executable path has no parent directory"))?;
    Ok(parent.join(format!("{}.new", file_name.to_string_lossy())))
}

fn install_replacement(current_exe: &Path, extracted_path: &Path) -> Result<()> {
    let replacement = replacement_path(current_exe)?;
    if replacement.exists() {
        fs::remove_file(&replacement)
            .with_context(|| format!("Failed to remove {}", replacement.display()))?;
    }

    fs::copy(extracted_path, &replacement).with_context(|| {
        format!(
            "Failed to copy {} to {}",
            extracted_path.display(),
            replacement.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&replacement, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("Failed to make {} executable", replacement.display()))?;
    }

    fs::rename(&replacement, current_exe).with_context(|| {
        format!(
            "Failed to replace running stipe binary at {}",
            current_exe.display()
        )
    })?;

    Ok(())
}

pub fn run(command: SelfCommand) -> Result<()> {
    match command {
        SelfCommand::Update { check } => run_update(check),
    }
}

fn run_update(check: bool) -> Result<()> {
    println!();
    println!("{}", "Stipe Self Update".bold());
    println!("{}", "─".repeat(75));
    println!();

    let current_version = env!("CARGO_PKG_VERSION");
    let current_exe =
        std::env::current_exe().context("Failed to determine current stipe executable path")?;
    let client = github::github_client();
    let release = fetch_latest_release("stipe", &client)?;

    if normalize_version(&release.version) == current_version {
        println!("  {} stipe is up to date ({current_version})", "✓".green());
        println!();
        return Ok(());
    }

    println!(
        "  {} stipe {} → {} available",
        "↑".cyan(),
        current_version,
        release.version
    );

    if check {
        println!();
        return Ok(());
    }

    let asset = find_matching_asset(&release, platform_key())?;
    println!("  {} Downloading {}...", "⏳".yellow(), asset.name);

    let progress = ProgressBar::new(0);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("{bar:30.cyan/blue} {bytes}/{total_bytes}")
            .expect("valid progress bar template")
            .progress_chars("=>-"),
    );
    let data = download_binary(asset, &progress, &client)?;

    // Verify SHA-256 before extraction.
    let sha256sums = find_checksum_asset(&release)
        .map(|cs_asset| download_sha256sums(cs_asset, &client))
        .transpose()?;
    if let Some(ref sums) = sha256sums {
        verify_asset_checksum(&data, &asset.name, sums)
            .with_context(|| format!("Checksum verification failed for {}", asset.name))?;
    } else {
        // TODO: upgrade to a hard failure once all releases publish SHA256SUMS.
        tracing::warn!(
            "no SHA256SUMS asset found for stipe {}; skipping checksum verification",
            release.version
        );
    }

    println!("  {} Extracting...", "⏳".yellow());
    let temp_guard = tempfile::TempDir::new()
        .context("Failed to create temporary directory for extraction")?;
    let extracted_path = extract_tarball(&data, temp_guard.path())?;

    println!("  {} Verifying...", "⏳".yellow());
    let verified_version = verify_binary(&extracted_path)?;

    // Require the extracted binary version to match the expected release tag.
    let expected_normalized = normalize_version(&release.version);
    if normalize_version(&verified_version) != expected_normalized {
        return Err(anyhow!(
            "Version mismatch after extraction: expected {}, binary reports {}",
            release.version,
            verified_version
        ));
    }

    println!("  {} Replacing {}...", "⏳".yellow(), current_exe.display());
    install_replacement(&current_exe, &extracted_path)?;

    println!(
        "  {} stipe updated: {} → {}",
        "✓".green(),
        current_version,
        verified_version
    );
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_version_trims_release_prefix() {
        assert_eq!(normalize_version("v0.5.0"), "0.5.0");
        assert_eq!(normalize_version("0.5.0"), "0.5.0");
    }

    #[test]
    fn replacement_path_uses_neighbor_file() {
        let path = Path::new("/tmp/stipe");
        assert_eq!(
            replacement_path(path).expect("replacement path"),
            PathBuf::from("/tmp/stipe.new")
        );
    }
}
