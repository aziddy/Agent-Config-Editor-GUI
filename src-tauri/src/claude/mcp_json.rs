//! `.mcp.json`-shaped documents.
//!
//! Two shapes exist on disk:
//!
//! - project files (`<repo>/.mcp.json`) wrap the servers: `{ "mcpServers": { "<name>": {…} } }`;
//! - plugin-root files (`<plugin cache dir>/.mcp.json`) put the servers at the top level.
//!
//! [`server_map`] probes for the wrapper: an object-valued `mcpServers` key selects the wrapped
//! shape, anything else treats the whole document as the server map.
//!
//! A server value is `{ type?: "stdio"|"http"|"sse", command, args, env, cwd, url, headers, … }`.
//! Only those keys are modeled; every other key (e.g. Codex-style `bearer_token_env_var`,
//! `tool_timeout_sec`, `default_tools_approval_mode`) is preserved in [`ParsedServer::extra`]
//! so a writer can round-trip it. `env` and `headers` are kept unmasked: masking is a UI concern.

use crate::model::{Locator, McpTransport};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedServer {
    pub transport: McpTransport,
    /// Keys not modeled by [`McpTransport`], in file order.
    pub extra: Map<String, Value>,
    /// Mirrors `extra["startup_timeout_sec"]` when numeric.
    pub startup_timeout_sec: Option<f64>,
    /// Mirrors `extra["tool_timeout_sec"]` when numeric.
    pub tool_timeout_sec: Option<f64>,
}

/// Locate the server map. Returns the map and whether it sat under an `mcpServers` wrapper.
pub fn server_map(doc: &Value) -> Option<(&Map<String, Value>, bool)> {
    let obj = doc.as_object()?;
    match obj.get("mcpServers") {
        Some(Value::Object(inner)) => Some((inner, true)),
        _ => Some((obj, false)),
    }
}

/// JSON pointer to a server inside a document of the given shape.
pub fn locator_for(name: &str, wrapped: bool) -> Locator {
    let pointer = if wrapped {
        super::json_pointer(&["mcpServers", name])
    } else {
        super::json_pointer(&[name])
    };
    Locator::JsonPointer { pointer }
}

