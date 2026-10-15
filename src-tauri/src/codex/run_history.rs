//! Indexed history reader with an incremental JSONL fallback for older Codex releases.

use super::automation_runs::{app_databases, columns, db_error, open_read_only, PAGE_SIZE};
use super::run_activity::{self, item_kind, label};
use crate::error::{AppError, AppResult};
use crate::fsutil::Homes;
use crate::model::{
    AutomationRun, AutomationRunActivity, AutomationRunConversation, RunConversationItem,
    RunItemKind,
};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

#[derive(Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "camelCase")]
enum ItemRef {
    History { turn: String, item: String },
    Log { offset: u64 },
    Saved { role: String },
}

fn reference(item: ItemRef) -> String {
    serde_json::to_string(&item).expect("item references contain only strings/numbers")
}

fn versioned_databases(homes: &Homes, prefix: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for root in [&homes.codex_home, &homes.codex_home.join("sqlite")] {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        let mut versions: Vec<_> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name();
                let version = name
                    .to_str()?
                    .strip_prefix(prefix)?
                    .strip_suffix(".sqlite")?
                    .parse::<u32>()
                    .ok()?;
                entry.path().is_file().then_some((version, entry.path()))
            })
            .collect();
        versions.sort_by_key(|entry| std::cmp::Reverse(entry.0));
        paths.extend(versions.into_iter().map(|(_, path)| path));
    }
    paths
}

fn history_database(
    homes: &Homes,
    thread: &str,
    notices: &mut Vec<String>,
) -> Option<(PathBuf, Connection)> {
    for path in versioned_databases(homes, "thread_history_") {
        let read = || -> AppResult<Option<Connection>> {
            let conn = open_read_only(&path)?;
            let names = columns(&conn, "thread_items").map_err(|e| db_error(e, &path))?;
            if ![
                "thread_id",
                "turn_id",
                "item_id",
                "item_type",
                "item_json",
                "rollout_ordinal",
                "created_at_ms",
            ]
            .iter()
            .all(|name| names.contains(*name))
            {
                return Err(AppError::parse(
                    "This indexed Codex conversation schema is not supported",
                    &path,
                ));
            }
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM thread_items WHERE thread_id=?1)",
                    [thread],
                    |row| row.get(0),
                )
                .map_err(|e| db_error(e, &path))?;
            Ok(exists.then_some(conn))
        };
        match read() {
            Ok(Some(conn)) => return Some((path, conn)),
            Ok(None) => {}
            Err(error) => notices.push(error.to_string()),
        }
    }
    None
}

fn allowed_rollout(homes: &Homes, path: &Path) -> Option<PathBuf> {
    let root = homes.codex_home.canonicalize().ok()?;
    let path = path.canonicalize().ok()?;
    (path.is_file()
        && (path.starts_with(root.join("sessions"))
            || path.starts_with(root.join("archived_sessions"))))
    .then_some(path)
}

fn rollout_path(homes: &Homes, thread: &str) -> Option<PathBuf> {
    for path in versioned_databases(homes, "state_") {
        let Ok(conn) = open_read_only(&path) else {
            continue;
        };
        let value: rusqlite::Result<Option<String>> = conn
            .query_row(
                "SELECT rollout_path FROM threads WHERE id=?1",
                [thread],
                |row| row.get(0),
            )
            .optional();
        if let Ok(Some(raw)) = value {
            let path = PathBuf::from(raw);
            let path = if path.is_absolute() {
                path
            } else {
                homes.codex_home.join(path)
            };
            if let Some(path) = allowed_rollout(homes, &path) {
                return Some(path);
            }
        }
    }
    // Pending placeholders have no conversation, and must not trigger a recursive file search.
    if thread.is_empty()
        || !thread
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    let suffix = format!("-{thread}.jsonl");
    for directory in ["sessions", "archived_sessions"] {
        for entry in walkdir::WalkDir::new(homes.codex_home.join(directory))
            .max_depth(5)
            .into_iter()
            .filter_map(Result::ok)
        {
            if entry.file_type().is_file() && entry.file_name().to_string_lossy().ends_with(&suffix)
            {
                if let Some(path) = allowed_rollout(homes, entry.path()) {
                    return Some(path);
                }
            }
        }
    }
    None
}

fn visible_sql() -> &'static str {
    "lower(replace(item_type, '_', '')) NOT IN ('reasoning', 'contextcompaction', 'compaction')"
}

