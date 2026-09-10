//! Subagents (`agents/*.md`) and slash commands (`commands/**/*.md`) under `~/.claude/` (user),
//! `<root>/.claude/` (project) and a plugin's install dir (plugin, via [`super::plugins`]).
//!
//! - Subagent `name` is the frontmatter `name` or the file stem; `description` may embed literal
//!   `\n` and `<example>` blocks, which are kept verbatim.
//! - Command `name` is the file stem. One level of subdirectories is supported: `dir/name.md`
//!   becomes the namespaced command `dir:name`. Frontmatter may be entirely absent.

use super::{
    clean_description, file_id, file_stem, is_markdown, origin, project_scope, read_markdown,
    sorted_dir, AGENT,
};
use crate::fsutil;
use crate::markdown::frontmatter::{field_bool, field_str, preview, string_or_list};
use crate::model::{EntityKind, Locator, Scope, SlashCommand, SubAgent};
use crate::scan::{ScanCtx, ScanOut};
use std::path::{Path, PathBuf};

/// User agents/commands plus project ones for every root in `roots`.
pub fn scan(ctx: &ScanCtx<'_>, out: &mut ScanOut, roots: &[PathBuf]) {
    scan_agents_dir(&ctx.homes.claude_dir.join("agents"), &Scope::User, out);
    scan_commands_dir(&ctx.homes.claude_dir.join("commands"), &Scope::User, out);
    for root in roots {
        let scope = project_scope(root);
        let dot_claude = root.join(".claude");
        scan_agents_dir(&dot_claude.join("agents"), &scope, out);
        scan_commands_dir(&dot_claude.join("commands"), &scope, out);
    }
}

/// Read `<dir>/*.md` as subagents. Returns the ids in directory order.
pub fn scan_agents_dir(dir: &Path, scope: &Scope, out: &mut ScanOut) -> Vec<String> {
    let mut ids = Vec::new();
    for path in sorted_dir(dir) {
        if !is_markdown(&path) {
            continue;
        }
        if let Some(agent) = read_agent(&path, scope, out) {
            ids.push(agent.id.clone());
            out.subagents.push(agent);
        }
    }
    ids
}

/// Read a single agent file. Exposed for plugins that declare agent files individually.
pub fn scan_agent_file(path: &Path, scope: &Scope, out: &mut ScanOut) -> Option<String> {
    let agent = read_agent(path, scope, out)?;
    let id = agent.id.clone();
    out.subagents.push(agent);
    Some(id)
}

/// Read `<dir>/*.md` and `<dir>/<ns>/*.md` as commands. Returns the ids in directory order.
pub fn scan_commands_dir(dir: &Path, scope: &Scope, out: &mut ScanOut) -> Vec<String> {
    let mut ids = Vec::new();
    for path in sorted_dir(dir) {
        if is_markdown(&path) {
            ids.extend(push_command(&path, None, scope, out));
        } else if path.is_dir() {
            let ns = super::file_name(&path);
            for sub in sorted_dir(&path) {
                if is_markdown(&sub) {
                    ids.extend(push_command(&sub, Some(&ns), scope, out));
                }
            }
        }
    }
    ids
}

/// Read a single command file. Exposed for plugins that declare command files individually.
pub fn scan_command_file(path: &Path, scope: &Scope, out: &mut ScanOut) -> Option<String> {
    push_command(path, None, scope, out)
}

fn push_command(
    path: &Path,
    namespace: Option<&str>,
    scope: &Scope,
    out: &mut ScanOut,
) -> Option<String> {
    let command = read_command(path, namespace, scope, out)?;
    let id = command.id.clone();
    out.commands.push(command);
    Some(id)
}

fn read_agent(path: &Path, scope: &Scope, out: &mut ScanOut) -> Option<SubAgent> {
    let (text, parsed) = read_markdown(path, out)?;
    let fm = &parsed.frontmatter;
    let name = field_str(fm, "name")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| file_stem(path));
    Some(SubAgent {
        id: file_id(EntityKind::SubAgent, path),
        name,
        agent: AGENT,
        scope: scope.clone(),
        origin: origin(scope.clone(), path, Locator::MarkdownFile),
        description: clean_description(field_str(fm, "description")),
        model: field_str(fm, "model"),
        tools: string_or_list(fm.fields.get("tools")),
        color: field_str(fm, "color"),
        effort: field_str(fm, "effort"),
        body_preview: preview(&parsed.body, 160),
        content_hash: fsutil::sha256_hex(text.as_bytes()),
        frontmatter: parsed.frontmatter,
        dup_group: None,
    })
}

fn read_command(
    path: &Path,
    namespace: Option<&str>,
    scope: &Scope,
    out: &mut ScanOut,
) -> Option<SlashCommand> {
    let (text, parsed) = read_markdown(path, out)?;
    let fm = &parsed.frontmatter;
    let stem = file_stem(path);
    let name = match namespace {
        Some(ns) => format!("{ns}:{stem}"),
        None => stem,
    };
    Some(SlashCommand {
        id: file_id(EntityKind::SlashCommand, path),
        name,
        agent: AGENT,
        scope: scope.clone(),
        origin: origin(scope.clone(), path, Locator::MarkdownFile),
        description: clean_description(field_str(fm, "description")),
        argument_hint: field_str(fm, "argument-hint"),
        allowed_tools: string_or_list(fm.fields.get("allowed-tools")),
        disable_model_invocation: field_bool(fm, "disable-model-invocation"),
        body_preview: preview(&parsed.body, 160),
        content_hash: fsutil::sha256_hex(text.as_bytes()),
        frontmatter: parsed.frontmatter,
        dup_group: None,
    })
}
