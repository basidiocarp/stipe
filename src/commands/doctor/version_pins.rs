use std::collections::HashMap;

/// Pinned tool versions from ecosystem-versions.toml (the [tools] table).
///
/// This is the canonical source for version drift detection. When this crate is
/// built from the workspace the values are current. When installed as a release
/// binary the values reflect the ecosystem state at release time.
///
/// These versions must stay synchronized with ecosystem-versions.toml [tools] table.
pub fn pinned_ecosystem_versions() -> HashMap<&'static str, &'static str> {
    let mut pins = HashMap::new();
    pins.insert("mycelium", "0.11.1");
    pins.insert("hyphae", "0.14.2");
    pins.insert("rhizome", "0.8.0");
    pins.insert("canopy", "0.8.1");
    pins.insert("cortina", "0.5.0");
    pins.insert("stipe", "0.8.2");
    pins.insert("volva", "0.3.1");
    pins.insert("hymenium", "0.8.1");
    pins.insert("annulus", "0.7.1");
    pins.insert("cap", "0.13.0");
    pins.insert("lamella", "0.5.15");
    pins.insert("spore", "0.6.0");
    pins
}
