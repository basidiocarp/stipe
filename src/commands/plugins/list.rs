//! `stipe plugins list` and `stipe plugins status` implementations.

use anyhow::Result;
use std::fs;

use super::state::load_disabled;

fn plugins_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|home| home.join(".claude/plugins/lamella"))
}

/// List installed plugins with their enabled/disabled status.
pub fn run() -> Result<()> {
    let Some(dir) = plugins_dir() else {
        println!("Could not determine home directory.");
        return Ok(());
    };

    let disabled = load_disabled();

    if !dir.exists() {
        println!(
            "No plugins installed (directory not found: {}).",
            dir.display()
        );
        println!("Run 'stipe plugins install --ecosystem' to install the ecosystem plugin set.");
        return Ok(());
    }

    let mut plugins: Vec<String> = fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    plugins.sort();

    if plugins.is_empty() {
        println!("No plugins installed in {}.", dir.display());
        println!("Run 'stipe plugins install --ecosystem' to install the ecosystem plugin set.");
        return Ok(());
    }

    println!("Installed plugins ({}):", plugins.len());
    for plugin in &plugins {
        let status = if disabled.contains(plugin) {
            "disabled"
        } else {
            "enabled"
        };
        println!("  {plugin:<30} {status}");
    }

    Ok(())
}

/// Print a short health summary: counts of installed, enabled, disabled plugins.
pub fn status() {
    let Some(dir) = plugins_dir() else {
        println!("Could not determine home directory.");
        return;
    };

    let disabled = load_disabled();

    let installed_count = if dir.exists() {
        fs::read_dir(&dir).map_or(0, |entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().is_dir())
                .count()
        })
    } else {
        0
    };

    let disabled_count = disabled.len();
    let enabled_count = installed_count.saturating_sub(disabled_count);
    let ecosystem_installed = dir.exists() && dir.join("ecosystem").exists();

    println!(
        "Plugins: {installed_count} installed, {enabled_count} enabled, {disabled_count} disabled"
    );
    if ecosystem_installed {
        println!("  ecosystem plugin: installed");
    } else {
        println!("  ecosystem plugin: not installed (run 'stipe plugins install --ecosystem')");
    }
}
