use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::model::InitSnapshot;
use crate::commands::claude_hooks;
use crate::commands::codex_notify;
use crate::commands::host_policy::{self, HostConfigScope, HostMode};
use crate::commands::repair::{RepairAction, RepairTier};
use crate::commands::tool_registry;
use crate::ecosystem::clients::{self, McpClient};

const BASELINE_SCHEMA_VERSION: &str = "1.0";
const MCP_SERVERS: &[&str] = &["hyphae", "rhizome"];
const CORTINA_COMMAND_PREFIX: &str = "cortina adapter claude-code";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct InitBaselineManifest {
    pub(crate) schema_version: String,
    pub(crate) generated_at_unix_nanos: u64,
    pub(crate) target_client: Option<String>,
    pub(crate) scope: String,
    pub(crate) config_files: Vec<BaselineConfigFile>,
    pub(crate) mcp_servers: Vec<BaselineMcpServer>,
    pub(crate) hooks: Vec<BaselineHook>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BaselineConfigFile {
    pub(crate) path: PathBuf,
    pub(crate) checksum: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BaselineMcpServer {
    pub(crate) path: PathBuf,
    pub(crate) name: String,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) binary_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BaselineHook {
    pub(crate) path: PathBuf,
    pub(crate) event: String,
    pub(crate) matcher: Option<String>,
    pub(crate) hook_type: String,
    pub(crate) command: String,
    pub(crate) timeout: u64,
    pub(crate) status_message: String,
    pub(crate) binary_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DriftReport {
    pub(crate) baseline_path: PathBuf,
    pub(crate) generated_at_unix_nanos: u64,
    pub(crate) findings: Vec<DriftFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum DriftFinding {
    MissingMcpRegistration {
        config_path: PathBuf,
        name: String,
        command: String,
        args: Vec<String>,
    },
    MissingMcpBinary {
        config_path: PathBuf,
        name: String,
        command: String,
        args: Vec<String>,
        binary_path: PathBuf,
    },
    MissingHookRegistration {
        config_path: PathBuf,
        event: String,
        matcher: Option<String>,
        hook_type: String,
        command: String,
    },
    MissingHookBinary {
        config_path: PathBuf,
        event: String,
        matcher: Option<String>,
        hook_type: String,
        command: String,
        binary_path: PathBuf,
    },
    ConfigFileModified {
        path: PathBuf,
        expected_checksum: String,
        actual_checksum: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ConfigKind {
    JsonMcp,
    ClaudeRoot,
    TomlConfig,
    HookSettings,
}

#[derive(Debug, Clone)]
struct ConfigCandidate {
    path: PathBuf,
    kind: ConfigKind,
}

fn baseline_path() -> Option<PathBuf> {
    dirs::data_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join(".local/share")))
        .map(|dir| dir.join("stipe").join("init-baseline.json"))
}

fn now_unix_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos() as u64)
}

fn checksum_bytes(bytes: &[u8]) -> String {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }

    format!("{hash:016x}")
}

fn checksum_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(checksum_bytes(&bytes))
}

fn file_exists_and_checksum(path: &Path) -> Option<String> {
    path.exists().then(|| checksum_file(path).ok()).flatten()
}

fn managed_binary_path(tool_name: &str) -> PathBuf {
    tool_registry::find(tool_name)
        .and_then(tool_registry::resolve_binary_path)
        .unwrap_or_else(|| PathBuf::from(tool_name))
}

fn registration_binary_path(name: &str) -> Option<PathBuf> {
    match name {
        "hyphae" | "rhizome" => Some(managed_binary_path(name)),
        _ => None,
    }
}

fn hook_binary_path() -> PathBuf {
    managed_binary_path("cortina")
}

