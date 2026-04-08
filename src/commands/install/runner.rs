use anyhow::{Context, Result, anyhow};
use colored::Colorize;
use dialoguer::{MultiSelect, theme::ColorfulTheme};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::blocking::Client;
use spore::logging::{SpanContext, workflow_span};
use std::fs;
use std::path::{Path, PathBuf};

use crate::commands::bin_paths;
use crate::commands::developer_tools;
use crate::commands::github;
use crate::commands::install::release::{
    download_binary, extract_tarball, fetch_latest_release, find_matching_asset, platform_key,
    verify_binary, verify_functional,
};
use crate::commands::install::save_selected_profile;
use crate::commands::install::selection::{
    print_install_preview, print_profile_install_preview, resolve_requested_tools,
    split_requested_tools,
};
use crate::commands::tool_registry::{self, InstallProfile, ToolSpec};

pub(crate) fn install_tool(tool: &str, prefix: &Path, force: bool, client: &Client) -> Result<()> {
    let spec = tool_registry::find(tool).ok_or_else(|| anyhow!("Unknown tool: {tool}"))?;

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

    // macOS requires ad-hoc re-signing after copying a binary to a new location.
    // Without this, the linker signature is invalidated and macOS kills the binary
    // with SIGKILL (exit 137) on execution.
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("codesign")
            .args([
                "--force",
                "--sign",
                "-",
                install_path.to_str().unwrap_or(""),
            ])
            .output();
    }

    match verify_functional(&install_path, spec) {
        Ok(()) => {
            if spec.smoke_test_args.is_some() {
                println!("  {} {} functional check passed", "✓".green(), tool);
            }
        }
        Err(error) => {
            return Err(anyhow!("{tool} functional check failed: {error}"));
        }
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

/// Default root directory for local source checkouts.
fn default_monorepo_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("projects")
        .join("claude-mycelium")
}

/// Resolve the cargo install path for a tool inside the monorepo.
///
/// Some tools live inside workspace sub-crates (e.g. `hyphae-cli` inside
/// the hyphae workspace). This function maps tool names to their `--path`
/// argument for `cargo install`.
fn source_install_path(tool_name: &str, source_dir: &Path) -> PathBuf {
    match tool_name {
        "hyphae" => source_dir.join("crates").join("hyphae-cli"),
        "rhizome" => source_dir.join("crates").join("rhizome-cli"),
        _ => source_dir.to_path_buf(),
    }
}

/// Build and install a tool from local source using `cargo install --path`.
pub(crate) fn install_from_source(
    tool_name: &str,
    spec: &ToolSpec,
    source_dir: &Path,
) -> Result<String> {
    let cargo_toml = source_dir.join("Cargo.toml");
    if !cargo_toml.exists() {
        anyhow::bail!(
            "No Cargo.toml found at {} — verify the source directory exists",
            source_dir.display()
        );
    }

    let install_path = source_install_path(tool_name, source_dir);

    println!(
        "  {} Building {} from source ({})",
        "⏳".yellow(),
        tool_name,
        install_path.display()
    );

    let status = std::process::Command::new("cargo")
        .args(["install", "--path"])
        .arg(&install_path)
        .status()
        .with_context(|| format!("Failed to run cargo install for {tool_name}"))?;

    if !status.success() {
        anyhow::bail!("cargo install failed for {tool_name}");
    }

    let binary = which::which(tool_name)
        .with_context(|| format!("{tool_name} not found in PATH after cargo install"))?;
    let version = verify_binary(&binary)?;

    match verify_functional(&binary, spec) {
        Ok(()) => {
            if spec.smoke_test_args.is_some() {
                println!("  {} {} functional check passed", "✓".green(), tool_name);
            }
        }
        Err(error) => {
            return Err(anyhow!("{tool_name} functional check failed: {error}"));
        }
    }

    println!(
        "  {} {} installed from source: {} → {}",
        "✓".green(),
        tool_name,
        version,
        binary.display()
    );

    Ok(version)
}

pub(crate) fn install_bin_dir() -> Result<PathBuf> {
    bin_paths::local_bin_dir().ok_or_else(|| anyhow!("Could not determine local bin directory"))
}

