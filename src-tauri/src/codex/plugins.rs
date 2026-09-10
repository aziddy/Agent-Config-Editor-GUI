//! Codex plugins: one [`Plugin`] per `[plugins."name@marketplace"]` entry in `config.toml`,
//! resolved against `<codex_home>/plugins/cache/<marketplace>/<name>/<version>/`.
//!
//! The version directory holds `.codex-plugin/plugin.json` (`skills`, `mcpServers`, optional
//! `commands` paths), a `.mcp.json` (observed **with** a `mcpServers` wrapper, but both shapes are
//! probed), `skills/<n>/SKILL.md` and `commands/*.md`. Several version dirs may coexist; the newest
//! by mtime is live and the rest are `stale_dirs`. Symlinks such as `latest` are ignored.

use super::config_toml::{self, CodexConfig, McpSource};
use super::skills;
use crate::fsutil::{self, read_text_opt};
use crate::markdown::frontmatter::{self, field_bool, field_str, string_or_list};
use crate::model::{
    Agent, EnabledState, EntityKind, Locator, Origin, Plugin, PluginContributions, Scope,
    SlashCommand,
};
use crate::scan::{ScanCtx, ScanOut};
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub fn scan(ctx: &ScanCtx<'_>, config: &CodexConfig, out: &mut ScanOut) {
    let cache_root = ctx.homes.codex_home.join("plugins").join("cache");
    for (id, enabled) in &config.plugins {
        let plugin = read_plugin(config, &cache_root, id, *enabled, out);
        out.plugins.push(plugin);
    }
}

/// Split `name@marketplace` at the last `@`. No `@` → marketplace is empty.
pub fn split_plugin_id(id: &str) -> (&str, &str) {
    match id.rfind('@') {
        Some(i) => (&id[..i], &id[i + 1..]),
        None => (id, ""),
    }
}

fn read_plugin(
    config: &CodexConfig,
    cache_root: &Path,
    id: &str,
    enabled: bool,
    out: &mut ScanOut,
) -> Plugin {
    let (name, marketplace) = split_plugin_id(id);
    let toggle = config.toggle_origin(vec![
        "plugins".to_string(),
        id.to_string(),
        "enabled".to_string(),
    ]);
    let unresolved_marketplace = marketplace.is_empty()
        || !(config.is_known_marketplace(marketplace) || cache_root.join(marketplace).is_dir());

    let mut versions = version_dirs(&cache_root.join(marketplace).join(name));
    let live = versions.pop();
    let stale_dirs = versions.iter().map(|(p, _)| fsutil::display(p)).collect();

    let mut plugin = Plugin {
        id: id.to_string(),
        name: name.to_string(),
        marketplace: marketplace.to_string(),
        agent: Agent::Codex,
        scope: Scope::User,
        enabled: EnabledState::toggleable(enabled, toggle.clone(), None),
        version: live
            .as_ref()
            .and_then(|(p, _)| p.file_name().map(|n| n.to_string_lossy().into_owned())),
        install_path: live.as_ref().map(|(p, _)| fsutil::display(p)),
        description: None,
        author: None,
        homepage: None,
        unresolved_marketplace,
        installed: live.is_some(),
        in_use: false,
        stale_dirs,
        contributions: PluginContributions::default(),
    };

    let Some((live_dir, _)) = live else {
        return plugin;
    };
    let Some(manifest) = read_manifest(&live_dir, out) else {
        return plugin;
    };

    if let Some(n) = manifest.get("name").and_then(Value::as_str) {
        plugin.name = n.to_string();
    }
    if let Some(v) = manifest.get("version").and_then(Value::as_str) {
        plugin.version = Some(v.to_string());
    }
    plugin.description = manifest
        .get("description")
        .and_then(Value::as_str)
        .map(String::from);
    plugin.homepage = manifest
        .get("homepage")
        .and_then(Value::as_str)
        .map(String::from);
    plugin.author = match manifest.get("author") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Object(o)) => o.get("name").and_then(Value::as_str).map(String::from),
        _ => None,
    };

    let scope = Scope::Plugin {
        plugin_id: id.to_string(),
    };

    // Skills: only the declared directory (Codex loads nothing undeclared).
    if let Some(rel) = manifest.get("skills").and_then(Value::as_str) {
        let skills_dir = resolve_rel(&live_dir, rel);
        for skill_md in skills::find_skill_files(&skills_dir, 2) {
            if let Some(skill) =
                skills::read_skill(config, &skill_md, scope.clone(), false, false, out)
            {
                plugin.contributions.skills.push(skill.id.clone());
                out.skills.push(skill);
            }
        }
    }

    // MCP servers from the declared .mcp.json.
    if let Some(rel) = manifest.get("mcpServers").and_then(Value::as_str) {
        let mcp_path = resolve_rel(&live_dir, rel);
        for server in read_plugin_mcp(&mcp_path, id, enabled, &scope, &toggle, out) {
            plugin.contributions.mcp_servers.push(server.id.clone());
            out.mcp_servers.push(server);
        }
    }

    // Commands: declared path, else the conventional `commands/` directory.
    let commands_dir = manifest
        .get("commands")
        .and_then(Value::as_str)
        .map(|rel| resolve_rel(&live_dir, rel))
        .unwrap_or_else(|| live_dir.join("commands"));
    for md in markdown_files(&commands_dir) {
        if let Some(cmd) = read_command(&md, scope.clone(), out) {
            plugin.contributions.commands.push(cmd.id.clone());
            out.commands.push(cmd);
        }
    }

    plugin
}

