//! Tauri command surface. Every command is thin: validate, delegate, map errors.

use crate::actions::{
    self, AutomationCreateInput, AutomationPatch, McpServerInput, McpTarget, NewEntityKind,
};
use crate::error::{AppError, AppResult};
use crate::fsutil::{self, display, read_text_with_meta};
use crate::model::{
    Agent, AppSettings, ApplyResult, BackupInfo, CodexMcpRuntime, DirEntryInfo, FileText,
    FrontmatterChange, Mutation, Scope, Snapshot, WritePlan,
};
use crate::writeplan::{self, ApplyCtx, PlanInput};
use crate::{codex, scan, settings, AppState};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

fn settings_of(state: &State<'_, AppState>) -> AppResult<AppSettings> {
    Ok(state
        .settings
        .lock()
        .map_err(|_| AppError::conflict("settings lock poisoned"))?
        .clone())
}

#[tauri::command]
pub fn scan_all(state: State<'_, AppState>) -> AppResult<Snapshot> {
    let settings = settings_of(&state)?;
    let snapshot = scan::scan_all(&state.homes, &settings);
    if let Some(w) = state.watcher.as_ref() {
        for p in &snapshot.projects {
            w.watch_project(Path::new(&p.root));
        }
    }
    Ok(snapshot)
}

// ---------------------------------------------------------------------------
// Previews (return a WritePlan; nothing is written)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn preview_save_file_text(
    state: State<'_, AppState>,
    path: String,
    text: String,
    base_hash: Option<String>,
) -> AppResult<WritePlan> {
    actions::plan_save_file_text(&state.homes, &path, text, base_hash)
}

#[tauri::command]
pub fn preview_upsert_mcp_server(
    state: State<'_, AppState>,
    target: McpTarget,
    input: McpServerInput,
) -> AppResult<WritePlan> {
    actions::plan_upsert_mcp(&state.homes, &target, &input)
}

#[tauri::command]
pub fn preview_delete_mcp_server(
    state: State<'_, AppState>,
    target: McpTarget,
) -> AppResult<WritePlan> {
    actions::plan_delete_mcp(&state.homes, &target)
}

#[tauri::command]
pub fn preview_set_mcp_enabled(
    state: State<'_, AppState>,
    target: McpTarget,
    enabled: bool,
    project_root: Option<String>,
) -> AppResult<WritePlan> {
    actions::plan_set_mcp_enabled(&state.homes, &target, enabled, project_root.as_deref())
}

#[tauri::command]
pub fn preview_set_plugin_enabled(
    state: State<'_, AppState>,
    agent: Agent,
    plugin_id: String,
    scope: Scope,
    enabled: bool,
) -> AppResult<WritePlan> {
    actions::plan_set_plugin_enabled(&state.homes, agent, &plugin_id, &scope, enabled)
}

#[tauri::command]
pub fn preview_set_skill_enabled(
    state: State<'_, AppState>,
    skill_id: String,
    enabled: bool,
) -> AppResult<WritePlan> {
    actions::plan_set_skill_enabled(&state.homes, &skill_id, enabled)
}

#[tauri::command]
pub fn preview_save_frontmatter(
    state: State<'_, AppState>,
    path: String,
    changes: Vec<FrontmatterChange>,
    body: Option<String>,
    base_hash: Option<String>,
) -> AppResult<WritePlan> {
    actions::plan_save_frontmatter(&state.homes, &path, changes, body, base_hash)
}

#[tauri::command]
pub fn preview_save_automation(
    state: State<'_, AppState>,
    id: String,
    patch: AutomationPatch,
) -> AppResult<WritePlan> {
    actions::plan_save_automation(&state.homes, &id, &patch)
}

#[tauri::command]
pub fn preview_set_automation_status(
    state: State<'_, AppState>,
    id: String,
    status: String,
) -> AppResult<WritePlan> {
    actions::plan_set_automation_status(&state.homes, &id, &status)
}

#[tauri::command]
pub fn preview_create_automation(
    state: State<'_, AppState>,
    input: AutomationCreateInput,
) -> AppResult<WritePlan> {
    actions::plan_create_automation(&state.homes, &input)
}

