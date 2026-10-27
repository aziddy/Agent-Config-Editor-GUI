//! Stored memories and instruction-file inventory. This does not simulate an agent's
//! effective instruction chain; all existing candidates remain independently editable.

use crate::fsutil::{self, Homes};
use crate::model::{Agent, MemoryCategory, MemoryFile, MemorySource, ProjectRef, Scope};
use crate::scan::{ScanCtx, ScanOut};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;

const MAX_DEPTH: usize = 64;
const MAX_ENTRIES: usize = 100_000;

pub fn scan(ctx: &ScanCtx<'_>, out: &mut ScanOut) {
    let projects = out.projects.clone();
    let automations = out.automations.clone();
    let mut inventory = Inventory {
        homes: ctx.homes,
        out,
        files: BTreeMap::new(),
    };
    let owners = project_store_owners(ctx.homes, &projects);
    for project_dir in inventory.children(&ctx.homes.claude_dir.join("projects")) {
        if !is_real_dir(&project_dir) {
            continue;
        }
        let key = project_dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let scopes = owners
            .get(key.as_ref())
            .cloned()
            .unwrap_or_else(|| vec![Scope::User]);
        inventory.markdown_tree(
            &project_dir.join("memory"),
            MemorySource::ClaudeProject,
            &[Agent::Claude],
            &scopes,
        );
    }
    inventory.subagent_stores(&ctx.homes.claude_dir.join("agent-memory"), &[Scope::User]);
    inventory.markdown_tree(
        &ctx.homes.codex_home.join("memories"),
        MemorySource::CodexMemory,
        &[Agent::Codex],
        &[Scope::User],
    );
    for automation in automations {
        if let Some(path) = automation.memory_path {
            let name = if automation.name.trim().is_empty() {
                Path::new(&automation.dir)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Automation")
            } else {
                &automation.name
            };
            inventory.add(
                Path::new(&path),
                Path::new(&automation.dir),
                MemorySource::Automation,
                &[Agent::Codex],
                &[automation.scope],
                Some(name),
            );
        }
    }

    let global_fallbacks = inventory.fallbacks(&ctx.homes.codex_home.join("config.toml"));
    inventory.instructions_at(&ctx.homes.home, &[Scope::User], &global_fallbacks, true);
    inventory.add_instruction(
        &ctx.homes.claude_dir.join("CLAUDE.md"),
        &[Agent::Claude],
        &[Scope::User],
    );
    inventory.rules(&ctx.homes.claude_dir.join("rules"), &[Scope::User]);
    for name in ["AGENTS.md", "AGENTS.override.md"] {
        inventory.add_instruction(
            &ctx.homes.codex_home.join(name),
            &[Agent::Codex],
            &[Scope::User],
        );
    }
    inventory.custom_store_settings(&ctx.homes.claude_dir.join("settings.json"), Scope::User);
    inventory.custom_store_settings(
        &ctx.homes.claude_dir.join("settings.local.json"),
        Scope::User,
    );
    if !ctx.homes.sandbox {
        let managed = crate::claude::settings::managed_settings_path();
        if let Some(parent) = managed.parent() {
            inventory.add_instruction(
                &parent.join("CLAUDE.md"),
                &[Agent::Claude],
                &[Scope::Managed],
            );
        }
        inventory.custom_store_settings(&managed, Scope::Managed);
    }

    let mut nested = Vec::new();
    for project in &projects {
        let root = ctx.homes.expand_tilde(&project.root);
        let scope = project_scope(ctx.homes, &project.root);
        let scopes = [scope.clone()];
        if !root.is_dir() {
            continue;
        }
        let mut fallbacks = global_fallbacks.clone();
        fallbacks.extend(inventory.fallbacks(&root.join(".codex/config.toml")));
        fallbacks.retain(|name| !is_agent_instruction_name(name));
        fallbacks.sort();
        fallbacks.dedup();
        inventory.instructions_at(&root, &scopes, &fallbacks, false);
        inventory.custom_store_settings(&root.join(".claude/settings.json"), scope.clone());
        inventory.custom_store_settings(&root.join(".claude/settings.local.json"), scope);
        for directory in ["agent-memory", "agent-memory-local"] {
            inventory.subagent_stores(&root.join(".claude").join(directory), &scopes);
        }
        let mut ancestor = root.parent();
        while let Some(dir) = ancestor {
            // A sandbox should never discover host-wide instructions above its fake home.
            if ctx.homes.sandbox && !dir.starts_with(&ctx.homes.home) {
                break;
            }
            inventory.instructions_at(dir, &scopes, &fallbacks, false);
            if dir == ctx.homes.home {
                break;
            }
            ancestor = dir.parent();
        }
        let physical_root = root.canonicalize().unwrap_or_else(|_| root.clone());
        let physical_home = ctx
            .homes
            .home
            .canonicalize()
            .unwrap_or_else(|_| ctx.homes.home.clone());
        if !physical_home.starts_with(&physical_root) {
            nested.push(NestedProject {
                root,
                physical_root,
                scopes: scopes.to_vec(),
                fallbacks,
            });
        }
    }
    // Seed every tracked root before traversing children: an explicitly tracked worktree
    // under a normally pruned directory still receives its own walk. Physical directories
    // are queued once across the entire forest, including overlapping project roots.
    inventory.nested_instructions(&nested);
    for file in inventory.files.values_mut() {
        file.agents.sort_by_key(|agent| agent.key());
        file.scopes.sort_by_key(Scope::key);
        file.paths.sort();
    }
    inventory.out.memories = inventory.files.into_values().collect();
}

