use anyhow::{Context, Result, anyhow};
use indicatif::ProgressBar;
use spore::logging::{SpanContext, subprocess_span, tool_span};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use tar::EntryType;

use crate::commands::github::GitHubClient;
use crate::commands::tool_registry;
use crate::commands::tool_registry::ToolSpec;

#[derive(Debug)]
pub(crate) struct GitHubRelease {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) assets: Vec<ReleaseAsset>,
}

#[derive(Debug)]
pub(crate) struct ReleaseAsset {
    pub(crate) name: String,
    pub(crate) download_url: String,
}

const VERSION_VERIFY_TIMEOUT: Duration = Duration::from_secs(5);
const FUNCTIONAL_VERIFY_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const MCP_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const MCP_INITIALIZE_REQUEST: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"stipe-verify","version":"0.1"}}}"#;

pub(crate) fn platform_key() -> &'static str {
    if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_arch = "x86_64", target_os = "macos")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(target_arch = "aarch64", target_os = "linux")) {
        "aarch64-unknown-linux-musl"
    } else if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        "x86_64-unknown-linux-musl"
    } else if cfg!(all(target_arch = "x86_64", target_os = "windows")) {
        "x86_64-pc-windows-msvc"
    } else {
        "unknown"
    }
}

fn release_repo(tool: &str) -> &str {
    tool_registry::find(tool).map_or(tool, |spec| spec.release_repo)
}

pub(crate) fn fetch_latest_release(tool: &str, client: &GitHubClient) -> Result<GitHubRelease> {
    let repo = release_repo(tool);
    let url = format!("https://api.github.com/repos/basidiocarp/{repo}/releases/latest");
    let data = crate::commands::github::get_github_json(
        client,
        &url,
        &format!("latest release for {repo}"),
    )?;

    let version = data
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("GitHub release missing 'tag_name' field"))?
        .to_string();

    let assets: Vec<ReleaseAsset> = data
        .get("assets")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|asset| {
                    let name = asset.get("name")?.as_str()?;
                    let download_url = asset.get("browser_download_url")?.as_str()?;
                    Some(ReleaseAsset {
                        name: name.to_string(),
                        download_url: download_url.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(GitHubRelease {
        name: tool.to_string(),
        version,
        assets,
    })
}

pub(crate) fn find_matching_asset<'a>(
    release: &'a GitHubRelease,
    platform_key: &str,
) -> Result<&'a ReleaseAsset> {
    release
        .assets
        .iter()
        .find(|asset| asset.name.contains(platform_key) && asset.name.ends_with(".tar.gz"))
        .ok_or_else(|| {
            anyhow!(
                "No tar.gz asset found for {} on platform {}",
                release.name,
                platform_key
            )
        })
}

pub(crate) fn download_binary(
    asset: &ReleaseAsset,
    progress: &ProgressBar,
    client: &GitHubClient,
) -> Result<Vec<u8>> {
    let mut response = client
        .get(&asset.download_url)
        .with_context(|| format!("Failed to download {}", asset.name))?;

    if !response.status().is_success() {
        let body = response
            .body_mut()
            .read_to_string()
            .ok()
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| "<empty response>".to_string());
        return Err(anyhow!(
            "Download failed for {}: {} ({body})",
            asset.name,
            response.status()
        ));
    }

    let total_size = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    progress.set_length(total_size);

    let mut bytes = Vec::new();
    response
        .body_mut()
        .with_config()
        .limit(u64::MAX)
        .reader()
        .read_to_end(&mut bytes)
        .context("Failed to read response body")?;

    progress.finish();
    Ok(bytes)
}

pub(crate) fn extract_tarball(data: &[u8], dest_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(dest_dir)
        .with_context(|| format!("Failed to create directory: {}", dest_dir.display()))?;

    let tar_gz = std::io::Cursor::new(data);
    let tar = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(tar);

    let mut binary_path = None;

    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        let path = entry.path()?.to_path_buf();

        // Reject non-regular entries before unpacking to prevent path traversal
        // via symlinks, hardlinks, or device files in a crafted archive.
        let entry_type = entry.header().entry_type();
        if !matches!(entry_type, EntryType::Regular | EntryType::Continuous) {
            if !matches!(entry_type, EntryType::Directory) {
                tracing::warn!(
                    "skipping non-regular tar entry: {} ({:?})",
                    path.display(),
                    entry_type
                );
            }
            continue;
        }

        if let Some(file_name) = path.file_name()
            && let Some(name_str) = file_name.to_str()
            && tool_registry::release_archive_binaries().contains(&name_str)
        {
            entry.unpack_in(dest_dir)?;

            // Verify that the extracted path resolves within dest_dir to guard
            // against any remaining traversal edge cases after unpack.
            let extracted = dest_dir.join(file_name);
            let canonical = fs::canonicalize(&extracted)
                .with_context(|| format!("Failed to canonicalize extracted path: {}", extracted.display()))?;
            let canonical_dest = fs::canonicalize(dest_dir)
                .with_context(|| format!("Failed to canonicalize dest dir: {}", dest_dir.display()))?;
            if !canonical.starts_with(&canonical_dest) {
                return Err(anyhow!(
                    "Extracted path {} escapes destination directory {}",
                    canonical.display(),
                    canonical_dest.display()
                ));
            }

            binary_path = Some(extracted);
        }
    }

    binary_path.ok_or_else(|| anyhow!("No binary found in archive"))
}

