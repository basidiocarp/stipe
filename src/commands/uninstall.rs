use anyhow::{Result, anyhow};
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};

use super::bin_paths;

const ALL_TOOLS: &[&str] = &[
    "mycelium", "hyphae", "rhizome", "canopy", "cortina", "stipe",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct UninstallTarget {
    tool: String,
    path: PathBuf,
    exists: bool,
}

fn resolve_uninstall_tools(all: bool, tools: &[String]) -> Option<Vec<String>> {
    if all {
        let mut resolved = ALL_TOOLS
            .iter()
            .map(|tool| (*tool).to_string())
            .collect::<Vec<_>>();
        for tool in tools {
            if !resolved.iter().any(|existing| existing == tool) {
                resolved.push(tool.clone());
            }
        }
        return Some(resolved);
    }

    if tools.is_empty() {
        None
    } else {
        Some(tools.to_vec())
    }
}

fn build_uninstall_targets(bin_dir: &Path, tools: &[String]) -> Vec<UninstallTarget> {
    tools
        .iter()
        .map(|tool| {
            let path = bin_dir.join(tool);
            UninstallTarget {
                tool: tool.clone(),
                exists: path.exists(),
                path,
            }
        })
        .collect()
}

fn render_uninstall_preview(targets: &[UninstallTarget], all: bool) -> Vec<String> {
    let mut lines = Vec::new();

    if all {
        lines.push(format!(
            "Would remove all ecosystem binaries from {}.",
            bin_paths::local_bin_dir_display()
        ));
    }

    for target in targets {
        if target.exists {
            lines.push(format!(
                "{}: would be removed from {}",
                target.tool,
                target.path.display()
            ));
        } else {
            lines.push(format!(
                "{}: not present at {}",
                target.tool,
                target.path.display()
            ));
        }
    }

    lines.push(
        "MCP registrations in editor config files would remain for manual cleanup.".to_string(),
    );
    lines
}

fn print_preview(targets: &[UninstallTarget], all: bool) {
    println!("{}", "Dry run: no changes will be made.".yellow());
    println!();

    for line in render_uninstall_preview(targets, all) {
        println!("  {line}");
    }

    println!();
}

pub fn run(all: bool, dry_run: bool, tools: &[String]) -> Result<()> {
    let bin_dir = bin_paths::local_bin_dir()
        .ok_or_else(|| anyhow!("Could not determine local bin directory"))?;

    let Some(requested_tools) = resolve_uninstall_tools(all, tools) else {
        println!("Specify tools to remove or use --all");
        return Ok(());
    };

    let targets = build_uninstall_targets(&bin_dir, &requested_tools);

    if dry_run {
        print_preview(&targets, all);
        return Ok(());
    }

    if all {
        println!("Removing all ecosystem tools and configuration...");
        println!();
    } else {
        println!();
    }

    for target in targets {
        if target.exists {
            fs::remove_file(&target.path)?;
            println!("  {} {} removed", "✓".green(), target.tool);
        } else {
            println!(
                "  {} {} not found in {}",
                "!".yellow(),
                target.tool,
                bin_dir.display()
            );
        }
    }

    println!();
    if all {
        println!(
            "{}",
            "Note: MCP registrations in editor config files must be removed manually.".yellow()
        );
        println!();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_uninstall_tools_all_mode_includes_stipe() {
        let resolved = resolve_uninstall_tools(true, &[]).unwrap();
        assert_eq!(
            resolved,
            vec![
                "mycelium".to_string(),
                "hyphae".to_string(),
                "rhizome".to_string(),
                "canopy".to_string(),
                "cortina".to_string(),
                "stipe".to_string(),
            ]
        );
    }

    #[test]
    fn test_build_uninstall_targets_marks_existing_files() {
        let temp_dir = std::env::temp_dir().join("stipe-uninstall-preview");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        fs::write(temp_dir.join("mycelium"), "").unwrap();

        let targets =
            build_uninstall_targets(&temp_dir, &["mycelium".to_string(), "hyphae".to_string()]);

        assert!(
            targets
                .iter()
                .any(|target| target.tool == "mycelium" && target.exists)
        );
        assert!(
            targets
                .iter()
                .any(|target| target.tool == "hyphae" && !target.exists)
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_render_uninstall_preview_mentions_manual_cleanup() {
        let targets = vec![UninstallTarget {
            tool: "mycelium".to_string(),
            path: PathBuf::from("/tmp/mycelium"),
            exists: true,
        }];

        let lines = render_uninstall_preview(&targets, true);
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Would remove all ecosystem binaries"))
        );
        assert!(lines.iter().any(|line| line.contains("manual cleanup")));
    }
}
