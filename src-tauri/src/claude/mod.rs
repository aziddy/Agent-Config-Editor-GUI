//! Claude Code readers.
//!
//! On-disk sources and the module that reads each one:
//!
//! | Source | Module |
//! | --- | --- |
//! | `~/.claude.json` live state: user MCP servers, tracked projects, per-project MCP approvals | [`state_json`] |
//! | `~/.claude/settings.json`, `settings.local.json`, managed settings, `<root>/.claude/settings*.json` | [`settings`] |
//! | `hooks` blocks inside every settings file | [`hooks`] |
//! | `.mcp.json` documents (wrapped project files, wrapper-less plugin files) | [`mcp_json`] |
//! | MCP enable/disable semantics and `~/.claude/mcp-needs-auth-cache.json` | [`mcp`] |
//! | `~/.claude/plugins/*` registry, marketplaces, cache dirs and their contributions | [`plugins`] |
//! | `skills/*/SKILL.md` at user, project and plugin scope | [`skills`] |
//! | `agents/*.md` and `commands/**/*.md` at user, project and plugin scope | [`agents_commands`] |
//!
//! Every file read is hashed into `ScanOut::file_hashes`; every parse failure becomes a warning
//! and the scan continues. Missing files and directories are normal and never warn.
//!
//! Entity ids: file-backed entities (skills, agents, commands) use `claude:<kind>:<file path>`;
//! config-backed entities (MCP servers, plugins, hooks) use `claude:<kind>:<scope key>:<name>`.

pub mod agents_commands;
pub mod hooks;
pub mod mcp;
pub mod mcp_json;
pub mod plugins;
pub mod settings;
pub mod skills;
pub mod state_json;

use crate::fsutil::{self, read_text_opt};
use crate::markdown::frontmatter::{self, ParsedMarkdown};
use crate::model::{Agent, EntityKind, Locator, Origin, Scope};
use crate::scan::{ScanCtx, ScanOut};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

pub const AGENT: Agent = Agent::Claude;

/// Read everything Claude Code knows about and push it into `out`.
pub fn scan(ctx: &ScanCtx<'_>, out: &mut ScanOut) {
    let state = state_json::load(ctx, out);
    let roots = project_roots(ctx, &state);
    let settings = settings::load_all(ctx, out, &roots);
    hooks::scan(&settings, out);
    let needs_auth = mcp::NeedsAuth::load(ctx, out);
    mcp::scan(ctx, out, &state, &needs_auth);
    plugins::scan(ctx, out, &settings, &state, &needs_auth);
    skills::scan(ctx, out, &roots);
    agents_commands::scan(ctx, out, &roots);
}

