use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use serde::Serialize;

use super::host_policy;
use super::host_policy::{HostAdapterKind, HostMode};
use super::init;
use super::install;
use super::repair::{RepairAction, dedupe_repair_actions};
use crate::ecosystem::clients::{self, McpClient};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostInventoryEntry {
    pub mode: HostMode,
    pub label: String,
    pub adapter_kind: HostAdapterKind,
    pub adapter_label: String,
    pub detected: bool,
    pub configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostDoctorCheck {
    pub host: HostMode,
    pub passed: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub repair_actions: Vec<RepairAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostDoctorReport {
    pub healthy: bool,
    pub summary: String,
    pub checks: Vec<HostDoctorCheck>,
    pub repair_actions: Vec<RepairAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum HostCommand {
    /// List known hosts and whether they are currently detected/configured
    List,

    /// Install and initialize a single host without assuming it is the only one
    Setup {
        /// Host to configure
        #[arg(value_enum)]
        mode: HostMode,

        /// Show what would change without mutating the machine
        #[arg(long)]
        dry_run: bool,
    },

    /// Check one host, or all known hosts, without collapsing them into one mode
    Doctor {
        /// Optional host to inspect
        #[arg(value_enum)]
        mode: Option<HostMode>,

        /// Emit structured JSON instead of human-readable text
        #[arg(long)]
        json: bool,
    },

    #[command(hide = true, name = "claude-code")]
    LegacyClaudeCode {
        #[arg(long)]
        dry_run: bool,
    },

    #[command(hide = true, name = "codex")]
    LegacyCodex {
        #[arg(long)]
        dry_run: bool,
    },

    #[command(hide = true, name = "cursor")]
    LegacyCursor {
        #[arg(long)]
        dry_run: bool,
    },
}

pub fn run(command: HostCommand) -> Result<()> {
    match command {
        HostCommand::List => run_list(),
        HostCommand::Setup { mode, dry_run } => run_setup(mode, dry_run),
        HostCommand::Doctor { mode, json } => run_doctor(mode, json),
        HostCommand::LegacyClaudeCode { dry_run } => run_setup(HostMode::ClaudeCode, dry_run),
        HostCommand::LegacyCodex { dry_run } => run_setup(HostMode::Codex, dry_run),
        HostCommand::LegacyCursor { dry_run } => run_setup(HostMode::Cursor, dry_run),
    }
}

pub fn build_inventory() -> Vec<HostInventoryEntry> {
    let detected_clients = clients::detect_clients();

    host_policy::supported_host_modes()
        .iter()
        .copied()
        .map(|mode| inventory_entry(mode, &detected_clients))
        .collect()
}

fn inventory_entry(mode: HostMode, detected_clients: &[McpClient]) -> HostInventoryEntry {
    let descriptor = mode.descriptor();
    let config_path = host_policy::host_config_path(mode);
    let config_exists = config_path.as_ref().is_some_and(|path| path.exists());
    let detected = host_policy::host_detected_with_clients(mode, detected_clients) || config_exists;
    let configured = host_configured(mode, config_exists);

    HostInventoryEntry {
        mode,
        label: descriptor.display_name.to_string(),
        adapter_kind: descriptor.adapter_kind,
        adapter_label: descriptor.adapter_kind.label().to_string(),
        detected,
        configured,
        config_path: config_path.map(|path| host_policy::format_user_path(&path)),
        detail: host_detail(mode, detected, configured, config_exists),
    }
}

fn host_configured(mode: HostMode, config_exists: bool) -> bool {
    match mode {
        HostMode::Codex => host_policy::codex_notify_configured(),
        HostMode::ClaudeCode | HostMode::Cursor => config_exists,
    }
}

fn host_detail(mode: HostMode, detected: bool, configured: bool, config_exists: bool) -> String {
    match mode {
        HostMode::Codex => {
            if !detected {
                "Codex is not detected on this machine yet.".to_string()
            } else {
                host_policy::codex_notify_detail(configured)
            }
        }
        HostMode::ClaudeCode => {
            if configured {
                "Claude Code config is present and ready for per-host setup.".to_string()
            } else if detected {
                format!(
                    "Claude Code is detected, but no {} was found yet.",
                    host_policy::host_config_display_path(mode)
                )
            } else {
                "Claude Code is not detected on this machine yet.".to_string()
            }
        }
        HostMode::Cursor => {
            if configured {
                "Cursor MCP config is present and ready for per-host setup.".to_string()
            } else if detected || config_exists {
                format!(
                    "Cursor is detected, but no {} was found yet.",
                    host_policy::host_config_display_path(mode)
                )
            } else {
                "Cursor is not detected on this machine yet.".to_string()
            }
        }
    }
}

fn setup_repair_action(mode: HostMode) -> RepairAction {
    host_policy::host_setup_repair_action(mode)
}

pub fn build_host_doctor_report(mode: Option<HostMode>) -> HostDoctorReport {
    let inventory = build_inventory();
    let selected = inventory
        .into_iter()
        .filter(|entry| mode.is_none_or(|selected_mode| selected_mode == entry.mode))
        .collect::<Vec<_>>();

    let checks = selected
        .iter()
        .flat_map(|entry| doctor_checks_for_entry(entry).into_iter())
        .collect::<Vec<_>>();
    let healthy = checks.iter().all(|check| check.passed);
    let failing = checks.iter().filter(|check| !check.passed).count();
    let repair_actions = dedupe_repair_actions(
        checks
            .iter()
            .flat_map(|check| check.repair_actions.clone())
            .collect(),
    );

    HostDoctorReport {
        healthy,
        summary: if healthy {
            match mode {
                Some(selected_mode) => format!("{} is ready.", selected_mode.label()),
                None => "All selected host checks passed.".to_string(),
            }
        } else {
            format!("{failing} host checks need attention.")
        },
        checks,
        repair_actions,
    }
}

fn doctor_checks_for_entry(entry: &HostInventoryEntry) -> Vec<HostDoctorCheck> {
    let setup_action = setup_repair_action(entry.mode);
    let mut checks = vec![HostDoctorCheck {
        host: entry.mode,
        passed: entry.detected,
        message: if entry.detected {
            format!("{} detected on this machine", entry.label)
        } else {
            format!("{} is not detected on this machine", entry.label)
        },
        repair_actions: if entry.detected {
            Vec::new()
        } else {
            vec![setup_action.clone()]
        },
    }];

    let repair_actions = match entry.mode {
        HostMode::Codex if !entry.configured => {
            vec![setup_action, host_policy::codex_notify_repair_action()]
        }
        _ if !entry.configured => vec![setup_action],
        _ => Vec::new(),
    };

    checks.push(HostDoctorCheck {
        host: entry.mode,
        passed: entry.configured,
        message: entry.detail.clone(),
        repair_actions,
    });

    checks
}

fn run_list() -> Result<()> {
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

    Ok(())
}

fn run_setup(mode: HostMode, dry_run: bool) -> Result<()> {
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
    init::run(Some(mode.client_flag()), dry_run, false)
}

fn run_doctor(mode: Option<HostMode>, json: bool) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::install::InstallProfile;

    #[test]
    fn test_host_mode_mappings_are_explicit() {
        assert_eq!(HostMode::Codex.client_flag(), "codex");
        assert_eq!(HostMode::Codex.install_profile(), InstallProfile::Codex);
        assert_eq!(HostMode::ClaudeCode.client_flag(), "claude-code");
        assert_eq!(
            HostMode::ClaudeCode.install_profile(),
            InstallProfile::ClaudeCode
        );
        assert_eq!(HostMode::Cursor.client_flag(), "cursor");
        assert_eq!(HostMode::Cursor.install_profile(), InstallProfile::Cursor);
    }

    #[test]
    fn test_codex_doctor_report_includes_notify_repair() {
        let entry = HostInventoryEntry {
            mode: HostMode::Codex,
            label: HostMode::Codex.label().to_string(),
            adapter_kind: HostAdapterKind::McpAndNotify,
            adapter_label: HostAdapterKind::McpAndNotify.label().to_string(),
            detected: true,
            configured: false,
            config_path: Some("/Users/test/.codex/config.toml".to_string()),
            detail: "Run `hyphae init` to add the Codex notify adapter.".to_string(),
        };

        let checks = doctor_checks_for_entry(&entry);
        let repair_actions = dedupe_repair_actions(
            checks
                .iter()
                .flat_map(|check| check.repair_actions.clone())
                .collect(),
        );

        assert!(
            repair_actions
                .iter()
                .any(|action| action.command.contains("hyphae init")
                    || action.command.contains("stipe host setup codex"))
        );
    }

    #[test]
    fn test_doctor_checks_reflect_inventory_entry() {
        let entry = HostInventoryEntry {
            mode: HostMode::Cursor,
            label: HostMode::Cursor.label().to_string(),
            adapter_kind: HostAdapterKind::Mcp,
            adapter_label: HostAdapterKind::Mcp.label().to_string(),
            detected: false,
            configured: false,
            config_path: Some("/Users/test/.cursor/mcp.json".to_string()),
            detail: "Cursor is not detected on this machine yet.".to_string(),
        };

        let checks = doctor_checks_for_entry(&entry);

        assert_eq!(checks.len(), 2);
        assert!(!checks[0].passed);
        assert!(
            checks[0]
                .repair_actions
                .iter()
                .any(|action| action.command == "stipe host setup cursor")
        );
        assert_eq!(checks[1].message, entry.detail);
    }

    #[test]
    fn test_inventory_entry_uses_shared_host_descriptor_metadata() {
        let entry = inventory_entry(HostMode::Codex, &[]);

        assert_eq!(entry.label, HostMode::Codex.label());
        assert_eq!(entry.adapter_kind, HostAdapterKind::McpAndNotify);
        assert_eq!(entry.adapter_label, "MCP + notify");
    }
}
