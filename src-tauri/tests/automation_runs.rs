use agent_config_editor_lib::codex::automation_runs::{activity, conversation, list_runs};
use agent_config_editor_lib::fsutil::{sha256_hex, Homes};
use agent_config_editor_lib::model::RunItemKind;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const RUN_SCHEMA: &str = "CREATE TABLE automation_runs (
    thread_id TEXT PRIMARY KEY, automation_id TEXT NOT NULL, status TEXT NOT NULL,
    read_at INTEGER, thread_title TEXT, source_cwd TEXT, inbox_title TEXT, inbox_summary TEXT,
    created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
    archived_user_message TEXT, archived_assistant_message TEXT, archived_reason TEXT
);";

struct Fixture {
    _temp: TempDir,
    homes: Homes,
    id: String,
    app: Connection,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let homes = Homes::for_test(temp.path());
        let directory = homes.codex_home.join("automations/directory-name");
        fs::create_dir_all(&directory).unwrap();
        fs::create_dir_all(homes.codex_home.join("sqlite")).unwrap();
        let file = directory.join("automation.toml");
        fs::write(
            &file,
            "id = 'stored-identity'\nname = 'Test'\nprompt = 'Current edited prompt'\n",
        )
        .unwrap();
        let app = Connection::open(homes.codex_home.join("sqlite/codex.db")).unwrap();
        app.execute_batch(RUN_SCHEMA).unwrap();
        Self {
            _temp: temp,
            homes,
            id: format!("codex:automation:{}", file.display()),
            app,
        }
    }

    fn run(&self, thread: &str, time: i64) {
        self.app.execute("INSERT INTO automation_runs (thread_id,automation_id,status,thread_title,created_at,updated_at) VALUES (?1,'stored-identity','PENDING_REVIEW','Recorded run',?2,?2)", params![thread,time]).unwrap();
    }

    fn indexed(&self) -> Connection {
        let db = Connection::open(self.homes.codex_home.join("thread_history_1.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE thread_items (thread_id TEXT,turn_id TEXT,item_id TEXT,item_type TEXT,item_json TEXT,rollout_ordinal INTEGER,created_at_ms INTEGER,updated_at_ordinal INTEGER DEFAULT 0,PRIMARY KEY(thread_id,turn_id,item_id)); CREATE TABLE thread_turns (thread_id TEXT,status TEXT,duration_ms INTEGER,rollout_ordinal INTEGER);").unwrap();
        db
    }

    fn rollout(&self, thread: &str, records: &[Value]) -> PathBuf {
        let directory = self.homes.codex_home.join("sessions/2026/09/29");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!("rollout-test-{thread}.jsonl"));
        let mut text = String::new();
        for record in records {
            text.push_str(&format!("{record}\n"));
        }
        fs::write(&path, text).unwrap();
        let db = Connection::open(self.homes.codex_home.join("state_5.sqlite")).unwrap();
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS threads (id TEXT PRIMARY KEY,rollout_path TEXT)",
        )
        .unwrap();
        db.execute(
            "INSERT OR REPLACE INTO threads VALUES (?1,?2)",
            params![thread, path.to_string_lossy()],
        )
        .unwrap();
        path
    }
}

fn indexed_item(db: &Connection, thread: &str, ordinal: i64, value: Value) {
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("item-{ordinal}"));
    db.execute("INSERT INTO thread_items (thread_id,turn_id,item_id,item_type,item_json,rollout_ordinal,created_at_ms) VALUES (?1,'turn',?2,?3,?4,?5,?5)", params![thread,id,value["type"].as_str().unwrap(),value.to_string(),ordinal]).unwrap();
}

fn event(item: Value) -> Value {
    json!({"type":"event_msg","payload":{"type":"item_completed","turn_id":"turn","item":item}})
}

fn append(path: &Path, text: &str) {
    let mut file = fs::OpenOptions::new().append(true).open(path).unwrap();
    file.write_all(text.as_bytes()).unwrap();
}

