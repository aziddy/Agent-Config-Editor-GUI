//! `<codex_home>/config.toml` and `<project>/.codex/config.toml`.
//!
//! Both are parsed with [`toml_edit::DocumentMut`] so a later write can be a targeted edit that
//! keeps comments (cmux sentinels), blank lines, empty tables, quoted keys and literal strings.
//! Nothing here ever re-serializes through the `toml` crate.

use crate::fsutil::{self, read_text_opt};
use crate::model::{
    Agent, EnabledState, EntityKind, Locator, McpServer, McpTransport, Origin, Scope,
};
use crate::scan::{ScanCtx, ScanOut};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, TableLike};

/// One `[[skills.config]]` element. Codex keys them by `name` *or* absolute `path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillsConfigEntry {
    /// Position inside the `[[skills.config]]` array of tables.
    pub index: usize,
    pub name: Option<String>,
    pub path: Option<String>,
    pub enabled: bool,
}

/// A parsed `<root>/.codex/config.toml`.
#[derive(Debug)]
pub struct ProjectConfig {
    pub root: PathBuf,
    pub path: PathBuf,
    pub doc: DocumentMut,
}

/// Read-only view over the user `config.toml` plus every tracked project config.
#[derive(Debug)]
pub struct CodexConfig {
    /// `<codex_home>/config.toml`, whether or not it exists.
    pub path: PathBuf,
    /// `None` when the file is missing or failed to parse (a warning was recorded).
    pub doc: Option<DocumentMut>,
    pub skills_config: Vec<SkillsConfigEntry>,
    /// `(plugin id "name@marketplace", enabled)` from `[plugins."…"]`.
    pub plugins: Vec<(String, bool)>,
    /// Names of `[marketplaces.<n>]` tables.
    pub marketplaces: Vec<String>,
    /// Roots of `[projects."<abs path>"]` tables, in file order.
    pub projects: Vec<PathBuf>,
    pub features: Map<String, Value>,
    /// Keys of `[hooks.state."…"]`, i.e. `"<hooks.json path>:<snake_event>:<i>:<j>"`.
    pub hook_trust_keys: Vec<String>,
    pub project_configs: Vec<ProjectConfig>,
}

impl CodexConfig {
    /// `[mcp_servers.<name>]` tables of the user config, in file order.
    pub fn mcp_servers(&self) -> Vec<(String, &dyn TableLike)> {
        self.doc
            .as_ref()
            .map(|d| mcp_servers_of(d.as_table()))
            .unwrap_or_default()
    }

    /// Find the `[[skills.config]]` entry governing a skill: exact `path` match first, then `name`.
    pub fn skills_entry(&self, path: &str, name: &str) -> Option<&SkillsConfigEntry> {
        self.skills_config
            .iter()
            .find(|e| e.path.as_deref() == Some(path))
            .or_else(|| {
                self.skills_config
                    .iter()
                    .find(|e| e.name.as_deref() == Some(name))
            })
    }

    pub fn is_known_marketplace(&self, name: &str) -> bool {
        self.marketplaces.iter().any(|m| m == name)
    }

    /// Where a toggle inside the user `config.toml` lives.
    pub fn toggle_origin(&self, path: Vec<String>) -> Origin {
        Origin {
            agent: Agent::Codex,
            scope: Scope::User,
            file: fsutil::display(&self.path),
            locator: Locator::TomlPath { path },
        }
    }
}

/// Parse TOML text, recording a warning (and returning `None`) on failure.
pub fn parse_doc(path: &Path, text: &str, out: &mut ScanOut) -> Option<DocumentMut> {
    match text.parse::<DocumentMut>() {
        Ok(doc) => Some(doc),
        Err(e) => {
            out.warn(Some(path), format!("TOML parse error: {}", e.message()));
            None
        }
    }
}

/// Read and parse a TOML file. A missing path (or one that is not a regular file, e.g. a repo
/// whose `.codex` is a file) is silently `None`; other failures record a warning.
fn load_toml(path: &Path, out: &mut ScanOut) -> Option<DocumentMut> {
    if !path.is_file() {
        return None;
    }
    let text = match read_text_opt(path) {
        Ok(Some(t)) => t,
        Ok(None) => return None,
        Err(e) => {
            out.warn(Some(path), e.message);
            return None;
        }
    };
    out.record_hash(path, &text);
    parse_doc(path, &text, out)
}

