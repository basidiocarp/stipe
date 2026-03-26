use anyhow::{Context, Result, anyhow};
use clap::ValueEnum;
use colored::Colorize;
use dialoguer::{MultiSelect, theme::ColorfulTheme};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::blocking::Client;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::bin_paths;
use super::host_policy;

const TOOLS: &[(&str, &str)] = &[
    ("mycelium", "token compression proxy"),
    ("hyphae", "agent memory system"),
    ("rhizome", "code intelligence server"),
    ("cortina", "hook runner & session tracking"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum InstallProfile {
    Minimal,
    ClaudeCode,
    Codex,
    Cursor,
    FullStack,
}

impl InstallProfile {
    fn mode_label(self) -> &'static str {
        match self {
            Self::Minimal => "minimal profile",
            Self::ClaudeCode => host_policy::CLAUDE_CODE_HOST_MODE_LABEL,
            Self::Codex => host_policy::CODEX_HOST_MODE_LABEL,
            Self::Cursor => "Cursor profile",
            Self::FullStack => "full-stack profile",
        }
    }

    fn tools(self) -> &'static [&'static str] {
        match self {
            Self::Minimal => &["mycelium"],
            Self::ClaudeCode | Self::FullStack => &["mycelium", "hyphae", "rhizome", "cortina"],
            Self::Codex | Self::Cursor => &["mycelium", "hyphae", "rhizome"],
        }
    }
}

#[derive(Debug)]
struct GitHubRelease {
    name: String,
    version: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug)]
struct ReleaseAsset {
    name: String,
    download_url: String,
}

fn unique_tools(base: Vec<String>, extras: &[String]) -> Vec<String> {
    let mut ordered = base;
    for tool in extras {
        if !ordered.iter().any(|existing| existing == tool) {
            ordered.push(tool.clone());
        }
    }
    ordered
}

fn resolve_requested_tools(
    all: bool,
    profile: Option<InstallProfile>,
    tools: &[String],
) -> Option<Vec<String>> {
    if all {
        let known = TOOLS.iter().map(|(name, _)| (*name).to_string()).collect();
        return Some(unique_tools(known, tools));
    }

    if let Some(profile) = profile {
        let selected = profile
            .tools()
            .iter()
            .map(|tool| (*tool).to_string())
            .collect();
        return Some(unique_tools(selected, tools));
    }

    if !tools.is_empty() {
        return Some(tools.to_vec());
    }

    None
}

fn format_install_preview(prefix: &Path, tools: &[String], mode_label: &str) -> Vec<String> {
    let mut lines = vec![format!("Mode: {mode_label}")];

    for tool in tools {
        let install_path = prefix.join(tool);
        if install_path.exists() {
            lines.push(format!(
                "{tool}: would be skipped because {} already exists",
                install_path.display()
            ));
        } else {
            lines.push(format!(
                "{tool}: would be downloaded and installed to {}",
                install_path.display()
            ));
        }
    }

    lines
}

fn print_install_preview(prefix: &Path, tools: &[String], mode_label: &str) {
    println!("{}", "Dry run: no changes will be made.".yellow());
    println!();

    if tools.is_empty() {
        println!(
            "{}",
            "Interactive selection would be shown with all tools preselected.".bold()
        );
        println!();
        for (tool, description) in TOOLS {
            println!("  {tool:<15} {description}");
        }
        return;
    }

    for line in format_install_preview(prefix, tools, mode_label) {
        println!("  {line}");
    }
}

fn platform_key() -> &'static str {
    if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_arch = "x86_64", target_os = "macos")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(target_arch = "aarch64", target_os = "linux")) {
        "aarch64-unknown-linux-musl"
    } else if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        "x86_64-unknown-linux-musl"
    } else if cfg!(all(target_arch = "x86_64", target_os = "windows")) {
        "x86_64-pc-windows-msvc"
    } else {
        "unknown"
    }
}

