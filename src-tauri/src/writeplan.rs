//! Two-phase writes: build a [`WritePlan`] (new text + unified diff) from a mutation,
//! then apply it with a freshness check, backup and atomic rename.

use crate::error::{AppError, AppResult, ErrorCode};
use crate::fsutil::{self, atomic_write, display, read_text_opt, sha256_hex, BackupStore, Homes};
use crate::markdown::frontmatter;
use crate::model::{ApplyResult, ExtraFile, FrontmatterChange, Mutation, WritePlan};
use serde_json::{Map, Value};
use similar::TextDiff;
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table};

// ---------------------------------------------------------------------------
// Building plans
// ---------------------------------------------------------------------------

pub struct PlanInput<'a> {
    pub homes: &'a Homes,
    pub path: &'a Path,
    pub mutation: Mutation,
    pub description: String,
    /// Hash the client saw; checked against disk when given.
    pub base_hash: Option<String>,
    /// Text for `RawText` / `CreateFile`.
    pub text: Option<String>,
    pub mode: Option<u32>,
    pub extra_files: Vec<ExtraFile>,
}

pub fn build(input: PlanInput<'_>) -> AppResult<WritePlan> {
    let path = input.path;
    input.homes.check_writable(path)?;
    let current = read_text_opt(path)?;
    let current_hash = current.as_deref().map(|t| sha256_hex(t.as_bytes()));
    if let (Some(base), Some(now)) = (&input.base_hash, &current_hash) {
        if base != now {
            return Err(AppError::external_change(path));
        }
    }
    let mut warnings = Vec::new();
    let new_text = match &input.mutation {
        Mutation::DeleteFile => None,
        Mutation::RawText | Mutation::CreateFile => {
            if matches!(input.mutation, Mutation::CreateFile) && current.is_some() {
                return Err(AppError::conflict(format!(
                    "{} already exists",
                    path.display()
                )));
            }
            Some(
                input
                    .text
                    .clone()
                    .ok_or_else(|| AppError::invalid("text is required for this mutation"))?,
            )
        }
        m => {
            let base = current.clone().unwrap_or_default();
            Some(apply_mutation(&base, m, path, &mut warnings)?)
        }
    };
    let mode = input
        .mode
        .or_else(|| fs::metadata(path).ok().map(|m| fsutil::mode_of(&m)));
    let unified_diff = unified_diff(
        path,
        current.as_deref().unwrap_or(""),
        new_text.as_deref().unwrap_or(""),
    );
    if current.as_deref() == new_text.as_deref() && !matches!(input.mutation, Mutation::DeleteFile)
    {
        warnings.push("no changes".to_string());
    }
    Ok(WritePlan {
        path: display(path),
        base_hash: current_hash,
        new_text,
        unified_diff,
        mode,
        warnings,
        extra_files: input.extra_files,
        mutation: input.mutation,
        description: input.description,
    })
}

pub fn unified_diff(path: &Path, old: &str, new: &str) -> String {
    let name = display(path);
    TextDiff::from_lines(old, new)
        .unified_diff()
        .context_radius(3)
        .header(&format!("a/{name}"), &format!("b/{name}"))
        .to_string()
}

/// Produce the new file text for a structured mutation. Pure.
pub fn apply_mutation(
    base: &str,
    mutation: &Mutation,
    path: &Path,
    warnings: &mut Vec<String>,
) -> AppResult<String> {
    match mutation {
        Mutation::JsonSet { pointer, value } => {
            let mut root = parse_json(base, path)?;
            json_set(&mut root, pointer, value.clone())?;
            Ok(serialize_json(&root, base))
        }
        Mutation::JsonDelete { pointer } => {
            let mut root = parse_json(base, path)?;
            json_delete(&mut root, pointer);
            Ok(serialize_json(&root, base))
        }
        Mutation::TomlSet { path: tpath, value } => {
            let mut doc = parse_toml(base, path)?;
            toml_set(&mut doc, tpath, value)?;
            Ok(doc.to_string())
        }
        Mutation::TomlDelete { path: tpath } => {
            let mut doc = parse_toml(base, path)?;
            toml_delete(&mut doc, tpath);
            Ok(doc.to_string())
        }
        Mutation::TomlSkillsConfig {
            skill_path,
            skill_name,
            enabled,
        } => {
            let mut doc = parse_toml(base, path)?;
            toml_skills_config(&mut doc, skill_path, skill_name.as_deref(), *enabled);
            Ok(doc.to_string())
        }
        Mutation::TomlAutomation { patch } => {
            let mut doc = parse_toml(base, path)?;
            toml_automation_patch(&mut doc, patch)?;
            Ok(doc.to_string())
        }
        Mutation::Frontmatter { changes, body } => {
            frontmatter_apply(base, changes, body.as_deref(), warnings)
        }
        Mutation::RawText | Mutation::CreateFile | Mutation::DeleteFile => Err(AppError::invalid(
            "mutation does not derive text from the current file",
        )),
    }
}

fn parse_json(text: &str, path: &Path) -> AppResult<Value> {
    if text.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    serde_json::from_str(text).map_err(|e| AppError::parse(format!("invalid JSON: {e}"), path))
}

