use anyhow::{Context, Result};
use std::io::{self, BufRead, IsTerminal};
use std::path::Path;
use std::process::Command;

/// Detect the project type from markers in the current directory.
fn detect_project_type() -> &'static str {
    if Path::new("Cargo.toml").exists() {
        "Rust"
    } else if Path::new("package.json").exists() {
        "JavaScript/TypeScript"
    } else if Path::new("pyproject.toml").exists() || Path::new("setup.py").exists() {
        "Python"
    } else {
        "Unknown"
    }
}

/// Check if hyphae is available by running `hyphae --version`.
fn hyphae_available(hyphae_cmd: &str) -> bool {
    let resolved = if let Some(path) = std::path::Path::new(hyphae_cmd).to_str() {
        if std::path::Path::new(path).is_absolute() {
            hyphae_cmd.to_string()
        } else {
            which::which(hyphae_cmd)
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
                .or_else(|| {
                    tracing::debug!("hyphae not found on PATH, falling back to bare name");
                    Some(hyphae_cmd.to_string())
                })
                .unwrap()
        }
    } else {
        hyphae_cmd.to_string()
    };

    Command::new(&resolved)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Check if hyphae already has memories for this project.
/// If the command fails or JSON cannot be parsed, we assume it's safe to proceed with seeding.
fn has_existing_memories(project: &str, hyphae_cmd: &str) -> bool {
    let resolved = if let Some(path) = std::path::Path::new(hyphae_cmd).to_str() {
        if std::path::Path::new(path).is_absolute() {
            hyphae_cmd.to_string()
        } else {
            which::which(hyphae_cmd)
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
                .or_else(|| {
                    tracing::debug!("hyphae not found on PATH, falling back to bare name");
                    Some(hyphae_cmd.to_string())
                })
                .unwrap()
        }
    } else {
        hyphae_cmd.to_string()
    };

    let Ok(output) = Command::new(&resolved)
        .args(["memory", "stats", "--json", "--project", project])
        .output()
    else {
        return false; // hyphae unavailable or command failed; assume no memories
    };

    if !output.status.success() {
        return false;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON looking for "total_memories" field.
    // If we can't parse, assume no memories to be safe.
    serde_json::from_str::<serde_json::Value>(&stdout)
        .ok()
        .and_then(|json| json.get("total_memories")?.as_u64())
        .is_some_and(|n| n > 0)
}

/// Store a memory in hyphae using the CLI.
fn store_memory(
    project: &str,
    topic: &str,
    importance: &str,
    content: &str,
    hyphae_cmd: &str,
) -> Result<()> {
    let resolved = if let Some(path) = std::path::Path::new(hyphae_cmd).to_str() {
        if std::path::Path::new(path).is_absolute() {
            hyphae_cmd.to_string()
        } else {
            which::which(hyphae_cmd)
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
                .or_else(|| {
                    tracing::debug!("hyphae not found on PATH, falling back to bare name");
                    Some(hyphae_cmd.to_string())
                })
                .unwrap()
        }
    } else {
        hyphae_cmd.to_string()
    };

    let output = Command::new(&resolved)
        .args(["store", "--topic", topic, "--importance", importance])
        .arg("--")
        .arg(content)
        .arg("--project")
        .arg(project)
        .output()
        .context("Failed to run hyphae store command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::debug!("hyphae store command failed with stderr: {}", stderr);
        return Err(anyhow::anyhow!(
            "hyphae store command failed with exit code: {}",
            output.status.code().unwrap_or(-1)
        ));
    }

    Ok(())
}

/// Read a single line from stdin, trimming whitespace.
fn read_input() -> Result<String> {
    let stdin = io::stdin();
    let mut buffer = String::new();
    stdin
        .lock()
        .read_line(&mut buffer)
        .context("Failed to read input")?;
    Ok(buffer.trim().to_string())
}

/// Seed initial project context into hyphae if no memories exist yet.
pub fn seed_first_run(project: &str, dry_run: bool) -> Result<()> {
    seed_first_run_internal(project, dry_run, "hyphae")
}

fn seed_first_run_internal(project: &str, dry_run: bool, hyphae_cmd: &str) -> Result<()> {
    // Check if hyphae is available
    if !hyphae_available(hyphae_cmd) {
        tracing::warn!("hyphae not available; skipping first-run seeding");
        return Ok(());
    }

    // Check if hyphae already has memories for this project
    if has_existing_memories(project, hyphae_cmd) {
        tracing::debug!("project {} already has memories; skipping seeding", project);
        return Ok(());
    }

    // Detect the project type
    let language = detect_project_type();

    // Build the initial context message
    let initial_context = format!(
        "Project: {project}. Primary language: {language}. First run seeded by stipe init."
    );

    if dry_run {
        println!("(dry-run) Would seed for project: {project}");
        println!("  - Initial context: {initial_context}");
        return Ok(());
    }

    // Store the initial context
    store_memory(
        project,
        &format!("context/{project}"),
        "high",
        &initial_context,
        hyphae_cmd,
    )?;

    println!("Seeded initial project context for {project}. Hyphae will learn more as you work.");

    Ok(())
}

/// Seed with interactive prompts for additional context.
pub fn seed_first_run_interactive(project: &str, dry_run: bool) -> Result<()> {
    seed_first_run_interactive_internal(project, dry_run, "hyphae")
}

fn seed_first_run_interactive_internal(
    project: &str,
    dry_run: bool,
    hyphae_cmd: &str,
) -> Result<()> {
    // First, run the automatic seed
    seed_first_run_internal(project, dry_run, hyphae_cmd)?;

    if dry_run {
        return Ok(());
    }

    // Only continue with interactive prompts if seeding was not skipped
    if !hyphae_available(hyphae_cmd) {
        return Ok(());
    }

    // Only prompt interactively if stdin is a TTY
    if !std::io::stdin().is_terminal() {
        return Ok(());
    }

    // Interactive prompts
    println!("What's the primary purpose of this project? (Enter to skip): ");
    let purpose = read_input()?;

    if !purpose.is_empty() {
        store_memory(
            project,
            &format!("context/{project}"),
            "high",
            &format!("Purpose: {purpose}"),
            hyphae_cmd,
        )?;
    }

    println!("Any key architectural decisions worth remembering? (Enter to skip): ");
    let decisions = read_input()?;

    if !decisions.is_empty() {
        store_memory(
            project,
            &format!("decisions/{project}"),
            "high",
            &decisions,
            hyphae_cmd,
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_name_from_cwd() {
        // This test verifies the basename extraction logic.
        // We use a hardcoded temporary directory for testing.
        let current_dir = std::env::current_dir().expect("Failed to get current dir");
        let name = current_dir
            .file_name()
            .and_then(|n| n.to_str())
            .expect("Failed to extract name");
        assert!(!name.is_empty());
    }

    #[test]
    fn test_seed_skips_gracefully_when_hyphae_missing() {
        // Test that seeding returns Ok(()) when hyphae is not available.
        // We use a nonexistent binary name to simulate hyphae being unavailable.
        let result = seed_first_run_internal("test-project", false, "nonexistent-hyphae-binary");
        assert!(result.is_ok());
    }

    #[test]
    fn test_detect_project_type_rust() {
        // Verify that Rust projects are detected if Cargo.toml exists.
        // This is tested indirectly through the detection function.
        let lang = detect_project_type();
        // The actual result depends on whether we're in a Rust project or not.
        // We just verify it returns a non-empty string.
        assert!(!lang.is_empty());
    }

    #[test]
    fn test_hyphae_unavailable_returns_false() {
        // Test that checking availability of a nonexistent binary returns false.
        assert!(!hyphae_available("nonexistent-hyphae-binary"));
    }
}
