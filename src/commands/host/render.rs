use anyhow::Result;
use colored::Colorize;

use crate::commands::host_policy;
use crate::commands::host_policy::{HostConfigScope, HostMode};
use crate::commands::init;
use crate::commands::install;

use super::doctor_report::build_host_doctor_report;
use super::inventory::build_inventory;

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
        println!(
            "{}",
            format!("Host setup preview | {}", mode.label()).bold()
        );
        println!(
            "{}",
            "Roll out the matching install profile first, then aim init at the selected host."
                .dimmed()
        );
        println!(
            "{}",
            "No files change in preview mode; this is the operator checklist before launch."
                .dimmed()
        );
        println!();
    }

    install::run(
        false,
        Some(mode.install_profile()),
        dry_run,
        false,
        None,
        &[],
    )?;
    init::run(Some(mode.client_flag()), scope, dry_run, false, false)
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
    if !report.repair_actions.is_empty() {
        lines.push(if colorize {
            "Recommended repair actions:".bold().to_string()
        } else {
            "Recommended repair actions:".to_string()
        });
        lines.extend(
            report
                .repair_actions
                .iter()
                .map(|action| format!("  - {}", action.command)),
        );
        lines.push(String::new());
    }

    lines
}
