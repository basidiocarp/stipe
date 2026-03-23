use crate::ecosystem;
use anyhow::{Result, anyhow};
use colored::Colorize;
use spore::{Tool, discover};

use crate::ecosystem::clients::{self, McpClient};

#[derive(Debug, Clone, PartialEq, Eq)]
struct InitSnapshot {
    target_client: Option<String>,
    detected_clients: Vec<String>,
    hyphae_installed: bool,
    rhizome_installed: bool,
    hyphae_db_exists: bool,
}

fn build_snapshot(client: Option<&str>) -> Result<InitSnapshot> {
    if let Some(target) = client
        && McpClient::from_flag(target).is_none()
    {
        return Err(anyhow!(
            "Unknown client '{}'. Known: claude-code, cursor, windsurf, cline, continue, claude-desktop",
            target
        ));
    }

    let detected_clients = clients::detect_clients()
        .into_iter()
        .filter(|client| *client != McpClient::ClaudeCode)
        .map(|client| client.name().to_string())
        .collect();

    let hyphae_installed = discover(Tool::Hyphae).is_some();
    let rhizome_installed = discover(Tool::Rhizome).is_some();
    let hyphae_db_exists = dirs::data_dir()
        .map(|dir| dir.join("hyphae").join("hyphae.db"))
        .is_some_and(|db_path| db_path.exists());

    Ok(InitSnapshot {
        target_client: client.map(ToOwned::to_owned),
        detected_clients,
        hyphae_installed,
        rhizome_installed,
        hyphae_db_exists,
    })
}

fn render_preview(snapshot: &InitSnapshot) -> Vec<String> {
    let mut lines = Vec::new();

    if let Some(client) = &snapshot.target_client {
        lines.push(format!("Target client: {client}"));
    } else if snapshot.detected_clients.is_empty() {
        lines.push("No supported MCP clients were detected.".to_string());
    } else {
        lines.push(format!(
            "Detected MCP clients: {}",
            snapshot.detected_clients.join(", ")
        ));
    }

    if snapshot.hyphae_installed {
        lines.push("Would register the hyphae MCP server.".to_string());
    } else {
        lines.push(
            "Would skip hyphae MCP registration because hyphae is not installed.".to_string(),
        );
    }

    if snapshot.rhizome_installed {
        lines.push("Would register the rhizome MCP server.".to_string());
    } else {
        lines.push(
            "Would skip rhizome MCP registration because rhizome is not installed.".to_string(),
        );
    }

    if snapshot.hyphae_db_exists {
        lines.push("Hyphae database already exists.".to_string());
    } else {
        lines.push("Would initialize the Hyphae database.".to_string());
    }

    lines.push("Would patch CLAUDE.md with ecosystem instructions.".to_string());
    lines
}

fn print_preview(snapshot: &InitSnapshot) {
    println!("{}", "Dry run: no changes will be made.".yellow());
    println!();

    for line in render_preview(snapshot) {
        println!("  {line}");
    }

    println!();
}

pub fn run(client: Option<&str>, dry_run: bool) -> Result<()> {
    if dry_run {
        let snapshot = build_snapshot(client)?;
        print_preview(&snapshot);
        return Ok(());
    }

    ecosystem::run_ecosystem(client, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_preview_mentions_target_client_and_actions() {
        let snapshot = InitSnapshot {
            target_client: Some("cursor".to_string()),
            detected_clients: vec!["Cursor".to_string(), "Continue".to_string()],
            hyphae_installed: true,
            rhizome_installed: false,
            hyphae_db_exists: false,
        };

        let lines = render_preview(&snapshot);
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Target client: cursor"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("register the hyphae MCP server"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("skip rhizome MCP registration"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("initialize the Hyphae database"))
        );
    }

    #[test]
    fn test_render_preview_lists_detected_clients_when_unfiltered() {
        let snapshot = InitSnapshot {
            target_client: None,
            detected_clients: vec!["Cursor".to_string(), "Continue".to_string()],
            hyphae_installed: false,
            rhizome_installed: false,
            hyphae_db_exists: true,
        };

        let lines = render_preview(&snapshot);
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Detected MCP clients: Cursor, Continue"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Hyphae database already exists"))
        );
    }
}