pub(crate) fn verify_binary(path: &Path) -> Result<String> {
    verify_binary_with_timeout(path, VERSION_VERIFY_TIMEOUT)
}

pub(crate) fn verify_binary_with_timeout(path: &Path, timeout: Duration) -> Result<String> {
    let tool_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("binary");
    let span_context = span_context_for_path(path, tool_name);
    let _tool_span = tool_span("verify_binary", &span_context).entered();
    let output = run_command_with_timeout(Command::new(path).arg("--version"), timeout)
        .with_context(|| format!("Failed to run {}", path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "Binary verification failed for {}: {}",
            path.display(),
            stderr.trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub(crate) fn verify_functional(
    binary_path: &Path,
    spec: &ToolSpec,
) -> std::result::Result<(), String> {
    let span_context = span_context_for_path(binary_path, spec.binary_name);
    let _tool_span = tool_span("verify_functional", &span_context).entered();
    let Some(args) = spec.smoke_test_args else {
        return Ok(());
    };

    let output = run_command_with_timeout(
        Command::new(binary_path).args(args),
        FUNCTIONAL_VERIFY_TIMEOUT,
    )
    .map_err(|err| format!("smoke test failed to execute: {err}"))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "smoke test failed: {} {} exited with {} (stdout: {}, stderr: {})",
            binary_path.display(),
            args.join(" "),
            output.status,
            stdout.trim(),
            stderr.trim()
        ));
    }

    if let Some(expected) = spec.smoke_test_expect {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stdout.contains(expected) && !stderr.contains(expected) {
            return Err(format!(
                "smoke test output missing '{expected}': stdout={}, stderr={}",
                stdout.trim(),
                stderr.trim()
            ));
        }
    }

    Ok(())
}

pub(crate) fn verify_mcp_handshake(
    binary_path: &Path,
    spec: &ToolSpec,
) -> std::result::Result<(), String> {
    let span_context = span_context_for_path(binary_path, spec.binary_name);
    let _tool_span = tool_span("verify_mcp_handshake", &span_context).entered();
    let Some(args) = spec.mcp_serve_args else {
        return Ok(());
    };

    probe_mcp_server(binary_path, args, spec.binary_name, MCP_HANDSHAKE_TIMEOUT)
}

pub(crate) fn parse_initialize_response(
    line: &str,
    expected_server: &str,
) -> std::result::Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_str(line).map_err(|err| format!("invalid JSON-RPC response: {err}"))?;

    if let Some(error) = value.get("error") {
        return Err(format!("initialize returned error: {error}"));
    }

    let result = value
        .get("result")
        .ok_or_else(|| "initialize returned no result".to_string())?;
    let protocol_version = result
        .get("protocolVersion")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "initialize response missing protocolVersion".to_string())?;
    if protocol_version.is_empty() {
        return Err("initialize response included empty protocolVersion".to_string());
    }

    let server_name = result
        .get("serverInfo")
        .and_then(|server| server.get("name"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "initialize response missing serverInfo.name".to_string())?;

    if server_name != expected_server {
        return Err(format!(
            "initialize returned server `{server_name}` instead of `{expected_server}`"
        ));
    }

    Ok(())
}

