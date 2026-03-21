use anyhow::{Context, Result};
use colored::Colorize;
use spore::{Tool, discover};
use std::process::Command;

fn claude_is_available() -> bool {
    Command::new("claude")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn mcp_exists(name: &str) -> bool {
    Command::new("claude")
        .args(["mcp", "get", name])
        .output()
        .is_ok_and(|o| o.status.success())
}

fn register_mcp(name: &str, args: &[&str]) -> Result<Option<&'static str>> {
    if mcp_exists(name) {
        return Ok(Some("already registered"));
    }

    let mut cmd = Command::new("claude");
    cmd.arg("mcp")
        .arg("add")
        .arg("--scope")
        .arg("user")
        .arg(name);
    cmd.arg("--");
    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output()?;
    if output.status.success() {
        Ok(Some("registered"))
    } else {
        Ok(None)
    }
}

fn init_hyphae_db() -> Result<()> {
    if let Some(data_dir) = dirs::data_dir() {
        let hyphae_dir = data_dir.join("hyphae");
        let db_path = hyphae_dir.join("hyphae.db");

        if !db_path.exists() {
            std::fs::create_dir_all(&hyphae_dir)
                .with_context(|| format!("Failed to create directory: {}", hyphae_dir.display()))?;

            Command::new("hyphae").arg("stats").output().ok();
        }
    }

    Ok(())
}

fn print_tool_status(name: &str, version: Option<&str>) {
    match version {
        Some(v) => {
            println!("  {:<12}v{:<10}{}", name.bold(), v, "✓ installed".green());
        }
        None => {
            println!(
                "  {:<12}{:<12}{}",
                name.bold(),
                "—",
                "✗ not installed".red()
            );
        }
    }
}

pub fn run(_client: Option<&str>) -> Result<()> {
    println!();
    println!("{}", "Basidiocarp Ecosystem Configuration".bold());
    println!("{}", "─".repeat(75));
    println!();

    println!("{}", "Discovering installed tools...".bold());
    println!();

    let mycelium_info = discover(Tool::Mycelium);
    let hyphae_info = discover(Tool::Hyphae);
    let rhizome_info = discover(Tool::Rhizome);

    print_tool_status(
        "mycelium",
        mycelium_info.as_ref().map(|i| i.version.as_str()),
    );
    print_tool_status("hyphae", hyphae_info.as_ref().map(|i| i.version.as_str()));
    print_tool_status("rhizome", rhizome_info.as_ref().map(|i| i.version.as_str()));

    println!();

    if claude_is_available() {
        println!("{}", "Configuring Claude Code...".bold());
        println!();

        let mut configured = Vec::new();

        if hyphae_info.is_some() {
            match register_mcp("hyphae", &["hyphae", "serve"]) {
                Ok(Some(status)) => {
                    let msg = if status == "already registered" {
                        "hyphae MCP (already registered)"
                    } else {
                        "hyphae MCP"
                    };
                    configured.push(msg);
                    println!("  {} {}", "✓".green(), msg);
                }
                Ok(None) => {
                    println!("  {} Failed to register hyphae MCP", "!".yellow());
                }
                Err(e) => {
                    println!("  {} hyphae MCP registration error: {}", "!".yellow(), e);
                }
            }
        }

        if let Ok(()) = init_hyphae_db() {
            configured.push("hyphae database initialized");
            println!("  {} hyphae database initialized", "✓".green());
        }

        if rhizome_info.is_some() {
            match register_mcp("rhizome", &["rhizome", "serve", "--expanded"]) {
                Ok(Some(status)) => {
                    let msg = if status == "already registered" {
                        "rhizome MCP (already registered)"
                    } else {
                        "rhizome MCP"
                    };
                    configured.push(msg);
                    println!("  {} {}", "✓".green(), msg);
                }
                Ok(None) => {
                    println!("  {} Failed to register rhizome MCP", "!".yellow());
                }
                Err(e) => {
                    println!("  {} rhizome MCP registration error: {}", "!".yellow(), e);
                }
            }
        }

        println!();
    } else {
        println!(
            "  {} {} not found in PATH — skipping Claude Code configuration.",
            "!".yellow(),
            "claude".bold()
        );
        println!();
    }

    let mut missing: Vec<(&str, &str)> = Vec::new();

    if hyphae_info.is_none() {
        missing.push((
            "hyphae",
            "cargo install --git https://github.com/basidiocarp/hyphae hyphae-cli --no-default-features",
        ));
    }
    if rhizome_info.is_none() {
        missing.push((
            "rhizome",
            "cargo install --git https://github.com/basidiocarp/rhizome rhizome-cli",
        ));
    }

    if !missing.is_empty() {
        println!("{}", "Missing tools:".bold());
        for (name, cmd) in &missing {
            println!("  {:<10}{} {}", name, "→".dimmed(), cmd.dimmed());
        }
        println!();
    }

    println!();
    println!("{}", "Configuration complete.".green());
    println!();

    Ok(())
}
