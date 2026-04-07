use anyhow::{Result, anyhow};

use super::model::{ClaudeSnapshot, CodexSnapshot, InitSnapshot, ToolSnapshot};
use crate::commands::claude_hooks;
use crate::commands::codex_notify;
use crate::commands::host_policy;
use crate::commands::host_policy::HostConfigScope;
use crate::commands::tool_registry::{self, ToolProbe};
use crate::ecosystem::clients::{self, McpClient};

pub(super) fn build_snapshot(client: Option<&str>, scope: HostConfigScope) -> Result<InitSnapshot> {
    let target_client = client.map(ToOwned::to_owned);

    if let Some(target) = client {
        if McpClient::from_flag(target).is_none() {
            return Err(anyhow!(
                "Unknown client '{target}'. Known: claude-code, cursor, windsurf, cline, continue, claude-desktop, codex, gemini, copilot"
            ));
        }
        if let Some(mode) = host_policy::host_mode_from_client_flag(target)
            && !host_policy::host_scope_supported(mode, scope)
        {
            return Err(anyhow!(
                "{} does not support the '{}' scope",
                mode.label(),
                host_policy::scope_name(scope)
            ));
        }
    }

    let detected_clients_raw = clients::detect_clients();
    let detected_hosts = host_policy::supported_host_modes()
        .iter()
        .copied()
        .filter(|mode| host_policy::host_detected_with_clients(*mode, &detected_clients_raw))
        .collect::<Vec<_>>();
    let selected_hosts = client
        .and_then(host_policy::host_mode_from_client_flag)
        .map_or_else(|| detected_hosts.clone(), |mode| vec![mode]);

    let detected_clients = detected_clients_raw
        .into_iter()
        .filter(|client| *client != McpClient::ClaudeCode)
        .map(|client| client.name().to_string())
        .collect();

    let hyphae_installed = tool_registry::find("hyphae")
        .is_some_and(|spec| matches!(tool_registry::probe(spec), ToolProbe::Installed(_)));
    let hyphae_broken = tool_registry::find("hyphae")
        .is_some_and(|spec| matches!(tool_registry::probe(spec), ToolProbe::Broken));
    let rhizome_installed = tool_registry::find("rhizome")
        .is_some_and(|spec| matches!(tool_registry::probe(spec), ToolProbe::Installed(_)));
    let rhizome_broken = tool_registry::find("rhizome")
        .is_some_and(|spec| matches!(tool_registry::probe(spec), ToolProbe::Broken));
    let cortina_probe = tool_registry::find("cortina").map(tool_registry::probe);
    let cortina_installed = matches!(cortina_probe, Some(ToolProbe::Installed(_)));
    let cortina_broken = matches!(cortina_probe, Some(ToolProbe::Broken));
    let hyphae_db_exists = dirs::data_dir()
        .map(|dir| dir.join("hyphae").join("hyphae.db"))
        .is_some_and(|db_path| db_path.exists());

    Ok(InitSnapshot {
        target_client,
        selected_hosts,
        detected_hosts,
        detected_clients,
        tools: ToolSnapshot {
            hyphae_installed,
            hyphae_broken,
            rhizome_installed,
            rhizome_broken,
            cortina_installed,
            cortina_broken,
            hyphae_db_exists,
        },
        codex: CodexSnapshot {
            notify_configured: codex_notify::codex_notify_configured(),
        },
        claude: ClaudeSnapshot {
            hooks_configured: claude_hooks::claude_hooks_configured(),
        },
    })
}
