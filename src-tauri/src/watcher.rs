//! Filesystem watcher that tells the webview when config files change on disk.

use crate::fsutil::Homes;
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
    watched: Mutex<HashSet<PathBuf>>,
}

impl ConfigWatcher {
    pub fn start(app: AppHandle, recent: Arc<RecentWrites>) -> notify::Result<Self> {
        let debouncer = new_debouncer(
            Duration::from_millis(300),
            move |res: DebounceEventResult| match res {
                Ok(events) => {
                    let mut paths: Vec<String> = events
                        .into_iter()
                        .map(|e| e.path)
                        .filter(|p| !is_noise(p))
                        .filter(|p| !recent.is_recent(p))
                        .map(|p| p.to_string_lossy().into_owned())
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
            watched: Mutex::new(HashSet::new()),
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
            homes.codex_home.join("config.toml"),
            homes.codex_home.join("hooks.json"),
            homes.agents_dir.join(".skill-lock.json"),
        ];
        let dirs = vec![
            homes.claude_dir.join("skills"),
            homes.claude_dir.join("agents"),
            homes.claude_dir.join("commands"),
            homes
                .claude_dir
                .join("plugins")
                .join("installed_plugins.json"),
            homes.codex_home.join("skills"),
            homes.codex_home.join("agents"),
            homes.codex_home.join("automations"),
            homes.agents_dir.join("skills"),
        ];
        files.extend(dirs.iter().filter(|p| p.is_file()).cloned());
        for f in files {
            self.watch_file(&f);
        }
        for d in dirs.into_iter().filter(|p| p.is_dir()) {
            self.watch_dir(&d);
        }
    }

    /// Watch the per-project config locations of tracked projects.
    pub fn watch_project(&self, root: &Path) {
        if !root.is_dir() {
            return;
        }
        self.watch_file(&root.join(".mcp.json"));
        for d in [
            root.join(".claude"),
            root.join(".agents").join("skills"),
            root.join(".codex"),
        ] {
            if d.is_dir() {
                self.watch_dir(&d);
            }
        }
    }

    fn watch_file(&self, file: &Path) {
        if let Some(parent) = file.parent() {
            if parent.is_dir() {
                self.add(parent, RecursiveMode::NonRecursive);
            }
        }
    }

    fn watch_dir(&self, dir: &Path) {
        self.add(dir, RecursiveMode::Recursive);
    }

    fn add(&self, path: &Path, mode: RecursiveMode) {
        let Ok(mut watched) = self.watched.lock() else {
            return;
        };
        if !watched.insert(path.to_path_buf()) {
            return;
        }
        if let Ok(mut d) = self.debouncer.lock() {
            if let Err(e) = d.watcher().watch(path, mode) {
                log::warn!("cannot watch {}: {e}", path.display());
                watched.remove(path);
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
