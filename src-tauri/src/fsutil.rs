//! Filesystem helpers: home resolution, hashed reads, atomic writes, backups.

use crate::error::{AppError, AppResult, ErrorCode};
use crate::model::{BackupInfo, DirEntryInfo, HomesInfo};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Resolved configuration roots. `ACE_HOME` overrides the real home for sandboxed testing.
#[derive(Debug, Clone)]
pub struct Homes {
    pub home: PathBuf,
    pub claude_dir: PathBuf,
    pub claude_state: PathBuf,
    pub codex_home: PathBuf,
    pub agents_dir: PathBuf,
    pub sandbox: bool,
    pub read_only: bool,
    pub allow_project_writes: bool,
}

impl Homes {
    pub fn detect() -> AppResult<Homes> {
        let (home, sandbox) = match std::env::var_os("ACE_HOME") {
            Some(v) if !v.is_empty() => (PathBuf::from(v), true),
            _ => (
                dirs::home_dir()
                    .ok_or_else(|| AppError::new(ErrorCode::Io, "cannot resolve home directory"))?,
                false,
            ),
        };
        let codex_home = match std::env::var_os("CODEX_HOME") {
            Some(v) if !v.is_empty() && !sandbox => PathBuf::from(v),
            _ => home.join(".codex"),
        };
        Ok(Homes {
            claude_dir: home.join(".claude"),
            claude_state: home.join(".claude.json"),
            codex_home,
            agents_dir: home.join(".agents"),
            sandbox,
            read_only: env_flag("ACE_READ_ONLY"),
            allow_project_writes: env_flag("ACE_ALLOW_PROJECT_WRITES"),
            home,
        })
    }

    pub fn for_test(home: impl Into<PathBuf>) -> Homes {
        let home = home.into();
        Homes {
            claude_dir: home.join(".claude"),
            claude_state: home.join(".claude.json"),
            codex_home: home.join(".codex"),
            agents_dir: home.join(".agents"),
            sandbox: true,
            read_only: false,
            allow_project_writes: true,
            home,
        }
    }

    pub fn info(&self) -> HomesInfo {
        HomesInfo {
            home: display(&self.home),
            claude_dir: display(&self.claude_dir),
            claude_state: display(&self.claude_state),
            codex_home: display(&self.codex_home),
            agents_dir: display(&self.agents_dir),
            sandbox: self.sandbox,
            read_only: self.read_only,
        }
    }

    /// Enforce `ACE_READ_ONLY` and the `ACE_HOME` sandbox boundary.
    pub fn check_writable(&self, path: &Path) -> AppResult<()> {
        if self.read_only {
            return Err(AppError::read_only(
                "ACE_READ_ONLY is set; writes are disabled",
                path,
            ));
        }
        if self.sandbox && !self.allow_project_writes && !path.starts_with(&self.home) {
            return Err(AppError::read_only(
                "sandbox mode: refusing to write outside ACE_HOME (set ACE_ALLOW_PROJECT_WRITES=1 to allow)",
                path,
            ));
        }
        Ok(())
    }

    /// Expand a leading `~` against the resolved home.
    pub fn expand_tilde(&self, raw: &str) -> PathBuf {
        if raw == "~" {
            return self.home.clone();
        }
        if let Some(rest) = raw.strip_prefix("~/") {
            return self.home.join(rest);
        }
        PathBuf::from(raw)
    }
}

fn env_flag(name: &str) -> bool {
    matches!(
        std::env::var(name).ok().as_deref().map(str::trim),
        Some("1") | Some("true") | Some("yes")
    )
}

pub fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[derive(Debug, Clone)]
pub struct ReadMeta {
    pub text: String,
    pub hash: String,
    pub mtime_ms: i64,
    pub mode: u32,
}

pub fn mtime_ms(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn mode_of(meta: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        meta.permissions().mode() & 0o777
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        0o644
    }
}

pub fn read_text_with_meta(path: &Path) -> AppResult<ReadMeta> {
    let bytes = fs::read(path).map_err(|e| AppError::io(e, path))?;
    let meta = fs::metadata(path).map_err(|e| AppError::io(e, path))?;
    let text =
        String::from_utf8(bytes).map_err(|_| AppError::parse("file is not valid UTF-8", path))?;
    Ok(ReadMeta {
        hash: sha256_hex(text.as_bytes()),
        mtime_ms: mtime_ms(&meta),
        mode: mode_of(&meta),
        text,
    })
}

/// Read a file, returning `Ok(None)` when it does not exist.
pub fn read_text_opt(path: &Path) -> AppResult<Option<String>> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(AppError::io(e, path)),
    }
}

pub fn file_hash(path: &Path) -> Option<String> {
    fs::read(path).ok().map(|b| sha256_hex(&b))
}

pub fn language_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
    {
        Some(e) if e == "json" => "json",
        Some(e) if e == "toml" => "toml",
        Some(e) if e == "md" || e == "markdown" => "markdown",
        Some(e) if e == "yml" || e == "yaml" => "yaml",
        _ => "text",
    }
}

/// Write atomically: temp file in the same directory, fsync, chmod, rename.
pub fn atomic_write(path: &Path, text: &str, mode: Option<u32>) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::invalid(format!("path has no parent: {}", path.display())))?;
    fs::create_dir_all(parent).map_err(|e| AppError::io(e, parent))?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".ace-")
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(|e| AppError::io(e, parent))?;
    tmp.write_all(text.as_bytes())
        .map_err(|e| AppError::io(e, path))?;
    tmp.as_file()
        .sync_all()
        .map_err(|e| AppError::io(e, path))?;
    #[cfg(unix)]
    if let Some(m) = mode {
        fs::set_permissions(tmp.path(), fs::Permissions::from_mode(m))
            .map_err(|e| AppError::io(e, path))?;
    }
    tmp.persist(path).map_err(|e| AppError::io(e.error, path))?;
    Ok(())
}

