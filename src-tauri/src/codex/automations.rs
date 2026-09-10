//! Codex automations: `<codex_home>/automations/<id>/automation.toml`, owned by the Codex desktop
//! app. Sibling `memory.md` is agent-maintained run memory; `report-YYYY-MM-DD.md` are outputs.
//!
//! Flat TOML: `version`, `id`, `kind`, `name`, `prompt` (single-line basic string with `\n`
//! escapes), `status` (`ACTIVE` | `PAUSED`), `rrule`, `model`, `reasoning_effort`,
//! `execution_environment`, `target` (inline table), `cwds`, `created_at` / `updated_at` (epoch ms).

use super::config_toml::{item_to_json, parse_doc};
use crate::fsutil::{self, read_text_opt};
use crate::model::{Agent, Automation, AutomationTarget, EntityKind, Locator, Origin, Scope};
use crate::scan::{ScanCtx, ScanOut};
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item};

pub fn scan(ctx: &ScanCtx<'_>, out: &mut ScanOut) {
    let root = ctx.homes.codex_home.join("automations");
    for dir in automation_dirs(&root) {
        let file = dir.join("automation.toml");
        let text = match read_text_opt(&file) {
            Ok(Some(t)) => t,
            Ok(None) => continue,
            Err(e) => {
                out.warn(Some(&file), e.message);
                continue;
            }
        };
        out.record_hash(&file, &text);
        let Some(doc) = parse_doc(&file, &text, out) else {
            continue;
        };
        out.automations.push(from_doc(&doc, &dir, &file));
    }
}

