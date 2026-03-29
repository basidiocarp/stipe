use clap::Subcommand;
use serde::Serialize;

use crate::commands::host_policy::{HostAdapterKind, HostConfigScope, HostMode};
use crate::commands::repair::RepairAction;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostInventoryEntry {
    pub mode: HostMode,
    pub label: String,
    pub adapter_kind: HostAdapterKind,
    pub adapter_label: String,
    pub detected: bool,
    pub configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostDoctorCheck {
    pub host: HostMode,
    pub passed: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub repair_actions: Vec<RepairAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostDoctorReport {
    pub healthy: bool,
    pub summary: String,
    pub checks: Vec<HostDoctorCheck>,
    pub repair_actions: Vec<RepairAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
pub enum HostCommand {
    /// List known hosts and whether they are currently detected/configured
    List,

    /// Install and initialize a single host without assuming it is the only one
    Setup {
        /// Host to configure
        #[arg(value_enum)]
        mode: HostMode,

        /// Scope for host-specific adapter configuration
        #[arg(long, value_enum, default_value_t = HostConfigScope::User)]
        scope: HostConfigScope,

        /// Show what would change without mutating the machine
        #[arg(long)]
        dry_run: bool,
    },

    /// Check one host, or all known hosts, without collapsing them into one mode
    Doctor {
        /// Optional host to inspect
        #[arg(value_enum)]
        mode: Option<HostMode>,

        /// Emit structured JSON instead of human-readable text
        #[arg(long)]
        json: bool,
    },

    #[command(hide = true, name = "claude-code")]
    LegacyClaudeCode {
        #[arg(long)]
        dry_run: bool,
    },

    #[command(hide = true, name = "codex")]
    LegacyCodex {
        #[arg(long)]
        dry_run: bool,
    },

    #[command(hide = true, name = "cursor")]
    LegacyCursor {
        #[arg(long)]
        dry_run: bool,
    },
}
