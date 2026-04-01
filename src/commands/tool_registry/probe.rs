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

fn spore_tool(binary_name: &str) -> Option<Tool> {
    match binary_name {
        "mycelium" => Some(Tool::Mycelium),
        "hyphae" => Some(Tool::Hyphae),
        "rhizome" => Some(Tool::Rhizome),
        "cortina" => Some(Tool::Cortina),
        "canopy" => Some(Tool::Canopy),
        "cap" => Some(Tool::Cap),
        _ => None,
    }
}

#[must_use]
pub fn resolve_binary_path(spec: &ToolSpec) -> Option<PathBuf> {
    if let Some(tool) = spore_tool(spec.binary_name) {
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
    use super::{VerifyLevel, spore_tool};

    #[test]
    fn supported_ecosystem_binaries_map_to_spore_tools() {
        assert_eq!(spore_tool("mycelium"), Some(spore::Tool::Mycelium));
        assert_eq!(spore_tool("hyphae"), Some(spore::Tool::Hyphae));
        assert_eq!(spore_tool("rhizome"), Some(spore::Tool::Rhizome));
        assert_eq!(spore_tool("cortina"), Some(spore::Tool::Cortina));
        assert_eq!(spore_tool("canopy"), Some(spore::Tool::Canopy));
        assert_eq!(spore_tool("cap"), Some(spore::Tool::Cap));
    }

    #[test]
    fn unmanaged_binaries_stay_outside_spore_mapping() {
        assert_eq!(spore_tool("stipe"), None);
    }

    #[test]
    fn verify_levels_are_ordered_from_shallow_to_deep() {
        assert!(VerifyLevel::Version < VerifyLevel::Functional);
        assert!(VerifyLevel::Functional < VerifyLevel::McpHandshake);
    }
}