fn current_scope_paths(snapshot: &InitSnapshot, scope: HostConfigScope) -> Vec<ConfigCandidate> {
    let mut candidates = Vec::new();

    if snapshot.claude_host_selected_or_detected()
        && let Some(path) = host_policy::claude_hook_settings_path(scope)
    {
        candidates.push(ConfigCandidate {
            path,
            kind: ConfigKind::ClaudeRoot,
        });
        candidates.push(ConfigCandidate {
            path: host_policy::claude_hook_settings_path(scope)
                .expect("Claude hook settings path should be available when selected"),
            kind: ConfigKind::HookSettings,
        });
    }

    if snapshot.codex_host_selected_or_detected()
        && let Some(path) = host_policy::codex_notify_config_path(scope)
    {
        candidates.push(ConfigCandidate {
            path,
            kind: ConfigKind::TomlConfig,
        });
    }

    if snapshot.host_in_scope(HostMode::Cursor)
        && let Some(path) = host_policy::host_config_path(HostMode::Cursor)
    {
        candidates.push(ConfigCandidate {
            path,
            kind: ConfigKind::JsonMcp,
        });
    }

    if snapshot.target_client.is_none() {
        for client in clients::detect_clients() {
            if client.handled_separately_in_ecosystem() {
                continue;
            }

            let Some(path) = client.config_path() else {
                continue;
            };

            let kind = match client {
                McpClient::CodexCli => ConfigKind::TomlConfig,
                McpClient::ClaudeCode => ConfigKind::ClaudeRoot,
                _ => ConfigKind::JsonMcp,
            };

            candidates.push(ConfigCandidate { path, kind });
        }
    }

    candidates
}

fn extract_json_mcp_servers(
    content: &str,
    config_path: &Path,
    allow_project_scoped: bool,
) -> Vec<BaselineMcpServer> {
    let Ok(parsed) = serde_json::from_str::<JsonValue>(content) else {
        return Vec::new();
    };

    let mut servers = Vec::new();

    if let Some(mcp_servers) = parsed.get("mcpServers").and_then(JsonValue::as_object) {
        for name in MCP_SERVERS {
            if let Some(server) = mcp_servers.get(*name).and_then(JsonValue::as_object) {
                if let (Some(command), Some(args)) = (
                    server.get("command").and_then(JsonValue::as_str),
                    server.get("args").and_then(JsonValue::as_array),
                ) {
                    servers.push(BaselineMcpServer {
                        path: config_path.to_path_buf(),
                        name: (*name).to_string(),
                        command: command.to_string(),
                        args: args
                            .iter()
                            .filter_map(JsonValue::as_str)
                            .map(ToOwned::to_owned)
                            .collect(),
                        binary_path: registration_binary_path(name)
                            .unwrap_or_else(|| PathBuf::from(command)),
                    });
                }
            }
        }
    }

    if allow_project_scoped && let Some(project_root) = host_policy::project_root() {
        let project_key = project_root.to_string_lossy();
        if let Some(project) = parsed
            .get("projects")
            .and_then(JsonValue::as_object)
            .and_then(|projects| projects.get(project_key.as_ref()))
            .and_then(JsonValue::as_object)
            && let Some(mcp_servers) = project.get("mcpServers").and_then(JsonValue::as_object)
        {
            for name in MCP_SERVERS {
                if let Some(server) = mcp_servers.get(*name).and_then(JsonValue::as_object) {
                    if let (Some(command), Some(args)) = (
                        server.get("command").and_then(JsonValue::as_str),
                        server.get("args").and_then(JsonValue::as_array),
                    ) {
                        servers.push(BaselineMcpServer {
                            path: config_path.to_path_buf(),
                            name: (*name).to_string(),
                            command: command.to_string(),
                            args: args
                                .iter()
                                .filter_map(JsonValue::as_str)
                                .map(ToOwned::to_owned)
                                .collect(),
                            binary_path: registration_binary_path(name)
                                .unwrap_or_else(|| PathBuf::from(command)),
                        });
                    }
                }
            }
        }
    }

    servers
}

