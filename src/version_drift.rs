//! Pure helpers for the build script's `ecosystem-versions.toml` `[tools]` handling.
//!
//! This logic lives in `src/` rather than inline in `build.rs` so it is testable:
//! Cargo does **not** run unit tests defined inside a build script, but it does run
//! integration tests under `tests/`. `build.rs` includes this module via
//! `#[path = "src/version_drift.rs"]`, and `tests/version_drift.rs` includes the same
//! file — so the `#[cfg(test)] mod tests` below actually executes under `cargo test`.

use std::collections::{BTreeMap, HashSet};

/// Tools tracked in `ecosystem-versions.toml` `[tools]` that stipe does not manage
/// as installable binaries. spore is a shared library, not a standalone binary.
pub const SKIP: &[&str] = &["spore"];

/// Extract the `[tools]` table from a parsed TOML document, filtering out SKIP entries.
/// Returns a map of (tool name → version string). Both sides of a drift comparison use this.
pub fn extract_tools(doc: &toml::Value) -> BTreeMap<&str, &str> {
    doc.get("tools")
        .and_then(toml::Value::as_table)
        .map(|tools| {
            tools
                .iter()
                .filter(|(k, _)| !SKIP.contains(&k.as_str()))
                .filter_map(|(k, v)| v.as_str().map(|s| (k.as_str(), s)))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default()
}

/// Compare two `[tools]` tables and return a list of `(key, local_version, root_version)` tuples
/// for all entries that differ or exist in only one. Empty vec means the tables match.
pub fn tools_diff(
    local: &toml::Value,
    root: &toml::Value,
) -> Vec<(String, Option<String>, Option<String>)> {
    let local_tools = extract_tools(local);
    let root_tools = extract_tools(root);

    let mut all_keys = HashSet::new();
    all_keys.extend(local_tools.keys().copied());
    all_keys.extend(root_tools.keys().copied());

    let mut diffs = Vec::new();
    for key in all_keys {
        let local_ver = local_tools.get(key).map(ToString::to_string);
        let root_ver = root_tools.get(key).map(ToString::to_string);
        if local_ver != root_ver {
            diffs.push((key.to_string(), local_ver, root_ver));
        }
    }

    diffs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_tools_no_diff() {
        let toml_str = r#"
[tools]
mycelium = "0.12.0"
hyphae = "0.16.0"
spore = "0.6.3"
"#;
        let doc = toml::from_str(toml_str).unwrap();
        let diffs = tools_diff(&doc, &doc);
        assert!(diffs.is_empty(), "Equal [tools] should produce no diffs");
    }

    #[test]
    fn differing_version_one_entry() {
        let local_str = r#"
[tools]
mycelium = "0.11.0"
hyphae = "0.16.0"
"#;
        let root_str = r#"
[tools]
mycelium = "0.12.0"
hyphae = "0.16.0"
"#;
        let local_doc = toml::from_str(local_str).unwrap();
        let root_doc = toml::from_str(root_str).unwrap();
        let diffs = tools_diff(&local_doc, &root_doc);
        assert_eq!(
            diffs.len(),
            1,
            "One differing version should produce one diff"
        );
        assert_eq!(diffs[0].0, "mycelium");
        assert_eq!(diffs[0].1, Some("0.11.0".to_string()));
        assert_eq!(diffs[0].2, Some("0.12.0".to_string()));
    }

    #[test]
    fn key_present_only_in_local() {
        let local_str = r#"
[tools]
mycelium = "0.12.0"
canopy = "0.12.0"
"#;
        let root_str = r#"
[tools]
mycelium = "0.12.0"
"#;
        let local_doc = toml::from_str(local_str).unwrap();
        let root_doc = toml::from_str(root_str).unwrap();
        let diffs = tools_diff(&local_doc, &root_doc);
        assert_eq!(
            diffs.len(),
            1,
            "Key present only in local should produce one diff"
        );
        assert_eq!(diffs[0].0, "canopy");
        assert_eq!(diffs[0].1, Some("0.12.0".to_string()));
        assert_eq!(diffs[0].2, None);
    }

    #[test]
    fn key_present_only_in_root() {
        let local_str = r#"
[tools]
mycelium = "0.12.0"
"#;
        let root_str = r#"
[tools]
mycelium = "0.12.0"
canopy = "0.12.0"
"#;
        let local_doc = toml::from_str(local_str).unwrap();
        let root_doc = toml::from_str(root_str).unwrap();
        let diffs = tools_diff(&local_doc, &root_doc);
        assert_eq!(
            diffs.len(),
            1,
            "Key present only in root should produce one diff"
        );
        assert_eq!(diffs[0].0, "canopy");
        assert_eq!(diffs[0].1, None);
        assert_eq!(diffs[0].2, Some("0.12.0".to_string()));
    }

    #[test]
    fn skip_filter_applied() {
        let local_str = r#"
[tools]
mycelium = "0.12.0"
spore = "0.6.1"
"#;
        let root_str = r#"
[tools]
mycelium = "0.12.0"
spore = "0.6.3"
"#;
        let local_doc = toml::from_str(local_str).unwrap();
        let root_doc = toml::from_str(root_str).unwrap();
        let diffs = tools_diff(&local_doc, &root_doc);
        assert!(
            diffs.is_empty(),
            "Differing spore versions should be ignored (in SKIP list)"
        );
    }
}
