use anyhow::{Context, Result, anyhow};
use indicatif::ProgressBar;
use reqwest::blocking::Client;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::commands::tool_registry;

#[derive(Debug)]
pub(super) struct GitHubRelease {
    pub(super) name: String,
    pub(super) version: String,
    pub(super) assets: Vec<ReleaseAsset>,
}

#[derive(Debug)]
pub(super) struct ReleaseAsset {
    pub(super) name: String,
    pub(super) download_url: String,
}

pub(super) fn platform_key() -> &'static str {
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

fn release_repo(tool: &str) -> &str {
    tool_registry::find(tool).map_or(tool, |spec| spec.release_repo)
}

pub(super) fn fetch_latest_release(tool: &str, client: &Client) -> Result<GitHubRelease> {
    let repo = release_repo(tool);
    let url = format!("https://api.github.com/repos/basidiocarp/{repo}/releases/latest");
    let data = crate::commands::github::get_github_json(
        client,
        &url,
        &format!("latest release for {repo}"),
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

pub(super) fn find_matching_asset<'a>(
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

pub(super) fn download_binary(
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

pub(super) fn extract_tarball(data: &[u8], dest_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(dest_dir)
        .with_context(|| format!("Failed to create directory: {}", dest_dir.display()))?;

    let tar_gz = std::io::Cursor::new(data);
    let tar = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(tar);

    let mut binary_path = None;

    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        let path = entry.path()?.to_path_buf();

        if let Some(file_name) = path.file_name()
            && let Some(name_str) = file_name.to_str()
            && tool_registry::release_archive_binaries().contains(&name_str)
        {
            entry.unpack_in(dest_dir)?;
            binary_path = Some(dest_dir.join(file_name));
        }
    }

    binary_path.ok_or_else(|| anyhow!("No binary found in archive"))
}

pub(super) fn verify_binary(path: &Path) -> Result<String> {
    let output = Command::new(path)
        .arg("--version")
        .output()
        .with_context(|| format!("Failed to run {}", path.display()))?;

    if !output.status.success() {
        return Err(anyhow!("Binary verification failed for {}", path.display()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