struct NestedProject {
    root: PathBuf,
    physical_root: PathBuf,
    scopes: Vec<Scope>,
    fallbacks: Vec<String>,
}

struct Inventory<'a> {
    homes: &'a Homes,
    out: &'a mut ScanOut,
    files: BTreeMap<String, MemoryFile>,
}

impl Inventory<'_> {
    fn add(
        &mut self,
        path: &Path,
        dir: &Path,
        source: MemorySource,
        agents: &[Agent],
        scopes: &[Scope],
        display_name: Option<&str>,
    ) {
        let target = match path.canonicalize() {
            Ok(target) if target.is_file() => target,
            Ok(_) => return,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                return
            }
            Err(error) => {
                self.out.warn(
                    Some(path),
                    format!("cannot resolve memory or instruction file: {error}"),
                );
                return;
            }
        };
        let alias = fsutil::display(path);
        let key = fsutil::display(&target);
        if let Some(existing) = self.files.get_mut(&key) {
            if matches!(source, MemorySource::Instruction | MemorySource::Rule) {
                existing.category = MemoryCategory::Instruction;
                existing.source = source;
                existing.generated = false;
                existing.name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
            }
            for agent in agents {
                if !existing.agents.contains(agent) {
                    existing.agents.push(*agent);
                }
            }
            for scope in scopes {
                if !existing.scopes.contains(scope) {
                    existing.scopes.push(scope.clone());
                }
            }
            if !existing.paths.contains(&alias) {
                existing.paths.push(alias.clone());
            }
            self.out
                .file_hashes
                .insert(alias, existing.content_hash.clone());
            return;
        }
        let text = match fsutil::read_text_opt(&target) {
            Ok(Some(text)) => text,
            Ok(None) => return,
            Err(error) => {
                self.out.warn(
                    Some(path),
                    format!("cannot read memory or instruction file: {}", error.message),
                );
                return;
            }
        };
        self.out.record_hash(path, &text);
        self.out.record_hash(&target, &text);
        if is_markdown(path) {
            if let Some(warning) = crate::markdown::frontmatter::parse(&text).warning {
                self.out.warn(Some(path), warning);
            }
        }
        let canonical_dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        let browse_dir = if target.starts_with(&canonical_dir) {
            canonical_dir
        } else {
            target.parent().unwrap_or(&target).to_path_buf()
        };
        let category = match source {
            MemorySource::Instruction | MemorySource::Rule => MemoryCategory::Instruction,
            _ => MemoryCategory::Memory,
        };
        let relative = path.strip_prefix(dir).unwrap_or(path);
        let name = if matches!(
            source,
            MemorySource::ClaudeProject | MemorySource::ClaudeSubagent
        ) {
            let owner = if source == MemorySource::ClaudeProject {
                scopes
                    .iter()
                    .find_map(|scope| match scope {
                        Scope::Project { root } => Some(Path::new(root)),
                        _ => None,
                    })
                    .unwrap_or_else(|| dir.parent().unwrap_or(dir))
            } else {
                dir
            };
            format!(
                "{} · {}",
                relative.display(),
                owner.file_name().unwrap_or_default().to_string_lossy()
            )
        } else {
            relative.to_string_lossy().into_owned()
        };
        self.files.insert(
            key.clone(),
            MemoryFile {
                id: format!("memory:{key}"),
                name: display_name.map(str::to_owned).unwrap_or(name),
                category,
                source,
                agents: agents.to_vec(),
                scopes: scopes.to_vec(),
                paths: vec![alias],
                file: key,
                dir: fsutil::display(&browse_dir),
                content_hash: fsutil::sha256_hex(text.as_bytes()),
                generated: category == MemoryCategory::Memory,
            },
        );
    }

    fn add_instruction(&mut self, path: &Path, agents: &[Agent], scopes: &[Scope]) {
        self.add(
            path,
            path.parent().unwrap_or(path),
            MemorySource::Instruction,
            agents,
            scopes,
            None,
        );
    }

    fn children(&mut self, path: &Path) -> Vec<PathBuf> {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return vec![],
            Err(error) => {
                self.out
                    .warn(Some(path), format!("cannot list memory directory: {error}"));
                return vec![];
            }
        };
        let mut paths = vec![];
        for entry in entries {
            match entry {
                Ok(entry) => paths.push(entry.path()),
                Err(error) => self
                    .out
                    .warn(Some(path), format!("cannot list memory directory: {error}")),
            }
        }
        paths.sort();
        paths
    }

    fn markdown_tree(
        &mut self,
        root: &Path,
        source: MemorySource,
        agents: &[Agent],
        scopes: &[Scope],
    ) {
        if !is_real_dir(root) {
            return;
        }
        for (count, entry) in WalkDir::new(root)
            .follow_links(false)
            .max_depth(MAX_DEPTH)
            .sort_by_file_name()
            .into_iter()
            .enumerate()
        {
            if count == MAX_ENTRIES {
                self.out.warn(
                    Some(root),
                    "Memory discovery stopped after 100000 entries; narrow this directory.",
                );
                break;
            }
            match entry {
                Ok(entry) if !entry.file_type().is_dir() && is_markdown(entry.path()) => {
                    self.add(entry.path(), root, source, agents, scopes, None)
                }
                Ok(entry) if entry.depth() == MAX_DEPTH && entry.file_type().is_dir() => {
                    self.out.warn(
                        Some(entry.path()),
                        "Memory discovery reached its depth limit.",
                    )
                }
                Ok(_) => {}
                Err(error) => self.out.warn(
                    error.path(),
                    format!(
                        "cannot scan memory directory: {}",
                        error
                            .io_error()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| "filesystem error".into())
                    ),
                ),
            }
        }
    }

    fn subagent_stores(&mut self, root: &Path, scopes: &[Scope]) {
        if !is_real_dir(root) {
            return;
        }
        for store in self.children(root) {
            self.markdown_tree(
                &store,
                MemorySource::ClaudeSubagent,
                &[Agent::Claude],
                scopes,
            );
        }
    }

    fn rules(&mut self, root: &Path, scopes: &[Scope]) {
        self.markdown_tree(root, MemorySource::Rule, &[Agent::Claude], scopes);
    }

    fn instructions_at(
        &mut self,
        dir: &Path,
        scopes: &[Scope],
        fallbacks: &[String],
        include_agent_instructions: bool,
    ) {
        // Named instruction documents belong here only at global/home locations.
        // Project rules and explicitly configured fallback files remain available.
        if include_agent_instructions {
            for name in ["CLAUDE.md", "CLAUDE.local.md"] {
                self.add_instruction(&dir.join(name), &[Agent::Claude], scopes);
            }
            self.add_instruction(
                &dir.join("AGENTS.md"),
                &[Agent::Claude, Agent::Codex],
                scopes,
            );
            self.add_instruction(&dir.join("AGENTS.override.md"), &[Agent::Codex], scopes);
            self.add_instruction(&dir.join(".claude/CLAUDE.md"), &[Agent::Claude], scopes);
        }
        self.rules(&dir.join(".claude/rules"), scopes);
        for name in fallbacks {
            self.add_instruction(&dir.join(name), &[Agent::Codex], scopes);
        }
    }

    fn nested_instructions(&mut self, projects: &[NestedProject]) {
        // Context changes only at another tracked root, already seeded separately.
        // Reuse it in descendants instead of testing every project prefix per directory.
        let contexts: Vec<_> = projects
            .iter()
            .map(|root| {
                let mut scopes = Vec::new();
                let mut fallbacks = BTreeSet::new();
                for project in projects {
                    if root.physical_root.starts_with(&project.physical_root) {
                        for scope in &project.scopes {
                            if !scopes.contains(scope) {
                                scopes.push(scope.clone());
                            }
                        }
                        fallbacks.extend(project.fallbacks.iter().cloned());
                    }
                }
                (scopes, fallbacks)
            })
            .collect();
        let mut queued = HashSet::new();
        let mut pending = Vec::new();
        let mut counts = vec![0; projects.len()];
        let mut truncated = BTreeSet::new();
        for (index, project) in projects.iter().enumerate().rev() {
            if is_real_dir(&project.root) && queued.insert(project.physical_root.clone()) {
                pending.push((project.physical_root.clone(), index, 0));
            }
        }
        while let Some((directory, owner, depth)) = pending.pop() {
            let (scopes, fallbacks) = &contexts[owner];
            let entries = match fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) => {
                    self.out.warn(
                        Some(&directory),
                        format!("cannot list instruction directory: {error}"),
                    );
                    continue;
                }
            };
            let mut entries: Vec<_> = entries
                .filter_map(|entry| match entry {
                    Ok(entry) => Some(entry),
                    Err(error) => {
                        self.out.warn(
                            Some(&directory),
                            format!("cannot list instruction directory: {error}"),
                        );
                        None
                    }
                })
                .collect();
            entries.sort_by_cached_key(fs::DirEntry::file_name);
            for entry in entries {
                let path = entry.path();
                counts[owner] += 1;
                if counts[owner] > MAX_ENTRIES {
                    if truncated.insert(owner) {
                        self.out.warn(Some(&projects[owner].root), "Instruction discovery stopped after 100000 entries; remaining files can be found after narrowing tracked projects.");
                    }
                    break;
                }
                let metadata = match entry.file_type() {
                    Ok(metadata) => metadata,
                    Err(error) => {
                        self.out.warn(
                            Some(&path),
                            format!("cannot inspect instruction candidate: {error}"),
                        );
                        continue;
                    }
                };
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if metadata.is_dir() {
                    if name == ".claude" {
                        self.rules(&path.join("rules"), scopes);
                    } else if !pruned(&name) {
                        if depth == MAX_DEPTH {
                            self.out.warn(
                                Some(&path),
                                "Instruction discovery reached its depth limit.",
                            );
                        } else if queued.insert(path.clone()) {
                            pending.push((path, owner, depth + 1));
                        }
                    }
                    continue;
                }
                if fallbacks.contains(name.as_ref()) {
                    self.add_instruction(&path, &[Agent::Codex], scopes);
                }
            }
        }
    }

    fn custom_store_settings(&mut self, path: &Path, scope: Scope) {
        let Some(text) = self.read_config(path) else {
            return;
        };
        let value = match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(value) => value,
            Err(_) => {
                self.warn_parse_once(
                    path,
                    "Cannot parse settings while discovering memory directories.",
                );
                return;
            }
        };
        let Some(raw) = value.get("autoMemoryDirectory") else {
            return;
        };
        let Some(raw) = raw.as_str() else {
            self.out.warn(
                Some(path),
                "autoMemoryDirectory should be an absolute path or start with ~/.",
            );
            return;
        };
        let directory = self.homes.expand_tilde(raw);
        if !directory.is_absolute() {
            self.out.warn(
                Some(path),
                "autoMemoryDirectory should be an absolute path or start with ~/.",
            );
            return;
        }
        self.markdown_tree(
            &directory,
            MemorySource::ClaudeProject,
            &[Agent::Claude],
            &[scope],
        );
    }

    fn fallbacks(&mut self, path: &Path) -> Vec<String> {
        let Some(text) = self.read_config(path) else {
            return vec![];
        };
        let doc = match text.parse::<toml_edit::DocumentMut>() {
            Ok(doc) => doc,
            Err(_) => {
                self.warn_parse_once(
                    path,
                    "Cannot parse Codex settings while discovering instruction filenames.",
                );
                return vec![];
            }
        };
        doc.get("project_doc_fallback_filenames")
            .and_then(toml_edit::Item::as_array)
            .map(|array| {
                array
                    .iter()
                    .filter_map(|value| value.as_str())
                    .filter(|name| valid_filename(name))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn read_config(&mut self, path: &Path) -> Option<String> {
        if !path.is_file() {
            return None;
        }
        match fsutil::read_text_opt(path) {
            Ok(Some(text)) => {
                self.out.record_hash(path, &text);
                Some(text)
            }
            Ok(None) => None,
            Err(error) => {
                self.out.warn(
                    Some(path),
                    format!("cannot read memory discovery settings: {}", error.message),
                );
                None
            }
        }
    }

    fn warn_parse_once(&mut self, path: &Path, message: &str) {
        let file = fsutil::display(path);
        if !self
            .out
            .warnings
            .iter()
            .any(|warning| warning.file.as_ref() == Some(&file))
        {
            self.out.warn(Some(path), message);
        }
    }
}

fn is_agent_instruction_name(name: &str) -> bool {
    matches!(
        name,
        "AGENTS.md" | "AGENTS.override.md" | "CLAUDE.md" | "CLAUDE.local.md"
    )
}

fn valid_filename(name: &str) -> bool {
    !name.is_empty()
        && Path::new(name).components().count() == 1
        && matches!(
            Path::new(name).components().next(),
            Some(Component::Normal(_))
        )
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
        })
}

fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

fn pruned(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".hg"
            | ".svn"
            | "node_modules"
            | "vendor"
            | "target"
            | "dist"
            | "build"
            | ".next"
            | ".nuxt"
            | ".venv"
            | "venv"
            | ".cache"
            | "__pycache__"
            | ".claude"
            | ".codex"
            | ".agents"
    )
}

fn project_scope(homes: &Homes, root: &str) -> Scope {
    if Path::new(root) == homes.home {
        Scope::User
    } else {
        Scope::Project {
            root: root.to_owned(),
        }
    }
}

fn encoded_project(path: &Path) -> String {
    fsutil::display(path)
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect()
}

/// Verify shared worktree ownership from Git's on-disk metadata, without starting Git.
fn repository_root(root: &Path) -> PathBuf {
    for directory in root.ancestors() {
        let marker = directory.join(".git");
        if marker.is_dir() {
            return directory.to_path_buf();
        }
        if let Ok(marker_text) = fs::read_to_string(&marker) {
            if let Some(raw) = marker_text.trim().strip_prefix("gitdir:") {
                let git_dir = directory.join(raw.trim());
                if let Ok(common) = fs::read_to_string(git_dir.join("commondir")) {
                    if let Ok(common) = git_dir.join(common.trim()).canonicalize() {
                        if common.file_name().is_some_and(|name| name == ".git") {
                            if let Some(repo) = common.parent() {
                                return repo.to_path_buf();
                            }
                        }
                    }
                }
            }
        }
    }
    root.to_path_buf()
}

fn project_store_owners(homes: &Homes, projects: &[ProjectRef]) -> BTreeMap<String, Vec<Scope>> {
    let mut repositories: BTreeMap<String, (BTreeSet<String>, Vec<Scope>)> = BTreeMap::new();
    for project in projects {
        let path = homes.expand_tilde(&project.root);
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        let repository = repository_root(&canonical);
        let identity = fsutil::display(&repository);
        let (keys, scopes) = repositories.entry(identity).or_default();
        keys.extend(
            [&path, &canonical, &repository]
                .into_iter()
                .map(|path| encoded_project(path)),
        );
        let scope = project_scope(homes, &project.root);
        if !scopes.contains(&scope) {
            scopes.push(scope);
        }
    }
    let mut candidates: BTreeMap<String, Vec<Vec<Scope>>> = BTreeMap::new();
    for (_, (keys, scopes)) in repositories {
        for key in keys {
            candidates.entry(key).or_default().push(scopes.clone());
        }
    }
    candidates
        .into_iter()
        .filter_map(|(key, owners)| {
            (owners.len() == 1).then(|| (key, owners.into_iter().next().unwrap_or_default()))
        })
        .collect()
}
