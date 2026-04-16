//! `stipe provider` — inspect and configure ecosystem providers.
//!
//! Subcommands:
//! - `list`          — show all configured providers with status
//! - `setup <name>`  — guided setup for a specific provider
//!
//! API key values are **never** printed. Presence is reported as `present` or
//! `not set`; format problems are flagged without revealing the value.

use std::io::{self, Write as _};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::commands::doctor::model::ApiKeyStatus;
use crate::commands::doctor::provider_checks::{collect_api_key_health, collect_mcp_health};

// ---------------------------------------------------------------------------
// Public CLI types
// ---------------------------------------------------------------------------

#[derive(Debug, Subcommand)]
pub enum ProviderCommand {
    /// Show all configured providers with status
    List,

    /// Run guided setup for a specific provider
    Setup {
        /// Provider to configure: `anthropic` or `volva`
        provider: String,

        /// Accept defaults without prompting (non-interactive mode)
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn run(command: ProviderCommand) -> Result<()> {
    match command {
        ProviderCommand::List => {
            print_provider_list();
            Ok(())
        }
        ProviderCommand::Setup { provider, yes } => run_setup(&provider, yes),
    }
}

// ---------------------------------------------------------------------------
// provider list
// ---------------------------------------------------------------------------

/// A single row in the provider list table.
struct ProviderRow {
    name: String,
    status: &'static str,
    api_key: &'static str,
    connection: &'static str,
}

fn print_provider_list() {
    let rows = build_provider_rows();

    println!("{:<20} {:<20} {:<12} CONNECTION", "PROVIDER", "STATUS", "API KEY");
    println!("{}", "─".repeat(72));

    for row in &rows {
        println!(
            "{:<20} {:<20} {:<12} {}",
            row.name, row.status, row.api_key, row.connection
        );
    }
}

fn build_provider_rows() -> Vec<ProviderRow> {
    let mut rows = Vec::new();

    // --- API-key-based providers (Anthropic, Volva backend) ---
    for health in collect_api_key_health() {
        let status = match health.status {
            ApiKeyStatus::Configured => "configured",
            ApiKeyStatus::Missing => "missing",
            ApiKeyStatus::UnexpectedFormat => "unexpected-format",
        };
        let api_key = match health.status {
            ApiKeyStatus::Configured => "present",
            _ => "not set",
        };

        rows.push(ProviderRow {
            name: health.provider,
            status,
            api_key,
            connection: "not checked",
        });
    }

    // --- MCP server providers (connection-level reachability) ---
    for mcp in collect_mcp_health() {
        for server in &mcp.registered_servers {
            rows.push(ProviderRow {
                name: format!("mcp:{server}"),
                status: "configured",
                api_key: "not checked",
                connection: if mcp.healthy { "registered" } else { "not checked" },
            });
        }
        for server in &mcp.missing_servers {
            rows.push(ProviderRow {
                name: format!("mcp:{server}"),
                status: "missing",
                api_key: "not checked",
                connection: "not reachable",
            });
        }
    }

    rows
}

// ---------------------------------------------------------------------------
// provider setup
// ---------------------------------------------------------------------------

fn run_setup(provider: &str, yes: bool) -> Result<()> {
    match provider {
        "anthropic" => setup_anthropic(yes),
        "volva" => setup_volva(yes),
        other => bail!(
            "unknown provider `{other}`; supported providers: anthropic, volva"
        ),
    }
}

// ---------------------------------------------------------------------------
// Anthropic setup
// ---------------------------------------------------------------------------

fn setup_anthropic(yes: bool) -> Result<()> {
    let existing = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();

    if !existing.is_empty() {
        // Key is already present — show status and confirm before proceeding.
        println!("ANTHROPIC_API_KEY is already set (value masked as ***).");

        if !yes && !confirm("Replace the existing key? [y/N] ")? {
            println!("No changes made.");
            return Ok(());
        }
    }

    if yes {
        // In non-interactive mode there is no key to supply — surface an error.
        bail!("--yes requires ANTHROPIC_API_KEY to already be set; set it before running with --yes");
    }

    let key = prompt_for_api_key()?;
    validate_anthropic_key(&key)?;
    write_to_env_destination(&key, yes)?;

    println!("\nProvider status after setup:");
    print_provider_list();
    Ok(())
}

fn prompt_for_api_key() -> Result<String> {
    print!("Enter your Anthropic API key (starts with sk-ant-): ");
    io::stdout().flush().context("flushing stdout")?;

    let mut key = String::new();
    io::stdin()
        .read_line(&mut key)
        .context("reading API key from stdin")?;

    Ok(key.trim().to_string())
}

fn validate_anthropic_key(key: &str) -> Result<()> {
    if key.is_empty() {
        bail!("API key cannot be empty");
    }
    if !key.starts_with("sk-ant-") {
        bail!("API key does not match expected format (must start with `sk-ant-`); value not shown");
    }
    Ok(())
}

fn write_to_env_destination(key: &str, yes: bool) -> Result<()> {
    // Ask where to persist the key unless --yes (then default to .env).
    let choice = if yes {
        EnvDestination::DotEnv
    } else {
        prompt_env_destination()?
    };

    match choice {
        EnvDestination::DotEnv => write_to_dotenv(key),
        EnvDestination::ShellProfile => write_to_shell_profile(key),
    }
}

enum EnvDestination {
    DotEnv,
    ShellProfile,
}

fn prompt_env_destination() -> Result<EnvDestination> {
    println!("Where should the key be saved?");
    println!("  1) .env  (current directory)");
    println!("  2) Shell profile (~/.zshrc, ~/.bashrc, etc.)");
    print!("Choice [1]: ");
    io::stdout().flush().context("flushing stdout")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("reading destination choice")?;

    match input.trim() {
        "2" => Ok(EnvDestination::ShellProfile),
        _ => Ok(EnvDestination::DotEnv),
    }
}

fn write_to_dotenv(key: &str) -> Result<()> {
    let path = std::env::current_dir()
        .context("determining current directory")?
        .join(".env");

    let line = format!("ANTHROPIC_API_KEY={key}\n");

    if path.exists() {
        // Append rather than overwrite; avoid duplicating the variable.
        let existing = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;

        if existing.contains("ANTHROPIC_API_KEY=") {
            println!(
                "ANTHROPIC_API_KEY already appears in {}; not overwriting.",
                path.display()
            );
            return Ok(());
        }

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .with_context(|| format!("opening {} for append", path.display()))?;
        file.write_all(line.as_bytes())
            .with_context(|| format!("writing to {}", path.display()))?;
    } else {
        std::fs::write(&path, &line)
            .with_context(|| format!("writing {}", path.display()))?;
    }

    println!("Written to {} (value masked as ***)", path.display());
    println!("Run `source {}` or restart your shell to apply.", path.display());
    Ok(())
}

fn write_to_shell_profile(key: &str) -> Result<()> {
    let profile = detect_shell_profile().context("detecting shell profile path")?;

    let existing = std::fs::read_to_string(&profile)
        .with_context(|| format!("reading {}", profile.display()))?;

    if existing.contains("ANTHROPIC_API_KEY=") {
        println!(
            "ANTHROPIC_API_KEY already appears in {}; not overwriting.",
            profile.display()
        );
        return Ok(());
    }

    let line = format!("\nexport ANTHROPIC_API_KEY={key}\n");

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&profile)
        .with_context(|| format!("opening {} for append", profile.display()))?;
    file.write_all(line.as_bytes())
        .with_context(|| format!("writing to {}", profile.display()))?;

    println!("Written to {} (value masked as ***)", profile.display());
    println!(
        "Run `source {}` or restart your shell to apply.",
        profile.display()
    );
    Ok(())
}

fn detect_shell_profile() -> Option<PathBuf> {
    let home = dirs::home_dir()?;

    // Prefer the shell reported by $SHELL, fall back to common profiles.
    let shell = std::env::var("SHELL").unwrap_or_default();
    if shell.contains("zsh") {
        return Some(home.join(".zshrc"));
    }
    if shell.contains("bash") {
        let bashrc = home.join(".bashrc");
        if bashrc.exists() {
            return Some(bashrc);
        }
        return Some(home.join(".bash_profile"));
    }

    // Generic fallbacks.
    for name in &[".zshrc", ".bashrc", ".bash_profile", ".profile"] {
        let path = home.join(name);
        if path.exists() {
            return Some(path);
        }
    }

    Some(home.join(".profile"))
}

// ---------------------------------------------------------------------------
// Volva setup
// ---------------------------------------------------------------------------

fn setup_volva(yes: bool) -> Result<()> {
    let config_path =
        volva_config_path().context("cannot determine home directory for volva config")?;

    if config_path.exists() {
        println!(
            "Volva backend config already exists at {}.",
            config_path.display()
        );

        if !yes && !confirm("Overwrite the existing config? [y/N] ")? {
            println!("No changes made.");
            return Ok(());
        }
    }

    let parent = config_path
        .parent()
        .context("config path has no parent directory")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("creating directory {}", parent.display()))?;

