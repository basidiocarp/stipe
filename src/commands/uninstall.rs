use anyhow::{Result, anyhow};
use colored::Colorize;
use spore::atomic_write_bytes;
use std::fs;
use std::path::{Path, PathBuf};

use super::bin_paths;
use super::host_policy;
use super::tool_registry;
use crate::verify;

#[derive(Debug, Clone, PartialEq, Eq)]
struct UninstallTarget {
    tool: String,
    path: PathBuf,
    exists: bool,
}

fn resolve_uninstall_tools(all: bool, tools: &[String]) -> Option<Vec<String>> {
    if all {
        let mut resolved = tool_registry::uninstall_all_specs()
            .into_iter()
            .map(|spec| spec.name.to_string())
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

fn render_preview_output(targets: &[UninstallTarget], all: bool) -> Vec<String> {
    let mut lines = vec![
        "Dry run: no changes will be made.".to_string(),
        String::new(),
    ];
    lines.extend(
        render_uninstall_preview(targets, all)
            .into_iter()
            .map(|line| format!("  {line}")),
    );
    lines.push(String::new());
    lines
}

fn print_preview(targets: &[UninstallTarget], all: bool) {
    for (index, line) in render_preview_output(targets, all).into_iter().enumerate() {
        if index == 0 {
            println!("{}", line.yellow());
        } else {
            println!("{line}");
        }
    }
}

/// Remove stipe-owned hook entries from a Claude Code settings.json so that
/// cortina / annulus commands are not invoked after the binaries are gone.
///
/// Entries are identified by well-known subcommand patterns; absolute-path
/// forms are handled by the `contains` checks on the command string.
fn remove_stipe_hooks_from_settings(settings_path: &Path) -> Result<()> {
    if !settings_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(settings_path)
        .map_err(|e| anyhow!("reading {}: {e}", settings_path.display()))?;

    if content.trim().is_empty() {
        return Ok(());
    }

    let mut root: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| anyhow!("parsing {}: {e}", settings_path.display()))?;

    let stipe_subcommands = [
        "adapter claude-code pre-tool-use",
        "adapter claude-code post-tool-use",
        "adapter claude-code stop",
        "adapter claude-code pre-compact",
        "adapter claude-code user-prompt-submit",
    ];

    let stipe_statusline_markers = ["cortina statusline", "annulus statusline"];

    let mut changed = false;

    // Remove hook entries whose command contains a stipe-owned subcommand.
    if let Some(hooks) = root
        .get_mut("hooks")
        .and_then(serde_json::Value::as_object_mut)
    {
        for event_entries in hooks.values_mut() {
            let Some(entries) = event_entries.as_array_mut() else {
                continue;
            };

            for entry in entries.iter_mut() {
                let Some(inner_hooks) = entry
                    .get_mut("hooks")
                    .and_then(serde_json::Value::as_array_mut)
                else {
                    continue;
                };

                let before = inner_hooks.len();
                inner_hooks.retain(|hook| {
                    let cmd = hook
                        .get("command")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("");
                    !stipe_subcommands
                        .iter()
                        .any(|pattern| cmd.contains(pattern))
                });
                if inner_hooks.len() != before {
                    changed = true;
                }
            }

            // Drop event entries whose inner hook list is now empty.
            let before = entries.len();
            entries.retain(|entry| {
                entry
                    .get("hooks")
                    .and_then(serde_json::Value::as_array)
                    .is_none_or(|h| !h.is_empty())
            });
            if entries.len() != before {
                changed = true;
            }
        }
    }

    // Remove the statusLine field if it references cortina or annulus.
    if let Some(status_line) = root.get("statusLine") {
        let cmd = status_line
            .get("command")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if stipe_statusline_markers
            .iter()
            .any(|marker| cmd.contains(marker))
        {
            root.as_object_mut()
                .expect("root must be an object")
                .remove("statusLine");
            changed = true;
        }
    }

    if changed {
        let serialized =
            serde_json::to_string_pretty(&root).map_err(|e| anyhow!("serializing: {e}"))?;
        atomic_write_bytes(settings_path, serialized.as_bytes())
            .map_err(|e| anyhow!("writing {}: {e}", settings_path.display()))?;
    }

    Ok(())
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

        // Remove ownership state file on uninstall (best-effort).
        if let Err(error) = verify::remove_ownership_state(&target.tool) {
            eprintln!(
                "  {} Could not remove ownership state for {}: {error}",
                "!".yellow(),
                target.tool
            );
        }
    }

    println!();

    // Remove cortina/annulus hook registrations from Claude Code settings so
    // subsequent tool calls do not attempt to run the now-missing binaries.
    for settings_path in host_policy::claude_hook_settings_paths() {
        if let Err(error) = remove_stipe_hooks_from_settings(&settings_path) {
            eprintln!(
                "  {} Could not remove hook entries from {}: {error}",
                "!".yellow(),
                settings_path.display()
            );
        } else if settings_path.exists() {
            println!(
                "  {} Removed cortina/annulus hooks from {}",
                "✓".green(),
                settings_path.display()
            );
        }
    }

    if all {
        println!();
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
                "volva".to_string(),
                "annulus".to_string(),
                "hymenium".to_string(),
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

    #[test]
    fn test_render_preview_output_snapshot() {
        let targets = vec![
            UninstallTarget {
                tool: "mycelium".to_string(),
                path: PathBuf::from("/tmp/mycelium"),
                exists: true,
            },
            UninstallTarget {
                tool: "hyphae".to_string(),
                path: PathBuf::from("/tmp/hyphae"),
                exists: false,
            },
        ];

        assert_eq!(
            render_preview_output(&targets, true),
            vec![
                "Dry run: no changes will be made.".to_string(),
                String::new(),
                format!(
                    "  Would remove all ecosystem binaries from {}.",
                    bin_paths::local_bin_dir_display()
                ),
                "  mycelium: would be removed from /tmp/mycelium".to_string(),
                "  hyphae: not present at /tmp/hyphae".to_string(),
                "  MCP registrations in editor config files would remain for manual cleanup."
                    .to_string(),
                String::new(),
            ]
        );
    }
}
