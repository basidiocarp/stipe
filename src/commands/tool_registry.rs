use clap::ValueEnum;
use std::process::Command;

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
            Self::ClaudeCode => super::host_policy::CLAUDE_CODE_HOST_MODE_LABEL,
            Self::Codex => super::host_policy::CODEX_HOST_MODE_LABEL,
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

const TOOL_SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "mycelium",
        binary_name: "mycelium",
        release_repo: "mycelium",
        description: "token compression proxy",
        installable: true,
        include_in_update_all: true,
        include_in_uninstall_all: true,
        include_in_status: true,
        include_in_ecosystem: true,
        include_in_install_all: true,
        doctor_coverage: DoctorCoverage::Required,
        install_profiles: &[
            InstallProfile::Minimal,
            InstallProfile::ClaudeCode,
            InstallProfile::Codex,
            InstallProfile::Cursor,
            InstallProfile::FullStack,
        ],
        missing_hint: None,
    },
    ToolSpec {
        name: "hyphae",
        binary_name: "hyphae",
        release_repo: "hyphae",
        description: "agent memory system",
        installable: true,
        include_in_update_all: true,
        include_in_uninstall_all: true,
        include_in_status: true,
        include_in_ecosystem: true,
        include_in_install_all: true,
        doctor_coverage: DoctorCoverage::Required,
        install_profiles: &[
            InstallProfile::ClaudeCode,
            InstallProfile::Codex,
            InstallProfile::Cursor,
            InstallProfile::FullStack,
        ],
        missing_hint: Some(
            "cargo install --git https://github.com/basidiocarp/hyphae hyphae-cli --no-default-features",
        ),
    },
    ToolSpec {
        name: "rhizome",
        binary_name: "rhizome",
        release_repo: "rhizome",
        description: "code intelligence server",
        installable: true,
        include_in_update_all: true,
        include_in_uninstall_all: true,
        include_in_status: true,
        include_in_ecosystem: true,
        include_in_install_all: true,
        doctor_coverage: DoctorCoverage::Required,
        install_profiles: &[
            InstallProfile::ClaudeCode,
            InstallProfile::Codex,
            InstallProfile::Cursor,
            InstallProfile::FullStack,
        ],
        missing_hint: Some(
            "cargo install --git https://github.com/basidiocarp/rhizome rhizome-cli",
        ),
    },
    ToolSpec {
        name: "canopy",
        binary_name: "canopy",
        release_repo: "canopy",
        description: "coordination runtime",
        installable: true,
        include_in_update_all: true,
        include_in_uninstall_all: true,
        include_in_status: true,
        include_in_ecosystem: true,
        include_in_install_all: true,
        doctor_coverage: DoctorCoverage::Optional,
        install_profiles: &[InstallProfile::FullStack],
        missing_hint: Some("stipe install canopy"),
    },
    ToolSpec {
        name: "cortina",
        binary_name: "cortina",
        release_repo: "cortina",
        description: "hook runner & session tracking",
        installable: true,
        include_in_update_all: true,
        include_in_uninstall_all: true,
        include_in_status: true,
        include_in_ecosystem: true,
        include_in_install_all: true,
        doctor_coverage: DoctorCoverage::Ignore,
        install_profiles: &[InstallProfile::ClaudeCode, InstallProfile::FullStack],
        missing_hint: Some("stipe install cortina"),
    },
    ToolSpec {
        name: "cap",
        binary_name: "cap",
        release_repo: "cap",
        description: "dashboard frontend",
        installable: false,
        include_in_update_all: false,
        include_in_uninstall_all: false,
        include_in_status: true,
        include_in_ecosystem: true,
        include_in_install_all: false,
        doctor_coverage: DoctorCoverage::Ignore,
        install_profiles: &[],
        missing_hint: Some(
            "git clone https://github.com/basidiocarp/cap && cd cap && npm i && npm run dev:all",
        ),
    },
    ToolSpec {
        name: "stipe",
        binary_name: "stipe",
        release_repo: "stipe",
        description: "ecosystem manager",
        installable: false,
        include_in_update_all: false,
        include_in_uninstall_all: true,
        include_in_status: false,
        include_in_ecosystem: false,
        include_in_install_all: false,
        doctor_coverage: DoctorCoverage::Ignore,
        install_profiles: &[],
        missing_hint: None,
    },
];

#[must_use]
pub fn installable_specs() -> Vec<&'static ToolSpec> {
    TOOL_SPECS.iter().filter(|spec| spec.installable).collect()
}

#[must_use]
pub fn install_all_specs() -> Vec<&'static ToolSpec> {
    TOOL_SPECS
        .iter()
        .filter(|spec| spec.include_in_install_all)
        .collect()
}

#[must_use]
pub fn specs_for_profile(profile: InstallProfile) -> Vec<&'static ToolSpec> {
    TOOL_SPECS
        .iter()
        .filter(|spec| spec.install_profiles.contains(&profile))
        .collect()
}