fn extract_toml_mcp_servers(content: &str, config_path: &Path) -> Vec<BaselineMcpServer> {
    let Ok(parsed) = toml::from_str::<toml::Value>(content) else {
        return Vec::new();
    };

    let Some(mcp_servers) = parsed.get("mcp_servers").and_then(toml::Value::as_table) else {
        return Vec::new();
    };

    let mut servers = Vec::new();
    for name in MCP_SERVERS {
        if let Some(server) = mcp_servers.get(*name).and_then(toml::Value::as_table)
            && let Some(command) = server.get("command").and_then(toml::Value::as_str)
        {
            let args = server
                .get("args")
                .and_then(toml::Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(toml::Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            servers.push(BaselineMcpServer {
                path: config_path.to_path_buf(),
                name: (*name).to_string(),
                command: command.to_string(),
                args,
                binary_path: registration_binary_path(name)
                    .unwrap_or_else(|| PathBuf::from(command)),
            });
        }
    }

    servers
}

fn current_hook_entries(config_path: &Path) -> Vec<BaselineHook> {
    let mut hooks = Vec::new();
    let Ok(entries) = claude_hooks::hook_entries_at_path(config_path) else {
        return hooks;
    };

    let binary_path = hook_binary_path();
    for entry in entries {
        if !entry.command.starts_with(CORTINA_COMMAND_PREFIX) {
            continue;
        }

        hooks.push(BaselineHook {
            path: config_path.to_path_buf(),
            event: entry.event,
            matcher: entry.matcher,
            hook_type: entry.hook_type,
            command: entry.command,
            timeout: entry.timeout,
            status_message: entry.status_message,
            binary_path: binary_path.clone(),
        });
    }

    hooks
}

fn add_checksum_if_relevant(config_files: &mut BTreeMap<PathBuf, String>, path: &Path) {
    if let Some(checksum) = file_exists_and_checksum(path) {
        config_files.insert(path.to_path_buf(), checksum);
    }
}

fn dedupe_config_candidates(candidates: Vec<ConfigCandidate>) -> Vec<ConfigCandidate> {
    let mut unique = BTreeMap::<(PathBuf, ConfigKind), ConfigCandidate>::new();
    for candidate in candidates {
        unique
            .entry((candidate.path.clone(), candidate.kind))
            .or_insert(candidate);
    }
    unique.into_values().collect()
}

fn build_current_manifest(
    snapshot: &InitSnapshot,
    scope: HostConfigScope,
) -> Result<InitBaselineManifest> {
    let mut config_files = BTreeMap::<PathBuf, String>::new();
    let mut mcp_servers = Vec::new();
    let mut hooks = Vec::new();

    for candidate in dedupe_config_candidates(current_scope_paths(snapshot, scope)) {
        let content = match fs::read_to_string(&candidate.path) {
            Ok(content) => content,
            Err(_) => continue,
        };

        match candidate.kind {
            ConfigKind::JsonMcp => {
                let entries = extract_json_mcp_servers(&content, &candidate.path, false);
                if !entries.is_empty() {
                    add_checksum_if_relevant(&mut config_files, &candidate.path);
                    mcp_servers.extend(entries);
                }
            }
            ConfigKind::ClaudeRoot => {
                let entries = extract_json_mcp_servers(&content, &candidate.path, true);
                if !entries.is_empty() {
                    add_checksum_if_relevant(&mut config_files, &candidate.path);
                    mcp_servers.extend(entries);
                }
            }
            ConfigKind::TomlConfig => {
                let mut entries = extract_toml_mcp_servers(&content, &candidate.path);
                if codex_notify::codex_notify_configured_at_path(&candidate.path) {
                    add_checksum_if_relevant(&mut config_files, &candidate.path);
                }
                if !entries.is_empty() {
                    add_checksum_if_relevant(&mut config_files, &candidate.path);
                    mcp_servers.append(&mut entries);
                }
            }
            ConfigKind::HookSettings => {
                let entries = current_hook_entries(&candidate.path);
                if !entries.is_empty() {
                    add_checksum_if_relevant(&mut config_files, &candidate.path);
                    hooks.extend(entries);
                }
            }
        }
    }

    mcp_servers.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.name.cmp(&right.name))
            .then(left.command.cmp(&right.command))
            .then(left.args.cmp(&right.args))
    });
    hooks.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.event.cmp(&right.event))
            .then(left.command.cmp(&right.command))
    });

    let config_files = config_files
        .into_iter()
        .map(|(path, checksum)| BaselineConfigFile { path, checksum })
        .collect::<Vec<_>>();

    Ok(InitBaselineManifest {
        schema_version: BASELINE_SCHEMA_VERSION.to_string(),
        generated_at_unix_nanos: now_unix_nanos(),
        target_client: snapshot.target_client.clone(),
        scope: host_policy::scope_name(scope).to_string(),
        config_files,
        mcp_servers,
        hooks,
    })
}

