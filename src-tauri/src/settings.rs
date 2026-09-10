//! Persistence for the app's own settings (`ace-settings.json`).

use crate::error::{AppError, AppResult};
use crate::fsutil::{atomic_write, read_text_opt};
use crate::model::AppSettings;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join("ace-settings.json"),
        }
    }

    pub fn load(&self) -> AppResult<AppSettings> {
        match read_text_opt(&self.path)? {
            Some(text) => serde_json::from_str::<AppSettings>(&text)
                .map_err(|e| AppError::parse(e.to_string(), &self.path)),
            None => Ok(AppSettings::default()),
        }
    }

    pub fn save(&self, settings: &AppSettings) -> AppResult<()> {
        let text = serde_json::to_string_pretty(settings)?;
        atomic_write(&self.path, &format!("{text}\n"), None)
    }
}

/// Pick a sensible default editor command for this machine.
pub fn detect_editor_command() -> String {
    let candidates: &[(&str, &str)] = &[
        ("cursor", "cursor --goto {path}:{line}"),
        ("code", "code --goto {path}:{line}"),
        ("zed", "zed {path}:{line}"),
        ("subl", "subl {path}:{line}"),
    ];
    for (bin, template) in candidates {
        if which(bin).is_some() {
            return (*template).to_string();
        }
    }
    if cfg!(target_os = "macos") {
        "open -t {path}".to_string()
    } else {
        "xdg-open {path}".to_string()
    }
}

/// Minimal PATH lookup that skips cmux shims.
pub fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        if dir.to_string_lossy().contains("cmux-cli-shims") {
            continue;
        }
        let candidate = dir.join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Locate the real Codex binary, ignoring cmux shims.
pub fn detect_codex_binary() -> Option<PathBuf> {
    for fixed in ["/opt/homebrew/bin/codex", "/usr/local/bin/codex"] {
        let p = PathBuf::from(fixed);
        if p.is_file() {
            return Some(p);
        }
    }
    which("codex")
}