struct DbItem {
    turn: String,
    id: String,
    item_type: String,
    position: i64,
    changed: i64,
    created_at: Option<i64>,
}

fn indexed_body(
    conn: &Connection,
    path: &Path,
    thread: &str,
    turn: &str,
    item: &str,
) -> AppResult<Value> {
    let raw: String = conn
        .query_row(
            "SELECT item_json FROM thread_items WHERE thread_id=?1 AND turn_id=?2 AND item_id=?3",
            [thread, turn, item],
            |row| row.get(0),
        )
        .map_err(|e| db_error(e, path))?;
    serde_json::from_str(&raw)
        .map_err(|_| AppError::parse("A recorded conversation item could not be read", path))
}

fn preview(
    value: &Value,
    id: String,
    position: i64,
    created_at: Option<i64>,
) -> AppResult<Option<RunConversationItem>> {
    let Some(kind) = item_kind(value) else {
        return Ok(None);
    };
    let (text, next_cursor) = if kind == RunItemKind::Activity {
        (None, None)
    } else {
        let (_, text) = run_activity::body(value);
        let chunk = run_activity::chunk(json!({}), &text, None)?;
        if chunk.text.is_empty() {
            return Ok(None);
        }
        (Some(chunk.text), chunk.next_cursor)
    };
    Ok(Some(RunConversationItem {
        id,
        position,
        version: position,
        kind,
        label: label(
            value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )
        .to_string(),
        text,
        phase: value
            .get("phase")
            .and_then(Value::as_str)
            .map(str::to_owned),
        created_at,
        next_cursor,
    }))
}

fn indexed_page(
    conn: &Connection,
    path: &Path,
    run: AutomationRun,
    before: Option<i64>,
    after: Option<i64>,
    rollout: Option<&Path>,
    mut notices: Vec<String>,
) -> AppResult<AutomationRunConversation> {
    let names = columns(conn, "thread_items").map_err(|e| db_error(e, path))?;
    let change = if names.contains("updated_at_ordinal") {
        "max(rollout_ordinal, updated_at_ordinal)"
    } else {
        "rollout_ordinal"
    };
    let filter = if before.is_some() {
        "AND rollout_ordinal < ?2"
    } else if after.is_some() {
        "AND changed > ?2"
    } else {
        ""
    };
    let order = if after.is_some() {
        "changed ASC, rollout_ordinal ASC"
    } else {
        "rollout_ordinal DESC"
    };
    let sql = format!("SELECT turn_id, item_id, item_type, rollout_ordinal, {change} AS changed, created_at_ms FROM thread_items WHERE thread_id=?1 AND {} {filter} ORDER BY {order} LIMIT {}", visible_sql(), PAGE_SIZE + 1);
    let mut rows = {
        let mut statement = conn.prepare(&sql).map_err(|e| db_error(e, path))?;
        let map = |row: &rusqlite::Row<'_>| -> rusqlite::Result<DbItem> {
            Ok(DbItem {
                turn: row.get(0)?,
                id: row.get(1)?,
                item_type: row.get(2)?,
                position: row.get(3)?,
                changed: row.get(4)?,
                created_at: row.get(5)?,
            })
        };
        let result = if let Some(cursor) = before.or(after) {
            statement.query_map(rusqlite::params![run.thread_id, cursor], map)
        } else {
            statement.query_map([&run.thread_id], map)
        }
        .map_err(|e| db_error(e, path))?;
        result
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| db_error(e, path))?
    };
    let more = rows.len() > PAGE_SIZE;
    rows.truncate(PAGE_SIZE);
    let after_cursor = rows
        .iter()
        .map(|row| row.changed)
        .max()
        .unwrap_or(after.unwrap_or(-1));
    let before_cursor = (more && after.is_none())
        .then(|| rows.iter().map(|row| row.position).min())
        .flatten();
    let mut items = Vec::new();
    for row in rows {
        let id = reference(ItemRef::History {
            turn: row.turn.clone(),
            item: row.id.clone(),
        });
        let kind = item_kind(&json!({"type":row.item_type}));
        if kind == Some(RunItemKind::Activity) {
            items.push(RunConversationItem {
                id,
                position: row.position,
                version: row.changed,
                kind: RunItemKind::Activity,
                label: label(&row.item_type).to_string(),
                text: None,
                phase: None,
                created_at: row.created_at,
                next_cursor: None,
            });
        } else {
            match indexed_body(conn, path, &run.thread_id, &row.turn, &row.id)
                .and_then(|body| preview(&body, id, row.position, row.created_at))
            {
                Ok(Some(mut item)) => {
                    item.version = row.changed;
                    items.push(item);
                }
                Ok(None) => {}
                Err(_) => notices.push("A recorded message could not be displayed.".to_string()),
            }
        }
    }
    // Early paginated histories omitted the initial user item. Recover the recorded prompt,
    // never the potentially edited automation.toml prompt.
    if before_cursor.is_none() && after.is_none() {
        let has_user: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM thread_items WHERE thread_id=?1 AND lower(item_type)='usermessage')", [&run.thread_id], |row| row.get(0)).map_err(|e| db_error(e, path))?;
        if !has_user {
            if let Some(item) = rollout.and_then(|path| initial_user(path).ok().flatten()) {
                items.push(item);
            }
        }
    }
    items.sort_by_key(|item| item.position);
    let turn: Option<(String, Option<i64>)> = conn.query_row("SELECT status, duration_ms FROM thread_turns WHERE thread_id=?1 ORDER BY rollout_ordinal DESC LIMIT 1", [&run.thread_id], |row| Ok((row.get(0)?, row.get(1)?))).optional().unwrap_or_default();
    let (execution_status, duration_ms) = turn
        .map(|(status, duration)| (Some(status), duration))
        .unwrap_or_default();
    Ok(AutomationRunConversation {
        run,
        items,
        before_cursor,
        after_cursor,
        has_more_after: more && after.is_some(),
        availability: "local".to_string(),
        notices,
        execution_status,
        duration_ms,
        history_source: format!("history:{}", path.display()),
        removed_item_ids: Vec::new(),
        reset: false,
    })
}