#[tauri::command]
pub fn preview_create_entity(
    state: State<'_, AppState>,
    kind: NewEntityKind,
    agent: Agent,
    scope: Scope,
    name: String,
) -> AppResult<WritePlan> {
    actions::plan_create_entity(&state.homes, kind, agent, &scope, &name)
}

#[tauri::command]
pub fn preview_delete_path(state: State<'_, AppState>, path: String) -> AppResult<WritePlan> {
    actions::plan_delete_path(&state.homes, &path)
}

// ---------------------------------------------------------------------------
// Apply + backups
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn apply_write_plan(state: State<'_, AppState>, plan: WritePlan) -> AppResult<ApplyResult> {
    let backups = state
        .backups
        .lock()
        .map_err(|_| AppError::conflict("backup lock poisoned"))?
        .clone();
    let recent = state.recent_writes.clone();
    let ctx = ApplyCtx {
        homes: &state.homes,
        backups: &backups,
        on_written: &|p| recent.record(p),
    };
    writeplan::apply(&ctx, &plan)
}

#[tauri::command]
pub fn list_backups(state: State<'_, AppState>, path: String) -> AppResult<Vec<BackupInfo>> {
    let backups = state
        .backups
        .lock()
        .map_err(|_| AppError::conflict("backup lock poisoned"))?
        .clone();
    backups.list(Path::new(&path))
}

#[tauri::command]
pub fn read_backup(state: State<'_, AppState>, id: String) -> AppResult<String> {
    let backups = state
        .backups
        .lock()
        .map_err(|_| AppError::conflict("backup lock poisoned"))?
        .clone();
    let (bak, _) = backups.resolve(&id)?;
    std::fs::read_to_string(&bak).map_err(|e| AppError::io(e, &bak))
}

#[tauri::command]
pub fn preview_restore_backup(state: State<'_, AppState>, id: String) -> AppResult<WritePlan> {
    let backups = state
        .backups
        .lock()
        .map_err(|_| AppError::conflict("backup lock poisoned"))?
        .clone();
    let (bak, original) = backups.resolve(&id)?;
    let text = std::fs::read_to_string(&bak).map_err(|e| AppError::io(e, &bak))?;
    let mut plan = writeplan::build(PlanInput {
        homes: &state.homes,
        path: &original,
        mutation: Mutation::RawText,
        description: format!("Restore backup {id}"),
        base_hash: None,
        text: Some(text),
        mode: None,
        extra_files: vec![],
    })?;
    // A restore is meant to win over whatever is there now.
    plan.base_hash = None;
    Ok(plan)
}

// ---------------------------------------------------------------------------
// Codex runtime overlay
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexMcpStatusEntry {
    pub name: String,
    #[serde(flatten)]
    pub runtime: CodexMcpRuntime,
}

const CODEX_STATUS_TTL: Duration = Duration::from_secs(30);

#[tauri::command]
pub async fn codex_mcp_status(
    state: State<'_, AppState>,
    force: Option<bool>,
) -> AppResult<Vec<CodexMcpStatusEntry>> {
    if !force.unwrap_or(false) {
        if let Ok(cache) = state.codex_status.lock() {
            if let Some((at, entries)) = cache.as_ref() {
                if at.elapsed() < CODEX_STATUS_TTL {
                    return Ok(entries.clone());
                }
            }
        }
    }
    let settings = settings_of(&state)?;
    let binary = settings
        .codex_binary
        .clone()
        .map(PathBuf::from)
        .or_else(crate::settings::detect_codex_binary)
        .ok_or_else(|| {
            AppError::not_found("codex binary not found; set it in Settings", "codex")
        })?;
    let codex_home = state.homes.codex_home.clone();
    let entries = tauri::async_runtime::spawn_blocking(move || {
        codex::runtime::fetch_mcp_status(&binary, &codex_home, Duration::from_secs(10))
    })
    .await
    .map_err(|e| AppError::conflict(e.to_string()))??;
    let entries: Vec<CodexMcpStatusEntry> = entries
        .into_iter()
        .map(|(name, runtime)| CodexMcpStatusEntry { name, runtime })
        .collect();
    if let Ok(mut cache) = state.codex_status.lock() {
        *cache = Some((Instant::now(), entries.clone()));
    }
    Ok(entries)
}

