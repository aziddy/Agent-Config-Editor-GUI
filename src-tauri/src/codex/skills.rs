//! Codex skills: `SKILL.md` files under four roots.
//!
//! 1. `<codex_home>/skills/**/SKILL.md` — user skills; `.system/` holds vendor built-ins
//!    (marked by `.codex-system-skills.marker`) → `Scope::System`, `is_system = true`.
//! 2. `<agents_dir>/skills/*/SKILL.md` — installer managed via `<agents_dir>/.skill-lock.json`.
//! 3. Plugin skills — pushed by [`super::plugins`] through [`read_skill`].
//! 4. `<project>/.agents/skills/*/SKILL.md` for every tracked project that exists.
//!
//! Enabled state is a deny-list in `[[skills.config]]` (`enabled = false` by `path` or `name`;
//! absent = enabled).

use super::config_toml::{same_file, CodexConfig};
use crate::fsutil::{self, read_text_opt};
use crate::markdown::frontmatter::{self, field_str};
use crate::model::{Agent, EnabledState, EntityKind, Locator, Origin, Scope, Skill};
use crate::scan::{ScanCtx, ScanOut};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const SYSTEM_DIR: &str = ".system";
pub const SYSTEM_MARKER: &str = ".codex-system-skills.marker";

pub fn scan(ctx: &ScanCtx<'_>, config: &CodexConfig, out: &mut ScanOut) {
    // (1) <codex_home>/skills, including .system
    let root = ctx.homes.codex_home.join("skills");
    let system_root = root.join(SYSTEM_DIR);
    for skill_md in find_skill_files(&root, 4) {
        let is_system = skill_md.starts_with(&system_root);
        let scope = if is_system {
            Scope::System
        } else {
            Scope::User
        };
        if let Some(skill) = read_skill(config, &skill_md, scope, is_system, false, out) {
            out.skills.push(skill);
        }
    }

    // (2) <agents_dir>/skills
    let agents_skills = ctx.homes.agents_dir.join("skills");
    let locked = read_skill_lock(&ctx.homes.agents_dir, out);
    for skill_md in find_skill_files(&agents_skills, 2) {
        let dir_name = skill_md
            .parent()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let lock_managed = locked.contains(&dir_name);
        if let Some(skill) = read_skill(config, &skill_md, Scope::User, false, lock_managed, out) {
            out.skills.push(skill);
        }
    }

    // (4) tracked projects
    for project_root in &config.projects {
        if !project_root.is_dir() {
            continue;
        }
        let project_skills = project_root.join(".agents").join("skills");
        // `[projects."<home>"]` is common; its `.agents/skills` is the user dir scanned above.
        if same_file(&project_skills, &agents_skills) {
            continue;
        }
        let scope = Scope::Project {
            root: fsutil::display(project_root),
        };
        for skill_md in find_skill_files(&project_skills, 2) {
            if let Some(skill) = read_skill(config, &skill_md, scope.clone(), false, false, out) {
                out.skills.push(skill);
            }
        }
    }
}

/// Every `SKILL.md` under `root` up to `max_depth` levels down, sorted. Hidden directories are
/// skipped except `.system`. Symlinks are followed (walkdir detects loops). Missing root → empty.
pub fn find_skill_files(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    if !root.is_dir() {
        return Vec::new();
    }
    WalkDir::new(root)
        .max_depth(max_depth)
        .follow_links(true)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            let name = e.file_name().to_string_lossy();
            !name.starts_with('.') || name == SYSTEM_DIR
        })
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.file_name() == "SKILL.md")
        .map(|e| e.into_path())
        .collect()
}

/// Skill names present in `<agents_dir>/.skill-lock.json` (`skills` map keys).
fn read_skill_lock(agents_dir: &Path, out: &mut ScanOut) -> HashSet<String> {
    let path = agents_dir.join(".skill-lock.json");
    let text = match read_text_opt(&path) {
        Ok(Some(t)) => t,
        Ok(None) => return HashSet::new(),
        Err(e) => {
            out.warn(Some(&path), e.message);
            return HashSet::new();
        }
    };
    out.record_hash(&path, &text);
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(v) => v
            .get("skills")
            .and_then(|s| s.as_object())
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default(),
        Err(e) => {
            out.warn(Some(&path), format!("JSON parse error: {e}"));
            HashSet::new()
        }
    }
}

/// Build a [`Skill`] from one `SKILL.md`. Read failures are warnings and yield `None`.
pub fn read_skill(
    config: &CodexConfig,
    skill_md: &Path,
    scope: Scope,
    is_system: bool,
    lock_managed: bool,
    out: &mut ScanOut,
) -> Option<Skill> {
    let text = match read_text_opt(skill_md) {
        Ok(Some(t)) => t,
        Ok(None) => return None,
        Err(e) => {
            out.warn(Some(skill_md), e.message);
            return None;
        }
    };
    out.record_hash(skill_md, &text);
    let parsed = frontmatter::parse(&text);
    if let Some(w) = &parsed.warning {
        out.warn(Some(skill_md), w.clone());
    }
    let dir = skill_md.parent().unwrap_or(skill_md);
    let dir_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = field_str(&parsed.frontmatter, "name")
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or(dir_name);
    let description = field_str(&parsed.frontmatter, "description").map(|d| d.trim().to_string());
    let path_str = fsutil::display(skill_md);

    let enabled = match config.skills_entry(&path_str, &name) {
        Some(entry) => {
            let reason = if entry.enabled {
                None
            } else if entry.path.is_some() {
                Some("disabled via [[skills.config]] path entry".to_string())
            } else {
                Some("disabled via [[skills.config]] name entry".to_string())
            };
            EnabledState::toggleable(
                entry.enabled,
                Origin {
                    agent: Agent::Codex,
                    scope: Scope::User,
                    file: fsutil::display(&config.path),
                    locator: Locator::TomlArrayItem {
                        path: vec!["skills".to_string(), "config".to_string()],
                        index: entry.index,
                    },
                },
                reason,
            )
        }
        None => EnabledState::toggleable(
            true,
            config.toggle_origin(vec!["skills".to_string(), "config".to_string()]),
            None,
        ),
    };

    Some(Skill {
        id: format!("codex:{}:{}", EntityKind::Skill.key(), path_str),
        name,
        description,
        agent: Agent::Codex,
        scope: scope.clone(),
        origin: Origin {
            agent: Agent::Codex,
            scope,
            file: path_str,
            locator: Locator::MarkdownFile,
        },
        dir: fsutil::display(dir),
        enabled,
        frontmatter: parsed.frontmatter,
        is_system,
        lock_managed,
        content_hash: fsutil::sha256_hex(text.as_bytes()),
        dup_group: None,
        body_preview: frontmatter::preview(&parsed.body, 160),
    })
}