/// Load the user config and every tracked project config. Registers projects with `out`.
pub fn load(ctx: &ScanCtx<'_>, out: &mut ScanOut) -> CodexConfig {
    let path = ctx.homes.codex_home.join("config.toml");
    let doc = load_toml(&path, out);

    let mut config = CodexConfig {
        path,
        doc: None,
        skills_config: Vec::new(),
        plugins: Vec::new(),
        marketplaces: Vec::new(),
        projects: Vec::new(),
        features: Map::new(),
        hook_trust_keys: Vec::new(),
        project_configs: Vec::new(),
    };

    if let Some(doc) = doc {
        let root = doc.as_table();

        config.skills_config = skills_config_of(root);

        if let Some(plugins) = root.get("plugins").and_then(Item::as_table_like) {
            for (id, item) in plugins.iter() {
                let enabled = item
                    .as_table_like()
                    .and_then(|t| t.get("enabled"))
                    .and_then(Item::as_bool)
                    .unwrap_or(true);
                config.plugins.push((id.to_string(), enabled));
            }
        }

        if let Some(mkts) = root.get("marketplaces").and_then(Item::as_table_like) {
            config.marketplaces = mkts.iter().map(|(k, _)| k.to_string()).collect();
        }

        if let Some(projects) = root.get("projects").and_then(Item::as_table_like) {
            for (root_key, _) in projects.iter() {
                let root = PathBuf::from(root_key);
                out.track_project(&root, Agent::Codex);
                config.projects.push(root);
            }
        }

        if let Some(features) = root.get("features").and_then(Item::as_table_like) {
            config.features = table_like_to_json(features);
        }

        if let Some(state) = root
            .get("hooks")
            .and_then(Item::as_table_like)
            .and_then(|h| h.get("state"))
            .and_then(Item::as_table_like)
        {
            config.hook_trust_keys = state.iter().map(|(k, _)| k.to_string()).collect();
        }

        config.doc = Some(doc);
    }

    for root in config.projects.clone() {
        if !root.is_dir() {
            continue;
        }
        let path = root.join(".codex").join("config.toml");
        // `[projects."<home>"]` is common; its `.codex/config.toml` *is* the user config.
        if same_file(&path, &config.path) {
            continue;
        }
        if let Some(doc) = load_toml(&path, out) {
            config
                .project_configs
                .push(ProjectConfig { root, path, doc });
        }
    }

    config
}

