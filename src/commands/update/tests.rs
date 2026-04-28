use super::*;
use crate::commands::tool_registry::{DoctorCoverage, ToolProbe, ToolSpec};

fn spec(name: &'static str, profiles: &'static [InstallProfile]) -> ToolSpec {
    ToolSpec {
        name,
        binary_name: name,
        release_repo: name,
        description: "test tool",
        installable: true,
        include_in_update_all: true,
        include_in_uninstall_all: true,
        include_in_status: true,
        include_in_ecosystem: true,
        include_in_install_all: true,
        doctor_coverage: DoctorCoverage::Required,
        install_profiles: profiles,
        missing_hint: None,
        smoke_test_args: None,
        smoke_test_expect: None,
        mcp_serve_args: None,
        capability_ids: &[],
        contract_ids: &[],
    }
}

#[test]
fn installed_profile_tools_only_keep_installed_or_broken_members() {
    let profile = InstallProfile::ClaudeCode;
    let mycelium = spec("mycelium", &[InstallProfile::ClaudeCode]);
    let hyphae = spec("hyphae", &[InstallProfile::ClaudeCode]);
    let cortina = spec("cortina", &[InstallProfile::ClaudeCode]);

    let specs = vec![&mycelium, &hyphae, &cortina];
    let selected = specs
        .into_iter()
        .filter_map(|candidate| {
            if !candidate.install_profiles.contains(&profile) {
                return None;
            }
            let probe = match candidate.name {
                "mycelium" => ToolProbe::Installed("0.8.0".to_string()),
                "hyphae" => ToolProbe::Broken,
                _ => ToolProbe::Missing,
            };
            probe
                .is_repairable_presence()
                .then_some(candidate.name.to_string())
        })
        .collect::<Vec<_>>();

    assert_eq!(selected, vec!["mycelium".to_string(), "hyphae".to_string()]);
}

#[test]
fn installed_profile_tools_with_helper_keeps_only_present_tools() {
    let selected =
        installed_profile_tools_with(InstallProfile::ClaudeCode, |spec| match spec.name {
            "mycelium" => ToolProbe::Installed("0.8.0".to_string()),
            "hyphae" => ToolProbe::Broken,
            _ => ToolProbe::Missing,
        });

    assert_eq!(selected, vec!["mycelium".to_string(), "hyphae".to_string()]);
}

#[test]
fn unique_tools_appends_explicit_extras_without_duplicates() {
    let resolved = unique_tools(
        vec!["mycelium".to_string(), "hyphae".to_string()],
        &["hyphae".to_string(), "canopy".to_string()],
    );

    assert_eq!(
        resolved,
        vec![
            "mycelium".to_string(),
            "hyphae".to_string(),
            "canopy".to_string()
        ]
    );
}
