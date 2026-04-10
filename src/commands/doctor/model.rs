use std::path::PathBuf;

use crate::commands::claude_hooks::HookPathSnapshot;
use crate::commands::developer_tools::DeveloperToolsReport;
use crate::commands::host_policy::HostMode;
use crate::commands::repair::RepairAction;
use crate::commands::runtime_policy::RuntimePolicyReport;
use serde::Serialize;

pub(super) use crate::commands::init::baseline::{DriftFinding, DriftReport};

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
pub(super) enum AuthFreshness {
    Fresh,
    Stale,
    Missing,
    Unknown,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct ProviderHealth {
    pub(super) host: HostMode,
    pub(super) provider: String,
    pub(super) available: bool,
    pub(super) healthy: bool,
    pub(super) status: String,
    pub(super) auth_freshness: AuthFreshness,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) auth_detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct McpHealth {
    pub(super) host: HostMode,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(super) config_paths: Vec<PathBuf>,
    pub(super) required_servers: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(super) registered_servers: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(super) missing_servers: Vec<String>,
    pub(super) healthy: bool,
    pub(super) status: String,
    pub(super) auth_freshness: AuthFreshness,
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
