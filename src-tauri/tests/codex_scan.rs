//! Integration test for the Codex readers against `tests/fixtures/codex_home`.
//!
//! The fixture is a miniature `$HOME`. Files that need absolute paths are stored as `*.tpl` with a
//! `__ROOT__` placeholder; the tree is copied into a temp dir and the placeholder substituted.

use agent_config_editor_lib::fsutil::Homes;
use agent_config_editor_lib::model::{
    AppSettings, AutomationTarget, Locator, McpTransport, Scope, Snapshot, Warning,
};
use agent_config_editor_lib::scan::scan_all;
use pretty_assertions::assert_eq;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("codex_home")
}

/// Copy the fixture into a fresh temp dir, substituting `__ROOT__` in `*.tpl` files.
fn make_home() -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();
    let root_str = root.to_string_lossy().into_owned();
    let src = fixture_root();
    for entry in walkdir::WalkDir::new(&src).sort_by_file_name() {
        let entry = entry.expect("walk fixture");
        let rel = entry.path().strip_prefix(&src).unwrap();
        let ft = entry.file_type();
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            fs::create_dir_all(root.join(rel)).unwrap();
            continue;
        }
        let dest = root.join(rel);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        match rel.extension().and_then(|e| e.to_str()) {
            Some("tpl") => {
                let text = fs::read_to_string(entry.path()).unwrap();
                fs::write(dest.with_extension(""), text.replace("__ROOT__", &root_str)).unwrap();
            }
            _ => {
                fs::copy(entry.path(), &dest).unwrap();
            }
        }
    }
    // Make `v1` unambiguously the newest version dir and add the `latest` symlink Codex leaves
    // behind, which must be ignored.
    let plugin_dir = root.join(".codex/plugins/cache/test-mkt/demo");
    if let Ok(f) = fs::File::open(plugin_dir.join("v1")) {
        let _ = f.set_modified(SystemTime::now() + Duration::from_secs(5));
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(plugin_dir.join("v1"), plugin_dir.join("latest")).unwrap();
    (tmp, root)
}

fn scan(root: &Path) -> Snapshot {
    scan_all(&Homes::for_test(root), &AppSettings::default())
}

/// Warnings that concern the Codex side of the fixture (the Claude readers run on it too).
fn codex_warnings(snapshot: &Snapshot) -> Vec<&Warning> {
    snapshot
        .warnings
        .iter()
        .filter(|w| !w.file.as_deref().is_some_and(|f| f.contains("/.claude")))
        .collect()
}

fn p(root: &Path, rel: &str) -> String {
    root.join(rel).to_string_lossy().into_owned()
}

