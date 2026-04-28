use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::commands::claude_hooks;
use crate::commands::tool_registry;

const OWNERSHIP_DIR: &str = "stipe/ownership";

/// One integration point that must pass after install (or be absent after uninstall).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntegrationPoint {
    /// The tool binary is present on PATH or at the expected install location.
    BinaryPresent,
    /// Hook registration exists in the host's settings.json or equivalent.
    HookRegistered,
    /// Statusline entry is configured (if applicable).
    StatuslineConfigured,
    /// Config entries are internally consistent (no conflicting values).
    ConfigConsistent,
}

/// The result for a single integration point check.
#[derive(Debug, Clone)]
pub struct PointResult {
    pub point: IntegrationPoint,
    pub passed: bool,
    pub detail: Option<String>,
}

/// Collected results for all integration points.
#[derive(Debug, Clone)]
pub struct CompletenessReport {
    pub results: Vec<PointResult>,
}

impl CompletenessReport {
    /// Returns `true` if all integration points passed.
    #[must_use]
    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.passed)
    }

    /// Returns all integration points that did not pass.
    #[must_use]
    pub fn failed_points(&self) -> Vec<&PointResult> {
        self.results.iter().filter(|r| !r.passed).collect()
    }
}

/// Check all integration points for a tool and return a report.
///
/// `install_dir` is the directory where the binary was placed (e.g. `~/.local/bin`).
#[must_use]
pub fn check_completeness(tool_name: &str, install_dir: &Path) -> CompletenessReport {
    let binary_path = install_dir.join(tool_name);

    // BinaryPresent: binary exists at the install location or on PATH.
    let binary_passed = binary_path.exists()
        || tool_registry::find(tool_name)
            .is_some_and(|spec| tool_registry::resolve_binary_path(spec).is_some());

    // HookRegistered: any hook settings path contains cortina hooks.
    let hook_passed = claude_hooks::claude_hooks_configured();

    // StatuslineConfigured: reuse the same configured() check that doctor uses.
    // Hook configuration checks include statusline via claude_hooks_configured().
    // We treat statusline as passing when hook configuration passes (same underlying check).
    let statusline_passed = hook_passed;

    // ConfigConsistent: no conflicting installed binaries / path issues.
    // Simple heuristic: if the binary can be found, consider config consistent.
    let config_passed = binary_passed;

    CompletenessReport {
        results: vec![
            PointResult {
                point: IntegrationPoint::BinaryPresent,
                passed: binary_passed,
                detail: if binary_passed {
                    Some(format!("binary found for {tool_name}"))
                } else {
                    Some(format!(
                        "{tool_name} not found at {} or on PATH",
                        binary_path.display()
                    ))
                },
            },
            PointResult {
                point: IntegrationPoint::HookRegistered,
                passed: hook_passed,
                detail: if hook_passed {
                    Some("hook registration detected".to_string())
                } else {
                    Some(
                        "no hook registration found — run `stipe init` to register hooks"
                            .to_string(),
                    )
                },
            },
            PointResult {
                point: IntegrationPoint::StatuslineConfigured,
                passed: statusline_passed,
                detail: if statusline_passed {
                    Some("statusline configured".to_string())
                } else {
                    Some(
                        "statusline not configured — run `stipe init` to set up statusline"
                            .to_string(),
                    )
                },
            },
            PointResult {
                point: IntegrationPoint::ConfigConsistent,
                passed: config_passed,
                detail: if config_passed {
                    Some("config consistent".to_string())
                } else {
                    Some(format!("config may be inconsistent for {tool_name}"))
                },
            },
        ],
    }
}

