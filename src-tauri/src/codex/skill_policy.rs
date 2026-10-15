//! Read and losslessly edit the invocation policy in a skill's `agents/openai.yaml`.

use crate::error::{AppError, AppResult};
use crate::fsutil::{display, read_text_opt, sha256_hex};
use crate::model::CodexSkillInvocation;
use serde_yaml_ng::{Mapping, Value};
use std::collections::BTreeSet;
use std::path::Path;

fn root(text: &str, path: &Path) -> AppResult<Mapping> {
    match serde_yaml_ng::from_str::<Value>(text) {
        Ok(Value::Mapping(map)) => Ok(map),
        Ok(Value::Null) => Ok(Mapping::new()),
        _ => Err(AppError::parse(
            "Expected a YAML mapping in agents/openai.yaml",
            path,
        )),
    }
}

fn implicit_invocation(map: &Mapping, path: &Path) -> AppResult<bool> {
    match map.get(Value::String("policy".into())) {
        None | Some(Value::Null) => Ok(true),
        Some(Value::Mapping(policy)) => {
            match policy.get(Value::String("allow_implicit_invocation".into())) {
                None => Ok(true),
                Some(Value::Bool(allowed)) => Ok(*allowed),
                _ => Err(AppError::parse(
                    "policy.allow_implicit_invocation must be a boolean",
                    path,
                )),
            }
        }
        _ => Err(AppError::parse("policy must be a YAML mapping", path)),
    }
}

pub fn read(path: &Path) -> AppResult<CodexSkillInvocation> {
    let text = read_text_opt(path)?;
    let map = root(text.as_deref().unwrap_or(""), path)?;
    Ok(CodexSkillInvocation {
        path: display(path),
        base_hash: text.as_ref().map(|t| sha256_hex(t.as_bytes())),
        allow_implicit_invocation: implicit_invocation(&map, path)?,
    })
}

fn verifies(text: &str, expected: &Value) -> bool {
    serde_yaml_ng::from_str::<Value>(text).is_ok_and(|value| &value == expected)
}

