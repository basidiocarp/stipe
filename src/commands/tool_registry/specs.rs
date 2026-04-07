use super::{DoctorCoverage, InstallProfile, ToolSpec};

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
            InstallProfile::Standard,
            InstallProfile::ClaudeCode,
            InstallProfile::Codex,
            InstallProfile::Cursor,
            InstallProfile::FullStack,
        ],
        missing_hint: None,
        smoke_test_args: Some(&["proxy", "echo", "stipe-verify"]),
        smoke_test_expect: Some("stipe-verify"),
        mcp_serve_args: None,
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
            InstallProfile::Minimal,
            InstallProfile::Standard,
            InstallProfile::ClaudeCode,
            InstallProfile::Codex,
            InstallProfile::Cursor,
            InstallProfile::FullStack,
        ],
        missing_hint: Some(
            "cargo install --git https://github.com/basidiocarp/hyphae hyphae-cli --no-default-features",
        ),
        smoke_test_args: Some(&["doctor"]),
        smoke_test_expect: None,
        mcp_serve_args: Some(&["serve"]),
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
            InstallProfile::Standard,
            InstallProfile::ClaudeCode,
            InstallProfile::Codex,
            InstallProfile::Cursor,
            InstallProfile::FullStack,
        ],
        missing_hint: Some(
            "cargo install --git https://github.com/basidiocarp/rhizome rhizome-cli",
        ),
        smoke_test_args: None,
        smoke_test_expect: None,
        mcp_serve_args: Some(&["serve", "--expanded"]),
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
        smoke_test_args: Some(&["task", "list"]),
        smoke_test_expect: None,
        mcp_serve_args: None,
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
        install_profiles: &[
            InstallProfile::Standard,
            InstallProfile::ClaudeCode,
            InstallProfile::FullStack,
        ],
        missing_hint: Some("stipe install cortina"),
        smoke_test_args: None,
        smoke_test_expect: None,
        mcp_serve_args: None,
    },
    ToolSpec {
        name: "volva",
        binary_name: "volva",
        release_repo: "volva",
        description: "backend operations CLI",
        installable: true,
        include_in_update_all: true,
        include_in_uninstall_all: true,
        include_in_status: true,
        include_in_ecosystem: true,
        include_in_install_all: true,
        doctor_coverage: DoctorCoverage::Optional,
        install_profiles: &[InstallProfile::FullStack],
        missing_hint: Some("stipe install volva"),
        smoke_test_args: Some(&["backend", "status"]),
        smoke_test_expect: None,
        mcp_serve_args: None,
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
        smoke_test_args: None,
        smoke_test_expect: None,
        mcp_serve_args: None,
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
        smoke_test_args: None,
        smoke_test_expect: None,
        mcp_serve_args: None,
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
