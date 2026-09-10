//! Maps UI-level requests (toggle this, upsert that) onto [`WritePlan`]s.

use crate::error::{AppError, AppResult};
use crate::fsutil::{display, read_text_opt, Homes};
use crate::markdown::frontmatter;
use crate::model::{Agent, ExtraFile, FrontmatterChange, Mutation, Scope, WritePlan};
use crate::writeplan::{self, build, json_array_add, json_array_remove, pointer, PlanInput};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Inputs (mirrored in src/lib/rpc.ts)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTarget {
    pub agent: Agent,
    pub scope: Scope,
    pub file: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TransportInput {
    Stdio {
        command: String,
        args: Vec<String>,
        env: Map<String, Value>,
        cwd: Option<String>,
    },
    Http {
        url: String,
        headers: Map<String, Value>,
        bearer_token_env_var: Option<String>,
    },
    Sse {
        url: String,
        headers: Map<String, Value>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInput {
    pub transport: TransportInput,
    pub enabled: Option<bool>,
    pub startup_timeout_sec: Option<f64>,
    pub tool_timeout_sec: Option<f64>,
    #[serde(default)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationPatch {
    pub name: Option<String>,
    pub prompt: Option<String>,
    pub status: Option<String>,
    pub rrule: Option<String>,
    #[serde(default, with = "double_option")]
    pub model: Option<Option<String>>,
    #[serde(default, with = "double_option")]
    pub reasoning_effort: Option<Option<String>>,
    #[serde(default, with = "double_option")]
    pub execution_environment: Option<Option<String>>,
    pub cwds: Option<Vec<String>>,
}

/// `Option<Option<T>>`: absent = untouched, `null` = delete, value = set.
mod double_option {
    use serde::{Deserialize, Deserializer};
    pub fn deserialize<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        Option::<T>::deserialize(d).map(Some)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationCreateInput {
    pub name: String,
    pub prompt: String,
    pub rrule: String,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub execution_environment: String,
    pub cwds: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NewEntityKind {
    Skill,
    SubAgent,
    SlashCommand,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn simple_plan(
    homes: &Homes,
    path: &Path,
    mutation: Mutation,
    description: String,
) -> AppResult<WritePlan> {
    build(PlanInput {
        homes,
        path,
        mutation,
        description,
        base_hash: None,
        text: None,
        mode: None,
        extra_files: vec![],
    })
}

fn is_claude_state(homes: &Homes, file: &Path) -> bool {
    file == homes.claude_state
}

fn is_mcp_json(file: &Path) -> bool {
    file.file_name().and_then(|n| n.to_str()) == Some(".mcp.json")
}

fn plugin_short_name(plugin_id: &str) -> &str {
    plugin_id.split('@').next().unwrap_or(plugin_id)
}

fn read_json(path: &Path) -> AppResult<Value> {
    match read_text_opt(path)? {
        Some(t) if !t.trim().is_empty() => serde_json::from_str(&t)
            .map_err(|e| AppError::parse(format!("invalid JSON: {e}"), path)),
        _ => Ok(Value::Object(Map::new())),
    }
}

/// Build a plan that mutates a JSON document through a closure, so several pointer edits
/// land in one write (e.g. add to `enabled…` and remove from `disabled…`).
fn json_edit_plan(
    homes: &Homes,
    path: &Path,
    description: String,
    mutation_label: Mutation,
    edit: impl FnOnce(&mut Value) -> AppResult<()>,
) -> AppResult<WritePlan> {
    homes.check_writable(path)?;
    let current = read_text_opt(path)?;
    let mut root = read_json(path)?;
    edit(&mut root)?;
    let mut out = serde_json::to_string_pretty(&root)?;
    if current
        .as_deref()
        .is_none_or(|c| c.is_empty() || c.ends_with('\n'))
    {
        out.push('\n');
    }
    let mut plan = build(PlanInput {
        homes,
        path,
        mutation: Mutation::RawText,
        description,
        base_hash: None,
        text: Some(out),
        mode: None,
        extra_files: vec![],
    })?;
    // Keep the semantic mutation so apply() can rebase; RawText would refuse on change.
    plan.mutation = mutation_label;
    Ok(plan)
}

pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed
    }
}

// ---------------------------------------------------------------------------
// Raw text
// ---------------------------------------------------------------------------

pub fn plan_save_file_text(
    homes: &Homes,
    path: &str,
    text: String,
    base_hash: Option<String>,
) -> AppResult<WritePlan> {
    let p = PathBuf::from(path);
    build(PlanInput {
        homes,
        path: &p,
        mutation: Mutation::RawText,
        description: format!(
            "Save {}",
            p.file_name().and_then(|n| n.to_str()).unwrap_or(path)
        ),
        base_hash,
        text: Some(text),
        mode: None,
        extra_files: vec![],
    })
}

// ---------------------------------------------------------------------------
// MCP servers
// ---------------------------------------------------------------------------

fn claude_server_value(input: &McpServerInput) -> Value {
    let mut obj = Map::new();
    match &input.transport {
        TransportInput::Stdio {
            command,
            args,
            env,
            cwd,
        } => {
            obj.insert("type".into(), Value::from("stdio"));
            obj.insert("command".into(), Value::from(command.as_str()));
            obj.insert(
                "args".into(),
                Value::Array(args.iter().map(|a| Value::from(a.as_str())).collect()),
            );
            if !env.is_empty() {
                obj.insert("env".into(), Value::Object(env.clone()));
            }
            if let Some(c) = cwd.as_ref().filter(|c| !c.is_empty()) {
                obj.insert("cwd".into(), Value::from(c.as_str()));
            }
        }
        TransportInput::Http {
            url,
            headers,
            bearer_token_env_var,
        } => {
            obj.insert("type".into(), Value::from("http"));
            obj.insert("url".into(), Value::from(url.as_str()));
            if !headers.is_empty() {
                obj.insert("headers".into(), Value::Object(headers.clone()));
            }
            if let Some(b) = bearer_token_env_var.as_ref().filter(|b| !b.is_empty()) {
                obj.insert("bearer_token_env_var".into(), Value::from(b.as_str()));
            }
        }
        TransportInput::Sse { url, headers } => {
            obj.insert("type".into(), Value::from("sse"));
            obj.insert("url".into(), Value::from(url.as_str()));
            if !headers.is_empty() {
                obj.insert("headers".into(), Value::Object(headers.clone()));
            }
        }
    }
    for (k, v) in &input.extra {
        obj.entry(k.clone()).or_insert(v.clone());
    }
    Value::Object(obj)
}

/// Codex table contents. `Null` values delete keys when merged into an existing table.
fn codex_server_value(input: &McpServerInput) -> Value {
    let mut obj = Map::new();
    match &input.transport {
        TransportInput::Stdio {
            command,
            args,
            env,
            cwd,
        } => {
            obj.insert("command".into(), Value::from(command.as_str()));
            obj.insert(
                "args".into(),
                Value::Array(args.iter().map(|a| Value::from(a.as_str())).collect()),
            );
            obj.insert(
                "cwd".into(),
                cwd.as_ref()
                    .filter(|c| !c.is_empty())
                    .map(|c| Value::from(c.as_str()))
                    .unwrap_or(Value::Null),
            );
            obj.insert("url".into(), Value::Null);
            obj.insert("bearer_token_env_var".into(), Value::Null);
            obj.insert(
                "env".into(),
                if env.is_empty() {
                    Value::Null
                } else {
                    Value::Object(env.clone())
                },
            );
        }
        TransportInput::Http { url, .. } | TransportInput::Sse { url, .. } => {
            obj.insert("url".into(), Value::from(url.as_str()));
            for k in ["command", "args", "cwd", "env"] {
                obj.insert(k.into(), Value::Null);
            }
            let bearer = match &input.transport {
                TransportInput::Http {
                    bearer_token_env_var: Some(b),
                    ..
                } if !b.is_empty() => Value::from(b.as_str()),
                _ => Value::Null,
            };
            obj.insert("bearer_token_env_var".into(), bearer);
        }
    }
    obj.insert(
        "startup_timeout_sec".into(),
        input
            .startup_timeout_sec
            .map(Value::from)
            .unwrap_or(Value::Null),
    );
    obj.insert(
        "tool_timeout_sec".into(),
        input
            .tool_timeout_sec
            .map(Value::from)
            .unwrap_or(Value::Null),
    );
    obj.insert(
        "enabled".into(),
        match input.enabled {
            Some(false) => Value::Bool(false),
            _ => Value::Null,
        },
    );
    for (k, v) in &input.extra {
        obj.entry(k.clone()).or_insert(v.clone());
    }
    Value::Object(obj)
}

fn drop_nulls(v: Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.into_iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k, drop_nulls(v)))
                .collect(),
        ),
        other => other,
    }
}

