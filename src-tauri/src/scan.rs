//! Assembles the [`Snapshot`] from every reader.

use crate::fsutil::{self, Homes};
use crate::model::{
    Agent, AppSettings, Automation, DuplicateGroup, EntityKind, HookEvent, McpServer, Plugin,
    ProjectRef, Skill, SlashCommand, Snapshot, SubAgent, Warning,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

/// Read-only inputs shared by every reader.
pub struct ScanCtx<'a> {
    pub homes: &'a Homes,
    pub settings: &'a AppSettings,
}

/// Accumulator that readers push into.
#[derive(Default)]
pub struct ScanOut {
    pub skills: Vec<Skill>,
    pub mcp_servers: Vec<McpServer>,
    pub plugins: Vec<Plugin>,
    pub subagents: Vec<SubAgent>,
    pub commands: Vec<SlashCommand>,
    pub hooks: Vec<HookEvent>,
    pub automations: Vec<Automation>,
    pub projects: Vec<ProjectRef>,
    pub warnings: Vec<Warning>,
    pub file_hashes: BTreeMap<String, String>,
}

impl ScanOut {
    pub fn warn(&mut self, file: Option<&Path>, message: impl Into<String>) {
        self.warnings.push(Warning {
            file: file.map(fsutil::display),
            message: message.into(),
        });
    }

    pub fn record_hash(&mut self, path: &Path, text: &str) {
        self.file_hashes
            .insert(fsutil::display(path), fsutil::sha256_hex(text.as_bytes()));
    }

    /// Register a project root tracked by `agent`, merging with an existing entry.
    pub fn track_project(&mut self, root: &Path, agent: Agent) {
        let key = fsutil::display(root);
        if let Some(existing) = self.projects.iter_mut().find(|p| p.root == key) {
            if !existing.tracked_by.contains(&agent) {
                existing.tracked_by.push(agent);
            }
            return;
        }
        self.projects.push(ProjectRef {
            exists: root.is_dir(),
            root: key,
            tracked_by: vec![agent],
        });
    }
}

pub fn scan_all(homes: &Homes, settings: &AppSettings) -> Snapshot {
    let started = Instant::now();
    let ctx = ScanCtx { homes, settings };
    let mut out = ScanOut::default();

    crate::claude::scan(&ctx, &mut out);
    crate::codex::scan(&ctx, &mut out);

    out.projects.sort_by(|a, b| a.root.cmp(&b.root));
    let dup_groups = dedupe(&mut out);

    Snapshot {
        generated_at: chrono::Utc::now().to_rfc3339(),
        homes: Some(homes.info()),
        skills: out.skills,
        mcp_servers: out.mcp_servers,
        plugins: out.plugins,
        subagents: out.subagents,
        commands: out.commands,
        hooks: out.hooks,
        automations: out.automations,
        dup_groups,
        projects: out.projects,
        warnings: out.warnings,
        file_hashes: out
            .file_hashes
            .into_iter()
            .map(|(k, v)| (k, Value::String(v)))
            .collect(),
        scan_millis: started.elapsed().as_millis() as u64,
    }
}

/// Group byte-identical entities (same kind + name + content hash) that live in different
/// project roots, e.g. the same skill checked into twelve git worktrees.
fn dedupe(out: &mut ScanOut) -> Vec<DuplicateGroup> {
    let mut groups: BTreeMap<String, DuplicateGroup> = BTreeMap::new();

    fn add(
        groups: &mut BTreeMap<String, DuplicateGroup>,
        kind: EntityKind,
        name: &str,
        hash: &str,
        id: &str,
        root: Option<&str>,
    ) -> String {
        let key = format!("{}:{}:{}", kind.key(), name, &hash[..12.min(hash.len())]);
        let g = groups.entry(key.clone()).or_insert_with(|| DuplicateGroup {
            key: key.clone(),
            kind,
            name: name.to_string(),
            content_hash: hash.to_string(),
            member_ids: Vec::new(),
            project_roots: Vec::new(),
        });
        g.member_ids.push(id.to_string());
        if let Some(r) = root {
            if !g.project_roots.iter().any(|x| x == r) {
                g.project_roots.push(r.to_string());
            }
        }
        key
    }

    fn root_of(scope: &crate::model::Scope) -> Option<&str> {
        match scope {
            crate::model::Scope::Project { root } => Some(root.as_str()),
            _ => None,
        }
    }

    for s in &mut out.skills {
        s.dup_group = Some(add(
            &mut groups,
            EntityKind::Skill,
            &s.name,
            &s.content_hash,
            &s.id,
            root_of(&s.scope),
        ));
    }
    for a in &mut out.subagents {
        a.dup_group = Some(add(
            &mut groups,
            EntityKind::SubAgent,
            &a.name,
            &a.content_hash,
            &a.id,
            root_of(&a.scope),
        ));
    }
    for c in &mut out.commands {
        c.dup_group = Some(add(
            &mut groups,
            EntityKind::SlashCommand,
            &c.name,
            &c.content_hash,
            &c.id,
            root_of(&c.scope),
        ));
    }

    // Only keep groups with more than one member; clear the others' dup_group.
    let multi: BTreeMap<String, DuplicateGroup> = groups
        .into_iter()
        .filter(|(_, g)| g.member_ids.len() > 1)
        .collect();
    for s in &mut out.skills {
        if !s.dup_group.as_ref().is_some_and(|k| multi.contains_key(k)) {
            s.dup_group = None;
        }
    }
    for a in &mut out.subagents {
        if !a.dup_group.as_ref().is_some_and(|k| multi.contains_key(k)) {
            a.dup_group = None;
        }
    }
    for c in &mut out.commands {
        if !c.dup_group.as_ref().is_some_and(|k| multi.contains_key(k)) {
            c.dup_group = None;
        }
    }
    multi.into_values().collect()
}
