
//! Lamella integration helpers: binary location and subprocess runner.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use wait_timeout::ChildExt;

const LAMELLA_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Locate the lamella executable.
///
/// Checks PATH via `which`. Returns `None` if lamella is not found.
pub fn find_lamella() -> Option<PathBuf> {
    which::which("lamella").ok()
}

/// Run `lamella <args>` as a subprocess, inheriting stdout/stderr.
///
/// Fails with a clear error if the subprocess does not exit within 5 minutes.
/// Returns `Ok(())` when lamella exits successfully, or an error describing
/// the non-zero exit status.  Callers that want best-effort behaviour (e.g.
/// a post-install step) should handle the error themselves.
pub fn run_lamella(args: &[&str]) -> Result<()> {
    let lamella = find_lamella().ok_or_else(|| {
        anyhow::anyhow!(
            "lamella not found on PATH. Install it or set LAMELLA_CONTENT_ROOT."
        )
    })?;

    let mut child = Command::new(&lamella)
        .args(args)
        .spawn()
        .with_context(|| format!("failed to launch lamella at {}", lamella.display()))?;

    if let Some(status) = child.wait_timeout(LAMELLA_TIMEOUT)? {
        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "lamella exited with {}",
                status.code().map_or_else(|| "signal".to_string(), |c| c.to_string())
            ))
        }
    } else {
        let _ = child.kill();
        let _ = child.wait();
        Err(anyhow::anyhow!(
            "lamella subprocess timed out after 5 minutes; \
             check for a hung MCP server or network stall"
        ))
    }
}