    let default_config = default_volva_config();
    std::fs::write(&config_path, default_config)
        .with_context(|| format!("writing volva config to {}", config_path.display()))?;

    println!(
        "Default volva config written to {}",
        config_path.display()
    );
    println!("\nNext steps:");
    println!(
        "  1. Edit {} and add your Anthropic API key under the `auth` section.",
        config_path.display()
    );
    println!("  2. Run `volva auth login anthropic` to authenticate.");
    println!("  3. Run `stipe provider list` to verify provider status.");

    Ok(())
}

fn volva_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".volva").join("auth").join("anthropic.json"))
}

fn default_volva_config() -> &'static str {
    r#"{
  "provider": "anthropic",
  "auth": {
    "api_key": ""
  }
}
"#
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt}");
    io::stdout().flush().context("flushing stdout")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("reading confirmation")?;

    Ok(matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_anthropic_key_rejects_empty() {
        assert!(validate_anthropic_key("").is_err());
    }

    #[test]
    fn validate_anthropic_key_rejects_wrong_prefix() {
        let err = validate_anthropic_key("wrong-key").unwrap_err();
        // Error must not leak the key value.
        assert!(!err.to_string().contains("wrong-key"));
    }

    #[test]
    fn validate_anthropic_key_accepts_correct_prefix() {
        assert!(validate_anthropic_key("sk-ant-testkey123").is_ok());
    }

    #[test]
    fn provider_list_builds_without_panic() {
        // Smoke-test: exercises all branches with whatever env state exists.
        let rows = build_provider_rows();
        assert!(!rows.is_empty(), "expected at least one provider row");
    }

    #[test]
    fn provider_list_always_includes_anthropic() {
        let rows = build_provider_rows();
        assert!(
            rows.iter().any(|r| r.name == "anthropic"),
            "anthropic should always appear"
        );
    }

    #[test]
    fn provider_list_api_key_field_never_shows_key_value() {
        let rows = build_provider_rows();
        for row in &rows {
            assert_ne!(
                row.api_key, "***",
                "masked sentinel should not be the display value"
            );
            assert!(
                row.api_key == "present"
                    || row.api_key == "not set"
                    || row.api_key == "not checked",
                "unexpected api_key field value: {}",
                row.api_key
            );
        }
    }

    #[test]
    fn provider_list_status_values_are_known() {
        let rows = build_provider_rows();
        for row in &rows {
            assert!(
                matches!(
                    row.status,
                    "configured" | "missing" | "unexpected-format"
                ),
                "unexpected status: {}",
                row.status
            );
        }
    }

    #[test]
    fn run_setup_unknown_provider_errors() {
        let err = run_setup("unknown-xyz", false).unwrap_err();
        assert!(err.to_string().contains("unknown provider"));
    }

    #[test]
    fn volva_config_path_is_under_home() {
        if let Some(path) = volva_config_path() {
            assert!(
                path.to_string_lossy().contains(".volva"),
                "expected volva path to include .volva"
            );
        }
    }
}