fn fetch_latest_release(tool: &str, client: &Client) -> Result<GitHubRelease> {
    let url = format!("https://api.github.com/repos/basidiocarp/{tool}/releases/latest");
    let data = crate::commands::github::get_github_json(
        client,
        &url,
        &format!("latest release for {tool}"),
    )?;

    let version = data
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("GitHub release missing 'tag_name' field"))?
        .to_string();

    let assets: Vec<ReleaseAsset> = data
        .get("assets")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|asset| {
                    let name = asset.get("name")?.as_str()?;
                    let download_url = asset.get("browser_download_url")?.as_str()?;
                    Some(ReleaseAsset {
                        name: name.to_string(),
                        download_url: download_url.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(GitHubRelease {
        name: tool.to_string(),
        version,
        assets,
    })
}

fn find_matching_asset<'a>(
    release: &'a GitHubRelease,
    platform_key: &str,
) -> Result<&'a ReleaseAsset> {
    release
        .assets
        .iter()
        .find(|asset| asset.name.contains(platform_key) && asset.name.ends_with(".tar.gz"))
        .ok_or_else(|| {
            anyhow!(
                "No tar.gz asset found for {} on platform {}",
                release.name,
                platform_key
            )
        })
}

fn download_binary(
    asset: &ReleaseAsset,
    progress: &ProgressBar,
    client: &Client,
) -> Result<Vec<u8>> {
    let response = client
        .get(&asset.download_url)
        .send()
        .with_context(|| format!("Failed to download {}", asset.name))?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Download failed for {}: {}",
            asset.name,
            response.status()
        ));
    }

    let total_size = response.content_length().unwrap_or(0);
    progress.set_length(total_size);

    let bytes = response.bytes().context("Failed to read response body")?;

    progress.finish();
    Ok(bytes.to_vec())
}

fn extract_tarball(data: &[u8], dest_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(dest_dir)
        .with_context(|| format!("Failed to create directory: {}", dest_dir.display()))?;

    let tar_gz = std::io::Cursor::new(data);
    let tar = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(tar);

    let mut binary_path = None;

    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        let path = entry.path()?.to_path_buf();

        if let Some(file_name) = path.file_name() {
            if let Some(name_str) = file_name.to_str() {
                if name_str == "mycelium"
                    || name_str == "hyphae"
                    || name_str == "rhizome"
                    || name_str == "cortina"
                    || name_str == "stipe"
                {
                    entry.unpack_in(dest_dir)?;
                    binary_path = Some(dest_dir.join(file_name));
                }
            }
        }
    }

    binary_path.ok_or_else(|| anyhow!("No binary found in archive"))
}

fn verify_binary(path: &Path) -> Result<String> {
    let output = Command::new(path)
        .arg("--version")
        .output()
        .with_context(|| format!("Failed to run {}", path.display()))?;

    if !output.status.success() {
        return Err(anyhow!("Binary verification failed for {}", path.display()));
    }

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(version)
}

pub fn install_tool(tool: &str, prefix: &Path, force: bool, client: &Client) -> Result<()> {
    println!("  {} Fetching release information...", "⏳".yellow());

    let release = fetch_latest_release(tool, client)?;
    let platform_key = platform_key();
    let asset = find_matching_asset(&release, platform_key)?;

    println!("  {} Found {}: {}", "✓".green(), tool, release.version);

    let install_path = prefix.join(tool);

    if install_path.exists() && !force {
        println!(
            "  {} {} already installed at {}. Use --force to replace.",
            "⊘".yellow(),
            tool,
            install_path.display()
        );
        return Ok(());
    }

    println!("  {} Downloading {}...", "⏳".yellow(), asset.name);
    let progress = ProgressBar::new(0);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("{bar:30.cyan/blue} {bytes}/{total_bytes}")
            .unwrap()
            .progress_chars("=>-"),
    );
    let data = download_binary(asset, &progress, client)?;

    println!("  {} Extracting...", "⏳".yellow());
    let temp_dir = std::env::temp_dir().join(format!("stipe-{tool}"));
    let extracted_path = extract_tarball(&data, &temp_dir)?;

    println!("  {} Verifying...", "⏳".yellow());
    let version = verify_binary(&extracted_path)?;

    fs::copy(&extracted_path, &install_path).with_context(|| {
        format!(
            "Failed to copy {} to {}",
            extracted_path.display(),
            install_path.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&install_path, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("Failed to make {} executable", install_path.display()))?;
    }

    println!(
        "  {} {} installed: {} → {}",
        "✓".green(),
        tool,
        version,
        install_path.display()
    );

    Ok(())
}

pub fn install_bin_dir() -> Result<PathBuf> {
    bin_paths::local_bin_dir().ok_or_else(|| anyhow!("Could not determine local bin directory"))
}