fn serialize_json(root: &Value, base: &str) -> String {
    let mut out = serde_json::to_string_pretty(root).unwrap_or_else(|_| "{}".to_string());
    if base.is_empty() || base.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn parse_toml(text: &str, path: &Path) -> AppResult<DocumentMut> {
    text.parse::<DocumentMut>()
        .map_err(|e| AppError::parse(format!("invalid TOML: {e}"), path))
}

// ---------------------------------------------------------------------------
// JSON pointer helpers (RFC 6901)
// ---------------------------------------------------------------------------

pub fn escape_segment(seg: &str) -> String {
    seg.replace('~', "~0").replace('/', "~1")
}

pub fn pointer(segments: &[&str]) -> String {
    let mut s = String::new();
    for seg in segments {
        s.push('/');
        s.push_str(&escape_segment(seg));
    }
    s
}

fn split_pointer(pointer: &str) -> AppResult<Vec<String>> {
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    if !pointer.starts_with('/') {
        return Err(AppError::invalid(format!(
            "JSON pointer must start with '/': {pointer}"
        )));
    }
    Ok(pointer[1..]
        .split('/')
        .map(|s| s.replace("~1", "/").replace("~0", "~"))
        .collect())
}

pub fn json_get<'a>(root: &'a Value, pointer: &str) -> Option<&'a Value> {
    root.pointer(pointer)
}

/// Set `value` at `pointer`, creating intermediate objects. Array indices are honoured for
/// existing arrays; `-` appends.
pub fn json_set(root: &mut Value, pointer: &str, value: Value) -> AppResult<()> {
    let segs = split_pointer(pointer)?;
    if segs.is_empty() {
        *root = value;
        return Ok(());
    }
    let mut cur = root;
    for (i, seg) in segs.iter().enumerate() {
        let last = i == segs.len() - 1;
        match cur {
            Value::Object(map) => {
                if last {
                    map.insert(seg.clone(), value);
                    return Ok(());
                }
                cur = map
                    .entry(seg.clone())
                    .or_insert_with(|| Value::Object(Map::new()));
            }
            Value::Array(arr) => {
                if seg == "-" {
                    if last {
                        arr.push(value);
                        return Ok(());
                    }
                    return Err(AppError::invalid(
                        "cannot descend through '-' in JSON pointer",
                    ));
                }
                let idx: usize = seg.parse().map_err(|_| {
                    AppError::invalid(format!("bad array index '{seg}' in {pointer}"))
                })?;
                if idx >= arr.len() {
                    return Err(AppError::invalid(format!(
                        "array index {idx} out of range in {pointer}"
                    )));
                }
                if last {
                    arr[idx] = value;
                    return Ok(());
                }
                cur = &mut arr[idx];
            }
            other => {
                *other = Value::Object(Map::new());
                if let Value::Object(map) = other {
                    if last {
                        map.insert(seg.clone(), value);
                        return Ok(());
                    }
                    cur = map
                        .entry(seg.clone())
                        .or_insert_with(|| Value::Object(Map::new()));
                } else {
                    unreachable!()
                }
            }
        }
    }
    Ok(())
}

/// Remove the value at `pointer`. Returns whether something was removed.
pub fn json_delete(root: &mut Value, pointer: &str) -> bool {
    let Ok(segs) = split_pointer(pointer) else {
        return false;
    };
    let Some((last, parents)) = segs.split_last() else {
        return false;
    };
    let parent_ptr = pointer_from_owned(parents);
    match root.pointer_mut(&parent_ptr) {
        Some(Value::Object(map)) => map.remove(last).is_some(),
        Some(Value::Array(arr)) => match last.parse::<usize>() {
            Ok(i) if i < arr.len() => {
                arr.remove(i);
                true
            }
            _ => false,
        },
        _ => false,
    }
}

fn pointer_from_owned(segs: &[String]) -> String {
    let refs: Vec<&str> = segs.iter().map(String::as_str).collect();
    pointer(&refs)
}

/// Add `item` to the string array at `pointer` (creating it) unless present.
pub fn json_array_add(root: &mut Value, pointer: &str, item: &str) -> AppResult<bool> {
    let existing = json_get(root, pointer)
        .cloned()
        .unwrap_or(Value::Array(Vec::new()));
    let mut arr = match existing {
        Value::Array(a) => a,
        _ => return Err(AppError::invalid(format!("{pointer} is not an array"))),
    };
    if arr.iter().any(|v| v.as_str() == Some(item)) {
        return Ok(false);
    }
    arr.push(Value::String(item.to_string()));
    json_set(root, pointer, Value::Array(arr))?;
    Ok(true)
}

/// Remove `item` from the string array at `pointer`; drops the key when it becomes empty.
pub fn json_array_remove(root: &mut Value, pointer: &str, item: &str) -> AppResult<bool> {
    let Some(Value::Array(existing)) = json_get(root, pointer).cloned() else {
        return Ok(false);
    };
    let before = existing.len();
    let arr: Vec<Value> = existing
        .into_iter()
        .filter(|v| v.as_str() != Some(item))
        .collect();
    if arr.len() == before {
        return Ok(false);
    }
    if arr.is_empty() {
        json_delete(root, pointer);
    } else {
        json_set(root, pointer, Value::Array(arr))?;
    }
    Ok(true)
}

// ---------------------------------------------------------------------------
// TOML helpers (toml_edit, lossless)
// ---------------------------------------------------------------------------

/// Escape for a TOML basic (single-line, double-quoted) string.
pub fn escape_basic(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\u{:04X}", c as u32))
            }
            c => out.push(c),
        }
    }
    out
}

