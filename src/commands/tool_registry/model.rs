use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum InstallProfile {
    Minimal,
    ClaudeCode,
    Codex,
    Cursor,
    FullStack,
}

impl InstallProfile {
    #[must_use]
    pub fn mode_label(self) -> &'static str {
        match self {
            Self::Minimal => "minimal profile",
            Self::ClaudeCode => super::super::host_policy::CLAUDE_CODE_HOST_MODE_LABEL,
            Self::Codex => super::super::host_policy::CODEX_HOST_MODE_LABEL,
            Self::Cursor => "Cursor profile",
            Self::FullStack => "full-stack profile",
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
}