fn recorded_item(record: &Value) -> Option<(Value, bool)> {
    let payload = record.get("payload")?;
    match record.get("type").and_then(Value::as_str)? {
        "event_msg" => {
            if payload.get("type").and_then(Value::as_str) == Some("item_completed") {
                Some((payload.get("item")?.clone(), true))
            } else {
                run_activity::legacy_event(payload).map(|item| (item, true))
            }
        }
        "response_item" => run_activity::protocol_item(payload).map(|item| (item, false)),
        _ => None,
    }
}

fn read_log_item(path: &Path, offset: u64) -> AppResult<Value> {
    let file = File::open(path).map_err(|e| AppError::io(e, path))?;
    let mut reader = BufReader::new(file);
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|e| AppError::io(e, path))?;
    let mut line = Vec::new();
    reader
        .read_until(b'\n', &mut line)
        .map_err(|e| AppError::io(e, path))?;
    let record = serde_json::from_slice(&line)
        .map_err(|_| AppError::parse("This conversation item changed; refresh the run", path))?;
    recorded_item(&record)
        .map(|(item, _)| item)
        .ok_or_else(|| AppError::invalid("This record is not visible conversation activity"))
}

fn initial_user(path: &Path) -> AppResult<Option<RunConversationItem>> {
    let file = File::open(path).map_err(|e| AppError::io(e, path))?;
    let mut reader = BufReader::new(file);
    let mut offset = 0;
    let mut line = Vec::new();
    for _ in 0..64 {
        line.clear();
        let length = reader
            .read_until(b'\n', &mut line)
            .map_err(|e| AppError::io(e, path))?;
        if length == 0 {
            break;
        }
        if let Ok(record) = serde_json::from_slice::<Value>(&line) {
            if let Some((item, _)) = recorded_item(&record) {
                if item_kind(&item) == Some(RunItemKind::Assistant) {
                    break;
                }
                if item_kind(&item) == Some(RunItemKind::User) {
                    let item = preview(&item, reference(ItemRef::Log { offset }), -1, None)?;
                    if item.is_some() {
                        return Ok(item);
                    }
                }
            }
        }
        offset += length as u64;
    }
    Ok(None)
}

#[derive(Clone)]
struct LogEntry {
    item: RunConversationItem,
    offset: u64,
    changed: i64,
    turn: String,
    canonical: bool,
    dedupe: Option<String>,
}

