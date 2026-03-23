use anyhow::Result;
use clap::ValueEnum;
use colored::Colorize;

use super::host_policy;
use super::init;
use super::install::{self, InstallProfile};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum HostMode {
    ClaudeCode,
    Codex,
    Cursor,
}

impl HostMode {
    fn client_flag(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => host_policy::CODEX_CLIENT_FLAG,
            Self::Cursor => "cursor",
        }
    }

    fn install_profile(self) -> InstallProfile {
        match self {
            Self::ClaudeCode => InstallProfile::ClaudeCode,
            Self::Codex => InstallProfile::Codex,
            Self::Cursor => InstallProfile::Cursor,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::ClaudeCode => host_policy::CLAUDE_CODE_HOST_MODE_LABEL,
            Self::Codex => host_policy::CODEX_HOST_MODE_LABEL,
            Self::Cursor => host_policy::CURSOR_HOST_MODE_LABEL,
        }
    }
}

pub fn run(mode: HostMode, dry_run: bool) -> Result<()> {
    if dry_run {
        println!("{} {}", "Planning".bold(), mode.label().bold());
        println!(
            "{}",
            "This runs the matching install profile and then targets init at the selected host."
                .dimmed()
        );
        println!();
    }

    install::run(false, Some(mode.install_profile()), dry_run, &[])?;
    init::run(Some(mode.client_flag()), dry_run, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_mode_mappings_are_explicit() {
        assert_eq!(HostMode::Codex.client_flag(), "codex");
        assert_eq!(HostMode::Codex.install_profile(), InstallProfile::Codex);
        assert_eq!(HostMode::ClaudeCode.client_flag(), "claude-code");
        assert_eq!(
            HostMode::ClaudeCode.install_profile(),
            InstallProfile::ClaudeCode
        );
        assert_eq!(HostMode::Cursor.client_flag(), "cursor");
        assert_eq!(HostMode::Cursor.install_profile(), InstallProfile::Cursor);
    }
}
