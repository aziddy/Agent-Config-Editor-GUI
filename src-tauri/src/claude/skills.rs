//! Skills: `skills/<name>/SKILL.md` under `~/.claude/` (user), `<root>/.claude/` (project) and a
//! plugin's install dir (plugin, via [`super::plugins`]).
//!
//! Claude Code has no enabled/disabled state for skills, so every skill is read-only with an
//! explanatory reason. `~/.agents/skills` is a Codex location and is not read here.

use super::{
    clean_description, file_id, file_name, origin, project_scope, read_markdown, sorted_dir, AGENT,
};
use crate::fsutil;
use crate::markdown::frontmatter::{field_str, preview};
use crate::model::{EnabledState, EntityKind, Locator, Scope, Skill};
use crate::scan::{ScanCtx, ScanOut};
use std::path::{Path, PathBuf};

pub const NO_TOGGLE_REASON: &str =
    "Claude Code has no per-skill enable switch; use disable-model-invocation in the frontmatter";

/// User skills plus project skills for every root in `roots`.
pub fn scan(ctx: &ScanCtx<'_>, out: &mut ScanOut, roots: &[PathBuf]) {
    scan_dir(&ctx.homes.claude_dir.join("skills"), &Scope::User, out);
    for root in roots {
        scan_dir(
            &root.join(".claude").join("skills"),
            &project_scope(root),
            out,
        );
    }
}

/// Read `<dir>/*/SKILL.md`, push each skill into `out` and return the ids in directory order.
pub fn scan_dir(dir: &Path, scope: &Scope, out: &mut ScanOut) -> Vec<String> {
    let mut ids = Vec::new();
    for entry in sorted_dir(dir) {
        if !entry.is_dir() {
            continue;
        }
        let path = entry.join("SKILL.md");
        if !path.is_file() {
            continue;
        }
        if let Some(skill) = read_skill(&path, scope, out) {
            ids.push(skill.id.clone());
            out.skills.push(skill);
        }
    }
    ids
}

fn read_skill(path: &Path, scope: &Scope, out: &mut ScanOut) -> Option<Skill> {
    let (text, parsed) = read_markdown(path, out)?;
    let dir = path.parent()?;
    let name = field_str(&parsed.frontmatter, "name")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| file_name(dir));
    Some(Skill {
        id: file_id(EntityKind::Skill, path),
        name,
        description: clean_description(field_str(&parsed.frontmatter, "description")),
        agent: AGENT,
        scope: scope.clone(),
        origin: origin(scope.clone(), path, Locator::MarkdownFile),
        dir: fsutil::display(dir),
        enabled: EnabledState::read_only(NO_TOGGLE_REASON),
        frontmatter: parsed.frontmatter,
        is_system: false,
        lock_managed: false,
        content_hash: fsutil::sha256_hex(text.as_bytes()),
        dup_group: None,
        body_preview: preview(&parsed.body, 160),
    })
}
