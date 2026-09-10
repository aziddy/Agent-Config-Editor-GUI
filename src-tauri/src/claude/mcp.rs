//! MCP servers at user, local and project scope, plus the enable/disable semantics shared with
//! plugin-provided servers.
//!
//! | Server lives in | `enabled` | `editable` | Toggle origin |
//! | --- | --- | --- | --- |
//! | `~/.claude.json` `/mcpServers/<n>` (user) | `Some(true)`; reason lists projects whose `disabledMcpServers` names it | yes | `~/.claude.json` `/projects` (per-project picker) |
//! | `~/.claude.json` `/projects/<root>/mcpServers/<n>` (local) | `Some(true)` | no | none: delete to remove |
//! | `<root>/.mcp.json` | `Some(true)` if in `enabledMcpjsonServers`, `Some(false)` if in `disabledMcpjsonServers`, else `Some(false)` "not approved yet" | yes | `~/.claude.json` `/projects/<root>/enabledMcpjsonServers` |
//! | plugin `.mcp.json` | plugin enabled AND not `plugin:<plugin>:<n>` in any `disabledMcpServers` | yes | the settings file's `/enabledPlugins/<id>` |
//!
//! When a server is disabled in some projects, `extra["disabledInProjects"]` carries their
//! roots. That key is synthetic: writers must strip it before serializing `extra`.
//!
//! `needs_auth` comes from `~/.claude/mcp-needs-auth-cache.json`, keyed by bare server name or
//! `plugin:<plugin>:<server>`.

use super::mcp_json::{self, ParsedServer};
use super::state_json::{ClaudeState, ProjectState};
use super::{config_id, load_json_object, origin, pointer_locator, project_scope, AGENT};
use crate::fsutil;
use crate::model::{EnabledState, EntityKind, McpServer, Origin, Scope};
use crate::scan::{ScanCtx, ScanOut};
use serde_json::Value;
use std::collections::HashSet;

pub const LOCAL_SCOPE_REASON: &str = "local-scope servers have no disable switch; delete to remove";
pub const NOT_APPROVED_REASON: &str = "not approved yet; Claude Code will prompt";
pub const DISABLED_FOR_PROJECT_REASON: &str = "disabled for this project";
pub const DISABLED_IN_PROJECTS_KEY: &str = "disabledInProjects";

/// Keys of `~/.claude/mcp-needs-auth-cache.json`.
#[derive(Debug, Clone, Default)]
pub struct NeedsAuth {
    keys: HashSet<String>,
}

impl NeedsAuth {
    pub fn load(ctx: &ScanCtx<'_>, out: &mut ScanOut) -> Self {
        let path = ctx.homes.claude_dir.join("mcp-needs-auth-cache.json");
        let keys = load_json_object(&path, out)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        Self { keys }
    }

    /// A user, local or project server by bare name.
    pub fn server(&self, name: &str) -> bool {
        self.keys.contains(name)
    }

    /// A plugin server, keyed `plugin:<plugin name>:<server>`.
    pub fn plugin_server(&self, plugin_name: &str, server: &str) -> bool {
        self.keys.contains(&plugin_server_key(plugin_name, server))
    }
}

/// The id Claude Code uses for a plugin server inside `disabledMcpServers` and the auth cache.
pub fn plugin_server_key(plugin_name: &str, server: &str) -> String {
    format!("plugin:{plugin_name}:{server}")
}

/// `"Disabled in projects: a, b"` or `None`.
pub fn disabled_in_projects_reason(roots: &[String]) -> Option<String> {
    if roots.is_empty() {
        None
    } else {
        Some(format!("Disabled in projects: {}", roots.join(", ")))
    }
}

/// Assemble an [`McpServer`]; `extra` and timeouts come from the parsed definition.
pub(crate) fn build_server(
    name: &str,
    scope: Scope,
    origin: Origin,
    parsed: ParsedServer,
    enabled: EnabledState,
    from_plugin: Option<String>,
    needs_auth: bool,
) -> McpServer {
    McpServer {
        id: config_id(EntityKind::McpServer, &scope, name),
        name: name.to_string(),
        agent: AGENT,
        scope,
        origin,
        transport: parsed.transport,
        enabled,
        startup_timeout_sec: parsed.startup_timeout_sec,
        tool_timeout_sec: parsed.tool_timeout_sec,
        from_plugin,
        extra: parsed.extra,
        runtime: None,
        needs_auth,
        dup_group: None,
    }
}

/// Record the projects that disable a server in `extra["disabledInProjects"]`.
pub(crate) fn note_disabled_in_projects(server: &mut McpServer, roots: &[String]) {
    if !roots.is_empty() {
        server.extra.insert(
            DISABLED_IN_PROJECTS_KEY.to_string(),
            Value::Array(roots.iter().cloned().map(Value::String).collect()),
        );
    }
}

