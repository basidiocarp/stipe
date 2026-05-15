//! Unix-socket endpoint for direct JSON-RPC 2.0 tool calls.
//!
//! Cap and other local clients use this endpoint to query `stipe_doctor` and
//! `stipe_init_plan` data without spawning a subprocess. Bind path is
//! `~/.local/share/basidiocarp/stipe/stipe.sock`. The endpoint descriptor at
//! `~/.config/stipe/stipe.endpoint.json` lets clients discover the socket
//! path via the `local-service-endpoint-v1` convention.
//!
//! # Supported methods
//!
//! - `PING` / `ping` — health probe, returns `{}`
//! - `stipe_doctor` — ecosystem health report; params: `developer?: bool, deep?: bool`
//! - `stipe_init_plan` — dry-run init plan; params: `client?: string`

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{Value, json};
use tracing::{debug, error};

const CAPABILITY_ID: &str = "ecosystem.setup.v1";
const PING_METHOD: &str = "PING";

fn write_endpoint_descriptor(socket_path: &Path) -> Result<()> {
    let config_dir = spore::paths::config_dir("stipe");
    std::fs::create_dir_all(&config_dir)?;
    let descriptor_path = config_dir.join("stipe.endpoint.json");
    let descriptor = json!({
        "schema_version": "1.0",
        "transport": "unix-socket",
        "endpoint": socket_path.to_string_lossy(),
        "capability_id": CAPABILITY_ID,
        "version": env!("CARGO_PKG_VERSION"),
        "health_probe": { "method": PING_METHOD, "timeout_ms": 1000 }
    });
    std::fs::write(&descriptor_path, serde_json::to_string_pretty(&descriptor)?)?;
    Ok(())
}

fn remove_stale_socket(socket_path: &Path) {
    if let Err(e) = std::fs::remove_file(socket_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(path = %socket_path.display(), error = %e, "could not remove stale socket");
        }
    }
}

// ---------------------------------------------------------------------------
// JSON-RPC helpers
// ---------------------------------------------------------------------------

#[allow(clippy::needless_pass_by_value)]
fn ok_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

#[allow(clippy::needless_pass_by_value)]
fn err_response(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message.into() }
    })
}

fn write_response(writer: &mut (impl Write + ?Sized), response: &Value) {
    if let Ok(bytes) = serde_json::to_vec(response) {
        if let Err(e) = writer
            .write_all(&bytes)
            .and_then(|()| writer.write_all(b"\n"))
            .and_then(|()| writer.flush())
        {
            tracing::warn!(error = %e, "failed to write JSON-RPC response to client");
        }
    }
}

// ---------------------------------------------------------------------------
// Method handlers
// ---------------------------------------------------------------------------

fn handle_doctor(params: &Value) -> Value {
    let developer = params
        .get("developer")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let deep = params.get("deep").and_then(Value::as_bool).unwrap_or(false);

    match crate::commands::doctor::run_json_string(developer, deep) {
        Ok(json_str) => serde_json::from_str(&json_str).unwrap_or_else(|_| json!({})),
        Err(e) => json!({ "error": format!("doctor query: {e}") }),
    }
}

fn handle_init_plan(params: &Value) -> Value {
    let client = params
        .get("client")
        .and_then(|v| v.as_str())
        .map(str::to_owned);

    match crate::commands::init::plan_json_string(client.as_deref()) {
        Ok(json_str) => serde_json::from_str(&json_str).unwrap_or_else(|_| json!({})),
        Err(e) => json!({ "error": format!("init plan query: {e}") }),
    }
}

// ---------------------------------------------------------------------------
// Connection handler
// ---------------------------------------------------------------------------

