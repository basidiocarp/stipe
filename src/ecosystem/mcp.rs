use anyhow::{Result, anyhow};
use serde_json::Value;
use spore::logging::{SpanContext, subprocess_span, tool_span};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::commands::host_policy::{self, HostConfigScope};

fn current_project_root() -> Option<PathBuf> {
    host_policy::project_root()
}

fn claude_mcp_project_path() -> Option<PathBuf> {
    current_project_root().map(|root| root.join(".mcp.json"))
}

fn load_json(path: &Path) -> Option<Value> {
    let content = fs::read_to_string(path).ok()?;
    if content.trim().is_empty() {
        Some(serde_json::json!({}))
    } else {
        serde_json::from_str(&content).ok()
    }
}

fn path_scoped_mcp_exists(root: &Value, project_root: &Path, name: &str) -> bool {
    let project_key = project_root.to_string_lossy();
    root.get("projects")
        .and_then(Value::as_object)
        .and_then(|projects| projects.get(project_key.as_ref()))
        .and_then(|project| project.get("mcpServers"))
        .and_then(Value::as_object)
        .is_some_and(|servers| servers.contains_key(name))
}

fn mcp_exists(name: &str, scope: HostConfigScope) -> bool {
    match scope {
        HostConfigScope::User => host_policy::host_config_path(host_policy::HostMode::ClaudeCode)
            .as_deref()
            .and_then(load_json)
            .and_then(|root| root.get("mcpServers").and_then(Value::as_object).cloned())
            .is_some_and(|servers| servers.contains_key(name)),
        HostConfigScope::Project => claude_mcp_project_path()
            .as_deref()
            .and_then(load_json)
            .and_then(|root| root.get("mcpServers").and_then(Value::as_object).cloned())
            .is_some_and(|servers| servers.contains_key(name)),
        HostConfigScope::Local => host_policy::host_config_path(host_policy::HostMode::ClaudeCode)
            .as_deref()
            .and_then(load_json)
            .zip(current_project_root())
            .is_some_and(|(root, project_root)| path_scoped_mcp_exists(&root, &project_root, name)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RegistrationStatus {
    Registered,
    AlreadyRegistered,
}

pub(super) fn register_mcp(
    name: &str,
    args: &[&str],
    scope: HostConfigScope,
    verbose: u8,
) -> Result<RegistrationStatus> {
    let scope_name = host_policy::scope_name(scope);
    let span_context = span_context_for_registration(name);
    let _tool_span = tool_span("register_mcp", &span_context).entered();

    if mcp_exists(name, scope) {
        if verbose > 0 {
            eprintln!("  {name} MCP already registered");
        }
        return Ok(RegistrationStatus::AlreadyRegistered);
    }

    let mut cmd = Command::new("claude");
    cmd.arg("mcp")
        .arg("add")
        .arg("--scope")
        .arg(scope_name)
        .arg(name);
    cmd.arg("--");
    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!(
            "  Running: claude mcp add --scope {} {} -- {}",
            scope_name,
            name,
            args.join(" ")
        );
    }

    let _subprocess_span = subprocess_span("claude mcp add", &span_context).entered();
    let output = cmd.output()?;
    if output.status.success() {
        Ok(RegistrationStatus::Registered)
    } else {
        Err(anyhow!(
            "claude mcp add failed for {name} ({scope_name} scope): {}",
            format_command_output(&output)
        ))
    }
}

fn span_context_for_registration(name: &str) -> SpanContext {
    let context = SpanContext::for_app("stipe").with_tool(name);
    match current_project_root().or_else(|| std::env::current_dir().ok()) {
        Some(path) => context.with_workspace_root(path.display().to_string()),
        None => context,
    }
}

fn format_command_output(output: &std::process::Output) -> String {
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