/// A string value that always renders as a single-line basic string (`"…\n…"`), matching
/// what Codex writes for `prompt`. toml_edit would otherwise pick `"""` for newlines.
pub fn single_line_string(s: &str) -> toml_edit::Value {
    let snippet = format!("v = \"{}\"\n", escape_basic(s));
    let doc: DocumentMut = snippet.parse().expect("escaped string is valid TOML");
    let mut v = doc["v"].as_value().expect("value").clone();
    v.decor_mut().clear();
    v
}

pub fn json_to_toml_value(v: &Value) -> AppResult<toml_edit::Value> {
    Ok(match v {
        Value::String(s) => single_line_string(s),
        Value::Bool(b) => toml_edit::Value::from(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml_edit::Value::from(i)
            } else if let Some(f) = n.as_f64() {
                // Codex writes integral timeouts as integers (`startup_timeout_sec = 120`).
                if f.fract() == 0.0 && f.abs() < 9.0e15 {
                    toml_edit::Value::from(f as i64)
                } else {
                    toml_edit::Value::from(f)
                }
            } else {
                return Err(AppError::invalid(format!("unsupported number {n}")));
            }
        }
        Value::Array(items) => {
            let mut arr = Array::new();
            for i in items {
                arr.push(json_to_toml_value(i)?);
            }
            toml_edit::Value::Array(arr)
        }
        Value::Object(map) => {
            let mut t = InlineTable::new();
            for (k, val) in map {
                t.insert(k, json_to_toml_value(val)?);
            }
            toml_edit::Value::InlineTable(t)
        }
        Value::Null => return Err(AppError::invalid("null cannot be written to TOML")),
    })
}

/// Objects become standard (non-inline) tables — the `[mcp_servers.x.env]` convention.
pub fn json_to_toml_item(v: &Value) -> AppResult<Item> {
    match v {
        Value::Object(map) => {
            let mut t = Table::new();
            t.set_implicit(false);
            for (k, val) in map {
                if val.is_null() {
                    continue;
                }
                t.insert(k, json_to_toml_item(val)?);
            }
            Ok(Item::Table(t))
        }
        Value::Array(items) if items.iter().all(|i| i.is_object()) && !items.is_empty() => {
            let mut aot = ArrayOfTables::new();
            for i in items {
                if let Item::Table(t) = json_to_toml_item(i)? {
                    aot.push(t);
                }
            }
            Ok(Item::ArrayOfTables(aot))
        }
        other => Ok(Item::Value(json_to_toml_value(other)?)),
    }
}

fn toml_navigate_mut<'a>(
    doc: &'a mut DocumentMut,
    path: &[String],
    create: bool,
) -> Option<&'a mut Item> {
    let mut cur: &mut Item = doc.as_item_mut();
    for seg in path {
        let table_like = cur.as_table_like_mut()?;
        if table_like.get(seg).is_none() {
            if !create {
                return None;
            }
            let mut t = Table::new();
            t.set_implicit(true);
            table_like.insert(seg, Item::Table(t));
        }
        cur = table_like.get_mut(seg)?;
    }
    Some(cur)
}

/// Set a value or table at `path`. Existing scalar values are replaced in place so their
/// surrounding decor survives; new tables are appended after their siblings.
pub fn toml_set(doc: &mut DocumentMut, path: &[String], value: &Value) -> AppResult<()> {
    let Some((last, parents)) = path.split_last() else {
        return Err(AppError::invalid("TOML path must not be empty"));
    };
    let parent = toml_navigate_mut(doc, parents, true).ok_or_else(|| {
        AppError::invalid(format!("cannot create TOML path {}", parents.join(".")))
    })?;
    let table = parent
        .as_table_like_mut()
        .ok_or_else(|| AppError::invalid(format!("{} is not a table", parents.join("."))))?;
    match (table.get_mut(last), value) {
        (Some(existing), v) if existing.is_value() && !v.is_object() && !v.is_array() => {
            // Replace the scalar but keep the existing decor (comments / spacing).
            let new_val = json_to_toml_value(v)?;
            if let Some(old) = existing.as_value_mut() {
                let decor = old.decor().clone();
                *old = new_val;
                *old.decor_mut() = decor;
            }
        }
        (Some(existing), Value::Object(map)) if existing.is_table_like() => {
            // Merge object keys into the existing table, one level at a time.
            for (k, v) in map {
                let mut sub = parents.to_vec();
                sub.push(last.clone());
                sub.push(k.clone());
                if v.is_null() {
                    toml_delete(doc, &sub);
                } else {
                    toml_set(doc, &sub, v)?;
                }
            }
            return Ok(());
        }
        _ => {
            let item = json_to_toml_item(value)?;
            table.insert(last, item);
            if let Some(Item::Table(t)) = table.get_mut(last) {
                t.set_implicit(false);
            }
        }
    }
    // Parents created on the way become explicit only if they hold scalar values.
    Ok(())
}

pub fn toml_delete(doc: &mut DocumentMut, path: &[String]) -> bool {
    let Some((last, parents)) = path.split_last() else {
        return false;
    };
    let Some(parent) = toml_navigate_mut(doc, parents, false) else {
        return false;
    };
    let Some(table) = parent.as_table_like_mut() else {
        return false;
    };
    table.remove(last).is_some()
}

