use colored::Colorize;
use std::path::Path;

use super::profile_surface::{ManualProfileMember, expected_profile_tools, manual_member};
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
        let selected = expected_profile_tools(profile);
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

pub(super) fn render_install_preview(
    prefix: &Path,
    tools: &[String],
    mode_label: &str,
) -> Vec<String> {
    let mut lines = vec![
        "Dry run: no changes will be made.".to_string(),
        String::new(),
    ];

    if tools.is_empty() {
        lines.push("Interactive selection would be shown with all tools preselected.".to_string());
        lines.push(String::new());
        lines.extend(
            tool_registry::installable_specs()
                .into_iter()
                .map(|spec| format!("  {:<15} {}", spec.name, spec.description)),
        );
        return lines;
    }

    lines.extend(
        format_install_preview(prefix, tools, mode_label)
            .into_iter()
            .map(|line| format!("  {line}")),
    );
    lines
}

pub(super) fn split_requested_tools(tools: &[String]) -> (Vec<String>, Vec<ManualProfileMember>) {
    let mut managed: Vec<String> = Vec::new();
    let mut manual: Vec<ManualProfileMember> = Vec::new();

    for tool in tools {
        if let Some(member) = manual_member(tool) {
            if !manual.iter().any(|existing| existing.name == member.name) {
                manual.push(member);
            }
        } else if !managed.iter().any(|existing| existing == tool) {
            managed.push(tool.clone());
        }
    }

    (managed, manual)
}

pub(super) fn render_profile_install_preview(
    prefix: &Path,
    profile: InstallProfile,
    tools: &[String],
) -> Vec<String> {
    let (managed, manual) = split_requested_tools(tools);
    let mut lines = vec![
        "Dry run: no changes will be made.".to_string(),
        String::new(),
        format!("Profile: {}", profile.mode_label()),
        "Would install:".to_string(),
    ];

    for tool in managed {
        lines.push(format!(
            "  {tool}: managed install to {}",
            prefix.join(&tool).display()
        ));
    }
    for member in manual {
        lines.push(format!(
            "  {}: manual follow-up ({})",
            member.name, member.install_hint
        ));
    }

    let skipped = expected_profile_tools(InstallProfile::FullStack)
        .into_iter()
        .filter(|candidate| !tools.iter().any(|tool| tool == candidate))
        .collect::<Vec<_>>();

    if !skipped.is_empty() {
        lines.push("Would skip:".to_string());
        for tool in skipped {
            lines.push(format!(
                "  {tool}: not in profile {}",
                profile.profile_name()
            ));
        }
    }

    lines
}

pub(super) fn print_install_preview(prefix: &Path, tools: &[String], mode_label: &str) {
    let lines = render_install_preview(prefix, tools, mode_label);

    for (index, line) in lines.into_iter().enumerate() {
        if index == 0 {
            println!("{}", line.yellow());
        } else if !tools.is_empty() && line.starts_with("  Mode:") {
            println!("{line}");
        } else if tools.is_empty() && index == 2 {
            println!("{}", line.bold());
        } else {
            println!("{line}");
        }
    }
}

pub(super) fn print_profile_install_preview(
    prefix: &Path,
    profile: InstallProfile,
    tools: &[String],
) {
    for (index, line) in render_profile_install_preview(prefix, profile, tools)
        .into_iter()
        .enumerate()
    {
        if index == 0 {
            println!("{}", line.yellow());
        } else if line.starts_with("Profile:") || line.ends_with(':') {
            println!("{}", line.bold());
        } else {
            println!("{line}");
        }
    }
}
