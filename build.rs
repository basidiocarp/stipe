use std::{collections::BTreeMap, env, fmt::Write, fs, path::PathBuf};

// Tools tracked in ecosystem-versions.toml [tools] that stipe does not manage
// as installable binaries. spore is a shared library, not a standalone binary.
const SKIP: &[&str] = &["spore"];

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // Prefer a repo-local copy (used in CI and standalone builds) over the
    // monorepo sibling path (used in local development). Both are watched so
    // whichever is present triggers a rebuild when changed.
    let local = manifest_dir.join("ecosystem-versions.toml");
    let sibling = manifest_dir.join("../ecosystem-versions.toml");
    let toml_path = if local.exists() { local } else { sibling };

    println!("cargo:rerun-if-changed={}", toml_path.display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("ecosystem-versions.toml").display()
    );

    let content = fs::read_to_string(&toml_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", toml_path.display()));

    let doc: toml::Value = toml::from_str(&content)
        .unwrap_or_else(|e| panic!("cannot parse ecosystem-versions.toml: {e}"));

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
