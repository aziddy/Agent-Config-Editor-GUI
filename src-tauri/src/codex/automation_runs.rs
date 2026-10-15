//! Read-only access to Codex desktop automation history. No app-server calls or write plans.

use super::run_history;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::fsutil::Homes;
use crate::model::{
    AutomationRun, AutomationRunActivity, AutomationRunConversation, AutomationRunPage,
};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use toml_edit::DocumentMut;

pub(super) const PAGE_SIZE: usize = 50;

pub(super) fn db_error(error: rusqlite::Error, path: &Path) -> AppError {
    // SQLite errors contain schema/path information, never the selected row contents.
    AppError::new(ErrorCode::Io, format!("Cannot read Codex history: {error}")).with_path(path)
}

pub(super) fn open_read_only(path: &Path) -> AppResult<Connection> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| db_error(e, path))?;
    connection
        .busy_timeout(Duration::from_secs(1))
        .and_then(|()| connection.execute_batch("PRAGMA query_only = ON;"))
        .map_err(|e| db_error(e, path))?;
    Ok(connection)
}

pub(super) fn columns(connection: &Connection, table: &str) -> rusqlite::Result<HashSet<String>> {
    // All callers supply a fixed, internal table name.
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| row.get(1))?;
    rows.collect()
}

fn automation_identity(homes: &Homes, id: &str) -> AppResult<String> {
    let path = id
        .strip_prefix("codex:automation:")
        .map(PathBuf::from)
        .ok_or_else(|| AppError::invalid("Select a Codex automation to view its runs"))?;
    let root = homes.codex_home.join("automations");
    let canonical_root = root.canonicalize().map_err(|e| AppError::io(e, &root))?;
    let canonical_path = path.canonicalize().map_err(|e| AppError::io(e, &path))?;
    if canonical_path.file_name().and_then(|n| n.to_str()) != Some("automation.toml")
        || canonical_path.parent().and_then(Path::parent) != Some(canonical_root.as_path())
    {
        return Err(AppError::invalid(
            "Automation is outside the resolved Codex home",
        ));
    }
    let text = std::fs::read_to_string(&canonical_path).map_err(|e| AppError::io(e, &path))?;
    let doc = text
        .parse::<DocumentMut>()
        .map_err(|_| AppError::parse("Cannot read the selected automation definition", &path))?;
    Ok(doc
        .get("id")
        .and_then(toml_edit::Item::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            canonical_path
                .parent()
                .and_then(Path::file_name)
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        }))
}

pub(super) fn app_databases(homes: &Homes) -> Vec<PathBuf> {
    ["codex.db", "codex-dev.db"]
        .iter()
        .map(|name| homes.codex_home.join("sqlite").join(name))
        .filter(|path| path.is_file())
        .collect()
}

fn optional_column(names: &HashSet<String>, name: &str) -> String {
    if names.contains(name) {
        format!("\"{name}\"")
    } else {
        "NULL".to_string()
    }
}

fn read_runs(path: &Path, automation_id: &str) -> AppResult<Vec<AutomationRun>> {
    let connection = open_read_only(path)?;
    let names = columns(&connection, "automation_runs").map_err(|e| db_error(e, path))?;
    if ![
        "thread_id",
        "automation_id",
        "status",
        "created_at",
        "updated_at",
    ]
    .iter()
    .all(|name| names.contains(*name))
    {
        return Err(AppError::parse(
            "This Codex run-history schema is not supported",
            path,
        ));
    }
    let optional = [
        "read_at",
        "thread_title",
        "inbox_title",
        "inbox_summary",
        "source_cwd",
        "archived_reason",
    ]
    .map(|name| optional_column(&names, name));
    let sql = format!(
        "SELECT thread_id, status, created_at, updated_at, {} FROM automation_runs WHERE automation_id = ?1",
        optional.join(", ")
    );
    let mut statement = connection.prepare(&sql).map_err(|e| db_error(e, path))?;
    let rows = statement
        .query_map([automation_id], |row| {
            let title: Option<String> = row.get(5)?;
            let inbox_title: Option<String> = row.get(6)?;
            Ok(AutomationRun {
                thread_id: row.get(0)?,
                status: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                read_at: row.get(4)?,
                title: inbox_title
                    .filter(|s| !s.is_empty())
                    .or(title)
                    .unwrap_or_else(|| "Automation run".to_string()),
                summary: row.get(7)?,
                source_cwd: row.get(8)?,
                archived_reason: row.get(9)?,
            })
        })
        .map_err(|e| db_error(e, path))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| db_error(e, path))
}

