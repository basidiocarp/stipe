use colored::Colorize;

use crate::commands::output;

use super::model::{InitSnapshot, InitStepStatus};
use super::plan::build_plan;

fn preview_target_label(target_client: &str) -> String {
    match target_client {
        "codex" => "Codex host mode".to_string(),
        "claude-code" => "Claude Code operator mode".to_string(),
        "cursor" => "Cursor mode".to_string(),
        other => other.to_string(),
    }
}

fn render_preview_with_footer(snapshot: &InitSnapshot, include_footer: bool) -> Vec<String> {
    let plan = build_plan(snapshot, true);
    let mut lines = vec![
        "Init preview | host bootstrap".to_string(),
        "Preview: dry run only; no files will change.".to_string(),
        String::new(),
    ];

    if let Some(target_client) = &snapshot.target_client {
        lines.push(format!("Target: {}", preview_target_label(target_client)));
    } else if !snapshot.detected_clients.is_empty() {
        lines.push(format!(
            "Detected clients: {}",
            snapshot.detected_clients.join(", ")
        ));
    } else {
        lines.push("Target: inferred from the local host inventory".to_string());
    }

    lines.push("Plan:".to_string());

    for step in plan.steps {
        let line = match step.status {
            InitStepStatus::Planned => format!("  - stage {}. {}", step.title, step.detail),
            InitStepStatus::AlreadyOk => format!("  - keep {}. {}", step.title, step.detail),
            InitStepStatus::Skipped => format!("  - skip {}. {}", step.title, step.detail),
        };
        lines.push(line);
    }

    if include_footer {
        lines.push(String::new());
        lines.extend(output::render_footer(
            "preview only; the host plan is staged but not applied",
            "run `stipe init` to apply the staged host plan",
            Some(
                "run `stipe doctor` first if you want a status readout before you apply it"
                    .to_string(),
            ),
        ));
    }

    lines
}

pub(super) fn render_preview(snapshot: &InitSnapshot) -> Vec<String> {
    render_preview_with_footer(snapshot, true)
}

pub(super) fn render_embedded_preview(snapshot: &InitSnapshot) -> Vec<String> {
    render_preview_with_footer(snapshot, false)
}

pub(super) fn print_preview(snapshot: &InitSnapshot) {
    for line in render_preview(snapshot) {
        if line == "Init preview | host bootstrap" {
            println!("{}", line.yellow());
        } else if line.starts_with("Preview:")
            || line.starts_with("State:")
            || line.starts_with("Optional follow-up:")
        {
            println!("{}", line.dimmed());
        } else if line.starts_with("Next step:") || line.ends_with(':') {
            println!("{}", line.bold());
        } else {
            println!("{line}");
        }
    }

    println!();
}

pub(super) fn print_embedded_preview(snapshot: &InitSnapshot) {
    for line in render_embedded_preview(snapshot) {
        if line == "Init preview | host bootstrap" {
            println!("{}", line.yellow());
        } else if line.starts_with("Preview:") {
            println!("{}", line.dimmed());
        } else if line.ends_with(':') {
            println!("{}", line.bold());
        } else {
            println!("{line}");
        }
    }

    println!();
}
