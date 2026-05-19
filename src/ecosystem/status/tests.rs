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
        annulus_probe: ToolProbe::Missing,
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
            "  annulus   —        ✗ not installed".to_string(),
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

#[test]
fn test_rhizome_proxy_when_socket_running() {
    let servers = build_ecosystem_servers_with_socket_check(false, true, true);
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].name, "rhizome");
    assert_eq!(servers[0].args, vec!["proxy"]);
}

#[test]
fn test_rhizome_serve_expanded_when_no_socket() {
    let servers = build_ecosystem_servers_with_socket_check(false, true, false);
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].args, vec!["serve", "--expanded"]);
}
