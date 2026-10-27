//! Filesystem watcher that tells the webview when config files change on disk.

use crate::fsutil::Homes;
use crate::model::{MemoryCategory, MemoryFile, MemorySource};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub const EVENT_NAME: &str = "config-changed";
const OWN_WRITE_GRACE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigChanged {
    pub paths: Vec<String>,
}

/// Records the app's own writes so the watcher can ignore the echo.
#[derive(Default)]
pub struct RecentWrites {
    inner: Mutex<HashMap<PathBuf, Instant>>,
}

impl RecentWrites {
    pub fn record(&self, path: &Path) {
        if let Ok(mut m) = self.inner.lock() {
            m.insert(path.to_path_buf(), Instant::now());
            m.retain(|_, t| t.elapsed() < OWN_WRITE_GRACE * 5);
        }
    }

    fn is_recent(&self, path: &Path) -> bool {
        self.inner
            .lock()
            .ok()
            .and_then(|m| m.get(path).map(|t| t.elapsed() < OWN_WRITE_GRACE))
            .unwrap_or(false)
    }
}

pub struct ConfigWatcher {
    debouncer: Mutex<Debouncer<notify::RecommendedWatcher>>,
    watched: Mutex<HashMap<PathBuf, RecursiveMode>>,
    interests: Arc<Mutex<Interests>>,
}

/// Parent-directory watches must not turn every nearby source/transcript edit into a rescan.
#[derive(Default)]
struct Interests {
    files: HashSet<PathBuf>,
    trees: HashSet<PathBuf>,
    probes: HashSet<PathBuf>,
    children: HashSet<PathBuf>,
    aliases: HashMap<PathBuf, HashSet<PathBuf>>,
}

impl Interests {
    fn includes(&self, path: &Path) -> bool {
        self.files.contains(path)
            || self.probes.contains(path)
            || path
                .parent()
                .is_some_and(|parent| self.children.contains(parent))
            || self.trees.iter().any(|root| path.starts_with(root))
    }

    fn expand(&self, path: PathBuf) -> Vec<String> {
        let mut paths = vec![path.to_string_lossy().into_owned()];
        if let Some(aliases) = self.aliases.get(&path) {
            paths.extend(aliases.iter().map(|p| p.to_string_lossy().into_owned()));
        }
        paths
    }
}

impl ConfigWatcher {
    pub fn start(app: AppHandle, recent: Arc<RecentWrites>) -> notify::Result<Self> {
        let interests = Arc::new(Mutex::new(Interests::default()));
        let event_interests = interests.clone();
        let debouncer = new_debouncer(
            Duration::from_millis(300),
            move |res: DebounceEventResult| match res {
                Ok(events) => {
                    let Ok(interests) = event_interests.lock() else {
                        return;
                    };
                    let mut paths: Vec<String> = events
                        .into_iter()
                        .map(|e| e.path)
                        .filter(|p| !is_noise(p))
                        .filter(|p| !recent.is_recent(p))
                        .filter(|p| interests.includes(p))
                        .flat_map(|p| interests.expand(p))
                        .collect();
                    paths.sort();
                    paths.dedup();
                    if !paths.is_empty() {
                        log::debug!("config changed: {paths:?}");
                        let _ = app.emit(EVENT_NAME, ConfigChanged { paths });
                    }
                }
                Err(e) => log::warn!("watch error: {e}"),
            },
        )?;
        Ok(Self {
            debouncer: Mutex::new(debouncer),
            watched: Mutex::new(HashMap::new()),
            interests,
        })
    }

