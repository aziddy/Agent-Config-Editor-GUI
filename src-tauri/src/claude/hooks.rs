//! Hooks live inside settings files:
//! `hooks: { "<Event>": [ { matcher?, hooks: [ { type, command, timeout? } ] } ] }`.
//!
//! One [`HookEvent`] is emitted per (scope, event). Entries from `settings.json` and
//! `settings.local.json` of the same scope are flattened into that one event; each entry's
//! origin points at its own file with the JSON pointer `/hooks/<Event>/<i>/hooks/<j>`.
//! Claude Code has no trust registry for hooks, so `trust_entry_present` is always `None`.

use super::settings::{self, SettingsFile, SettingsSet};
use super::{config_id, pointer_locator, str_field, value_kind, AGENT};
use crate::model::{EntityKind, HookEntry, HookEvent};
use crate::scan::ScanOut;
use serde_json::Value;

/// Emit every hook event found across `settings`.
pub fn scan(settings: &SettingsSet, out: &mut ScanOut) {
    let mut events: Vec<HookEvent> = Vec::new();
    for file in &settings.files {
        for (event, entries) in entries_from_file(settings, file, out) {
            let existing = events
                .iter_mut()
                .find(|e| e.scope == file.scope && e.event == event);
            match existing {
                Some(e) => e.entries.extend(entries),
                None => events.push(HookEvent {
                    id: config_id(EntityKind::Hook, &file.scope, &event),
                    agent: AGENT,
                    scope: file.scope.clone(),
                    event,
                    entries,
                }),
            }
        }
    }
    out.hooks.extend(events);
}

/// Flatten the `hooks` object of one settings file into `(event, entries)` pairs, in file order.
pub fn entries_from_file(
    settings: &SettingsSet,
    file: &SettingsFile,
    out: &mut ScanOut,
) -> Vec<(String, Vec<HookEntry>)> {
    let Some(hooks) = settings::hooks(&file.value) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for (event, groups) in hooks {
        let Some(groups) = groups.as_array() else {
            out.warn(
                Some(&file.path),
                format!(
                    "/hooks/{event} should be an array, got {}",
                    value_kind(groups)
                ),
            );
            continue;
        };
        let mut entries = Vec::new();
        for (i, group) in groups.iter().enumerate() {
            let Some(group) = group.as_object() else {
                out.warn(
                    Some(&file.path),
                    format!("/hooks/{event}/{i} should be an object"),
                );
                continue;
            };
            let matcher = str_field(group, "matcher");
            let Some(inner) = group.get("hooks").and_then(Value::as_array) else {
                out.warn(
                    Some(&file.path),
                    format!("/hooks/{event}/{i}/hooks is missing or not an array"),
                );
                continue;
            };
            for (j, hook) in inner.iter().enumerate() {
                let Some(hook) = hook.as_object() else {
                    out.warn(
                        Some(&file.path),
                        format!("/hooks/{event}/{i}/hooks/{j} should be an object"),
                    );
                    continue;
                };
                let i_s = i.to_string();
                let j_s = j.to_string();
                entries.push(HookEntry {
                    matcher: matcher.clone(),
                    hook_type: str_field(hook, "type").unwrap_or_else(|| "command".to_string()),
                    // `prompt`-type hooks carry their text under `prompt` instead of `command`.
                    command: str_field(hook, "command")
                        .or_else(|| str_field(hook, "prompt"))
                        .unwrap_or_default(),
                    timeout: hook.get("timeout").and_then(Value::as_f64),
                    origin: settings.origin_in(
                        file,
                        pointer_locator(&["hooks", event, &i_s, "hooks", &j_s]),
                    ),
                    trust_entry_present: None,
                });
            }
        }
        if !entries.is_empty() {
            result.push((event.clone(), entries));
        }
    }
    result
}