fn replace_token(
    base: &str,
    tokens: &[&str],
    replacement: &str,
    expected: &Value,
) -> Option<String> {
    for token in tokens {
        for (start, _) in base.match_indices(token) {
            let end = start + token.len();
            let word = |c: char| c.is_alphanumeric() || c == '_';
            if base[..start].chars().next_back().is_some_and(word)
                || base[end..].chars().next().is_some_and(word)
            {
                continue;
            }
            let mut candidate = base.to_string();
            candidate.replace_range(start..end, replacement);
            if verifies(&candidate, expected) {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn set_implicit_invocation(base: &str, allowed: bool, path: &Path) -> AppResult<String> {
    let mut map = root(base, path)?;
    let current = implicit_invocation(&map, path)?;
    let policy_key = Value::String("policy".into());
    let allow_key = Value::String("allow_implicit_invocation".into());
    let has_policy = map.contains_key(&policy_key);
    let has_value = map
        .get(&policy_key)
        .and_then(Value::as_mapping)
        .is_some_and(|p| p.contains_key(&allow_key));
    if has_value && current == allowed {
        return Ok(base.to_string());
    }
    let policy = map
        .entry(policy_key)
        .or_insert(Value::Mapping(Mapping::new()));
    if policy.is_null() {
        *policy = Value::Mapping(Mapping::new());
    }
    policy
        .as_mapping_mut()
        .ok_or_else(|| AppError::parse("policy must be a YAML mapping", path))?
        .insert(allow_key, Value::Bool(allowed));
    let expected = Value::Mapping(map);
    let value = allowed.to_string();

    // YAML is parsed for meaning, never re-serialized. Accept a textual splice only if
    // reparsing changes exactly the requested value. This protects other booleans,
    // comments, quoted strings, block scalars, aliases and arbitrary unknown metadata.
    if has_value {
        return replace_token(
            base,
            &["true", "false", "True", "False", "TRUE", "FALSE"],
            &value,
            &expected,
        )
        .ok_or_else(|| {
            AppError::invalid(
                "Cannot safely edit this YAML layout; edit agents/openai.yaml in the Files tab",
            )
        });
    }

    let entry = format!("allow_implicit_invocation: {value}");
    let flow_entry = if has_policy {
        entry.clone()
    } else {
        format!("policy: {{ {entry} }}")
    };
    if let Some(candidate) = replace_token(
        base,
        &["null", "Null", "NULL", "~"],
        &format!("{{ {flow_entry} }}"),
        &expected,
    ) {
        return Ok(candidate);
    }
    // Flow mappings (including empty mappings and quoted keys).
    for (offset, _) in base.match_indices('}') {
        for separator in [", ", ""] {
            let mut candidate = base.to_string();
            candidate.insert_str(offset, &format!("{separator}{flow_entry}"));
            if verifies(&candidate, &expected) {
                return Ok(candidate);
            }
        }
    }

    let newline = if base.contains("\r\n") { "\r\n" } else { "\n" };
    let entry = if has_policy {
        entry
    } else {
        format!("policy:\n  {entry}")
    };
    let mut offsets = vec![0];
    let mut indents = BTreeSet::from([0, 2]);
    let mut offset = 0;
    for line in base.split_inclusive('\n') {
        let indent = line.bytes().take_while(|b| *b == b' ').count();
        indents.extend([indent, indent + 2]);
        offset += line.len();
        offsets.push(offset);
    }
    for offset in offsets.into_iter().rev() {
        for indent in &indents {
            let prefix = if offset > 0 && !base[..offset].ends_with('\n') {
                newline
            } else {
                ""
            };
            let insertion: String = entry
                .lines()
                .map(|line| format!("{}{line}{newline}", " ".repeat(*indent)))
                .collect();
            let mut candidate = base.to_string();
            candidate.insert_str(offset, &format!("{prefix}{insertion}"));
            if verifies(&candidate, &expected) {
                return Ok(candidate);
            }
        }
    }
    Err(AppError::invalid(
        "Cannot safely edit this YAML layout; edit agents/openai.yaml in the Files tab",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(base: &str, allowed: bool) -> String {
        set_implicit_invocation(base, allowed, Path::new("agents/openai.yaml")).unwrap()
    }

    #[test]
    fn preserves_comments_unknown_fields_and_line_endings() {
        let base = "# header\r\ninterface:\r\n  display_name: 'true false'\r\npolicy:\r\n  other: true\r\n  allow_implicit_invocation: true # keep comment\r\ndependencies: { tools: [] }\r\n";
        let changed = set(base, false);
        assert_eq!(
            changed,
            base.replace("invocation: true", "invocation: false")
        );
        assert_eq!(set(&changed, true), base);
        assert_eq!(set(base, true), base);
    }

    #[test]
    fn adds_missing_keys_to_block_and_flow_mappings() {
        for base in [
            "",
            "# metadata\n",
            "---\ninterface: {display_name: Demo}\n...\n",
            "policy:\n  other: true\ninterface: {}\n",
            "'policy': {other: true}\n",
            "policy: {}\n",
            "{interface: {display_name: Demo}}\n",
            "policy: null # retain\n",
            "policy: # retain\n",
            "null\n",
        ] {
            let changed = set(base, false);
            assert!(!implicit_invocation(
                &root(&changed, Path::new("test")).unwrap(),
                Path::new("test")
            )
            .unwrap());
            assert_eq!(
                set(&changed, true),
                changed.replace(
                    "allow_implicit_invocation: false",
                    "allow_implicit_invocation: true"
                )
            );
        }
    }

    #[test]
    fn edits_quoted_keys_and_does_not_touch_boolean_text_in_strings() {
        let base = "interface:\n  default_prompt: |\n    Use true or false.\n\"policy\": {\"allow_implicit_invocation\": FALSE, other: true} # keep\n";
        assert_eq!(set(base, true), base.replace(": FALSE", ": true"));
    }

    #[test]
    fn rejects_invalid_yaml_and_non_boolean_policy() {
        for base in [
            "policy: [",
            "policy: bad",
            "policy: {allow_implicit_invocation: 'false'}",
            "policy: {allow_implicit_invocation: null}",
        ] {
            assert!(set_implicit_invocation(base, false, Path::new("test")).is_err());
        }
    }

    #[test]
    fn missing_file_defaults_to_automatic_use() {
        let dir = tempfile::tempdir().unwrap();
        let state = read(&dir.path().join("agents/openai.yaml")).unwrap();
        assert!(state.allow_implicit_invocation);
        assert_eq!(state.base_hash, None);
    }
}