/// Same path textually, or the same file after resolving symlinks.
pub fn same_file(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

/// Push one [`McpServer`] per `[mcp_servers.<n>]` in the user config and every project config.
pub fn scan_mcp_servers(config: &CodexConfig, out: &mut ScanOut) {
    for (name, table) in config.mcp_servers() {
        let src = McpSource::config(&config.path, Scope::User, &name);
        if let Some(server) = mcp_server_from_fields(&name, table_like_to_json(table), src, out) {
            out.mcp_servers.push(server);
        }
    }
    for project in &config.project_configs {
        let scope = Scope::Project {
            root: fsutil::display(&project.root),
        };
        for (name, table) in mcp_servers_of(project.doc.as_table()) {
            let src = McpSource::config(&project.path, scope.clone(), &name);
            if let Some(server) = mcp_server_from_fields(&name, table_like_to_json(table), src, out)
            {
                out.mcp_servers.push(server);
            }
        }
    }
}

fn mcp_servers_of(root: &dyn TableLike) -> Vec<(String, &dyn TableLike)> {
    root.get("mcp_servers")
        .and_then(Item::as_table_like)
        .map(|servers| {
            servers
                .iter()
                .filter_map(|(name, item)| item.as_table_like().map(|t| (name.to_string(), t)))
                .collect()
        })
        .unwrap_or_default()
}

fn skills_config_of(root: &dyn TableLike) -> Vec<SkillsConfigEntry> {
    let Some(item) = root
        .get("skills")
        .and_then(Item::as_table_like)
        .and_then(|s| s.get("config"))
    else {
        return Vec::new();
    };
    tables_of(item)
        .into_iter()
        .enumerate()
        .map(|(index, t)| SkillsConfigEntry {
            index,
            name: t.get("name").and_then(Item::as_str).map(String::from),
            path: t.get("path").and_then(Item::as_str).map(String::from),
            enabled: t.get("enabled").and_then(Item::as_bool).unwrap_or(true),
        })
        .collect()
}

/// The tables of an array-of-tables (`[[x]]`) or an inline array of inline tables (`x = [{…}]`).
pub fn tables_of(item: &Item) -> Vec<&dyn TableLike> {
    if let Some(aot) = item.as_array_of_tables() {
        return aot.iter().map(|t| t as &dyn TableLike).collect();
    }
    if let Some(arr) = item.as_array() {
        return arr
            .iter()
            .filter_map(|v| v.as_inline_table().map(|t| t as &dyn TableLike))
            .collect();
    }
    Vec::new()
}

// ---------------------------------------------------------------------------
// TOML → JSON (for `extra`, `features`, frontmatter-like views)
// ---------------------------------------------------------------------------

pub fn value_to_json(v: &toml_edit::Value) -> Value {
    match v {
        toml_edit::Value::String(s) => Value::String(s.value().clone()),
        toml_edit::Value::Integer(i) => Value::from(*i.value()),
        toml_edit::Value::Float(f) => serde_json::Number::from_f64(*f.value())
            .map(Value::Number)
            .unwrap_or(Value::Null),
        toml_edit::Value::Boolean(b) => Value::Bool(*b.value()),
        toml_edit::Value::Datetime(d) => Value::String(d.value().to_string()),
        toml_edit::Value::Array(a) => Value::Array(a.iter().map(value_to_json).collect()),
        toml_edit::Value::InlineTable(t) => Value::Object(
            t.iter()
                .map(|(k, v)| (k.to_string(), value_to_json(v)))
                .collect(),
        ),
    }
}

pub fn item_to_json(item: &Item) -> Value {
    match item {
        Item::None => Value::Null,
        Item::Value(v) => value_to_json(v),
        Item::Table(t) => Value::Object(table_like_to_json(t)),
        Item::ArrayOfTables(a) => Value::Array(
            a.iter()
                .map(|t| Value::Object(table_like_to_json(t)))
                .collect(),
        ),
    }
}

pub fn table_like_to_json(t: &dyn TableLike) -> Map<String, Value> {
    t.iter()
        .map(|(k, item)| (k.to_string(), item_to_json(item)))
        .collect()
}

// ---------------------------------------------------------------------------
// MCP server field mapping, shared by config.toml tables and plugin .mcp.json objects
// ---------------------------------------------------------------------------

/// Where an MCP server definition came from and where its toggle lives.
#[derive(Debug, Clone)]
pub struct McpSource {
    pub scope: Scope,
    /// The server definition itself.
    pub origin: Origin,
    /// Where `enabled` would be written.
    pub toggle: Origin,
    pub from_plugin: Option<String>,
    /// `Some(enabled)` for plugin servers, whose state follows the plugin.
    pub plugin_enabled: Option<bool>,
}

impl McpSource {
    /// A `[mcp_servers.<name>]` table in `config_file`.
    pub fn config(config_file: &Path, scope: Scope, name: &str) -> Self {
        let origin = |path: Vec<String>| Origin {
            agent: Agent::Codex,
            scope: scope.clone(),
            file: fsutil::display(config_file),
            locator: Locator::TomlPath { path },
        };
        McpSource {
            origin: origin(vec!["mcp_servers".to_string(), name.to_string()]),
            toggle: origin(vec![
                "mcp_servers".to_string(),
                name.to_string(),
                "enabled".to_string(),
            ]),
            scope,
            from_plugin: None,
            plugin_enabled: None,
        }
    }
}

fn take_str(v: Value, extra: &mut Map<String, Value>, key: &str) -> Option<String> {
    match v {
        Value::String(s) => Some(s),
        other => {
            extra.insert(key.to_string(), other);
            None
        }
    }
}

fn take_f64(v: Value, extra: &mut Map<String, Value>, key: &str) -> Option<f64> {
    match v.as_f64() {
        Some(f) => Some(f),
        None => {
            extra.insert(key.to_string(), v);
            None
        }
    }
}

fn take_map(v: Value, extra: &mut Map<String, Value>, key: &str) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        other => {
            extra.insert(key.to_string(), other);
            Map::new()
        }
    }
}