fn write_manifest(path: &Path, manifest: &InitBaselineManifest) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }

    let content = serde_json::to_string_pretty(manifest).context("serializing init baseline")?;
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

pub(super) fn record_current_baseline(
    snapshot: &InitSnapshot,
    scope: HostConfigScope,
) -> Result<()> {
    let Some(path) = baseline_path() else {
        return Ok(());
    };

    let manifest = build_current_manifest(snapshot, scope)?;
    write_manifest(&path, &manifest)
}

pub(crate) fn load_baseline() -> Result<Option<InitBaselineManifest>> {
    let Some(path) = baseline_path() else {
        return Ok(None);
    };

    if !path.exists() {
        return Ok(None);
    }

    let content =
        fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let manifest =
        serde_json::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;
    Ok(Some(manifest))
}

fn config_file_repair_action() -> RepairAction {
    RepairAction::stipe(
        "repair-init",
        "Repair the init baseline",
        "Reapply shared ecosystem configuration and refresh the baseline manifest.",
        &["init", "--repair"],
        RepairTier::Primary,
    )
}

fn install_tool_repair_action(tool: &str) -> RepairAction {
    RepairAction::manual(
        format!("Install {}", title_case(tool)),
        format!("Install {tool} through the managed stipe release path."),
        format!("stipe install {tool}"),
        vec!["install".to_string(), tool.to_string()],
        RepairTier::Primary,
    )
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

pub(crate) fn repair_action_for_finding(finding: &DriftFinding) -> RepairAction {
    match finding {
        DriftFinding::MissingMcpRegistration { name: _, .. } => config_file_repair_action(),
        DriftFinding::MissingMcpBinary { name, .. } => install_tool_repair_action(name),
        DriftFinding::MissingHookRegistration { .. } => config_file_repair_action(),
        DriftFinding::MissingHookBinary { .. } => install_tool_repair_action("cortina"),
        DriftFinding::ConfigFileModified { .. } => config_file_repair_action(),
    }
}

pub(crate) fn repair_actions_for_report(report: &DriftReport) -> Vec<RepairAction> {
    let mut actions = Vec::new();
    for finding in &report.findings {
        let action = repair_action_for_finding(finding);
        if !actions
            .iter()
            .any(|existing: &RepairAction| existing.command == action.command)
        {
            actions.push(action);
        }
    }
    actions
}

fn mcp_registration_missing(
    config_path: PathBuf,
    name: String,
    command: String,
    args: Vec<String>,
) -> DriftFinding {
    DriftFinding::MissingMcpRegistration {
        config_path,
        name,
        command,
        args,
    }
}

fn mcp_binary_missing(
    config_path: PathBuf,
    name: String,
    command: String,
    args: Vec<String>,
    binary_path: PathBuf,
) -> DriftFinding {
    DriftFinding::MissingMcpBinary {
        config_path,
        name,
        command,
        args,
        binary_path,
    }
}

fn hook_registration_missing(
    config_path: PathBuf,
    event: String,
    matcher: Option<String>,
    hook_type: String,
    command: String,
) -> DriftFinding {
    DriftFinding::MissingHookRegistration {
        config_path,
        event,
        matcher,
        hook_type,
        command,
    }
}

fn hook_binary_missing(
    config_path: PathBuf,
    event: String,
    matcher: Option<String>,
    hook_type: String,
    command: String,
    binary_path: PathBuf,
) -> DriftFinding {
    DriftFinding::MissingHookBinary {
        config_path,
        event,
        matcher,
        hook_type,
        command,
        binary_path,
    }
}

fn config_modified(
    path: PathBuf,
    expected_checksum: String,
    actual_checksum: Option<String>,
) -> DriftFinding {
    DriftFinding::ConfigFileModified {
        path,
        expected_checksum,
        actual_checksum,
    }
}

fn current_mcp_servers_for_path(path: &Path) -> Vec<BaselineMcpServer> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };

    match path.extension().and_then(|ext| ext.to_str()) {
        Some("toml") => extract_toml_mcp_servers(&content, path),
        _ => extract_json_mcp_servers(&content, path, true),
    }
}

