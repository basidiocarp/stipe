use crate::commands::host_policy::{self, HostMode};
use crate::commands::tool_registry::ToolProbe;

use super::clients;
use super::status::{discover_codex_version, tool_probe};

#[derive(Debug, Clone)]
pub(super) struct EcosystemContext {
    pub(super) target_host: Option<HostMode>,
    pub(super) claude_runtime_relevant: bool,
    pub(super) mycelium_probe: ToolProbe,
    pub(super) hyphae_probe: ToolProbe,
    pub(super) rhizome_probe: ToolProbe,
    pub(super) canopy_probe: ToolProbe,
    pub(super) cortina_probe: ToolProbe,
    pub(super) annulus_probe: ToolProbe,
    pub(super) cap_probe: ToolProbe,
    pub(super) codex_version: Option<String>,
}

impl EcosystemContext {
    pub(super) fn build(client: Option<&str>) -> Self {
        let target_host = client.and_then(host_policy::host_mode_from_client_flag);
        let detected_clients = clients::detect_clients();
        let claude_runtime_relevant = target_host == Some(HostMode::ClaudeCode)
            || (target_host.is_none()
                && host_policy::host_detected_with_clients(
                    HostMode::ClaudeCode,
                    &detected_clients,
                ));

        Self {
            target_host,
            claude_runtime_relevant,
            mycelium_probe: tool_probe("mycelium"),
            hyphae_probe: tool_probe("hyphae"),
            rhizome_probe: tool_probe("rhizome"),
            canopy_probe: tool_probe("canopy"),
            cortina_probe: tool_probe("cortina"),
            annulus_probe: tool_probe("annulus"),
            cap_probe: tool_probe("cap"),
            codex_version: discover_codex_version(),
        }
    }

    pub(super) fn codex_probe(&self) -> ToolProbe {
        self.codex_version
            .as_ref()
            .map_or(ToolProbe::Missing, |version| {
                ToolProbe::Installed(version.clone())
            })
    }

    pub(super) fn probe_for_tool(&self, tool_name: &str) -> Option<&ToolProbe> {
        match tool_name {
            "mycelium" => Some(&self.mycelium_probe),
            "hyphae" => Some(&self.hyphae_probe),
            "rhizome" => Some(&self.rhizome_probe),
            "canopy" => Some(&self.canopy_probe),
            "cortina" => Some(&self.cortina_probe),
            "annulus" => Some(&self.annulus_probe),
            "cap" => Some(&self.cap_probe),
            _ => None,
        }
    }
}
