//! `~/.claude.json`: the live state file Claude Code rewrites on every run.
//!
//! Only the MCP-related keys and the tracked project list are extracted:
//!
//! - top-level `mcpServers` (user scope),
//! - `projects["<abs root>"]` with `mcpServers` (local scope), `enabledMcpjsonServers`,
//!   `disabledMcpjsonServers` and the undocumented but load-bearing `disabledMcpServers`
//!   (bare user-server names or `plugin:<plugin>:<server>`).
//!
//! The remaining ~90 top-level keys (caches, onboarding flags, telemetry) and ~30 per-project
//! keys are ignored here, but the file hash is recorded so writers can detect external change.

use super::{load_json_object, str_list, AGENT};
use crate::fsutil;
use crate::scan::{ScanCtx, ScanOut};
use serde_json::{Map, Value};
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct ProjectState {
    pub root: PathBuf,
    pub exists: bool,
    /// `/projects/<root>/mcpServers`: local-scope servers, no disable switch.
    pub mcp_servers: Map<String, Value>,
    /// `<root>/.mcp.json` servers the user approved for this project.
    pub enabled_mcpjson_servers: Vec<String>,
    /// `<root>/.mcp.json` servers the user rejected for this project.
    pub disabled_mcpjson_servers: Vec<String>,
    /// User-scope servers (bare name) and plugin servers (`plugin:<plugin>:<server>`) turned off
    /// for this project.
    pub disabled_mcp_servers: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ClaudeState {
    pub path: PathBuf,
    /// The file exists and parsed.
    pub present: bool,
    /// Top-level `mcpServers` (user scope).
    pub mcp_servers: Map<String, Value>,
    /// One entry per `projects` key, in file order.
    pub projects: Vec<ProjectState>,
}

impl ClaudeState {
    /// Display paths of every tracked project whose `disabledMcpServers` lists `server_id`.
    pub fn projects_disabling(&self, server_id: &str) -> Vec<String> {
        self.projects
            .iter()
            .filter(|p| p.disabled_mcp_servers.iter().any(|s| s == server_id))
            .map(|p| fsutil::display(&p.root))
            .collect()
    }
}

/// Load `~/.claude.json` and register every tracked project with `out`.
pub fn load(ctx: &ScanCtx<'_>, out: &mut ScanOut) -> ClaudeState {
    let path = ctx.homes.claude_state.clone();
    let mut state = ClaudeState {
        path: path.clone(),
        ..Default::default()
    };
    let Some(doc) = load_json_object(&path, out) else {
        return state;
    };
    state.present = true;

    match doc.get("mcpServers") {
        Some(Value::Object(m)) => state.mcp_servers = m.clone(),
        Some(other) if !other.is_null() => {
            out.warn(
                Some(&path),
                format!(
                    "/mcpServers should be an object, got {}",
                    super::value_kind(other)
                ),
            );
        }
        _ => {}
    }

    let Some(projects) = doc.get("projects") else {
        return state;
    };
    let Some(projects) = projects.as_object() else {
        out.warn(Some(&path), "/projects should be an object");
        return state;
    };
    for (root_key, v) in projects {
        let root = ctx.homes.expand_tilde(root_key);
        out.track_project(&root, AGENT);
        let obj = v.as_object();
        let get = |key: &str| obj.and_then(|o| o.get(key));
        state.projects.push(ProjectState {
            exists: root.is_dir(),
            root,
            mcp_servers: get("mcpServers")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
            enabled_mcpjson_servers: str_list(get("enabledMcpjsonServers")),
            disabled_mcpjson_servers: str_list(get("disabledMcpjsonServers")),
            disabled_mcp_servers: str_list(get("disabledMcpServers")),
        });
    }
    state
}
