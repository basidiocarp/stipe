use std::{collections::BTreeMap, env, fmt::Write, fs, path::PathBuf};

// Pure drift/extraction helpers live in src/version_drift.rs so they are unit-testable:
// Cargo does not run tests defined inside a build script, but tests/version_drift.rs
// includes the same module and exercises them under `cargo test`.
#[path = "src/version_drift.rs"]
mod version_drift;
use version_drift::{SKIP, tools_diff};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // Prefer the root SSOT sibling (used in monorepo development) when present.
    // If absent (standalone CI/release checkout), fall back to the local copy.
    // Both are watched so either file changing triggers a rebuild.
    let local = manifest_dir.join("ecosystem-versions.toml");
    let sibling = manifest_dir.join("../ecosystem-versions.toml");
    let toml_path = if sibling.exists() {
        sibling.clone()
    } else {
        local.clone()
    };

    println!("cargo:rerun-if-changed={}", toml_path.display());
    println!("cargo:rerun-if-changed={}", local.display());

    let content = fs::read_to_string(&toml_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", toml_path.display()));

    let doc: toml::Value = toml::from_str(&content)
        .unwrap_or_else(|e| panic!("cannot parse ecosystem-versions.toml: {e}"));

    // If both files exist, check for drift in the [tools] section.
    // This catches silent misalignment in the monorepo.
    if local.exists() && sibling.exists() {
        let local_content = fs::read_to_string(&local)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", local.display()));
        let local_doc: toml::Value = toml::from_str(&local_content)
            .unwrap_or_else(|e| panic!("cannot parse {}: {e}", local.display()));

        let diffs = tools_diff(&local_doc, &doc);
        if !diffs.is_empty() {
            let mut msg = format!(
                "ecosystem-versions [tools] drift: {} disagrees with root {} on:\n",
                local.display(),
                sibling.display()
            );
            for (key, local_ver, root_ver) in diffs {
                let local_str = local_ver.as_deref().unwrap_or("(absent)");
                let root_str = root_ver.as_deref().unwrap_or("(absent)");
                let _ = writeln!(msg, "  {key}: local {local_str} vs root {root_str}");
            }
            msg.push_str("The root file is the SSOT — sync the local copy.");
            panic!("{}", msg);
        }
    }

    let tools = doc
        .get("tools")
        .and_then(toml::Value::as_table)
        .unwrap_or_else(|| panic!("ecosystem-versions.toml missing [tools] table"));

    let sorted: BTreeMap<_, _> = tools
        .iter()
        .filter(|(k, _)| !SKIP.contains(&k.as_str()))
        .filter_map(|(k, v)| v.as_str().map(|s| (k.as_str(), s)))
        .collect();

    let mut entries = String::new();
    for (name, version) in &sorted {
        let _ = writeln!(entries, "        pins.insert(\"{name}\", \"{version}\");");
    }

    let generated = format!(
        "// @generated — do not edit. Source: ecosystem-versions.toml [tools] via build.rs.\n\
         use std::collections::HashMap;\n\
         use std::sync::OnceLock;\n\
         \n\
         pub fn pinned_ecosystem_versions() -> HashMap<&'static str, &'static str> {{\n\
             static PINS: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();\n\
             PINS.get_or_init(|| {{\n\
                 let mut pins = HashMap::new();\n\
         {entries}\
                 pins\n\
             }})\n\
             .clone()\n\
         }}\n"
    );

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("version_pins.rs"), generated)
        .unwrap_or_else(|e| panic!("cannot write version_pins.rs: {e}"));
}
