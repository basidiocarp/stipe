use std::process::Command;

use crate::commands::repair::{RepairAction, RepairTier};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeveloperToolTier {
    Tier1,
    Tier2,
    Tier3,
}

impl DeveloperToolTier {
    #[must_use]
    pub fn heading(self) -> &'static str {
        match self {
            Self::Tier1 => "Tier 1 — install these (agent + developer benefit)",
            Self::Tier2 => "Tier 2 — install these (developer benefit, occasional agent use)",
            Self::Tier3 => "Tier 3 — install if you want them (developer convenience)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeveloperToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub rationale: &'static str,
    pub tier: DeveloperToolTier,
    pub install_hint: &'static str,
    pub version_args: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DeveloperToolCheck {
    pub name: String,
    pub tier: DeveloperToolTier,
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_hint: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub repair_actions: Vec<RepairAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DeveloperToolsReport {
    pub summary: String,
    pub checks: Vec<DeveloperToolCheck>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub repair_actions: Vec<RepairAction>,
}

const DEVELOPER_TOOLS: &[DeveloperToolSpec] = &[
    DeveloperToolSpec {
        name: "jq",
        description: "JSON processor",
        rationale: "Hard dependency for contract validation and JSON response inspection.",
        tier: DeveloperToolTier::Tier1,
        install_hint: "brew install jq",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "fd",
        description: "fast file finder",
        rationale: "Replaces slow glob and find patterns while respecting .gitignore by default.",
        tier: DeveloperToolTier::Tier1,
        install_hint: "brew install fd",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "rg",
        description: "fast recursive search",
        rationale: "Primary text search primitive across the workspace.",
        tier: DeveloperToolTier::Tier1,
        install_hint: "brew install ripgrep",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "shellcheck",
        description: "shell script validation",
        rationale: "Catches shell portability and quoting issues before they land in scripts.",
        tier: DeveloperToolTier::Tier1,
        install_hint: "brew install shellcheck",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "tokei",
        description: "language-aware line counts",
        rationale: "More accurate than wc -l for real code-size and comment-density checks.",
        tier: DeveloperToolTier::Tier1,
        install_hint: "brew install tokei",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "yq",
        description: "YAML processor",
        rationale: "Useful for workflow, manifest, and structured config inspection.",
        tier: DeveloperToolTier::Tier2,
        install_hint: "brew install yq",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "ast-grep",
        description: "structural code search",
        rationale: "Useful when text grep is too blunt for refactors or audits.",
        tier: DeveloperToolTier::Tier2,
        install_hint: "brew install ast-grep",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "scc",
        description: "code complexity metrics",
        rationale: "Fast high-level codebase metrics for audits and repo comparisons.",
        tier: DeveloperToolTier::Tier2,
        install_hint: "brew install scc",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "bat",
        description: "syntax-highlighted cat",
        rationale: "Helpful when inspecting printed output with line numbers and syntax awareness.",
        tier: DeveloperToolTier::Tier2,
        install_hint: "brew install bat",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "dust",
        description: "disk usage analysis",
        rationale: "Useful for debugging oversized target directories and build caches.",
        tier: DeveloperToolTier::Tier2,
        install_hint: "brew install dust",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "difft",
        description: "structural diff viewer",
        rationale: "Better semantic diffs than plain text hunks when reviewing code movement.",
        tier: DeveloperToolTier::Tier2,
        install_hint: "brew install difftastic",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "hyperfine",
        description: "benchmarking",
        rationale: "Fast command benchmarking for CLI and filter performance work.",
        tier: DeveloperToolTier::Tier2,
        install_hint: "brew install hyperfine",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "just",
        description: "task runner",
        rationale: "Could replace ad hoc shell entrypoints with consistent per-project recipes.",
        tier: DeveloperToolTier::Tier2,
        install_hint: "brew install just",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "cargo-nextest",
        description: "parallel Rust test runner",
        rationale: "Useful for large Rust suites and cleaner test output.",
        tier: DeveloperToolTier::Tier2,
        install_hint: "brew install cargo-nextest",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "cargo-deny",
        description: "dependency and license auditing",
        rationale: "Covers license and vulnerability checks across the Rust repos.",
        tier: DeveloperToolTier::Tier2,
        install_hint: "brew install cargo-deny",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "comby",
        description: "structural rewrite",
        rationale: "Useful for targeted multi-file codemods and structural rewrites.",
        tier: DeveloperToolTier::Tier2,
        install_hint: "brew install comby",
        version_args: &["-version"],
    },
    DeveloperToolSpec {
        name: "bandwhich",
        description: "network monitoring by process",
        rationale: "Useful when diagnosing slow or hung local network activity by tool.",
        tier: DeveloperToolTier::Tier2,
        install_hint: "brew install bandwhich",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "cargo-insta",
        description: "snapshot review workflow",
        rationale: "Supports the existing insta-heavy snapshot workflow across Rust repos.",
        tier: DeveloperToolTier::Tier2,
        install_hint: "cargo install cargo-insta",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "sd",
        description: "find-replace",
        rationale: "Convenient command-line text replacement for quick refactors.",
        tier: DeveloperToolTier::Tier3,
        install_hint: "brew install sd",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "zoxide",
        description: "smart directory jumping",
        rationale: "Helpful when switching repeatedly among many repo roots.",
        tier: DeveloperToolTier::Tier3,
        install_hint: "brew install zoxide",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "procs",
        description: "structured process listing",
        rationale: "Better process inspection for hung LSPs and orphaned runtimes.",
        tier: DeveloperToolTier::Tier3,
        install_hint: "brew install procs",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "watchexec",
        description: "file watcher",
        rationale: "Convenient for local rebuild or verification loops.",
        tier: DeveloperToolTier::Tier3,
        install_hint: "brew install watchexec",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "delta",
        description: "syntax-highlighted diffs",
        rationale: "Makes Git diff output easier to review from the terminal.",
        tier: DeveloperToolTier::Tier3,
        install_hint: "brew install git-delta",
        version_args: &["--version"],
    },
    DeveloperToolSpec {
        name: "cargo-outdated",
        description: "Rust dependency drift check",
        rationale: "Useful for ecosystem version maintenance and cargo upgrade audits.",
        tier: DeveloperToolTier::Tier3,
        install_hint: "cargo install cargo-outdated",
        version_args: &["--version"],
    },
];

#[must_use]
pub fn find(name: &str) -> Option<&'static DeveloperToolSpec> {
    DEVELOPER_TOOLS.iter().find(|spec| spec.name == name)
}

#[must_use]
pub fn unknown_requested_tools(requested: &[String]) -> Vec<String> {
    requested
        .iter()
        .filter(|name| find(name).is_none())
        .cloned()
        .collect()
}

fn selected_specs(requested: &[String]) -> Vec<&'static DeveloperToolSpec> {
    if requested.is_empty() {
        return DEVELOPER_TOOLS.iter().collect();
    }

    requested.iter().filter_map(|name| find(name)).collect()
}

#[must_use]
pub fn install_report(requested: &[String]) -> DeveloperToolsReport {
    build_report(&selected_specs(requested))
}

#[must_use]
pub fn doctor_report() -> DeveloperToolsReport {
    build_report(&DEVELOPER_TOOLS.iter().collect::<Vec<_>>())
}

fn build_report(selected: &[&DeveloperToolSpec]) -> DeveloperToolsReport {
    let checks = selected.iter().copied().map(check_tool).collect::<Vec<_>>();
    let repair_actions = checks
        .iter()
        .flat_map(|check| check.repair_actions.clone())
        .collect::<Vec<_>>();
    let missing_tier1 = checks
        .iter()
        .filter(|check| !check.installed && check.tier == DeveloperToolTier::Tier1)
        .count();

    let summary = if checks.is_empty() {
        "No developer tools selected.".to_string()
    } else if missing_tier1 == 0 {
        "Tier 1 developer tools are installed.".to_string()
    } else if missing_tier1 == 1 {
        "1 Tier 1 developer tool is missing.".to_string()
    } else {
        format!("{missing_tier1} Tier 1 developer tools are missing.")
    };

    DeveloperToolsReport {
        summary,
        checks,
        repair_actions,
    }
}

#[must_use]
pub fn render_install_advice(report: &DeveloperToolsReport) -> Vec<String> {
    let mut lines = vec![
        "Developer tools are advisory only; stipe does not install or update them.".to_string(),
        "Use your package manager to add the ones you want.".to_string(),
        String::new(),
    ];
    lines.extend(render_report(report));
    lines
}

#[must_use]
pub fn render_report(report: &DeveloperToolsReport) -> Vec<String> {
    let mut lines = vec![
        "Developer Tools".to_string(),
        "─".repeat(75),
        report.summary.clone(),
        String::new(),
    ];

    for tier in [
        DeveloperToolTier::Tier1,
        DeveloperToolTier::Tier2,
        DeveloperToolTier::Tier3,
    ] {
        let tier_checks = report
            .checks
            .iter()
            .filter(|check| check.tier == tier)
            .collect::<Vec<_>>();
        if tier_checks.is_empty() {
            continue;
        }

        lines.push(tier.heading().to_string());
        for check in tier_checks {
            let status = if check.installed {
                match check.version.as_deref() {
                    Some(version) => format!("✓ {version}"),
                    None => "✓ installed".to_string(),
                }
            } else {
                "✗ missing".to_string()
            };
            lines.push(format!(
                "  {:<18} {:<16} {}",
                check.name, status, check.message
            ));
            if let Some(hint) = &check.install_hint {
                lines.push(format!("    → {hint}"));
            }
        }
        lines.push(String::new());
    }

    lines
}

fn check_tool(spec: &DeveloperToolSpec) -> DeveloperToolCheck {
    let binary_path = which::which(spec.name).ok();
    let version = binary_path.as_ref().and_then(|binary_path| {
        let output = Command::new(binary_path)
            .args(spec.version_args)
            .output()
            .ok()?;
        output.status.success().then(|| {
            parse_version(&String::from_utf8_lossy(&output.stdout))
                .or_else(|| parse_version(&String::from_utf8_lossy(&output.stderr)))
                .unwrap_or_else(|| "unknown".to_string())
        })
    });
    let installed = binary_path.is_some();
    let install_hint = (!installed).then(|| spec.install_hint.to_string());
    let repair_actions = install_hint
        .as_ref()
        .map(|hint| {
            let action_key = format!("install_dev_tool_{}", spec.name.to_lowercase().replace('-', "_"));
            vec![RepairAction::manual(
                action_key,
                format!("Install {}", spec.name),
                format!("Install {} with your preferred package manager.", spec.name),
                hint.clone(),
                hint.split_whitespace().map(str::to_string).collect(),
                RepairTier::Manual,
            )]
        })
        .unwrap_or_default();

    DeveloperToolCheck {
        name: spec.name.to_string(),
        tier: spec.tier,
        installed,
        version,
        message: format!("{} {}", spec.description, spec.rationale),
        install_hint,
        repair_actions,
    }
}

fn parse_version(output: &str) -> Option<String> {
    output
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| {
            line.split_whitespace()
                .last()
                .filter(|token| token.chars().any(|ch| ch.is_ascii_digit()))
                .unwrap_or(line)
                .trim()
                .to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn developer_profile_tools_cover_all_tiers() {
        let report = install_report(&[]);
        assert!(report.checks.iter().any(|check| check.name == "jq"));
        assert!(report.checks.iter().any(|check| check.name == "cargo-deny"));
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.name == "cargo-outdated")
        );
    }

    #[test]
    fn unknown_requested_tools_are_reported() {
        assert_eq!(
            unknown_requested_tools(&["jq".to_string(), "unknown".to_string()]),
            vec!["unknown".to_string()]
        );
    }

    #[test]
    fn install_advice_mentions_advisory_boundary() {
        let report = DeveloperToolsReport {
            summary: "summary".to_string(),
            checks: Vec::new(),
            repair_actions: Vec::new(),
        };
        let lines = render_install_advice(&report);
        assert!(lines[0].contains("advisory only"));
        assert!(lines[1].contains("package manager"));
    }

    #[test]
    fn parse_version_handles_plain_and_prefixed_output() {
        assert_eq!(parse_version("jq-1.7.1"), Some("jq-1.7.1".to_string()));
        assert_eq!(
            parse_version("cargo-nextest 0.9.92"),
            Some("0.9.92".to_string())
        );
    }
}
