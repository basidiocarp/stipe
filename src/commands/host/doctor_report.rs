use crate::commands::codex_notify;
use crate::commands::host_policy;
use crate::commands::host_policy::HostMode;
use crate::commands::repair::{RepairAction, dedupe_repair_actions};

use super::inventory::build_inventory;
use super::model::{HostDoctorCheck, HostDoctorReport, HostInventoryEntry};

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

pub fn doctor_checks_for_entry(entry: &HostInventoryEntry) -> Vec<HostDoctorCheck> {
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
            vec![setup_action, codex_notify::codex_notify_repair_action()]
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