    /// Watch the fixed home locations. Files are watched via their parent directory so
    /// absent files never error; directories are watched recursively only when present.
    pub fn watch_homes(&self, homes: &Homes) {
        let mut files = vec![
            homes.claude_state.clone(),
            homes.claude_dir.join("settings.json"),
            homes.claude_dir.join("settings.local.json"),
            homes.claude_dir.join("mcp-needs-auth-cache.json"),
            homes.claude_dir.join("CLAUDE.md"),
            homes.codex_home.join("config.toml"),
            homes.codex_home.join("hooks.json"),
            homes.codex_home.join("AGENTS.md"),
            homes.codex_home.join("AGENTS.override.md"),
            homes.agents_dir.join(".skill-lock.json"),
        ];
        let dirs = vec![
            homes.claude_dir.join("skills"),
            homes.claude_dir.join("agents"),
            homes.claude_dir.join("commands"),
            homes.claude_dir.join("rules"),
            homes.claude_dir.join("agent-memory"),
            homes
                .claude_dir
                .join("plugins")
                .join("installed_plugins.json"),
            homes.codex_home.join("skills"),
            homes.codex_home.join("agents"),
            homes.codex_home.join("automations"),
            homes.codex_home.join("memories"),
            homes.agents_dir.join("skills"),
        ];
        files.extend(dirs.iter().filter(|p| p.is_file()).cloned());
        for f in files {
            self.watch_file(&f);
        }
        for d in dirs {
            self.watch_dir(&d);
        }
        for name in ["settings.json", "settings.local.json"] {
            self.watch_custom_store(&homes.claude_dir.join(name), homes);
        }
        if !homes.sandbox {
            self.watch_custom_store(&crate::claude::settings::managed_settings_path(), homes);
        }
        // Watch only the store directories and their memory subtrees, never transcript trees.
        let projects = homes.claude_dir.join("projects");
        if let Ok(mut interests) = self.interests.lock() {
            interests.children.insert(projects.clone());
        }
        self.watch_probe(&projects);
        if let Ok(entries) = std::fs::read_dir(&projects) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|t| t.is_dir()) {
                    self.watch_probe(&entry.path());
                    self.watch_dir(&entry.path().join("memory"));
                }
            }
        }
    }

    /// Watch the per-project config locations of tracked projects.
    pub fn watch_project(&self, root: &Path, homes: &Homes) {
        if !root.is_dir() {
            return;
        }
        self.watch_file(&root.join(".mcp.json"));
        for name in ["settings.json", "settings.local.json"] {
            self.watch_custom_store(&root.join(".claude").join(name), homes);
        }
        if homes.home.starts_with(root) {
            return;
        }
        for d in [
            root.join(".claude"),
            root.join(".agents").join("skills"),
            root.join(".codex"),
        ] {
            self.watch_dir(&d);
        }
    }

    fn watch_custom_store(&self, settings: &Path, homes: &Homes) {
        let Some(value) = std::fs::read_to_string(settings)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        else {
            return;
        };
        let Some(raw) = value
            .get("autoMemoryDirectory")
            .and_then(|value| value.as_str())
        else {
            return;
        };
        let dir = homes.expand_tilde(raw);
        if dir.is_absolute() {
            self.watch_dir(&dir);
        }
    }

    /// Canonical targets and aliases both invalidate the logical editor query.
    pub fn watch_memories(&self, memories: &[MemoryFile]) {
        for memory in memories {
            let target = PathBuf::from(&memory.file);
            self.watch_file(&target);
            for alias in &memory.paths {
                let alias = PathBuf::from(alias);
                self.watch_file(&alias);
                if let Ok(mut interests) = self.interests.lock() {
                    interests
                        .aliases
                        .entry(target.clone())
                        .or_default()
                        .insert(alias.clone());
                    interests
                        .aliases
                        .entry(alias)
                        .or_default()
                        .insert(target.clone());
                }
            }
            if memory.category == MemoryCategory::Memory || memory.source == MemorySource::Rule {
                self.watch_dir(Path::new(&memory.dir));
            }
        }
    }

    fn watch_file(&self, file: &Path) {
        if let Ok(mut interests) = self.interests.lock() {
            interests.files.insert(file.to_path_buf());
        }
        if let Some(parent) = file.parent() {
            if parent.is_dir() {
                self.add(parent, RecursiveMode::NonRecursive);
            }
        }
    }

    fn watch_dir(&self, dir: &Path) {
        if let Ok(mut interests) = self.interests.lock() {
            interests.trees.insert(dir.to_path_buf());
        }
        if dir.is_dir() {
            self.add(dir, RecursiveMode::Recursive);
        } else {
            self.watch_probe(dir);
        }
    }

    /// Rearm missing locations after their nearest existing parent reports creation.
    fn watch_probe(&self, dir: &Path) {
        let mut cursor = dir;
        loop {
            if let Ok(mut interests) = self.interests.lock() {
                interests.probes.insert(cursor.to_path_buf());
            }
            if cursor.is_dir() {
                self.add(cursor, RecursiveMode::NonRecursive);
                break;
            }
            let Some(parent) = cursor.parent() else { break };
            cursor = parent;
        }
    }

    fn add(&self, path: &Path, mode: RecursiveMode) {
        let Ok(mut watched) = self.watched.lock() else {
            return;
        };
        if watched
            .get(path)
            .is_some_and(|old| *old == RecursiveMode::Recursive || *old == mode)
        {
            return;
        }
        if let Ok(mut d) = self.debouncer.lock() {
            if watched.contains_key(path) {
                let _ = d.watcher().unwatch(path);
            }
            if let Err(e) = d.watcher().watch(path, mode) {
                log::warn!("cannot watch {}: {e}", path.display());
                watched.remove(path);
            } else {
                watched.insert(path.to_path_buf(), mode);
            }
        }
    }
}

fn is_noise(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name == ".DS_Store"
        || name.starts_with(".ace-")
        || name.ends_with(".tmp")
        || name.ends_with("~")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_interests_exclude_transcripts_and_unrelated_source_edits() {
        let mut interests = Interests::default();
        interests.files.insert(PathBuf::from("/codex/AGENTS.md"));
        interests
            .trees
            .insert(PathBuf::from("/claude/projects/project/memory"));
        interests
            .probes
            .insert(PathBuf::from("/claude/projects/project/memory"));
        interests.children.insert(PathBuf::from("/claude/projects"));
        assert!(interests.includes(Path::new("/codex/AGENTS.md")));
        assert!(!interests.includes(Path::new("/project/AGENTS.md")));
        assert!(!interests.includes(Path::new("/project/CLAUDE.md")));
        assert!(interests.includes(Path::new("/claude/projects/project/memory/MEMORY.md")));
        assert!(interests.includes(Path::new("/claude/projects/new-project")));
        assert!(!interests.includes(Path::new("/project/src/index.ts")));
        assert!(!interests.includes(Path::new("/claude/projects/project/transcript.jsonl")));
        assert!(!interests.includes(Path::new(
            "/claude/projects/project/subagents/session.jsonl"
        )));
    }

    #[test]
    fn target_events_also_invalidate_linked_editor_paths() {
        let mut interests = Interests::default();
        interests.aliases.insert(
            PathBuf::from("/shared/instructions.md"),
            HashSet::from([PathBuf::from("/project/CLAUDE.md")]),
        );
        let paths = interests.expand(PathBuf::from("/shared/instructions.md"));
        assert!(paths.contains(&"/shared/instructions.md".to_string()));
        assert!(paths.contains(&"/project/CLAUDE.md".to_string()));
    }
}
