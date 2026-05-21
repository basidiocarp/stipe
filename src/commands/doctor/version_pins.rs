// @generated — do not edit. Source: ecosystem-versions.toml [tools] via build.rs.
include!(concat!(env!("OUT_DIR"), "/version_pins.rs"));

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