#[derive(Clone, Default)]
struct LogIndex {
    scanned: u64,
    file_size: u64,
    modified: Option<SystemTime>,
    entries: Vec<LogEntry>,
    current_turn: String,
    malformed: usize,
    execution_status: Option<String>,
    duration_ms: Option<i64>,
    generation: u64,
    file_id: u64,
}

static LOG_CACHE: OnceLock<Mutex<HashMap<PathBuf, LogIndex>>> = OnceLock::new();

fn log_index(path: &Path) -> AppResult<LogIndex> {
    let metadata = fs::metadata(path).map_err(|e| AppError::io(e, path))?;
    let mut cache = LOG_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| AppError::conflict("Conversation cache is unavailable"))?;
    // Bound cache growth when browsing many automations. Eviction affects only read performance.
    if cache.len() >= 8 && !cache.contains_key(path) {
        if let Some(old) = cache
            .iter()
            .min_by_key(|(_, index)| index.modified)
            .map(|(path, _)| path.clone())
        {
            cache.remove(&old);
        }
    }
    let index = cache.entry(path.to_path_buf()).or_default();
    let modified = metadata.modified().ok();
    #[cfg(unix)]
    let file_id = {
        use std::os::unix::fs::MetadataExt;
        metadata.ino()
    };
    #[cfg(not(unix))]
    let file_id = 0;
    if metadata.len() < index.file_size
        || (index.modified.is_some() && file_id != index.file_id)
        || (metadata.len() == index.file_size && modified != index.modified)
    {
        let generation = index.generation + 1;
        *index = LogIndex {
            generation,
            ..LogIndex::default()
        };
    }
    if index.file_size == metadata.len() && index.modified == modified {
        return Ok(index.clone());
    }
    let file = File::open(path).map_err(|e| AppError::io(e, path))?;
    let mut reader = BufReader::new(file);
    reader
        .seek(SeekFrom::Start(index.scanned))
        .map_err(|e| AppError::io(e, path))?;
    let mut line = Vec::new();
    let mut ids: HashMap<String, usize> = index
        .entries
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| entry.dedupe.as_ref().map(|id| (id.clone(), i)))
        .collect();
    loop {
        line.clear();
        let offset = index.scanned;
        let length = reader
            .read_until(b'\n', &mut line)
            .map_err(|e| AppError::io(e, path))?;
        if length == 0 || line.last() != Some(&b'\n') {
            break;
        }
        index.scanned += length as u64;
        let record: Value = match serde_json::from_slice(&line) {
            Ok(record) => record,
            Err(_) => {
                index.malformed += 1;
                continue;
            }
        };
        let payload = record.get("payload").unwrap_or(&Value::Null);
        if record.get("type").and_then(Value::as_str) == Some("event_msg") {
            match payload.get("type").and_then(Value::as_str) {
                Some("task_started") => {
                    index.current_turn = payload
                        .get("turn_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    index.execution_status = Some("inProgress".to_string());
                    index.duration_ms = None;
                }
                Some("task_complete") => {
                    index.execution_status = Some("completed".to_string());
                    index.duration_ms = payload.get("duration_ms").and_then(Value::as_i64);
                }
                Some("turn_aborted") => {
                    index.execution_status = Some("interrupted".to_string());
                    index.duration_ms = payload.get("duration_ms").and_then(Value::as_i64);
                }
                _ => {}
            }
        }
        let Some((value, canonical)) = recorded_item(&record) else {
            continue;
        };
        let turn = payload
            .get("turn_id")
            .and_then(Value::as_str)
            .unwrap_or(&index.current_turn)
            .to_string();
        let created_at = record
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.timestamp_millis());
        let Some(mut item) = preview(
            &value,
            reference(ItemRef::Log { offset }),
            offset as i64,
            created_at,
        )?
        else {
            continue;
        };
        // Completed UI records can replace the same item after a streaming update.
        let dedupe = value
            .get("id")
            .and_then(Value::as_str)
            .map(|id| format!("{turn}:{canonical}:{}:{id}", run_activity::type_key(&value)));
        if let Some(existing) = dedupe.as_ref().and_then(|id| ids.get(id)).copied() {
            item.id = index.entries[existing].item.id.clone();
            item.position = index.entries[existing].item.position;
            index.entries[existing] = LogEntry {
                item,
                offset,
                changed: offset as i64,
                turn,
                canonical,
                dedupe,
            };
        } else {
            if let Some(id) = &dedupe {
                ids.insert(id.clone(), index.entries.len());
            }
            index.entries.push(LogEntry {
                item,
                offset,
                changed: offset as i64,
                turn,
                canonical,
                dedupe,
            });
        }
    }
    index.file_size = metadata.len();
    index.modified = modified;
    index.file_id = file_id;
    Ok(index.clone())
}

