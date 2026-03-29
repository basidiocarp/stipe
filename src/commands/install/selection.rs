use colored::Colorize;
use std::path::Path;

use crate::commands::install::InstallProfile;
use crate::commands::tool_registry;

fn unique_tools(base: Vec<String>, extras: &[String]) -> Vec<String> {
    let mut ordered = base;
    for tool in extras {
        if !ordered.iter().any(|existing| existing == tool) {
            ordered.push(tool.clone());
        }
    }
    ordered
}

pub(super) fn resolve_requested_tools(
    all: bool,
    profile: Option<InstallProfile>,
    tools: &[String],
) -> Option<Vec<String>> {
    if all {
        let known = tool_registry::install_all_specs()
            .into_iter()
            .map(|spec| spec.name.to_string())
            .collect();
        return Some(unique_tools(known, tools));
    }

    if let Some(profile) = profile {
        let selected = tool_registry::specs_for_profile(profile)
            .into_iter()
            .map(|spec| spec.name.to_string())
            .collect();
        return Some(unique_tools(selected, tools));
    }

    if !tools.is_empty() {
        return Some(tools.to_vec());
    }

    None
}

pub(super) fn format_install_preview(
    prefix: &Path,
    tools: &[String],
    mode_label: &str,
) -> Vec<String> {
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

pub(super) fn print_install_preview(prefix: &Path, tools: &[String], mode_label: &str) {
    println!("{}", "Dry run: no changes will be made.".yellow());
    println!();

    if tools.is_empty() {
        println!(
            "{}",
            "Interactive selection would be shown with all tools preselected.".bold()
        );
        println!();
        for spec in tool_registry::installable_specs() {
            println!("  {:<15} {}", spec.name, spec.description);
        }
        return;
    }

    for line in format_install_preview(prefix, tools, mode_label) {
        println!("  {line}");
    }
}
