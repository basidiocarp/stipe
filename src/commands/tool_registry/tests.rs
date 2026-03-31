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
        InstallProfile::DeveloperTools,
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
