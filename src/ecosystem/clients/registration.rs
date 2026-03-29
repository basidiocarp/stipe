use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::{Map, Value, json};
use spore::editors::{self, Editor, McpServer as SporeMcpServer};
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::commands::host_policy::{self, HostConfigScope};

use super::{McpClient, ServerConfig};

pub(super) fn register_servers(
    client: McpClient,
    servers: &[ServerConfig],
    scope: HostConfigScope,
    verbose: u8,
) -> Result<bool> {
    if let Some(editor) = client.shared_editor() {
        return register_shared_editor(client, editor, servers, scope, verbose);
    }

    match client {
        McpClient::ClaudeCode => register_claude_code(servers, scope, verbose),
        McpClient::Continue => register_continue(servers, verbose),
        McpClient::Cline => {
            print_cline_snippet(servers);
            Ok(true)
        }
        McpClient::Cursor
        | McpClient::Windsurf
        | McpClient::ClaudeDesktop
        | McpClient::CodexCli
        | McpClient::GeminiCli
        | McpClient::CopilotCli => unreachable!("shared editors handled above"),
    }
}

pub(super) fn print_generic_config(servers: &[ServerConfig]) {
    println!("{}", "Generic MCP Configuration".bold());
    println!("{}", "─".repeat(60));
    println!();
    println!("Add the following to your MCP client's config:\n");

    let mut mcp_servers = Map::new();
    for server in servers {
        mcp_servers.insert(
            server.name.clone(),
            json!({
                "command": server.command,
                "args": server.args,
            }),
        );
    }

    let config = json!({ "mcpServers": mcp_servers });
    println!(
        "{}",
        serde_json::to_string_pretty(&config).unwrap_or_default()
    );
    println!();
    println!(
        "  {}",
        "Paste this into your MCP client's settings file.".dimmed()
    );
}

fn register_claude_code(
    servers: &[ServerConfig],
    scope: HostConfigScope,
    verbose: u8,
) -> Result<bool> {
    let mut all_ok = true;
    for server in servers {
        let mut cmd = Command::new("claude");
        cmd.arg("mcp")
            .arg("add")
            .arg("--scope")
            .arg(match scope {
                HostConfigScope::User => "user",
                HostConfigScope::Project => "project",
                HostConfigScope::Local => "local",
            })
            .arg(&server.name)
            .arg("--");
        cmd.arg(&server.command);
        for arg in &server.args {
            cmd.arg(arg);
        }

        if verbose > 0 {
            eprintln!(
                "  Running: claude mcp add --scope {} {} -- {} {}",
                match scope {
                    HostConfigScope::User => "user",
                    HostConfigScope::Project => "project",
                    HostConfigScope::Local => "local",
                },
                server.name,
                server.command,
                server.args.join(" ")
            );
        }

        let output = cmd.output().context("failed to run `claude mcp add`")?;
        if !output.status.success() {
            all_ok = false;
        }
    }
    Ok(all_ok)
}

fn register_shared_editor(
    client: McpClient,
    editor: Editor,
    servers: &[ServerConfig],
    scope: HostConfigScope,
    verbose: u8,
) -> Result<bool> {
    if client == McpClient::CodexCli && scope == HostConfigScope::Project {
        let config_path =
            host_policy::codex_notify_config_path(scope).context("no project Codex config path")?;
        return register_codex_toml_at_path(&config_path, servers, verbose);
    }

    if client == McpClient::ClaudeCode {
        return register_claude_code(servers, scope, verbose);
    }

    let config_path =
        editors::config_path(editor).map_err(|err| anyhow::anyhow!(err.to_string()))?;

    let arg_slices: Vec<Vec<&str>> = servers
        .iter()
        .map(|server| server.args.iter().map(String::as_str).collect())
        .collect();
    let spore_servers: Vec<SporeMcpServer<'_>> = servers
        .iter()
        .zip(arg_slices.iter())
        .map(|(server, args)| SporeMcpServer {
            name: &server.name,
            command: &server.command,
            args: args.as_slice(),
        })
        .collect();

    editors::register_mcp_servers(editor, &spore_servers)
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;

    if verbose > 0 {
        eprintln!(
            "  Wrote {} server(s) to {}",
            servers.len(),
            config_path.display()
        );
    }

    Ok(true)
}