#[test]
fn scans_the_fixture_home() {
    let (_tmp, root) = make_home();
    let snap = scan(&root);
    let config_toml = p(&root, ".codex/config.toml");
    let repo_a = p(&root, "projects/repo a");

    assert_eq!(
        codex_warnings(&snap),
        Vec::<&Warning>::new(),
        "clean fixture must not warn"
    );

    // ---- MCP servers -------------------------------------------------------------------------
    let mcp = |name: &str| {
        snap.mcp_servers
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("mcp server {name} missing"))
    };
    assert_eq!(snap.mcp_servers.len(), 6);

    let alpha = mcp("alpha");
    assert_eq!(alpha.id, "codex:mcp:user:alpha");
    assert_eq!(alpha.scope, Scope::User);
    assert_eq!(alpha.origin.file, config_toml);
    assert_eq!(
        alpha.origin.locator,
        Locator::TomlPath {
            path: vec!["mcp_servers".into(), "alpha".into()]
        }
    );
    match &alpha.transport {
        McpTransport::Stdio {
            command,
            args,
            env,
            cwd,
        } => {
            assert_eq!(command, "npx");
            assert_eq!(args, &["alpha-mcp@latest", "--stdio"]);
            assert_eq!(env["ALPHA_JSON"], Value::String(r#"{"a":1}"#.into()));
            assert_eq!(env["ALPHA_TOKEN"], Value::String("REDACTED".into()));
            assert_eq!(*cwd, None);
        }
        other => panic!("alpha should be stdio, got {other:?}"),
    }
    assert_eq!(alpha.startup_timeout_sec, Some(120.0));
    assert_eq!(alpha.tool_timeout_sec, Some(30.5));
    assert_eq!(alpha.enabled.enabled, Some(true));
    assert!(alpha.enabled.editable);
    assert!(alpha.extra.is_empty());

    let beta = mcp("beta");
    assert_eq!(beta.enabled.enabled, Some(false));
    let toggle = beta.enabled.toggle.as_ref().unwrap();
    assert_eq!(toggle.file, config_toml);
    assert_eq!(
        toggle.locator,
        Locator::TomlPath {
            path: vec!["mcp_servers".into(), "beta".into(), "enabled".into()]
        }
    );
    assert!(
        matches!(&beta.transport, McpTransport::Stdio { cwd: Some(c), args, .. } if c == "." && args.is_empty())
    );

    let docs = mcp("docs");
    match &docs.transport {
        McpTransport::Http {
            url,
            headers,
            bearer_token_env_var,
        } => {
            assert_eq!(url, "https://example.com/mcp");
            assert!(headers.is_empty());
            assert_eq!(bearer_token_env_var.as_deref(), Some("DOCS_TOKEN"));
        }
        other => panic!("docs should be http, got {other:?}"),
    }
    assert_eq!(
        docs.extra.get("oauth_resource"),
        Some(&Value::String("https://example.com/mcp".into()))
    );

    let proj = mcp("proj-local");
    assert_eq!(
        proj.scope,
        Scope::Project {
            root: repo_a.clone()
        }
    );
    assert_eq!(proj.id, format!("codex:mcp:project:{repo_a}:proj-local"));
    assert_eq!(
        proj.origin.file,
        p(&root, "projects/repo a/.codex/config.toml")
    );

    let demo_api = mcp("demo-api");
    assert_eq!(demo_api.from_plugin.as_deref(), Some("demo@test-mkt"));
    assert_eq!(
        demo_api.scope,
        Scope::Plugin {
            plugin_id: "demo@test-mkt".into()
        }
    );
    assert_eq!(demo_api.id, "codex:mcp:plugin:demo@test-mkt:demo-api");
    assert_eq!(
        demo_api.origin.file,
        p(&root, ".codex/plugins/cache/test-mkt/demo/v1/.mcp.json")
    );
    assert_eq!(
        demo_api.origin.locator,
        Locator::JsonPointer {
            pointer: "/mcpServers/demo-api".into()
        }
    );
    assert!(
        matches!(&demo_api.transport, McpTransport::Http { url, .. } if url == "https://demo.example.com/mcp")
    );
    assert_eq!(demo_api.enabled.enabled, Some(true));
    assert_eq!(
        demo_api.enabled.toggle.as_ref().unwrap().locator,
        Locator::TomlPath {
            path: vec!["plugins".into(), "demo@test-mkt".into(), "enabled".into()]
        }
    );
    let demo_local = mcp("demo-local");
    assert!(
        matches!(&demo_local.transport, McpTransport::Stdio { command, .. } if command == "./bin/demo-launcher")
    );
    assert_eq!(demo_local.startup_timeout_sec, Some(120.0));
    assert_eq!(
        demo_local.extra.get("env_vars"),
        Some(&serde_json::json!(["CODEX_HOME"]))
    );
    assert!(!demo_local.extra.contains_key("enabled"));

    // ---- Skills ------------------------------------------------------------------------------
    let skill = |name: &str| {
        snap.skills
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("skill {name} missing"))
    };
    assert_eq!(snap.skills.len(), 6);
    assert_eq!(
        snap.skills.iter().filter(|s| s.name == "locked").count(),
        1,
        "~/.agents/skills is not re-read as a project skill dir of the tracked home"
    );
    let count =
        |pred: &dyn Fn(&Scope) -> bool| snap.skills.iter().filter(|s| pred(&s.scope)).count();
    assert_eq!(count(&|s| *s == Scope::User), 3, "one, two, locked");
    assert_eq!(count(&|s| *s == Scope::System), 1);
    assert_eq!(count(&|s| matches!(s, Scope::Plugin { .. })), 1);
    assert_eq!(count(&|s| matches!(s, Scope::Project { .. })), 1);

    let one = skill("one");
    assert_eq!(
        one.id,
        format!("codex:skill:{}", p(&root, ".codex/skills/one/SKILL.md"))
    );
    assert_eq!(one.dir, p(&root, ".codex/skills/one"));
    assert_eq!(one.origin.locator, Locator::MarkdownFile);
    assert_eq!(one.enabled.enabled, Some(false));
    assert_eq!(
        one.enabled.toggle.as_ref().unwrap().locator,
        Locator::TomlArrayItem {
            path: vec!["skills".into(), "config".into()],
            index: 1
        }
    );
    assert_eq!(one.enabled.toggle.as_ref().unwrap().file, config_toml);
    assert_eq!(
        one.enabled.reason.as_deref(),
        Some("disabled via [[skills.config]] path entry")
    );
    assert_eq!(
        one.description.as_deref(),
        Some("First user skill, disabled through a `[[skills.config]]` path entry.")
    );
    assert!(one.frontmatter.present);
    assert_eq!(one.body_preview, "# One Body of skill one.");
    assert!(!one.is_system);
    assert!(!one.lock_managed);

    let two = skill("two");
    assert_eq!(two.enabled.enabled, Some(false));
    assert_eq!(
        two.enabled.toggle.as_ref().unwrap().locator,
        Locator::TomlArrayItem {
            path: vec!["skills".into(), "config".into()],
            index: 0
        }
    );
    assert_eq!(
        two.enabled.reason.as_deref(),
        Some("disabled via [[skills.config]] name entry")
    );

    let sys = skill("sys");
    assert!(sys.is_system);
    assert_eq!(sys.scope, Scope::System);
    assert_eq!(sys.enabled.enabled, Some(true));
    assert_eq!(
        sys.enabled.toggle.as_ref().unwrap().locator,
        Locator::TomlPath {
            path: vec!["skills".into(), "config".into()]
        }
    );

    let locked = skill("locked");
    assert!(locked.lock_managed);
    assert_eq!(locked.scope, Scope::User);
    assert_eq!(locked.enabled.enabled, Some(true));
    assert_eq!(
        locked.enabled.toggle.as_ref().unwrap().locator,
        Locator::TomlArrayItem {
            path: vec!["skills".into(), "config".into()],
            index: 2
        }
    );
    assert_eq!(locked.dir, p(&root, ".agents/skills/locked"));

    let ps = skill("ps");
    assert_eq!(
        ps.scope,
        Scope::Plugin {
            plugin_id: "demo@test-mkt".into()
        }
    );
    assert_eq!(
        ps.description.as_deref(),
        Some("Plugin-provided skill.\n\nUse when: the demo plugin is enabled.")
    );

    let proj_skill = skill("proj");
    assert_eq!(
        proj_skill.scope,
        Scope::Project {
            root: repo_a.clone()
        }
    );
    assert_eq!(
        proj_skill.dir,
        p(&root, "projects/repo a/.agents/skills/proj")
    );

    // ---- Plugins -----------------------------------------------------------------------------
    assert_eq!(snap.plugins.len(), 2);
    let demo = snap
        .plugins
        .iter()
        .find(|p| p.id == "demo@test-mkt")
        .unwrap();
    assert_eq!(demo.name, "demo");
    assert_eq!(demo.marketplace, "test-mkt");
    assert_eq!(demo.enabled.enabled, Some(true));
    assert!(demo.enabled.editable);
    assert!(demo.installed);
    assert!(!demo.unresolved_marketplace);
    assert!(!demo.in_use);
    assert_eq!(demo.version.as_deref(), Some("1.0.0"));
    assert_eq!(
        demo.install_path.as_deref(),
        Some(p(&root, ".codex/plugins/cache/test-mkt/demo/v1").as_str())
    );
    assert_eq!(
        demo.stale_dirs,
        vec![p(&root, ".codex/plugins/cache/test-mkt/demo/v0")]
    );
    assert_eq!(demo.author.as_deref(), Some("Fixture Author"));
    assert_eq!(demo.homepage.as_deref(), Some("https://example.com/demo"));
    assert_eq!(demo.contributions.skills, vec![ps.id.clone()]);
    assert_eq!(
        demo.contributions.mcp_servers,
        vec![demo_api.id.clone(), demo_local.id.clone()]
    );
    assert_eq!(demo.contributions.commands.len(), 1);
    assert!(demo.contributions.agents.is_empty());

    let ghost = snap
        .plugins
        .iter()
        .find(|p| p.id == "ghost@nowhere")
        .unwrap();
    assert!(!ghost.installed);
    assert!(ghost.unresolved_marketplace);
    assert_eq!(ghost.enabled.enabled, Some(false));
    assert_eq!(ghost.version, None);
    assert!(ghost.stale_dirs.is_empty());

    // ---- Commands ----------------------------------------------------------------------------
    assert_eq!(snap.commands.len(), 1);
    let run = &snap.commands[0];
    assert_eq!(run.name, "run");
    assert_eq!(run.id, demo.contributions.commands[0]);
    assert_eq!(
        run.scope,
        Scope::Plugin {
            plugin_id: "demo@test-mkt".into()
        }
    );
    assert_eq!(run.description.as_deref(), Some("Run the demo workflow"));
    assert_eq!(run.argument_hint.as_deref(), Some("[target]"));
    assert_eq!(run.allowed_tools, vec!["Read", "Bash"]);
    assert!(!run.disable_model_invocation);

    // ---- Subagents ---------------------------------------------------------------------------
    assert_eq!(snap.subagents.len(), 1);
    let reviewer = &snap.subagents[0];
    assert_eq!(reviewer.name, "reviewer");
    assert_eq!(
        reviewer.id,
        format!("codex:agent:{}", p(&root, ".codex/agents/reviewer.toml"))
    );
    assert_eq!(reviewer.model.as_deref(), Some("gpt-5.6-terra"));
    assert_eq!(reviewer.effort.as_deref(), Some("high"));
    assert_eq!(
        reviewer.description.as_deref(),
        Some("Review diffs on gpt-5.6-terra with high reasoning.")
    );
    assert!(reviewer
        .body_preview
        .starts_with("Review the diff carefully. Do not delegate."));
    assert!(reviewer.tools.is_empty());
    assert_eq!(reviewer.origin.locator, Locator::TomlPath { path: vec![] });
    assert!(reviewer.frontmatter.present);
    assert_eq!(
        reviewer.frontmatter.fields.get("model_reasoning_effort"),
        Some(&Value::String("high".into()))
    );
    assert!(
        !reviewer.frontmatter.fields.contains_key("skills"),
        "sub-tables are not fields"
    );
    assert_eq!(
        reviewer.frontmatter.raw,
        fs::read_to_string(root.join(".codex/agents/reviewer.toml")).unwrap()
    );

    // ---- Hooks -------------------------------------------------------------------------------
    assert_eq!(snap.hooks.len(), 2);
    let pre = snap.hooks.iter().find(|h| h.event == "PreToolUse").unwrap();
    assert_eq!(pre.id, "codex:hook:user:PreToolUse");
    assert_eq!(pre.entries.len(), 1);
    assert_eq!(pre.entries[0].command, "/hooks/pre-tool-use.sh");
    assert_eq!(pre.entries[0].hook_type, "command");
    assert_eq!(pre.entries[0].timeout, Some(5.0));
    assert_eq!(pre.entries[0].matcher, None);
    assert_eq!(pre.entries[0].trust_entry_present, Some(true));
    assert_eq!(pre.entries[0].origin.file, p(&root, ".codex/hooks.json"));
    assert_eq!(
        pre.entries[0].origin.locator,
        Locator::JsonPointer {
            pointer: "/hooks/PreToolUse/0/hooks/0".into()
        }
    );
    let stop = snap.hooks.iter().find(|h| h.event == "Stop").unwrap();
    assert_eq!(stop.entries[0].trust_entry_present, Some(false));
    assert_eq!(stop.entries[0].matcher.as_deref(), Some("*"));

    // ---- Automations -------------------------------------------------------------------------
    assert_eq!(snap.automations.len(), 3);
    let names: Vec<&str> = snap.automations.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["🚀 One active", "Three projectless", "Two paused"]
    );

    let one_active = &snap.automations[0];
    assert_eq!(
        one_active.id,
        format!(
            "codex:automation:{}",
            p(&root, ".codex/automations/one-active/automation.toml")
        )
    );
    assert_eq!(one_active.dir, p(&root, ".codex/automations/one-active"));
    assert_eq!(one_active.status, "ACTIVE");
    assert_eq!(one_active.kind, "cron");
    assert_eq!(
        one_active.prompt,
        "Line one.\n\nLine `two` with code.\n- bullet"
    );
    assert_eq!(
        one_active.rrule.as_deref(),
        Some("RRULE:FREQ=WEEKLY;BYHOUR=1;BYMINUTE=15;BYDAY=SU,MO,TU,WE,TH,FR,SA")
    );
    assert_eq!(one_active.model.as_deref(), Some("gpt-6-astra"));
    assert_eq!(one_active.reasoning_effort.as_deref(), Some("high"));
    assert_eq!(
        one_active.execution_environment.as_deref(),
        Some("worktree")
    );
    assert_eq!(
        one_active.target,
        AutomationTarget::Project {
            project_id: "local-0059b83a63155ebe17679fb4cf0c3f55".into(),
            local: true
        }
    );
    assert_eq!(one_active.cwds, vec![repo_a.clone()]);
    assert_eq!(one_active.created_at, Some(1778733369975));
    assert_eq!(one_active.updated_at, Some(1789065184555));
    assert_eq!(
        one_active.memory_path.as_deref(),
        Some(p(&root, ".codex/automations/one-active/memory.md").as_str())
    );
    assert_eq!(
        one_active.report_paths,
        vec![
            p(&root, ".codex/automations/one-active/report-2025-12-25.md"),
            p(&root, ".codex/automations/one-active/report-2026-01-01.md"),
        ]
    );
    assert_eq!(one_active.extra.get("version"), Some(&Value::from(1)));
    assert_eq!(
        one_active.extra.get("id"),
        Some(&Value::String("one-active".into()))
    );
    assert_eq!(
        one_active.origin.locator,
        Locator::TomlPath { path: vec![] }
    );

    let projectless = &snap.automations[1];
    assert_eq!(projectless.target, AutomationTarget::Projectless);
    assert_eq!(projectless.status, "PAUSED");
    assert_eq!(projectless.cwds, vec!["~"]);
    assert_eq!(
        projectless.rrule.as_deref(),
        Some("FREQ=WEEKLY;BYDAY=MO;BYHOUR=9;BYMINUTE=0;BYSECOND=0")
    );
    assert_eq!(projectless.memory_path, None);
    assert!(projectless.report_paths.is_empty());

    let paused = &snap.automations[2];
    assert_eq!(
        paused.target,
        AutomationTarget::Project {
            project_id: "2ae4553d-9ebf-4989-aae3-77fdb84b5135".into(),
            local: false
        }
    );
    assert_eq!(paused.status, "PAUSED");

    // ---- Projects ----------------------------------------------------------------------------
    let codex_projects: Vec<&str> = snap
        .projects
        .iter()
        .filter(|p| {
            p.tracked_by
                .contains(&agent_config_editor_lib::model::Agent::Codex)
        })
        .map(|p| p.root.as_str())
        .collect();
    assert_eq!(
        codex_projects,
        vec![
            root.to_string_lossy().as_ref(),
            repo_a.as_str(),
            p(&root, "projects/repo-b").as_str()
        ],
        "sorted by root; the home itself is tracked too and must not double-load config.toml"
    );
    assert!(snap.projects.iter().all(|p| p.exists));
    assert_eq!(
        snap.mcp_servers
            .iter()
            .filter(|s| s.name == "alpha")
            .count(),
        1,
        "user config.toml is not re-read as a project config"
    );

    // ---- File hashes -------------------------------------------------------------------------
    for rel in [
        ".codex/config.toml",
        ".codex/hooks.json",
        ".codex/agents/reviewer.toml",
        ".codex/skills/one/SKILL.md",
        ".codex/skills/.system/sys/SKILL.md",
        ".agents/.skill-lock.json",
        ".agents/skills/locked/SKILL.md",
        ".codex/plugins/cache/test-mkt/demo/v1/.codex-plugin/plugin.json",
        ".codex/plugins/cache/test-mkt/demo/v1/.mcp.json",
        ".codex/plugins/cache/test-mkt/demo/v1/commands/run.md",
        ".codex/automations/one-active/automation.toml",
        "projects/repo a/.codex/config.toml",
        "projects/repo a/.agents/skills/proj/SKILL.md",
    ] {
        assert!(
            snap.file_hashes.contains_key(&p(&root, rel)),
            "missing hash for {rel}"
        );
    }
}

