use anyhow::{Context, Result, anyhow};
use colored::Colorize;
use reqwest::blocking::Client;
use std::process::Command;

use super::install;
use super::tool_registry;
use super::tool_registry::InstallProfile;

fn get_installed_version(tool: &str) -> Result<String> {
    let output = Command::new(tool)
        .arg("--version")
        .output()
        .with_context(|| format!("Failed to get version for {tool}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "Failed to get version for {tool}: {}",
            stderr.trim()
        ));
    }

    let version_output = String::from_utf8_lossy(&output.stdout);
    let version = version_output
        .split_whitespace()
        .last()
        .ok_or_else(|| anyhow!("Empty version output from {tool}"))?;

    Ok(version.to_string())
}

fn fetch_latest_version(tool: &str, client: &Client) -> Result<String> {
    let repo = tool_registry::find(tool).map_or(tool, |spec| spec.release_repo);
    let url = format!("https://api.github.com/repos/basidiocarp/{repo}/releases/latest");
    let data = crate::commands::github::get_github_json(
        client,
        &url,
        &format!("latest release for {repo}"),
    )?;
    let version = data
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Could not parse version from GitHub release"))?
        .to_string();

    Ok(version)
}

struct UpdateInfo {
    installed: String,
    latest: String,
    update_available: bool,
    needs_reinstall: bool,
}

fn check_tool_update(tool: &str, client: &Client) -> Result<UpdateInfo> {
    let (installed, needs_reinstall) = if let Some(spec) = tool_registry::find(tool) {
        match tool_registry::probe(spec) {
            tool_registry::ToolProbe::Installed(version) => (version, false),
            tool_registry::ToolProbe::Broken => ("broken".to_string(), true),
            tool_registry::ToolProbe::Missing => {
                return Err(anyhow!("{tool} is not installed"));
            }
        }
    } else {
        (get_installed_version(tool)?, false)
    };
    let latest = fetch_latest_version(tool, client)?;

    let update_available = needs_reinstall || installed != latest;

    Ok(UpdateInfo {
        installed,
        latest,
        update_available,
        needs_reinstall,
    })
}

fn update_tool(tool: &str, client: &reqwest::blocking::Client) -> Result<()> {
    println!("  {} Checking for updates...", "⏳".yellow());

    let update_info = check_tool_update(tool, client)?;

    if !update_info.update_available {
        println!(
            "  {} {} is up to date ({})",
            "✓".green(),
            tool,
            update_info.installed
        );
        return Ok(());
    }

    if update_info.needs_reinstall {
        println!(
            "  {} {} is installed but broken → reinstall {}",
            "↑".cyan(),
            tool,
            update_info.latest
        );
    } else {
        println!(
            "  {} {} {} → {} available",
            "↑".cyan(),
            tool,
            update_info.installed,
            update_info.latest
        );
    }

    println!("  {} Downloading and installing...", "⏳".yellow());

    let prefix = install::install_bin_dir()?;

    super::install::install_tool(tool, &prefix, true, client)?;

    println!(
        "  {} {} updated to {}",
        "✓".green(),
        tool,
        update_info.latest
    );

    Ok(())
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

fn installed_profile_tools_with<F>(profile: InstallProfile, mut probe: F) -> Vec<String>
where
    F: FnMut(&tool_registry::ToolSpec) -> tool_registry::ToolProbe,
{
    tool_registry::specs_for_profile(profile)
        .into_iter()
        .filter_map(|spec| {
            probe(spec)
                .is_repairable_presence()
                .then_some(spec.name.to_string())
        })
        .collect()
}

fn resolve_requested_tools(
    all: bool,
    profile: Option<InstallProfile>,
    tools: &[String],
) -> Option<Vec<String>> {
    if all {
        let installed = tool_registry::update_all_specs()
            .into_iter()
            .filter_map(|spec| {
                tool_registry::probe(spec)
                    .is_repairable_presence()
                    .then_some(spec.name.to_string())
            })
            .collect::<Vec<_>>();
        return Some(unique_tools(installed, tools));
    }

    if let Some(profile) = profile {
        let installed = installed_profile_tools_with(profile, tool_registry::probe);
        return Some(unique_tools(installed, tools));
    }

    if !tools.is_empty() {
        return Some(tools.to_vec());
    }

    None
}

fn profile_flag_name(profile: InstallProfile) -> &'static str {
    match profile {
        InstallProfile::Minimal => "minimal",
        InstallProfile::ClaudeCode => "claude-code",
        InstallProfile::Codex => "codex",
        InstallProfile::Cursor => "cursor",
        InstallProfile::FullStack => "full-stack",
    }
}

#[allow(clippy::unnecessary_wraps)]
pub fn run(
    all: bool,
    profile: Option<InstallProfile>,
    check: bool,
    tools: &[String],
) -> Result<()> {
    println!();
    println!("{}", "Basidiocarp Ecosystem Update".bold());
    println!("{}", "─".repeat(75));
    println!();

    let tools_to_check: Vec<String> =
        if let Some(requested) = resolve_requested_tools(all, profile, tools) {
            if (all || profile.is_some()) && requested.is_empty() {
                if let Some(profile) = profile {
                    println!(
                        "No installed tools found for {}. Run 'stipe install --profile {}' first.",
                        profile.mode_label(),
                        profile_flag_name(profile)
                    );
                } else {
                    println!("No installed tools found. Run 'stipe install --all' first.");
                }
                println!();
                return Ok(());
            }

            requested
        } else {
            if all {
                println!("No installed tools found. Run 'stipe install --all' first.");
            } else {
                println!("Specify tools to update:");
                println!("  {} stipe update mycelium", "→".dimmed());
                println!("  {} stipe update hyphae rhizome canopy", "→".dimmed());
                println!("  {} stipe update --profile claude-code", "→".dimmed());
                println!("  {} stipe update --all", "→".dimmed());
                println!();
                println!("Check without installing:");
                println!("  {} stipe update --check --profile codex", "→".dimmed());
                println!("  {} stipe update --check --all", "→".dimmed());
            }
            println!();
            return Ok(());
        };

    let client = crate::commands::github::github_client()?;

    for tool in &tools_to_check {
        match check_tool_update(tool, &client) {
            Ok(info) => {
                if check {
                    if info.needs_reinstall {
                        println!(
                            "  {} {} is installed but broken → reinstall {}",
                            "!".yellow(),
                            tool,
                            info.latest
                        );
                    } else if info.update_available {
                        println!(
                            "  {} {} {} → {}",
                            "↑".cyan(),
                            tool,
                            info.installed,
                            info.latest
                        );
                    } else {
                        println!(
                            "  {} {} is up to date ({})",
                            "✓".green(),
                            tool,
                            info.installed
                        );
                    }
                } else if info.update_available {
                    if let Err(e) = update_tool(tool, &client) {
                        eprintln!("  {} Failed to update {}: {}", "!".red(), tool, e);
                    }
                } else {
                    println!(
                        "  {} {} is up to date ({})",
                        "✓".green(),
                        tool,
                        info.installed
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "  {} Failed to check {} for updates: {}",
                    "!".red(),
                    tool,
                    e
                );
            }
        }
    }

    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
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
}
