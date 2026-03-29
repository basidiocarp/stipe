use std::fs;
use std::path::Path;
use std::process::Command;

use super::{ALL_CLIENTS, Editor, McpClient};

pub(super) fn detect_clients() -> Vec<McpClient> {
    let detected_editors = spore::editors::detect_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.editor)
        .collect::<Vec<_>>();
    collect_detected_clients(
        &detected_editors,
        claude_cli_installed(),
        cline_installed(),
        continue_installed(),
    )
}

pub(super) fn collect_detected_clients(
    detected_editors: &[Editor],
    claude_cli_available: bool,
    cline_detected: bool,
    continue_detected: bool,
) -> Vec<McpClient> {
    ALL_CLIENTS
        .iter()
        .copied()
        .filter(|client| {
            shared_client_detected(*client, detected_editors)
                || (*client == McpClient::ClaudeCode && claude_cli_available)
                || (*client == McpClient::Cline && cline_detected)
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
    Command::new("claude")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn cline_installed() -> bool {
    vscode_cline_extension_exists() || McpClient::Cline.config_path().is_some_and(|p| p.exists())
}

fn continue_installed() -> bool {
    McpClient::Continue
        .config_path()
        .is_some_and(|p| p.exists() || p.parent().is_some_and(Path::exists))
}

fn vscode_cline_extension_exists() -> bool {
    dirs::home_dir()
        .map(|home| home.join(".vscode").join("extensions"))
        .is_some_and(|ext_dir| {
            ext_dir.exists()
                && fs::read_dir(ext_dir).ok().is_some_and(|entries| {
                    entries.filter_map(Result::ok).any(|entry| {
                        entry
                            .file_name()
                            .to_string_lossy()
                            .starts_with("saoudrizwan.claude-dev")
                    })
                })
        })
}
