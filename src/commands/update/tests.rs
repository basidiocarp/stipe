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

#[test]
fn restore_after_failed_update_restores_previous_binary() {
    use crate::backup::{BackupManifest, BinaryRecord};
    use std::fs;

    // Build a backup snapshot + manifest by hand (no env var, no network) and
    // verify the restore helper rewrites the clobbered binary with the pre-update
    // bytes — invariant #1: a failed update restores the previous binary in place.
    // load_manifest / restore_from_backup read only the manifest's recorded paths,
    // not STIPE_BACKUP_DIR, so this is race-free against backup.rs's env tests.
    let tmp = tempfile::TempDir::new().expect("create tempdir");
    let root = tmp.path();

    let installed = root.join("installed").join("faketool");
    fs::create_dir_all(installed.parent().unwrap()).unwrap();

    let backup_dir = root.join("backup-1");
    let backup_bin = backup_dir.join("bin");
    fs::create_dir_all(&backup_bin).unwrap();
    let backup_path = backup_bin.join("faketool");
    fs::write(&backup_path, b"v1-good").unwrap();

    let manifest = BackupManifest {
        timestamp: "1".to_string(),
        stipe_version: "test".to_string(),
        binaries: vec![BinaryRecord {
            tool_name: "faketool".to_string(),
            original_path: installed.clone(),
            backup_path,
            version: None,
        }],
        config_files: Vec::new(),
    };
    fs::write(
        backup_dir.join("manifest.json"),
        serde_json::to_string(&manifest).unwrap(),
    )
    .unwrap();

    // Simulate the post-deploy broken binary a failed smoke check would leave live.
    fs::write(&installed, b"v2-broken").unwrap();

    restore_after_failed_update(&backup_dir, "faketool");

    assert_eq!(fs::read(&installed).unwrap(), b"v1-good");
}

#[test]
fn pre_update_backup_name_does_not_collide_with_bulk_backup() {
    // run()'s bulk pre-update backup uses a bare `backup_timestamp()`. The
    // per-tool name must NOT equal that bare timestamp, or the single-tool
    // manifest overwrites the bulk all-tools manifest and `stipe rollback`
    // restores only one tool. Guard against a regression to the bare name.
    let bare = crate::backup::backup_timestamp();
    let per_tool = pre_update_backup_name("mycelium");

    assert_ne!(per_tool, bare);
    assert!(per_tool.ends_with("-mycelium-preupdate"));
    // The bulk name is all digits; the per-tool name must not be (else a future
    // refactor could let them collide again under string comparison).
    assert!(!per_tool.chars().all(|c| c.is_ascii_digit()));
}