fn catalog(homes: &Homes, id: &str) -> AppResult<(Vec<AutomationRun>, Vec<String>, bool)> {
    let identity = automation_identity(homes, id)?;
    let mut by_thread: HashMap<String, AutomationRun> = HashMap::new();
    let mut warnings = Vec::new();
    let mut available = false;
    for path in app_databases(homes) {
        match read_runs(&path, &identity) {
            Ok(runs) => {
                available = true;
                for run in runs {
                    let replace = by_thread
                        .get(&run.thread_id)
                        .is_none_or(|old| run.updated_at > old.updated_at);
                    if replace {
                        by_thread.insert(run.thread_id.clone(), run);
                    }
                }
            }
            Err(error) => warnings.push(error.to_string()),
        }
    }
    let mut runs: Vec<_> = by_thread.into_values().collect();
    runs.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.thread_id.cmp(&a.thread_id))
    });
    Ok((runs, warnings, available))
}

#[derive(Serialize, Deserialize)]
struct RunCursor {
    created_at: i64,
    thread_id: String,
}

pub fn list_runs(homes: &Homes, id: &str, cursor: Option<&str>) -> AppResult<AutomationRunPage> {
    let (mut runs, warnings, storage_available) = catalog(homes, id)?;
    if let Some(cursor) = cursor {
        let cursor: RunCursor = serde_json::from_str(cursor)
            .map_err(|_| AppError::invalid("Invalid run-history cursor"))?;
        runs.retain(|run| {
            (run.created_at, &run.thread_id) < (cursor.created_at, &cursor.thread_id)
        });
    }
    let more = runs.len() > PAGE_SIZE;
    runs.truncate(PAGE_SIZE);
    let next_cursor = if more {
        runs.last()
            .map(|run| {
                serde_json::to_string(&RunCursor {
                    created_at: run.created_at,
                    thread_id: run.thread_id.clone(),
                })
            })
            .transpose()?
    } else {
        None
    };
    Ok(AutomationRunPage {
        runs,
        next_cursor,
        storage_available,
        warnings,
    })
}

fn owned_run(homes: &Homes, id: &str, thread_id: &str) -> AppResult<(String, AutomationRun)> {
    let identity = automation_identity(homes, id)?;
    let (runs, warnings, available) = catalog(homes, id)?;
    if !available && !warnings.is_empty() {
        return Err(AppError::new(ErrorCode::Io, warnings.join("\n")));
    }
    let run = runs
        .into_iter()
        .find(|run| run.thread_id == thread_id)
        .ok_or_else(|| AppError::invalid("This run does not belong to the selected automation"))?;
    Ok((identity, run))
}

pub fn conversation(
    homes: &Homes,
    id: &str,
    thread_id: &str,
    before: Option<i64>,
    after: Option<i64>,
) -> AppResult<AutomationRunConversation> {
    if before.is_some() && after.is_some() {
        return Err(AppError::invalid(
            "Use either an earlier-history cursor or a refresh cursor",
        ));
    }
    let (identity, run) = owned_run(homes, id, thread_id)?;
    run_history::conversation(homes, &identity, run, before, after)
}

pub fn activity(
    homes: &Homes,
    id: &str,
    thread_id: &str,
    item_id: &str,
    cursor: Option<&str>,
) -> AppResult<AutomationRunActivity> {
    let (identity, run) = owned_run(homes, id, thread_id)?;
    run_history::activity(homes, &identity, &run, item_id, cursor)
}
