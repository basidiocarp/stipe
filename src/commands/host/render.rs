use anyhow::Result;
use colored::Colorize;

use crate::commands::host_policy;
use crate::commands::host_policy::{HostConfigScope, HostMode};
use crate::commands::init;
use crate::commands::install;

use super::doctor_report::build_host_doctor_report;
use super::inventory::build_inventory;

pub fn run_list() {
    let inventory = build_inventory();

    println!();
    println!("{}", "Configured Hosts".bold());
    println!("{}", "─".repeat(75));
    println!();

    for entry in inventory {
        let detection = if entry.detected {
            "detected".green()
        } else {
            "not detected".yellow()
        };
        let configured = if entry.configured {
            "configured".green()
        } else {
            "needs setup".yellow()
        };

        println!(
            "  {:<14} {:<14} {}",
            entry.mode.client_flag().bold(),
            detection,
            configured
        );
        println!("  {:<14} {}", "", entry.adapter_label.dimmed());
        if let Some(path) = entry.config_path {
            println!("  {:<14} {}", "", path.dimmed());
        } else {
            println!(
                "  {:<14} {}",
                "",
                host_policy::host_config_label(entry.mode).dimmed()
            );
        }
        println!("  {:<14} {}", "", entry.detail.dimmed());
        println!();
    }
}

pub fn run_setup(mode: HostMode, scope: HostConfigScope, dry_run: bool) -> Result<()> {
    if !host_policy::host_scope_supported(mode, scope) {
        return Err(anyhow::anyhow!(
            "{} does not support the '{}' scope",
            mode.label(),
            match scope {
                HostConfigScope::User => "user",
                HostConfigScope::Project => "project",
                HostConfigScope::Local => "local",
            }
        ));
    }

    if dry_run {
        println!("{} {}", "Planning".bold(), mode.label().bold());
        println!(
            "{}",
            "This runs the matching install profile and then targets init at the selected host."
                .dimmed()
        );
        println!();
    }

    install::run(false, Some(mode.install_profile()), dry_run, &[])?;
    init::run(Some(mode.client_flag()), scope, dry_run, false)
}

pub fn run_doctor(mode: Option<HostMode>, json: bool) -> Result<()> {
    let report = build_host_doctor_report(mode);

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!();
    println!("{}", "Host Health".bold());
    println!("{}", "─".repeat(75));
    println!();

    for check in &report.checks {
        let status = if check.passed {
            format!("{} {}", "✓".green(), check.message.green())
        } else {
            format!("{} {}", "✗".red(), check.message.red())
        };

        println!("  {:<14} {}", check.host.client_flag().bold(), status);
    }

    println!();

    if !report.repair_actions.is_empty() {
        println!("{}", "Recommended repair actions:".bold());
        for action in &report.repair_actions {
            println!("  - {}", action.command);
        }
        println!();
    }

    Ok(())
}