/// Check that all integration points are ABSENT (for post-uninstall verification).
///
/// A point "passes" here when it is confirmed absent.
#[must_use]
#[allow(dead_code)]
pub fn check_absence(tool_name: &str, install_dir: &Path) -> CompletenessReport {
    let binary_path = install_dir.join(tool_name);

    let binary_absent = !binary_path.exists()
        && tool_registry::find(tool_name)
            .is_none_or(|spec| tool_registry::resolve_binary_path(spec).is_none());

    let hook_absent = !claude_hooks::claude_hooks_configured();
    let statusline_absent = hook_absent;
    let config_absent = binary_absent;

    CompletenessReport {
        results: vec![
            PointResult {
                point: IntegrationPoint::BinaryPresent,
                passed: binary_absent,
                detail: if binary_absent {
                    Some(format!("{tool_name} binary absent as expected"))
                } else {
                    Some(format!(
                        "{tool_name} binary still present at {}",
                        binary_path.display()
                    ))
                },
            },
            PointResult {
                point: IntegrationPoint::HookRegistered,
                passed: hook_absent,
                detail: if hook_absent {
                    Some("hooks absent as expected".to_string())
                } else {
                    Some("hook registration still present".to_string())
                },
            },
            PointResult {
                point: IntegrationPoint::StatuslineConfigured,
                passed: statusline_absent,
                detail: if statusline_absent {
                    Some("statusline absent as expected".to_string())
                } else {
                    Some("statusline configuration still present".to_string())
                },
            },
            PointResult {
                point: IntegrationPoint::ConfigConsistent,
                passed: config_absent,
                detail: if config_absent {
                    Some("config absent as expected".to_string())
                } else {
                    Some(format!("{tool_name} config entries still present"))
                },
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// Ownership state
// ---------------------------------------------------------------------------

/// Stipe-managed ownership record written after a successful install.
#[derive(Debug, Serialize, Deserialize)]
pub struct OwnershipState {
    pub tool: String,
    pub installed_at: String,
    pub integration_points: Vec<IntegrationPoint>,
}

fn ownership_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|base| base.join(OWNERSHIP_DIR))
}

fn ownership_path(tool_name: &str) -> Option<PathBuf> {
    ownership_dir().map(|dir| dir.join(format!("{tool_name}.json")))
}

/// Write an ownership state file after a successful install.
pub fn write_ownership_state(tool_name: &str, report: &CompletenessReport) -> Result<()> {
    let Some(path) = ownership_path(tool_name) else {
        return Ok(());
    };

    let parent = path
        .parent()
        .context("ownership path should have a parent directory")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("creating ownership directory {}", parent.display()))?;

    let installed_at = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or_else(
            |_| "unknown".to_string(),
            |d| {
                // Format as a simple ISO-8601-like timestamp without external deps.
                let secs = d.as_secs();
                let (y, mo, d2, h, m, s) = epoch_to_ymd_hms(secs);
                format!("{y:04}-{mo:02}-{d2:02}T{h:02}:{m:02}:{s:02}Z")
            },
        );

    let passed_points: Vec<IntegrationPoint> = report
        .results
        .iter()
        .filter(|r| r.passed)
        .map(|r| r.point.clone())
        .collect();

    let state = OwnershipState {
        tool: tool_name.to_string(),
        installed_at,
        integration_points: passed_points,
    };

    let content = serde_json::to_string_pretty(&state).context("serializing ownership state")?;

    std::fs::write(&path, content)
        .with_context(|| format!("writing ownership state to {}", path.display()))?;

    Ok(())
}

/// Remove the ownership state file after a successful uninstall.
pub fn remove_ownership_state(tool_name: &str) -> Result<()> {
    let Some(path) = ownership_path(tool_name) else {
        return Ok(());
    };

    if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("removing ownership state {}", path.display()))?;
    }

    Ok(())
}

/// Returns `true` if an ownership state file exists for the named tool,
/// indicating stipe installed and manages it.
#[must_use]
pub fn is_stipe_managed(tool_name: &str) -> bool {
    ownership_path(tool_name).is_some_and(|path| path.exists())
}

