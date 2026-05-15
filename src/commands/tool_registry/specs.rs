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
        capability_ids: &["command.filter.v1"],
        contract_ids: &["command-output-v1", "mycelium-gain-v1"],
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
        smoke_test_args: Some(&["--version"]),
        smoke_test_expect: None,
        mcp_serve_args: Some(&["serve"]),
        capability_ids: &["memory.store.v1", "memory.recall.v1", "memoir.import.v1"],
        contract_ids: &["command-output-v1", "code-graph-v1"],
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
        capability_ids: &["code.graph.v1", "code.symbols.v1"],
        contract_ids: &["code-graph-v1"],
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
        capability_ids: &["coordination.task.v1"],
        contract_ids: &["canopy-task-detail-v1", "dispatch-request-v1"],
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
        capability_ids: &["lifecycle.capture.v1"],
        contract_ids: &["cortina-lifecycle-event-v1"],
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
        capability_ids: &["hook.execution.v1"],
        contract_ids: &["volva-hook-event-v1"],
    },
    ToolSpec {
        name: "annulus",
        binary_name: "annulus",
        release_repo: "annulus",
        description: "operator utilities",
        installable: true,
        include_in_update_all: true,
        include_in_uninstall_all: true,
        include_in_status: true,
        include_in_ecosystem: true,
        include_in_install_all: true,
        doctor_coverage: DoctorCoverage::Optional,
        install_profiles: &[InstallProfile::FullStack],
        missing_hint: Some("stipe install annulus"),
        smoke_test_args: Some(&["--version"]),
        smoke_test_expect: None,
        mcp_serve_args: None,
        capability_ids: &["statusline.render.v1"],
        contract_ids: &["annulus-statusline-v1"],
    },
    ToolSpec {
        name: "hymenium",
        binary_name: "hymenium",
        release_repo: "hymenium",
        description: "workflow orchestration engine",
        installable: true,
        include_in_update_all: true,
        include_in_uninstall_all: true,
        include_in_status: true,
        include_in_ecosystem: true,
        include_in_install_all: true,
        doctor_coverage: DoctorCoverage::Optional,
        install_profiles: &[InstallProfile::FullStack],
        missing_hint: Some("stipe install hymenium"),
        smoke_test_args: Some(&["status"]),
        smoke_test_expect: None,
        mcp_serve_args: None,
        capability_ids: &["workflow.dispatch.v1"],
        contract_ids: &["dispatch-request-v1", "workflow-outcome-v1"],
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
        capability_ids: &[],
        contract_ids: &["canopy-snapshot-v1", "stipe-doctor-v1"],
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
        capability_ids: &[],
        contract_ids: &["stipe-doctor-v1", "stipe-init-plan-v1"],
    },
    ToolSpec {
        name: "lamella",
        binary_name: "lamella",
        release_repo: "lamella",
        description: "skills and plugin packaging",
        installable: true,
        include_in_update_all: true,
        include_in_uninstall_all: true,
        include_in_status: true,
        include_in_ecosystem: true,
        include_in_install_all: true,
        doctor_coverage: DoctorCoverage::Optional,
        install_profiles: &[InstallProfile::FullStack],
        missing_hint: Some("stipe install lamella"),
        smoke_test_args: Some(&["--version"]),
        smoke_test_expect: None,
        mcp_serve_args: None,
        capability_ids: &["plugin.packaging.v1"],
        contract_ids: &["lamella-package-v1"],
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
pub fn all_specs() -> Vec<&'static ToolSpec> {
    TOOL_SPECS.iter().collect()
}

#[must_use]
pub fn release_archive_binaries() -> Vec<&'static str> {
    TOOL_SPECS
        .iter()
        .filter(|spec| spec.installable || spec.name == "stipe")
        .map(|spec| spec.binary_name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::doctor::version_pins;

    #[test]
    fn pinned_and_installable_specs_stay_in_sync() {
        let pins = version_pins::pinned_ecosystem_versions();

        // Check that all pinned tools have a corresponding TOOL_SPECS entry
        for (pinned_name, _version) in pins.iter() {
            let spec = find(pinned_name);
            assert!(
                spec.is_some(),
                "tool '{}' is pinned but not found in TOOL_SPECS",
                pinned_name
            );
        }

        // Check that all installable tools have a version pin
        for spec in installable_specs() {
            assert!(
                pins.contains_key(spec.name),
                "tool '{}' is installable but has no version pin",
                spec.name
            );
        }
    }
}
