//! Normalize only visible conversation/activity fields. Reasoning and provider context are omitted.

use crate::error::{AppError, AppResult};
use crate::model::{AutomationRunActivity, RunItemKind};
use serde_json::{json, Value};

pub(super) const TEXT_CHUNK: usize = 64 * 1024;

pub(super) fn type_key(value: &Value) -> String {
    value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .replace('_', "")
        .to_lowercase()
}

pub(super) fn item_kind(value: &Value) -> Option<RunItemKind> {
    if value
        .get("phase")
        .and_then(Value::as_str)
        .or_else(|| value.get("channel").and_then(Value::as_str))
        == Some("analysis")
    {
        return None;
    }
    match type_key(value).as_str() {
        "reasoning" | "contextcompaction" | "compaction" => None,
        "usermessage" => Some(RunItemKind::User),
        "agentmessage" => Some(RunItemKind::Assistant),
        _ => Some(RunItemKind::Activity),
    }
}

pub(super) fn label(item_type: &str) -> &'static str {
    match item_type.replace('_', "").to_lowercase().as_str() {
        "usermessage" => "You",
        "agentmessage" => "Codex",
        "commandexecution" => "Ran command",
        "filechange" => "Edited files",
        "mcptoolcall" | "dynamictoolcall" | "functioncall" => "Used tool",
        "functioncalloutput" => "Tool output",
        "subagentactivity" | "collabagenttoolcall" => "Agent activity",
        "websearch" => "Searched the web",
        "imageview" | "imagegeneration" => "Image activity",
        _ => "Unsupported activity",
    }
}

pub(super) fn message_text(value: &Value) -> String {
    if let Some(text) = value.get("text").and_then(Value::as_str) {
        return text.to_string();
    }
    value
        .get("content")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| {
                    if let Some(text) = part.get("text").and_then(Value::as_str) {
                        // These are injected user-role context, rather than the recorded automation prompt.
                        if value.get("protocolContext").and_then(Value::as_bool) == Some(true)
                            && (text
                                .trim_start()
                                .starts_with("# AGENTS.md instructions for ")
                                || text.trim_start().starts_with("<environment_context>"))
                        {
                            return None;
                        }
                        Some(text.to_string())
                    } else {
                        Some("[Attachment]".to_string())
                    }
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        })
        .unwrap_or_default()
}

// Mirrors the secret-key policy in src/lib/mask.ts for structured output before pagination.
fn secret_key(key: &str) -> bool {
    let key = key.to_lowercase();
    ![
        "allowed-tools",
        "allowed_tools",
        "hotkey",
        "keymap",
        "keybind",
    ]
    .iter()
    .any(|prefix| key.starts_with(prefix))
        && [
            "token",
            "secret",
            "key",
            "password",
            "passwd",
            "auth",
            "credential",
            "bearer",
        ]
        .iter()
        .any(|word| key.contains(word))
}

fn masked(value: &Value) -> Value {
    if value
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| matches!(kind, "image" | "audio" | "input_image" | "input_audio"))
    {
        return json!({"type":value.get("type"),"attachment":"Recorded media attachment"});
    }
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        if secret_key(key) && !value.is_null() {
                            Value::String("••••••".to_string())
                        } else {
                            masked(value)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(masked).collect()),
        Value::String(text) => {
            if let Ok(json @ (Value::Object(_) | Value::Array(_))) =
                serde_json::from_str::<Value>(text)
            {
                Value::String(serde_json::to_string(&masked(&json)).unwrap_or_default())
            } else {
                value.clone()
            }
        }
        _ => value.clone(),
    }
}

fn output(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => serde_json::from_str::<Value>(text)
            .ok()
            .map(|json| output(&json))
            .unwrap_or_else(|| text.clone()),
        _ => serde_json::to_string_pretty(&masked(value)).unwrap_or_default(),
    }
}

fn first<'a>(value: &'a Value, names: &[&str]) -> &'a Value {
    names
        .iter()
        .find_map(|name| value.get(*name).filter(|v| !v.is_null()))
        .unwrap_or(&Value::Null)
}

fn selected(value: &Value, fields: &[&str]) -> Value {
    Value::Object(
        fields
            .iter()
            .filter_map(|field| value.get(*field).map(|v| ((*field).to_string(), masked(v))))
            .collect(),
    )
}

fn bound_details(value: &mut Value) {
    match value {
        Value::String(text) if text.len() > 2048 => {
            let mut end = 2048;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
            text.push_str("… (full text below)");
        }
        Value::Object(map) => map.values_mut().for_each(bound_details),
        Value::Array(items) => {
            items.truncate(50);
            items.iter_mut().for_each(bound_details);
        }
        _ => {}
    }
}