/// Real (non-symlink) subdirectories of `plugin_dir`, sorted oldest → newest by mtime, then name.
fn version_dirs(plugin_dir: &Path) -> Vec<(PathBuf, SystemTime)> {
    let Ok(entries) = fs::read_dir(plugin_dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<(PathBuf, SystemTime)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let ft = e.file_type().ok()?;
            if ft.is_symlink() || !meta.is_dir() {
                return None;
            }
            let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            Some((e.path(), mtime))
        })
        .collect();
    dirs.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    dirs
}

fn read_manifest(live_dir: &Path, out: &mut ScanOut) -> Option<Map<String, Value>> {
    let path = live_dir.join(".codex-plugin").join("plugin.json");
    let text = match read_text_opt(&path) {
        Ok(Some(t)) => t,
        Ok(None) => return None,
        Err(e) => {
            out.warn(Some(&path), e.message);
            return None;
        }
    };
    out.record_hash(&path, &text);
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(m)) => Some(m),
        Ok(_) => {
            out.warn(Some(&path), "plugin.json is not a JSON object");
            None
        }
        Err(e) => {
            out.warn(Some(&path), format!("JSON parse error: {e}"));
            None
        }
    }
}

/// Resolve a manifest path (`./skills/`, `.mcp.json`, absolute) against the plugin directory.
pub fn resolve_rel(base: &Path, rel: &str) -> PathBuf {
    let trimmed = rel.strip_prefix("./").unwrap_or(rel);
    let trimmed = trimmed.trim_end_matches('/');
    if Path::new(trimmed).is_absolute() {
        PathBuf::from(trimmed)
    } else {
        base.join(trimmed)
    }
}

/// RFC 6901 escaping for one JSON pointer segment.
pub fn json_pointer_segment(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}

fn read_plugin_mcp(
    mcp_path: &Path,
    plugin_id: &str,
    plugin_enabled: bool,
    scope: &Scope,
    toggle: &Origin,
    out: &mut ScanOut,
) -> Vec<crate::model::McpServer> {
    let text = match read_text_opt(mcp_path) {
        Ok(Some(t)) => t,
        Ok(None) => return Vec::new(),
        Err(e) => {
            out.warn(Some(mcp_path), e.message);
            return Vec::new();
        }
    };
    out.record_hash(mcp_path, &text);
    let root = match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(m)) => m,
        Ok(_) => {
            out.warn(Some(mcp_path), ".mcp.json is not a JSON object");
            return Vec::new();
        }
        Err(e) => {
            out.warn(Some(mcp_path), format!("JSON parse error: {e}"));
            return Vec::new();
        }
    };
    // Probe both shapes: `{ "mcpServers": { … } }` (observed for Codex plugins) and a bare map.
    let (servers, prefix) = match root.get("mcpServers") {
        Some(Value::Object(inner)) => (inner.clone(), "/mcpServers"),
        _ => (root, ""),
    };

    let mut result = Vec::new();
    for (name, def) in servers {
        let Value::Object(fields) = def else {
            out.warn(
                Some(mcp_path),
                format!("mcp server `{name}` is not a JSON object; skipped"),
            );
            continue;
        };
        let src = McpSource {
            scope: scope.clone(),
            origin: Origin {
                agent: Agent::Codex,
                scope: scope.clone(),
                file: fsutil::display(mcp_path),
                locator: Locator::JsonPointer {
                    pointer: format!("{prefix}/{}", json_pointer_segment(&name)),
                },
            },
            toggle: toggle.clone(),
            from_plugin: Some(plugin_id.to_string()),
            plugin_enabled: Some(plugin_enabled),
        };
        if let Some(server) = config_toml::mcp_server_from_fields(&name, fields, src, out) {
            result.push(server);
        }
    }
    result
}

