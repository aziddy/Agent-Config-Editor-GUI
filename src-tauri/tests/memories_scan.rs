//! Memory discovery exercises miniature homes only; it never reads the user's real memories.

use agent_config_editor_lib::fsutil::{self, Homes};
use agent_config_editor_lib::model::{
    Agent, AppSettings, MemoryCategory, MemoryFile, MemorySource, Scope, Snapshot,
};
use agent_config_editor_lib::scan::scan_all;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

struct Home {
    _temp: tempfile::TempDir,
    root: PathBuf,
}

impl Home {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        Self {
            root: temp.path().to_path_buf(),
            _temp: temp,
        }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    fn put(&self, relative: &str, text: &str) -> PathBuf {
        let file = self.path(relative);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, text).unwrap();
        file
    }

    fn track(&self, roots: &[PathBuf]) {
        let projects = roots
            .iter()
            .map(|root| (fsutil::display(root), json!({})))
            .collect::<serde_json::Map<_, _>>();
        self.put(".claude.json", &json!({ "projects": projects }).to_string());
        let mut toml = String::from("project_doc_fallback_filenames = [\"TEAM_GUIDE.md\"]\n");
        for root in roots {
            fs::create_dir_all(root).unwrap();
            toml.push_str(&format!(
                "[projects.{}]\ntrust_level = \"trusted\"\n",
                serde_json::to_string(&fsutil::display(root)).unwrap()
            ));
        }
        self.put(".codex/config.toml", &toml);
    }

    fn scan(&self) -> Snapshot {
        scan_all(&Homes::for_test(&self.root), &AppSettings::default())
    }
}

fn canonical(path: &Path) -> String {
    fsutil::display(&fs::canonicalize(path).unwrap())
}

fn memory<'a>(snapshot: &'a Snapshot, path: &Path) -> &'a MemoryFile {
    let target = canonical(path);
    let matches: Vec<_> = snapshot
        .memories
        .iter()
        .filter(|memory| memory.file == target)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected one memory for {}",
        path.display()
    );
    matches[0]
}

#[test]
fn discovers_orphan_claude_and_nested_codex_memories_without_content_deduplication() {
    let home = Home::new();
    let orphan = home.put(
        ".claude/projects/-missing-repository/memory/topic.md",
        "same text\n",
    );
    let index = home.put(".codex/memories/MEMORY.md", "same text\n");
    let nested = home.put(
        ".codex/memories/rollout_summaries/earlier.md",
        "same text\n",
    );
    home.put(
        ".codex/automations/daily/automation.toml",
        "name = \"Daily\"\n",
    );
    let automation = home.put(".codex/automations/daily/memory.md", "Prior run notes\n");
    home.put(
        ".codex/automations/daily/report-today.md",
        "Output, not memory\n",
    );

    let snapshot = home.scan();
    assert_eq!(snapshot.memories.len(), 4);
    let orphan = memory(&snapshot, &orphan);
    assert_eq!(orphan.source, MemorySource::ClaudeProject);
    assert_eq!(orphan.category, MemoryCategory::Memory);
    assert_eq!(orphan.agents, [Agent::Claude]);
    assert!(orphan.scopes.contains(&Scope::User));
    assert_eq!(memory(&snapshot, &index).source, MemorySource::CodexMemory);
    assert_eq!(memory(&snapshot, &nested).source, MemorySource::CodexMemory);
    assert_eq!(
        memory(&snapshot, &automation).source,
        MemorySource::Automation
    );
    assert_eq!(memory(&snapshot, &automation).name, "Daily");
    assert_eq!(orphan.content_hash, memory(&snapshot, &index).content_hash);
    assert_ne!(orphan.id, memory(&snapshot, &index).id);
    for entry in &snapshot.memories {
        assert_eq!(snapshot.file_hashes[&entry.file], entry.content_hash);
    }
}