pub(super) fn body(value: &Value) -> (Value, String) {
    if matches!(
        item_kind(value),
        Some(RunItemKind::User | RunItemKind::Assistant)
    ) {
        return (json!({}), message_text(value));
    }
    let (mut details, text) = match type_key(value).as_str() {
        "commandexecution" => {
            let command = first(value, &["command"]);
            let command = if let Some(parts) = command.as_array() {
                parts
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                command.as_str().unwrap_or_default().to_string()
            };
            let mut details = selected(
                value,
                &[
                    "cwd",
                    "status",
                    "exitCode",
                    "exit_code",
                    "durationMs",
                    "duration",
                ],
            );
            details["command"] = Value::String(command.clone());
            let mut stdout = output(first(
                value,
                &["aggregatedOutput", "aggregated_output", "formatted_output"],
            ));
            if stdout.is_empty() {
                stdout = [
                    output(first(value, &["stdout"])),
                    output(first(value, &["stderr"])),
                ]
                .into_iter()
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            }
            (details, format!("$ {command}\n\n{stdout}"))
        }
        "filechange" => {
            let changes = value.get("changes").unwrap_or(&Value::Null);
            let mut text = String::new();
            match changes {
                Value::Object(files) => {
                    for (path, change) in files {
                        text.push_str(&format!(
                            "{path}\n{}\n\n",
                            output(first(change, &["unified_diff", "diff"]))
                        ));
                    }
                }
                Value::Array(files) => {
                    for change in files {
                        let path = change
                            .get("path")
                            .and_then(Value::as_str)
                            .unwrap_or("Changed file");
                        text.push_str(&format!(
                            "{path}\n{}\n\n",
                            output(first(change, &["diff", "unified_diff"]))
                        ));
                    }
                }
                _ => {}
            }
            (selected(value, &["status"]), text)
        }
        "mcptoolcall" | "dynamictoolcall" | "functioncall" | "functioncalloutput" => {
            let details = selected(
                value,
                &[
                    "name",
                    "namespace",
                    "server",
                    "tool",
                    "status",
                    "durationMs",
                    "duration",
                ],
            );
            let arguments = output(first(value, &["arguments", "input"]));
            let result = output(first(value, &["result", "output", "content", "error"]));
            let text = if arguments.is_empty() {
                result
            } else {
                format!("Arguments\n{arguments}\n\nOutput\n{result}")
            };
            (details, text)
        }
        "subagentactivity" | "collabagenttoolcall" => {
            let details = selected(
                value,
                &[
                    "kind",
                    "agent_path",
                    "agent_thread_id",
                    "agentPath",
                    "agentThreadId",
                    "status",
                    "senderThreadId",
                    "receiverThreadIds",
                ],
            );
            let text = output(&details);
            (details, text)
        }
        "websearch" | "imageview" | "imagegeneration" => {
            let details = selected(value, &["query", "queries", "action", "path", "status"]);
            let text = output(&details);
            (details, text)
        }
        _ => (
            json!({"type": value.get("type")}),
            "This recorded activity format cannot be displayed yet.".to_string(),
        ),
    };
    bound_details(&mut details);
    (details, text)
}

pub(super) fn chunk(
    details: Value,
    text: &str,
    cursor: Option<&str>,
) -> AppResult<AutomationRunActivity> {
    let start = cursor
        .map(|s| {
            s.parse::<usize>()
                .map_err(|_| AppError::invalid("Invalid activity cursor"))
        })
        .transpose()?
        .unwrap_or(0);
    if start > text.len() || !text.is_char_boundary(start) {
        return Err(AppError::invalid(
            "Activity changed; reopen it to reload the output",
        ));
    }
    let mut end = start.saturating_add(TEXT_CHUNK).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok(AutomationRunActivity {
        details,
        text: text[start..end].to_string(),
        next_cursor: (end < text.len()).then(|| end.to_string()),
    })
}

/// Translate legacy public protocol records to the same visible item vocabulary.
pub(super) fn protocol_item(payload: &Value) -> Option<Value> {
    if payload.get("channel").and_then(Value::as_str) == Some("analysis") {
        return None;
    }
    match payload.get("type").and_then(Value::as_str)? {
        "message" => match payload.get("role").and_then(Value::as_str)? {
            "user" => Some(
                json!({"type":"userMessage", "content":payload.get("content"), "id":payload.get("id"), "protocolContext":true}),
            ),
            "assistant" => Some(
                json!({"type":"agentMessage", "content":payload.get("content"), "phase":payload.get("phase"), "id":payload.get("id")}),
            ),
            _ => None,
        },
        "function_call" | "custom_tool_call" => Some(
            json!({"type":"functionCall", "name":payload.get("name"), "arguments":first(payload, &["arguments", "input"]), "id":first(payload, &["call_id", "id"])}),
        ),
        "function_call_output" | "custom_tool_call_output" => Some(
            json!({"type":"functionCallOutput", "output":payload.get("output"), "id":first(payload, &["call_id", "id"])}),
        ),
        _ => None,
    }
}

pub(super) fn legacy_event(payload: &Value) -> Option<Value> {
    let item_type = match payload.get("type").and_then(Value::as_str)? {
        "user_message" => "userMessage",
        "agent_message" => "agentMessage",
        _ => return None,
    };
    Some(json!({"type":item_type, "text":payload.get("message"), "phase":payload.get("phase")}))
}
