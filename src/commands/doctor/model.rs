use crate::commands::repair::RepairAction;
use serde::Serialize;

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
    pub(super) checks: Vec<HealthCheck>,
    pub(super) repair_actions: Vec<RepairAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConfigFormat {
    Json,
    Toml,
    ClaudeRoot,
}