/// Tracked project roots whose `.claude/` tree is scanned at project scope.
///
/// The root must exist on disk, and it must not be the home directory itself: Claude Code
/// tracks `~` as a project when launched from there, but `~/.claude/` is the user-scope tree
/// and must be read exactly once.
pub fn project_roots(ctx: &ScanCtx<'_>, state: &state_json::ClaudeState) -> Vec<PathBuf> {
    state
        .projects
        .iter()
        .filter(|p| p.exists && p.root.join(".claude") != ctx.homes.claude_dir)
        .map(|p| p.root.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// Shared helpers for the submodules
// ---------------------------------------------------------------------------

/// Id for a file-backed entity: `claude:<kind>:<path>`.
pub(crate) fn file_id(kind: EntityKind, path: &Path) -> String {
    format!("claude:{}:{}", kind.key(), fsutil::display(path))
}

/// Id for a config-backed entity: `claude:<kind>:<scope key>:<name>`.
pub(crate) fn config_id(kind: EntityKind, scope: &Scope, name: &str) -> String {
    format!("claude:{}:{}:{}", kind.key(), scope.key(), name)
}

/// RFC 6901 escaping for a single reference token (`~` → `~0`, `/` → `~1`).
pub(crate) fn escape_pointer_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// Build a JSON pointer from unescaped tokens.
pub(crate) fn json_pointer(tokens: &[&str]) -> String {
    let mut out = String::new();
    for t in tokens {
        out.push('/');
        out.push_str(&escape_pointer_token(t));
    }
    out
}

pub(crate) fn pointer_locator(tokens: &[&str]) -> Locator {
    Locator::JsonPointer {
        pointer: json_pointer(tokens),
    }
}

pub(crate) fn origin(scope: Scope, file: &Path, locator: Locator) -> Origin {
    Origin {
        agent: AGENT,
        scope,
        file: fsutil::display(file),
        locator,
    }
}

pub(crate) fn project_scope(root: &Path) -> Scope {
    Scope::Project {
        root: fsutil::display(root),
    }
}

/// Read and parse a JSON file. `None` when missing, unreadable or malformed (the latter two warn).
pub(crate) fn load_json(path: &Path, out: &mut ScanOut) -> Option<Value> {
    let text = match read_text_opt(path) {
        Ok(Some(t)) => t,
        Ok(None) => return None,
        Err(e) => {
            out.warn(Some(path), format!("cannot read file: {}", e.message));
            return None;
        }
    };
    out.record_hash(path, &text);
    match serde_json::from_str::<Value>(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            out.warn(Some(path), format!("invalid JSON: {e}"));
            None
        }
    }
}

/// Like [`load_json`] but also requires the document to be an object.
pub(crate) fn load_json_object(path: &Path, out: &mut ScanOut) -> Option<Map<String, Value>> {
    match load_json(path, out)? {
        Value::Object(m) => Some(m),
        other => {
            out.warn(
                Some(path),
                format!(
                    "expected a JSON object at the top level, got {}",
                    value_kind(&other)
                ),
            );
            None
        }
    }
}

pub(crate) fn value_kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Read a Markdown file and split its frontmatter. Records the hash.
///
/// Claude Code's own frontmatter reader is more lenient than strict YAML: real agent files carry
/// an unquoted `description` that embeds `: ` (`Context: …` inside an `<example>`), which YAML
/// rejects as "mapping values are not allowed". When strict parsing fails, the fields are
/// recovered line by line with [`lenient_fields`] and no warning is raised, matching what Claude
/// Code accepts. Only a fence whose keys cannot be recovered at all still warns.
pub(crate) fn read_markdown(path: &Path, out: &mut ScanOut) -> Option<(String, ParsedMarkdown)> {
    let text = match read_text_opt(path) {
        Ok(Some(t)) => t,
        Ok(None) => return None,
        Err(e) => {
            out.warn(Some(path), format!("cannot read file: {}", e.message));
            return None;
        }
    };
    out.record_hash(path, &text);
    let mut parsed = frontmatter::parse(&text);
    if let Some(w) = parsed.warning.clone() {
        let recovered = if parsed.frontmatter.present
            && parsed.frontmatter.fields.is_empty()
            && w.starts_with("frontmatter YAML error")
        {
            lenient_fields(&parsed.frontmatter.raw)
        } else {
            Map::new()
        };
        if recovered.is_empty() {
            out.warn(Some(path), w);
        } else {
            parsed.frontmatter.fields = recovered;
        }
    }
    Some((text, parsed))
}

/// Recover top-level `key: value` pairs from frontmatter text that strict YAML rejects.
///
/// Handles what Claude Code files actually contain: plain scalars (kept verbatim, including
/// literal `\n` sequences), quoted scalars (decoded), `true`/`false`, folded continuation lines,
/// one level of `- item` block sequences and one level of nested `key: value` mappings.
pub(crate) fn lenient_fields(raw: &str) -> Map<String, Value> {
    let mut map: Map<String, Value> = Map::new();
    let mut current: Option<String> = None;
    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        if let Some((key, rest)) = split_key(line) {
            map.insert(key.to_string(), lenient_scalar(rest.trim()));
            current = Some(key.to_string());
            continue;
        }
        let Some(key) = &current else {
            continue;
        };
        if !(line.starts_with(' ') || line.starts_with('\t')) {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(entry) = map.get_mut(key) else {
            continue;
        };
        if let Some(item) = trimmed.strip_prefix("- ") {
            match entry {
                Value::Array(items) => items.push(lenient_scalar(item.trim())),
                Value::String(s) if s.is_empty() => {
                    *entry = Value::Array(vec![lenient_scalar(item.trim())])
                }
                _ => append_text(entry, trimmed),
            }
        } else if let Some((sub_key, sub_rest)) = split_key(trimmed) {
            match entry {
                Value::Object(obj) => {
                    obj.insert(sub_key.to_string(), lenient_scalar(sub_rest.trim()));
                }
                Value::String(s) if s.is_empty() => {
                    let mut obj = Map::new();
                    obj.insert(sub_key.to_string(), lenient_scalar(sub_rest.trim()));
                    *entry = Value::Object(obj);
                }
                _ => append_text(entry, trimmed),
            }
        } else {
            append_text(entry, trimmed);
        }
    }
    map
}

/// `key: rest` when `key` is a plain identifier at column 0 and the colon is followed by
/// whitespace or the end of the line.
fn split_key(line: &str) -> Option<(&str, &str)> {
    let idx = line.find(':')?;
    let key = &line[..idx];
    let valid_key = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if !valid_key {
        return None;
    }
    let rest = &line[idx + 1..];
    if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
        Some((key, rest))
    } else {
        None
    }
}