#[allow(clippy::too_many_lines)]
pub(crate) fn run(
    all: bool,
    profile: Option<InstallProfile>,
    dry_run: bool,
    from_source: bool,
    source_dir: Option<PathBuf>,
    tools: &[String],
) -> Result<()> {
    let span_context = install_span_context();
    let _workflow_span = workflow_span("install", &span_context).entered();
    let prefix = install_bin_dir()?;
    let mut failures = Vec::new();

    crate::banner::print_banner();
    println!("{}", "Basidiocarp Ecosystem Installer".bold());
    println!("{}", "─".repeat(75));
    println!();

    if profile == Some(InstallProfile::DeveloperTools) {
        let unknown = developer_tools::unknown_requested_tools(tools);
        let report = developer_tools::install_report(tools);

        for line in developer_tools::render_install_advice(&report) {
            println!("{line}");
        }

        if !unknown.is_empty() {
            println!("Unknown developer tools:");
            for name in unknown {
                println!("  - {name}");
            }
            println!();
        }

        return Ok(());
    }

    let tools_to_install = resolve_requested_tools(all, profile, tools);

    if dry_run {
        match tools_to_install {
            Some(ref requested) => {
                if let Some(profile) = profile {
                    print_profile_install_preview(&prefix, profile, requested);
                } else {
                    let label = if all {
                        "all".to_string()
                    } else {
                        "explicit tools".to_string()
                    };
                    print_install_preview(&prefix, requested, &label);
                }
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
        let installable_specs = tool_registry::installable_specs();
        println!(
            "{}",
            "Select tools to install (all selected by default):".bold()
        );
        println!();

        let tool_items: Vec<(String, bool)> = installable_specs
            .iter()
            .map(|spec| (format!("{:<15} — {}", spec.name, spec.description), true))
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
            .map(|&idx| installable_specs[idx].name.to_string())
            .collect()
    };
    let (tools_to_install, manual_tools) = split_requested_tools(&tools_to_install);

    println!();

    if from_source {
        let monorepo_root = source_dir.unwrap_or_else(default_monorepo_root);

        for tool in &tools_to_install {
            let tool_source = monorepo_root.join(tool);
            let spec = tool_registry::find(tool);
            let Some(spec) = spec else {
                eprintln!("  {} Unknown tool: {}", "!".red(), tool);
                continue;
            };
            match install_from_source(tool, spec, &tool_source) {
                Ok(_version) => {}
                Err(error) => {
                    eprintln!(
                        "  {} Failed to build {} from source: {}",
                        "!".red(),
                        tool,
                        error
                    );
                    failures.push(format!("{tool}: {error}"));
                }
            }
        }
    } else {
        let client = github::github_client()?;

        for tool in &tools_to_install {
            match install_tool(tool, &prefix, false, &client) {
                Ok(()) => {}
                Err(error) => {
                    eprintln!("  {} Failed to install {}: {}", "!".red(), tool, error);
                    failures.push(format!("{tool}: {error}"));
                }
            }
        }
    }

    if let Some(profile) = profile.filter(|selected| *selected != InstallProfile::DeveloperTools)
        && let Some(config_path) = save_selected_profile(profile)?
    {
        println!();
        println!(
            "{} {} ({})",
            "✓".green(),
            format_args!("Saved install profile: {}", profile.mode_label()),
            config_path.display()
        );
    }

    if !manual_tools.is_empty() {
        println!();
        println!("{}", "Manual follow-up:".bold());
        for member in manual_tools {
            println!("  - {}: {}", member.name, member.install_hint);
        }
    }

    println!();

    if failures.is_empty() {
        println!(
            "{}",
            "Installation complete. Run 'stipe init' to configure.".green()
        );
        println!();
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "installation failed for {} tool(s): {}",
            failures.len(),
            failures.join("; ")
        ))
    }
}

fn install_span_context() -> SpanContext {
    let context = SpanContext::for_app("stipe");
    match crate::commands::host_policy::project_root().or_else(|| std::env::current_dir().ok()) {
        Some(path) => context.with_workspace_root(path.display().to_string()),
        None => context,
    }
}
