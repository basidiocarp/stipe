use super::profile_config;
use super::*;
use crate::commands::host_policy;
use crate::commands::install::runner::{
    render_install_success_summary, selected_profile_for_persistence,
};
use crate::commands::runtime_policy;
use crate::commands::tool_registry;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn test_profile_tools_cover_expected_sets() {
    let minimal = tool_registry::specs_for_profile(InstallProfile::Minimal)
        .into_iter()
        .map(|spec| spec.name)
        .collect::<Vec<_>>();
    assert_eq!(minimal, vec!["mycelium", "hyphae"]);

    let standard = tool_registry::specs_for_profile(InstallProfile::Standard)
        .into_iter()
        .map(|spec| spec.name)
        .collect::<Vec<_>>();
    assert_eq!(standard, vec!["mycelium", "hyphae", "rhizome", "cortina"]);

    let claude = tool_registry::specs_for_profile(InstallProfile::ClaudeCode)
        .into_iter()
        .map(|spec| spec.name)
        .collect::<Vec<_>>();
    assert_eq!(claude, vec!["mycelium", "hyphae", "rhizome", "cortina"]);

    let codex = tool_registry::specs_for_profile(InstallProfile::Codex)
        .into_iter()
        .map(|spec| spec.name)
        .collect::<Vec<_>>();
    assert_eq!(codex, vec!["mycelium", "hyphae", "rhizome"]);

    let cursor = tool_registry::specs_for_profile(InstallProfile::Cursor)
        .into_iter()
        .map(|spec| spec.name)
        .collect::<Vec<_>>();
    assert_eq!(cursor, vec!["mycelium", "hyphae", "rhizome"]);

    let full_stack = tool_registry::specs_for_profile(InstallProfile::FullStack)
        .into_iter()
        .map(|spec| spec.name)
        .collect::<Vec<_>>();
    assert_eq!(
        full_stack,
        vec![
            "mycelium", "hyphae", "rhizome", "canopy", "cortina", "volva", "annulus", "hymenium"
        ]
    );

    let developer_tools = tool_registry::specs_for_profile(InstallProfile::DeveloperTools)
        .into_iter()
        .map(|spec| spec.name)
        .collect::<Vec<_>>();
    assert!(developer_tools.is_empty());
}

#[test]
fn test_resolve_requested_tools_uses_profile_and_dedupes_extras() {
    let resolved = resolve_requested_tools(
        false,
        Some(InstallProfile::Cursor),
        &["rhizome".to_string(), "cortina".to_string()],
    )
    .expect("profile should resolve");

    assert_eq!(
        resolved,
        vec![
            "mycelium".to_string(),
            "hyphae".to_string(),
            "rhizome".to_string(),
            "cortina".to_string(),
        ]
    );
}

#[test]
fn test_resolve_requested_tools_handles_all_mode() {
    let resolved = resolve_requested_tools(true, Some(InstallProfile::Minimal), &[]).unwrap();

    assert_eq!(
        resolved,
        vec![
            "mycelium".to_string(),
            "hyphae".to_string(),
            "rhizome".to_string(),
            "canopy".to_string(),
            "cortina".to_string(),
            "volva".to_string(),
            "annulus".to_string(),
            "hymenium".to_string(),
        ]
    );
}

#[test]
fn test_resolve_requested_tools_includes_manual_profile_members() {
    let resolved = resolve_requested_tools(false, Some(InstallProfile::Standard), &[])
        .expect("standard profile should resolve");

    assert_eq!(
        resolved,
        vec![
            "mycelium".to_string(),
            "hyphae".to_string(),
            "rhizome".to_string(),
            "cortina".to_string(),
            "lamella".to_string(),
        ]
    );
}