pub fn toml_get<'a>(doc: &'a DocumentMut, path: &[String]) -> Option<&'a Item> {
    let mut cur: &Item = doc.as_item();
    for seg in path {
        cur = cur.as_table_like()?.get(seg)?;
    }
    Some(cur)
}

/// Codex `[[skills.config]]` deny-list. Disabling adds/updates an entry keyed by `path`;
/// enabling removes every entry that matches by path or name.
pub fn toml_skills_config(
    doc: &mut DocumentMut,
    skill_path: &str,
    skill_name: Option<&str>,
    enabled: bool,
) {
    let matches = |t: &Table| {
        t.get("path").and_then(Item::as_str) == Some(skill_path)
            || (skill_name.is_some() && t.get("name").and_then(Item::as_str) == skill_name)
    };
    if enabled {
        let mut removed_any = false;
        if let Some(Item::ArrayOfTables(aot)) =
            doc.get_mut("skills").and_then(|s| s.get_mut("config"))
        {
            let mut i = 0;
            while i < aot.len() {
                if aot.get(i).is_some_and(matches) {
                    aot.remove(i);
                    removed_any = true;
                } else {
                    i += 1;
                }
            }
        }
        if removed_any {
            let empty = doc
                .get("skills")
                .and_then(|s| s.get("config"))
                .and_then(Item::as_array_of_tables)
                .is_some_and(|a| a.is_empty());
            if empty {
                if let Some(skills) = doc.get_mut("skills").and_then(Item::as_table_like_mut) {
                    skills.remove("config");
                    if skills.is_empty() {
                        doc.remove("skills");
                    }
                }
            }
        }
        return;
    }
    // Disable: update an existing matching entry or append a new path-keyed one.
    let skills = doc.entry("skills").or_insert({
        let mut t = Table::new();
        t.set_implicit(true);
        Item::Table(t)
    });
    let Some(skills_table) = skills.as_table_like_mut() else {
        return;
    };
    let config = skills_table
        .entry("config")
        .or_insert(Item::ArrayOfTables(ArrayOfTables::new()));
    let Some(aot) = config.as_array_of_tables_mut() else {
        return;
    };
    for t in aot.iter_mut() {
        if matches(t) {
            t["enabled"] = toml_edit::value(false);
            return;
        }
    }
    let mut t = Table::new();
    t.insert("path", Item::Value(single_line_string(skill_path)));
    t.insert("enabled", toml_edit::value(false));
    aot.push(t);
}

/// Patch a flat `automation.toml`: set the given keys in place, bump `updated_at`.
pub fn toml_automation_patch(doc: &mut DocumentMut, patch: &Value) -> AppResult<()> {
    let Value::Object(map) = patch else {
        return Err(AppError::invalid("automation patch must be an object"));
    };
    for (key, v) in map {
        if key == "target" || key == "id" || key == "version" || key == "created_at" {
            return Err(AppError::invalid(format!(
                "automation field `{key}` is read-only"
            )));
        }
        if v.is_null() {
            doc.remove(key);
            continue;
        }
        toml_set(doc, std::slice::from_ref(key), v)?;
    }
    let now = chrono::Utc::now().timestamp_millis();
    toml_set(doc, &["updated_at".to_string()], &Value::from(now))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Frontmatter: targeted key splicing
// ---------------------------------------------------------------------------

fn yaml_scalar(s: &str) -> String {
    let needs_quotes = s.is_empty()
        || s != s.trim()
        || s.contains(": ")
        || s.contains(" #")
        || s.starts_with(|c: char| "-?:,[]{}#&*!|>'\"%@`".contains(c))
        || s.ends_with(':')
        || matches!(
            s.to_ascii_lowercase().as_str(),
            "true" | "false" | "yes" | "no" | "null" | "~" | "on" | "off"
        )
        || s.parse::<f64>().is_ok();
    if needs_quotes {
        serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\""))
    } else {
        s.to_string()
    }
}

/// Render `key: value` YAML for one top-level key, ending with a newline.
pub fn yaml_entry(key: &str, value: &Value) -> String {
    match value {
        Value::String(s) if s.contains('\n') => {
            let trimmed = s.trim_end_matches('\n');
            let chomp = if s.ends_with('\n') { "" } else { "-" };
            let body: String = trimmed.lines().map(|l| format!("  {l}\n")).collect();
            format!("{key}: |{chomp}\n{body}")
        }
        Value::String(s) => format!("{key}: {}\n", yaml_scalar(s)),
        Value::Bool(b) => format!("{key}: {b}\n"),
        Value::Number(n) => format!("{key}: {n}\n"),
        Value::Null => format!("{key}: null\n"),
        Value::Array(items) if items.iter().all(|i| i.is_string()) => {
            let mut out = format!("{key}:\n");
            for i in items {
                out.push_str(&format!("  - {}\n", yaml_scalar(i.as_str().unwrap_or(""))));
            }
            out
        }
        other => {
            let inner = serde_yaml_ng::to_string(other).unwrap_or_default();
            let indented: String = inner.lines().map(|l| format!("  {l}\n")).collect();
            format!("{key}:\n{indented}")
        }
    }
}