#[test]
fn matches_toml_identity_and_pages_without_skipping_on_new_insert() {
    let f = Fixture::new();
    for n in 0..110 {
        f.run(&format!("run-{n:03}"), n);
    }
    f.app
        .execute(
            "UPDATE automation_runs SET status='FUTURE_STATUS' WHERE thread_id='run-109'",
            [],
        )
        .unwrap();
    let first = list_runs(&f.homes, &f.id, None).unwrap();
    assert_eq!(first.runs.len(), 50);
    assert_eq!(first.runs[0].thread_id, "run-109");
    assert_eq!(first.runs[0].status, "FUTURE_STATUS");
    f.run("new-run", 200);
    let second = list_runs(&f.homes, &f.id, first.next_cursor.as_deref()).unwrap();
    assert_eq!(second.runs[0].thread_id, "run-059");
    let last = list_runs(&f.homes, &f.id, second.next_cursor.as_deref()).unwrap();
    assert_eq!(last.runs.len(), 10);
    assert!(last.next_cursor.is_none());
}

#[test]
fn reads_wal_and_leaves_configuration_database_and_review_state_unchanged() {
    let f = Fixture::new();
    f.app
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
        .unwrap();
    f.run("wal-run", 1);
    let file = f
        .homes
        .codex_home
        .join("automations/directory-name/automation.toml");
    let db = f.homes.codex_home.join("sqlite/codex.db");
    let before = (
        fs::read(&file).unwrap(),
        fs::read(&db).unwrap(),
        fs::read(db.with_extension("db-wal")).unwrap(),
    );
    let page = list_runs(&f.homes, &f.id, None).unwrap();
    assert_eq!(page.runs[0].thread_id, "wal-run");
    conversation(&f.homes, &f.id, "wal-run", None, None).unwrap();
    assert_eq!(
        before,
        (
            fs::read(file).unwrap(),
            fs::read(&db).unwrap(),
            fs::read(db.with_extension("db-wal")).unwrap()
        )
    );
    let row: (String, Option<i64>) = f
        .app
        .query_row("SELECT status,read_at FROM automation_runs", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(row, ("PENDING_REVIEW".into(), None));
}

#[test]
fn merges_desktop_databases_and_handles_missing_or_unsupported_storage() {
    let f = Fixture::new();
    f.run("shared", 1);
    let dev = Connection::open(f.homes.codex_home.join("sqlite/codex-dev.db")).unwrap();
    dev.execute_batch(RUN_SCHEMA).unwrap();
    dev.execute("INSERT INTO automation_runs (thread_id,automation_id,status,created_at,updated_at) VALUES ('shared','stored-identity','ARCHIVED',1,2)", []).unwrap();
    let page = list_runs(&f.homes, &f.id, None).unwrap();
    assert_eq!(page.runs.len(), 1);
    assert_eq!(page.runs[0].status, "ARCHIVED");
    drop(dev);
    fs::remove_file(f.homes.codex_home.join("sqlite/codex-dev.db")).unwrap();
    f.app
        .execute_batch(
            "DROP TABLE automation_runs; CREATE TABLE automation_runs (different_schema TEXT)",
        )
        .unwrap();
    let page = list_runs(&f.homes, &f.id, None).unwrap();
    assert!(!page.storage_available);
    assert_eq!(page.warnings.len(), 1);
    drop(f.app);
    fs::remove_file(f.homes.codex_home.join("sqlite/codex.db")).unwrap();
    let page = list_runs(&f.homes, &f.id, None).unwrap();
    assert!(!page.storage_available);
    assert!(page.warnings.is_empty());
    assert!(!f.homes.codex_home.join("sqlite/codex.db").exists());
}

#[test]
fn pages_indexed_messages_and_refreshes_updated_items_without_reasoning() {
    let f = Fixture::new();
    f.run("run", 1);
    let db = f.indexed();
    indexed_item(
        &db,
        "run",
        0,
        json!({"type":"userMessage","content":[{"type":"text","text":"Original recorded prompt"}]}),
    );
    for n in 1..120 {
        indexed_item(
            &db,
            "run",
            n,
            json!({"type":"agentMessage","text":format!("Reply {n}"),"phase":"commentary"}),
        );
    }
    indexed_item(
        &db,
        "run",
        120,
        json!({"type":"reasoning","text":"hidden reasoning"}),
    );
    indexed_item(
        &db,
        "run",
        121,
        json!({"type":"commandExecution","id":"command","command":"echo test","aggregatedOutput":"x".repeat(400000)}),
    );
    db.execute(
        "INSERT INTO thread_turns VALUES ('run','completed',44000,130)",
        [],
    )
    .unwrap();
    let first = conversation(&f.homes, &f.id, "run", None, None).unwrap();
    assert_eq!(first.items.len(), 50);
    assert_eq!(first.duration_ms, Some(44000));
    assert!(first.items.last().unwrap().text.is_none());
    let mut all = first.items.clone();
    let mut before = first.before_cursor;
    while let Some(cursor) = before {
        let earlier = conversation(&f.homes, &f.id, "run", Some(cursor), None).unwrap();
        all.extend(earlier.items);
        before = earlier.before_cursor;
    }
    assert_eq!(all.len(), 121);
    assert!(all
        .iter()
        .any(|item| item.text.as_deref() == Some("Original recorded prompt")));
    db.execute("UPDATE thread_items SET item_json=?1,updated_at_ordinal=200 WHERE item_id='command'", [json!({"type":"commandExecution","id":"command","command":"echo test","aggregatedOutput":"done"}).to_string()]).unwrap();
    let update = conversation(&f.homes, &f.id, "run", None, Some(first.after_cursor)).unwrap();
    assert_eq!(update.items.len(), 1);
    assert_eq!(update.items[0].id, first.items.last().unwrap().id);
    assert_eq!(update.items[0].version, 200);
    assert_eq!(update.items[0].position, 121);
}

#[test]
fn lazily_chunks_unicode_activity_and_masks_structured_secrets() {
    let f = Fixture::new();
    f.run("run", 1);
    let db = f.indexed();
    let output = "λ🧑🏽‍💻".repeat(15000);
    indexed_item(
        &db,
        "run",
        1,
        json!({"type":"commandExecution","command":"echo test","aggregatedOutput":output,"exitCode":0}),
    );
    indexed_item(
        &db,
        "run",
        2,
        json!({"type":"mcpToolCall","server":"demo","tool":"example","arguments":{"api_key":"do-not-show","nested":{"password":"also-private"}},"result":{"content":[{"type":"text","text":"{\"token\":\"nested-private\"}"}]}}),
    );
    let page = conversation(&f.homes, &f.id, "run", None, None).unwrap();
    let mut combined = String::new();
    let mut cursor = None;
    loop {
        let part = activity(&f.homes, &f.id, "run", &page.items[0].id, cursor.as_deref()).unwrap();
        assert!(part.text.len() <= 64 * 1024);
        combined.push_str(&part.text);
        cursor = part.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(combined, format!("$ echo test\n\n{output}"));
    let tool = activity(&f.homes, &f.id, "run", &page.items[1].id, None).unwrap();
    assert!(!tool.text.contains("do-not-show"));
    assert!(!tool.text.contains("also-private"));
    assert!(!tool.text.contains("nested-private"));
    assert!(activity(&f.homes, &f.id, "other-run", &page.items[0].id, None).is_err());
    indexed_item(
        &db,
        "run",
        3,
        json!({"type":"reasoning","id":"private","text":"hidden"}),
    );
    assert!(activity(
        &f.homes,
        &f.id,
        "run",
        &json!({"source":"history","turn":"turn","item":"private"}).to_string(),
        None
    )
    .is_err());
    indexed_item(
        &db,
        "run",
        4,
        json!({"type":"agentMessage","id":"analysis","phase":"analysis","text":"private analysis"}),
    );
    assert!(activity(
        &f.homes,
        &f.id,
        "run",
        &json!({"source":"history","turn":"turn","item":"analysis"}).to_string(),
        None
    )
    .is_err());
    indexed_item(
        &db,
        "run",
        5,
        json!({"type":"mcpToolCall","id":"media","result":{"content":[{"type":"image","data":"binary-payload"}]}}),
    );
    let media = activity(
        &f.homes,
        &f.id,
        "run",
        &json!({"source":"history","turn":"turn","item":"media"}).to_string(),
        None,
    )
    .unwrap();
    assert!(!media.text.contains("binary-payload"));
    assert!(media.text.contains("attachment"));
    indexed_item(
        &db,
        "run",
        6,
        json!({"type":"commandExecution","id":"streams","command":"test","stdout":"standard output","stderr":"error output"}),
    );
    let streams = activity(
        &f.homes,
        &f.id,
        "run",
        &json!({"source":"history","turn":"turn","item":"streams"}).to_string(),
        None,
    )
    .unwrap();
    assert!(streams.text.contains("standard output\nerror output"));
}

#[test]
fn older_logs_deduplicate_visible_events_and_incrementally_complete_partial_lines() {
    let f = Fixture::new();
    f.run("run", 1);
    let original = json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Original recorded prompt"}]}});
    let raw_reply = json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Reply"}]}});
    let path = f.rollout(
        "run",
        &[
            json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"turn"}}),
            original,
            raw_reply,
        ],
    );
    let first = conversation(&f.homes, &f.id, "run", None, None).unwrap();
    assert_eq!(first.items.len(), 2);
    append(
        &path,
        &format!(
            "{}\n",
            event(
                json!({"type":"AgentMessage","id":"answer","content":[{"type":"Text","text":"Reply"}],"phase":"final_answer"})
            )
        ),
    );
    let update = conversation(&f.homes, &f.id, "run", None, Some(first.after_cursor)).unwrap();
    assert_eq!(update.items.len(), 1);
    assert!(update.removed_item_ids.contains(&first.items[1].id));
    let pending = event(json!({"type":"AgentMessage","id":"next","text":"Next reply"})).to_string();
    append(&path, &pending[..pending.len() / 2]);
    let partial = conversation(&f.homes, &f.id, "run", None, Some(update.after_cursor)).unwrap();
    assert!(partial.items.is_empty());
    append(
        &path,
        &format!("{}\ninvalid JSON\n", &pending[pending.len() / 2..]),
    );
    let complete = conversation(&f.homes, &f.id, "run", None, Some(update.after_cursor)).unwrap();
    assert_eq!(complete.items.len(), 1);
    assert_eq!(complete.notices.len(), 1);
    append(
        &path,
        &format!(
            "{}\n",
            event(json!({"type":"AgentMessage","id":"next","text":"Updated reply"}))
        ),
    );
    let replacement =
        conversation(&f.homes, &f.id, "run", None, Some(complete.after_cursor)).unwrap();
    assert_eq!(replacement.items.len(), 1);
    assert_eq!(replacement.items[0].id, complete.items[0].id);
    assert!(replacement.items[0].version > complete.items[0].version);
    fs::write(
        path,
        format!(
            "{}\n",
            event(json!({"type":"AgentMessage","text":"Replaced log"}))
        ),
    )
    .unwrap();
    let reset = conversation(&f.homes, &f.id, "run", None, Some(replacement.after_cursor)).unwrap();
    assert!(reset.reset);
    assert_eq!(reset.items[0].text.as_deref(), Some("Replaced log"));
}

