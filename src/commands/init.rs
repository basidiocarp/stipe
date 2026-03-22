use anyhow::Result;
use colored::Colorize;
use dialoguer::{MultiSelect, theme::ColorfulTheme};
use spore::{Tool, discover, editors};

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

fn register_mcp_for_editor(
    editor: spore::editors::Editor,
    hyphae_info: &Option<spore::ToolInfo>,
    rhizome_info: &Option<spore::ToolInfo>,
) {
    if hyphae_info.is_some() {
        match which::which("hyphae") {
            Ok(binary_path) => {
                match editors::register_mcp_server(
                    editor,
                    "hyphae",
                    binary_path.to_str().unwrap_or("hyphae"),
                    &["serve"],
                ) {
                    Ok(()) => {
                        println!("  {} {}: hyphae MCP registered", "✓".green(), editor.name());
                    }
                    Err(e) => {
                        println!(
                            "  {} {}: failed to register hyphae MCP — {}",
                            "!".yellow(),
                            editor.name(),
                            e
                        );
                    }
                }
            }
            Err(_) => {
                println!(
                    "  {} {}: hyphae binary not found in PATH",
                    "!".yellow(),
                    editor.name()
                );
            }
        }
    }

    if rhizome_info.is_some() {
        match which::which("rhizome") {
            Ok(binary_path) => {
                match editors::register_mcp_server(
                    editor,
                    "rhizome",
                    binary_path.to_str().unwrap_or("rhizome"),
                    &["serve", "--expanded"],
                ) {
                    Ok(()) => {
                        println!(
                            "  {} {}: rhizome MCP registered",
                            "✓".green(),
                            editor.name()
                        );
                    }
                    Err(e) => {
                        println!(
                            "  {} {}: failed to register rhizome MCP — {}",
                            "!".yellow(),
                            editor.name(),
                            e
                        );
                    }
                }
            }
            Err(_) => {
                println!(
                    "  {} {}: rhizome binary not found in PATH",
                    "!".yellow(),
                    editor.name()
                );
            }
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

    let detected_editors = editors::detect();

    if detected_editors.is_empty() {
        println!(
            "  {} No supported editors found. Supported editors: Claude Code, VS Code, Cursor, Zed, Windsurf",
            "!".yellow()
        );
        println!();
        println!("{}", "Configuration complete.".green());
        println!();
        return Ok(());
    }

    println!("{}", "Configuring editors...".bold());
    println!();

    let theme = ColorfulTheme::default();
    let editor_items: Vec<(String, bool)> = detected_editors
        .iter()
        .map(|editor| {
            let name = format!("{:<15} — MCP server host", editor.name());
            (name, true)
        })
        .collect();

    let selections = if detected_editors.len() > 1 {
        MultiSelect::with_theme(&theme)
            .items_checked(&editor_items)
            .interact()?
    } else {
        vec![0]
    };

    if selections.is_empty() {
        println!();
        println!("{}", "No editors selected. Exiting.".yellow());
        println!();
        return Ok(());
    }

    println!();

    for &idx in &selections {
        let editor = detected_editors[idx];
        register_mcp_for_editor(editor, &hyphae_info, &rhizome_info);
    }

    println!();

    let mut missing: Vec<(&str, &str)> = Vec::new();

    if hyphae_info.is_none() {
        missing.push(("hyphae", "cargo install --path hyphae/crates/hyphae-cli"));
    }
    if rhizome_info.is_none() {
        missing.push(("rhizome", "cargo install --path rhizome/crates/rhizome-cli"));
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
