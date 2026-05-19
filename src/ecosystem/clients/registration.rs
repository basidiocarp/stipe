use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::{Map, Value, json};
use spore::atomic_write_bytes;
use spore::editors::{Editor, McpServer as SporeMcpServer, register_mcp_servers};
use spore::logging::{SpanContext, subprocess_span, tool_span};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::commands::host_policy::{self, HostConfigScope};
use crate::commands::install::release::run_command_with_timeout;

use super::{McpClient, ServerConfig};

const MCP_REGISTER_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) fn register_servers(
    client: McpClient,
    servers: &[ServerConfig],
    scope: HostConfigScope,
    verbose: u8,
) -> Result<()> {
    if let Some(editor) = client.shared_editor() {
        return register_shared_editor(client, editor, servers, scope, verbose);
    }

    match client {
        McpClient::ClaudeCode => register_claude_code(servers, scope, verbose),
        McpClient::Continue => register_continue(servers, verbose),
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
) -> Result<()> {
    let scope_name = host_policy::scope_name(scope);
    let mut failures = Vec::new();
    for server in servers {
        let span_context = registration_span_context(&server.name);
        let _tool_span = tool_span("register_claude_code_server", &span_context).entered();
        let mut cmd = Command::new("claude");
        cmd.arg("mcp")
            .arg("add")
            .arg("--scope")
            .arg(scope_name)
            .arg(&server.name)
            .arg("--");
        cmd.arg(&server.command);
        for arg in &server.args {
            cmd.arg(arg);
        }

        if verbose > 0 {
            eprintln!(
                "  Running: claude mcp add --scope {} {} -- {} {}",
                scope_name,
                server.name,
                server.command,
                server.args.join(" ")
            );
        }

        let _subprocess_span = subprocess_span("claude mcp add", &span_context).entered();
        let output = run_command_with_timeout(&mut cmd, MCP_REGISTER_TIMEOUT)
            .with_context(|| format!("failed to run `claude mcp add` for {}", server.name))?;
        if !output.status.success() {
            failures.push(format!(
                "{}: {}",
                server.name,
                describe_command_failure(&output)
            ));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Claude Code MCP registration failed: {}",
            failures.join(" | ")
        ))
    }
}

fn register_shared_editor(
    client: McpClient,
    editor: Editor,
    servers: &[ServerConfig],
    scope: HostConfigScope,
    verbose: u8,
) -> Result<()> {
    if client == McpClient::CodexCli {
        let config_path =
            host_policy::codex_notify_config_path(scope).context("no project Codex config path")?;
        return register_codex_toml_at_path(&config_path, servers, verbose);
    }

    if client == McpClient::ClaudeCode {
        return register_claude_code(servers, scope, verbose);
    }

    let config_path = editor
        .descriptor()
        .map(|descriptor| descriptor.config_path)
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;

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

    register_mcp_servers(editor, &spore_servers).map_err(|err| anyhow::anyhow!(err.to_string()))?;

    if verbose > 0 {
        eprintln!(
            "  Wrote {} server(s) to {}",
            servers.len(),
            config_path.display()
        );
    }

    Ok(())
}

fn register_codex_toml_at_path(
    config_path: &Path,
    servers: &[ServerConfig],
    verbose: u8,
) -> Result<()> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let existing_content = if config_path.exists() {
        fs::read_to_string(config_path)?
    } else {
        String::new()
    };

    let mut root: toml::Value = if existing_content.trim().is_empty() {
        toml::Value::Table(toml::map::Map::new())
    } else {
        toml::from_str(&existing_content)?
    };

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

    let new_content = toml::to_string_pretty(&root)?;
    if new_content == existing_content {
        return Ok(());
    }

    if !existing_content.is_empty() {
        let backup = config_path.with_extension("toml.bak");
        fs::copy(config_path, &backup)?;
    }

    atomic_write_bytes(config_path, new_content.as_bytes())
        .with_context(|| format!("write Codex config: {}", config_path.display()))?;

    if verbose > 0 {
        eprintln!(
            "  Wrote {} server(s) to {}",
            servers.len(),
            config_path.display()
        );
    }

    Ok(())
}

fn register_continue(servers: &[ServerConfig], verbose: u8) -> Result<()> {
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
        match serde_json::from_str(&content) {
            Ok(value) => value,
            Err(err) => {
                eprintln!(
                    "  Warning: {} is not valid JSON ({}); treating as empty config. \
                     Original backed up to {}.",
                    config_path.display(),
                    err,
                    backup.display()
                );
                json!({})
            }
        }
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
    atomic_write_bytes(&config_path, json_str.as_bytes())
        .with_context(|| format!("write Continue config: {}", config_path.display()))?;

    if verbose > 0 {
        eprintln!(
            "  Wrote {} server(s) to {}",
            servers.len(),
            config_path.display()
        );
    }

    Ok(())
}

fn registration_span_context(tool: &str) -> SpanContext {
    let context = SpanContext::for_app("stipe").with_tool(tool);
    match host_policy::project_root().or_else(|| std::env::current_dir().ok()) {
        Some(path) => context.with_workspace_root(path.display().to_string()),
        None => context,
    }
}

fn describe_command_failure(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let mut details = vec![format!("exit status {}", output.status)];
    if !stdout.is_empty() {
        details.push(format!("stdout: {stdout}"));
    }
    if !stderr.is_empty() {
        details.push(format!("stderr: {stderr}"));
    }
    details.join("; ")
}
