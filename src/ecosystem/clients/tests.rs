use super::*;

#[test]
fn test_client_flag_roundtrip() {
    for client in ALL_CLIENTS {
        let flag = client.flag();
        let parsed = McpClient::from_flag(flag);
        assert_eq!(parsed, Some(client), "roundtrip failed for {flag}");
    }
}

#[test]
fn test_client_name_not_empty() {
    for client in ALL_CLIENTS {
        assert!(!client.name().is_empty());
    }
}

#[test]
fn test_from_flag_aliases() {
    assert_eq!(McpClient::from_flag("claude"), Some(McpClient::ClaudeCode));
    assert_eq!(McpClient::from_flag("CURSOR"), Some(McpClient::Cursor));
    assert_eq!(McpClient::from_flag("unknown"), None);
}

#[test]
fn test_shared_editor_mapping_covers_supported_shared_hosts() {
    assert_eq!(McpClient::Cursor.shared_editor(), Some(Editor::Cursor));
    assert_eq!(McpClient::Windsurf.shared_editor(), Some(Editor::Windsurf));
    assert_eq!(McpClient::CodexCli.shared_editor(), Some(Editor::CodexCli));
    assert_eq!(McpClient::Continue.shared_editor(), None);
    assert_eq!(McpClient::Cline.shared_editor(), None);
}

#[test]
fn test_ecosystem_special_case_clients_stay_explicit() {
    assert!(McpClient::ClaudeCode.handled_separately_in_ecosystem());
    assert!(McpClient::CodexCli.handled_separately_in_ecosystem());
    assert!(!McpClient::Cursor.handled_separately_in_ecosystem());
}

#[test]
fn test_collect_detected_clients_preserves_inventory_order() {
    let detected = super::detection::collect_detected_clients(
        &[Editor::CodexCli, Editor::Cursor],
        true,
        true,
        false,
    );

    assert_eq!(
        detected,
        vec![
            McpClient::ClaudeCode,
            McpClient::Cursor,
            McpClient::Cline,
            McpClient::CodexCli,
        ]
    );
}

#[test]
fn test_collect_detected_clients_keeps_claude_hybrid_detection() {
    let detected = super::detection::collect_detected_clients(&[], true, false, false);

    assert_eq!(detected, vec![McpClient::ClaudeCode]);
}

#[test]
fn test_collect_detected_clients_does_not_map_vscode_to_cline() {
    let detected =
        super::detection::collect_detected_clients(&[Editor::VsCode], false, false, false);

    assert!(detected.is_empty());
}

#[test]
fn test_collect_detected_clients_keeps_continue_outside_shared_overlap() {
    let detected =
        super::detection::collect_detected_clients(&[Editor::Cursor], false, false, true);

    assert_eq!(detected, vec![McpClient::Cursor, McpClient::Continue]);
}

#[test]
fn test_shared_host_config_paths_resolve_via_spore() {
    assert_eq!(
        McpClient::Cursor.config_path(),
        editors::config_path(Editor::Cursor).ok()
    );
    assert_eq!(
        McpClient::ClaudeDesktop.config_path(),
        editors::config_path(Editor::ClaudeDesktop).ok()
    );
    assert_eq!(
        McpClient::CodexCli.config_path(),
        editors::config_path(Editor::CodexCli).ok()
    );
}

#[test]
fn test_detect_clients_does_not_panic() {
    let _clients = detect_clients();
}

#[test]
fn test_print_generic_config() {
    let servers = vec![ServerConfig {
        name: "hyphae".to_string(),
        command: "hyphae".to_string(),
        args: vec!["serve".to_string()],
    }];
    print_generic_config(&servers);
}
