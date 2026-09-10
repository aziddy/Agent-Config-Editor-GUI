//! Codex hooks: `<codex_home>/hooks.json`.
//!
//! Observed shape (codex-cli 0.153.x) wraps the events under a top-level `"hooks"` key:
//! `{ "hooks": { "<PascalCaseEvent>": [ { "matcher"?, "hooks": [ { "type", "command", "timeout" } ] } ] } }`.
//! A bare `{ "<Event>": [...] }` map is accepted too. Each hook's trust state is recorded in
//! `config.toml` as `[hooks.state."<hooks.json path>:<snake_case_event>:<i>:<j>"]`; only the
//! presence of that entry is reported because the hash algorithm is not public.

use super::config_toml::CodexConfig;
use super::plugins::json_pointer_segment;
use crate::fsutil::{self, read_text_opt};
use crate::model::{Agent, EntityKind, HookEntry, HookEvent, Locator, Origin, Scope};
use crate::scan::{ScanCtx, ScanOut};
use serde_json::Value;

pub fn scan(ctx: &ScanCtx<'_>, config: &CodexConfig, out: &mut ScanOut) {
    let path = ctx.homes.codex_home.join("hooks.json");
    let text = match read_text_opt(&path) {
        Ok(Some(t)) => t,
        Ok(None) => return,
        Err(e) => {
            out.warn(Some(&path), e.message);
            return;
        }
    };
    out.record_hash(&path, &text);
    let root = match serde_json::from_str::<Value>(&text) {
        Ok(v) => v,
        Err(e) => {
            out.warn(Some(&path), format!("JSON parse error: {e}"));
            return;
        }
    };
    let Some(root_obj) = root.as_object() else {
        out.warn(Some(&path), "hooks.json is not a JSON object");
        return;
    };
    let (events, prefix) = match root_obj.get("hooks") {
        Some(Value::Object(inner)) => (inner, "/hooks"),
        _ => (root_obj, ""),
    };

    let path_str = fsutil::display(&path);
    for (event, groups) in events {
        let Some(groups) = groups.as_array() else {
            continue;
        };
        let mut entries = Vec::new();
        for (i, group) in groups.iter().enumerate() {
            let matcher = group
                .get("matcher")
                .and_then(Value::as_str)
                .map(String::from);
            let Some(hooks) = group.get("hooks").and_then(Value::as_array) else {
                continue;
            };
            for (j, hook) in hooks.iter().enumerate() {
                let hook_type = hook
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("command")
                    .to_string();
                let command = match hook.get("command").and_then(Value::as_str) {
                    Some(c) => c.to_string(),
                    // `mcp_tool` hooks (seen in plugin manifests) carry `server` + `tool` instead.
                    None => match (
                        hook.get("server").and_then(Value::as_str),
                        hook.get("tool").and_then(Value::as_str),
                    ) {
                        (Some(server), Some(tool)) => format!("mcp:{server}/{tool}"),
                        _ => String::new(),
                    },
                };
                let trust_key = format!("{path_str}:{}:{i}:{j}", snake_case(event));
                entries.push(HookEntry {
                    matcher: matcher.clone(),
                    hook_type,
                    command,
                    timeout: hook.get("timeout").and_then(Value::as_f64),
                    origin: Origin {
                        agent: Agent::Codex,
                        scope: Scope::User,
                        file: path_str.clone(),
                        locator: Locator::JsonPointer {
                            pointer: format!(
                                "{prefix}/{}/{i}/hooks/{j}",
                                json_pointer_segment(event)
                            ),
                        },
                    },
                    trust_entry_present: Some(config.hook_trust_keys.contains(&trust_key)),
                });
            }
        }
        out.hooks.push(HookEvent {
            id: format!(
                "codex:{}:{}:{}",
                EntityKind::Hook.key(),
                Scope::User.key(),
                event
            ),
            agent: Agent::Codex,
            scope: Scope::User,
            event: event.clone(),
            entries,
        });
    }
}

/// `PreToolUse` → `pre_tool_use`, the form Codex uses in `[hooks.state."…"]` keys.
pub fn snake_case(pascal: &str) -> String {
    let mut out = String::with_capacity(pascal.len() + 4);
    for (i, ch) in pascal.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_to_snake() {
        assert_eq!(snake_case("PreToolUse"), "pre_tool_use");
        assert_eq!(snake_case("Stop"), "stop");
        assert_eq!(snake_case("UserPromptSubmit"), "user_prompt_submit");
        assert_eq!(snake_case("SessionStart"), "session_start");
        assert_eq!(snake_case("already_snake"), "already_snake");
    }
}