#[test]
fn keeps_global_instructions_rules_and_fallbacks_but_excludes_repo_instruction_variants() {
    let home = Home::new();
    let repo = home.path("projects/repo");
    home.track(std::slice::from_ref(&repo));
    let global_claude = home.put(".claude/CLAUDE.md", "Personal Claude instructions\n");
    let global_codex = home.put(".codex/AGENTS.md", "Personal Codex instructions\n");
    let home_claude = home.put("CLAUDE.md", "Home Claude instructions\n");
    let home_codex = home.put("AGENTS.md", "Home instructions\n");
    let global_override = home.put(".codex/AGENTS.override.md", "Global override\n");
    home.put(
        "projects/repo/.codex/config.toml",
        "project_doc_fallback_filenames = [\"TEAM_GUIDE.md\", \"AGENTS.md\", \"AGENTS.override.md\", \"CLAUDE.md\", \"CLAUDE.local.md\"]\n",
    );
    let mut excluded = Vec::new();
    for directory in ["projects", "projects/repo", "projects/repo/packages/ui"] {
        for name in [
            "AGENTS.md",
            "AGENTS.override.md",
            "CLAUDE.md",
            "CLAUDE.local.md",
            ".claude/CLAUDE.md",
        ] {
            excluded.push(home.put(&format!("{directory}/{name}"), "Repository instructions\n"));
        }
    }
    home.put("projects/repo/.gitignore", "CLAUDE.local.md\n");
    let fallback = home.put(
        "projects/repo/packages/ui/TEAM_GUIDE.md",
        "Codex fallback\n",
    );
    let rule = home.put(
        "projects/repo/.claude/rules/frontend/style.md",
        "Path scoped rules\n",
    );
    for relative in [
        "projects/repo/node_modules/vendor/TEAM_GUIDE.md",
        "projects/repo/target/debug/TEAM_GUIDE.md",
        "projects/repo/.git/hooks/TEAM_GUIDE.md",
        "projects/repo/.claude/plugins/demo/TEAM_GUIDE.md",
    ] {
        home.put(relative, "Must not be indexed\n");
    }

    let snapshot = home.scan();
    for file in [
        &global_claude,
        &global_codex,
        &home_claude,
        &home_codex,
        &global_override,
        &fallback,
    ] {
        assert_eq!(
            memory(&snapshot, file).category,
            MemoryCategory::Instruction
        );
    }
    for file in excluded {
        assert!(
            !snapshot
                .memories
                .iter()
                .any(|entry| entry.file == canonical(&file)),
            "excluded {}",
            file.display()
        );
    }
    assert_eq!(memory(&snapshot, &rule).source, MemorySource::Rule);
    assert!(snapshot.memories.iter().all(|entry| {
        !entry.paths.iter().any(|path| {
            path.contains("/node_modules/")
                || path.contains("/target/")
                || path.contains("/.git/")
                || path.contains("/.claude/plugins/")
        })
    }));
}

#[test]
fn custom_and_subagent_memories_remain_visible_when_memory_is_disabled() {
    let home = Home::new();
    let repo = home.path("projects/repo");
    home.track(std::slice::from_ref(&repo));
    home.put(
        ".claude/settings.json",
        &json!({
            "autoMemoryEnabled": false,
            "autoMemoryDirectory": fsutil::display(&home.path("custom-memories"))
        })
        .to_string(),
    );
    let custom = home.put("custom-memories/MEMORY.md", "Custom notes\n");
    let user_agent = home.put(
        ".claude/agent-memory/reviewer/MEMORY.md",
        "User agent memory\n",
    );
    let project_agent = home.put(
        "projects/repo/.claude/agent-memory/reviewer/notes.md",
        "Project memory\n",
    );
    let local_agent = home.put(
        "projects/repo/.claude/agent-memory-local/reviewer/notes.md",
        "Local memory\n",
    );

    let snapshot = home.scan();
    assert_eq!(memory(&snapshot, &custom).category, MemoryCategory::Memory);
    let user = memory(&snapshot, &user_agent);
    assert_eq!(user.source, MemorySource::ClaudeSubagent);
    assert!(user.scopes.contains(&Scope::User));
    for path in [&project_agent, &local_agent] {
        let entry = memory(&snapshot, path);
        assert_eq!(entry.source, MemorySource::ClaudeSubagent);
        assert!(entry.scopes.contains(&Scope::Project {
            root: fsutil::display(&repo)
        }));
    }
}

#[cfg(unix)]
#[test]
fn symlinked_instruction_files_merge_aliases_and_agents_without_following_directory_links() {
    let home = Home::new();
    let target = home.put("shared/team.md", "Shared physical instructions\n");
    let claude = home.path(".claude/CLAUDE.md");
    let codex = home.path(".codex/AGENTS.md");
    for alias in [&claude, &codex] {
        fs::create_dir_all(alias.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&target, alias).unwrap();
    }
    let repo = home.path("projects/repo");
    home.track(std::slice::from_ref(&repo));
    home.put(
        "outside/AGENTS.md",
        "Directory links must not be followed\n",
    );
    std::os::unix::fs::symlink(home.path("outside"), repo.join("linked-directory")).unwrap();

    let snapshot = home.scan();
    let entry = memory(&snapshot, &target);
    assert!(entry.agents.contains(&Agent::Claude));
    assert!(entry.agents.contains(&Agent::Codex));
    assert!(entry.paths.contains(&fsutil::display(&claude)));
    assert!(entry.paths.contains(&fsutil::display(&codex)));
    assert_eq!(snapshot.memories.len(), 1);
}

