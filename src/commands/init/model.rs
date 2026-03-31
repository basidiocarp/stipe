use crate::commands::host_policy::{self, HostMode};
use crate::commands::repair::RepairAction;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct InitSnapshot {
    pub(super) target_client: Option<String>,
    pub(super) selected_hosts: Vec<HostMode>,
    pub(super) detected_hosts: Vec<HostMode>,
    pub(super) detected_clients: Vec<String>,
    pub(super) tools: ToolSnapshot,
    pub(super) codex: CodexSnapshot,
    pub(super) claude: ClaudeSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Init planning tracks a small fixed install/configuration matrix"
)]
pub(super) struct ToolSnapshot {
    pub(super) hyphae_installed: bool,
    pub(super) hyphae_broken: bool,
    pub(super) rhizome_installed: bool,
    pub(super) rhizome_broken: bool,
    pub(super) cortina_installed: bool,
    pub(super) cortina_broken: bool,
    pub(super) hyphae_db_exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub(super) struct CodexSnapshot {
    pub(super) notify_configured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub(super) struct ClaudeSnapshot {
    pub(super) hooks_configured: bool,
}

impl InitSnapshot {
    pub(super) fn target_host_mode(&self) -> Option<HostMode> {
        self.target_client
            .as_deref()
            .and_then(host_policy::host_mode_from_client_flag)
    }

    pub(super) fn target_is_codex(&self) -> bool {
        host_policy::codex_target_requested(self.target_client.as_deref())
    }

    pub(super) fn host_in_scope(&self, mode: HostMode) -> bool {
        if self.target_host_mode().is_some() {
            self.selected_hosts.contains(&mode)
        } else {
            self.selected_hosts.contains(&mode) || self.detected_hosts.contains(&mode)
        }
    }

    pub(super) fn codex_host_selected_or_detected(&self) -> bool {
        self.host_in_scope(HostMode::Codex)
    }

    pub(super) fn claude_host_selected_or_detected(&self) -> bool {
        self.host_in_scope(HostMode::ClaudeCode)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum InitStepStatus {
    Planned,
    AlreadyOk,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct InitStep {
    pub(super) status: InitStepStatus,
    pub(super) title: String,
    pub(super) detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct InitPlan {
    pub(super) schema_version: String,
    pub(super) dry_run: bool,
    pub(super) target_client: Option<String>,
    pub(super) selected_hosts: Vec<String>,
    pub(super) detected_hosts: Vec<String>,
    pub(super) detected_clients: Vec<String>,
    pub(super) steps: Vec<InitStep>,
    pub(super) repair_actions: Vec<RepairAction>,
}
