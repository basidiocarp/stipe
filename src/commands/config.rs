use anyhow::{Context, Result, anyhow};
use clap::Subcommand;

use crate::commands::install;

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Set a configuration value
    Set { key: String, value: String },
}

pub fn run(command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Set { key, value } => set_config(&key, &value),
    }
}

fn set_config(key: &str, value: &str) -> Result<()> {
    // Parse doctor.suppress.<slug>
    if !key.starts_with("doctor.suppress.") {
        return Err(anyhow!(
            "unknown config key '{}'; supported keys: doctor.suppress.<slug>",
            key
        ));
    }

    let slug = key
        .strip_prefix("doctor.suppress.")
        .expect("already validated prefix");

    if slug.is_empty() {
        return Err(anyhow!("doctor.suppress requires a check slug"));
    }

    // Parse value as bool
    let suppressed = match value.to_lowercase().as_str() {
        "true" | "1" | "yes" => true,
        "false" | "0" | "no" => false,
        _ => {
            return Err(anyhow!(
                "invalid value '{}'; must be true/false (also accepts yes/no, 1/0)",
                value
            ));
        }
    };

    // Load the saved profile to get the canonical path
    let saved_profile = install::load_saved_profile()
        .ok_or_else(|| anyhow!("no install profile saved; run `stipe setup` first to create one before suppressing doctor checks"))?;

    // Set the suppression using the canonical path from the saved profile
    install::set_doctor_suppression(&saved_profile.path, slug, suppressed)
        .with_context(|| format!("setting {}", key))
}