fn handle_connection(stream: std::os::unix::net::UnixStream) {
    let writer_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            error!("failed to clone unix stream: {e}");
            return;
        }
    };
    let mut reader = BufReader::new(stream);
    let mut writer = writer_stream;

    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => return,
            Ok(_) => {}
            Err(e) => {
                error!("socket read error: {e}");
                return;
            }
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let msg: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                let resp = err_response(Value::Null, -32700, format!("parse error: {e}"));
                write_response(&mut writer, &resp);
                return;
            }
        };

        let id = match msg.get("id").cloned() {
            Some(id) if !id.is_null() => id,
            _ => continue, // notification — no response
        };

        let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");
        debug!("socket request: {method}");

        let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));

        let response = match method {
            m if m == PING_METHOD || m == "ping" => ok_response(id, json!({})),
            "stipe_doctor" => {
                let result = handle_doctor(&params);
                if result.get("error").is_some() {
                    let msg = result["error"]
                        .as_str()
                        .unwrap_or("doctor error")
                        .to_string();
                    err_response(id, -32000, msg)
                } else {
                    ok_response(id, result)
                }
            }
            "stipe_init_plan" => {
                let result = handle_init_plan(&params);
                if result.get("error").is_some() {
                    let msg = result["error"]
                        .as_str()
                        .unwrap_or("init plan error")
                        .to_string();
                    err_response(id, -32000, msg)
                } else {
                    ok_response(id, result)
                }
            }
            _ => err_response(id, -32601, format!("method not found: {method}")),
        };

        write_response(&mut writer, &response);
    }
}

// ---------------------------------------------------------------------------
// Server entry point
// ---------------------------------------------------------------------------

/// Start the stipe unix-socket service endpoint.
///
/// Binds to `~/.local/share/basidiocarp/stipe/stipe.sock`, writes the
/// endpoint descriptor to `~/.config/stipe/stipe.endpoint.json`, then
/// accepts connections indefinitely. Each connection is handled in a
/// background thread.
pub fn run_socket_server() -> Result<()> {
    let socket_path: PathBuf = spore::paths::data_dir("basidiocarp")
        .join("stipe")
        .join("stipe.sock");

    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    remove_stale_socket(&socket_path);

    let listener = std::os::unix::net::UnixListener::bind(&socket_path).map_err(|e| {
        anyhow::anyhow!("failed to bind stipe socket {}: {e}", socket_path.display())
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;
    }

    write_endpoint_descriptor(&socket_path)?;

    tracing::info!(
        socket = %socket_path.display(),
        "stipe socket endpoint ready"
    );

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                std::thread::spawn(move || handle_connection(stream));
            }
            Err(e) => error!("stipe socket accept error: {e}"),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use tempfile::TempDir;

    fn temp_socket_path(dir: &TempDir) -> PathBuf {
        dir.path().join("test.sock")
    }

    #[test]
    fn socket_server_ping_responds_ok() {
        let tmp = TempDir::new().unwrap();
        let socket_path = temp_socket_path(&tmp);

        remove_stale_socket(&socket_path);
        let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        let socket_path_clone = socket_path.clone();

        let handle = std::thread::spawn(move || {
            if let Ok(stream) = listener.accept().map(|(s, _)| s) {
                handle_connection(stream);
            }
        });

        let mut client = std::os::unix::net::UnixStream::connect(&socket_path_clone).unwrap();
        let request = r#"{"jsonrpc":"2.0","id":1,"method":"PING","params":null}"#;
        client.write_all(request.as_bytes()).unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();

        let reader = BufReader::new(&client);
        let line = reader.lines().next().expect("response").unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["id"], 1);
        assert!(v.get("result").is_some());
        assert!(v.get("error").is_none());

        handle.join().unwrap();
    }

    #[test]
    fn socket_server_unknown_method_returns_method_not_found() {
        let tmp = TempDir::new().unwrap();
        let socket_path = temp_socket_path(&tmp);

        remove_stale_socket(&socket_path);
        let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        let socket_path_clone = socket_path.clone();

        let handle = std::thread::spawn(move || {
            if let Ok(stream) = listener.accept().map(|(s, _)| s) {
                handle_connection(stream);
            }
        });

        let mut client = std::os::unix::net::UnixStream::connect(&socket_path_clone).unwrap();
        let request = r#"{"jsonrpc":"2.0","id":2,"method":"no_such_method","params":{}}"#;
        client.write_all(request.as_bytes()).unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();

        let reader = BufReader::new(&client);
        let line = reader.lines().next().expect("response").unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["id"], 2);
        assert!(v.get("error").is_some());
        assert_eq!(v["error"]["code"], -32601);

        handle.join().unwrap();
    }
}