fn claude_server_pointer(homes: &Homes, target: &McpTarget, file: &Path) -> AppResult<String> {
    if is_claude_state(homes, file) {
        return Ok(match &target.scope {
            Scope::User => pointer(&["mcpServers", &target.name]),
            Scope::Project { root } => pointer(&["projects", root, "mcpServers", &target.name]),
            _ => {
                return Err(AppError::invalid(
                    "unsupported scope for ~/.claude.json server",
                ))
            }
        });
    }
    if is_mcp_json(file) {
        if matches!(target.scope, Scope::Plugin { .. }) {
            return Err(AppError::read_only(
                "plugin-provided servers are managed by the plugin; edit the raw file only if you must",
                file,
            ));
        }
        return Ok(pointer(&["mcpServers", &target.name]));
    }
    Err(AppError::invalid(format!(
        "unknown Claude MCP config file: {}",
        file.display()
    )))
}

pub fn plan_upsert_mcp(
    homes: &Homes,
    target: &McpTarget,
    input: &McpServerInput,
) -> AppResult<WritePlan> {
    let file = PathBuf::from(&target.file);
    let desc = format!("Save MCP server \"{}\"", target.name);
    match target.agent {
        Agent::Claude => {
            let ptr = claude_server_pointer(homes, target, &file)?;
            let value = claude_server_value(input);
            simple_plan(
                homes,
                &file,
                Mutation::JsonSet {
                    pointer: ptr,
                    value,
                },
                desc,
            )
        }
        Agent::Codex => {
            if matches!(target.scope, Scope::Plugin { .. }) || is_mcp_json(&file) {
                return Err(AppError::read_only(
                    "plugin-provided servers are managed by the plugin",
                    &file,
                ));
            }
            let path = vec!["mcp_servers".to_string(), target.name.clone()];
            let exists = read_text_opt(&file)?
                .and_then(|t| t.parse::<toml_edit::DocumentMut>().ok())
                .is_some_and(|d| writeplan::toml_get(&d, &path).is_some());
            let value = codex_server_value(input);
            let value = if exists { value } else { drop_nulls(value) };
            simple_plan(homes, &file, Mutation::TomlSet { path, value }, desc)
        }
    }
}

