//! Projection of the user config for the separate cards/forms view.
//! Values outside this projection are never rewritten by a form save.

use crate::error::{AppError, AppResult};
use crate::fsutil::{self, Homes, ReadMeta};
use crate::model::CodexSettings;
use serde_json::{Map, Value};
use std::path::PathBuf;
use toml_edit::{DocumentMut, Item};

#[derive(Clone, Copy)]
pub enum SettingKind {
    Text,
    Boolean,
    StringList,
    Choice(&'static [&'static str]),
}

pub const FIELDS: &[(&[&str], SettingKind)] = &[
    (&["model"], SettingKind::Text),
    (&["model_provider"], SettingKind::Text),
    (&["service_tier"], SettingKind::Text),
    (&["model_reasoning_effort"], SettingKind::Text),
    (&["plan_mode_reasoning_effort"], SettingKind::Text),
    (
        &["model_reasoning_summary"],
        SettingKind::Choice(&["auto", "concise", "detailed", "none"]),
    ),
    (
        &["model_verbosity"],
        SettingKind::Choice(&["low", "medium", "high"]),
    ),
    (
        &["approval_policy"],
        SettingKind::Choice(&["on-request", "never"]),
    ),
    (
        &["approvals_reviewer"],
        SettingKind::Choice(&["user", "auto_review"]),
    ),
    (
        &["sandbox_mode"],
        SettingKind::Choice(&["read-only", "workspace-write", "danger-full-access"]),
    ),
    (&["default_permissions"], SettingKind::Text),
    (
        &["sandbox_workspace_write", "network_access"],
        SettingKind::Boolean,
    ),
    (
        &["sandbox_workspace_write", "writable_roots"],
        SettingKind::StringList,
    ),
    (
        &["web_search"],
        SettingKind::Choice(&["disabled", "cached", "indexed", "live"]),
    ),
    (&["developer_instructions"], SettingKind::Text),
    (&["memories", "generate_memories"], SettingKind::Boolean),
    (&["memories", "use_memories"], SettingKind::Boolean),
    (&["tui", "status_line"], SettingKind::StringList),
    (&["tui", "theme"], SettingKind::Text),
    (&["tui", "animations"], SettingKind::Boolean),
    (&["notify"], SettingKind::StringList),
];

impl SettingKind {
    /// Existing custom choice strings remain visible without being coerced.
    pub fn matches_shape(self, value: &Value) -> bool {
        match self {
            Self::Text | Self::Choice(_) => value.is_string(),
            Self::Boolean => value.is_boolean(),
            Self::StringList => value
                .as_array()
                .is_some_and(|v| v.iter().all(Value::is_string)),
        }
    }

    pub fn validate(self, value: &Value) -> AppResult<()> {
        if !self.matches_shape(value) {
            return Err(AppError::invalid("setting value has an unsupported type"));
        }
        if let Self::Choice(choices) = self {
            if !choices.contains(&value.as_str().unwrap_or_default()) {
                return Err(AppError::invalid("setting value is not a supported option"));
            }
        }
        Ok(())
    }
}

pub fn setting_kind(path: &[String]) -> Option<SettingKind> {
    if path.len() == 2 && path[0] == "features" && !path[1].is_empty() {
        return Some(SettingKind::Boolean);
    }
    FIELDS
        .iter()
        .find(|(candidate, _)| {
            candidate.len() == path.len() && candidate.iter().zip(path).all(|(a, b)| a == b)
        })
        .map(|(_, kind)| *kind)
}

pub fn read_document(homes: &Homes) -> AppResult<(PathBuf, ReadMeta, DocumentMut)> {
    let path = homes.codex_home.join("config.toml");
    let meta = fsutil::read_text_with_meta(&path)?;
    let doc = meta.text.parse::<DocumentMut>().map_err(|e| {
        // A full TOML error includes source lines, which may contain credentials.
        AppError::parse(format!("Invalid TOML: {}", e.message()), &path)
    })?;
    Ok((path, meta, doc))
}

/// Distinguishes an absent key from a non-table ancestor that forms cannot edit.
pub fn item_at<'a>(doc: &'a DocumentMut, path: &[String]) -> AppResult<Option<&'a Item>> {
    let mut item = doc.as_item();
    for segment in path {
        let table = item.as_table_like().ok_or_else(|| {
            AppError::invalid("setting has a custom TOML parent; edit it in Codex Config")
        })?;
        match table.get(segment) {
            Some(next) => item = next,
            None => return Ok(None),
        }
    }
    Ok(Some(item))
}

pub fn simple_value(item: &Item) -> Option<Value> {
    if let Some(value) = item.as_str() {
        Some(Value::String(value.into()))
    } else if let Some(value) = item.as_bool() {
        Some(Value::Bool(value))
    } else if let Some(array) = item.as_array() {
        array
            .iter()
            .map(|value| value.as_str().map(|s| Value::String(s.into())))
            .collect::<Option<Vec<_>>>()
            .map(Value::Array)
    } else {
        None
    }
}

fn insert_value(root: &mut Map<String, Value>, path: &[String], value: Value) {
    let (first, rest) = path.split_first().expect("setting paths are non-empty");
    if rest.is_empty() {
        root.insert(first.clone(), value);
    } else {
        let entry = root
            .entry(first)
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(map) = entry {
            insert_value(map, rest, value);
        }
    }
}

pub fn read(homes: &Homes) -> AppResult<CodexSettings> {
    let (path, meta, doc) = read_document(homes)?;
    let mut values = Map::new();
    let mut unsupported_paths = Vec::new();
    for (segments, kind) in FIELDS {
        let path: Vec<String> = segments.iter().map(|s| (*s).into()).collect();
        match item_at(&doc, &path) {
            Ok(None) => {}
            Ok(Some(item)) => match simple_value(item).filter(|value| kind.matches_shape(value)) {
                Some(value) => insert_value(&mut values, &path, value),
                None => unsupported_paths.push(path),
            },
            Err(_) => unsupported_paths.push(path),
        }
    }
    if let Some(features) = doc.get("features") {
        if let Some(table) = features.as_table_like() {
            for (name, item) in table.iter() {
                let path = vec!["features".into(), name.into()];
                if let Some(value) = item.as_bool() {
                    insert_value(&mut values, &path, Value::Bool(value));
                } else {
                    unsupported_paths.push(path);
                }
            }
        } else {
            unsupported_paths.push(vec!["features".into()]);
        }
    }
    let names = |key: &str| -> Vec<String> {
        doc.get(key)
            .and_then(Item::as_table_like)
            .map(|table| table.iter().map(|(name, _)| name.into()).collect())
            .unwrap_or_default()
    };
    Ok(CodexSettings {
        path: fsutil::display(&path),
        hash: meta.hash,
        mode: meta.mode,
        values,
        provider_names: names("model_providers"),
        permission_profiles: names("permissions"),
        named_permissions: doc.contains_key("default_permissions"),
        unsupported_paths,
    })
}