/// Subdirectories of `root` whose name does not start with `.`, sorted.
fn automation_dirs(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// `"local-" + sha256(path)[..32]`, the project id the Codex desktop app derives from a local path.
pub fn local_project_id(path: &Path) -> String {
    let hash = fsutil::sha256_hex(fsutil::display(path).as_bytes());
    format!("local-{}", &hash[..32])
}

fn as_i64(item: &Item) -> Option<i64> {
    item.as_integer()
        .or_else(|| item.as_float().map(|f| f as i64))
}

pub fn from_doc(doc: &DocumentMut, dir: &Path, file: &Path) -> Automation {
    let table = doc.as_table();
    let get_str = |key: &str| table.get(key).and_then(Item::as_str).map(String::from);
    let dir_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let target = match table.get("target") {
        None => AutomationTarget::Other { raw: Value::Null },
        Some(item) => match item.as_table_like() {
            Some(t) => match t.get("type").and_then(Item::as_str) {
                Some("project") => {
                    let project_id = t
                        .get("project_id")
                        .and_then(Item::as_str)
                        .unwrap_or_default()
                        .to_string();
                    AutomationTarget::Project {
                        local: project_id.starts_with("local-"),
                        project_id,
                    }
                }
                Some("projectless") => AutomationTarget::Projectless,
                _ => AutomationTarget::Other {
                    raw: item_to_json(item),
                },
            },
            None => AutomationTarget::Other {
                raw: item_to_json(item),
            },
        },
    };

    let cwds = table
        .get("cwds")
        .and_then(Item::as_array)
        .map(|a| {
            a.iter()
                .map(|v| match v.as_str() {
                    Some(s) => s.to_string(),
                    None => v.to_string().trim().to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    const MODELED: &[&str] = &[
        "name",
        "kind",
        "status",
        "prompt",
        "rrule",
        "model",
        "reasoning_effort",
        "execution_environment",
        "target",
        "cwds",
        "created_at",
        "updated_at",
    ];
    let extra: Map<String, Value> = table
        .iter()
        .filter(|(k, _)| !MODELED.contains(k))
        .map(|(k, item)| (k.to_string(), item_to_json(item)))
        .collect();

    let mut report_paths: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file()
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.starts_with("report-") && n.ends_with(".md"))
                })
                .map(|p| fsutil::display(&p))
                .collect()
        })
        .unwrap_or_default();
    report_paths.sort();
    let memory = dir.join("memory.md");
    let memory_path = memory.is_file().then(|| fsutil::display(&memory));

    let file_str = fsutil::display(file);
    Automation {
        id: format!("codex:{}:{}", EntityKind::Automation.key(), file_str),
        agent: Agent::Codex,
        scope: Scope::User,
        name: get_str("name").unwrap_or_else(|| dir_name.clone()),
        kind: get_str("kind").unwrap_or_else(|| "cron".to_string()),
        status: get_str("status").unwrap_or_default(),
        prompt: get_str("prompt").unwrap_or_default(),
        rrule: get_str("rrule"),
        model: get_str("model"),
        reasoning_effort: get_str("reasoning_effort"),
        execution_environment: get_str("execution_environment"),
        target,
        cwds,
        created_at: table.get("created_at").and_then(as_i64),
        updated_at: table.get("updated_at").and_then(as_i64),
        dir: fsutil::display(dir),
        origin: Origin {
            agent: Agent::Codex,
            scope: Scope::User,
            file: file_str,
            locator: Locator::TomlPath { path: Vec::new() },
        },
        memory_path,
        report_paths,
        extra,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn local_project_id_is_sha256_prefix_of_path() {
        assert_eq!(
            local_project_id(Path::new("/Users/me/Documents/GitHub/my-repo")),
            "local-b65220693b3896ad8555b34b3ede0186"
        );
    }

    #[test]
    fn parses_all_three_target_forms_and_extra() {
        let text = "version = 1\nid = \"x\"\nkind = \"cron\"\nname = \"X\"\nprompt = \"a\\nb\"\nstatus = \"ACTIVE\"\nrrule = \"RRULE:FREQ=DAILY\"\ntarget = { type = \"project\", project_id = \"local-abc\" }\ncwds = [\"~\"]\ncreated_at = 1778733369975\nupdated_at = 1789065184555\n";
        let doc: DocumentMut = text.parse().unwrap();
        let a = from_doc(
            &doc,
            Path::new("/h/.codex/automations/x"),
            Path::new("/h/.codex/automations/x/automation.toml"),
        );
        assert_eq!(
            a.id,
            "codex:automation:/h/.codex/automations/x/automation.toml"
        );
        assert_eq!(a.prompt, "a\nb");
        assert_eq!(
            a.target,
            AutomationTarget::Project {
                project_id: "local-abc".into(),
                local: true
            }
        );
        assert_eq!(a.cwds, vec!["~"]);
        assert_eq!(a.created_at, Some(1778733369975));
        assert_eq!(a.extra.get("version"), Some(&Value::from(1)));
        assert_eq!(a.extra.get("id"), Some(&Value::String("x".into())));
        assert!(!a.extra.contains_key("prompt"));

        let cloud: DocumentMut =
            "target = { type = \"project\", project_id = \"2ae4553d-9ebf\" }\n"
                .parse()
                .unwrap();
        assert_eq!(
            from_doc(&cloud, Path::new("/a/b"), Path::new("/a/b/automation.toml")).target,
            AutomationTarget::Project {
                project_id: "2ae4553d-9ebf".into(),
                local: false
            }
        );
        let projectless: DocumentMut = "target = { type = \"projectless\" }\n".parse().unwrap();
        assert_eq!(
            from_doc(
                &projectless,
                Path::new("/a/b"),
                Path::new("/a/b/automation.toml")
            )
            .target,
            AutomationTarget::Projectless
        );
        let odd: DocumentMut = "target = { type = \"team\", team_id = 7 }\n"
            .parse()
            .unwrap();
        let a = from_doc(&odd, Path::new("/a/b"), Path::new("/a/b/automation.toml"));
        assert_eq!(a.name, "b", "falls back to the directory name");
        assert_eq!(
            a.target,
            AutomationTarget::Other {
                raw: serde_json::json!({ "type": "team", "team_id": 7 })
            }
        );
    }
}
