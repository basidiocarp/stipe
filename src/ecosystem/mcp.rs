use anyhow::Result;
use serde_json::Value;
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

pub(super) fn register_mcp(
    name: &str,
    args: &[&str],
    scope: HostConfigScope,
    verbose: u8,
) -> Result<Option<&'static str>> {
    let scope_name = host_policy::scope_name(scope);

    if mcp_exists(name, scope) {
        if verbose > 0 {
            eprintln!("  {name} MCP already registered");
        }
        return Ok(Some("already registered"));
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

    let output = cmd.output()?;
    if output.status.success() {
        Ok(Some("registered"))
    } else {
        Ok(None)
    }
}
