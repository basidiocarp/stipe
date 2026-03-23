use anyhow::{Context, Result, anyhow};
use colored::Colorize;
use spore::{Tool, discover};
use std::process::Command;

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

fn fetch_latest_version(tool: &str) -> Result<String> {
    let url = format!("https://api.github.com/repos/basidiocarp/{tool}/releases/latest");

    let client = reqwest::blocking::Client::new();
    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .with_context(|| format!("Failed to fetch release info for {tool}"))?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "GitHub API error for {}: {}",
            tool,
            response.status()
        ));
    }

    let data: serde_json::Value = response.json()?;
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
}

fn check_tool_update(tool: &str) -> Result<UpdateInfo> {
    let installed = get_installed_version(tool)?;
    let latest = fetch_latest_version(tool)?;

    let update_available = installed != latest;

    Ok(UpdateInfo {
        installed,
        latest,
        update_available,
    })
}

fn update_tool(tool: &str, client: &reqwest::blocking::Client) -> Result<()> {
    println!("  {} Checking for updates...", "⏳".yellow());

    let update_info = check_tool_update(tool)?;

    if !update_info.update_available {
        println!(
            "  {} {} is up to date ({})",
            "✓".green(),
            tool,
            update_info.installed
        );
        return Ok(());
    }

    println!(
        "  {} {} {} → {} available",
        "↑".cyan(),
        tool,
        update_info.installed,
        update_info.latest
    );

    println!("  {} Downloading and installing...", "⏳".yellow());

    let prefix = dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not determine home directory"))?
        .join(".local")
        .join("bin");

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
        let mut all_tools = vec![];

        if discover(Tool::Mycelium).is_some() {
            all_tools.push("mycelium");
        }
        if discover(Tool::Hyphae).is_some() {
            all_tools.push("hyphae");
        }
        if discover(Tool::Rhizome).is_some() {
            all_tools.push("rhizome");
        }

        if all_tools.is_empty() {
            println!("No installed tools found. Run 'stipe install --all' first.");
            println!();
            return Ok(());
        }

        all_tools
    } else if tools.is_empty() {
        println!("Specify tools to update:");
        println!("  {} stipe update mycelium", "→".dimmed());
        println!("  {} stipe update hyphae rhizome", "→".dimmed());
        println!("  {} stipe update --all", "→".dimmed());
        println!();
        println!("Check without installing:");
        println!("  {} stipe update --check --all", "→".dimmed());
        println!();
        return Ok(());
    } else {
        tools.iter().map(String::as_str).collect()
    };

    let client = reqwest::blocking::Client::new();

    for tool in &tools_to_check {
        match check_tool_update(tool) {
            Ok(info) => {
                if check {
                    if info.update_available {
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
