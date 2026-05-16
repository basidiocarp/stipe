use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use spore::atomic_write_bytes;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::commands::host_policy;
use crate::commands::install::InstallProfile;

const BASIDIOCARP_CONFIG_DIR: &str = "basidiocarp";
const USER_POLICY_FILE: &str = "runtime-policy.toml";
const PROJECT_POLICY_FILE: &str = "stipe-runtime-policy.toml";
const PROJECT_POLICY_DIR: &str = ".basidiocarp";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PolicyScope {
    Project,
    User,
}

impl PolicyScope {
    fn label(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::User => "user",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PolicyDecision {
    Allow,
    Deny,
}

impl PolicyDecision {
    fn label(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DecisionSource {
    OperatorProfile,
    OperatorPolicyFile,
    ImportedConfig,
}

impl DecisionSource {
    fn label(self) -> &'static str {
        match self {
            Self::OperatorProfile => "operator-profile",
            Self::OperatorPolicyFile => "operator-policy-file",
            Self::ImportedConfig => "imported-config",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RememberedDecision {
    pub(crate) subject: String,
    pub(crate) scope: PolicyScope,
    pub(crate) decision: PolicyDecision,
    pub(crate) source: DecisionSource,
    pub(crate) updated_at_unix: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) note: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct RuntimePolicyReport {
    pub(crate) configured: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) config_paths: Vec<PathBuf>,
    pub(crate) precedence: Vec<PolicyScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) load_error: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) remembered_decisions: Vec<RememberedDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) active_install_profile: Option<RememberedDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StoredRuntimePolicy {
    #[serde(default)]
    remembered_decisions: Vec<RememberedDecision>,
}

pub(crate) fn collect_runtime_policy(
    active_profile: Option<InstallProfile>,
) -> RuntimePolicyReport {
    let precedence = precedence_order();
    let (config_paths, remembered_decisions, load_error) = load_runtime_policy_state();
    let active_install_profile = active_profile.and_then(|profile| {
        effective_install_profile_decision(&remembered_decisions, profile).cloned()
    });

    RuntimePolicyReport {
        configured: !config_paths.is_empty(),
        config_paths,
        precedence,
        load_error,
        remembered_decisions,
        active_install_profile,
    }
}

pub(crate) fn remember_install_profile_approval(
    profile: InstallProfile,
) -> Result<Option<PathBuf>> {
    let Some(path) = user_policy_path() else {
        return Ok(None);
    };

    let mut stored = load_policy_file(&path)?;
    let timestamp = current_timestamp();
    let subject = install_profile_subject(profile);

    if let Some(record) = stored
        .remembered_decisions
        .iter_mut()
        .find(|record| record.subject == subject && record.scope == PolicyScope::User)
    {
        record.decision = PolicyDecision::Allow;
        record.source = DecisionSource::OperatorProfile;
        record.updated_at_unix = timestamp;
        record.note = Some(format!(
            "Remembered approval recorded after explicit stipe install --profile {}.",
            profile.profile_name()
        ));
    } else {
        stored.remembered_decisions.push(RememberedDecision {
            subject,
            scope: PolicyScope::User,
            decision: PolicyDecision::Allow,
            source: DecisionSource::OperatorProfile,
            updated_at_unix: timestamp,
            note: Some(format!(
                "Remembered approval recorded after explicit stipe install --profile {}.",
                profile.profile_name()
            )),
        });
    }

    save_policy_file(&path, &stored)?;
    Ok(Some(path))
}

pub(crate) fn render_install_policy_lines(
    profile: InstallProfile,
    report: &RuntimePolicyReport,
) -> Vec<String> {
    let mut lines = vec![
        "Runtime policy:".to_string(),
        format!(
            "  policy scope precedence: {}",
            format_precedence(&report.precedence)
        ),
    ];

    if let Some(decision) = &report.active_install_profile {
        lines.push(format!(
            "  active decision for {}: {} ({}, source: {}, updated: {})",
            profile.profile_name(),
            decision.decision.label(),
            decision.scope.label(),
            decision.source.label(),
            decision.updated_at_unix
        ));
    } else {
        lines.push(format!(
            "  active decision for {}: no remembered approval or deny decision recorded",
            profile.profile_name()
        ));
    }

    if let Some(load_error) = &report.load_error {
        lines.push(format!("  load error: {load_error}"));
    }

    if report.remembered_decisions.is_empty() {
        lines.push("  approval memory: none recorded".to_string());
    } else {
        let allow_count = report
            .remembered_decisions
            .iter()
            .filter(|record| record.decision == PolicyDecision::Allow)
            .count();
        let deny_count = report
            .remembered_decisions
            .iter()
            .filter(|record| record.decision == PolicyDecision::Deny)
            .count();
        lines.push(format!(
            "  approval memory: {allow_count} allow, {deny_count} deny"
        ));
    }

    lines
}

pub(crate) fn enforce_install_profile_policy(
    profile: InstallProfile,
    report: &RuntimePolicyReport,
) -> Result<()> {
    if let Some(load_error) = &report.load_error {
        return Err(anyhow!(
            "runtime policy for install profile {} could not be loaded: {load_error}",
            profile.profile_name()
        ));
    }

    if let Some(active) = &report.active_install_profile
        && active.decision == PolicyDecision::Deny
    {
        return Err(anyhow!(
            "runtime policy denies install profile {} at {} scope (source: {}, updated: {})",
            profile.profile_name(),
            active.scope.label(),
            active.source.label(),
            active.updated_at_unix
        ));
    }

    Ok(())
}

pub(crate) fn describe_runtime_policy(report: &RuntimePolicyReport) -> String {
    if let Some(load_error) = &report.load_error {
        format!("Runtime policy could not be loaded: {load_error}")
    } else if let Some(active) = &report.active_install_profile {
        format!(
            "Saved install profile is governed by a remembered {} decision at {} scope.",
            active.decision.label(),
            active.scope.label()
        )
    } else if report.remembered_decisions.is_empty() {
        "No remembered approvals or deny decisions are currently stored.".to_string()
    } else {
        format!(
            "{} remembered approval-memory decision(s) recorded with {} precedence.",
            report.remembered_decisions.len(),
            format_precedence(&report.precedence)
        )
    }
}

pub(crate) fn policy_conflicts_with_active_profile(report: &RuntimePolicyReport) -> bool {
    if report.load_error.is_some() {
        return true;
    }
    report
        .active_install_profile
        .as_ref()
        .is_some_and(|decision| decision.decision == PolicyDecision::Deny)
}

fn load_runtime_policy_state() -> (Vec<PathBuf>, Vec<RememberedDecision>, Option<String>) {
    let mut config_paths = Vec::new();
    let mut remembered_decisions = Vec::new();
    let mut load_error = None;

    for path in policy_paths_in_precedence() {
        if path.exists() {
            config_paths.push(path.clone());
        }
        match load_policy_file(&path) {
            Ok(policy) => remembered_decisions.extend(policy.remembered_decisions),
            Err(error) if path.exists() => {
                load_error = Some(error.to_string());
                break;
            }
            Err(_) => {}
        }
    }

    (config_paths, remembered_decisions, load_error)
}

fn policy_paths_in_precedence() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(path) = project_policy_path() {
        paths.push(path);
    }
    if let Some(path) = user_policy_path() {
        paths.push(path);
    }
    paths
}

fn precedence_order() -> Vec<PolicyScope> {
    vec![PolicyScope::Project, PolicyScope::User]
}

fn format_precedence(precedence: &[PolicyScope]) -> String {
    precedence
        .iter()
        .map(|scope| scope.label())
        .collect::<Vec<_>>()
        .join(" -> ")
}

fn install_profile_subject(profile: InstallProfile) -> String {
    format!("install-profile:{}", profile.profile_name())
}

fn effective_install_profile_decision(
    remembered_decisions: &[RememberedDecision],
    profile: InstallProfile,
) -> Option<&RememberedDecision> {
    let subject = install_profile_subject(profile);
    for scope in precedence_order() {
        if let Some(decision) = remembered_decisions
            .iter()
            .filter(|record| record.subject == subject && record.scope == scope)
            .max_by_key(|record| record.updated_at_unix)
        {
            return Some(decision);
        }
    }
    None
}

fn user_policy_path() -> Option<PathBuf> {
    current_config_dir().map(|dir| dir.join(BASIDIOCARP_CONFIG_DIR).join(USER_POLICY_FILE))
}

fn project_policy_path() -> Option<PathBuf> {
    host_policy::project_root().map(|root| root.join(PROJECT_POLICY_DIR).join(PROJECT_POLICY_FILE))
}

fn current_config_dir() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(path) = test_config_dir_override() {
        return Some(path);
    }

    dirs::config_dir()
}

#[cfg(test)]
thread_local! {
    static TEST_CONFIG_DIR_OVERRIDE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn test_config_dir_override() -> Option<PathBuf> {
    TEST_CONFIG_DIR_OVERRIDE.with(|path| path.borrow().clone())
}

#[cfg(test)]
pub(crate) fn with_config_dir_override<T>(root: PathBuf, f: impl FnOnce() -> T) -> T {
    TEST_CONFIG_DIR_OVERRIDE.with(|path| {
        let previous = path.replace(Some(root));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        path.replace(previous);
        match result {
            Ok(value) => value,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    })
}

fn load_policy_file(path: &Path) -> Result<StoredRuntimePolicy> {
    if !path.exists() {
        return Ok(StoredRuntimePolicy::default());
    }

    let content =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))
}

fn save_policy_file(path: &Path, policy: &StoredRuntimePolicy) -> Result<()> {
    let parent = path
        .parent()
        .context("runtime policy path should have a parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;

    let content = toml::to_string_pretty(policy).context("serializing runtime policy")?;
    atomic_write_bytes(path, content.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
pub(crate) fn save_policy_to_path(path: &Path, policy: &[RememberedDecision]) -> Result<()> {
    save_policy_file(
        path,
        &StoredRuntimePolicy {
            remembered_decisions: policy.to_vec(),
        },
    )
}

#[cfg(test)]
pub(crate) fn load_policy_from_path(path: &Path) -> Result<Vec<RememberedDecision>> {
    load_policy_file(path).map(|policy| policy.remembered_decisions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_policy_round_trips_remembered_decisions() {
        let temp_dir = std::env::temp_dir().join("stipe-runtime-policy-roundtrip");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("create temp dir");

        let policy_path = temp_dir.join("runtime-policy.toml");
        let remembered = vec![RememberedDecision {
            subject: "install-profile:codex".to_string(),
            scope: PolicyScope::User,
            decision: PolicyDecision::Allow,
            source: DecisionSource::OperatorProfile,
            updated_at_unix: 42,
            note: Some("Remembered approval".to_string()),
        }];

        save_policy_to_path(&policy_path, &remembered).expect("save policy");
        let loaded = load_policy_from_path(&policy_path).expect("load policy");

        assert_eq!(loaded, remembered);

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_project_scope_takes_precedence_over_user_scope() {
        let remembered = vec![
            RememberedDecision {
                subject: "install-profile:codex".to_string(),
                scope: PolicyScope::User,
                decision: PolicyDecision::Allow,
                source: DecisionSource::OperatorProfile,
                updated_at_unix: 100,
                note: None,
            },
            RememberedDecision {
                subject: "install-profile:codex".to_string(),
                scope: PolicyScope::Project,
                decision: PolicyDecision::Deny,
                source: DecisionSource::OperatorPolicyFile,
                updated_at_unix: 90,
                note: None,
            },
        ];

        let active = effective_install_profile_decision(&remembered, InstallProfile::Codex)
            .expect("active policy decision");

        assert_eq!(active.scope, PolicyScope::Project);
        assert_eq!(active.decision, PolicyDecision::Deny);
    }

    #[test]
    fn test_render_install_policy_lines_mentions_approval_memory() {
        let report = RuntimePolicyReport {
            configured: true,
            config_paths: vec![PathBuf::from("/tmp/runtime-policy.toml")],
            precedence: precedence_order(),
            load_error: None,
            remembered_decisions: vec![RememberedDecision {
                subject: "install-profile:codex".to_string(),
                scope: PolicyScope::User,
                decision: PolicyDecision::Allow,
                source: DecisionSource::OperatorProfile,
                updated_at_unix: 42,
                note: None,
            }],
            active_install_profile: Some(RememberedDecision {
                subject: "install-profile:codex".to_string(),
                scope: PolicyScope::User,
                decision: PolicyDecision::Allow,
                source: DecisionSource::OperatorProfile,
                updated_at_unix: 42,
                note: None,
            }),
        };

        let lines = render_install_policy_lines(InstallProfile::Codex, &report);
        assert!(lines.iter().any(|line| line.contains("Runtime policy")));
        assert!(lines.iter().any(|line| line.contains("approval memory")));
        assert!(lines.iter().any(|line| line.contains("allow")));
    }

    #[test]
    fn test_enforce_install_profile_policy_blocks_remembered_deny() {
        let report = RuntimePolicyReport {
            configured: true,
            config_paths: vec![PathBuf::from("/tmp/runtime-policy.toml")],
            precedence: precedence_order(),
            load_error: None,
            remembered_decisions: vec![RememberedDecision {
                subject: "install-profile:codex".to_string(),
                scope: PolicyScope::Project,
                decision: PolicyDecision::Deny,
                source: DecisionSource::OperatorPolicyFile,
                updated_at_unix: 42,
                note: Some("Operator denied this profile".to_string()),
            }],
            active_install_profile: Some(RememberedDecision {
                subject: "install-profile:codex".to_string(),
                scope: PolicyScope::Project,
                decision: PolicyDecision::Deny,
                source: DecisionSource::OperatorPolicyFile,
                updated_at_unix: 42,
                note: Some("Operator denied this profile".to_string()),
            }),
        };

        let error = enforce_install_profile_policy(InstallProfile::Codex, &report)
            .expect_err("deny decisions should block install");
        assert!(error.to_string().contains("denies install profile codex"));
        assert!(error.to_string().contains("project scope"));
    }

    #[test]
    fn test_enforce_install_profile_policy_blocks_load_errors() {
        let report = RuntimePolicyReport {
            configured: true,
            config_paths: vec![PathBuf::from("/tmp/runtime-policy.toml")],
            precedence: precedence_order(),
            load_error: Some("parsing /tmp/runtime-policy.toml".to_string()),
            remembered_decisions: Vec::new(),
            active_install_profile: None,
        };

        let error = enforce_install_profile_policy(InstallProfile::Codex, &report)
            .expect_err("policy load errors should block install");
        assert!(error.to_string().contains("could not be loaded"));
        assert!(error.to_string().contains("codex"));
    }
}
