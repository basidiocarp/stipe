use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RepairTier {
    Primary,
    Secondary,
    Manual,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RepairAction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_key: Option<String>,
    pub label: String,
    pub description: String,
    pub command: String,
    pub args: Vec<String>,
    pub tier: RepairTier,
}

impl RepairAction {
    pub fn stipe(
        action_key: &'static str,
        label: &'static str,
        description: &'static str,
        args: &[&str],
        tier: RepairTier,
    ) -> Self {
        let args_vec = args
            .iter()
            .map(|arg| (*arg).to_string())
            .collect::<Vec<_>>();
        Self {
            action_key: Some(action_key.to_string()),
            label: label.to_string(),
            description: description.to_string(),
            command: format!("stipe {}", args.join(" ")),
            args: args_vec,
            tier,
        }
    }

    pub fn manual(
        action_key: String,
        label: String,
        description: String,
        command: String,
        args: Vec<String>,
        tier: RepairTier,
    ) -> Self {
        Self {
            action_key: Some(action_key),
            label,
            description,
            command,
            args,
            tier,
        }
    }
}

pub fn cargo_install_action(tool: &'static str) -> RepairAction {
    let label = format!("Install {}", title_case(tool));
    let action_key = format!("cargo_install_{}", tool);
    RepairAction::manual(
        action_key,
        label,
        format!("Install {tool} from crates.io."),
        format!("cargo install {tool}"),
        vec!["install".to_string(), tool.to_string()],
        RepairTier::Primary,
    )
}

pub fn dedupe_repair_actions(actions: Vec<RepairAction>) -> Vec<RepairAction> {
    let mut deduped = Vec::new();
    for action in actions {
        if !deduped
            .iter()
            .any(|existing: &RepairAction| existing.command == action.command)
        {
            deduped.push(action);
        }
    }
    deduped
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manual_repair_action_has_action_key_and_valid_tier_for_init_plan() {
        // Test a representative manual repair action for init-plan
        let action = RepairAction::manual(
            "test_host_setup".to_string(),
            "Set up Claude Code".to_string(),
            "Initialize Claude Code with MCP registration.".to_string(),
            "stipe host setup claude-code".to_string(),
            vec!["host".to_string(), "setup".to_string(), "claude-code".to_string()],
            RepairTier::Primary,
        );

        // Verify action_key is non-empty
        assert!(action.action_key.is_some());
        assert!(!action.action_key.as_ref().unwrap().is_empty());
        assert_eq!(action.action_key.as_ref().unwrap(), "test_host_setup");

        // Verify tier is a valid init-plan enum value (Primary, Secondary, not Manual)
        assert_eq!(action.tier, RepairTier::Primary);

        // Verify serialization includes action_key
        let json = serde_json::to_string(&action).expect("should serialize");
        assert!(json.contains("test_host_setup"));
        assert!(json.contains("primary")); // kebab-case for Primary
    }

    #[test]
    fn test_cargo_install_action_has_action_key() {
        let action = cargo_install_action("hyphae");

        // Verify action_key is present and follows naming pattern
        assert!(action.action_key.is_some());
        let key = action.action_key.as_ref().unwrap();
        assert!(key.starts_with("cargo_install_"));
        assert!(key.contains("hyphae"));

        // Tier must be a valid init-plan enum value (init-plan rejects "manual").
        assert_eq!(action.tier, RepairTier::Primary);
    }

    #[test]
    fn test_repair_action_manual_serialization_includes_action_key() {
        let action = RepairAction::manual(
            "test_action_key".to_string(),
            "Test Label".to_string(),
            "Test Description".to_string(),
            "test command".to_string(),
            vec!["arg".to_string()],
            RepairTier::Secondary,
        );

        let json = serde_json::json!(&action);
        assert_eq!(json["action_key"], "test_action_key");
        assert_eq!(json["tier"], "secondary");
        assert_eq!(json["label"], "Test Label");
    }
}