#[test]
fn test_format_install_preview_reports_existing_and_missing_tools() {
    let temp_dir = std::env::temp_dir().join("stipe-install-preview");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();
    std::fs::write(temp_dir.join("mycelium"), "").unwrap();

    let lines = format_install_preview(
        &temp_dir,
        &["mycelium".to_string(), "hyphae".to_string()],
        "minimal profile",
    );

    assert!(
        lines
            .iter()
            .any(|line| line.contains("Mode: minimal profile"))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("mycelium") && line.contains("keep existing binary"))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("hyphae") && line.contains("install release"))
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_render_install_preview_snapshot_for_explicit_tools() {
    let temp_dir = std::env::temp_dir().join("stipe-install-preview-snapshot");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();
    std::fs::write(temp_dir.join("mycelium"), "").unwrap();

    assert_eq!(
        render_install_preview(
            &temp_dir,
            &["mycelium".to_string(), "hyphae".to_string()],
            "minimal profile"
        ),
        vec![
            "Install preview | dry run".to_string(),
            String::new(),
            "Mode: minimal profile".to_string(),
            "Plan:".to_string(),
            format!(
                "  - mycelium     keep existing binary at {}",
                temp_dir.join("mycelium").display()
            ),
            format!(
                "  - hyphae       install release to {}",
                temp_dir.join("hyphae").display()
            ),
            String::new(),
            "Next step: run `stipe install ...` to apply this plan.".to_string(),
        ]
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_render_install_preview_snapshot_for_interactive_mode() {
    assert_eq!(
        render_install_preview(std::path::Path::new("/tmp/tools"), &[], "minimal profile"),
        vec![
            "Install preview | dry run".to_string(),
            String::new(),
            "Mode: interactive selection".to_string(),
            "Selection flow:".to_string(),
            "  Managed tools open with every entry preselected.".to_string(),
            "  Use space to toggle entries and enter to confirm.".to_string(),
            String::new(),
            "Available tools:".to_string(),
            "  mycelium        token compression proxy".to_string(),
            "  hyphae          agent memory system".to_string(),
            "  rhizome         code intelligence server".to_string(),
            "  canopy          coordination runtime".to_string(),
            "  cortina         hook runner & session tracking".to_string(),
            "  volva           backend operations CLI".to_string(),
            "  annulus         operator utilities".to_string(),
            "  hymenium        workflow orchestration engine".to_string(),
        ]
    );
}


#[test]
fn test_split_requested_tools_keeps_manual_members_out_of_managed_installs() {
    let (managed, manual) = split_requested_tools(&[
        "mycelium".to_string(),
        "lamella".to_string(),
        "cap".to_string(),
    ]);

    assert_eq!(managed, vec!["mycelium".to_string()]);
    assert_eq!(
        manual.iter().map(|member| member.name).collect::<Vec<_>>(),
        vec!["lamella", "cap"]
    );
}

#[test]
fn test_render_profile_install_preview_snapshot() {
    let install_root = std::path::Path::new("/tmp/tools");
    let preview = render_profile_install_preview(
        install_root,
        InstallProfile::Minimal,
        &["mycelium".to_string(), "hyphae".to_string()],
    );

    assert_eq!(
        preview,
        vec![
            "Install preview | dry run".to_string(),
            String::new(),
            "Profile: minimal profile".to_string(),
            "Managed installs:".to_string(),
            format!(
                "  - mycelium     managed install to {}",
                install_root.join("mycelium").display()
            ),
            format!(
                "  - hyphae       managed install to {}",
                install_root.join("hyphae").display()
            ),
            String::new(),
            "Not included in this profile:".to_string(),
            "  - rhizome".to_string(),
            "  - cortina".to_string(),
            "  - lamella".to_string(),
            "  - cap".to_string(),
            "  - canopy".to_string(),
            "  - volva".to_string(),
            String::new(),
            "Next step: run `stipe install --profile minimal` to apply this plan.".to_string(),
        ]
    );
}

#[test]
fn test_render_embedded_profile_install_preview_omits_next_step() {
    let install_root = std::path::Path::new("/tmp/tools");
    let preview = render_embedded_profile_install_preview(
        install_root,
        InstallProfile::Cursor,
        &[
            "mycelium".to_string(),
            "hyphae".to_string(),
            "rhizome".to_string(),
        ],
    );

    assert!(!preview.iter().any(|line| line.starts_with("Next step:")));
}

#[test]
fn test_profile_mode_labels_make_codex_explicit() {
    assert_eq!(InstallProfile::Standard.mode_label(), "standard profile");
    assert_eq!(InstallProfile::Codex.mode_label(), "Codex host mode");
    assert_eq!(
        InstallProfile::ClaudeCode.mode_label(),
        "Claude Code operator mode"
    );
    assert_eq!(
        InstallProfile::DeveloperTools.mode_label(),
        "developer-tools profile"
    );
    assert_eq!(InstallProfile::FullStack.mode_label(), "full profile");
}

#[test]
fn test_selected_profile_for_persistence_skips_failed_installs() {
    let failures = vec!["rhizome: download failed".to_string()];

    assert_eq!(
        selected_profile_for_persistence(&failures, Some(InstallProfile::Codex)),
        None
    );
}

#[test]
fn test_selected_profile_for_persistence_keeps_successful_non_developer_profile() {
    let failures = Vec::new();

    assert_eq!(
        selected_profile_for_persistence(&failures, Some(InstallProfile::Codex)),
        Some(InstallProfile::Codex)
    );
    assert_eq!(
        selected_profile_for_persistence(&failures, Some(InstallProfile::DeveloperTools)),
        None
    );
}

#[test]
fn test_profile_config_round_trips() {
    let temp_dir = std::env::temp_dir().join("stipe-test-profile-config");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let config_path = temp_dir.join("profile.toml");
    save_profile_to_path(&config_path, InstallProfile::Standard).unwrap();

    let loaded = load_profile_from_path(&config_path).unwrap();
    assert_eq!(loaded, Some(InstallProfile::Standard));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_install_run_honors_project_runtime_policy_deny() {
    with_test_project_root("runtime-policy-deny", |project_root, _config_root| {
        write_runtime_policy_file(
            &project_root,
            r#"
[[remembered_decisions]]
subject = "install-profile:codex"
scope = "project"
decision = "deny"
source = "operator-policy-file"
updated_at_unix = 42
"#,
        );

        let error = run(
            false,
            Some(InstallProfile::Codex),
            true,
            false,
            None,
            &Vec::new(),
        )
        .expect_err("project-scoped deny should block install");

        let message = error.to_string();
        assert!(message.contains("runtime policy denies install profile codex"));
        assert!(message.contains("project scope"));
    });
}

#[test]
fn test_install_run_fails_when_project_runtime_policy_cannot_load() {
    with_test_project_root("runtime-policy-load-error", |project_root, _config_root| {
        write_runtime_policy_file(
            &project_root,
            r#"
[[remembered_decisions]]
subject = "install-profile:codex"
scope = "project"
decision = "allow"
source = "operator-policy-file"
updated_at_unix = "not-an-integer"
"#,
        );

        let error = run(
            false,
            Some(InstallProfile::Codex),
            true,
            false,
            None,
            &Vec::new(),
        )
        .expect_err("policy load failures should block install");

        let message = error.to_string();
        assert!(message.contains("runtime policy for install profile codex could not be loaded"));
        assert!(message.contains("stipe-runtime-policy.toml"));
    });
}

#[test]
fn test_install_run_honors_user_runtime_policy_deny_without_project_override() {
    with_test_project_root("runtime-policy-user-deny", |project_root, config_root| {
        write_user_runtime_policy_file(
            &config_root,
            r#"
[[remembered_decisions]]
subject = "install-profile:codex"
scope = "user"
decision = "deny"
source = "operator-policy-file"
updated_at_unix = 84
"#,
        );

        let error = run(
            false,
            Some(InstallProfile::Codex),
            true,
            false,
            None,
            &Vec::new(),
        )
        .expect_err("user-scoped deny should block install when no project override exists");

        let message = error.to_string();
        assert!(message.contains("runtime policy denies install profile codex"));
        assert!(message.contains("user scope"));
        assert!(
            !project_root
                .join(".basidiocarp")
                .join("stipe-runtime-policy.toml")
                .exists()
        );
    });
}

#[test]
fn test_install_run_prefers_project_runtime_policy_over_user_policy() {
    with_test_project_root(
        "runtime-policy-project-over-user",
        |project_root, config_root| {
            write_runtime_policy_file(
                &project_root,
                r#"
[[remembered_decisions]]
subject = "install-profile:codex"
scope = "project"
decision = "allow"
source = "operator-policy-file"
updated_at_unix = 120
"#,
            );
            write_user_runtime_policy_file(
                &config_root,
                r#"
[[remembered_decisions]]
subject = "install-profile:codex"
scope = "user"
decision = "deny"
source = "operator-policy-file"
updated_at_unix = 240
"#,
            );

            let install_bin_dir = project_root.join("bin");
            fs::create_dir_all(&install_bin_dir).expect("create install bin dir");

            let result = crate::commands::install::runner::with_install_test_overrides(
                install_bin_dir,
                Ok(()),
                || {
                    run(
                        false,
                        Some(InstallProfile::Codex),
                        true,
                        false,
                        None,
                        &Vec::new(),
                    )
                },
            );

            result.expect("project-scoped allow should override user-scoped deny");
        },
    );
}

#[test]
fn test_install_run_persists_profile_and_approval_memory_on_success() {
    with_test_project_root("runtime-policy-success", |project_root, config_root| {
        let install_bin_dir = project_root.join("bin");
        fs::create_dir_all(&install_bin_dir).expect("create install bin dir");

        let result = profile_config::with_config_dir_override(config_root.clone(), || {
            crate::commands::install::runner::with_install_test_overrides(
                install_bin_dir.clone(),
                Ok(()),
                || {
                    run(
                        false,
                        Some(InstallProfile::Codex),
                        false,
                        false,
                        None,
                        &Vec::new(),
                    )
                },
            )
        });

        result.expect("success-path install should complete");

        let saved_profile =
            load_profile_from_path(&config_root.join("basidiocarp").join("profile.toml"))
                .expect("load saved profile");
        assert_eq!(saved_profile, Some(InstallProfile::Codex));

        let remembered = runtime_policy::load_policy_from_path(
            &config_root.join("basidiocarp").join("runtime-policy.toml"),
        )
        .expect("load runtime policy");
        let approval = remembered
            .iter()
            .find(|record| record.subject == "install-profile:codex")
            .expect("remembered approval for codex profile");
        assert_eq!(approval.scope, runtime_policy::PolicyScope::User);
        assert_eq!(approval.decision, runtime_policy::PolicyDecision::Allow);
        assert_eq!(
            approval.source,
            runtime_policy::DecisionSource::OperatorProfile
        );
    });
}

#[test]
fn test_render_install_success_summary_stages_next_step() {
    assert_eq!(
        render_install_success_summary(Some(InstallProfile::Codex), false),
        vec![
            "Installation complete.".to_string(),
            "Profile checkpoint: Codex host mode is saved for this project.".to_string(),
            "State: the local canopy is ready for host wiring".to_string(),
            "Next step: run `stipe init` to wire hosts and shared MCP state".to_string(),
            "Optional follow-up: run `stipe doctor` first if you want a status readout before wiring hosts".to_string(),
        ]
    );
}

#[test]
fn test_render_install_success_summary_mentions_manual_follow_up_when_needed() {
    assert_eq!(
        render_install_success_summary(Some(InstallProfile::Standard), true),
        vec![
            "Installation complete.".to_string(),
            "Profile checkpoint: standard profile is saved for this project.".to_string(),
            "State: the managed canopy is in place; finish the manual follow-up to complete setup"
                .to_string(),
            "Next step: run `stipe init` to wire hosts and shared MCP state".to_string(),
            "Optional follow-up: run `stipe doctor` first if you want a status readout before wiring hosts".to_string(),
        ]
    );
}

#[test]
fn test_platform_key_known() {
    let key = platform_key();
    assert_ne!(
        key, "unknown",
        "platform_key should return a known platform"
    );
    assert!(
        matches!(
            key,
            "aarch64-apple-darwin"
                | "x86_64-apple-darwin"
                | "aarch64-unknown-linux-musl"
                | "x86_64-unknown-linux-musl"
                | "x86_64-pc-windows-msvc"
        ),
        "platform_key returned unexpected value: {key}"
    );
}

fn with_test_project_root(label: &str, test: impl FnOnce(PathBuf, PathBuf)) {
    let project_root = temp_test_dir(label);
    let config_root = project_root.join("config-root");
    fs::create_dir_all(&project_root).expect("create project root");
    fs::create_dir_all(&config_root).expect("create config root");

    host_policy::with_project_root_override(project_root.clone(), || {
        runtime_policy::with_config_dir_override(config_root.clone(), || {
            test(project_root.clone(), config_root.clone())
        })
    });

    let _ = fs::remove_dir_all(project_root);
}

fn write_runtime_policy_file(project_root: &Path, content: &str) {
    let policy_dir = project_root.join(".basidiocarp");
    fs::create_dir_all(&policy_dir).expect("create policy dir");
    fs::write(
        policy_dir.join("stipe-runtime-policy.toml"),
        content.trim_start(),
    )
    .expect("write runtime policy");
}

fn write_user_runtime_policy_file(config_root: &Path, content: &str) {
    let policy_dir = config_root.join("basidiocarp");
    fs::create_dir_all(&policy_dir).expect("create user policy dir");
    fs::write(policy_dir.join("runtime-policy.toml"), content.trim_start())
        .expect("write user runtime policy");
}

fn temp_test_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("stipe-install-tests-{label}-{nanos}"))
}

#[test]
fn test_find_matching_asset_success() {
    let release = GitHubRelease {
        name: "mycelium".to_string(),
        version: "v0.1.0".to_string(),
        assets: vec![
            ReleaseAsset {
                name: "mycelium-aarch64-apple-darwin.tar.gz".to_string(),
                download_url: "https://example.com/1".to_string(),
            },
            ReleaseAsset {
                name: "mycelium-x86_64-linux-musl.tar.gz".to_string(),
                download_url: "https://example.com/2".to_string(),
            },
        ],
    };

    let asset = find_matching_asset(&release, "aarch64-apple-darwin");
    assert!(asset.is_ok());
    assert_eq!(asset.unwrap().name, "mycelium-aarch64-apple-darwin.tar.gz");
}

#[test]
fn test_find_matching_asset_missing_platform() {
    let release = GitHubRelease {
        name: "mycelium".to_string(),
        version: "v0.1.0".to_string(),
        assets: vec![ReleaseAsset {
            name: "mycelium-aarch64-apple-darwin.tar.gz".to_string(),
            download_url: "https://example.com/1".to_string(),
        }],
    };

    let asset = find_matching_asset(&release, "x86_64-pc-windows-msvc");
    assert!(asset.is_err());
    assert!(
        asset
            .err()
            .unwrap()
            .to_string()
            .contains("No tar.gz asset found")
    );
}

#[test]
fn test_find_matching_asset_requires_tar_gz() {
    let release = GitHubRelease {
        name: "mycelium".to_string(),
        version: "v0.1.0".to_string(),
        assets: vec![ReleaseAsset {
            name: "mycelium-aarch64-apple-darwin.zip".to_string(),
            download_url: "https://example.com/1".to_string(),
        }],
    };

    let asset = find_matching_asset(&release, "aarch64-apple-darwin");
    assert!(asset.is_err(), "Should reject non-tar.gz files");
}

#[test]
fn test_extract_tarball_with_binary() {
    let temp_dir = std::env::temp_dir().join("stipe-test-extract");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let tarball_path = temp_dir.join("test.tar.gz");
    {
        let tar_file = std::fs::File::create(&tarball_path).unwrap();
        let gz = flate2::write::GzEncoder::new(tar_file, flate2::Compression::default());
        let mut tar = tar::Builder::new(gz);

        let mut header = tar::Header::new_gnu();
        header.set_size(5);
        header.set_cksum();

        tar.append_data(&mut header, "mycelium", &b"hello"[..])
            .unwrap();
        tar.finish().unwrap();
    }

    let extract_dir = temp_dir.join("extract");
    let tarball_data = std::fs::read(&tarball_path).unwrap();
    let result = extract_tarball(&tarball_data, &extract_dir);

    assert!(result.is_ok(), "Extraction should succeed");
    let extracted_path = result.unwrap();
    assert_eq!(
        extracted_path.file_name().unwrap().to_str().unwrap(),
        "mycelium"
    );
    assert!(extracted_path.exists(), "Binary should be extracted");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_extract_tarball_missing_binary() {
    let temp_dir = std::env::temp_dir().join("stipe-test-extract-fail");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let tarball_path = temp_dir.join("test.tar.gz");
    {
        let tar_file = std::fs::File::create(&tarball_path).unwrap();
        let gz = flate2::write::GzEncoder::new(tar_file, flate2::Compression::default());
        let mut tar = tar::Builder::new(gz);

        let mut header = tar::Header::new_gnu();
        header.set_size(5);
        header.set_cksum();

        tar.append_data(&mut header, "unknown-binary", &b"hello"[..])
            .unwrap();
        tar.finish().unwrap();
    }

    let extract_dir = temp_dir.join("extract");
    let tarball_data = std::fs::read(&tarball_path).unwrap();
    let result = extract_tarball(&tarball_data, &extract_dir);

    assert!(
        result.is_err(),
        "Should fail when no recognized binary found"
    );
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("No binary found")
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}
