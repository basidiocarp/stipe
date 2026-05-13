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

fn format_install_preview_with_footer(
    prefix: &Path,
    tools: &[String],
    mode_label: &str,
    include_footer: bool,
) -> Vec<String> {
    let mut lines = vec![
        "Install preview | dry run".to_string(),
        String::new(),
        format!("Mode: {mode_label}"),
        "Plan:".to_string(),
    ];

    for tool in tools {
        let install_path = prefix.join(tool);
        if install_path.exists() {
            lines.push(format!(
                "  - {tool:<12} keep existing binary at {}",
                install_path.display()
            ));
        } else {
            lines.push(format!(
                "  - {tool:<12} install release to {}",
                install_path.display()
            ));
        }
    }

    if include_footer {
        lines.push(String::new());
        lines.push("Next step: run `stipe install ...` to apply this plan.".to_string());
    }

    lines
}

pub(super) fn format_install_preview(
    prefix: &Path,
    tools: &[String],
    mode_label: &str,
) -> Vec<String> {
    format_install_preview_with_footer(prefix, tools, mode_label, true)
}

pub(super) fn render_install_preview(
    prefix: &Path,
    tools: &[String],
    mode_label: &str,
) -> Vec<String> {
    if tools.is_empty() {
        let mut lines = vec![
            "Install preview | dry run".to_string(),
            String::new(),
            "Mode: interactive selection".to_string(),
            "Selection flow:".to_string(),
            "  Managed tools open with every entry preselected.".to_string(),
            "  Use space to toggle entries and enter to confirm.".to_string(),
            String::new(),
            "Available tools:".to_string(),
        ];
        lines.extend(
            tool_registry::installable_specs()
                .into_iter()
                .map(|spec| format!("  {:<15} {}", spec.name, spec.description)),
        );
        return lines;
    }

    format_install_preview(prefix, tools, mode_label)
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

fn render_profile_install_preview_with_footer(
    prefix: &Path,
    profile: InstallProfile,
    tools: &[String],
    include_footer: bool,
) -> Vec<String> {
    let (managed, manual) = split_requested_tools(tools);
    let mut lines = vec![
        "Install preview | dry run".to_string(),
        String::new(),
        format!("Profile: {}", profile.mode_label()),
        "Managed installs:".to_string(),
    ];

    for tool in managed {
        lines.push(format!(
            "  - {tool:<12} managed install to {}",
            prefix.join(&tool).display()
        ));
    }
    for member in manual {
        if !lines.iter().any(|line| line == "Manual follow-up:") {
            lines.push(String::new());
            lines.push("Manual follow-up:".to_string());
        }
        lines.push(format!("  - {} ({})", member.name, member.install_hint));
    }

    let skipped = expected_profile_tools(InstallProfile::FullStack)
        .into_iter()
        .filter(|candidate| !tools.iter().any(|tool| tool == candidate))
        .collect::<Vec<_>>();

    if !skipped.is_empty() {
        lines.push(String::new());
        lines.push("Not included in this profile:".to_string());
        for tool in skipped {
            lines.push(format!("  - {tool}"));
        }
    }

    if include_footer {
        lines.push(String::new());
        lines.push(format!(
            "Next step: run `stipe install --profile {}` to apply this plan.",
            profile.profile_name()
        ));
    }

    lines
}

#[cfg(test)]
pub(super) fn render_profile_install_preview(
    prefix: &Path,
    profile: InstallProfile,
    tools: &[String],
) -> Vec<String> {
    render_profile_install_preview_with_footer(prefix, profile, tools, true)
}

pub(super) fn render_embedded_profile_install_preview(
    prefix: &Path,
    profile: InstallProfile,
    tools: &[String],
) -> Vec<String> {
    render_profile_install_preview_with_footer(prefix, profile, tools, false)
}

pub(super) fn print_install_preview(prefix: &Path, tools: &[String], mode_label: &str) {
    let lines = render_install_preview(prefix, tools, mode_label);

    for (index, line) in lines.into_iter().enumerate() {
        if index == 0 {
            println!("{}", line.yellow());
        } else if line.ends_with(':') || line.starts_with("Mode:") {
            println!("{}", line.bold());
        } else {
            println!("{line}");
        }
    }
}
