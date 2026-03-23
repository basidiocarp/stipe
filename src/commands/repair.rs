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
        label: String,
        description: String,
        command: String,
        args: Vec<String>,
        tier: RepairTier,
    ) -> Self {
        Self {
            action_key: None,
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
    RepairAction::manual(
        label,
        format!("Install {tool} from crates.io."),
        format!("cargo install {tool}"),
        vec!["install".to_string(), tool.to_string()],
        RepairTier::Manual,
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
