//! Plugins: `~/.claude/plugins/installed_plugins.json`, `known_marketplaces.json`, the
//! marketplace catalogs and the plugin cache.
//!
//! - `installed_plugins.json`: `{ version: 2, plugins: { "<name>@<mkt>": [ { scope, installPath,
//!   version, installedAt, lastUpdated } ] } }`. The value is an **array**, one entry per scope
//!   (an older object-valued shape is accepted too). One [`Plugin`] is emitted per entry.
//! - `known_marketplaces.json`: `{ "<mkt>": { source, installLocation, lastUpdated } }`. A plugin
//!   whose marketplace is missing here is flagged `unresolved_marketplace`.
//! - `<installLocation>/.claude-plugin/marketplace.json` lists `plugins: [ { name, description,
//!   homepage, author, category, version, … } ]` and fills in metadata the installed
//!   `plugin.json` lacks.
//! - `enabled` comes from `enabledPlugins` in the settings file matching the entry's scope; the
//!   toggle origin is that file with pointer `/enabledPlugins/<id>`. Ids listed in
//!   `enabledPlugins` but not installed produce `installed: false` rows.
//! - `<installPath>/.in_use/<pid>` markers set `in_use`; sibling version dirs under
//!   `cache/<mkt>/<name>/` other than the live ones are `stale_dirs`.
//! - Contributions: `skills/*/SKILL.md`, `agents/*.md`, `commands/**/*.md` and a wrapper-less
//!   `.mcp.json` under the live install path, plus any extra paths `plugin.json` declares under
//!   `skills` / `agents` / `commands` / `mcpServers` (string or array of paths; `mcpServers` may
//!   also be an inline object). They are pushed with `Scope::Plugin { plugin_id }` and their ids
//!   recorded in [`PluginContributions`].
//! - `version` prefers the semantic version in `plugin.json`, then the catalog, then the content
//!   hash recorded by `installed_plugins.json`.

use super::mcp::{self, NeedsAuth};
use super::settings::{self, SettingsSet};
use super::state_json::ClaudeState;
use super::{
    agents_commands, load_json_object, mcp_json, origin, pointer_locator, project_scope, skills,
    sorted_dir, str_field, value_kind, AGENT,
};
use crate::fsutil;
use crate::model::{EnabledState, Origin, Plugin, PluginContributions, Scope};
use crate::scan::{ScanCtx, ScanOut};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// `name@marketplace` → `(name, marketplace)`. A missing `@` yields an empty marketplace.
pub fn split_id(id: &str) -> (String, String) {
    match id.rsplit_once('@') {
        Some((name, mkt)) => (name.to_string(), mkt.to_string()),
        None => (id.to_string(), String::new()),
    }
}

#[derive(Debug, Clone)]
struct InstalledEntry {
    scope: Scope,
    /// Installed through `settings.local.json` (`scope: "local"`).
    local: bool,
    install_path: PathBuf,
    /// Content hash recorded by Claude Code.
    version: Option<String>,
}

#[derive(Debug, Default)]
struct Marketplaces {
    /// Marketplace name → install location.
    known: BTreeMap<String, PathBuf>,
    /// Marketplace name → plugin name → catalog entry.
    catalogs: BTreeMap<String, Map<String, Value>>,
}

impl Marketplaces {
    fn load(ctx: &ScanCtx<'_>, plugins_dir: &Path, out: &mut ScanOut) -> Self {
        let mut markets = Marketplaces::default();
        let path = plugins_dir.join("known_marketplaces.json");
        let Some(doc) = load_json_object(&path, out) else {
            return markets;
        };
        for (name, v) in &doc {
            let Some(obj) = v.as_object() else {
                out.warn(
                    Some(&path),
                    format!("marketplace '{name}' should be an object"),
                );
                continue;
            };
            let Some(loc) = str_field(obj, "installLocation") else {
                continue;
            };
            let loc = ctx.homes.expand_tilde(&loc);
            let catalog_path = loc.join(".claude-plugin").join("marketplace.json");
            let mut catalog = Map::new();
            if let Some(cat) = load_json_object(&catalog_path, out) {
                if let Some(entries) = cat.get("plugins").and_then(Value::as_array) {
                    for entry in entries {
                        if let Some(obj) = entry.as_object() {
                            if let Some(plugin_name) = str_field(obj, "name") {
                                catalog.insert(plugin_name, Value::Object(obj.clone()));
                            }
                        }
                    }
                }
            }
            markets.known.insert(name.clone(), loc);
            markets.catalogs.insert(name.clone(), catalog);
        }
        markets
    }