#[test]
fn unreadable_utf8_warns_without_hiding_other_memories() {
    let home = Home::new();
    let good = home.put(".codex/memories/MEMORY.md", "Readable\n");
    let binary = home.put(".codex/memories/binary.md", "");
    fs::write(&binary, [0xff, 0xfe, 0x00]).unwrap();
    let snapshot = home.scan();
    memory(&snapshot, &good);
    assert_eq!(snapshot.memories.len(), 1);
    assert!(snapshot.warnings.iter().any(|warning| {
        warning
            .file
            .as_deref()
            .is_some_and(|path| path == fsutil::display(&binary) || path == canonical(&binary))
    }));
}

#[cfg(unix)]
#[test]
fn instruction_alias_promotes_a_memory_record_without_losing_its_associations() {
    let home = Home::new();
    let repo = home.path("projects/repo");
    home.track(std::slice::from_ref(&repo));
    let target = home.put(
        ".codex/memories/shared.md",
        "Shared memory and instructions\n",
    );
    let claude = home.path(".claude/CLAUDE.md");
    let shared = home.path(".codex/AGENTS.md");
    fs::create_dir_all(claude.parent().unwrap()).unwrap();
    for alias in [&claude, &shared] {
        std::os::unix::fs::symlink(&target, alias).unwrap();
    }
    let excluded = [repo.join("CLAUDE.md"), repo.join("AGENTS.md")];
    for alias in &excluded {
        std::os::unix::fs::symlink(&target, alias).unwrap();
    }

    let snapshot = home.scan();
    let entry = memory(&snapshot, &target);
    assert_eq!(snapshot.memories.len(), 1);
    assert_eq!(entry.category, MemoryCategory::Instruction);
    assert_eq!(entry.source, MemorySource::Instruction);
    assert!(!entry.generated);
    assert!(entry.agents.contains(&Agent::Claude));
    assert!(entry.agents.contains(&Agent::Codex));
    assert!(entry.scopes.contains(&Scope::User));
    assert_eq!(entry.scopes, [Scope::User]);
    for alias in [&target, &claude, &shared] {
        assert!(entry.paths.contains(&fsutil::display(alias)));
    }
    for alias in &excluded {
        assert!(!entry.paths.contains(&fsutil::display(alias)));
    }
}

#[test]
fn tracking_home_does_not_recursively_inventory_untracked_directories() {
    let home = Home::new();
    let tracked = home.path("projects/tracked");
    home.track(&[home.root.clone(), tracked.clone()]);
    home.put("untracked/deep/TEAM_GUIDE.md", "Not a tracked project\n");
    let instruction = home.put(
        "projects/tracked/deep/TEAM_GUIDE.md",
        "Tracked nested instructions\n",
    );
    let snapshot = home.scan();
    let entry = memory(&snapshot, &instruction);
    assert!(entry.scopes.contains(&Scope::Project {
        root: fsutil::display(&tracked)
    }));
    assert!(snapshot
        .memories
        .iter()
        .all(|entry| { !entry.paths.iter().any(|path| path.contains("/untracked/")) }));
}

#[test]
fn worktrees_share_verified_store_ownership_but_colliding_names_remain_unassigned() {
    let home = Home::new();
    let main = home.path("projects/main");
    let worktree = home.path("projects/worktree");
    let dashed = home.path("projects/a-b");
    let nested = home.path("projects/a/b");
    home.track(&[main.clone(), worktree.clone(), dashed, nested]);
    fs::create_dir_all(main.join(".git/worktrees/branch")).unwrap();
    home.put("projects/main/.git/worktrees/branch/commondir", "../..\n");
    home.put(
        "projects/worktree/.git",
        &format!("gitdir: {}\n", main.join(".git/worktrees/branch").display()),
    );
    let encode = |path: &Path| {
        fsutil::display(path)
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character
                } else {
                    '-'
                }
            })
            .collect::<String>()
    };
    let shared = home.put(
        &format!(".claude/projects/{}/memory/MEMORY.md", encode(&main)),
        "Main repository notes\n",
    );
    let collision = home.put(
        &format!(
            ".claude/projects/{}/memory/MEMORY.md",
            encode(&home.path("projects/a-b"))
        ),
        "Ownership cannot be guessed\n",
    );

    let snapshot = home.scan();
    let shared = memory(&snapshot, &shared);
    for root in [&main, &worktree] {
        assert!(shared.scopes.contains(&Scope::Project {
            root: fsutil::display(root)
        }));
    }
    assert_eq!(memory(&snapshot, &collision).scopes, [Scope::User]);
}

