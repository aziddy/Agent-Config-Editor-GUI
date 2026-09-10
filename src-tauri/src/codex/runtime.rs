//! Runtime overlay from the Codex binary: `codex mcp list --json`.
//!
//! This is a status overlay only, never the source of truth: it includes plugin-provided servers
//! that are absent from `config.toml`. It is slow (spawns a process), so [`super::scan`] never
//! calls it; a Tauri command does, with a timeout and a short cache.

use crate::error::{AppError, AppResult, ErrorCode};
use crate::model::CodexMcpRuntime;
use serde_json::Value;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Run `codex mcp list --json` with `CODEX_HOME` pointed at `codex_home`, killing the process if
/// it exceeds `timeout`. Returns `(server name, runtime state)` pairs in the CLI's order.
pub fn fetch_mcp_status(
    codex_binary: &Path,
    codex_home: &Path,
    timeout: Duration,
) -> AppResult<Vec<(String, CodexMcpRuntime)>> {
    let mut child = Command::new(codex_binary)
        .args(["mcp", "list", "--json"])
        .env("CODEX_HOME", codex_home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::io(e, codex_binary))?;

    // Drain both pipes on helper threads so a chatty child can never block on a full pipe.
    let stdout_thread = spawn_reader(child.stdout.take());
    let stderr_thread = spawn_reader(child.stderr.take());

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(AppError::new(
                        ErrorCode::Io,
                        format!(
                            "`codex mcp list --json` timed out after {} ms",
                            timeout.as_millis()
                        ),
                    )
                    .with_path(codex_binary));
                }
                thread::sleep(Duration::from_millis(25));
            }
            Err(e) => return Err(AppError::io(e, codex_binary)),
        }
    };

    let stdout = stdout_thread.join().unwrap_or_default();
    let stderr = stderr_thread.join().unwrap_or_default();
    if !status.success() {
        return Err(AppError::new(
            ErrorCode::Io,
            format!(
                "`codex mcp list --json` exited with {status}: {}",
                stderr.trim()
            ),
        )
        .with_path(codex_binary));
    }
    parse_mcp_list(&stdout)
}

fn spawn_reader<R: Read + Send + 'static>(pipe: Option<R>) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut buf = String::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_string(&mut buf);
        }
        buf
    })
}

/// Parse the JSON array printed by `codex mcp list --json`:
/// `[{ name, enabled, disabled_reason, transport: { type, … }, auth_status, … }]`.
pub fn parse_mcp_list(json: &str) -> AppResult<Vec<(String, CodexMcpRuntime)>> {
    let value: Value = serde_json::from_str(json.trim())
        .map_err(|e| AppError::new(ErrorCode::Parse, format!("codex mcp list output: {e}")))?;
    let Some(items) = value.as_array() else {
        return Err(AppError::new(
            ErrorCode::Parse,
            "codex mcp list output is not a JSON array",
        ));
    };
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        let Some(name) = item.get("name").and_then(Value::as_str) else {
            continue;
        };
        let str_field = |key: &str| item.get(key).and_then(Value::as_str).map(String::from);
        result.push((
            name.to_string(),
            CodexMcpRuntime {
                enabled: item.get("enabled").and_then(Value::as_bool).unwrap_or(true),
                disabled_reason: str_field("disabled_reason"),
                auth_status: str_field("auth_status"),
                transport_type: item
                    .get("transport")
                    .and_then(|t| t.get("type"))
                    .and_then(Value::as_str)
                    .map(String::from),
            },
        ));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parses_codex_mcp_list_output() {
        let sample = r#"[
  {
    "name": "chrome-devtools",
    "enabled": true,
    "disabled_reason": null,
    "transport": { "type": "stdio", "command": "npx", "args": ["chrome-devtools-mcp@latest"], "env": null, "env_vars": [], "cwd": null },
    "startup_timeout_sec": null,
    "tool_timeout_sec": null,
    "auth_status": "unsupported"
  },
  {
    "name": "computer-use",
    "enabled": false,
    "disabled_reason": "disabled in config",
    "transport": { "type": "stdio", "command": "./x", "args": ["mcp"], "env": null, "env_vars": [], "cwd": "." },
    "startup_timeout_sec": 120.0,
    "tool_timeout_sec": null,
    "auth_status": "unsupported"
  },
  {
    "name": "linear",
    "enabled": true,
    "disabled_reason": null,
    "transport": { "type": "streamable_http", "url": "https://mcp.linear.app/mcp" },
    "auth_status": "not_logged_in"
  }
]"#;
        let parsed = parse_mcp_list(sample).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].0, "chrome-devtools");
        assert_eq!(
            parsed[0].1,
            CodexMcpRuntime {
                enabled: true,
                disabled_reason: None,
                auth_status: Some("unsupported".into()),
                transport_type: Some("stdio".into()),
            }
        );
        assert!(!parsed[1].1.enabled);
        assert_eq!(
            parsed[1].1.disabled_reason.as_deref(),
            Some("disabled in config")
        );
        assert_eq!(
            parsed[2].1.transport_type.as_deref(),
            Some("streamable_http")
        );
        assert_eq!(parsed[2].1.auth_status.as_deref(), Some("not_logged_in"));
    }

    #[test]
    fn rejects_non_array() {
        assert!(parse_mcp_list("{}").is_err());
        assert!(parse_mcp_list("not json").is_err());
    }
}