/// User-scope servers from `~/.claude.json`, local-scope servers from its `projects` map, and
/// `<root>/.mcp.json` for every existing tracked project.
pub fn scan(_ctx: &ScanCtx<'_>, out: &mut ScanOut, state: &ClaudeState, needs_auth: &NeedsAuth) {
    scan_user_servers(out, state, needs_auth);
    for project in &state.projects {
        if !project.exists {
            continue;
        }
        scan_local_servers(out, state, project, needs_auth);
        scan_project_mcp_json(out, state, project, needs_auth);
    }
}

fn scan_user_servers(out: &mut ScanOut, state: &ClaudeState, needs_auth: &NeedsAuth) {
    for (name, v) in &state.mcp_servers {
        let parsed = match mcp_json::parse_server(name, v) {
            Ok(p) => p,
            Err(e) => {
                out.warn(Some(&state.path), format!("/mcpServers: {e}"));
                continue;
            }
        };
        let disabled_in = state.projects_disabling(name);
        let enabled = EnabledState::toggleable(
            true,
            origin(Scope::User, &state.path, pointer_locator(&["projects"])),
            disabled_in_projects_reason(&disabled_in),
        );
        let mut server = build_server(
            name,
            Scope::User,
            origin(
                Scope::User,
                &state.path,
                pointer_locator(&["mcpServers", name]),
            ),
            parsed,
            enabled,
            None,
            needs_auth.server(name),
        );
        note_disabled_in_projects(&mut server, &disabled_in);
        out.mcp_servers.push(server);
    }
}

fn scan_local_servers(
    out: &mut ScanOut,
    state: &ClaudeState,
    project: &ProjectState,
    needs_auth: &NeedsAuth,
) {
    let root = fsutil::display(&project.root);
    for (name, v) in &project.mcp_servers {
        let parsed = match mcp_json::parse_server(name, v) {
            Ok(p) => p,
            Err(e) => {
                out.warn(
                    Some(&state.path),
                    format!("/projects/{root}/mcpServers: {e}"),
                );
                continue;
            }
        };
        let scope = project_scope(&project.root);
        let mut server = build_server(
            name,
            scope.clone(),
            origin(
                scope,
                &state.path,
                pointer_locator(&["projects", &root, "mcpServers", name]),
            ),
            parsed,
            EnabledState::fixed(true, Some(LOCAL_SCOPE_REASON.to_string())),
            None,
            needs_auth.server(name),
        );
        // A `.mcp.json` server of the same name would collide on id; keep both rows distinct.
        if out.mcp_servers.iter().any(|s| s.id == server.id) {
            server.id.push_str("#local");
        }
        out.mcp_servers.push(server);
    }
}

fn scan_project_mcp_json(
    out: &mut ScanOut,
    state: &ClaudeState,
    project: &ProjectState,
    needs_auth: &NeedsAuth,
) {
    let path = project.root.join(".mcp.json");
    let Some(doc) = load_json_object(&path, out) else {
        return;
    };
    let doc = Value::Object(doc);
    let Some((servers, wrapped)) = mcp_json::server_map(&doc) else {
        return;
    };
    if !wrapped {
        out.warn(
            Some(&path),
            "no mcpServers wrapper; treating top-level keys as servers",
        );
    }
    let root = fsutil::display(&project.root);
    for (name, v) in servers {
        let parsed = match mcp_json::parse_server(name, v) {
            Ok(p) => p,
            Err(e) => {
                out.warn(Some(&path), e);
                continue;
            }
        };
        let scope = project_scope(&project.root);
        let (on, reason) = if project.enabled_mcpjson_servers.iter().any(|s| s == name) {
            (true, None)
        } else if project.disabled_mcpjson_servers.iter().any(|s| s == name) {
            (false, Some(DISABLED_FOR_PROJECT_REASON.to_string()))
        } else {
            (false, Some(NOT_APPROVED_REASON.to_string()))
        };
        let toggle = origin(
            scope.clone(),
            &state.path,
            pointer_locator(&["projects", &root, "enabledMcpjsonServers"]),
        );
        let mut server = build_server(
            name,
            scope.clone(),
            origin(scope, &path, mcp_json::locator_for(name, wrapped)),
            parsed,
            EnabledState::toggleable(on, toggle, reason),
            None,
            needs_auth.server(name),
        );
        if out.mcp_servers.iter().any(|s| s.id == server.id) {
            server.id.push_str("#mcpjson");
        }
        out.mcp_servers.push(server);
    }
}
