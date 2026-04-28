use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum InstallProfile {
    Minimal,
    Standard,
    ClaudeCode,
    Codex,
    Cursor,
    #[value(name = "full", alias = "full-stack")]
    FullStack,
    #[value(name = "developer-tools", alias = "developer")]
    DeveloperTools,
}

impl InstallProfile {
    #[must_use]
    pub fn mode_label(self) -> &'static str {
        match self {
            Self::Minimal => "minimal profile",
            Self::Standard => "standard profile",
            Self::ClaudeCode => super::super::host_policy::CLAUDE_CODE_HOST_MODE_LABEL,
            Self::Codex => super::super::host_policy::CODEX_HOST_MODE_LABEL,
            Self::Cursor => "Cursor profile",
            Self::FullStack => "full profile",
            Self::DeveloperTools => "developer-tools profile",
        }
    }

    #[must_use]
    pub fn profile_name(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Standard => "standard",
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
            Self::FullStack => "full",
            Self::DeveloperTools => "developer-tools",
        }
    }

    #[must_use]
    pub fn from_profile_name(value: &str) -> Option<Self> {
        match value {
            "minimal" => Some(Self::Minimal),
            "standard" => Some(Self::Standard),
            "claude-code" => Some(Self::ClaudeCode),
            "codex" => Some(Self::Codex),
            "cursor" => Some(Self::Cursor),
            "full" | "full-stack" => Some(Self::FullStack),
            "developer-tools" | "developer" => Some(Self::DeveloperTools),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorCoverage {
    Required,
    Optional,
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolProbe {
    Installed(String),
    Missing,
    Broken,
}

impl ToolProbe {
    #[must_use]
    pub fn is_installed(&self) -> bool {
        matches!(self, Self::Installed(_))
    }

    #[must_use]
    pub fn is_repairable_presence(&self) -> bool {
        matches!(self, Self::Installed(_) | Self::Broken)
    }

    #[must_use]
    pub fn version(&self) -> Option<&str> {
        match self {
            Self::Installed(version) => Some(version.as_str()),
            Self::Missing | Self::Broken => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Tool registry tracks multiple independent command-surface memberships"
)]
pub struct ToolSpec {
    pub name: &'static str,
    pub binary_name: &'static str,
    pub release_repo: &'static str,
    pub description: &'static str,
    pub installable: bool,
    pub include_in_update_all: bool,
    pub include_in_uninstall_all: bool,
    pub include_in_status: bool,
    pub include_in_ecosystem: bool,
    pub include_in_install_all: bool,
    pub doctor_coverage: DoctorCoverage,
    pub install_profiles: &'static [InstallProfile],
    pub missing_hint: Option<&'static str>,
    pub smoke_test_args: Option<&'static [&'static str]>,
    pub smoke_test_expect: Option<&'static str>,
    pub mcp_serve_args: Option<&'static [&'static str]>,
    /// Stable capability ids this tool satisfies, e.g. `["memory.store.v1"]`.
    /// Populated in the `capability-registry-v1` contract written by Stipe.
    pub capability_ids: &'static [&'static str],
    /// Related Septa contract ids produced or consumed by this tool.
    pub contract_ids: &'static [&'static str],
}