#[test]
fn overlapping_projects_keep_all_scopes_and_pruned_worktrees_are_scanned_independently() {
    let home = Home::new();
    let repo = home.path("projects/repo");
    let child = repo.join("packages/app");
    let branch = repo.join(".claude/worktrees/branch");
    home.track(&[repo.clone(), child.clone(), branch.clone()]);
    let nested = home.put(
        "projects/repo/packages/app/src/nested/TEAM_GUIDE.md",
        "Instructions in overlapping project roots\n",
    );
    let branch_instruction = home.put(
        "projects/repo/.claude/worktrees/branch/deep/TEAM_GUIDE.md",
        "Instructions in independently tracked worktree\n",
    );

    let snapshot = home.scan();
    let nested = memory(&snapshot, &nested);
    for root in [&repo, &child] {
        assert!(nested.scopes.contains(&Scope::Project {
            root: fsutil::display(root)
        }));
    }
    assert!(memory(&snapshot, &branch_instruction)
        .scopes
        .contains(&Scope::Project {
            root: fsutil::display(&branch)
        }));
    assert_eq!(snapshot.memories.len(), 2);
}

#[test]
fn instruction_filenames_inside_memory_stores_remain_memories() {
    let home = Home::new();
    let repo = home.path("projects/repo");
    home.track(std::slice::from_ref(&repo));
    for (store, source) in [
        (".codex/memories", MemorySource::CodexMemory),
        (
            ".claude/projects/-missing-repository/memory",
            MemorySource::ClaudeProject,
        ),
    ] {
        for name in [
            "AGENTS.md",
            "CLAUDE.md",
            "CLAUDE.local.md",
            "AGENTS.override.md",
        ] {
            let file = home.put(&format!("{store}/{name}"), "Saved memory\n");
            let snapshot = home.scan();
            assert_eq!(memory(&snapshot, &file).source, source);
            assert_eq!(memory(&snapshot, &file).category, MemoryCategory::Memory);
        }
    }
}

#[test]
fn malformed_opposite_agent_project_configs_produce_one_warning_each() {
    let home = Home::new();
    let claude = home.path("projects/claude-only");
    let codex = home.path("projects/codex-only");
    home.put(
        ".claude.json",
        &json!({ "projects": { fsutil::display(&claude): {} } }).to_string(),
    );
    home.put(
        ".codex/config.toml",
        &format!(
            "[projects.{}]\ntrust_level = \"trusted\"\n",
            serde_json::to_string(&fsutil::display(&codex)).unwrap()
        ),
    );
    let claude_config = home.put(
        "projects/codex-only/.claude/settings.json",
        "{ invalid json",
    );
    let codex_config = home.put("projects/claude-only/.codex/config.toml", "[broken");

    let snapshot = home.scan();
    for file in [&claude_config, &codex_config] {
        let file_name = fsutil::display(file);
        let matching: Vec<_> = snapshot
            .warnings
            .iter()
            .filter(|warning| warning.file.as_deref() == Some(file_name.as_str()))
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "one parse warning for {}",
            file.display()
        );
    }
}

#[test]
fn nested_project_fallback_names_do_not_leak_into_sibling_directories() {
    let home = Home::new();
    let repo = home.path("projects/repo");
    let child = repo.join("packages/app");
    home.track(&[repo.clone(), child.clone()]);
    let mut config = String::new();
    for root in [&repo, &child] {
        config.push_str(&format!(
            "[projects.{}]\ntrust_level = \"trusted\"\n",
            serde_json::to_string(&fsutil::display(root)).unwrap()
        ));
    }
    home.put(".codex/config.toml", &config);
    home.put(
        "projects/repo/packages/app/.codex/config.toml",
        "project_doc_fallback_filenames = [\"TEAM_GUIDE.md\"]\n",
    );
    let applicable = home.put(
        "projects/repo/packages/app/TEAM_GUIDE.md",
        "Child project instructions\n",
    );
    let sibling = home.put(
        "projects/repo/packages/other/TEAM_GUIDE.md",
        "Not configured as instructions in this project\n",
    );

    let snapshot = home.scan();
    assert_eq!(
        memory(&snapshot, &applicable).category,
        MemoryCategory::Instruction
    );
    assert!(!snapshot.memories.iter().any(|entry| {
        entry.file == canonical(&sibling) || entry.paths.contains(&fsutil::display(&sibling))
    }));
}