    fn is_known(&self, mkt: &str) -> bool {
        self.known.contains_key(mkt)
    }

    fn entry(&self, mkt: &str, name: &str) -> Option<&Map<String, Value>> {
        self.catalogs.get(mkt)?.get(name)?.as_object()
    }
}

fn load_installed(
    ctx: &ScanCtx<'_>,
    plugins_dir: &Path,
    out: &mut ScanOut,
) -> Vec<(String, Vec<InstalledEntry>)> {
    let path = plugins_dir.join("installed_plugins.json");
    let Some(doc) = load_json_object(&path, out) else {
        return Vec::new();
    };
    let plugins = match doc.get("plugins") {
        Some(Value::Object(m)) => m,
        Some(other) => {
            out.warn(
                Some(&path),
                format!("/plugins should be an object, got {}", value_kind(other)),
            );
            return Vec::new();
        }
        None => return Vec::new(),
    };
    let mut result = Vec::new();
    for (id, v) in plugins {
        let raw: Vec<&Map<String, Value>> = match v {
            Value::Array(items) => items.iter().filter_map(Value::as_object).collect(),
            Value::Object(o) => vec![o],
            other => {
                out.warn(
                    Some(&path),
                    format!(
                        "/plugins/{id} should be an array of entries, got {}",
                        value_kind(other)
                    ),
                );
                continue;
            }
        };
        let mut entries = Vec::new();
        for e in raw {
            let Some(install_path) = str_field(e, "installPath") else {
                out.warn(
                    Some(&path),
                    format!("plugin {id}: entry without installPath"),
                );
                continue;
            };
            let scope_name = str_field(e, "scope").unwrap_or_else(|| "user".to_string());
            let (scope, local) = match scope_name.as_str() {
                "user" => (Scope::User, false),
                "managed" => (Scope::Managed, false),
                "project" | "local" => {
                    let root = ["projectPath", "projectRoot", "root", "path"]
                        .iter()
                        .find_map(|k| str_field(e, k));
                    match root {
                        Some(r) => (
                            project_scope(&ctx.homes.expand_tilde(&r)),
                            scope_name == "local",
                        ),
                        None => {
                            out.warn(
                                Some(&path),
                                format!("plugin {id}: {scope_name}-scope entry without a project path; treating as user scope"),
                            );
                            (Scope::User, false)
                        }
                    }
                }
                other => {
                    out.warn(
                        Some(&path),
                        format!("plugin {id}: unknown scope '{other}'; treating as user scope"),
                    );
                    (Scope::User, false)
                }
            };
            entries.push(InstalledEntry {
                scope,
                local,
                install_path: ctx.homes.expand_tilde(&install_path),
                version: str_field(e, "version"),
            });
        }
        result.push((id.clone(), entries));
    }
    result
}

