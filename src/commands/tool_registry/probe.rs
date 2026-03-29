use std::process::Command;

use super::{ToolProbe, ToolSpec};

#[must_use]
pub fn probe(spec: &ToolSpec) -> ToolProbe {
    let Ok(binary_path) = which::which(spec.binary_name) else {
        return ToolProbe::Missing;
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
