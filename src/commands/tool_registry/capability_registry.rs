use std::path::Path;

use anyhow::{Context as _, Result};
use serde_json::json;
use spore::atomic_write_bytes;

use super::model::ToolProbe;
use super::probe::{VerifyLevel, probe_with_level, resolve_binary_path};
use super::specs::installable_specs;

/// Schema version for capability-registry-v1. Must match septa/capability-registry-v1.schema.json.
const CAPABILITY_REGISTRY_SCHEMA_VERSION: &str = "1.0";

/// Write a `capability-registry-v1` snapshot to `path`.
///
/// Iterates all installable tool specs, probes each for its installed version,
/// and records capability ids, contract ids, transport, and binary path.
/// Non-installed tools are included with a `"missing"` health hint so
/// consumers can detect gaps without a separate probe.
///
/// # Errors
///
/// Returns an error if the parent directory cannot be created or the registry
/// file cannot be written.
pub fn write_capability_registry(path: &Path) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());

    let entries: Vec<serde_json::Value> = installable_specs()
        .into_iter()
        .map(|spec| {
            let probe = probe_with_level(spec, VerifyLevel::Version);
            let version = probe.version().unwrap_or("unknown").to_string();
            let binary_path = resolve_binary_path(spec).map(|p| p.display().to_string());

            let health = if matches!(probe, ToolProbe::Missing) {
                json!({ "status": "missing" })
            } else {
                json!({ "status": "ok" })
            };

            let transport = if spec.mcp_serve_args.is_some() {
                "stdio"
            } else {
                "cli"
            };

            let mut entry = json!({
                "tool": spec.name,
                "version": version,
                "manager": "stipe",
                "capability_ids": spec.capability_ids,
                "contract_ids": spec.contract_ids,
                "transport": transport,
                "health": health,
            });

            if let Some(bin_path) = binary_path {
                entry["binary_path"] = json!(bin_path);
            }

            entry
        })
        .collect();

    let registry = json!({
        "schema_version": CAPABILITY_REGISTRY_SCHEMA_VERSION,
        "written_at_unix": now,
        "entries": entries,
    });

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create registry directory {}", parent.display()))?;
    }

    let content = serde_json::to_string_pretty(&registry)?;
    atomic_write_bytes(path, content.as_bytes())
        .with_context(|| format!("cannot write capability registry to {}", path.display()))?;

    Ok(())
}

/// Return the default path for the capability registry file.
#[must_use]
pub fn default_registry_path() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("basidiocarp")
        .join("capability-registry.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_version_is_correct() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());

        let registry = json!({
            "schema_version": CAPABILITY_REGISTRY_SCHEMA_VERSION,
            "written_at_unix": now,
            "entries": [],
        });

        let json_str = serde_json::to_string(&registry).expect("valid json");
        assert!(
            json_str.contains("\"schema_version\":\"1.0\""),
            "schema_version must be \"1.0\", found: {json_str}",
        );
        assert_eq!(
            CAPABILITY_REGISTRY_SCHEMA_VERSION, "1.0",
            "CAPABILITY_REGISTRY_SCHEMA_VERSION constant must be '1.0'"
        );
    }
}
