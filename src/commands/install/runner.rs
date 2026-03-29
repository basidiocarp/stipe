use anyhow::{Context, Result, anyhow};
use colored::Colorize;
use dialoguer::{MultiSelect, theme::ColorfulTheme};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::blocking::Client;
use std::fs;
use std::path::{Path, PathBuf};

use crate::commands::bin_paths;
use crate::commands::github;
use crate::commands::install::release::{
    download_binary, extract_tarball, fetch_latest_release, find_matching_asset, platform_key,
    verify_binary,
};
use crate::commands::install::selection::{print_install_preview, resolve_requested_tools};
use crate::commands::tool_registry::{self, InstallProfile};

pub(crate) fn install_tool(tool: &str, prefix: &Path, force: bool, client: &Client) -> Result<()> {
    println!("  {} Fetching release information...", "⏳".yellow());

    let release = fetch_latest_release(tool, client)?;
    let platform_key = platform_key();
    let asset = find_matching_asset(&release, platform_key)?;

    println!("  {} Found {}: {}", "✓".green(), tool, release.version);

    let install_path = prefix.join(tool);

    if install_path.exists() && !force {
        println!(
            "  {} {} already installed at {}. Use --force to replace.",
            "⊘".yellow(),
            tool,
            install_path.display()
        );
        return Ok(());
    }

    println!("  {} Downloading {}...", "⏳".yellow(), asset.name);
    let progress = ProgressBar::new(0);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("{bar:30.cyan/blue} {bytes}/{total_bytes}")
            .unwrap()
            .progress_chars("=>-"),
    );
    let data = download_binary(asset, &progress, client)?;

    println!("  {} Extracting...", "⏳".yellow());
    let temp_dir = std::env::temp_dir().join(format!("stipe-{tool}"));
    let extracted_path = extract_tarball(&data, &temp_dir)?;

    println!("  {} Verifying...", "⏳".yellow());
    let version = verify_binary(&extracted_path)?;

    fs::copy(&extracted_path, &install_path).with_context(|| {
        format!(
            "Failed to copy {} to {}",
            extracted_path.display(),
            install_path.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&install_path, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("Failed to make {} executable", install_path.display()))?;
    }

    println!(
        "  {} {} installed: {} → {}",
        "✓".green(),
        tool,
        version,
        install_path.display()
    );

    Ok(())
}

pub(crate) fn install_bin_dir() -> Result<PathBuf> {
    bin_paths::local_bin_dir().ok_or_else(|| anyhow!("Could not determine local bin directory"))
}

pub(crate) fn run(
    all: bool,
    profile: Option<InstallProfile>,
    dry_run: bool,
    tools: &[String],
) -> Result<()> {
    let prefix = install_bin_dir()?;

    crate::banner::print_banner();
    println!("{}", "Basidiocarp Ecosystem Installer".bold());
    println!("{}", "─".repeat(75));
    println!();

    let tools_to_install = resolve_requested_tools(all, profile, tools);

    if dry_run {
        if let Some(profile) = profile {
            println!("Selected mode: {}", profile.mode_label().bold());
            println!();
        }

        match tools_to_install {
            Some(ref requested) => {
                let label = if all {
                    "all".to_string()
                } else if let Some(profile) = profile {
                    profile.mode_label().to_string()
                } else {
                    "explicit tools".to_string()
                };
                print_install_preview(&prefix, requested, &label);
            }
            None => {
                print_install_preview(&prefix, &[], "interactive selection");
            }
        }

        println!();
        return Ok(());
    }

    let tools_to_install: Vec<String> = if let Some(tools) = tools_to_install {
        tools
    } else {
        let theme = ColorfulTheme::default();
        let installable_specs = tool_registry::installable_specs();
        println!(
            "{}",
            "Select tools to install (all selected by default):".bold()
        );
        println!();

        let tool_items: Vec<(String, bool)> = installable_specs
            .iter()
            .map(|spec| (format!("{:<15} — {}", spec.name, spec.description), true))
            .collect();

        let selections = MultiSelect::with_theme(&theme)
            .items_checked(&tool_items)
            .interact()?;

        if selections.is_empty() {
            println!();
            println!("{}", "No tools selected. Exiting.".yellow());
            println!();
            return Ok(());
        }

        selections
            .iter()
            .map(|&idx| installable_specs[idx].name.to_string())
            .collect()
    };

    println!();

    let client = github::github_client()?;

    for tool in &tools_to_install {
        match install_tool(tool, &prefix, false, &client) {
            Ok(()) => {}
            Err(error) => {
                eprintln!("  {} Failed to install {}: {}", "!".red(), tool, error);
            }
        }
    }

    println!();
    println!(
        "{}",
        "Installation complete. Run 'stipe init' to configure.".green()
    );
    println!();

    Ok(())
}
