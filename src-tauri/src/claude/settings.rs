//! Claude Code settings files.
//!
//! Precedence in Claude Code, highest first: managed → project `settings.local.json` →
//! project `settings.json` → user `settings.local.json` → user `settings.json`. Every file is
//! optional. Files are kept separately (not merged) so each entity can point at the exact file
//! and JSON pointer that governs it.
//!
//! Managed settings slot: `/Library/Application Support/ClaudeCode/managed-settings.json` on
//! macOS, `/etc/claude-code/managed-settings.json` on Linux. It is read when present and the
//! app is not running against an `ACE_HOME` sandbox (a sandbox must not leak host-wide policy
//! into a test home). Scope is [`Scope::Managed`].

use super::{load_json_object, origin, pointer_locator, project_scope};
use crate::model::{Locator, Origin, Scope};
use crate::scan::{ScanCtx, ScanOut};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

pub const MANAGED_SETTINGS_MACOS: &str =
    "/Library/Application Support/ClaudeCode/managed-settings.json";
pub const MANAGED_SETTINGS_LINUX: &str = "/etc/claude-code/managed-settings.json";

pub fn managed_settings_path() -> PathBuf {
    if cfg!(target_os = "macos") {
        PathBuf::from(MANAGED_SETTINGS_MACOS)
    } else {
        PathBuf::from(MANAGED_SETTINGS_LINUX)
    }
}

#[derive(Debug, Clone)]
pub struct SettingsFile {
    pub path: PathBuf,
    pub scope: Scope,
    /// `settings.local.json` rather than `settings.json`.
    pub local: bool,
    /// Always a JSON object.
    pub value: Value,
}

/// Every settings file that exists, most specific first (managed, projects, user local, user).
#[derive(Debug, Clone, Default)]
pub struct SettingsSet {
    pub files: Vec<SettingsFile>,
    user_dir: PathBuf,
    managed_path: PathBuf,
}

impl SettingsSet {
    /// Files that govern entities at `scope` (project files for a project, user files for User,
    /// the managed file for Managed), local before shared.
    pub fn files_for<'a>(&'a self, scope: &Scope) -> impl Iterator<Item = &'a SettingsFile> + 'a {
        let scope = scope.clone();
        self.files.iter().filter(move |f| f.scope == scope)
    }

    /// Where a new key for `scope` would be written when no existing file holds it.
    pub fn canonical_path(&self, scope: &Scope, local: bool) -> PathBuf {
        let name = if local {
            "settings.local.json"
        } else {
            "settings.json"
        };
        match scope {
            Scope::Project { root } => Path::new(root).join(".claude").join(name),
            Scope::Managed => self.managed_path.clone(),
            _ => self.user_dir.join(name),
        }
    }

    /// `enabledPlugins[id]` for `scope`, with the file holding it.
    pub fn plugin_enabled(&self, scope: &Scope, id: &str) -> Option<(bool, &SettingsFile)> {
        self.files_for(scope).find_map(|f| {
            enabled_plugins(&f.value)
                .into_iter()
                .find(|(k, _)| k == id)
                .map(|(_, v)| (v, f))
        })
    }

    /// Origin of the `enabledPlugins/<id>` toggle for `scope`: the file that already holds the key,
    /// else the canonical file for that scope.
    pub fn plugin_toggle_origin(&self, scope: &Scope, prefer_local: bool, id: &str) -> Origin {
        let path = match self.plugin_enabled(scope, id) {
            Some((_, f)) => f.path.clone(),
            None => self.canonical_path(scope, prefer_local),
        };
        origin(
            scope.clone(),
            &path,
            pointer_locator(&["enabledPlugins", id]),
        )
    }

    pub fn origin_in(&self, file: &SettingsFile, locator: Locator) -> Origin {
        origin(file.scope.clone(), &file.path, locator)
    }
}

/// Load user, managed and per-project settings files. `roots` are the existing project roots.
pub fn load_all(ctx: &ScanCtx<'_>, out: &mut ScanOut, roots: &[PathBuf]) -> SettingsSet {
    let mut set = SettingsSet {
        files: Vec::new(),
        user_dir: ctx.homes.claude_dir.clone(),
        managed_path: managed_settings_path(),
    };
    if !ctx.homes.sandbox {
        if let Some(f) = load_file(&set.managed_path, Scope::Managed, false, out) {
            set.files.push(f);
        }
    }
    for root in roots {
        let scope = project_scope(root);
        let dir = root.join(".claude");
        for (name, local) in [("settings.local.json", true), ("settings.json", false)] {
            if let Some(f) = load_file(&dir.join(name), scope.clone(), local, out) {
                set.files.push(f);
            }
        }
    }
    for (name, local) in [("settings.local.json", true), ("settings.json", false)] {
        if let Some(f) = load_file(&ctx.homes.claude_dir.join(name), Scope::User, local, out) {
            set.files.push(f);
        }
    }
    set
}

/// Load one settings file. `None` when missing or malformed (malformed warns).
pub fn load_file(
    path: &Path,
    scope: Scope,
    local: bool,
    out: &mut ScanOut,
) -> Option<SettingsFile> {
    let obj = load_json_object(path, out)?;
    Some(SettingsFile {
        path: path.to_path_buf(),
        scope,
        local,
        value: Value::Object(obj),
    })
}

/// `enabledPlugins: { "<name>@<marketplace>": bool }` in file order. Non-boolean values are skipped.
pub fn enabled_plugins(v: &Value) -> Vec<(String, bool)> {
    match v.get("enabledPlugins") {
        Some(Value::Object(m)) => m
            .iter()
            .filter_map(|(k, v)| v.as_bool().map(|b| (k.clone(), b)))
            .collect(),
        _ => Vec::new(),
    }
}

/// The `hooks` object: `{ "<Event>": [ { matcher, hooks: [ … ] } ] }`.
pub fn hooks(v: &Value) -> Option<&Map<String, Value>> {
    v.get("hooks").and_then(Value::as_object)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn enabled_plugins_skips_non_bools_and_keeps_order() {
        let v = json!({ "enabledPlugins": { "b@m": true, "a@m": false, "weird@m": "yes" } });
        assert_eq!(
            enabled_plugins(&v),
            vec![("b@m".to_string(), true), ("a@m".to_string(), false)]
        );
        assert!(enabled_plugins(&json!({})).is_empty());
        assert!(hooks(&json!({ "hooks": [] })).is_none());
    }

    #[test]
    fn canonical_paths_per_scope() {
        let set = SettingsSet {
            files: Vec::new(),
            user_dir: PathBuf::from("/h/.claude"),
            managed_path: PathBuf::from("/managed.json"),
        };
        assert_eq!(
            set.canonical_path(&Scope::User, false),
            PathBuf::from("/h/.claude/settings.json")
        );
        assert_eq!(
            set.canonical_path(&Scope::User, true),
            PathBuf::from("/h/.claude/settings.local.json")
        );
        assert_eq!(
            set.canonical_path(&Scope::Project { root: "/r".into() }, false),
            PathBuf::from("/r/.claude/settings.json")
        );
        assert_eq!(
            set.canonical_path(&Scope::Managed, false),
            PathBuf::from("/managed.json")
        );
    }
}