/// Parse one server definition. The transport is taken from `type` when present, otherwise
/// inferred: `url` → http, `command` → stdio.
pub fn parse_server(name: &str, v: &Value) -> Result<ParsedServer, String> {
    let Some(obj) = v.as_object() else {
        return Err(format!(
            "server '{name}' should be an object, got {}",
            super::value_kind(v)
        ));
    };
    let declared = obj
        .get("type")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_ascii_lowercase());
    let kind = match declared.as_deref() {
        Some("stdio") => "stdio",
        Some("http") => "http",
        Some("sse") => "sse",
        Some(other) => {
            return Err(format!(
                "server '{name}': unsupported transport type '{other}'"
            ))
        }
        None if obj.get("url").is_some_and(Value::is_string) => "http",
        None if obj.get("command").is_some_and(Value::is_string) => "stdio",
        None => {
            return Err(format!(
                "server '{name}': cannot infer transport (no type, url or command)"
            ))
        }
    };

    let modeled: &[&str] = match kind {
        "stdio" => &["type", "command", "args", "env", "cwd"],
        _ => &["type", "url", "headers"],
    };
    let extra: Map<String, Value> = obj
        .iter()
        .filter(|(k, _)| !modeled.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let string = |key: &str| obj.get(key).and_then(Value::as_str).map(str::to_string);
    let object = |key: &str| {
        obj.get(key)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    };

    let transport = match kind {
        "stdio" => McpTransport::Stdio {
            command: string("command").unwrap_or_default(),
            args: obj
                .get("args")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|i| match i {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            env: object("env"),
            cwd: string("cwd"),
        },
        "http" => McpTransport::Http {
            url: string("url").unwrap_or_default(),
            headers: object("headers"),
            // Not a Claude Code key, but it shows up in shared `.mcp.json` files. Mirrored for the
            // form; `extra` stays authoritative for round-tripping.
            bearer_token_env_var: string("bearer_token_env_var"),
        },
        _ => McpTransport::Sse {
            url: string("url").unwrap_or_default(),
            headers: object("headers"),
        },
    };

    Ok(ParsedServer {
        transport,
        startup_timeout_sec: extra.get("startup_timeout_sec").and_then(Value::as_f64),
        tool_timeout_sec: extra.get("tool_timeout_sec").and_then(Value::as_f64),
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn wrapper_probe_detects_both_shapes() {
        let wrapped = json!({ "mcpServers": { "a": { "url": "https://x" } }, "other": 1 });
        let (map, is_wrapped) = server_map(&wrapped).unwrap();
        assert!(is_wrapped);
        assert_eq!(map.keys().collect::<Vec<_>>(), vec!["a"]);

        let bare = json!({ "linear": { "type": "http", "url": "https://mcp.linear.app/mcp" } });
        let (map, is_wrapped) = server_map(&bare).unwrap();
        assert!(!is_wrapped);
        assert_eq!(map.keys().collect::<Vec<_>>(), vec!["linear"]);

        // A non-object `mcpServers` value is not a wrapper.
        let odd = json!({ "mcpServers": [], "s": { "command": "x" } });
        let (map, is_wrapped) = server_map(&odd).unwrap();
        assert!(!is_wrapped);
        assert_eq!(map.len(), 2);

        assert!(server_map(&json!([1, 2])).is_none());
        assert_eq!(
            locator_for("a/b", true),
            Locator::JsonPointer {
                pointer: "/mcpServers/a~1b".into()
            }
        );
        assert_eq!(
            locator_for("a", false),
            Locator::JsonPointer {
                pointer: "/a".into()
            }
        );
    }

    #[test]
    fn transport_inference_and_extra_passthrough() {
        let http = parse_server("h", &json!({ "url": "https://x", "headers": { "Authorization": "Bearer ${T}" }, "bearer_token_env_var": "T", "tool_timeout_sec": 120 })).unwrap();
        match &http.transport {
            McpTransport::Http {
                url,
                headers,
                bearer_token_env_var,
            } => {
                assert_eq!(url, "https://x");
                assert_eq!(headers.get("Authorization").unwrap(), "Bearer ${T}");
                assert_eq!(bearer_token_env_var.as_deref(), Some("T"));
            }
            other => panic!("expected http, got {other:?}"),
        }
        assert_eq!(http.tool_timeout_sec, Some(120.0));
        assert_eq!(
            http.extra.keys().collect::<Vec<_>>(),
            vec!["bearer_token_env_var", "tool_timeout_sec"]
        );

        let stdio = parse_server(
            "s",
            &json!({ "command": "npx", "env": { "API_KEY": "secret" }, "startup_timeout_sec": 5 }),
        )
        .unwrap();
        match &stdio.transport {
            McpTransport::Stdio {
                command,
                args,
                env,
                cwd,
            } => {
                assert_eq!(command, "npx");
                assert!(args.is_empty(), "args default to empty");
                assert_eq!(
                    env.get("API_KEY").unwrap(),
                    "secret",
                    "env is not masked here"
                );
                assert_eq!(cwd, &None);
            }
            other => panic!("expected stdio, got {other:?}"),
        }
        assert_eq!(stdio.startup_timeout_sec, Some(5.0));

        let sse = parse_server("e", &json!({ "type": "SSE", "url": "https://s" })).unwrap();
        assert!(matches!(sse.transport, McpTransport::Sse { .. }));
        assert!(sse.extra.is_empty());

        // Explicit `type` wins over inference; foreign keys land in `extra`.
        let explicit = parse_server(
            "x",
            &json!({ "type": "stdio", "command": "c", "args": ["-a", 1], "url": "ignored" }),
        )
        .unwrap();
        match &explicit.transport {
            McpTransport::Stdio { args, .. } => {
                assert_eq!(args, &vec!["-a".to_string(), "1".to_string()])
            }
            other => panic!("expected stdio, got {other:?}"),
        }
        assert_eq!(explicit.extra.get("url").unwrap(), "ignored");

        assert!(parse_server("bad", &json!({ "type": "ws", "url": "wss://x" })).is_err());
        assert!(parse_server("bad", &json!({ "args": [] })).is_err());
        assert!(parse_server("bad", &json!("nope")).is_err());
    }
}