#[test]
fn recovers_initial_prompt_and_handles_saved_or_unavailable_archived_runs() {
    let f = Fixture::new();
    f.run("run", 1);
    let db = f.indexed();
    indexed_item(
        &db,
        "run",
        1,
        json!({"type":"agentMessage","text":"Final reply","phase":"final_answer"}),
    );
    f.rollout("run", &[json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Original prompt"},{"type":"input_text","text":"# AGENTS.md instructions for /project\nContext"},{"type":"input_text","text":"<environment_context>Context</environment_context>"}]}})]);
    let page = conversation(&f.homes, &f.id, "run", None, None).unwrap();
    assert_eq!(page.items[0].text.as_deref(), Some("Original prompt"));
    f.run("archived", 2);
    f.app.execute("UPDATE automation_runs SET status='ARCHIVED',archived_assistant_message='Saved final result' WHERE thread_id='archived'", []).unwrap();
    let saved = conversation(&f.homes, &f.id, "archived", None, None).unwrap();
    assert_eq!(saved.availability, "saved");
    assert_eq!(saved.items[0].text.as_deref(), Some("Saved final result"));
    f.run("pending:placeholder", 3);
    let absent = conversation(&f.homes, &f.id, "pending:placeholder", None, None).unwrap();
    assert_eq!(absent.availability, "unavailable");
    assert!(absent.items.is_empty());
    let external = tempfile::NamedTempFile::new().unwrap();
    assert!(list_runs(
        &f.homes,
        &format!("codex:automation:{}", external.path().display()),
        None
    )
    .is_err());
}

#[test]
#[ignore = "Reads live Codex history without writes; run explicitly with ACE_READ_ONLY=1"]
fn live_codex_history_is_read_only() {
    let homes = Homes::detect().unwrap();
    assert!(
        homes.read_only,
        "Set ACE_READ_ONLY=1 for the live history smoke test"
    );
    let directory = homes.codex_home.join("automations");
    let mut count = 0;
    for entry in fs::read_dir(directory).unwrap().filter_map(Result::ok) {
        let path = entry.path().join("automation.toml");
        if !path.is_file() {
            continue;
        }
        let before = sha256_hex(&fs::read(&path).unwrap());
        let id = format!("codex:automation:{}", path.display());
        let history = list_runs(&homes, &id, None).unwrap();
        assert!(
            history.warnings.is_empty(),
            "Unexpected history storage warning"
        );
        if let Some(run) = history.runs.first() {
            let page = conversation(&homes, &id, &run.thread_id, None, None).unwrap();
            assert!(page.notices.is_empty(), "Unexpected conversation warning");
            assert_eq!(page.availability, "local");
            assert!(!page.items.is_empty());
            let refresh =
                conversation(&homes, &id, &run.thread_id, None, Some(page.after_cursor)).unwrap();
            assert!(
                refresh.items.is_empty(),
                "Completed run changed during smoke test"
            );
            if let Some(item) = page
                .items
                .iter()
                .find(|item| item.kind == RunItemKind::Activity)
            {
                let output = activity(&homes, &id, &run.thread_id, &item.id, None).unwrap();
                assert!(output.text.len() <= 64 * 1024);
            }
            let again = list_runs(&homes, &id, None).unwrap();
            let unchanged = again
                .runs
                .iter()
                .find(|item| item.thread_id == run.thread_id)
                .unwrap();
            assert_eq!(
                (unchanged.read_at, &unchanged.status),
                (run.read_at, &run.status)
            );
        }
        assert_eq!(before, sha256_hex(&fs::read(&path).unwrap()));
        count += 1;
    }
    println!("Read-only live history verified for {count} automations; no prompt/output contents logged.");
}

#[test]
#[ignore = "Read-only performance smoke test against the largest local automation run"]
fn live_large_history_is_paged() {
    let homes = Homes::detect().unwrap();
    assert!(
        homes.read_only,
        "Set ACE_READ_ONLY=1 for live history smoke tests"
    );
    let state = homes.codex_home.join("state_5.sqlite");
    let conn =
        Connection::open_with_flags(&state, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    conn.execute_batch("PRAGMA query_only=ON").unwrap();
    let mut largest: Option<(u64, String, String)> = None;
    for entry in fs::read_dir(homes.codex_home.join("automations"))
        .unwrap()
        .filter_map(Result::ok)
    {
        let file = entry.path().join("automation.toml");
        if !file.is_file() {
            continue;
        }
        let id = format!("codex:automation:{}", file.display());
        let mut cursor = None;
        loop {
            let page = list_runs(&homes, &id, cursor.as_deref()).unwrap();
            for run in page.runs {
                let path: rusqlite::Result<String> = conn.query_row(
                    "SELECT rollout_path FROM threads WHERE id=?1",
                    [&run.thread_id],
                    |row| row.get(0),
                );
                let size = path
                    .ok()
                    .and_then(|path| fs::metadata(path).ok())
                    .map(|meta| meta.len());
                if let Some(size) = size {
                    if largest
                        .as_ref()
                        .is_none_or(|(previous, _, _)| size > *previous)
                    {
                        largest = Some((size, id.clone(), run.thread_id));
                    }
                }
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
    }
    let (size, id, thread) = largest.expect("No local automation conversations found");
    let started = std::time::Instant::now();
    let page = conversation(&homes, &id, &thread, None, None).unwrap();
    let payload_bytes = serde_json::to_vec(&page).unwrap().len();
    assert!(page.items.len() <= 50);
    assert!(payload_bytes < 4 * 1024 * 1024);
    assert!(page
        .items
        .iter()
        .filter(|item| item.kind == RunItemKind::Activity)
        .all(|item| item.text.is_none()));
    println!("Largest local run: {} MiB; first page: {} items, {} KiB, {} ms. No conversation contents logged.", size / (1024*1024), page.items.len(), payload_bytes / 1024, started.elapsed().as_millis());
}
