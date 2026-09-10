//! YAML frontmatter parsing for SKILL.md, agent and command files.
//!
//! Parsing is lenient: a file with no leading `---` fence has `present == false`;
//! a fence whose YAML fails to parse keeps the raw text and reports a warning.

use crate::model::Frontmatter;
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedMarkdown {
    pub frontmatter: Frontmatter,
    /// Everything after the closing fence (or the whole file when absent).
    pub body: String,
    /// Byte range of the whole fenced block including both fences and the trailing newline.
    pub fence_span: Option<(usize, usize)>,
    pub warning: Option<String>,
}

fn line_is_fence(line: &str) -> bool {
    let t = line.trim_end_matches('\r');
    t == "---" || t == "..."
}

/// Split and parse. Never fails.
pub fn parse(text: &str) -> ParsedMarkdown {
    let first_line_end = text.find('\n').unwrap_or(text.len());
    let first_line = &text[..first_line_end];
    if first_line.trim_end_matches('\r') != "---" {
        return ParsedMarkdown {
            frontmatter: Frontmatter::default(),
            body: text.to_string(),
            fence_span: None,
            warning: None,
        };
    }
    // Scan subsequent lines for the closing fence.
    let mut pos = first_line_end + 1;
    let raw_start = pos.min(text.len());
    while pos <= text.len() {
        let rest = &text[pos..];
        let (line, next) = match rest.find('\n') {
            Some(i) => (&rest[..i], pos + i + 1),
            None => (rest, text.len() + 1),
        };
        if line_is_fence(line) {
            let raw = &text[raw_start..pos];
            let raw = raw.strip_suffix('\n').unwrap_or(raw);
            let span_end = next.min(text.len());
            let body = text[span_end..].to_string();
            let (fields, warning) = parse_yaml_fields(raw);
            return ParsedMarkdown {
                frontmatter: Frontmatter {
                    fields,
                    raw: raw.to_string(),
                    present: true,
                },
                body,
                fence_span: Some((0, span_end)),
                warning,
            };
        }
        if next > text.len() {
            break;
        }
        pos = next;
    }
    // Opening fence without a closing one: treat as no frontmatter.
    ParsedMarkdown {
        frontmatter: Frontmatter::default(),
        body: text.to_string(),
        fence_span: None,
        warning: Some("frontmatter opening fence has no closing fence".to_string()),
    }
}

fn parse_yaml_fields(raw: &str) -> (Map<String, Value>, Option<String>) {
    if raw.trim().is_empty() {
        return (Map::new(), None);
    }
    match serde_yaml_ng::from_str::<Value>(raw) {
        Ok(Value::Object(map)) => (map, None),
        Ok(Value::Null) => (Map::new(), None),
        Ok(other) => (
            Map::new(),
            Some(format!(
                "frontmatter is not a mapping (got {})",
                value_kind(&other)
            )),
        ),
        Err(e) => (Map::new(), Some(format!("frontmatter YAML error: {e}"))),
    }
}

fn value_kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// `allowed-tools` and friends appear both as `"A, B"` and `["A", "B"]`.
pub fn string_or_list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::String(s)) => s
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|i| match i {
                Value::String(s) => s.trim().to_string(),
                other => other.to_string(),
            })
            .filter(|p| !p.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

pub fn field_str(fm: &Frontmatter, key: &str) -> Option<String> {
    match fm.fields.get(key) {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Bool(b)) => Some(b.to_string()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

pub fn field_bool(fm: &Frontmatter, key: &str) -> bool {
    match fm.fields.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => matches!(s.trim().to_ascii_lowercase().as_str(), "true" | "yes"),
        _ => false,
    }
}

/// First `max_chars` of the body with whitespace collapsed, for list previews.
pub fn preview(body: &str, max_chars: usize) -> String {
    let collapsed: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut out: String = collapsed.chars().take(max_chars).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parses_folded_description_and_string_allowed_tools() {
        let text = "---\nname: use-yaak\ndescription: >\n  Build and run HTTP API requests\n  with the Yaak CLI.\nallowed-tools: Bash(yaak:*), Bash(which:*)\n---\n# Body\n\nHello\n";
        let p = parse(text);
        assert!(p.frontmatter.present);
        assert_eq!(p.warning, None);
        assert_eq!(
            field_str(&p.frontmatter, "name").as_deref(),
            Some("use-yaak")
        );
        assert_eq!(
            field_str(&p.frontmatter, "description").as_deref(),
            Some("Build and run HTTP API requests with the Yaak CLI.\n")
        );
        assert_eq!(
            string_or_list(p.frontmatter.fields.get("allowed-tools")),
            vec!["Bash(yaak:*)", "Bash(which:*)"]
        );
        assert_eq!(p.body, "# Body\n\nHello\n");
        assert_eq!(p.fence_span, Some((0, text.find("# Body").unwrap())));
    }

    #[test]
    fn array_allowed_tools_and_no_frontmatter() {
        let p = parse("---\ndescription: \"x\"\nallowed-tools: [\"Bash\", \"Read\"]\n---\nbody");
        assert_eq!(
            string_or_list(p.frontmatter.fields.get("allowed-tools")),
            vec!["Bash", "Read"]
        );
        let none = parse("# Just a command\n\nDo the thing.\n");
        assert!(!none.frontmatter.present);
        assert_eq!(none.body, "# Just a command\n\nDo the thing.\n");
        assert_eq!(none.fence_span, None);
    }

    #[test]
    fn keeps_raw_on_yaml_error_and_preserves_key_order() {
        let p = parse("---\nname: a\ndescription: \"unterminated\n---\nbody");
        assert!(p.frontmatter.present);
        assert!(p.warning.is_some());
        assert_eq!(p.frontmatter.raw, "name: a\ndescription: \"unterminated");
        let ordered = parse("---\nzeta: 1\nalpha: 2\nmid: 3\n---\n");
        let keys: Vec<_> = ordered.frontmatter.fields.keys().cloned().collect();
        assert_eq!(keys, vec!["zeta", "alpha", "mid"]);
    }

    #[test]
    fn literal_escapes_in_double_quoted_scalar_survive() {
        let p = parse("---\nname: ui-tweaker\ndescription: \"Use this agent\\n\\n<example>\\nctx\\n</example>\"\nmodel: sonnet\n---\n");
        assert_eq!(
            field_str(&p.frontmatter, "description").as_deref(),
            Some("Use this agent\n\n<example>\nctx\n</example>")
        );
        assert_eq!(preview("a  b\n\nc", 3), "a b…");
    }
}
