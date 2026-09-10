//! Codex subagents: `<codex_home>/agents/<name>.toml`.
//!
//! Flat TOML: `name`, `description`, `model`, `model_reasoning_effort`, a triple-quoted
//! `developer_instructions`, and optionally nested `[[skills.config]]`. Read-only in the UI.

use super::config_toml::{item_to_json, parse_doc};
use crate::fsutil::{self, read_text_opt};
use crate::markdown::frontmatter::preview;
use crate::model::{Agent, EntityKind, Frontmatter, Locator, Origin, Scope, SubAgent};
use crate::scan::{ScanCtx, ScanOut};
use serde_json::Map;
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::Item;

pub fn scan(ctx: &ScanCtx<'_>, out: &mut ScanOut) {
    let dir = ctx.homes.codex_home.join("agents");
    for path in toml_files(&dir) {
        if let Some(agent) = read_agent(&path, out) {
            out.subagents.push(agent);
        }
    }
}

fn toml_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("toml"))
        .collect();
    files.sort();
    files
}

pub fn read_agent(path: &Path, out: &mut ScanOut) -> Option<SubAgent> {
    let text = match read_text_opt(path) {
        Ok(Some(t)) => t,
        Ok(None) => return None,
        Err(e) => {
            out.warn(Some(path), e.message);
            return None;
        }
    };
    out.record_hash(path, &text);
    let doc = parse_doc(path, &text, out)?;
    let table = doc.as_table();

    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let get_str = |key: &str| table.get(key).and_then(Item::as_str).map(String::from);

    // Top-level values (scalars, arrays, inline tables) as the "frontmatter"; sub-tables such as
    // `[[skills.config]]` are left out.
    let fields: Map<String, serde_json::Value> = table
        .iter()
        .filter(|(_, item)| item.is_value())
        .map(|(k, item)| (k.to_string(), item_to_json(item)))
        .collect();

    let path_str = fsutil::display(path);
    Some(SubAgent {
        id: format!("codex:{}:{}", EntityKind::SubAgent.key(), path_str),
        name: get_str("name")
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(stem),
        agent: Agent::Codex,
        scope: Scope::User,
        origin: Origin {
            agent: Agent::Codex,
            scope: Scope::User,
            file: path_str,
            locator: Locator::TomlPath { path: Vec::new() },
        },
        description: get_str("description"),
        model: get_str("model"),
        tools: Vec::new(),
        color: None,
        effort: get_str("model_reasoning_effort"),
        frontmatter: Frontmatter {
            fields,
            raw: text.clone(),
            present: true,
        },
        body_preview: preview(&get_str("developer_instructions").unwrap_or_default(), 160),
        content_hash: fsutil::sha256_hex(text.as_bytes()),
        dup_group: None,
    })
}