fn log_page(
    path: &Path,
    run: AutomationRun,
    before: Option<i64>,
    after: Option<i64>,
    mut notices: Vec<String>,
) -> AppResult<AutomationRunConversation> {
    let index = log_index(path)?;
    if after.is_some_and(|after| after >= index.scanned as i64) {
        let mut page = log_page(path, run, None, None, notices)?;
        page.reset = true;
        return Ok(page);
    }
    if index.malformed > 0 {
        notices.push(format!(
            "Skipped {} unreadable log record(s).",
            index.malformed
        ));
    }
    let mut canonical: HashMap<(String, u8), i64> = HashMap::new();
    let kind_key = |kind: RunItemKind| match kind {
        RunItemKind::User => 0,
        RunItemKind::Assistant => 1,
        RunItemKind::Activity => 2,
    };
    for entry in &index.entries {
        if entry.canonical {
            canonical
                .entry((entry.turn.clone(), kind_key(entry.item.kind)))
                .and_modify(|last| *last = (*last).max(entry.changed))
                .or_insert(entry.changed);
        }
    }
    let removed_item_ids = index
        .entries
        .iter()
        .filter(|entry| {
            !entry.canonical
                && canonical
                    .get(&(entry.turn.clone(), kind_key(entry.item.kind)))
                    .is_some_and(|last| *last >= entry.changed)
        })
        .map(|entry| entry.item.id.clone())
        .collect();
    let mut entries: Vec<_> = index
        .entries
        .iter()
        .filter(|entry| {
            // Current logs contain both protocol and completed UI events. Keep legacy events
            // in older turns and in-flight protocol items until their UI record is completed.
            (entry.canonical
                || canonical
                    .get(&(entry.turn.clone(), kind_key(entry.item.kind)))
                    .is_none_or(|last| entry.changed > *last))
                && before.is_none_or(|before| entry.item.position < before)
                && after.is_none_or(|after| entry.changed > after)
        })
        .collect();
    if after.is_some() {
        entries.sort_by_key(|entry| entry.changed);
    } else {
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.item.position));
    }
    let more = entries.len() > PAGE_SIZE;
    entries.truncate(PAGE_SIZE);
    let after_cursor = entries
        .iter()
        .map(|entry| entry.changed)
        .max()
        .unwrap_or(after.unwrap_or(-1));
    let before_cursor = (more && after.is_none())
        .then(|| entries.iter().map(|entry| entry.item.position).min())
        .flatten();
    let mut items: Vec<_> = entries.iter().map(|entry| entry.item.clone()).collect();
    items.sort_by_key(|item| item.position);
    Ok(AutomationRunConversation {
        run,
        items,
        before_cursor,
        after_cursor,
        has_more_after: more && after.is_some(),
        availability: "local".to_string(),
        notices,
        execution_status: index.execution_status,
        duration_ms: index.duration_ms,
        history_source: format!(
            "log:{}:{}:{}",
            path.display(),
            index.file_id,
            index.generation
        ),
        removed_item_ids,
        reset: false,
    })
}

fn saved_messages(
    homes: &Homes,
    identity: &str,
    thread: &str,
) -> Option<(Option<String>, Option<String>)> {
    let mut result = None;
    let mut newest = i64::MIN;
    for path in app_databases(homes) {
        let Ok(conn) = open_read_only(&path) else {
            continue;
        };
        let Ok(names) = columns(&conn, "automation_runs") else {
            continue;
        };
        if !names.contains("archived_user_message") || !names.contains("archived_assistant_message")
        {
            continue;
        }
        let row: rusqlite::Result<(Option<String>, Option<String>, i64)> = conn.query_row("SELECT archived_user_message, archived_assistant_message, updated_at FROM automation_runs WHERE automation_id=?1 AND thread_id=?2", [identity, thread], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)));
        if let Ok((user, assistant, updated)) = row {
            if updated > newest && (user.is_some() || assistant.is_some()) {
                newest = updated;
                result = Some((user, assistant));
            }
        }
    }
    result
}

