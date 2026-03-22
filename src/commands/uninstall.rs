use anyhow::{Result, anyhow};
use colored::Colorize;
use std::fs;

pub fn run(all: bool, tools: &[String]) -> Result<()> {
    let bin_dir = dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not determine home directory"))?
        .join(".local")
        .join("bin");

    if all {
        println!("Removing all ecosystem tools and configuration...");
        println!();

        let all_tools = vec!["mycelium", "hyphae", "rhizome", "cortina", "stipe"];

        for tool in all_tools {
            let binary_path = bin_dir.join(tool);
            if binary_path.exists() {
                fs::remove_file(&binary_path)?;
                println!("  {} {} removed", "✓".green(), tool);
            }
        }

        println!();
        println!(
            "{}",
            "Note: MCP registrations in editor config files must be removed manually.".yellow()
        );
        println!();
    } else if tools.is_empty() {
        println!("Specify tools to remove or use --all");
    } else {
        println!();

        for tool in tools {
            let binary_path = bin_dir.join(tool);
            if binary_path.exists() {
                fs::remove_file(&binary_path)?;
                println!("  {} {} removed", "✓".green(), tool);
            } else {
                println!(
                    "  {} {} not found in {}",
                    "!".yellow(),
                    tool,
                    bin_dir.display()
                );
            }
        }

        println!();
    }
    Ok(())
}
