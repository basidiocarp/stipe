use super::*;
use crate::commands::tool_registry;

#[test]
fn test_profile_tools_cover_expected_sets() {
    let minimal = tool_registry::specs_for_profile(InstallProfile::Minimal)
        .into_iter()
        .map(|spec| spec.name)
        .collect::<Vec<_>>();
    assert_eq!(minimal, vec!["mycelium"]);

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
        vec!["mycelium", "hyphae", "rhizome", "canopy", "cortina", "volva"]
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
            .any(|line| line.contains("mycelium") && line.contains("already exists"))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("hyphae") && line.contains("would be downloaded"))
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
            "Dry run: no changes will be made.".to_string(),
            String::new(),
            "  Mode: minimal profile".to_string(),
            format!(
                "  mycelium: would be skipped because {} already exists",
                temp_dir.join("mycelium").display()
            ),
            format!(
                "  hyphae: would be downloaded and installed to {}",
                temp_dir.join("hyphae").display()
            ),
        ]
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_render_install_preview_snapshot_for_interactive_mode() {
    assert_eq!(
        render_install_preview(std::path::Path::new("/tmp/tools"), &[], "minimal profile"),
        vec![
            "Dry run: no changes will be made.".to_string(),
            String::new(),
            "Interactive selection would be shown with all tools preselected.".to_string(),
            String::new(),
            "  mycelium        token compression proxy".to_string(),
            "  hyphae          agent memory system".to_string(),
            "  rhizome         code intelligence server".to_string(),
            "  canopy          coordination runtime".to_string(),
            "  cortina         hook runner & session tracking".to_string(),
            "  volva           backend operations CLI".to_string(),
        ]
    );
}

#[test]
fn test_profile_mode_labels_make_codex_explicit() {
    assert_eq!(InstallProfile::Codex.mode_label(), "Codex host mode");
    assert_eq!(
        InstallProfile::ClaudeCode.mode_label(),
        "Claude Code operator mode"
    );
    assert_eq!(
        InstallProfile::DeveloperTools.mode_label(),
        "developer-tools profile"
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