pub fn list_dir(path: &Path) -> AppResult<Vec<DirEntryInfo>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(path).map_err(|e| AppError::io(e, path))? {
        let entry = entry.map_err(|e| AppError::io(e, path))?;
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".DS_Store" {
            continue;
        }
        out.push(DirEntryInfo {
            path: display(&entry.path()),
            name,
            is_dir: meta.is_dir(),
            size: meta.len(),
            mtime_ms: mtime_ms(&meta),
        });
    }
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

/// Timestamped copies of files before the app modifies them.
#[derive(Debug, Clone)]
pub struct BackupStore {
    root: PathBuf,
    limit: usize,
}

impl BackupStore {
    pub fn new(root: PathBuf, limit: usize) -> Self {
        Self { root, limit }
    }

    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit.max(1);
    }

    fn bucket_for(&self, original: &Path) -> PathBuf {
        let hash = sha256_hex(display(original).as_bytes());
        let base = original
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".to_string());
        self.root.join(format!("{}-{}", &hash[..12], base))
    }

    /// Copy `original` into the store. Returns `Ok(None)` when the file does not exist yet.
    pub fn backup(&self, original: &Path) -> AppResult<Option<PathBuf>> {
        let bytes = match fs::read(original) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(AppError::io(e, original)),
        };
        let bucket = self.bucket_for(original);
        fs::create_dir_all(&bucket).map_err(|e| AppError::io(e, &bucket))?;
        let meta_path = bucket.join("meta.json");
        if !meta_path.exists() {
            let meta = serde_json::json!({ "originalPath": display(original) });
            fs::write(&meta_path, serde_json::to_vec_pretty(&meta)?)
                .map_err(|e| AppError::io(e, &meta_path))?;
        }
        let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string();
        let target = bucket.join(format!("{stamp}.bak"));
        fs::write(&target, &bytes).map_err(|e| AppError::io(e, &target))?;
        self.prune(&bucket)?;
        Ok(Some(target))
    }

    fn prune(&self, bucket: &Path) -> AppResult<()> {
        let mut baks: Vec<PathBuf> = fs::read_dir(bucket)
            .map_err(|e| AppError::io(e, bucket))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("bak"))
            .collect();
        baks.sort();
        while baks.len() > self.limit {
            let victim = baks.remove(0);
            let _ = fs::remove_file(victim);
        }
        Ok(())
    }

    pub fn list(&self, original: &Path) -> AppResult<Vec<BackupInfo>> {
        let bucket = self.bucket_for(original);
        let mut out = Vec::new();
        let entries = match fs::read_dir(&bucket) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(AppError::io(e, &bucket)),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("bak") {
                continue;
            }
            let meta = entry.metadata().map_err(|e| AppError::io(e, &path))?;
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            out.push(BackupInfo {
                id: self.id_for(&path),
                path: display(&path),
                original_path: display(original),
                created_at: stamp_to_rfc3339(&stem),
                size: meta.len(),
            });
        }
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(out)
    }

    fn id_for(&self, backup_path: &Path) -> String {
        backup_path
            .strip_prefix(&self.root)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| display(backup_path))
    }

    /// Resolve a backup id (relative path inside the store) to the backup file and its original path.
    pub fn resolve(&self, id: &str) -> AppResult<(PathBuf, PathBuf)> {
        if id.contains("..") || id.starts_with('/') {
            return Err(AppError::invalid("invalid backup id"));
        }
        let backup_path = self.root.join(id);
        if !backup_path.is_file() {
            return Err(AppError::not_found("backup not found", &backup_path));
        }
        let bucket = backup_path
            .parent()
            .ok_or_else(|| AppError::invalid("invalid backup id"))?;
        let meta: serde_json::Value = serde_json::from_slice(
            &fs::read(bucket.join("meta.json")).map_err(|e| AppError::io(e, bucket))?,
        )?;
        let original = meta
            .get("originalPath")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::parse("backup meta.json missing originalPath", bucket))?;
        Ok((backup_path, PathBuf::from(original)))
    }
}

fn stamp_to_rfc3339(stem: &str) -> String {
    chrono::NaiveDateTime::parse_from_str(stem, "%Y%m%dT%H%M%S%.3fZ")
        .map(|dt| dt.and_utc().to_rfc3339())
        .unwrap_or_else(|_| stem.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_creates_parent_and_preserves_mode() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("nested/deeper/file.toml");
        atomic_write(&target, "a = 1\n", Some(0o600)).unwrap();
        let meta = fs::metadata(&target).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "a = 1\n");
        #[cfg(unix)]
        assert_eq!(mode_of(&meta), 0o600);
    }

    #[test]
    fn backup_store_round_trips_and_prunes() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("config.toml");
        fs::write(&original, "v1").unwrap();
        let store = BackupStore::new(dir.path().join("backups"), 2);
        for v in ["v1", "v2", "v3"] {
            fs::write(&original, v).unwrap();
            store.backup(&original).unwrap().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let list = store.list(&original).unwrap();
        assert_eq!(list.len(), 2, "pruned to limit");
        let (bak, orig) = store.resolve(&list[0].id).unwrap();
        assert_eq!(orig, original);
        assert_eq!(fs::read_to_string(bak).unwrap(), "v3");
    }

    #[test]
    fn language_detection() {
        assert_eq!(language_for(Path::new("a/SKILL.md")), "markdown");
        assert_eq!(language_for(Path::new("config.toml")), "toml");
        assert_eq!(language_for(Path::new(".mcp.json")), "json");
        assert_eq!(language_for(Path::new("default.rules")), "text");
    }
}