fn current_hooks_for_path(path: &Path) -> Vec<BaselineHook> {
    current_hook_entries(path)
}

fn config_checksum_for_path(path: &Path) -> Option<String> {
    checksum_file(path).ok()
}

pub(crate) fn evaluate_drift_from_manifest(manifest: &InitBaselineManifest) -> Result<DriftReport> {
    let mut findings = Vec::new();
    let mut config_files_by_path = BTreeMap::<PathBuf, String>::new();
    let mut mcp_by_path = BTreeMap::<PathBuf, Vec<&BaselineMcpServer>>::new();
    let mut hooks_by_path = BTreeMap::<PathBuf, Vec<&BaselineHook>>::new();

    for config in &manifest.config_files {
        config_files_by_path.insert(config.path.clone(), config.checksum.clone());
    }
    for server in &manifest.mcp_servers {
        mcp_by_path
            .entry(server.path.clone())
            .or_default()
            .push(server);
    }
    for hook in &manifest.hooks {
        hooks_by_path
            .entry(hook.path.clone())
            .or_default()
            .push(hook);
    }

    for (path, expected_checksum) in &config_files_by_path {
        match config_checksum_for_path(path) {
            Some(actual_checksum) if actual_checksum != *expected_checksum => {
                findings.push(config_modified(
                    path.clone(),
                    expected_checksum.clone(),
                    Some(actual_checksum),
                ));
            }
            None if mcp_by_path.get(path).is_none() && hooks_by_path.get(path).is_none() => {
                findings.push(config_modified(
                    path.clone(),
                    expected_checksum.clone(),
                    None,
                ));
            }
            _ => {}
        }
    }

    for server in &manifest.mcp_servers {
        let current = current_mcp_servers_for_path(&server.path);
        let matching = current.iter().find(|entry| {
            entry.name == server.name
                && entry.command == server.command
                && entry.args == server.args
        });

        if matching.is_none() {
            findings.push(mcp_registration_missing(
                server.path.clone(),
                server.name.clone(),
                server.command.clone(),
                server.args.clone(),
            ));
            continue;
        }

        if !server.binary_path.exists() {
            findings.push(mcp_binary_missing(
                server.path.clone(),
                server.name.clone(),
                server.command.clone(),
                server.args.clone(),
                server.binary_path.clone(),
            ));
        }
    }

    for hook in &manifest.hooks {
        let current = current_hooks_for_path(&hook.path);
        let matching = current.iter().find(|entry| {
            entry.event == hook.event
                && entry.matcher == hook.matcher
                && entry.hook_type == hook.hook_type
                && entry.command == hook.command
                && entry.timeout == hook.timeout
                && entry.status_message == hook.status_message
        });

        if matching.is_none() {
            findings.push(hook_registration_missing(
                hook.path.clone(),
                hook.event.clone(),
                hook.matcher.clone(),
                hook.hook_type.clone(),
                hook.command.clone(),
            ));
            continue;
        }

        if !hook.binary_path.exists() {
            findings.push(hook_binary_missing(
                hook.path.clone(),
                hook.event.clone(),
                hook.matcher.clone(),
                hook.hook_type.clone(),
                hook.command.clone(),
                hook.binary_path.clone(),
            ));
        }
    }

    findings.sort_by(|left, right| {
        let left_key = finding_sort_key(left);
        let right_key = finding_sort_key(right);
        left_key.cmp(&right_key)
    });

    let baseline_path =
        baseline_path().unwrap_or_else(|| PathBuf::from("~/.local/share/stipe/init-baseline.json"));
    Ok(DriftReport {
        baseline_path,
        generated_at_unix_nanos: now_unix_nanos(),
        findings,
    })
}