/// `*.md` files directly inside `dir`, sorted. Missing dir → empty.
pub fn markdown_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("md"))
        .collect();
    files.sort();
    files
}

/// `argument-hint: [target]` is valid YAML for a one-element array; render it back as written.
fn argument_hint(fm: &crate::model::Frontmatter) -> Option<String> {
    match fm.fields.get("argument-hint") {
        Some(Value::Array(items)) => Some(format!(
            "[{}]",
            items
                .iter()
                .map(|i| match i {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect::<Vec<_>>()
                .join(", ")
        )),
        _ => field_str(fm, "argument-hint"),
    }
}

/// Build a [`SlashCommand`] from a `commands/<name>.md` file.
pub fn read_command(path: &Path, scope: Scope, out: &mut ScanOut) -> Option<SlashCommand> {
    let text = match read_text_opt(path) {
        Ok(Some(t)) => t,
        Ok(None) => return None,
        Err(e) => {
            out.warn(Some(path), e.message);
            return None;
        }
    };
    out.record_hash(path, &text);
    let parsed = frontmatter::parse(&text);
    if let Some(w) = &parsed.warning {
        out.warn(Some(path), w.clone());
    }
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let path_str = fsutil::display(path);
    Some(SlashCommand {
        id: format!("codex:{}:{}", EntityKind::SlashCommand.key(), path_str),
        name,
        agent: Agent::Codex,
        scope: scope.clone(),
        origin: Origin {
            agent: Agent::Codex,
            scope,
            file: path_str,
            locator: Locator::MarkdownFile,
        },
        description: field_str(&parsed.frontmatter, "description").map(|d| d.trim().to_string()),
        argument_hint: argument_hint(&parsed.frontmatter),
        allowed_tools: string_or_list(parsed.frontmatter.fields.get("allowed-tools")),
        disable_model_invocation: field_bool(&parsed.frontmatter, "disable-model-invocation"),
        frontmatter: parsed.frontmatter,
        body_preview: frontmatter::preview(&parsed.body, 160),
        content_hash: fsutil::sha256_hex(text.as_bytes()),
        dup_group: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_ids_and_resolves_paths() {
        assert_eq!(
            split_plugin_id("linear@openai-curated"),
            ("linear", "openai-curated")
        );
        assert_eq!(split_plugin_id("a@b@c"), ("a@b", "c"));
        assert_eq!(split_plugin_id("bare"), ("bare", ""));
        let base = Path::new("/cache/mkt/p/1.0");
        assert_eq!(
            resolve_rel(base, "./skills/"),
            PathBuf::from("/cache/mkt/p/1.0/skills")
        );
        assert_eq!(
            resolve_rel(base, ".mcp.json"),
            PathBuf::from("/cache/mkt/p/1.0/.mcp.json")
        );
        assert_eq!(resolve_rel(base, "/abs/x"), PathBuf::from("/abs/x"));
        assert_eq!(json_pointer_segment("a/b~c"), "a~1b~0c");
    }

    #[test]
    fn plugin_mcp_json_without_wrapper_and_disabled_plugin() {
        let dir = tempfile::tempdir().unwrap();
        let mcp = dir.path().join(".mcp.json");
        std::fs::write(
            &mcp,
            r#"{ "bare": { "command": "node", "args": ["x.mjs"] }, "remote": { "url": "https://r/mcp" } }"#,
        )
        .unwrap();
        let scope = Scope::Plugin {
            plugin_id: "p@m".into(),
        };
        let toggle = Origin {
            agent: Agent::Codex,
            scope: Scope::User,
            file: "/h/.codex/config.toml".into(),
            locator: Locator::TomlPath {
                path: vec!["plugins".into(), "p@m".into(), "enabled".into()],
            },
        };
        let mut out = ScanOut::default();
        let servers = read_plugin_mcp(&mcp, "p@m", false, &scope, &toggle, &mut out);
        assert!(out.warnings.is_empty());
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].name, "bare");
        assert_eq!(
            servers[0].origin.locator,
            Locator::JsonPointer {
                pointer: "/bare".into()
            }
        );
        assert_eq!(servers[0].from_plugin.as_deref(), Some("p@m"));
        assert_eq!(
            servers[0].enabled.enabled,
            Some(false),
            "follows the disabled plugin"
        );
        assert_eq!(servers[0].enabled.toggle.as_ref(), Some(&toggle));
        assert_eq!(servers[0].id, "codex:mcp:plugin:p@m:bare");
        assert!(matches!(
            &servers[1].transport,
            crate::model::McpTransport::Http { .. }
        ));
    }
}