/// Emit every installed plugin, every `enabledPlugins` id that is not installed, and the
/// skills / agents / commands / MCP servers contributed by live install paths.
pub fn scan(
    ctx: &ScanCtx<'_>,
    out: &mut ScanOut,
    settings: &SettingsSet,
    state: &ClaudeState,
    needs_auth: &NeedsAuth,
) {
    let plugins_dir = ctx.homes.claude_dir.join("plugins");
    let markets = Marketplaces::load(ctx, &plugins_dir, out);
    let installed = load_installed(ctx, &plugins_dir, out);
    let mut seen: HashSet<(String, String)> = HashSet::new();

    for (id, entries) in &installed {
        let live: HashSet<PathBuf> = entries.iter().map(|e| e.install_path.clone()).collect();
        for entry in entries {
            let plugin = build_installed(
                ctx, out, settings, state, needs_auth, &markets, id, entry, &live,
            );
            seen.insert((plugin.scope.key(), id.clone()));
            out.plugins.push(plugin);
        }
    }

    for file in &settings.files {
        for (id, on) in settings::enabled_plugins(&file.value) {
            if installed.iter().any(|(i, _)| *i == id) {
                continue;
            }
            if !seen.insert((file.scope.key(), id.clone())) {
                continue;
            }
            let (name, mkt) = split_id(&id);
            let catalog = markets.entry(&mkt, &name);
            let unresolved = !markets.is_known(&mkt);
            let mut reasons = vec!["listed in enabledPlugins but not installed".to_string()];
            if unresolved {
                reasons.push(unresolved_reason(&mkt));
            }
            let toggle = settings.origin_in(file, pointer_locator(&["enabledPlugins", &id]));
            out.plugins.push(Plugin {
                id: id.clone(),
                name,
                marketplace: mkt,
                agent: AGENT,
                scope: file.scope.clone(),
                enabled: EnabledState::toggleable(on, toggle, Some(reasons.join("; "))),
                version: catalog.and_then(|c| str_field(c, "version")),
                install_path: None,
                description: catalog.and_then(|c| str_field(c, "description")),
                author: catalog.and_then(author_name),
                homepage: catalog.and_then(|c| str_field(c, "homepage")),
                unresolved_marketplace: unresolved,
                installed: false,
                in_use: false,
                stale_dirs: Vec::new(),
                contributions: PluginContributions::default(),
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_installed(
    ctx: &ScanCtx<'_>,
    out: &mut ScanOut,
    settings: &SettingsSet,
    state: &ClaudeState,
    needs_auth: &NeedsAuth,
    markets: &Marketplaces,
    id: &str,
    entry: &InstalledEntry,
    live: &HashSet<PathBuf>,
) -> Plugin {
    let (name, mkt) = split_id(id);
    let catalog = markets.entry(&mkt, &name);
    let install_path = &entry.install_path;
    let exists = install_path.is_dir();
    if !exists {
        out.warn(
            Some(install_path),
            format!("plugin {id}: install path recorded in installed_plugins.json does not exist"),
        );
    }
    let manifest = if exists {
        load_json_object(
            &install_path.join(".claude-plugin").join("plugin.json"),
            out,
        )
    } else {
        None
    };
    let m = manifest.as_ref();

    let (on, missing_reason) = match settings.plugin_enabled(&entry.scope, id) {
        Some((b, _)) => (b, None),
        None => (false, Some("not listed in enabledPlugins".to_string())),
    };
    let toggle = settings.plugin_toggle_origin(&entry.scope, entry.local, id);
    let unresolved = !markets.is_known(&mkt);
    let mut reasons: Vec<String> = missing_reason.into_iter().collect();
    if unresolved {
        reasons.push(unresolved_reason(&mkt));
    }
    let reason = if reasons.is_empty() {
        None
    } else {
        Some(reasons.join("; "))
    };

    let contributions = if exists {
        collect_contributions(
            ctx,
            out,
            state,
            needs_auth,
            id,
            &name,
            install_path,
            m,
            on,
            &toggle,
        )
    } else {
        PluginContributions::default()
    };

    Plugin {
        id: id.to_string(),
        name,
        marketplace: mkt,
        agent: AGENT,
        scope: entry.scope.clone(),
        enabled: EnabledState::toggleable(on, toggle, reason),
        version: m
            .and_then(|m| str_field(m, "version"))
            .or_else(|| catalog.and_then(|c| str_field(c, "version")))
            .or_else(|| entry.version.clone()),
        install_path: Some(fsutil::display(install_path)),
        description: m
            .and_then(|m| str_field(m, "description"))
            .or_else(|| catalog.and_then(|c| str_field(c, "description"))),
        author: m
            .and_then(author_name)
            .or_else(|| catalog.and_then(author_name)),
        homepage: m
            .and_then(|m| str_field(m, "homepage"))
            .or_else(|| catalog.and_then(|c| str_field(c, "homepage"))),
        unresolved_marketplace: unresolved,
        installed: true,
        in_use: has_in_use_marker(install_path),
        stale_dirs: stale_siblings(install_path, live),
        contributions,
    }
}

fn unresolved_reason(mkt: &str) -> String {
    if mkt.is_empty() {
        "plugin id has no @marketplace suffix".to_string()
    } else {
        format!("marketplace '{mkt}' is not in known_marketplaces.json")
    }
}

/// `author` is either a string or `{ name, email? }`.
fn author_name(obj: &Map<String, Value>) -> Option<String> {
    match obj.get("author") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Object(a)) => str_field(a, "name"),
        _ => None,
    }
}

/// `<installPath>/.in_use/` holds one marker file per running Claude Code process.
fn has_in_use_marker(install_path: &Path) -> bool {
    std::fs::read_dir(install_path.join(".in_use"))
        .map(|rd| rd.flatten().any(|e| e.path().is_file()))
        .unwrap_or(false)
}

/// Sibling version directories under `cache/<mkt>/<name>/` that are not live for this plugin.
fn stale_siblings(install_path: &Path, live: &HashSet<PathBuf>) -> Vec<String> {
    let Some(parent) = install_path.parent() else {
        return Vec::new();
    };
    sorted_dir(parent)
        .into_iter()
        .filter(|p| p.is_dir() && !live.contains(p))
        .map(|p| fsutil::display(&p))
        .collect()
}

/// Resolve a path declared in `plugin.json` relative to the install root.
fn resolve_declared(root: &Path, raw: &str) -> PathBuf {
    let rel = raw.strip_prefix("./").unwrap_or(raw);
    root.join(rel)
}

/// Extra paths declared in `plugin.json` under `key` (string or array of strings).
fn declared_paths(manifest: Option<&Map<String, Value>>, key: &str, root: &Path) -> Vec<PathBuf> {
    match manifest.and_then(|m| m.get(key)) {
        Some(Value::String(s)) => vec![resolve_declared(root, s)],
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(|s| resolve_declared(root, s))
            .collect(),
        _ => Vec::new(),
    }
}

/// The default directory plus declared extras, without duplicates.
fn contribution_paths(
    manifest: Option<&Map<String, Value>>,
    key: &str,
    root: &Path,
    default_dir: &str,
) -> Vec<PathBuf> {
    let mut paths = vec![root.join(default_dir)];
    for p in declared_paths(manifest, key, root) {
        if !paths.contains(&p) {
            paths.push(p);
        }
    }
    paths
}

#[allow(clippy::too_many_arguments)]
fn collect_contributions(
    ctx: &ScanCtx<'_>,
    out: &mut ScanOut,
    state: &ClaudeState,
    needs_auth: &NeedsAuth,
    id: &str,
    plugin_name: &str,
    root: &Path,
    manifest: Option<&Map<String, Value>>,
    plugin_on: bool,
    toggle: &Origin,
) -> PluginContributions {
    let scope = Scope::Plugin {
        plugin_id: id.to_string(),
    };
    let mut c = PluginContributions::default();

    for dir in contribution_paths(manifest, "skills", root, "skills") {
        c.skills.extend(skills::scan_dir(&dir, &scope, out));
    }
    for p in contribution_paths(manifest, "agents", root, "agents") {
        if p.is_dir() {
            c.agents
                .extend(agents_commands::scan_agents_dir(&p, &scope, out));
        } else if super::is_markdown(&p) {
            c.agents
                .extend(agents_commands::scan_agent_file(&p, &scope, out));
        }
    }
    for p in contribution_paths(manifest, "commands", root, "commands") {
        if p.is_dir() {
            c.commands
                .extend(agents_commands::scan_commands_dir(&p, &scope, out));
        } else if super::is_markdown(&p) {
            c.commands
                .extend(agents_commands::scan_command_file(&p, &scope, out));
        }
    }

    // MCP: the wrapper-less `.mcp.json` at the root, plus whatever `plugin.json` declares.
    let mut sources: Vec<(PathBuf, Value)> = Vec::new();
    let default_mcp = root.join(".mcp.json");
    if let Some(doc) = load_json_object(&default_mcp, out) {
        sources.push((default_mcp.clone(), Value::Object(doc)));
    }
    match manifest.and_then(|m| m.get("mcpServers")) {
        Some(Value::String(s)) => {
            let p = resolve_declared(root, s);
            if p != default_mcp {
                if let Some(doc) = load_json_object(&p, out) {
                    sources.push((p, Value::Object(doc)));
                }
            }
        }
        Some(Value::Object(inline)) => {
            let mut wrapped = Map::new();
            wrapped.insert("mcpServers".to_string(), Value::Object(inline.clone()));
            sources.push((
                root.join(".claude-plugin").join("plugin.json"),
                Value::Object(wrapped),
            ));
        }
        _ => {}
    }
    for (file, doc) in sources {
        let Some((servers, wrapped)) = mcp_json::server_map(&doc) else {
            continue;
        };
        for (server_name, v) in servers {
            let parsed = match mcp_json::parse_server(server_name, v) {
                Ok(p) => p,
                Err(e) => {
                    out.warn(Some(&file), e);
                    continue;
                }
            };
            let disabled_in =
                state.projects_disabling(&mcp::plugin_server_key(plugin_name, server_name));
            let mut reasons = Vec::new();
            if !plugin_on {
                reasons.push(format!("plugin {id} is disabled"));
            }
            reasons.extend(mcp::disabled_in_projects_reason(&disabled_in));
            let reason = if reasons.is_empty() {
                None
            } else {
                Some(reasons.join("; "))
            };
            let enabled = EnabledState::toggleable(
                plugin_on && disabled_in.is_empty(),
                toggle.clone(),
                reason,
            );
            let mut server = mcp::build_server(
                server_name,
                scope.clone(),
                origin(
                    scope.clone(),
                    &file,
                    mcp_json::locator_for(server_name, wrapped),
                ),
                parsed,
                enabled,
                Some(id.to_string()),
                needs_auth.plugin_server(plugin_name, server_name),
            );
            mcp::note_disabled_in_projects(&mut server, &disabled_in);
            c.mcp_servers.push(server.id.clone());
            out.mcp_servers.push(server);
        }
    }
    let _ = ctx;
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_id_handles_missing_marketplace() {
        assert_eq!(
            split_id("linear@claude-plugins-official"),
            ("linear".into(), "claude-plugins-official".into())
        );
        assert_eq!(split_id("bare"), ("bare".into(), String::new()));
        assert_eq!(split_id("a@b@c"), ("a@b".into(), "c".into()));
    }

    #[test]
    fn declared_paths_accept_string_and_array() {
        let root = Path::new("/p");
        let m: Map<String, Value> =
            serde_json::from_str(r#"{"skills": "./custom", "agents": ["./a/x.md", "b"]}"#).unwrap();
        assert_eq!(
            declared_paths(Some(&m), "skills", root),
            vec![PathBuf::from("/p/custom")]
        );
        assert_eq!(
            declared_paths(Some(&m), "agents", root),
            vec![PathBuf::from("/p/a/x.md"), PathBuf::from("/p/b")]
        );
        assert!(declared_paths(Some(&m), "commands", root).is_empty());
        assert_eq!(
            contribution_paths(Some(&m), "skills", root, "skills"),
            vec![PathBuf::from("/p/skills"), PathBuf::from("/p/custom")]
        );
    }
}
