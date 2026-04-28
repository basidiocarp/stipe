use anyhow::Result;
use colored::Colorize;

use crate::commands::host_policy;
use crate::commands::host_policy::{HostConfigScope, HostMode};
use crate::commands::init;
use crate::commands::install;
use crate::commands::output;

use super::doctor_report::build_host_doctor_report;
use super::inventory::build_inventory;

pub(super) fn render_setup_preview(mode: HostMode) -> Vec<String> {
    let mut lines = vec![format!("Host setup preview | {}", mode.label())];
    lines.extend(output::render_footer(
        "preview only; the host rollout is staged but not applied",
        format!(
            "review the embedded install and init previews below, then rerun `stipe host setup {}` without `--dry-run` to apply the host flow",
            mode.client_flag()
        ),
        Some(format!(
            "run `stipe install --profile {} --dry-run` to inspect the install surface on its own",
            mode.install_profile().profile_name()
        )),
    ));
    lines
}

fn render_inventory(colorize: bool) -> Vec<String> {
    let inventory = build_inventory();
    let mut lines = vec![
        String::new(),
        if colorize {
            "Basidiocarp Host Inventory".bold().to_string()
        } else {
            "Basidiocarp Host Inventory".to_string()
        },
        "─".repeat(75),
        String::new(),
    ];

    for entry in inventory {
        let detection = if entry.detected {
            "detected"
        } else {
            "not detected"
        };
        let configured = if entry.configured {
            "configured"
        } else {
            "needs setup"
        };

        let detection = if colorize {
            if entry.detected {
                detection.green().to_string()
            } else {
                detection.yellow().to_string()
            }
        } else {
            detection.to_string()
        };
        let configured = if colorize {
            if entry.configured {
                configured.green().to_string()
            } else {
                configured.yellow().to_string()
            }
        } else {
            configured.to_string()
        };
        let client_flag = if colorize {
            entry.mode.client_flag().bold().to_string()
        } else {
            entry.mode.client_flag().to_string()
        };
        let adapter_label = if colorize {
            entry.adapter_label.dimmed().to_string()
        } else {
            entry.adapter_label.clone()
        };

        lines.push(format!("  {client_flag:<14} {detection:<14} {configured}"));
        lines.push(format!("  {:<14} {}", "", adapter_label));
        if let Some(path) = entry.config_path {
            let path = if colorize {
                path.dimmed().to_string()
            } else {
                path
            };
            lines.push(format!("  {:<14} {}", "", path));
        } else {
            let label = host_policy::host_config_label(entry.mode);
            let label = if colorize {
                label.dimmed().to_string()
            } else {
                label.to_string()
            };
            lines.push(format!("  {:<14} {}", "", label));
        }
        let detail = if colorize {
            entry.detail.dimmed().to_string()
        } else {
            entry.detail
        };
        lines.push(format!("  {:<14} {}", "", detail));
        lines.push(String::new());
    }

    lines
}

#[cfg(test)]
pub(super) fn render_list() -> Vec<String> {
    render_inventory(false)
}

pub fn run_list() {
    for line in render_inventory(true) {
        println!("{line}");
    }
}

pub fn run_setup(mode: HostMode, scope: HostConfigScope, dry_run: bool) -> Result<()> {
    if !host_policy::host_scope_supported(mode, scope) {
        return Err(anyhow::anyhow!(
            "{} does not support the '{}' scope",
            mode.label(),
            host_policy::scope_name(scope)
        ));
    }

    if dry_run {
        for (index, line) in render_setup_preview(mode).into_iter().enumerate() {
            if index == 0 || line.starts_with("Next step:") {
                println!("{}", line.bold());
            } else {
                println!("{}", line.dimmed());
            }
        }
        println!();
        install::run_embedded_preview(mode.install_profile())?;
        return init::run_embedded_preview(Some(mode.client_flag()), scope);
    }

    install::run(
        false,
        Some(mode.install_profile()),
        false,
        false,
        None,
        false,
        &[],
    )?;
    init::run(Some(mode.client_flag()), scope, false, false, false, false)
}

pub fn run_doctor(mode: Option<HostMode>, json: bool) -> Result<()> {
    let report = build_host_doctor_report(mode);

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    for line in render_doctor(&report, true) {
        println!("{line}");
    }

    Ok(())
}

pub(super) fn render_doctor(
    report: &crate::commands::host::model::HostDoctorReport,
    colorize: bool,
) -> Vec<String> {
    let mut lines = vec![
        String::new(),
        if colorize {
            "Basidiocarp Host Health".bold().to_string()
        } else {
            "Basidiocarp Host Health".to_string()
        },
        "─".repeat(75),
        String::new(),
    ];

    for check in &report.checks {
        let symbol = if check.passed { "✓" } else { "✗" };
        let message = if colorize {
            if check.passed {
                check.message.green().to_string()
            } else {
                check.message.red().to_string()
            }
        } else {
            check.message.clone()
        };
        let host = if colorize {
            check.host.client_flag().bold().to_string()
        } else {
            check.host.client_flag().to_string()
        };
        lines.push(format!("  {host:<14} {symbol} {message}"));
    }

    lines.push(String::new());
    if report.healthy {
        lines.extend(render_footer_lines(
            &report.summary,
            "continue with the current host configuration; no repair action is required",
            None,
            colorize,
        ));
    } else if let Some(primary) = report.repair_actions.first() {
        let optional_follow_up = report
            .repair_actions
            .iter()
            .skip(1)
            .find(|action| action.command != primary.command)
            .map(|action| format!("run `{}`", action.command));
        lines.extend(render_footer_lines(
            &report.summary,
            &format!("run `{}`", primary.command),
            optional_follow_up.clone(),
            colorize,
        ));

        let additional_actions = report
            .repair_actions
            .iter()
            .filter(|action| action.command != primary.command)
            .filter(|action| {
                optional_follow_up
                    .as_ref()
                    .is_none_or(|follow_up| follow_up != &format!("run `{}`", action.command))
            })
            .map(|action| format!("  - {}", action.command))
            .collect::<Vec<_>>();

        if !additional_actions.is_empty() {
            lines.push(String::new());
            lines.push(if colorize {
                "Additional repair actions:".bold().to_string()
            } else {
                "Additional repair actions:".to_string()
            });
            lines.extend(additional_actions);
        }
    }

    lines.push(String::new());

    lines
}

fn render_footer_lines(
    state: &str,
    next_step: &str,
    optional_follow_up: Option<String>,
    colorize: bool,
) -> Vec<String> {
    output::render_footer(state.to_string(), next_step.to_string(), optional_follow_up)
        .into_iter()
        .map(|line| {
            if colorize {
                if line.starts_with("Next step:") {
                    line.bold().to_string()
                } else {
                    line.dimmed().to_string()
                }
            } else {
                line
            }
        })
        .collect()
}
