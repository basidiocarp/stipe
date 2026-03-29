use anyhow::{Context, Result, anyhow};
use colored::Colorize;
use reqwest::blocking::Client;
use std::process::Command;

use super::install;
use super::tool_registry;

fn get_installed_version(tool: &str) -> Result<String> {
    let output = Command::new(tool)
        .arg("--version")
        .output()
        .with_context(|| format!("Failed to get version for {tool}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "Failed to get version for {tool}: {}",
            stderr.trim()
        ));
    }

    let version_output = String::from_utf8_lossy(&output.stdout);
    let version = version_output
        .split_whitespace()
        .last()
        .ok_or_else(|| anyhow!("Empty version output from {tool}"))?;

    Ok(version.to_string())
}

fn fetch_latest_version(tool: &str, client: &Client) -> Result<String> {
    let repo = tool_registry::find(tool).map_or(tool, |spec| spec.release_repo);
    let url = format!("https://api.github.com/repos/basidiocarp/{repo}/releases/latest");
    let data = crate::commands::github::get_github_json(
        client,
        &url,
        &format!("latest release for {repo}"),
    )?;
    let version = data
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Could not parse version from GitHub release"))?
        .to_string();

    Ok(version)
}

struct UpdateInfo {
    installed: String,
    latest: String,
    update_available: bool,
    needs_reinstall: bool,
}

fn check_tool_update(tool: &str, client: &Client) -> Result<UpdateInfo> {
    let (installed, needs_reinstall) = if let Some(spec) = tool_registry::find(tool) {
        match tool_registry::probe(spec) {
            tool_registry::ToolProbe::Installed(version) => (version, false),
            tool_registry::ToolProbe::Broken => ("broken".to_string(), true),
            tool_registry::ToolProbe::Missing => {
                return Err(anyhow!("{tool} is not installed"));
            }
        }
    } else {
        (get_installed_version(tool)?, false)
    };
    let latest = fetch_latest_version(tool, client)?;

    let update_available = needs_reinstall || installed != latest;

    Ok(UpdateInfo {
        installed,
        latest,
        update_available,
        needs_reinstall,
    })
}

fn update_tool(tool: &str, client: &reqwest::blocking::Client) -> Result<()> {
    println!("  {} Checking for updates...", "⏳".yellow());

    let update_info = check_tool_update(tool, client)?;

    if !update_info.update_available {
        println!(
            "  {} {} is up to date ({})",
            "✓".green(),
            tool,
            update_info.installed
        );
        return Ok(());
    }

    if update_info.needs_reinstall {
        println!(
            "  {} {} is installed but broken → reinstall {}",
            "↑".cyan(),
            tool,
            update_info.latest
        );
    } else {
        println!(
            "  {} {} {} → {} available",
            "↑".cyan(),
            tool,
            update_info.installed,
            update_info.latest
        );
    }

    println!("  {} Downloading and installing...", "⏳".yellow());

    let prefix = install::install_bin_dir()?;

    super::install::install_tool(tool, &prefix, true, client)?;

    println!(
        "  {} {} updated to {}",
        "✓".green(),
        tool,
        update_info.latest
    );

    Ok(())
}

#[allow(clippy::unnecessary_wraps)]
pub fn run(all: bool, check: bool, tools: &[String]) -> Result<()> {
    println!();
    println!("{}", "Basidiocarp Ecosystem Update".bold());
    println!("{}", "─".repeat(75));
    println!();

    let tools_to_check: Vec<&str> = if all {
        let all_tools = tool_registry::update_all_specs()
            .into_iter()
            .filter_map(|spec| {
                tool_registry::probe(spec)
                    .is_repairable_presence()
                    .then_some(spec.name)
            })
            .collect::<Vec<_>>();

        if all_tools.is_empty() {
            println!("No installed tools found. Run 'stipe install --all' first.");
            println!();
            return Ok(());
        }

        all_tools
    } else if tools.is_empty() {
        println!("Specify tools to update:");
        println!("  {} stipe update mycelium", "→".dimmed());
        println!("  {} stipe update hyphae rhizome canopy", "→".dimmed());
        println!("  {} stipe update --all", "→".dimmed());
        println!();
        println!("Check without installing:");
        println!("  {} stipe update --check --all", "→".dimmed());
        println!();
        return Ok(());
    } else {
        tools.iter().map(String::as_str).collect()
    };

    let client = crate::commands::github::github_client()?;

    for tool in &tools_to_check {
        match check_tool_update(tool, &client) {
            Ok(info) => {
                if check {
                    if info.needs_reinstall {
                        println!(
                            "  {} {} is installed but broken → reinstall {}",
                            "!".yellow(),
                            tool,
                            info.latest
                        );
                    } else if info.update_available {
                        println!(
                            "  {} {} {} → {}",
                            "↑".cyan(),
                            tool,
                            info.installed,
                            info.latest
                        );
                    } else {
                        println!(
                            "  {} {} is up to date ({})",
                            "✓".green(),
                            tool,
                            info.installed
                        );
                    }
                } else if info.update_available {
                    if let Err(e) = update_tool(tool, &client) {
                        eprintln!("  {} Failed to update {}: {}", "!".red(), tool, e);
                    }
                } else {
                    println!(
                        "  {} {} is up to date ({})",
                        "✓".green(),
                        tool,
                        info.installed
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "  {} Failed to check {} for updates: {}",
                    "!".red(),
                    tool,
                    e
                );
            }
        }
    }

    println!();

    Ok(())
}
