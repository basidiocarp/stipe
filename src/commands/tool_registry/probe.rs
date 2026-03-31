use std::process::Command;

use spore::{Tool, discover};

use super::{ToolProbe, ToolSpec};

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
pub fn probe(spec: &ToolSpec) -> ToolProbe {
    let binary_path = if let Some(tool) = spore_tool(spec.binary_name) {
        let Some(info) = discover(tool) else {
            return ToolProbe::Missing;
        };
        info.binary_path
    } else {
        let Ok(binary_path) = which::which(spec.binary_name) else {
            return ToolProbe::Missing;
        };
        binary_path
    };

    match Command::new(&binary_path).arg("--version").output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let version = parse_version(&stdout);
            ToolProbe::Installed(version)
        }
        Ok(_) | Err(_) => ToolProbe::Broken,
    }
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
    use super::spore_tool;

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
}