pub(crate) fn probe_mcp_server(
    command: &Path,
    args: &[&str],
    expected_server: &str,
    timeout: Duration,
) -> std::result::Result<(), String> {
    let span_context = span_context_for_path(command, expected_server);
    let _subprocess_span = subprocess_span(expected_server, &span_context).entered();
    let mut child = Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to spawn `{}`: {err}", command.display()))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(MCP_INITIALIZE_REQUEST.as_bytes())
            .map_err(|err| format!("failed to write initialize request: {err}"))?;
        stdin
            .write_all(b"\n")
            .map_err(|err| format!("failed to terminate initialize request: {err}"))?;
        stdin
            .flush()
            .map_err(|err| format!("failed to flush initialize request: {err}"))?;
    } else {
        return Err("child stdin unavailable".to_string());
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "child stdout unavailable".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "child stderr unavailable".to_string())?;

    let (stdout_tx, stdout_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let result = match reader.read_line(&mut line) {
            Ok(0) => Err("connection closed before initialize response".to_string()),
            Ok(_) => Ok(line),
            Err(err) => Err(format!("failed reading initialize response: {err}")),
        };
        let _ = stdout_tx.send(result);
    });

    let (stderr_tx, stderr_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut output = String::new();
        let _ = Read::read_to_string(&mut reader, &mut output);
        let _ = stderr_tx.send(output);
    });

    let response = match stdout_rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("initialize timed out after {}s", timeout.as_secs()));
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("initialize response channel disconnected".to_string());
        }
    };

    let _ = child.kill();
    let _ = child.wait();

    response.and_then(|line| {
        parse_initialize_response(&line, expected_server).map_err(|err| {
            let stderr_output = stderr_rx
                .recv_timeout(Duration::from_millis(200))
                .unwrap_or_default();
            if stderr_output.trim().is_empty() {
                err
            } else {
                format!("{err}; stderr: {}", stderr_output.trim())
            }
        })
    })
}

fn run_command_with_timeout(command: &mut Command, timeout: Duration) -> std::io::Result<Output> {
    let tool_name = command.get_program().to_string_lossy().to_string();
    let span_context = span_context_for_command(command, &tool_name);
    let command_name = command.get_program().to_string_lossy().to_string();
    let _subprocess_span = subprocess_span(&command_name, &span_context).entered();
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("child stdout unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("child stderr unavailable"))?;

    let stdout_handle = thread::spawn(move || {
        let mut reader = stdout;
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        Ok::<Vec<u8>, std::io::Error>(buf)
    });
    let stderr_handle = thread::spawn(move || {
        let mut reader = stderr;
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        Ok::<Vec<u8>, std::io::Error>(buf)
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("command timed out after {}s", timeout.as_secs()),
            ));
        }
        thread::sleep(Duration::from_millis(10));
    };

    let stdout = stdout_handle
        .join()
        .map_err(|_| std::io::Error::other("stdout reader panicked"))??;
    let stderr = stderr_handle
        .join()
        .map_err(|_| std::io::Error::other("stderr reader panicked"))??;

    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn span_context_for_path(path: &Path, tool: &str) -> SpanContext {
    let context = SpanContext::for_app("stipe").with_tool(tool);
    match path.parent() {
        Some(parent) => context.with_workspace_root(parent.display().to_string()),
        None => context,
    }
}

fn span_context_for_command(command: &Command, tool: &str) -> SpanContext {
    let context = SpanContext::for_app("stipe").with_tool(tool);
    match command.get_current_dir() {
        Some(current_dir) => context.with_workspace_root(current_dir.display().to_string()),
        None => match std::env::current_dir() {
            Ok(path) => context.with_workspace_root(path.display().to_string()),
            Err(_) => context,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tool_registry;
    use std::fs;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn parse_initialize_response_requires_protocol_version() {
        let line = r#"{"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"hyphae"}}}"#;
        let error = parse_initialize_response(line, "hyphae").unwrap_err();
        assert!(error.contains("protocolVersion"));
    }

    #[cfg(unix)]
    #[test]
    fn verify_functional_checks_expected_output() {
        let dir =
            std::env::temp_dir().join(format!("stipe-release-functional-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let script = dir.join("mycelium");
        fs::write(&script, "#!/bin/sh\nprintf 'stipe-verify\\n'\n").unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();

        let spec = tool_registry::find("mycelium").expect("mycelium spec should exist");
        assert!(verify_functional(&script, spec).is_ok());

        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn verify_mcp_handshake_accepts_initialize_round_trip() {
        let dir = std::env::temp_dir().join(format!("stipe-release-mcp-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let script = dir.join("hyphae");
        fs::write(
            &script,
            "#!/bin/sh\nIFS= read -r line || exit 1\ncase \"$line\" in\n  *'\"method\":\"initialize\"'*)\n    printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":\"2024-11-05\",\"serverInfo\":{\"name\":\"hyphae\"}}}'\n    ;;\n  *)\n    printf '%s\\n' 'unexpected initialize payload' >&2\n    exit 1\n    ;;\nesac\n",
        )
        .unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();

        let spec = tool_registry::find("hyphae").expect("hyphae spec should exist");
        assert!(verify_mcp_handshake(&script, spec).is_ok());

        let _ = fs::remove_dir_all(&dir);
    }
}
