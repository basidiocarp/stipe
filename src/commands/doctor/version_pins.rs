use std::collections::HashMap;
use std::sync::OnceLock;

/// Pinned tool versions from ecosystem-versions.toml (the [tools] table).
///
/// This is the canonical source for version drift detection. When this crate is
/// built from the workspace the values are current. When installed as a release
/// binary the values reflect the ecosystem state at release time.
///
/// These versions must stay synchronized with ecosystem-versions.toml [tools] table.
pub fn pinned_ecosystem_versions() -> HashMap<&'static str, &'static str> {
    static PINS: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    PINS.get_or_init(|| {
        let mut pins = HashMap::new();
        pins.insert("mycelium", "0.11.1");
        pins.insert("hyphae", "0.15.1");
        pins.insert("rhizome", "0.11.1");
        pins.insert("canopy", "0.9.1");
        pins.insert("cortina", "0.6.0");
        pins.insert("stipe", "0.8.5");
        pins.insert("volva", "0.3.2");
        pins.insert("hymenium", "0.8.2");
        pins.insert("annulus", "0.7.2");
        pins.insert("cap", "0.13.0");
        pins.insert("lamella", "0.5.15");
        pins
    })
    .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tool_registry;

    #[test]
    fn all_installable_specs_have_version_pins() {
        let pins = pinned_ecosystem_versions();
        for spec in tool_registry::installable_specs() {
            assert!(
                pins.contains_key(spec.name),
                "tool '{}' is installable but has no version pin",
                spec.name
            );
        }
    }
}
