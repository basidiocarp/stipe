use std::path::PathBuf;

use spore::{Tool, discover};

use crate::commands::install::release::{verify_binary, verify_functional, verify_mcp_handshake};

use super::{ToolProbe, ToolSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VerifyLevel {
    Version,
    Functional,
    McpHandshake,
}

#[must_use]
pub fn resolve_binary_path(spec: &ToolSpec) -> Option<PathBuf> {
    if let Some(tool) = Tool::from_binary_name(spec.binary_name) {
        return discover(tool).map(|info| info.binary_path);
    }

    which::which(spec.binary_name).ok()
}

#[must_use]
pub fn probe(spec: &ToolSpec) -> ToolProbe {
    probe_with_level(spec, VerifyLevel::Version)
}

#[must_use]
pub fn probe_with_level(spec: &ToolSpec, level: VerifyLevel) -> ToolProbe {
    let Some(binary_path) = resolve_binary_path(spec) else {
        return ToolProbe::Missing;
    };

    let Ok(version_output) = verify_binary(&binary_path) else {
        return ToolProbe::Broken;
    };

    if level >= VerifyLevel::Functional && verify_functional(&binary_path, spec).is_err() {
        return ToolProbe::Broken;
    }

    if level >= VerifyLevel::McpHandshake && verify_mcp_handshake(&binary_path, spec).is_err() {
        return ToolProbe::Broken;
    }

    ToolProbe::Installed(parse_version(&version_output))
}

#[must_use]
pub fn parse_version(output: &str) -> String {
    output
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().last())
        .filter(|version| version.contains('.'))
        .map_or_else(|| "unknown".to_string(), str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::VerifyLevel;
    use spore::Tool;

    #[test]
    fn supported_ecosystem_binaries_map_to_spore_tools() {
        assert_eq!(Tool::from_binary_name("mycelium"), Some(Tool::Mycelium));
        assert_eq!(Tool::from_binary_name("hyphae"), Some(Tool::Hyphae));
        assert_eq!(Tool::from_binary_name("rhizome"), Some(Tool::Rhizome));
        assert_eq!(Tool::from_binary_name("cortina"), Some(Tool::Cortina));
        assert_eq!(Tool::from_binary_name("canopy"), Some(Tool::Canopy));
        assert_eq!(Tool::from_binary_name("volva"), Some(Tool::Volva));
        assert_eq!(Tool::from_binary_name("cap"), Some(Tool::Cap));
        assert_eq!(Tool::from_binary_name("stipe"), Some(Tool::Stipe));
    }

    #[test]
    fn unknown_binaries_stay_outside_spore_mapping() {
        assert_eq!(Tool::from_binary_name("__unknown__"), None);
    }

    #[test]
    fn verify_levels_are_ordered_from_shallow_to_deep() {
        assert!(VerifyLevel::Version < VerifyLevel::Functional);
        assert!(VerifyLevel::Functional < VerifyLevel::McpHandshake);
    }
}