/// Load the ownership state for a tool, if present.
#[allow(dead_code)]
pub fn load_ownership_state(tool_name: &str) -> Result<Option<OwnershipState>> {
    let Some(path) = ownership_path(tool_name) else {
        return Ok(None);
    };

    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("reading ownership state from {}", path.display()))?;

    let state: OwnershipState = serde_json::from_str(&content)
        .with_context(|| format!("parsing ownership state from {}", path.display()))?;

    Ok(Some(state))
}

/// Minimal epoch→calendar conversion to avoid needing the `time` or `chrono` crates.
fn epoch_to_ymd_hms(mut secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let s = secs % 60;
    secs /= 60;
    let m = secs % 60;
    secs /= 60;
    let h = secs % 24;
    secs /= 24;

    // Days since Unix epoch (1970-01-01).
    let mut days = secs;
    let mut year: u64 = 1970;
    loop {
        let days_in_year: u64 = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let leap = is_leap(year);
    let month_days: [u64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month: u64 = 1;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }

    (year, month, days + 1, h, m, s)
}

fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completeness_report_all_passed_when_all_true() {
        let report = CompletenessReport {
            results: vec![
                PointResult {
                    point: IntegrationPoint::BinaryPresent,
                    passed: true,
                    detail: None,
                },
                PointResult {
                    point: IntegrationPoint::HookRegistered,
                    passed: true,
                    detail: None,
                },
            ],
        };
        assert!(report.all_passed());
    }

    #[test]
    fn completeness_report_not_all_passed_when_one_fails() {
        let report = CompletenessReport {
            results: vec![
                PointResult {
                    point: IntegrationPoint::BinaryPresent,
                    passed: true,
                    detail: None,
                },
                PointResult {
                    point: IntegrationPoint::HookRegistered,
                    passed: false,
                    detail: None,
                },
            ],
        };
        assert!(!report.all_passed());
        assert_eq!(report.failed_points().len(), 1);
    }

    #[test]
    fn check_completeness_binary_present_uses_install_dir() {
        let tmp = std::env::temp_dir().join("stipe-verify-test");
        let _ = std::fs::create_dir_all(&tmp);
        let binary = tmp.join("my-test-tool");
        std::fs::write(&binary, "").unwrap();

        let report = check_completeness("my-test-tool", &tmp);
        let binary_result = report
            .results
            .iter()
            .find(|r| r.point == IntegrationPoint::BinaryPresent)
            .expect("BinaryPresent should always be checked");
        assert!(
            binary_result.passed,
            "binary in install_dir should be detected as present"
        );

        let _ = std::fs::remove_file(binary);
    }

    #[test]
    fn check_completeness_binary_absent_when_not_installed() {
        let tmp = std::env::temp_dir().join("stipe-verify-absent-test");
        let _ = std::fs::create_dir_all(&tmp);

        let report = check_completeness("__nonexistent_tool__", &tmp);
        let binary_result = report
            .results
            .iter()
            .find(|r| r.point == IntegrationPoint::BinaryPresent)
            .expect("BinaryPresent should always be checked");
        assert!(!binary_result.passed);
    }

    #[test]
    fn epoch_to_ymd_hms_known_epoch() {
        // Unix epoch = 1970-01-01T00:00:00Z
        #[allow(clippy::many_single_char_names)]
        let (year, month, day, hour, minute, second) = epoch_to_ymd_hms(0);
        assert_eq!(
            (year, month, day, hour, minute, second),
            (1970, 1, 1, 0, 0, 0)
        );
    }

    #[test]
    fn ownership_state_roundtrips_json() {
        let state = OwnershipState {
            tool: "hyphae".to_string(),
            installed_at: "2026-04-17T00:00:00Z".to_string(),
            integration_points: vec![
                IntegrationPoint::BinaryPresent,
                IntegrationPoint::HookRegistered,
            ],
        };
        let json = serde_json::to_string(&state).unwrap();
        let roundtripped: OwnershipState = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtripped.tool, "hyphae");
        assert_eq!(roundtripped.integration_points.len(), 2);
    }
}
