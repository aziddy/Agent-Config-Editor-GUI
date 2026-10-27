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
    Integer { min: i64, max: i64 },
    Notifications,
    EnvironmentFilters,
}

// Keep integers exact when they cross the JavaScript RPC boundary.
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
const POSITIVE: SettingKind = SettingKind::Integer {
    min: 1,
    max: MAX_SAFE_INTEGER,
};
const NON_NEGATIVE: SettingKind = SettingKind::Integer {
    min: 0,
    max: MAX_SAFE_INTEGER,
};

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
    (&["review_model"], SettingKind::Text),
    (&["model_context_window"], POSITIVE),
    (&["model_auto_compact_token_limit"], POSITIVE),
    (
        &["model_auto_compact_token_limit_scope"],
        SettingKind::Choice(&["total", "body_after_prefix"]),
    ),
    (&["tool_output_token_limit"], NON_NEGATIVE),
    (&["agents", "enabled"], SettingKind::Boolean),
    (&["agents", "max_concurrent_threads_per_session"], POSITIVE),
    (&["agents", "max_threads"], POSITIVE),
    (&["agents", "default_subagent_model"], SettingKind::Text),
    (
        &["agents", "default_subagent_reasoning_effort"],
        SettingKind::Text,
    ),
    (&["agents", "interrupt_message"], SettingKind::Boolean),
    (
        &["history", "persistence"],
        SettingKind::Choice(&["save-all", "none"]),
    ),
    (&["history", "max_bytes"], NON_NEGATIVE),
    (&["analytics", "enabled"], SettingKind::Boolean),
    (&["feedback", "enabled"], SettingKind::Boolean),
    (&["allow_login_shell"], SettingKind::Boolean),
    (
        &["shell_environment_policy", "inherit"],
        SettingKind::Choice(&["all", "core", "none"]),
    ),
    (
        &["shell_environment_policy", "ignore_default_excludes"],
        SettingKind::Boolean,
    ),
    (
        &["shell_environment_policy", "filters"],
        SettingKind::EnvironmentFilters,
    ),
    // Read legacy settings so forms can explain conflicts before a save.
    (
        &["shell_environment_policy", "exclude"],
        SettingKind::StringList,
    ),
    (
        &["shell_environment_policy", "include_only"],
        SettingKind::StringList,
    ),
    (&["project_doc_max_bytes"], NON_NEGATIVE),
    (&["project_doc_fallback_filenames"], SettingKind::StringList),
    (&["project_root_markers"], SettingKind::StringList),
    (
        &["skills", "max_context_tokens"],
        SettingKind::Integer { min: 1, max: 10000 },
    ),
    (
        &["memories", "disable_on_external_context"],
        SettingKind::Boolean,
    ),
    (
        &["memories", "max_unused_days"],
        SettingKind::Integer { min: 0, max: 365 },
    ),
    (
        &["memories", "max_rollout_age_days"],
        SettingKind::Integer { min: 0, max: 90 },
    ),
    (
        &["memories", "max_raw_memories_for_consolidation"],
        SettingKind::Integer { min: 0, max: 4096 },
    ),
    (
        &["memories", "max_rollouts_per_startup"],
        SettingKind::Integer { min: 0, max: 128 },
    ),
    (
        &["memories", "min_rollout_idle_hours"],
        SettingKind::Integer { min: 1, max: 48 },
    ),
    (
        &["memories", "min_rate_limit_remaining_percent"],
        SettingKind::Integer { min: 0, max: 100 },
    ),
    (&["memories", "extract_model"], SettingKind::Text),
    (&["memories", "consolidation_model"], SettingKind::Text),
    (&["tui", "notifications"], SettingKind::Notifications),
    (
        &["tui", "notification_method"],
        SettingKind::Choice(&["auto", "osc9", "bel"]),
    ),
    (
        &["tui", "notification_condition"],
        SettingKind::Choice(&["unfocused", "always"]),
    ),
    (&["tui", "vim_mode_default"], SettingKind::Boolean),
    (
        &["tui", "resume_cwd"],
        SettingKind::Choice(&["current", "session"]),
    ),
    (
        &["tui", "alternate_screen"],
        SettingKind::Choice(&["auto", "always", "never"]),
    ),
    (&["tui", "auto_recap"], SettingKind::Boolean),
    (&["tui", "show_tooltips"], SettingKind::Boolean),
    (
        &["file_opener"],
        SettingKind::Choice(&["vscode", "vscode-insiders", "windsurf", "cursor", "none"]),
    ),
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
            Self::Integer { .. } => value
                .as_i64()
                .is_some_and(|v| (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&v)),
            Self::Notifications => value.is_boolean() || Self::StringList.matches_shape(value),
            Self::EnvironmentFilters => value.as_object().is_some_and(|map| {
                map.iter().all(|(pattern, policy)| {
                    !pattern.trim().is_empty()
                        && matches!(policy.as_str(), Some("include" | "exclude"))
                })
            }),
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
        if let Self::Integer { min, max } = self {
            if !value.as_i64().is_some_and(|v| (min..=max).contains(&v)) {
                return Err(AppError::invalid(format!(
                    "setting must be a whole number from {min} to {max}"
                )));
            }
        }
        if matches!(self, Self::StringList | Self::Notifications)
            && value.as_array().is_some_and(|rows| {
                rows.iter()
                    .any(|v| v.as_str().is_none_or(|s| s.trim().is_empty()))
            })
        {
            return Err(AppError::invalid("setting list cannot contain empty rows"));
        }
        if let Self::EnvironmentFilters = self {
            let map = value.as_object().expect("validated object shape");
            let mut patterns = std::collections::HashSet::new();
            if map
                .keys()
                .any(|key| !patterns.insert(key.to_ascii_lowercase()))
            {
                return Err(AppError::invalid(
                    "environment filter patterns must be unique (case-insensitive)",
                ));
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
    } else if let Some(value) = item.as_integer() {
        Some(Value::from(value))
    } else if let Some(array) = item.as_array() {
        array
            .iter()
            .map(|value| value.as_str().map(|s| Value::String(s.into())))
            .collect::<Option<Vec<_>>>()
            .map(Value::Array)
    } else if let Some(table) = item.as_table_like() {
        table
            .iter()
            .map(|(key, value)| {
                value
                    .as_str()
                    .map(|v| (key.into(), Value::String(v.into())))
            })
            .collect::<Option<Map<_, _>>>()
            .map(Value::Object)
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
