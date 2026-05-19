use std::io;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use super::{ALL_CLIENTS, Editor, McpClient};
use crate::commands::install::release::run_command_with_timeout;

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn detect_clients() -> Vec<McpClient> {
    let detected_editors = spore::editors::detect_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.editor)
        .collect::<Vec<_>>();
    collect_detected_clients(
        &detected_editors,
        claude_cli_installed(),
        continue_installed(),
    )
}

pub(super) fn collect_detected_clients(
    detected_editors: &[Editor],
    claude_cli_available: bool,
    continue_detected: bool,
) -> Vec<McpClient> {
    ALL_CLIENTS
        .iter()
        .copied()
        .filter(|client| {
            shared_client_detected(*client, detected_editors)
                || (*client == McpClient::ClaudeCode && claude_cli_available)
                || (*client == McpClient::Continue && continue_detected)
        })
        .collect()
}

fn shared_client_detected(client: McpClient, detected_editors: &[Editor]) -> bool {
    client
        .shared_editor()
        .is_some_and(|editor| detected_editors.contains(&editor))
}

fn claude_cli_installed() -> bool {
    let mut cmd = Command::new("claude");
    cmd.arg("--version");
    match run_command_with_timeout(&mut cmd, PROBE_TIMEOUT) {
        Ok(o) => {
            if o.status.success() {
                true
            } else {
                tracing::debug!("claude --version returned non-zero exit code");
                false
            }
        }
        Err(e) if e.kind() == io::ErrorKind::TimedOut => {
            tracing::debug!("claude --version timed out");
            false
        }
        Err(_) => {
            tracing::debug!("claude --version failed to run");
            false
        }
    }
}

fn continue_installed() -> bool {
    McpClient::Continue
        .config_path()
        .is_some_and(|p| p.exists() || p.parent().is_some_and(Path::exists))
}