#[tauri::command]
pub fn get_file_text(path: String) -> AppResult<FileText> {
    let path = PathBuf::from(path);
    let meta = read_text_with_meta(&path)?;
    Ok(FileText {
        path: display(&path),
        text: meta.text,
        hash: meta.hash,
        mtime_ms: meta.mtime_ms,
        mode: meta.mode,
        language: fsutil::language_for(&path).to_string(),
    })
}

#[tauri::command]
pub fn list_dir(path: String) -> AppResult<Vec<DirEntryInfo>> {
    fsutil::list_dir(Path::new(&path))
}

#[tauri::command]
pub fn get_app_settings(state: State<'_, AppState>) -> AppResult<AppSettings> {
    settings_of(&state)
}

#[tauri::command]
pub fn set_app_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> AppResult<AppSettings> {
    state.settings_store.save(&settings)?;
    if let Ok(mut b) = state.backups.lock() {
        b.set_limit(settings.backup_limit as usize);
    }
    let mut guard = state
        .settings
        .lock()
        .map_err(|_| AppError::conflict("settings lock poisoned"))?;
    *guard = settings.clone();
    Ok(settings)
}

#[tauri::command]
pub fn reveal_in_file_manager(app: AppHandle, path: String) -> AppResult<()> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(AppError::not_found("path does not exist", &p));
    }
    app.opener()
        .reveal_item_in_dir(&p)
        .map_err(|e| AppError::new(crate::error::ErrorCode::Io, e.to_string()).with_path(&p))
}

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> AppResult<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(AppError::invalid("only http(s) URLs can be opened"));
    }
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| AppError::new(crate::error::ErrorCode::Io, e.to_string()))
}

#[tauri::command]
pub fn open_in_editor(
    state: State<'_, AppState>,
    path: String,
    line: Option<u32>,
) -> AppResult<()> {
    let target = PathBuf::from(&path);
    if !target.exists() {
        return Err(AppError::not_found("path does not exist", &target));
    }
    let template = {
        let guard = state
            .settings
            .lock()
            .map_err(|_| AppError::conflict("settings lock poisoned"))?;
        guard
            .editor_command
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(settings::detect_editor_command)
    };
    launch_editor(&template, &target, line)
}

/// Expand `{path}` / `{line}` in the template and spawn it detached.
pub fn launch_editor(template: &str, target: &Path, line: Option<u32>) -> AppResult<()> {
    let parts = split_command(template)?;
    let (program, args) = parts
        .split_first()
        .ok_or_else(|| AppError::invalid("editor command is empty"))?;
    let path_str = display(target);
    let line_str = line.unwrap_or(1).to_string();
    let resolved: Vec<String> = args
        .iter()
        .map(|a| {
            let a = a.replace("{path}", &path_str);
            if line.is_some() {
                a.replace("{line}", &line_str)
            } else {
                // Strip a dangling ":{line}" suffix when no line was requested.
                a.replace(":{line}", "").replace("{line}", "")
            }
        })
        .filter(|a| !a.is_empty())
        .collect();
    let program_path = if program.contains('/') {
        PathBuf::from(program)
    } else {
        settings::which(program).unwrap_or_else(|| PathBuf::from(program))
    };
    Command::new(&program_path)
        .args(&resolved)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| AppError::invalid(format!("failed to launch `{program}`: {e}")))?;
    Ok(())
}

/// Shell-like splitting with single/double quotes and backslash escapes.
pub fn split_command(input: &str) -> AppResult<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut has_token = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double => {
                in_single = !in_single;
                has_token = true;
            }
            '"' if !in_single => {
                in_double = !in_double;
                has_token = true;
            }
            '\\' if !in_single => {
                if let Some(next) = chars.next() {
                    cur.push(next);
                    has_token = true;
                }
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if has_token {
                    out.push(std::mem::take(&mut cur));
                    has_token = false;
                }
            }
            c => {
                cur.push(c);
                has_token = true;
            }
        }
    }
    if in_single || in_double {
        return Err(AppError::invalid("unterminated quote in editor command"));
    }
    if has_token {
        out.push(cur);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_command_handles_quotes() {
        assert_eq!(
            split_command(r#"code --goto "{path}:{line}" 'a b' c\ d"#).unwrap(),
            vec!["code", "--goto", "{path}:{line}", "a b", "c d"]
        );
        assert!(split_command("code \"oops").is_err());
    }
}
