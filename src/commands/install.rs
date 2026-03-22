use anyhow::{Context, Result, anyhow};
use colored::Colorize;
use dialoguer::{MultiSelect, theme::ColorfulTheme};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
struct GitHubRelease {
    name: String,
    version: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone)]
struct ReleaseAsset {
    name: String,
    download_url: String,
}

fn get_platform_key() -> String {
    let os = if cfg!(target_os = "macos") {
        "apple-darwin"
    } else if cfg!(target_os = "linux") {
        "unknown-linux-musl"
    } else if cfg!(target_os = "windows") {
        "pc-windows-msvc"
    } else {
        "unknown"
    };

    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else {
        "unknown"
    };

    format!("{}-{}", arch, os)
}

fn fetch_latest_release(tool: &str) -> Result<GitHubRelease> {
    let url = format!(
        "https://api.github.com/repos/basidiocarp/{}/releases/latest",
        tool
    );

    let client = reqwest::blocking::Client::new();
    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .with_context(|| format!("Failed to fetch latest release for {}", tool))?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "GitHub API error for {}: {}",
            tool,
            response.status()
        ));
    }

    let data: serde_json::Value = response.json().context("Failed to parse release JSON")?;

    let version = data
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
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

fn find_matching_asset(release: &GitHubRelease, platform_key: &str) -> Result<ReleaseAsset> {
    release
        .assets
        .iter()
        .find(|asset| asset.name.contains(platform_key) && asset.name.ends_with(".tar.gz"))
        .cloned()
        .ok_or_else(|| {
            anyhow!(
                "No tar.gz asset found for {} on platform {}",
                release.name,
                platform_key
            )
        })
}

fn download_binary(asset: &ReleaseAsset, progress: &ProgressBar) -> Result<Vec<u8>> {
    let client = reqwest::blocking::Client::new();
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

        if path
            .file_name()
            .and_then(|n| n.to_str())
            .map_or(false, |n| {
                n == "mycelium" || n == "hyphae" || n == "rhizome" || n == "cortina" || n == "stipe"
            })
        {
            entry.unpack_in(dest_dir)?;
            binary_path = Some(dest_dir.join(path.file_name().unwrap()));
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

fn install_tool(tool: &str, prefix: &Path, force: bool) -> Result<()> {
    println!("  {} Fetching release information...", "⏳".yellow());

    let release = fetch_latest_release(tool)?;
    let platform_key = get_platform_key();
    let asset = find_matching_asset(&release, &platform_key)?;

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
    let data = download_binary(&asset, &progress)?;

    println!("  {} Extracting...", "⏳".yellow());
    let temp_dir = std::env::temp_dir().join(format!("stipe-{}", tool));
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

pub fn run(all: bool, tools: &[String]) -> Result<()> {
    let prefix = dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not determine home directory"))?
        .join(".local")
        .join("bin");

    println!();
    println!("{}", "Basidiocarp Ecosystem Installer".bold());
    println!("{}", "─".repeat(75));
    println!();

    let available_tools = vec![
        ("mycelium", "token compression proxy"),
        ("hyphae", "agent memory system"),
        ("rhizome", "code intelligence server"),
        ("cortina", "hook runner & session tracking"),
    ];

    let tools_to_install: Vec<&str> = if all {
        available_tools.iter().map(|(name, _)| *name).collect()
    } else if !tools.is_empty() {
        tools.iter().map(|s| s.as_str()).collect()
    } else {
        let theme = ColorfulTheme::default();
        println!(
            "{}",
            "Select tools to install (all selected by default):".bold()
        );
        println!();

        let tool_items: Vec<(String, bool)> = available_tools
            .iter()
            .map(|(name, desc)| (format!("{:<15} — {}", name, desc), true))
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
            .map(|&idx| available_tools[idx].0)
            .collect()
    };

    println!();

    for tool in tools_to_install {
        match install_tool(tool, &prefix, false) {
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