fn lenient_scalar(v: &str) -> Value {
    match v {
        "" | ">" | "|" | ">-" | "|-" | ">+" | "|+" => Value::String(String::new()),
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ if v.len() >= 2
            && ((v.starts_with('"') && v.ends_with('"'))
                || (v.starts_with('\'') && v.ends_with('\''))) =>
        {
            serde_yaml_ng::from_str::<Value>(v)
                .ok()
                .filter(Value::is_string)
                .unwrap_or_else(|| Value::String(v[1..v.len() - 1].to_string()))
        }
        _ => Value::String(v.to_string()),
    }
}

fn append_text(entry: &mut Value, text: &str) {
    match entry {
        Value::String(s) => {
            if !s.is_empty() {
                s.push(' ');
            }
            s.push_str(text);
        }
        other => *other = Value::String(text.to_string()),
    }
}

/// Directory entries sorted by file name. Missing or unreadable directories yield nothing.
pub(crate) fn sorted_dir(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    entries
}

pub(crate) fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(crate) fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(crate) fn is_markdown(path: &Path) -> bool {
    path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md")
}

pub(crate) fn str_field(obj: &Map<String, Value>, key: &str) -> Option<String> {
    obj.get(key).and_then(Value::as_str).map(str::to_string)
}

/// `["a", "b"]` → `vec!["a", "b"]`; anything else → empty.
pub(crate) fn str_list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|i| i.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// Folded YAML scalars (`description: >`) end in a newline; trim the tail but keep inner `\n`.
pub(crate) fn clean_description(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim_end().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_escaping_follows_rfc_6901() {
        assert_eq!(escape_pointer_token("a/b~c"), "a~1b~0c");
        assert_eq!(
            json_pointer(&["projects", "/Users/me/repo", "enabledMcpjsonServers"]),
            "/projects/~1Users~1me~1repo/enabledMcpjsonServers"
        );
        assert_eq!(
            json_pointer(&["enabledPlugins", "demo@mkt"]),
            "/enabledPlugins/demo@mkt"
        );
    }

    #[test]
    fn ids_follow_the_two_shapes() {
        assert_eq!(
            file_id(EntityKind::Skill, Path::new("/h/.claude/skills/a/SKILL.md")),
            "claude:skill:/h/.claude/skills/a/SKILL.md"
        );
        assert_eq!(
            config_id(EntityKind::McpServer, &Scope::User, "workos"),
            "claude:mcp:user:workos"
        );
        assert_eq!(
            config_id(EntityKind::Plugin, &project_scope(Path::new("/r")), "x@m"),
            "claude:plugin:project:/r:x@m"
        );
    }

    #[test]
    fn lenient_fields_recover_what_claude_code_accepts() {
        // The real `ui-tweaker.md` shape: unquoted description containing `: ` and literal `\n`.
        let raw = "name: ui-tweaker\ndescription: Use this agent.\\n\\n<example>\\nContext: The user asks\\nuser: \"hi\"\\n</example>\nmodel: sonnet\ncolor: green";
        assert!(
            serde_yaml_ng::from_str::<Value>(raw).is_err(),
            "precondition: strict YAML rejects this"
        );
        let f = lenient_fields(raw);
        assert_eq!(
            f.keys().collect::<Vec<_>>(),
            vec!["name", "description", "model", "color"]
        );
        assert_eq!(
            f["description"],
            "Use this agent.\\n\\n<example>\\nContext: The user asks\\nuser: \"hi\"\\n</example>"
        );
        assert_eq!(f["model"], "sonnet");

        // Quoted scalars decode escapes; folded, list and nested forms survive one level.
        let raw = "description: \"a\\nb\"\ntools:\n  - Read\n  - Grep\nfolded: >\n  one\n  two\nmetadata:\n  version: \"2\"\nflag: true";
        let f = lenient_fields(raw);
        assert_eq!(f["description"], "a\nb");
        assert_eq!(f["tools"], serde_json::json!(["Read", "Grep"]));
        assert_eq!(f["folded"], "one two");
        assert_eq!(f["metadata"]["version"], "2");
        assert_eq!(f["flag"], true);

        assert!(lenient_fields("just prose, no keys").is_empty());
        assert_eq!(
            split_key("http://x"),
            None,
            "colon not followed by space is not a key"
        );
        assert_eq!(split_key("key:"), Some(("key", "")));
    }

    #[test]
    fn description_cleanup_keeps_inner_newlines() {
        assert_eq!(
            clean_description(Some("a\n\nb\n".into())).as_deref(),
            Some("a\n\nb")
        );
        assert_eq!(clean_description(Some("  \n".into())), None);
        assert_eq!(clean_description(None), None);
    }
}