pub fn plan_delete_mcp(homes: &Homes, target: &McpTarget) -> AppResult<WritePlan> {
    let file = PathBuf::from(&target.file);
    let desc = format!("Remove MCP server \"{}\"", target.name);
    match target.agent {
        Agent::Claude => {
            let ptr = claude_server_pointer(homes, target, &file)?;
            simple_plan(homes, &file, Mutation::JsonDelete { pointer: ptr }, desc)
        }
        Agent::Codex => {
            if matches!(target.scope, Scope::Plugin { .. }) || is_mcp_json(&file) {
                return Err(AppError::read_only(
                    "plugin-provided servers are managed by the plugin",
                    &file,
                ));
            }
            simple_plan(
                homes,
                &file,
                Mutation::TomlDelete {
                    path: vec!["mcp_servers".into(), target.name.clone()],
                },
                desc,
            )
        }
    }
}

pub fn plan_set_mcp_enabled(
    homes: &Homes,
    target: &McpTarget,
    enabled: bool,
    project_root: Option<&str>,
) -> AppResult<WritePlan> {
    let file = PathBuf::from(&target.file);
    let verb = if enabled { "Enable" } else { "Disable" };
    match (target.agent, &target.scope) {
        (Agent::Codex, Scope::Plugin { plugin_id }) => {
            plan_set_plugin_enabled(homes, Agent::Codex, plugin_id, &Scope::User, enabled)
        }
        (Agent::Codex, _) => {
            let path = vec!["mcp_servers".into(), target.name.clone(), "enabled".into()];
            let mutation = if enabled {
                Mutation::TomlDelete { path }
            } else {
                Mutation::TomlSet {
                    path,
                    value: Value::Bool(false),
                }
            };
            simple_plan(
                homes,
                &file,
                mutation,
                format!("{verb} MCP server \"{}\"", target.name),
            )
        }
        (Agent::Claude, Scope::Project { root }) if is_mcp_json(&file) => {
            // Project .mcp.json approval lists live in ~/.claude.json under the project.
            let state = homes.claude_state.clone();
            let name = target.name.clone();
            let root = root.clone();
            let enabled_ptr = pointer(&["projects", &root, "enabledMcpjsonServers"]);
            let disabled_ptr = pointer(&["projects", &root, "disabledMcpjsonServers"]);
            let label = Mutation::JsonSet {
                pointer: enabled_ptr.clone(),
                value: Value::Bool(enabled),
            };
            json_edit_plan(
                homes,
                &state,
                format!("{verb} project MCP server \"{name}\" for {root}"),
                label,
                move |root_v| {
                    if enabled {
                        json_array_add(root_v, &enabled_ptr, &name)?;
                        json_array_remove(root_v, &disabled_ptr, &name)?;
                    } else {
                        json_array_add(root_v, &disabled_ptr, &name)?;
                        json_array_remove(root_v, &enabled_ptr, &name)?;
                    }
                    Ok(())
                },
            )
        }
        (Agent::Claude, Scope::Project { .. }) => Err(AppError::read_only(
            "local-scope servers have no disable switch; delete the server instead",
            &file,
        )),
        (Agent::Claude, scope) => {
            let Some(root) = project_root.filter(|r| !r.is_empty()) else {
                return Err(AppError::invalid(
                    "Claude Code disables user and plugin servers per project: choose a project first",
                ));
            };
            let id = match scope {
                Scope::Plugin { plugin_id } => {
                    format!("plugin:{}:{}", plugin_short_name(plugin_id), target.name)
                }
                _ => target.name.clone(),
            };
            let state = homes.claude_state.clone();
            let ptr = pointer(&["projects", root, "disabledMcpServers"]);
            let label = Mutation::JsonSet {
                pointer: ptr.clone(),
                value: Value::Bool(enabled),
            };
            let root_s = root.to_string();
            json_edit_plan(
                homes,
                &state,
                format!("{verb} \"{}\" in {root_s}", target.name),
                label,
                move |root_v| {
                    if enabled {
                        json_array_remove(root_v, &ptr, &id)?;
                    } else {
                        json_array_add(root_v, &ptr, &id)?;
                    }
                    Ok(())
                },
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Plugins
// ---------------------------------------------------------------------------

pub fn plan_set_plugin_enabled(
    homes: &Homes,
    agent: Agent,
    plugin_id: &str,
    scope: &Scope,
    enabled: bool,
) -> AppResult<WritePlan> {
    let verb = if enabled { "Enable" } else { "Disable" };
    let desc = format!("{verb} plugin {plugin_id}");
    match agent {
        Agent::Claude => {
            let file = match scope {
                Scope::Project { root } => {
                    PathBuf::from(root).join(".claude").join("settings.json")
                }
                _ => homes.claude_dir.join("settings.json"),
            };
            simple_plan(
                homes,
                &file,
                Mutation::JsonSet {
                    pointer: pointer(&["enabledPlugins", plugin_id]),
                    value: Value::Bool(enabled),
                },
                desc,
            )
        }
        Agent::Codex => {
            let file = homes.codex_home.join("config.toml");
            simple_plan(
                homes,
                &file,
                Mutation::TomlSet {
                    path: vec!["plugins".into(), plugin_id.to_string(), "enabled".into()],
                    value: Value::Bool(enabled),
                },
                desc,
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Skills (Codex deny-list) and frontmatter
// ---------------------------------------------------------------------------

fn path_from_id(id: &str, kind_key: &str) -> AppResult<PathBuf> {
    let prefix = format!("codex:{kind_key}:");
    id.strip_prefix(&prefix)
        .map(PathBuf::from)
        .ok_or_else(|| AppError::invalid(format!("not a Codex {kind_key} id: {id}")))
}

pub fn plan_set_skill_enabled(
    homes: &Homes,
    skill_id: &str,
    enabled: bool,
) -> AppResult<WritePlan> {
    if skill_id.starts_with("claude:") {
        return Err(AppError::invalid(
            "Claude Code has no per-skill enable switch",
        ));
    }
    let skill_path = path_from_id(skill_id, "skill")?;
    let text = read_text_opt(&skill_path)?
        .ok_or_else(|| AppError::not_found("skill file not found", &skill_path))?;
    let parsed = frontmatter::parse(&text);
    let name = frontmatter::field_str(&parsed.frontmatter, "name").or_else(|| {
        skill_path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
    });
    let config = homes.codex_home.join("config.toml");
    let verb = if enabled { "Enable" } else { "Disable" };
    simple_plan(
        homes,
        &config,
        Mutation::TomlSkillsConfig {
            skill_path: display(&skill_path),
            skill_name: name.clone(),
            enabled,
        },
        format!(
            "{verb} skill {}",
            name.unwrap_or_else(|| display(&skill_path))
        ),
    )
}

pub fn plan_save_frontmatter(
    homes: &Homes,
    path: &str,
    changes: Vec<FrontmatterChange>,
    body: Option<String>,
    base_hash: Option<String>,
) -> AppResult<WritePlan> {
    let p = PathBuf::from(path);
    build(PlanInput {
        homes,
        path: &p,
        mutation: Mutation::Frontmatter { changes, body },
        description: format!(
            "Update {}",
            p.file_name().and_then(|n| n.to_str()).unwrap_or(path)
        ),
        base_hash,
        text: None,
        mode: None,
        extra_files: vec![],
    })
}

// ---------------------------------------------------------------------------
// Automations
// ---------------------------------------------------------------------------

fn automation_patch_value(patch: &AutomationPatch) -> Value {
    let mut m = Map::new();
    if let Some(v) = &patch.name {
        m.insert("name".into(), Value::from(v.as_str()));
    }
    if let Some(v) = &patch.prompt {
        m.insert("prompt".into(), Value::from(v.as_str()));
    }
    if let Some(v) = &patch.status {
        m.insert("status".into(), Value::from(v.as_str()));
    }
    if let Some(v) = &patch.rrule {
        m.insert("rrule".into(), Value::from(v.as_str()));
    }
    if let Some(v) = &patch.model {
        m.insert(
            "model".into(),
            v.as_deref().map(Value::from).unwrap_or(Value::Null),
        );
    }
    if let Some(v) = &patch.reasoning_effort {
        m.insert(
            "reasoning_effort".into(),
            v.as_deref().map(Value::from).unwrap_or(Value::Null),
        );
    }
    if let Some(v) = &patch.execution_environment {
        m.insert(
            "execution_environment".into(),
            v.as_deref().map(Value::from).unwrap_or(Value::Null),
        );
    }
    if let Some(v) = &patch.cwds {
        m.insert(
            "cwds".into(),
            Value::Array(v.iter().map(|s| Value::from(s.as_str())).collect()),
        );
    }
    Value::Object(m)
}

pub fn plan_save_automation(
    homes: &Homes,
    id: &str,
    patch: &AutomationPatch,
) -> AppResult<WritePlan> {
    let path = path_from_id(id, "automation")?;
    simple_plan(
        homes,
        &path,
        Mutation::TomlAutomation {
            patch: automation_patch_value(patch),
        },
        "Update automation".to_string(),
    )
}

pub fn plan_set_automation_status(homes: &Homes, id: &str, status: &str) -> AppResult<WritePlan> {
    let status = status.trim().to_ascii_uppercase();
    if status != "ACTIVE" && status != "PAUSED" {
        return Err(AppError::invalid("status must be ACTIVE or PAUSED"));
    }
    let path = path_from_id(id, "automation")?;
    let verb = if status == "ACTIVE" {
        "Resume"
    } else {
        "Pause"
    };
    simple_plan(
        homes,
        &path,
        Mutation::TomlAutomation {
            patch: serde_json::json!({ "status": status }),
        },
        format!("{verb} automation"),
    )
}

pub fn local_project_id(path: &str) -> String {
    format!(
        "local-{}",
        &crate::fsutil::sha256_hex(path.as_bytes())[..32]
    )
}

pub fn render_new_automation(
    id: &str,
    input: &AutomationCreateInput,
    now_ms: i64,
) -> AppResult<String> {
    use toml_edit::{value, DocumentMut, InlineTable, Item};
    let mut doc = DocumentMut::new();
    doc["version"] = value(1);
    doc["id"] = Item::Value(writeplan::single_line_string(id));
    doc["kind"] = value("cron");
    doc["name"] = Item::Value(writeplan::single_line_string(&input.name));
    doc["prompt"] = Item::Value(writeplan::single_line_string(&input.prompt));
    doc["status"] = value("ACTIVE");
    doc["rrule"] = Item::Value(writeplan::single_line_string(&input.rrule));
    if let Some(m) = input.model.as_ref().filter(|m| !m.is_empty()) {
        doc["model"] = value(m.as_str());
    }
    if let Some(r) = input.reasoning_effort.as_ref().filter(|r| !r.is_empty()) {
        doc["reasoning_effort"] = value(r.as_str());
    }
    doc["execution_environment"] = value(input.execution_environment.as_str());
    let mut target = InlineTable::new();
    let cwds: Vec<String> = input
        .cwds
        .iter()
        .filter(|c| !c.trim().is_empty())
        .cloned()
        .collect();
    match cwds.first() {
        Some(first) if first != "~" => {
            target.insert("type", "project".into());
            target.insert("project_id", local_project_id(first).into());
        }
        _ => {
            target.insert("type", "projectless".into());
        }
    }
    doc["target"] = Item::Value(toml_edit::Value::InlineTable(target));
    let mut arr = toml_edit::Array::new();
    if cwds.is_empty() {
        arr.push("~");
    } else {
        for c in &cwds {
            arr.push(c.as_str());
        }
    }
    doc["cwds"] = value(arr);
    doc["created_at"] = value(now_ms);
    doc["updated_at"] = value(now_ms);
    Ok(doc.to_string())
}

pub fn plan_create_automation(
    homes: &Homes,
    input: &AutomationCreateInput,
) -> AppResult<WritePlan> {
    if input.name.trim().is_empty() {
        return Err(AppError::invalid("automation name is required"));
    }
    if !input.rrule.trim_start().starts_with("RRULE:") {
        return Err(AppError::invalid(
            "schedule must be an iCalendar RRULE string starting with RRULE:",
        ));
    }
    let base = homes.codex_home.join("automations");
    let slug = slugify(&input.name);
    let mut id = slug.clone();
    let mut n = 2;
    while base.join(&id).exists() {
        id = format!("{slug}-{n}");
        n += 1;
    }
    let dir = base.join(&id);
    let now = chrono::Utc::now().timestamp_millis();
    let text = render_new_automation(&id, input, now)?;
    let toml_path = dir.join("automation.toml");
    build(PlanInput {
        homes,
        path: &toml_path,
        mutation: Mutation::CreateFile,
        description: format!("Create automation \"{}\"", input.name),
        base_hash: None,
        text: Some(text),
        mode: Some(0o644),
        extra_files: vec![ExtraFile {
            path: display(&dir.join("memory.md")),
            text: String::new(),
            mode: Some(0o644),
        }],
    })
}

// ---------------------------------------------------------------------------
// New skills / agents / commands and deletion
// ---------------------------------------------------------------------------

pub fn plan_create_entity(
    homes: &Homes,
    kind: NewEntityKind,
    agent: Agent,
    scope: &Scope,
    name: &str,
) -> AppResult<WritePlan> {
    let slug = slugify(name);
    if slug == "untitled" && name.trim().is_empty() {
        return Err(AppError::invalid("name is required"));
    }
    let base: PathBuf = match (agent, scope) {
        (Agent::Claude, Scope::User) => homes.claude_dir.clone(),
        (Agent::Claude, Scope::Project { root }) => PathBuf::from(root).join(".claude"),
        (Agent::Codex, Scope::User) => homes.codex_home.clone(),
        (Agent::Codex, Scope::Project { root }) => PathBuf::from(root).join(".agents"),
        _ => {
            return Err(AppError::invalid(
                "entities can only be created at user or project scope",
            ))
        }
    };
    let (path, text) = match (kind, agent) {
        (NewEntityKind::Skill, _) => (
            base.join("skills").join(&slug).join("SKILL.md"),
            format!(
                "---\nname: {slug}\ndescription: Describe when this skill should be used.\n---\n\n# {name}\n\nInstructions go here.\n"
            ),
        ),
        (NewEntityKind::SubAgent, Agent::Claude) => (
            base.join("agents").join(format!("{slug}.md")),
            format!(
                "---\nname: {slug}\ndescription: Use this agent when …\nmodel: sonnet\n---\n\nYou are {name}. Describe the agent's job here.\n"
            ),
        ),
        (NewEntityKind::SubAgent, Agent::Codex) => (
            base.join("agents").join(format!("{slug}.toml")),
            format!(
                "name = \"{slug}\"\ndescription = \"Use this agent when …\"\ndeveloper_instructions = \"\"\"\nYou are {name}. Describe the agent's job here.\n\"\"\"\n"
            ),
        ),
        (NewEntityKind::SlashCommand, Agent::Claude) => (
            base.join("commands").join(format!("{slug}.md")),
            format!("---\ndescription: {name}\n---\n\nDescribe what /{slug} should do. Use $ARGUMENTS for input.\n"),
        ),
        (NewEntityKind::SlashCommand, Agent::Codex) => {
            return Err(AppError::invalid("Codex has no user-level slash commands; commands ship inside plugins"))
        }
    };
    build(PlanInput {
        homes,
        path: &path,
        mutation: Mutation::CreateFile,
        description: format!(
            "Create {}",
            path.file_name().and_then(|n| n.to_str()).unwrap_or("file")
        ),
        base_hash: None,
        text: Some(text),
        mode: Some(0o644),
        extra_files: vec![],
    })
}

pub fn plan_delete_path(homes: &Homes, path: &str) -> AppResult<WritePlan> {
    let p = PathBuf::from(path);
    if !p.exists() {
        return Err(AppError::not_found("path does not exist", &p));
    }
    writeplan::guard_protected(homes, &p, &Mutation::DeleteFile)?;
    build(PlanInput {
        homes,
        path: &p,
        mutation: Mutation::DeleteFile,
        description: format!(
            "Delete {}",
            p.file_name().and_then(|n| n.to_str()).unwrap_or(path)
        ),
        base_hash: None,
        text: None,
        mode: None,
        extra_files: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::fs;

    #[test]
    fn slugify_matches_codex_style_ids() {
        assert_eq!(
            slugify("🚀📝 CF-Linear-GitHub - Linear Deploy Updates"),
            "cf-linear-github-linear-deploy-updates"
        );
        assert_eq!(slugify("Stale GitHub branches"), "stale-github-branches");
        assert_eq!(slugify("   "), "untitled");
    }

    #[test]
    fn local_project_id_is_sha256_prefix_of_path() {
        assert_eq!(
            local_project_id("/Users/me/Documents/GitHub/my-repo"),
            "local-b65220693b3896ad8555b34b3ede0186"
        );
    }

    #[test]
    fn new_automation_renders_in_codex_key_order() {
        let input = AutomationCreateInput {
            name: "Daily digest 📰".into(),
            prompt: "Line 1\nLine 2".into(),
            rrule: "RRULE:FREQ=DAILY;BYHOUR=9;BYMINUTE=0".into(),
            model: Some("gpt-6".into()),
            reasoning_effort: None,
            execution_environment: "local".into(),
            cwds: vec!["/Users/me/Documents/GitHub/my-repo".into()],
        };
        let text = render_new_automation("daily-digest", &input, 1234).unwrap();
        assert_eq!(
            text,
            "version = 1\nid = \"daily-digest\"\nkind = \"cron\"\nname = \"Daily digest 📰\"\nprompt = \"Line 1\\nLine 2\"\nstatus = \"ACTIVE\"\nrrule = \"RRULE:FREQ=DAILY;BYHOUR=9;BYMINUTE=0\"\nmodel = \"gpt-6\"\nexecution_environment = \"local\"\ntarget = { type = \"project\", project_id = \"local-b65220693b3896ad8555b34b3ede0186\" }\ncwds = [\"/Users/me/Documents/GitHub/my-repo\"]\ncreated_at = 1234\nupdated_at = 1234\n"
        );
        let projectless = AutomationCreateInput {
            cwds: vec![],
            ..input
        };
        let text = render_new_automation("x", &projectless, 1).unwrap();
        assert!(text.contains("target = { type = \"projectless\" }\ncwds = [\"~\"]\n"));
    }

    #[test]
    fn claude_toggle_plans_edit_the_right_lists() {
        let dir = tempfile::tempdir().unwrap();
        let homes = Homes::for_test(dir.path());
        fs::write(
            &homes.claude_state,
            r#"{"mcpServers":{"workos":{"type":"http","url":"u"}},"projects":{"/p/a":{"lastCost":1.5,"disabledMcpServers":["workos"]}}}"#,
        )
        .unwrap();
        let target = McpTarget {
            agent: Agent::Claude,
            scope: Scope::User,
            file: display(&homes.claude_state),
            name: "workos".into(),
        };
        // Re-enable in project /p/a: list becomes empty and is dropped.
        let plan = plan_set_mcp_enabled(&homes, &target, true, Some("/p/a")).unwrap();
        let v: Value = serde_json::from_str(plan.new_text.as_deref().unwrap()).unwrap();
        assert!(v["projects"]["/p/a"].get("disabledMcpServers").is_none());
        assert_eq!(v["projects"]["/p/a"]["lastCost"], 1.5);
        // Plugin server disabled per project uses the plugin:<plugin>:<server> id.
        let plug = McpTarget {
            agent: Agent::Claude,
            scope: Scope::Plugin {
                plugin_id: "linear@claude-plugins-official".into(),
            },
            file: "/x/.mcp.json".into(),
            name: "linear".into(),
        };
        let plan = plan_set_mcp_enabled(&homes, &plug, false, Some("/p/a")).unwrap();
        let v: Value = serde_json::from_str(plan.new_text.as_deref().unwrap()).unwrap();
        assert_eq!(
            v["projects"]["/p/a"]["disabledMcpServers"],
            serde_json::json!(["workos", "plugin:linear:linear"])
        );
        // Project .mcp.json approval moves between the two lists.
        let proj = McpTarget {
            agent: Agent::Claude,
            scope: Scope::Project {
                root: "/p/a".into(),
            },
            file: "/p/a/.mcp.json".into(),
            name: "cf".into(),
        };
        let plan = plan_set_mcp_enabled(&homes, &proj, true, None).unwrap();
        let v: Value = serde_json::from_str(plan.new_text.as_deref().unwrap()).unwrap();
        assert_eq!(
            v["projects"]["/p/a"]["enabledMcpjsonServers"],
            serde_json::json!(["cf"])
        );
        assert!(
            plan_set_mcp_enabled(&homes, &target, false, None).is_err(),
            "needs a project"
        );
    }

    #[test]
    fn codex_upsert_creates_and_updates_server_tables() {
        let dir = tempfile::tempdir().unwrap();
        let homes = Homes::for_test(dir.path());
        let cfg = homes.codex_home.join("config.toml");
        fs::create_dir_all(&homes.codex_home).unwrap();
        fs::write(
            &cfg,
            "model = \"m\"\n\n[mcp_servers.docs]\nurl = \"https://d\"\nenabled = false\n",
        )
        .unwrap();
        let target = McpTarget {
            agent: Agent::Codex,
            scope: Scope::User,
            file: display(&cfg),
            name: "chrome".into(),
        };
        let mut env = Map::new();
        env.insert("FOO".into(), Value::from("bar"));
        let input = McpServerInput {
            transport: TransportInput::Stdio {
                command: "npx".into(),
                args: vec!["cdm@latest".into()],
                env,
                cwd: None,
            },
            enabled: Some(true),
            startup_timeout_sec: Some(120.0),
            tool_timeout_sec: None,
            extra: Map::new(),
        };
        let plan = plan_upsert_mcp(&homes, &target, &input).unwrap();
        let text = plan.new_text.clone().unwrap();
        assert!(text.contains("[mcp_servers.chrome]\ncommand = \"npx\"\nargs = [\"cdm@latest\"]\nstartup_timeout_sec = 120\n\n[mcp_servers.chrome.env]\nFOO = \"bar\"\n"), "{text}");
        assert!(!text.contains("enabled = true"));
        // Update the existing `docs` server: enabling removes the key; timeouts set in place.
        let docs = McpTarget {
            name: "docs".into(),
            ..target.clone()
        };
        let upd = McpServerInput {
            transport: TransportInput::Http {
                url: "https://d2".into(),
                headers: Map::new(),
                bearer_token_env_var: Some("TOK".into()),
            },
            enabled: Some(true),
            startup_timeout_sec: None,
            tool_timeout_sec: Some(30.0),
            extra: Map::new(),
        };
        let plan = plan_upsert_mcp(&homes, &docs, &upd).unwrap();
        let text = plan.new_text.unwrap();
        assert!(text.contains("[mcp_servers.docs]\nurl = \"https://d2\"\nbearer_token_env_var = \"TOK\"\ntool_timeout_sec = 30\n"), "{text}");
        assert!(!text.contains("enabled"));
        // Delete.
        let plan = plan_delete_mcp(&homes, &docs).unwrap();
        assert!(!plan.new_text.unwrap().contains("docs"));
    }
}