/// Byte spans of each top-level key inside a frontmatter block.
fn top_level_spans(raw: &str) -> Vec<(String, usize, usize)> {
    let mut spans: Vec<(String, usize, usize)> = Vec::new();
    let mut offset = 0usize;
    for line in raw.split_inclusive('\n') {
        let is_key = !line.starts_with([' ', '\t', '#', '-'])
            && line.find(':').is_some_and(|i| {
                i > 0
                    && line[..i]
                        .chars()
                        .all(|c| c.is_alphanumeric() || "_-.".contains(c))
            });
        if is_key {
            let key = line[..line.find(':').unwrap()].to_string();
            if let Some(last) = spans.last_mut() {
                last.2 = offset;
            }
            spans.push((key, offset, raw.len()));
        }
        offset += line.len();
    }
    spans
}

pub fn frontmatter_apply(
    base: &str,
    changes: &[FrontmatterChange],
    body: Option<&str>,
    warnings: &mut Vec<String>,
) -> AppResult<String> {
    let parsed = frontmatter::parse(base);
    let mut raw = if parsed.frontmatter.present {
        parsed.frontmatter.raw.clone()
    } else {
        String::new()
    };
    if !raw.is_empty() && !raw.ends_with('\n') {
        raw.push('\n');
    }
    for change in changes {
        let spans = top_level_spans(&raw);
        let existing = spans.iter().find(|(k, _, _)| k == &change.key).cloned();
        match (&change.value, existing) {
            (None, Some((_, start, end))) => raw.replace_range(start..end, ""),
            (None, None) => {}
            (Some(v), Some((_, start, end))) => {
                raw.replace_range(start..end, &yaml_entry(&change.key, v))
            }
            (Some(v), None) => raw.push_str(&yaml_entry(&change.key, v)),
        }
    }
    // Verify the spliced YAML parses and carries the intended values.
    let verified = match serde_yaml_ng::from_str::<Value>(&raw) {
        Ok(Value::Object(map)) => changes
            .iter()
            .all(|c| map.get(&c.key).cloned() == c.value.clone()),
        Ok(Value::Null) => changes.iter().all(|c| c.value.is_none()),
        _ => false,
    };
    // Files whose original block is not strict YAML (Claude Code tolerates e.g. unquoted
    // `description: … Context: …`) can only be edited textually: a wholesale re-emit from the
    // parsed fields would drop the keys the strict parser could not read.
    let original_strict = !parsed.frontmatter.present
        || parsed.frontmatter.raw.trim().is_empty()
        || matches!(
            serde_yaml_ng::from_str::<Value>(&parsed.frontmatter.raw),
            Ok(Value::Object(_)) | Ok(Value::Null)
        );
    if !verified && !original_strict {
        warnings.push(
            "frontmatter is not strict YAML; the edit was spliced textually and could not be verified by re-parsing"
                .to_string(),
        );
    } else if !verified {
        warnings.push(
            "frontmatter was re-emitted wholesale because a targeted edit did not round-trip"
                .to_string(),
        );
        let mut fields = parsed.frontmatter.fields.clone();
        for c in changes {
            match &c.value {
                Some(v) => {
                    fields.insert(c.key.clone(), v.clone());
                }
                None => {
                    fields.remove(&c.key);
                }
            }
        }
        raw = serde_yaml_ng::to_string(&Value::Object(fields))
            .map_err(|e| AppError::invalid(e.to_string()))?;
    }
    let body_text = body.unwrap_or(parsed.body.as_str());
    let raw_trimmed = raw.trim_end_matches('\n');
    if raw_trimmed.is_empty() {
        return Ok(body_text.to_string());
    }
    Ok(format!("---\n{raw_trimmed}\n---\n{body_text}"))
}

// ---------------------------------------------------------------------------
// Applying plans
// ---------------------------------------------------------------------------

pub struct ApplyCtx<'a> {
    pub homes: &'a Homes,
    pub backups: &'a BackupStore,
    pub on_written: &'a dyn Fn(&Path),
}

pub fn apply(ctx: &ApplyCtx<'_>, plan: &WritePlan) -> AppResult<ApplyResult> {
    let path = PathBuf::from(&plan.path);
    ctx.homes.check_writable(&path)?;
    guard_protected(ctx.homes, &path, &plan.mutation)?;

    let current = read_text_opt(&path)?;
    let current_hash = current.as_deref().map(|t| sha256_hex(t.as_bytes()));

    let new_text: Option<String> = if current_hash == plan.base_hash {
        plan.new_text.clone()
    } else {
        // Someone else wrote the file since the preview. Structured mutations can be
        // rebased onto the fresh content; raw edits cannot.
        match &plan.mutation {
            Mutation::RawText | Mutation::CreateFile | Mutation::DeleteFile => {
                return Err(AppError::external_change(&path));
            }
            m => {
                let mut warnings = Vec::new();
                Some(apply_mutation(
                    current.as_deref().unwrap_or(""),
                    m,
                    &path,
                    &mut warnings,
                )?)
            }
        }
    };

    let backup_path = ctx.backups.backup(&path)?;
    match new_text {
        None => {
            if path.is_dir() {
                for entry in walkdir::WalkDir::new(&path).into_iter().flatten() {
                    if entry.file_type().is_file() {
                        ctx.backups.backup(entry.path())?;
                    }
                }
                fs::remove_dir_all(&path).map_err(|e| AppError::io(e, &path))?;
            } else if path.exists() {
                fs::remove_file(&path).map_err(|e| AppError::io(e, &path))?;
            }
            (ctx.on_written)(&path);
            Ok(ApplyResult {
                path: plan.path.clone(),
                new_hash: None,
                backup_path: backup_path.map(|p| display(&p)),
            })
        }
        Some(text) => {
            for extra in &plan.extra_files {
                let ep = PathBuf::from(&extra.path);
                ctx.homes.check_writable(&ep)?;
                if !ep.exists() {
                    atomic_write(
                        &ep,
                        &extra.text,
                        extra.mode.or_else(|| default_mode(ctx.homes, &ep)),
                    )?;
                    (ctx.on_written)(&ep);
                }
            }
            let mode = plan.mode.or_else(|| default_mode(ctx.homes, &path));
            atomic_write(&path, &text, mode)?;
            (ctx.on_written)(&path);
            Ok(ApplyResult {
                path: plan.path.clone(),
                new_hash: Some(sha256_hex(text.as_bytes())),
                backup_path: backup_path.map(|p| display(&p)),
            })
        }
    }
}