#[must_use]
pub fn uninstall_all_specs() -> Vec<&'static ToolSpec> {
    TOOL_SPECS
        .iter()
        .filter(|spec| spec.include_in_uninstall_all)
        .collect()
}

#[must_use]
pub fn update_all_specs() -> Vec<&'static ToolSpec> {
    TOOL_SPECS
        .iter()
        .filter(|spec| spec.include_in_update_all)
        .collect()
}

#[must_use]
pub fn status_specs() -> Vec<&'static ToolSpec> {
    TOOL_SPECS
        .iter()
        .filter(|spec| spec.include_in_status)
        .collect()
}

#[must_use]
pub fn ecosystem_specs() -> Vec<&'static ToolSpec> {
    TOOL_SPECS
        .iter()
        .filter(|spec| spec.include_in_ecosystem)
        .collect()
}

#[must_use]
pub fn doctor_specs() -> Vec<&'static ToolSpec> {
    TOOL_SPECS
        .iter()
        .filter(|spec| spec.doctor_coverage != DoctorCoverage::Ignore)
        .collect()
}

#[must_use]
pub fn find(name: &str) -> Option<&'static ToolSpec> {
    TOOL_SPECS.iter().find(|spec| spec.name == name)
}

#[must_use]
pub fn release_archive_binaries() -> Vec<&'static str> {
    TOOL_SPECS
        .iter()
        .filter(|spec| spec.installable || spec.name == "stipe")
        .map(|spec| spec.binary_name)
        .collect()
}

#[must_use]
pub fn probe(spec: &ToolSpec) -> ToolProbe {
    let Ok(binary_path) = which::which(spec.binary_name) else {
        return ToolProbe::Missing;
    };

    match Command::new(&binary_path).arg("--version").output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let version = parse_version(&stdout);
            ToolProbe::Installed(version)
        }
        Ok(_) | Err(_) => ToolProbe::Broken,
    }
}

#[must_use]
pub fn parse_version(output: &str) -> String {
    output
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().last())
        .filter(|version| version.contains('.'))
        .map_or_else(|| "unknown".to_string(), str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_tools_cover_expected_sets() {
        let minimal = specs_for_profile(InstallProfile::Minimal)
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();
        assert_eq!(minimal, vec!["mycelium"]);

        let claude = specs_for_profile(InstallProfile::ClaudeCode)
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();
        assert_eq!(claude, vec!["mycelium", "hyphae", "rhizome", "cortina"]);

        let full_stack = specs_for_profile(InstallProfile::FullStack)
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();
        assert_eq!(
            full_stack,
            vec!["mycelium", "hyphae", "rhizome", "canopy", "cortina"]
        );
    }

    #[test]
    fn test_status_specs_include_optional_and_managed_tools() {
        let names = status_specs()
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();

        assert!(names.contains(&"mycelium"));
        assert!(names.contains(&"cortina"));
        assert!(names.contains(&"canopy"));
        assert!(names.contains(&"cap"));
        assert!(!names.contains(&"stipe"));
    }

    #[test]
    fn test_release_archive_binaries_include_managed_tools_and_stipe() {
        let names = release_archive_binaries();
        assert!(names.contains(&"mycelium"));
        assert!(names.contains(&"canopy"));
        assert!(names.contains(&"cortina"));
        assert!(names.contains(&"stipe"));
        assert!(!names.contains(&"cap"));
    }

    #[test]
    fn test_doctor_specs_include_optional_canopy() {
        let names = doctor_specs()
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["mycelium", "hyphae", "rhizome", "canopy"]);
    }

    #[test]
    fn test_install_profiles_only_reference_installable_tools() {
        let installable = installable_specs()
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();

        for profile in [
            InstallProfile::Minimal,
            InstallProfile::ClaudeCode,
            InstallProfile::Codex,
            InstallProfile::Cursor,
            InstallProfile::FullStack,
        ] {
            for spec in specs_for_profile(profile) {
                assert!(
                    installable.contains(&spec.name),
                    "{} includes non-installable tool {}",
                    profile.mode_label(),
                    spec.name
                );
            }
        }
    }

    #[test]
    fn test_install_all_and_update_all_cover_same_managed_release_tools() {
        let install_all = install_all_specs()
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();
        let update_all = update_all_specs()
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();

        assert_eq!(install_all, update_all);
    }

    #[test]
    fn test_ecosystem_and_status_views_only_reference_visible_tools() {
        let status = status_specs()
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();

        for spec in ecosystem_specs() {
            assert!(
                status.contains(&spec.name),
                "ecosystem view references tool missing from status: {}",
                spec.name
            );
        }
    }

    #[test]
    fn test_optional_doctor_tools_have_install_hints() {
        for spec in doctor_specs() {
            if spec.doctor_coverage == DoctorCoverage::Optional {
                assert!(
                    spec.missing_hint.is_some(),
                    "optional doctor tool {} should have a repair hint",
                    spec.name
                );
            }
        }
    }
}