#[test]
fn broken_automation_yields_exactly_one_warning() {
    let (_tmp, root) = make_home();
    let broken_dir = root.join(".codex/automations/broken");
    fs::create_dir_all(&broken_dir).unwrap();
    let broken = broken_dir.join("automation.toml");
    fs::write(&broken, "name = \"unterminated\nstatus = \"ACTIVE\"\n").unwrap();

    let snap = scan(&root);
    let warnings = codex_warnings(&snap);
    assert_eq!(warnings.len(), 1, "warnings: {warnings:?}");
    assert_eq!(
        warnings[0].file.as_deref(),
        Some(broken.to_string_lossy().as_ref())
    );
    assert!(warnings[0].message.starts_with("TOML parse error"));
    assert_eq!(
        snap.automations.len(),
        3,
        "the broken one is skipped, the rest still load"
    );
}

#[test]
fn missing_home_is_silent() {
    let tmp = tempfile::tempdir().unwrap();
    let snap = scan(tmp.path());
    assert_eq!(codex_warnings(&snap), Vec::<&Warning>::new());
    assert!(snap
        .mcp_servers
        .iter()
        .all(|s| s.agent != agent_config_editor_lib::model::Agent::Codex));
    assert!(snap.automations.is_empty());
}

/// Read-only sanity check against this machine. Prints counts only, never values.
/// `cargo test --manifest-path src-tauri/Cargo.toml -- --ignored codex_real_machine --nocapture`
#[test]
#[ignore]
fn codex_real_machine() {
    use agent_config_editor_lib::model::Agent;
    let homes = Homes::detect().expect("homes");
    let snap = scan_all(&homes, &AppSettings::default());

    let config_servers = snap
        .mcp_servers
        .iter()
        .filter(|s| s.agent == Agent::Codex && s.from_plugin.is_none())
        .count();
    let config_disabled = snap
        .mcp_servers
        .iter()
        .filter(|s| {
            s.agent == Agent::Codex && s.from_plugin.is_none() && s.enabled.enabled == Some(false)
        })
        .count();
    let plugin_servers = snap
        .mcp_servers
        .iter()
        .filter(|s| s.agent == Agent::Codex && s.from_plugin.is_some())
        .count();
    let codex_skills: Vec<_> = snap
        .skills
        .iter()
        .filter(|s| s.agent == Agent::Codex)
        .collect();
    let by_scope = |f: &dyn Fn(&Scope) -> bool| codex_skills.iter().filter(|s| f(&s.scope)).count();
    let codex_home_str = homes.codex_home.to_string_lossy().into_owned();
    let agents_dir_str = homes.agents_dir.to_string_lossy().into_owned();
    println!("codex_home: {codex_home_str}");
    let user_servers = snap
        .mcp_servers
        .iter()
        .filter(|s| s.agent == Agent::Codex && s.from_plugin.is_none() && s.scope == Scope::User)
        .count();
    println!(
        "config MCP servers: {config_servers} ({config_disabled} disabled) = user {user_servers} + project {}",
        config_servers - user_servers
    );
    let mut project_files: Vec<&str> = snap
        .mcp_servers
        .iter()
        .filter(|s| s.agent == Agent::Codex && matches!(s.scope, Scope::Project { .. }))
        .map(|s| s.origin.file.as_str())
        .collect();
    project_files.sort();
    project_files.dedup();
    for f in project_files {
        println!("  project mcp config: {f}");
    }
    println!("plugin MCP servers: {plugin_servers}");
    println!(
        "skills: total {} | codex_home user {} | system {} | ~/.agents {} (lock-managed {}) | plugin {} | project {} | disabled {}",
        codex_skills.len(),
        codex_skills.iter().filter(|s| s.scope == Scope::User && s.dir.starts_with(&codex_home_str)).count(),
        by_scope(&|s| *s == Scope::System),
        codex_skills.iter().filter(|s| s.scope == Scope::User && s.dir.starts_with(&agents_dir_str)).count(),
        codex_skills.iter().filter(|s| s.lock_managed).count(),
        by_scope(&|s| matches!(s, Scope::Plugin { .. })),
        by_scope(&|s| matches!(s, Scope::Project { .. })),
        codex_skills.iter().filter(|s| s.enabled.enabled == Some(false)).count(),
    );
    let plugins: Vec<_> = snap
        .plugins
        .iter()
        .filter(|p| p.agent == Agent::Codex)
        .collect();
    println!(
        "plugins: {} (installed {}, unresolved marketplace {}, with stale dirs {})",
        plugins.len(),
        plugins.iter().filter(|p| p.installed).count(),
        plugins.iter().filter(|p| p.unresolved_marketplace).count(),
        plugins.iter().filter(|p| !p.stale_dirs.is_empty()).count(),
    );
    println!(
        "commands: {} | subagents: {} | hook events: {} (entries with trust {} / without {})",
        snap.commands
            .iter()
            .filter(|c| c.agent == Agent::Codex)
            .count(),
        snap.subagents
            .iter()
            .filter(|a| a.agent == Agent::Codex)
            .count(),
        snap.hooks
            .iter()
            .filter(|h| h.agent == Agent::Codex)
            .count(),
        snap.hooks
            .iter()
            .filter(|h| h.agent == Agent::Codex)
            .flat_map(|h| &h.entries)
            .filter(|e| e.trust_entry_present == Some(true))
            .count(),
        snap.hooks
            .iter()
            .filter(|h| h.agent == Agent::Codex)
            .flat_map(|h| &h.entries)
            .filter(|e| e.trust_entry_present == Some(false))
            .count(),
    );
    println!(
        "automations: {} (active {}, paused {}, local-project {}, projectless {}) | with memory.md {} | report files {}",
        snap.automations.len(),
        snap.automations.iter().filter(|a| a.status == "ACTIVE").count(),
        snap.automations.iter().filter(|a| a.status == "PAUSED").count(),
        snap.automations.iter().filter(|a| matches!(a.target, AutomationTarget::Project { local: true, .. })).count(),
        snap.automations.iter().filter(|a| a.target == AutomationTarget::Projectless).count(),
        snap.automations.iter().filter(|a| a.memory_path.is_some()).count(),
        snap.automations.iter().map(|a| a.report_paths.len()).sum::<usize>(),
    );
    println!(
        "projects tracked by codex: {} (missing {})",
        snap.projects
            .iter()
            .filter(|p| p.tracked_by.contains(&Agent::Codex))
            .count(),
        snap.projects
            .iter()
            .filter(|p| p.tracked_by.contains(&Agent::Codex) && !p.exists)
            .count(),
    );
    println!(
        "dup groups: {} | file hashes: {} | scan ms: {}",
        snap.dup_groups.len(),
        snap.file_hashes.len(),
        snap.scan_millis
    );
    println!("warnings: {}", snap.warnings.len());
    for w in &snap.warnings {
        let msg: String = w.message.chars().take(100).collect();
        println!("  - {}: {msg}", w.file.as_deref().unwrap_or("-"));
    }

    // Round-trip the real config.toml through toml_edit without touching it.
    let config = homes.codex_home.join("config.toml");
    if let Ok(text) = fs::read_to_string(&config) {
        let doc: toml_edit::DocumentMut = text.parse().expect("real config.toml parses");
        println!(
            "config.toml round-trip: {}",
            if doc.to_string() == text {
                "byte-identical"
            } else {
                "DIFFERENT"
            }
        );
    }
}