fn register_codex_toml_at_path(
    config_path: &Path,
    servers: &[ServerConfig],
    verbose: u8,
) -> Result<bool> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut root: toml::Value = if config_path.exists() {
        let content = fs::read_to_string(config_path)?;
        if content.trim().is_empty() {
            toml::Value::Table(toml::map::Map::new())
        } else {
            toml::from_str(&content)?
        }
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    if config_path.exists() {
        let backup = config_path.with_extension("toml.bak");
        fs::copy(config_path, &backup)?;
    }

    let root_table = root
        .as_table_mut()
        .context("Codex config root must be a TOML table")?;
    let server_map = root_table
        .entry("mcp_servers")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .context("Codex mcp_servers must be a TOML table")?;

    for server in servers {
        let mut server_table = toml::map::Map::new();
        server_table.insert(
            "command".to_string(),
            toml::Value::String(server.command.clone()),
        );
        server_table.insert(
            "args".to_string(),
            toml::Value::Array(
                server
                    .args
                    .iter()
                    .map(|arg| toml::Value::String(arg.clone()))
                    .collect(),
            ),
        );
        server_map.insert(server.name.clone(), toml::Value::Table(server_table));
    }

    fs::write(config_path, toml::to_string_pretty(&root)?)?;

    if verbose > 0 {
        eprintln!(
            "  Wrote {} server(s) to {}",
            servers.len(),
            config_path.display()
        );
    }

    Ok(true)
}

fn register_continue(servers: &[ServerConfig], verbose: u8) -> Result<bool> {
    let config_path = McpClient::Continue
        .config_path()
        .context("no Continue config path")?;

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut root: Value = if config_path.exists() {
        let backup = config_path.with_extension("json.bak");
        fs::copy(&config_path, &backup)?;
        if verbose > 0 {
            eprintln!(
                "  Backed up {} → {}",
                config_path.display(),
                backup.display()
            );
        }
        let content = fs::read_to_string(&config_path)?;
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    let obj = root
        .as_object_mut()
        .context("config root is not an object")?;
    let experimental = obj
        .entry("experimental")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("experimental is not an object")?;
    let mcp_array = experimental
        .entry("modelContextProtocolServers")
        .or_insert_with(|| json!([]));
    let arr = mcp_array
        .as_array_mut()
        .context("modelContextProtocolServers is not an array")?;

    for server in servers {
        arr.retain(|entry| entry.get("name").and_then(Value::as_str) != Some(&server.name));
        arr.push(json!({
            "name": server.name,
            "command": server.command,
            "args": server.args,
        }));
    }

    let json_str = serde_json::to_string_pretty(&root)?;
    fs::write(&config_path, json_str)?;

    if verbose > 0 {
        eprintln!(
            "  Wrote {} server(s) to {}",
            servers.len(),
            config_path.display()
        );
    }

    Ok(true)
}

fn print_cline_snippet(servers: &[ServerConfig]) {
    println!();
    println!(
        "  {} Cline uses VS Code settings. Add this to your VS Code settings.json:",
        "→".dimmed()
    );
    println!();

    let mut mcp_servers = Map::new();
    for server in servers {
        mcp_servers.insert(
            server.name.clone(),
            json!({
                "command": server.command,
                "args": server.args,
            }),
        );
    }

    let snippet = json!({ "cline.mcpServers": mcp_servers });
    println!(
        "{}",
        serde_json::to_string_pretty(&snippet).unwrap_or_default()
    );
    println!();
}