/// Build an [`McpServer`] from its raw key/value fields. Unmodeled keys land in `extra`;
/// modeled keys with an unexpected type also land in `extra` so nothing is lost on save.
/// Returns `None` (with a warning) when there is neither `command` nor `url`.
pub fn mcp_server_from_fields(
    name: &str,
    fields: Map<String, Value>,
    src: McpSource,
    out: &mut ScanOut,
) -> Option<McpServer> {
    let mut extra = Map::new();
    let mut command = None;
    let mut args = Vec::new();
    let mut env = Map::new();
    let mut cwd = None;
    let mut url = None;
    let mut headers = Map::new();
    let mut bearer = None;
    let mut transport_type = None;
    let mut startup_timeout_sec = None;
    let mut tool_timeout_sec = None;
    let mut enabled_flag = None;

    for (key, value) in fields {
        match key.as_str() {
            "command" => command = take_str(value, &mut extra, &key),
            "args" => match value {
                Value::Array(items) => {
                    args = items
                        .into_iter()
                        .map(|i| match i {
                            Value::String(s) => s,
                            other => other.to_string(),
                        })
                        .collect()
                }
                other => {
                    extra.insert(key, other);
                }
            },
            "env" => env = take_map(value, &mut extra, &key),
            "cwd" => cwd = take_str(value, &mut extra, &key),
            "url" => url = take_str(value, &mut extra, &key),
            "headers" => headers = take_map(value, &mut extra, &key),
            "bearer_token_env_var" => bearer = take_str(value, &mut extra, &key),
            "type" => transport_type = take_str(value, &mut extra, &key),
            "startup_timeout_sec" => startup_timeout_sec = take_f64(value, &mut extra, &key),
            "tool_timeout_sec" => tool_timeout_sec = take_f64(value, &mut extra, &key),
            "enabled" => match value {
                Value::Bool(b) => enabled_flag = Some(b),
                other => {
                    extra.insert(key, other);
                }
            },
            _ => {
                extra.insert(key, value);
            }
        }
    }

    let transport = match (command, url) {
        (Some(command), url) => {
            if let Some(u) = url {
                extra.insert("url".to_string(), Value::String(u));
            }
            if let Some(b) = bearer {
                extra.insert("bearer_token_env_var".to_string(), Value::String(b));
            }
            if !headers.is_empty() {
                extra.insert("headers".to_string(), Value::Object(headers));
            }
            McpTransport::Stdio {
                command,
                args,
                env,
                cwd,
            }
        }
        (None, Some(url)) => {
            if !env.is_empty() {
                extra.insert("env".to_string(), Value::Object(env));
            }
            if transport_type.as_deref() == Some("sse") {
                if let Some(b) = bearer {
                    extra.insert("bearer_token_env_var".to_string(), Value::String(b));
                }
                McpTransport::Sse { url, headers }
            } else {
                McpTransport::Http {
                    url,
                    headers,
                    bearer_token_env_var: bearer,
                }
            }
        }
        (None, None) => {
            out.warn(
                Some(Path::new(&src.origin.file)),
                format!("mcp server `{name}` has neither `command` nor `url`; skipped"),
            );
            return None;
        }
    };
    if let Some(t) = transport_type {
        if !matches!(t.as_str(), "stdio" | "http" | "sse") {
            extra.insert("type".to_string(), Value::String(t));
        }
    }

    let own_enabled = enabled_flag.unwrap_or(true);
    let enabled = match (&src.plugin_enabled, &src.from_plugin) {
        (Some(plugin_enabled), Some(plugin_id)) => {
            let reason = if !own_enabled {
                format!("disabled in the plugin's .mcp.json (plugin {plugin_id})")
            } else {
                format!("follows plugin {plugin_id}")
            };
            EnabledState::toggleable(*plugin_enabled && own_enabled, src.toggle, Some(reason))
        }
        _ => EnabledState::toggleable(own_enabled, src.toggle, None),
    };

    Some(McpServer {
        id: format!(
            "codex:{}:{}:{}",
            EntityKind::McpServer.key(),
            src.scope.key(),
            name
        ),
        name: name.to_string(),
        agent: Agent::Codex,
        scope: src.scope,
        origin: src.origin,
        transport,
        enabled,
        startup_timeout_sec,
        tool_timeout_sec,
        from_plugin: src.from_plugin,
        extra,
        runtime: None,
        needs_auth: false,
        dup_group: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const FIXTURE: &str = include_str!("../../tests/fixtures/codex_home/.codex/config.toml.tpl");

    #[test]
    fn fixture_round_trips_byte_identical() {
        // Sentinel comments spanning table boundaries, an empty `[tui.keymap.chat]`, quoted keys
        // with spaces, mixed quoted/bare keys and a literal string all have to survive.
        let doc: DocumentMut = FIXTURE.parse().expect("fixture parses");
        assert_eq!(doc.to_string(), FIXTURE);
    }

    #[test]
    fn extracts_config_views() {
        let doc: DocumentMut = FIXTURE.parse().unwrap();
        let root = doc.as_table();

        let servers = mcp_servers_of(root);
        let names: Vec<&str> = servers.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["alpha", "beta", "docs"]);

        let entries = skills_config_of(root);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name.as_deref(), Some("two"));
        assert!(!entries[0].enabled);
        assert_eq!(
            entries[1].path.as_deref(),
            Some("__ROOT__/.codex/skills/one/SKILL.md")
        );
        assert_eq!(entries[2].index, 2);
        assert!(entries[2].enabled);

        let projects: Vec<String> = root
            .get("projects")
            .and_then(Item::as_table_like)
            .unwrap()
            .iter()
            .map(|(k, _)| k.to_string())
            .collect();
        assert_eq!(
            projects,
            vec![
                "__ROOT__/projects/repo a",
                "__ROOT__/projects/repo-b",
                "__ROOT__"
            ]
        );

        let trust: Vec<String> = root
            .get("hooks")
            .and_then(Item::as_table_like)
            .and_then(|h| h.get("state"))
            .and_then(Item::as_table_like)
            .unwrap()
            .iter()
            .map(|(k, _)| k.to_string())
            .collect();
        assert_eq!(trust, vec!["__ROOT__/.codex/hooks.json:pre_tool_use:0:0"]);
    }

    #[test]
    fn env_sub_table_literal_string_is_verbatim() {
        let doc: DocumentMut = FIXTURE.parse().unwrap();
        let (name, alpha) = &mcp_servers_of(doc.as_table())[0];
        let fields = table_like_to_json(*alpha);
        assert_eq!(
            fields["env"]["ALPHA_JSON"],
            Value::String(r#"{"a":1}"#.to_string())
        );
        assert_eq!(fields["startup_timeout_sec"], Value::from(120));
        let mut out = ScanOut::default();
        let src = McpSource::config(Path::new("/x/config.toml"), Scope::User, name);
        let server = mcp_server_from_fields(name, fields, src, &mut out).unwrap();
        assert_eq!(server.id, "codex:mcp:user:alpha");
        assert_eq!(server.startup_timeout_sec, Some(120.0));
        assert_eq!(server.tool_timeout_sec, Some(30.5));
        match &server.transport {
            McpTransport::Stdio {
                command,
                args,
                env,
                cwd,
            } => {
                assert_eq!(command, "npx");
                assert_eq!(args, &["alpha-mcp@latest", "--stdio"]);
                assert_eq!(env["ALPHA_JSON"], r#"{"a":1}"#);
                assert_eq!(*cwd, None);
            }
            other => panic!("expected stdio, got {other:?}"),
        }
        assert_eq!(server.enabled.enabled, Some(true));
        assert_eq!(
            server.enabled.toggle.as_ref().unwrap().locator,
            Locator::TomlPath {
                path: vec!["mcp_servers".into(), "alpha".into(), "enabled".into()]
            }
        );
        assert!(out.warnings.is_empty());
    }

    #[test]
    fn server_without_command_or_url_warns() {
        let mut out = ScanOut::default();
        let mut fields = Map::new();
        fields.insert("enabled".to_string(), Value::Bool(true));
        let src = McpSource::config(Path::new("/x/config.toml"), Scope::User, "broken");
        assert!(mcp_server_from_fields("broken", fields, src, &mut out).is_none());
        assert_eq!(out.warnings.len(), 1);
    }

    #[test]
    fn sse_and_unknown_keys() {
        let mut out = ScanOut::default();
        let fields: Map<String, Value> = serde_json::from_str(
            r#"{"type":"sse","url":"https://s","headers":{"A":"b"},"oauth_resource":"r","enabled":false}"#,
        )
        .unwrap();
        let src = McpSource::config(Path::new("/x/config.toml"), Scope::User, "s");
        let server = mcp_server_from_fields("s", fields, src, &mut out).unwrap();
        assert!(matches!(server.transport, McpTransport::Sse { .. }));
        assert_eq!(server.enabled.enabled, Some(false));
        assert_eq!(
            server.extra.get("oauth_resource"),
            Some(&Value::String("r".into()))
        );
        assert!(!server.extra.contains_key("type"));
    }
}