fn finding_sort_key(finding: &DriftFinding) -> String {
    match finding {
        DriftFinding::MissingMcpRegistration {
            config_path, name, ..
        } => format!("mcp-registration:{}:{}", config_path.display(), name),
        DriftFinding::MissingMcpBinary {
            config_path, name, ..
        } => format!("mcp-binary:{}:{}", config_path.display(), name),
        DriftFinding::MissingHookRegistration {
            config_path, event, ..
        } => format!("hook-registration:{}:{}", config_path.display(), event),
        DriftFinding::MissingHookBinary {
            config_path, event, ..
        } => format!("hook-binary:{}:{}", config_path.display(), event),
        DriftFinding::ConfigFileModified { path, .. } => {
            format!("config-modified:{}", path.display())
        }
    }
}

pub(crate) fn evaluate_drift() -> Result<Option<DriftReport>> {
    let Some(manifest) = load_baseline()? else {
        return Ok(None);
    };

    Ok(Some(evaluate_drift_from_manifest(&manifest)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("stipe-baseline-{name}-{unique}"))
    }

    #[test]
    fn checksum_bytes_changes_with_content() {
        assert_ne!(checksum_bytes(b"alpha"), checksum_bytes(b"beta"));
    }

    #[test]
    fn evaluate_drift_reports_modified_config_file() {
        let file = temp_path("config.json");
        fs::write(&file, "{\"mcpServers\":{}}").unwrap();

        let manifest = InitBaselineManifest {
            schema_version: BASELINE_SCHEMA_VERSION.to_string(),
            generated_at_unix_nanos: 1,
            target_client: None,
            scope: "user".to_string(),
            config_files: vec![BaselineConfigFile {
                path: file.clone(),
                checksum: checksum_file(&file).unwrap(),
            }],
            mcp_servers: Vec::new(),
            hooks: Vec::new(),
        };

        fs::write(&file, "{\"mcpServers\":{\"hyphae\":{}}}").unwrap();
        let report = evaluate_drift_from_manifest(&manifest).unwrap();

        assert!(matches!(
            report.findings.as_slice(),
            [DriftFinding::ConfigFileModified { path, .. }] if path == &file
        ));

        let _ = fs::remove_file(&file);
    }

    #[test]
    fn evaluate_drift_reports_missing_mcp_registration() {
        let file = temp_path("mcp.json");
        fs::write(&file, "{\"mcpServers\":{}}").unwrap();

        let manifest = InitBaselineManifest {
            schema_version: BASELINE_SCHEMA_VERSION.to_string(),
            generated_at_unix_nanos: 1,
            target_client: None,
            scope: "user".to_string(),
            config_files: vec![BaselineConfigFile {
                path: file.clone(),
                checksum: checksum_file(&file).unwrap(),
            }],
            mcp_servers: vec![BaselineMcpServer {
                path: file.clone(),
                name: "hyphae".to_string(),
                command: "hyphae".to_string(),
                args: vec!["serve".to_string()],
                binary_path: PathBuf::from("/tmp/missing-hyphae"),
            }],
            hooks: Vec::new(),
        };

        let report = evaluate_drift_from_manifest(&manifest).unwrap();
        assert!(report
            .findings
            .iter()
            .any(|finding| matches!(finding, DriftFinding::MissingMcpRegistration { name, .. } if name == "hyphae")));

        let _ = fs::remove_file(&file);
    }
}
