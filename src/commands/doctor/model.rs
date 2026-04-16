use std::path::PathBuf;

use crate::commands::claude_hooks::HookPathSnapshot;
use crate::commands::developer_tools::DeveloperToolsReport;
use crate::commands::host_policy::HostMode;
use crate::commands::repair::RepairAction;
use crate::commands::runtime_policy::RuntimePolicyReport;
use serde::Serialize;

pub(super) use crate::commands::init::baseline::{DriftFinding, DriftReport};

// ---------------------------------------------------------------------------
// MCP server binary health
// ---------------------------------------------------------------------------

/// Coarse status for a single registered MCP server binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum McpServerStatus {
    NotInstalled,
    InstalledNotResponding,
    Running,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct McpServerHealth {
    pub(super) name: String,
    pub(super) status: McpServerStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) detail: Option<String>,
}

// ---------------------------------------------------------------------------
// Provider API key health
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ApiKeyStatus {
    Configured,
    Missing,
    UnexpectedFormat,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct ApiKeyHealth {
    pub(crate) provider: String,
    pub(crate) status: ApiKeyStatus,
    /// Human-readable note — keys are never included.
    pub(crate) note: String,
}

// ---------------------------------------------------------------------------
// Plugin and hook inventory
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum PluginPathStatus {
    Valid,
    Stale,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum VersionDriftStatus {
    UpToDate,
    Behind,
    Unknown,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct PluginInventoryItem {
    /// Logical name of the skill / hook / command.
    pub(super) name: String,
    /// Category: "skill", "hook", "command".
    pub(super) category: String,
    pub(super) path_status: PluginPathStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) installed_version: Option<String>,
    pub(super) version_drift: VersionDriftStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) pinned_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct PluginInventory {
    /// Whether `annulus validate-hooks` was available and used.
    pub(super) annulus_validator_used: bool,
    pub(super) items: Vec<PluginInventoryItem>,
    pub(super) skills_count: usize,
    pub(super) hooks_count: usize,
    pub(super) stale_count: usize,
    pub(super) missing_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct HealthCheck {
    pub(super) name: String,
    pub(super) passed: bool,
    pub(super) message: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(super) repair_actions: Vec<RepairAction>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct DoctorReport {
    pub(super) schema_version: String,
    pub(super) healthy: bool,
    pub(super) summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) install_profile: Option<InstallProfileSummary>,
    pub(super) checks: Vec<HealthCheck>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(super) hook_paths: Vec<HookPathSnapshot>,
    pub(super) repair_actions: Vec<RepairAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) drift: Option<DriftReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) developer_tools: Option<DeveloperToolsReport>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(super) provider_health: Vec<ProviderHealth>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(super) mcp_health: Vec<McpHealth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) runtime_policy: Option<RuntimePolicyReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) package_inventory: Option<PackageInventory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) worktree_config: Option<WorktreeConfigDiscovery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) package_drift: Option<PackageDrift>,
    /// MCP server binary health (presence + responsiveness).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(super) mcp_server_health: Vec<McpServerHealth>,
    /// Provider and API key presence / format checks.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(super) api_key_health: Vec<ApiKeyHealth>,
    /// Installed lamella skills, hooks, and commands with version drift.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) plugin_inventory: Option<PluginInventory>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct InstallProfileSummary {
    pub(super) profile: String,
    pub(super) config_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConfigFormat {
    Json,
    Toml,
    #[allow(dead_code)]
    ClaudeRoot,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AuthFreshness {
    Fresh,
    Stale,
    Missing,
    Unknown,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct ProviderHealth {
    pub(crate) host: HostMode,
    pub(crate) provider: String,
    pub(crate) available: bool,
    pub(crate) healthy: bool,
    pub(crate) status: String,
    pub(crate) auth_freshness: AuthFreshness,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) auth_detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct McpHealth {
    pub(crate) host: HostMode,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) config_paths: Vec<PathBuf>,
    pub(crate) required_servers: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) registered_servers: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) missing_servers: Vec<String>,
    pub(crate) healthy: bool,
    pub(crate) status: String,
    pub(crate) auth_freshness: AuthFreshness,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct PackageInventory {
    pub(super) package_metadata_available: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(super) metadata_sources: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(super) discovered_packages: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(super) discovered_plugins: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct WorktreeConfigDiscovery {
    pub(super) detected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) project_root: Option<PathBuf>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(super) discovered_configs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct PackageDrift {
    pub(super) metadata_available: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(super) expected_packages: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(super) installed_packages: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(super) missing_packages: Vec<String>,
}