fn default_mode(homes: &Homes, path: &Path) -> Option<u32> {
    // Codex keeps config.toml and auth.json private; everything else it writes is 0644.
    let private = path.parent() == Some(homes.codex_home.as_path())
        && matches!(
            path.file_name().and_then(|n| n.to_str()),
            Some("config.toml") | Some("auth.json")
        );
    Some(if private { 0o600 } else { 0o644 })
}

/// Refuse writes that would corrupt vendor-managed state.
pub fn guard_protected(homes: &Homes, path: &Path, mutation: &Mutation) -> AppResult<()> {
    let p = display(path);
    let deleting = matches!(mutation, Mutation::DeleteFile);
    if p.contains("/plugins/cache/") && deleting {
        return Err(AppError::read_only(
            "plugin cache directories are managed by the agent; disable the plugin instead",
            path,
        ));
    }
    if path.starts_with(homes.codex_home.join("skills").join(".system")) && deleting {
        return Err(AppError::read_only(
            "Codex system skills cannot be deleted",
            path,
        ));
    }
    if deleting && path.starts_with(homes.agents_dir.join("skills")) {
        let lock = homes.agents_dir.join(".skill-lock.json");
        if let Ok(Some(text)) = read_text_opt(&lock) {
            if let Ok(Value::Object(root)) = serde_json::from_str::<Value>(&text) {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if root
                    .get("skills")
                    .and_then(|s| s.as_object())
                    .is_some_and(|m| m.contains_key(name))
                {
                    return Err(AppError::read_only(
                        "this skill is managed by an installer (.skill-lock.json); remove it with that tool",
                        path,
                    ));
                }
            }
        }
    }
    if path.is_dir() && deleting {
        let count = walkdir::WalkDir::new(path).into_iter().flatten().count();
        if count > 200 {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("refusing to delete a directory with {count} entries"),
            )
            .with_path(path));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const CODEX_FIXTURE: &str = r#"model = "gpt-6"

# cmux-codex-hooks-feature-abc begin
[features]
hooks = true
# cmux-codex-hooks-feature-abc end

[mcp_servers.node_repl]
command = "/Applications/x/node_repl"
args = []
startup_timeout_sec = 120
[mcp_servers.node_repl.env]
CODEX_HOME = "/Users/me/.codex"
TRUSTED = '{"a":1}'

[mcp_servers.docs]
url = "https://example.com/mcp"

[[skills.config]]
name = "workos"
enabled = false

[tui.keymap.chat]

[tui.model_availability_nux]
"gpt-5.5" = 4
gpt-6-astra = 4
"#;

    fn doc() -> DocumentMut {
        CODEX_FIXTURE.parse().unwrap()
    }

    #[test]
    fn toml_round_trip_is_byte_identical() {
        assert_eq!(doc().to_string(), CODEX_FIXTURE);
    }

    #[test]
    fn toml_set_scalar_in_place_keeps_everything_else() {
        let mut d = doc();
        toml_set(
            &mut d,
            &["mcp_servers".into(), "docs".into(), "enabled".into()],
            &Value::Bool(false),
        )
        .unwrap();
        let out = d.to_string();
        assert!(out.contains("url = \"https://example.com/mcp\"\nenabled = false\n"));
        assert!(out.contains("# cmux-codex-hooks-feature-abc end"));
        assert!(out.contains("[tui.keymap.chat]\n"));
        assert!(out.contains("TRUSTED = '{\"a\":1}'"));
        // Flip it back to true in place.
        toml_set(
            &mut d,
            &["mcp_servers".into(), "docs".into(), "enabled".into()],
            &Value::Bool(true),
        )
        .unwrap();
        assert!(d.to_string().contains("enabled = true\n"));
    }

    #[test]
    fn toml_add_server_appends_after_siblings_with_env_subtable() {
        let mut d = doc();
        let server = serde_json::json!({
            "command": "npx", "args": ["chrome-devtools-mcp@latest"], "env": {"FOO": "bar"}
        });
        toml_set(&mut d, &["mcp_servers".into(), "chrome".into()], &server).unwrap();
        let out = d.to_string();
        let pos_docs = out.find("[mcp_servers.docs]").unwrap();
        let pos_new = out.find("[mcp_servers.chrome]").unwrap();
        let pos_skills = out.find("[[skills.config]]").unwrap();
        assert!(
            pos_docs < pos_new && pos_new < pos_skills,
            "new table sits after last sibling:\n{out}"
        );
        assert!(
            out.contains("[mcp_servers.chrome.env]\nFOO = \"bar\"\n"),
            "env is a sub-table:\n{out}"
        );
        assert!(
            out.contains("args = [\"chrome-devtools-mcp@latest\"]"),
            "array rendering:\n{out}"
        );
        assert!(
            !out.contains("\n[mcp_servers]\n"),
            "no explicit empty parent header"
        );
    }

    #[test]
    fn toml_delete_server_removes_env_too() {
        let mut d = doc();
        assert!(toml_delete(
            &mut d,
            &["mcp_servers".into(), "node_repl".into()]
        ));
        let out = d.to_string();
        assert!(!out.contains("node_repl"));
        assert!(out.contains("[mcp_servers.docs]"));
    }

    #[test]
    fn skills_config_toggle_by_path_and_name() {
        let mut d = doc();
        toml_skills_config(
            &mut d,
            "/Users/me/.agents/skills/yaak/SKILL.md",
            Some("use-yaak"),
            false,
        );
        let out = d.to_string();
        assert_eq!(out.matches("[[skills.config]]").count(), 2);
        assert!(out.contains("path = \"/Users/me/.agents/skills/yaak/SKILL.md\"\nenabled = false"));
        // Enable removes the path entry only.
        toml_skills_config(
            &mut d,
            "/Users/me/.agents/skills/yaak/SKILL.md",
            Some("use-yaak"),
            true,
        );
        assert_eq!(d.to_string().matches("[[skills.config]]").count(), 1);
        // Enabling by name removes the remaining name entry and the now-empty skills table.
        toml_skills_config(&mut d, "/nowhere/SKILL.md", Some("workos"), true);
        let out = d.to_string();
        assert!(!out.contains("skills"), "{out}");
        assert!(out.contains("[tui.keymap.chat]"));
    }

    #[test]
    fn automation_prompt_stays_single_line_and_bumps_updated_at() {
        let src = "version = 1\nid = \"x\"\nname = \"Old\"\nprompt = \"a\\nb\"\nstatus = \"ACTIVE\"\ntarget = { type = \"projectless\" }\ncreated_at = 1\nupdated_at = 2\n";
        let mut d: DocumentMut = src.parse().unwrap();
        let patch = serde_json::json!({ "prompt": "line1\nline2 \"q\"", "status": "PAUSED", "name": "New 🚀" });
        toml_automation_patch(&mut d, &patch).unwrap();
        let out = d.to_string();
        assert!(
            out.contains("prompt = \"line1\\nline2 \\\"q\\\"\"\n"),
            "{out}"
        );
        assert!(!out.contains("\"\"\""));
        assert!(out.contains("status = \"PAUSED\""));
        assert!(out.contains("name = \"New 🚀\""));
        assert!(out.contains("target = { type = \"projectless\" }"));
        assert!(!out.contains("updated_at = 2\n"));
        let keys: Vec<&str> = out.lines().filter_map(|l| l.split(" = ").next()).collect();
        assert_eq!(
            keys,
            vec![
                "version",
                "id",
                "name",
                "prompt",
                "status",
                "target",
                "created_at",
                "updated_at"
            ]
        );
        assert!(toml_automation_patch(&mut d, &serde_json::json!({ "target": {} })).is_err());
    }

    #[test]
    fn json_pointer_set_delete_and_arrays() {
        let mut root =
            serde_json::json!({ "projects": { "/a/b": { "lastCost": 1.5 } }, "mcpServers": {} });
        let p = pointer(&["projects", "/a/b", "disabledMcpServers"]);
        assert_eq!(p, "/projects/~1a~1b/disabledMcpServers");
        assert!(json_array_add(&mut root, &p, "workos").unwrap());
        assert!(!json_array_add(&mut root, &p, "workos").unwrap());
        json_set(
            &mut root,
            "/mcpServers/x",
            serde_json::json!({ "type": "http", "url": "u" }),
        )
        .unwrap();
        assert_eq!(
            root["projects"]["/a/b"]["disabledMcpServers"],
            serde_json::json!(["workos"])
        );
        assert!(json_array_remove(&mut root, &p, "workos").unwrap());
        assert!(
            root["projects"]["/a/b"].get("disabledMcpServers").is_none(),
            "empty array key dropped"
        );
        assert_eq!(root["projects"]["/a/b"]["lastCost"], 1.5);
        assert!(json_delete(&mut root, "/mcpServers/x"));
        assert!(!json_delete(&mut root, "/mcpServers/x"));
        let text = serialize_json(&root, "{}\n");
        assert!(text.ends_with('\n'));
        assert!(
            text.starts_with("{\n  \"projects\""),
            "2-space pretty, order preserved: {text}"
        );
    }

    #[test]
    fn frontmatter_targeted_splice_keeps_untouched_bytes() {
        let src = "---\nname: use-yaak\ndescription: >\n  Build and run HTTP\n  requests.\nallowed-tools: Bash(yaak:*), Bash(which:*)\n---\n# Body\n";
        let mut w = Vec::new();
        let out = frontmatter_apply(
            src,
            &[
                FrontmatterChange {
                    key: "allowed-tools".into(),
                    value: Some(serde_json::json!(["Bash(yaak:*)"])),
                },
                FrontmatterChange {
                    key: "disable-model-invocation".into(),
                    value: Some(Value::Bool(true)),
                },
            ],
            None,
            &mut w,
        )
        .unwrap();
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(
            out,
            "---\nname: use-yaak\ndescription: >\n  Build and run HTTP\n  requests.\nallowed-tools:\n  - Bash(yaak:*)\ndisable-model-invocation: true\n---\n# Body\n"
        );
        // Deleting a key and replacing the body.
        let out2 = frontmatter_apply(
            &out,
            &[FrontmatterChange {
                key: "description".into(),
                value: None,
            }],
            Some("new body\n"),
            &mut w,
        )
        .unwrap();
        assert_eq!(
            out2,
            "---\nname: use-yaak\nallowed-tools:\n  - Bash(yaak:*)\ndisable-model-invocation: true\n---\nnew body\n"
        );
    }

    #[test]
    fn frontmatter_created_when_absent_and_multiline_uses_block_scalar() {
        let mut w = Vec::new();
        let out = frontmatter_apply(
            "# Just a command\n",
            &[FrontmatterChange {
                key: "description".into(),
                value: Some(Value::String("Line one\nLine two".into())),
            }],
            None,
            &mut w,
        )
        .unwrap();
        assert_eq!(
            out,
            "---\ndescription: |-\n  Line one\n  Line two\n---\n# Just a command\n"
        );
        let reparsed = frontmatter::parse(&out);
        assert_eq!(
            frontmatter::field_str(&reparsed.frontmatter, "description").as_deref(),
            Some("Line one\nLine two")
        );
    }

    #[test]
    fn frontmatter_double_quoted_scalar_untouched_by_unrelated_edit() {
        let src = "---\nname: ui-tweaker\ndescription: \"Use this agent\\n\\n<example>\\nctx\\n</example>\"\nmodel: sonnet\n---\nbody\n";
        let mut w = Vec::new();
        let out = frontmatter_apply(
            src,
            &[FrontmatterChange {
                key: "model".into(),
                value: Some(Value::String("opus".into())),
            }],
            None,
            &mut w,
        )
        .unwrap();
        assert_eq!(
            out,
            "---\nname: ui-tweaker\ndescription: \"Use this agent\\n\\n<example>\\nctx\\n</example>\"\nmodel: opus\n---\nbody\n"
        );
    }

    #[test]
    fn lenient_frontmatter_is_spliced_not_reemitted() {
        // Strict YAML rejects the unquoted `Context:` inside the description; Claude Code accepts it.
        let src = "---\nname: ui-tweaker\ndescription: Use this agent. Context: the user asks for UI changes\nmodel: sonnet\ncolor: green\n---\nbody\n";
        let mut w = Vec::new();
        let out = frontmatter_apply(
            src,
            &[FrontmatterChange {
                key: "model".into(),
                value: Some(Value::String("opus".into())),
            }],
            None,
            &mut w,
        )
        .unwrap();
        assert_eq!(
            out,
            "---\nname: ui-tweaker\ndescription: Use this agent. Context: the user asks for UI changes\nmodel: opus\ncolor: green\n---\nbody\n"
        );
        assert_eq!(
            w.len(),
            1,
            "warns that verification was textual only: {w:?}"
        );
        assert!(out.contains("color: green"), "no keys dropped");
    }

    #[test]
    fn build_and_apply_with_external_change_detection() {
        let dir = tempfile::tempdir().unwrap();
        let homes = Homes::for_test(dir.path());
        let path = dir.path().join(".codex").join("config.toml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, CODEX_FIXTURE).unwrap();
        let plan = build(PlanInput {
            homes: &homes,
            path: &path,
            mutation: Mutation::TomlSet {
                path: vec!["mcp_servers".into(), "docs".into(), "enabled".into()],
                value: Value::Bool(false),
            },
            description: "disable docs".into(),
            base_hash: None,
            text: None,
            mode: None,
            extra_files: vec![],
        })
        .unwrap();
        assert!(plan.unified_diff.contains("+enabled = false"));
        // Simulate another writer touching an unrelated key: structured mutation rebases.
        fs::write(
            &path,
            CODEX_FIXTURE.replace("model = \"gpt-6\"", "model = \"gpt-7\""),
        )
        .unwrap();
        let backups = BackupStore::new(dir.path().join("bk"), 5);
        let written = std::cell::RefCell::new(Vec::new());
        let ctx = ApplyCtx {
            homes: &homes,
            backups: &backups,
            on_written: &|p| written.borrow_mut().push(p.to_path_buf()),
        };
        let res = apply(&ctx, &plan).unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert!(after.contains("model = \"gpt-7\"") && after.contains("enabled = false"));
        assert!(res.backup_path.is_some());
        assert_eq!(written.borrow().len(), 1);
        #[cfg(unix)]
        assert_eq!(
            Some(fsutil::mode_of(&fs::metadata(&path).unwrap())),
            plan.mode,
            "mode kept from original"
        );
        // A raw-text plan built against stale content is refused.
        let raw = build(PlanInput {
            homes: &homes,
            path: &path,
            mutation: Mutation::RawText,
            description: "raw".into(),
            base_hash: None,
            text: Some("x = 1\n".into()),
            mode: None,
            extra_files: vec![],
        })
        .unwrap();
        fs::write(&path, "y = 2\n").unwrap();
        let err = apply(&ctx, &raw).unwrap_err();
        assert_eq!(err.code, ErrorCode::ExternalChange);
    }
}