fn saved_page(
    homes: &Homes,
    identity: &str,
    run: AutomationRun,
    mut notices: Vec<String>,
) -> AppResult<AutomationRunConversation> {
    let mut items = Vec::new();
    if let Some((user, assistant)) = saved_messages(homes, identity, &run.thread_id) {
        for (position, (role, text)) in [("user", user), ("assistant", assistant)]
            .into_iter()
            .enumerate()
        {
            if let Some(text) = text {
                let value = json!({"type":if role == "user" { "userMessage" } else { "agentMessage" }, "text":text, "phase":"final_answer"});
                if let Some(item) = preview(
                    &value,
                    reference(ItemRef::Saved {
                        role: role.to_string(),
                    }),
                    position as i64,
                    None,
                )? {
                    items.push(item);
                }
            }
        }
    }
    let availability = if items.is_empty() {
        "unavailable"
    } else {
        "saved"
    };
    notices.push(if items.is_empty() {
        "The full conversation is not stored locally for this run."
    } else { "Showing saved messages; the full conversation and tool activity are not available locally." }.to_string());
    Ok(AutomationRunConversation {
        run,
        items,
        before_cursor: None,
        after_cursor: -1,
        has_more_after: false,
        availability: availability.to_string(),
        notices,
        execution_status: None,
        duration_ms: None,
        history_source: "saved".to_string(),
        removed_item_ids: Vec::new(),
        reset: false,
    })
}

pub(super) fn conversation(
    homes: &Homes,
    identity: &str,
    run: AutomationRun,
    before: Option<i64>,
    after: Option<i64>,
) -> AppResult<AutomationRunConversation> {
    let mut notices = Vec::new();
    let rollout = rollout_path(homes, &run.thread_id);
    if let Some((path, conn)) = history_database(homes, &run.thread_id, &mut notices) {
        match indexed_page(
            &conn,
            &path,
            run.clone(),
            before,
            after,
            rollout.as_deref(),
            notices.clone(),
        ) {
            Ok(page) => return Ok(page),
            Err(error) => notices.push(error.to_string()),
        }
    }
    if let Some(path) = rollout {
        return log_page(&path, run, before, after, notices);
    }
    saved_page(homes, identity, run, notices)
}

pub(super) fn activity(
    homes: &Homes,
    identity: &str,
    run: &AutomationRun,
    item_id: &str,
    cursor: Option<&str>,
) -> AppResult<AutomationRunActivity> {
    let item: ItemRef = serde_json::from_str(item_id)
        .map_err(|_| AppError::invalid("Invalid conversation item"))?;
    let value = match item {
        ItemRef::History { turn, item } => {
            let (_, conn) =
                history_database(homes, &run.thread_id, &mut Vec::new()).ok_or_else(|| {
                    AppError::invalid("The indexed conversation is no longer available")
                })?;
            indexed_body(&conn, &homes.codex_home, &run.thread_id, &turn, &item)?
        }
        ItemRef::Log { offset } => {
            let path = rollout_path(homes, &run.thread_id)
                .ok_or_else(|| AppError::invalid("The conversation log is no longer available"))?;
            // Index resolution preserves the original id when an append updates an item.
            let cached_offset = LOG_CACHE
                .get()
                .and_then(|cache| cache.lock().ok())
                .and_then(|cache| {
                    cache.get(&path).and_then(|index| {
                        index
                            .entries
                            .iter()
                            .find(|entry| entry.item.id == item_id)
                            .map(|entry| entry.offset)
                    })
                });
            read_log_item(&path, cached_offset.unwrap_or(offset))?
        }
        ItemRef::Saved { role } => {
            let (user, assistant) = saved_messages(homes, identity, &run.thread_id)
                .ok_or_else(|| AppError::invalid("The saved message is no longer available"))?;
            let text = match role.as_str() {
                "user" => user,
                "assistant" => assistant,
                _ => None,
            }
            .ok_or_else(|| AppError::invalid("Invalid saved message"))?;
            json!({"type":if role == "user" { "userMessage" } else { "agentMessage" }, "text":text})
        }
    };
    if item_kind(&value).is_none() {
        return Err(AppError::invalid(
            "This is not visible conversation activity",
        ));
    }
    let (details, text) = run_activity::body(&value);
    run_activity::chunk(details, &text, cursor)
}
