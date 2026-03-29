use colored::Colorize;

use super::model::{InitSnapshot, InitStepStatus};
use super::plan::build_plan;

pub(super) fn render_preview(snapshot: &InitSnapshot) -> Vec<String> {
    let plan = build_plan(snapshot, true);
    let mut lines = Vec::new();

    for step in plan.steps {
        let line = match step.status {
            InitStepStatus::Planned => format!("Would {}. {}", step.title, step.detail),
            InitStepStatus::AlreadyOk => format!("Already OK: {}. {}", step.title, step.detail),
            InitStepStatus::Skipped => format!("Would skip: {}. {}", step.title, step.detail),
        };
        lines.push(line);
    }

    lines
}

pub(super) fn print_preview(snapshot: &InitSnapshot) {
    println!("{}", "Dry run: no changes will be made.".yellow());
    println!();

    for line in render_preview(snapshot) {
        println!("  {line}");
    }

    println!();
}
