use colored::Colorize;
use std::process::Command;

use crate::commands::tool_registry::{self, ToolProbe};

use super::clients::ServerConfig;
use super::context::EcosystemContext;

pub(super) fn tool_probe(tool_name: &str) -> ToolProbe {
    tool_registry::find(tool_name).map_or(ToolProbe::Missing, tool_registry::probe)
}

pub(super) fn discover_codex_version() -> Option<String> {
    let output = Command::new("codex").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().last())
        .filter(|v| v.contains('.'))
        .unwrap_or("unknown")
        .to_string();
    Some(version)
}

pub(super) fn claude_is_available() -> bool {
    Command::new("claude")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

pub(super) fn render_tool_status(
    name: &str,
    version: Option<&str>,
    probe: &ToolProbe,
    colorize: bool,
) -> String {
    let raw_name = name;
    let name = if colorize {
        name.bold().to_string()
    } else {
        name.to_string()
    };

    if let Some(version) = version {
        let installed = if colorize {
            "✓ installed".green().to_string()
        } else {
            "✓ installed".to_string()
        };
        format!("  {name:<10}v{version:<8}{installed}")
    } else if matches!(probe, ToolProbe::Broken) {
        let broken = if colorize {
            "✗ installed but broken".red().to_string()
        } else {
            "✗ installed but broken".to_string()
        };
        let indicator = "!";
        format!("  {name:<10}{indicator:<8} {broken}")
    } else {
        let hint = match raw_name {
            "canopy" => " (optional outside the coordination runtime path: stipe install canopy)",
            "cap" => {
                " (optional: git clone https://github.com/basidiocarp/cap && cd cap && npm i && npm run dev:all)"
            }
            _ => "",
        };
        let missing = if colorize {
            "✗ not installed".red().to_string()
        } else {
            "✗ not installed".to_string()
        };
        let hint = if colorize {
            hint.dimmed().to_string()
        } else {
            hint.to_string()
        };
        let indicator = "—";
        format!("  {name:<10}{indicator:<8} {missing}{hint}")
    }
}

pub(super) fn render_status_report(context: &EcosystemContext, colorize: bool) -> Vec<String> {
    let mut lines = vec![
        String::new(),
        if colorize {
            "Basidiocarp Ecosystem Status".bold().to_string()
        } else {
            "Basidiocarp Ecosystem Status".to_string()
        },
        "─".repeat(75),
        String::new(),
    ];

    for spec in tool_registry::ecosystem_specs() {
        let Some(probe) = context.probe_for_tool(spec.name) else {
            continue;
        };
        lines.push(render_tool_status(
            spec.name,
            probe.version(),
            probe,
            colorize,
        ));
    }

    let codex_probe = context.codex_probe();
    lines.push(render_tool_status(
        "codex",
        context.codex_version.as_deref(),
        &codex_probe,
        colorize,
    ));
    lines.push(String::new());
    lines
}

pub(super) fn print_status_report(context: &EcosystemContext) {
    for line in render_status_report(context, true) {
        println!("{line}");
    }
}

pub(super) fn build_server_configs() -> Vec<ServerConfig> {
    build_ecosystem_servers(
        tool_probe("hyphae").is_installed(),
        tool_probe("rhizome").is_installed(),
    )
}

pub(super) fn build_ecosystem_servers(
    hyphae_installed: bool,
    rhizome_installed: bool,
) -> Vec<ServerConfig> {
    let mut servers = Vec::new();
    if hyphae_installed {
        servers.push(ServerConfig {
            name: "hyphae".to_string(),
            command: "hyphae".to_string(),
            args: vec!["serve".to_string()],
        });
    }
    if rhizome_installed {
        servers.push(ServerConfig {
            name: "rhizome".to_string(),
            command: "rhizome".to_string(),
            args: vec!["serve".to_string(), "--expanded".to_string()],
        });
    }
    servers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::host_policy::HostMode;

    #[test]
    fn test_render_tool_status_snapshot_for_optional_missing() {
        assert_eq!(
            render_tool_status("canopy", None, &ToolProbe::Missing, false),
            "  canopy    —        ✗ not installed (optional outside the coordination runtime path: stipe install canopy)"
        );
    }

    #[test]
    fn test_render_tool_status_snapshot_for_installed() {
        assert_eq!(
            render_tool_status(
                "mycelium",
                Some("0.7.4"),
                &ToolProbe::Installed("0.7.4".to_string()),
                false
            ),
            "  mycelium  v0.7.4   ✓ installed"
        );
    }

    #[test]
    fn test_render_status_report_snapshot() {
        let context = EcosystemContext {
            target_host: Some(HostMode::Codex),
            claude_runtime_relevant: false,
            mycelium_probe: ToolProbe::Installed("0.7.4".to_string()),
            hyphae_probe: ToolProbe::Missing,
            rhizome_probe: ToolProbe::Installed("0.4.0".to_string()),
            canopy_probe: ToolProbe::Missing,
            cortina_probe: ToolProbe::Broken,
            cap_probe: ToolProbe::Missing,
            codex_version: Some("0.31.0".to_string()),
        };

        assert_eq!(
            render_status_report(&context, false),
            vec![
                String::new(),
                "Basidiocarp Ecosystem Status".to_string(),
                "─".repeat(75),
                String::new(),
                "  mycelium  v0.7.4   ✓ installed".to_string(),
                "  hyphae    —        ✗ not installed".to_string(),
                "  rhizome   v0.4.0   ✓ installed".to_string(),
                "  canopy    —        ✗ not installed (optional outside the coordination runtime path: stipe install canopy)".to_string(),
                "  cortina   !        ✗ installed but broken".to_string(),
                "  cap       —        ✗ not installed (optional: git clone https://github.com/basidiocarp/cap && cd cap && npm i && npm run dev:all)".to_string(),
                "  codex     v0.31.0  ✓ installed".to_string(),
                String::new(),
            ]
        );
    }

    #[test]
    fn test_installed_version_does_not_panic_for_optional_tool() {
        let _result = tool_probe("cap");
    }

    #[test]
    fn test_discover_codex_version_does_not_panic() {
        let _result = discover_codex_version();
    }

    #[test]
    fn test_claude_is_available_does_not_panic() {
        let _result = claude_is_available();
    }
}