pub fn run(
    all: bool,
    profile: Option<InstallProfile>,
    dry_run: bool,
    tools: &[String],
) -> Result<()> {
    let prefix = install_bin_dir()?;

    crate::banner::print_banner();
    println!("{}", "Basidiocarp Ecosystem Installer".bold());
    println!("{}", "─".repeat(75));
    println!();

    let tools_to_install = resolve_requested_tools(all, profile, tools);

    if dry_run {
        if let Some(profile) = profile {
            println!("Selected mode: {}", profile.mode_label().bold());
            println!();
        }

        match tools_to_install {
            Some(ref requested) => {
                let label = if all {
                    "all".to_string()
                } else if let Some(profile) = profile {
                    profile.mode_label().to_string()
                } else {
                    "explicit tools".to_string()
                };
                print_install_preview(&prefix, requested, &label);
            }
            None => {
                print_install_preview(&prefix, &[], "interactive selection");
            }
        }

        println!();
        return Ok(());
    }

    let tools_to_install: Vec<String> = if let Some(tools) = tools_to_install {
        tools
    } else {
        let theme = ColorfulTheme::default();
        println!(
            "{}",
            "Select tools to install (all selected by default):".bold()
        );
        println!();

        let tool_items: Vec<(String, bool)> = TOOLS
            .iter()
            .map(|(name, desc)| (format!("{name:<15} — {desc}"), true))
            .collect();

        let selections = MultiSelect::with_theme(&theme)
            .items_checked(&tool_items)
            .interact()?;

        if selections.is_empty() {
            println!();
            println!("{}", "No tools selected. Exiting.".yellow());
            println!();
            return Ok(());
        }

        selections
            .iter()
            .map(|&idx| TOOLS[idx].0.to_string())
            .collect()
    };

    println!();

    let client = crate::commands::github::github_client()?;

    for tool in &tools_to_install {
        match install_tool(tool, &prefix, false, &client) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("  {} Failed to install {}: {}", "!".red(), tool, e);
            }
        }
    }

    println!();
    println!(
        "{}",
        "Installation complete. Run 'stipe init' to configure.".green()
    );
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_tools_cover_expected_sets() {
        assert_eq!(InstallProfile::Minimal.tools(), &["mycelium"]);
        assert_eq!(
            InstallProfile::ClaudeCode.tools(),
            &["mycelium", "hyphae", "rhizome", "cortina"]
        );
        assert_eq!(
            InstallProfile::Codex.tools(),
            &["mycelium", "hyphae", "rhizome"]
        );
        assert_eq!(
            InstallProfile::Cursor.tools(),
            &["mycelium", "hyphae", "rhizome"]
        );
        assert_eq!(
            InstallProfile::FullStack.tools(),
            &["mycelium", "hyphae", "rhizome", "cortina"]
        );
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
                "cortina".to_string(),
            ]
        );
    }

    #[test]
    fn test_format_install_preview_reports_existing_and_missing_tools() {
        let temp_dir = std::env::temp_dir().join("stipe-install-preview");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        fs::write(temp_dir.join("mycelium"), "").unwrap();

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

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_profile_mode_labels_make_codex_explicit() {
        assert_eq!(InstallProfile::Codex.mode_label(), "Codex host mode");
        assert_eq!(
            InstallProfile::ClaudeCode.mode_label(),
            "Claude Code operator mode"
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
            "platform_key returned unexpected value: {}",
            key
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
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let tarball_path = temp_dir.join("test.tar.gz");
        {
            let tar_file = fs::File::create(&tarball_path).unwrap();
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
        let tarball_data = fs::read(&tarball_path).unwrap();
        let result = extract_tarball(&tarball_data, &extract_dir);

        assert!(result.is_ok(), "Extraction should succeed");
        let extracted_path = result.unwrap();
        assert_eq!(
            extracted_path.file_name().unwrap().to_str().unwrap(),
            "mycelium"
        );
        assert!(extracted_path.exists(), "Binary should be extracted");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_extract_tarball_missing_binary() {
        let temp_dir = std::env::temp_dir().join("stipe-test-extract-fail");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let tarball_path = temp_dir.join("test.tar.gz");
        {
            let tar_file = fs::File::create(&tarball_path).unwrap();
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
        let tarball_data = fs::read(&tarball_path).unwrap();
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

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
