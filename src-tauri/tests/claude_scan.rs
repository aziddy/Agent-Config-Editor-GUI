//! Integration test for the Claude Code readers against `tests/fixtures/claude_home`.
//!
//! The fixture is a miniature `$HOME`. Files ending in `.tpl` contain `__ROOT__` placeholders
//! (Claude Code stores absolute paths in `~/.claude.json` and the plugin registry), so the tree
//! is copied into a temp dir and the placeholders are substituted before scanning.

use agent_config_editor_lib::fsutil::Homes;
use agent_config_editor_lib::model::{
    Agent, AppSettings, EntityKind, Locator, McpServer, McpTransport, Plugin, Scope, Skill,
    Snapshot,
};
use agent_config_editor_lib::scan::scan_all;
use pretty_assertions::assert_eq;
use std::fs;
use std::path::{Path, PathBuf};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude_home")
}

fn copy_tree(src: &Path, dst: &Path, root: &str) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let from = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if from.is_dir() {
            copy_tree(&from, &dst.join(&name), root);
        } else if let Some(stem) = name.strip_suffix(".tpl") {
            let text = fs::read_to_string(&from).unwrap().replace("__ROOT__", root);
            fs::write(dst.join(stem), text).unwrap();
        } else {
            fs::copy(&from, dst.join(&name)).unwrap();
        }
    }
}

struct Fixture {
    _tmp: tempfile::TempDir,
    home: PathBuf,
}

impl Fixture {
    fn build() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_path_buf();
        copy_tree(&fixture_root(), &home, &home.to_string_lossy());
        Fixture { _tmp: tmp, home }
    }

    fn scan(&self) -> Snapshot {
        scan_all(&Homes::for_test(&self.home), &AppSettings::default())
    }

    fn path(&self, rel: &str) -> String {
        self.home.join(rel).to_string_lossy().into_owned()
    }

    fn alpha(&self) -> String {
        self.path("projects/alpha-repo")
    }

    fn beta(&self) -> String {
        self.path("projects/beta-repo")
    }

    fn project(&self, root: &str) -> Scope {
        Scope::Project {
            root: self.path(root),
        }
    }
}

fn pointer(p: &str) -> Locator {
    Locator::JsonPointer {
        pointer: p.to_string(),
    }
}

fn escape(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}

fn skill<'a>(snap: &'a Snapshot, name: &str, scope: &Scope) -> &'a Skill {
    snap.skills
        .iter()
        .find(|s| s.name == name && &s.scope == scope)
        .unwrap_or_else(|| panic!("skill {name} at {scope:?} not found"))
}

fn server<'a>(snap: &'a Snapshot, name: &str) -> &'a McpServer {
    let matches: Vec<_> = snap.mcp_servers.iter().filter(|s| s.name == name).collect();
    assert_eq!(matches.len(), 1, "expected exactly one server named {name}");
    matches[0]
}

fn plugin<'a>(snap: &'a Snapshot, id: &str) -> &'a Plugin {
    let matches: Vec<_> = snap.plugins.iter().filter(|p| p.id == id).collect();
    assert_eq!(matches.len(), 1, "expected exactly one plugin {id}");
    matches[0]
}

const DEMO: &str = "demo@test-mkt";

#[test]
fn clean_fixture_scans_without_warnings() {
    let fx = Fixture::build();
    let snap = fx.scan();
    assert!(
        snap.warnings.is_empty(),
        "unexpected warnings: {:#?}",
        snap.warnings
    );
    assert!(snap.homes.as_ref().unwrap().sandbox);
}

#[test]
fn skills_by_scope_with_frontmatter_shapes() {
    let fx = Fixture::build();
    let snap = fx.scan();
    assert_eq!(
        snap.skills.len(),
        5,
        "{:#?}",
        snap.skills.iter().map(|s| &s.id).collect::<Vec<_>>()
    );

    let plugin_scope = Scope::Plugin {
        plugin_id: DEMO.into(),
    };
    let count = |scope: &Scope| snap.skills.iter().filter(|s| &s.scope == scope).count();
    assert_eq!(count(&Scope::User), 2);
    assert_eq!(count(&plugin_scope), 1);
    assert_eq!(count(&fx.project("projects/alpha-repo")), 1);
    assert_eq!(count(&fx.project("projects/beta-repo")), 1);
    for s in &snap.skills {
        assert_eq!(s.agent, Agent::Claude);
        assert_eq!(s.content_hash.len(), 64);
        assert!(!s.is_system && !s.lock_managed);
        assert_eq!(
            s.enabled.enabled, None,
            "Claude skills have no enabled state"
        );
        assert!(!s.enabled.editable);
        assert!(s
            .enabled
            .reason
            .as_deref()
            .unwrap()
            .contains("disable-model-invocation"));
        assert_eq!(s.origin.locator, Locator::MarkdownFile);
    }

    // Folded `>` description + string allowed-tools.
    let alpha = skill(&snap, "alpha", &Scope::User);
    assert_eq!(
        alpha.id,
        format!("claude:skill:{}", fx.path(".claude/skills/alpha/SKILL.md"))
    );
    assert_eq!(alpha.dir, fx.path(".claude/skills/alpha"));
    assert_eq!(alpha.origin.file, fx.path(".claude/skills/alpha/SKILL.md"));
    assert_eq!(
        alpha.description.as_deref(),
        Some("Alpha skill with a folded description.")
    );
    assert!(alpha.frontmatter.present);
    assert_eq!(
        alpha.frontmatter.fields.get("allowed-tools").unwrap(),
        "Bash(git:*), Read",
        "original shape is preserved in the frontmatter fields"
    );
    assert_eq!(alpha.body_preview, "# Alpha Do alpha things.");

    // Array allowed-tools + nested metadata.
    let beta = skill(&snap, "beta", &Scope::User);
    assert_eq!(beta.description.as_deref(), Some("Beta skill"));
    assert_eq!(
        beta.frontmatter.fields.get("allowed-tools").unwrap(),
        &serde_json::json!(["Bash", "Read"])
    );
    assert_eq!(
        beta.frontmatter
            .fields
            .get("disable-model-invocation")
            .unwrap(),
        true
    );
    assert_eq!(beta.frontmatter.fields["metadata"]["version"], "2");

    let plug = skill(&snap, "plug", &plugin_scope);
    assert!(plug
        .id
        .ends_with("/cache/test-mkt/demo/abc123/skills/plug/SKILL.md"));
}

#[test]
fn byte_identical_project_skills_form_one_dup_group() {
    let fx = Fixture::build();
    let snap = fx.scan();
    assert_eq!(snap.dup_groups.len(), 1, "{:#?}", snap.dup_groups);
    let g = &snap.dup_groups[0];
    assert_eq!(g.kind, EntityKind::Skill);
    assert_eq!(g.name, "shared");
    assert_eq!(g.member_ids.len(), 2);
    assert_eq!(g.project_roots, vec![fx.alpha(), fx.beta()]);

    let a = skill(&snap, "shared", &fx.project("projects/alpha-repo"));
    let b = skill(&snap, "shared", &fx.project("projects/beta-repo"));
    assert_eq!(a.content_hash, b.content_hash);
    assert_eq!(a.dup_group.as_deref(), Some(g.key.as_str()));
    assert_eq!(b.dup_group.as_deref(), Some(g.key.as_str()));
    assert!(g.member_ids.contains(&a.id) && g.member_ids.contains(&b.id));
    assert_eq!(skill(&snap, "alpha", &Scope::User).dup_group, None);
}

#[test]
fn mcp_user_scope_server_reports_projects_that_disable_it() {
    let fx = Fixture::build();
    let snap = fx.scan();
    assert_eq!(
        snap.mcp_servers.len(),
        6,
        "{:#?}",
        snap.mcp_servers.iter().map(|s| &s.id).collect::<Vec<_>>()
    );

    let workos = server(&snap, "workos");
    assert_eq!(workos.id, "claude:mcp:user:workos");
    assert_eq!(workos.scope, Scope::User);
    assert_eq!(workos.origin.file, fx.path(".claude.json"));
    assert_eq!(workos.origin.locator, pointer("/mcpServers/workos"));
    assert_eq!(workos.enabled.enabled, Some(true));
    assert!(workos.enabled.editable);
    let toggle = workos.enabled.toggle.as_ref().unwrap();
    assert_eq!(toggle.file, fx.path(".claude.json"));
    assert_eq!(toggle.locator, pointer("/projects"));
    assert_eq!(
        workos.enabled.reason.as_deref(),
        Some(format!("Disabled in projects: {}", fx.alpha()).as_str())
    );
    assert_eq!(
        workos.extra["disabledInProjects"],
        serde_json::json!([fx.alpha()])
    );
    assert!(workos.needs_auth, "flagged in mcp-needs-auth-cache.json");
    assert!(workos.from_plugin.is_none());
    assert!(
        matches!(&workos.transport, McpTransport::Http { url, .. } if url == "https://mcp.workos.com/mcp")
    );
}

#[test]
fn mcp_local_scope_server_is_fixed_on() {
    let fx = Fixture::build();
    let snap = fx.scan();
    let local = server(&snap, "local-echo");
    assert_eq!(local.scope, fx.project("projects/alpha-repo"));
    assert_eq!(local.origin.file, fx.path(".claude.json"));
    assert_eq!(
        local.origin.locator,
        pointer(&format!(
            "/projects/{}/mcpServers/local-echo",
            escape(&fx.alpha())
        ))
    );
    assert_eq!(local.enabled.enabled, Some(true));
    assert!(!local.enabled.editable);
    assert!(local.enabled.toggle.is_none());
    assert!(local
        .enabled
        .reason
        .as_deref()
        .unwrap()
        .contains("delete to remove"));
    assert!(!local.needs_auth);
    match &local.transport {
        McpTransport::Stdio {
            command,
            args,
            env,
            cwd,
        } => {
            assert_eq!(command, "echo");
            assert_eq!(args, &vec!["hi".to_string()]);
            assert!(env.is_empty());
            assert_eq!(cwd, &None);
        }
        other => panic!("expected stdio, got {other:?}"),
    }
}

#[test]
fn mcp_project_mcp_json_has_three_approval_states() {
    let fx = Fixture::build();
    let snap = fx.scan();
    let mcp_json = fx.path("projects/alpha-repo/.mcp.json");
    let toggle_pointer = pointer(&format!(
        "/projects/{}/enabledMcpjsonServers",
        escape(&fx.alpha())
    ));

    let approved = server(&snap, "approved-srv");
    assert_eq!(approved.scope, fx.project("projects/alpha-repo"));
    assert_eq!(approved.origin.file, mcp_json);
    assert_eq!(
        approved.origin.locator,
        pointer("/mcpServers/approved-srv"),
        "wrapped shape"
    );
    assert_eq!(approved.enabled.enabled, Some(true));
    assert!(approved.enabled.editable);
    assert_eq!(approved.enabled.reason, None);
    let toggle = approved.enabled.toggle.as_ref().unwrap();
    assert_eq!(toggle.file, fx.path(".claude.json"));
    assert_eq!(toggle.locator, toggle_pointer);
    assert_eq!(
        approved.extra["tool_timeout_sec"], 120,
        "unknown keys pass through in extra"
    );
    assert_eq!(approved.extra["bearer_token_env_var"], "APPROVED_TOKEN");
    assert_eq!(approved.tool_timeout_sec, Some(120.0));
    assert!(
        matches!(&approved.transport, McpTransport::Http { bearer_token_env_var: Some(v), .. } if v == "APPROVED_TOKEN")
    );
    assert!(!approved.needs_auth);

    let blocked = server(&snap, "blocked-srv");
    assert_eq!(blocked.enabled.enabled, Some(false));
    assert!(blocked.enabled.editable);
    assert_eq!(
        blocked.enabled.reason.as_deref(),
        Some("disabled for this project")
    );
    assert_eq!(
        blocked.enabled.toggle.as_ref().unwrap().locator,
        toggle_pointer
    );

    let pending = server(&snap, "pending-srv");
    assert_eq!(pending.enabled.enabled, Some(false));
    assert!(pending.enabled.editable);
    assert_eq!(
        pending.enabled.reason.as_deref(),
        Some("not approved yet; Claude Code will prompt")
    );
    match &pending.transport {
        McpTransport::Stdio { env, .. } => {
            assert_eq!(
                env["API_TOKEN"], "REDACTED_TOKEN",
                "env is unmasked in the model"
            );
        }
        other => panic!("expected stdio, got {other:?}"),
    }
}

#[test]
fn mcp_plugin_server_from_wrapperless_file() {
    let fx = Fixture::build();
    let snap = fx.scan();
    let demo = server(&snap, "demo-mcp");
    assert_eq!(demo.id, format!("claude:mcp:plugin:{DEMO}:demo-mcp"));
    assert_eq!(
        demo.scope,
        Scope::Plugin {
            plugin_id: DEMO.into()
        }
    );
    assert_eq!(demo.from_plugin.as_deref(), Some(DEMO));
    assert_eq!(
        demo.origin.file,
        fx.path(".claude/plugins/cache/test-mkt/demo/abc123/.mcp.json")
    );
    assert_eq!(
        demo.origin.locator,
        pointer("/demo-mcp"),
        "wrapper-less shape"
    );
    assert_eq!(
        demo.enabled.enabled,
        Some(false),
        "disabled via plugin:demo:demo-mcp in alpha-repo"
    );
    assert!(demo.enabled.editable);
    assert_eq!(
        demo.enabled.reason.as_deref(),
        Some(format!("Disabled in projects: {}", fx.alpha()).as_str())
    );
    let toggle = demo.enabled.toggle.as_ref().unwrap();
    assert_eq!(toggle.file, fx.path(".claude/settings.json"));
    assert_eq!(toggle.locator, pointer("/enabledPlugins/demo@test-mkt"));
    assert!(demo.needs_auth);
    match &demo.transport {
        McpTransport::Http { url, headers, .. } => {
            assert_eq!(url, "https://mcp.example.com/mcp");
            assert_eq!(
                headers["Authorization"],
                "Bearer ${GITHUB_PERSONAL_ACCESS_TOKEN}"
            );
        }
        other => panic!("expected http, got {other:?}"),
    }
}

#[test]
fn plugins_registry_array_shape_marketplaces_and_cache_state() {
    let fx = Fixture::build();
    let snap = fx.scan();
    assert_eq!(
        snap.plugins.len(),
        3,
        "{:#?}",
        snap.plugins.iter().map(|p| &p.id).collect::<Vec<_>>()
    );

    let demo = plugin(&snap, DEMO);
    assert_eq!(demo.name, "demo");
    assert_eq!(demo.marketplace, "test-mkt");
    assert_eq!(demo.agent, Agent::Claude);
    assert_eq!(demo.scope, Scope::User);
    assert!(demo.installed);
    assert_eq!(demo.enabled.enabled, Some(true));
    assert!(demo.enabled.editable);
    assert_eq!(demo.enabled.reason, None);
    let toggle = demo.enabled.toggle.as_ref().unwrap();
    assert_eq!(toggle.file, fx.path(".claude/settings.json"));
    assert_eq!(toggle.locator, pointer("/enabledPlugins/demo@test-mkt"));
    assert_eq!(toggle.scope, Scope::User);
    assert_eq!(
        demo.version.as_deref(),
        Some("1.2.3"),
        "plugin.json version beats the content hash"
    );
    assert_eq!(
        demo.install_path.as_deref(),
        Some(
            fx.path(".claude/plugins/cache/test-mkt/demo/abc123")
                .as_str()
        )
    );
    assert_eq!(
        demo.description.as_deref(),
        Some("Demo plugin from the catalog"),
        "from marketplace.json"
    );
    assert_eq!(demo.author.as_deref(), Some("Demo Author"));
    assert_eq!(demo.homepage.as_deref(), Some("https://example.com/demo"));
    assert!(!demo.unresolved_marketplace);
    assert!(demo.in_use, ".in_use/12345 marker");
    assert_eq!(
        demo.stale_dirs,
        vec![fx.path(".claude/plugins/cache/test-mkt/demo/old999")]
    );
    let plug_skill = snap.skills.iter().find(|s| s.name == "plug").unwrap();
    let go = snap.commands.iter().find(|c| c.name == "go").unwrap();
    let helper = snap.subagents.iter().find(|a| a.name == "helper").unwrap();
    assert_eq!(demo.contributions.skills, vec![plug_skill.id.clone()]);
    assert_eq!(demo.contributions.commands, vec![go.id.clone()]);
    assert_eq!(demo.contributions.agents, vec![helper.id.clone()]);
    assert_eq!(
        demo.contributions.mcp_servers,
        vec![server(&snap, "demo-mcp").id.clone()]
    );

    let ghost = plugin(&snap, "ghost@unknown-mkt");
    assert!(!ghost.installed);
    assert!(ghost.unresolved_marketplace);
    assert_eq!(ghost.scope, Scope::User);
    assert_eq!(ghost.enabled.enabled, Some(true));
    let reason = ghost.enabled.reason.as_deref().unwrap();
    assert!(
        reason.contains("not installed") && reason.contains("unknown-mkt"),
        "{reason}"
    );
    assert!(ghost.contributions.skills.is_empty());
    assert_eq!(ghost.install_path, None);

    let proj = plugin(&snap, "proj-plug@test-mkt");
    assert!(!proj.installed);
    assert!(!proj.unresolved_marketplace);
    assert_eq!(proj.scope, fx.project("projects/alpha-repo"));
    assert_eq!(
        proj.description.as_deref(),
        Some("Project-only plugin that is not installed")
    );
    let toggle = proj.enabled.toggle.as_ref().unwrap();
    assert_eq!(
        toggle.file,
        fx.path("projects/alpha-repo/.claude/settings.json")
    );
    assert_eq!(
        toggle.locator,
        pointer("/enabledPlugins/proj-plug@test-mkt")
    );
}

#[test]
fn hooks_are_flattened_with_json_pointers() {
    let fx = Fixture::build();
    let snap = fx.scan();
    assert_eq!(
        snap.hooks.len(),
        3,
        "{:#?}",
        snap.hooks.iter().map(|h| &h.id).collect::<Vec<_>>()
    );
    let user_settings = fx.path(".claude/settings.json");

    let pre = snap.hooks.iter().find(|h| h.event == "PreToolUse").unwrap();
    assert_eq!(pre.id, "claude:hook:user:PreToolUse");
    assert_eq!(pre.scope, Scope::User);
    assert_eq!(pre.agent, Agent::Claude);
    assert_eq!(pre.entries.len(), 2);
    assert_eq!(pre.entries[0].matcher.as_deref(), Some("Bash"));
    assert_eq!(pre.entries[0].hook_type, "command");
    assert_eq!(pre.entries[0].command, "echo pre-bash");
    assert_eq!(pre.entries[0].timeout, Some(30.0));
    assert_eq!(pre.entries[0].origin.file, user_settings);
    assert_eq!(
        pre.entries[0].origin.locator,
        pointer("/hooks/PreToolUse/0/hooks/0")
    );
    assert_eq!(pre.entries[0].trust_entry_present, None);
    assert_eq!(pre.entries[1].matcher.as_deref(), Some(""));
    assert_eq!(pre.entries[1].command, "echo pre-any");
    assert_eq!(pre.entries[1].timeout, None);
    assert_eq!(
        pre.entries[1].origin.locator,
        pointer("/hooks/PreToolUse/1/hooks/0")
    );

    let start = snap
        .hooks
        .iter()
        .find(|h| h.event == "SessionStart")
        .unwrap();
    assert_eq!(start.entries.len(), 2);
    assert_eq!(start.entries[0].matcher, None);
    assert_eq!(
        start.entries[0].origin.locator,
        pointer("/hooks/SessionStart/0/hooks/0")
    );
    assert_eq!(
        start.entries[1].origin.locator,
        pointer("/hooks/SessionStart/0/hooks/1")
    );
    assert_eq!(start.entries[1].timeout, Some(5.0));

    let stop = snap.hooks.iter().find(|h| h.event == "Stop").unwrap();
    assert_eq!(stop.id, format!("claude:hook:project:{}:Stop", fx.alpha()));
    assert_eq!(stop.scope, fx.project("projects/alpha-repo"));
    assert_eq!(stop.entries.len(), 1);
    assert_eq!(
        stop.entries[0].origin.file,
        fx.path("projects/alpha-repo/.claude/settings.json")
    );
    assert_eq!(
        stop.entries[0].origin.locator,
        pointer("/hooks/Stop/0/hooks/0")
    );
}

#[test]
fn commands_including_frontmatter_less_and_namespaced() {
    let fx = Fixture::build();
    let snap = fx.scan();
    let mut names: Vec<&str> = snap.commands.iter().map(|c| c.name.as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["go", "ns:sub", "top"]);

    let go = snap.commands.iter().find(|c| c.name == "go").unwrap();
    assert!(!go.frontmatter.present, "frontmatter-less command");
    assert!(go.frontmatter.fields.is_empty());
    assert_eq!(go.frontmatter.raw, "");
    assert_eq!(go.description, None);
    assert!(go.allowed_tools.is_empty());
    assert!(!go.disable_model_invocation);
    assert_eq!(go.body_preview, "# Go Run the thing with $ARGUMENTS.");
    assert_eq!(
        go.scope,
        Scope::Plugin {
            plugin_id: DEMO.into()
        }
    );
    assert_eq!(
        go.id,
        format!(
            "claude:command:{}",
            fx.path(".claude/plugins/cache/test-mkt/demo/abc123/commands/go.md")
        )
    );

    let top = snap.commands.iter().find(|c| c.name == "top").unwrap();
    assert_eq!(top.scope, fx.project("projects/alpha-repo"));
    assert_eq!(top.description.as_deref(), Some("Top-level command"));
    assert_eq!(top.argument_hint.as_deref(), Some("[issue]"));
    assert_eq!(top.allowed_tools, vec!["Bash(gh:*)".to_string()]);
    assert!(top.disable_model_invocation);
    assert!(top.frontmatter.present);

    let sub = snap.commands.iter().find(|c| c.name == "ns:sub").unwrap();
    assert!(sub.id.ends_with("/.claude/commands/ns/sub.md"));
    assert_eq!(sub.description.as_deref(), Some("Namespaced command"));
}

#[test]
fn subagents_keep_literal_newlines_and_example_blocks() {
    let fx = Fixture::build();
    let snap = fx.scan();
    assert_eq!(snap.subagents.len(), 3);

    // Unquoted description with `: ` inside: strict YAML fails, Claude Code accepts it, and so do we.
    let sloppy = snap.subagents.iter().find(|a| a.name == "sloppy").unwrap();
    assert!(sloppy.frontmatter.present);
    assert_eq!(
        sloppy.description.as_deref(),
        Some("Use this agent for sloppy things. Examples:\\n\\n<example>\\nContext: The user wants help\\nuser: \"help me\"\\nassistant: \"I'll use sloppy.\"\\n</example>"),
        "plain scalar is kept verbatim, literal backslash-n included"
    );
    assert_eq!(sloppy.model.as_deref(), Some("haiku"));
    assert_eq!(sloppy.color.as_deref(), Some("blue"));
    assert_eq!(sloppy.scope, fx.project("projects/alpha-repo"));
    assert!(
        snap.warnings.is_empty(),
        "lenient recovery is silent: {:#?}",
        snap.warnings
    );

    let helper = snap.subagents.iter().find(|a| a.name == "helper").unwrap();
    assert_eq!(
        helper.description.as_deref(),
        Some("Use this agent when helping.\n\n<example>\nContext: needs help\nuser: \"help\"\nassistant: \"I'll use helper.\"\n</example>")
    );
    assert_eq!(helper.model.as_deref(), Some("sonnet"));
    assert_eq!(helper.color.as_deref(), Some("green"));
    assert_eq!(
        helper.tools,
        vec!["Read".to_string(), "Grep".to_string()],
        "string form"
    );
    assert_eq!(helper.effort, None);
    assert_eq!(
        helper.scope,
        Scope::Plugin {
            plugin_id: DEMO.into()
        }
    );
    assert_eq!(helper.body_preview, "You are a helper.");
    assert_eq!(helper.content_hash.len(), 64);

    let reviewer = snap
        .subagents
        .iter()
        .find(|a| a.name == "reviewer")
        .unwrap();
    assert_eq!(reviewer.scope, fx.project("projects/alpha-repo"));
    assert_eq!(
        reviewer.tools,
        vec!["Read".to_string(), "Grep".to_string()],
        "array form"
    );
    assert_eq!(reviewer.effort.as_deref(), Some("high"));
    assert_eq!(
        reviewer.id,
        format!(
            "claude:agent:{}",
            fx.path("projects/alpha-repo/.claude/agents/reviewer.md")
        )
    );
}

#[test]
fn projects_are_tracked_and_files_hashed() {
    let fx = Fixture::build();
    let snap = fx.scan();
    let roots: Vec<&str> = snap.projects.iter().map(|p| p.root.as_str()).collect();
    assert_eq!(
        roots,
        vec![fx.alpha(), fx.beta(), fx.path("projects/missing-repo")]
    );
    for p in &snap.projects {
        assert!(p.tracked_by.contains(&Agent::Claude));
    }
    assert!(snap.projects[0].exists && snap.projects[1].exists);
    assert!(
        !snap.projects[2].exists,
        "missing roots are tracked but not scanned"
    );

    for rel in [
        ".claude.json",
        ".claude/settings.json",
        ".claude/mcp-needs-auth-cache.json",
        ".claude/plugins/installed_plugins.json",
        ".claude/plugins/known_marketplaces.json",
        ".claude/plugins/marketplaces/test-mkt/.claude-plugin/marketplace.json",
        ".claude/plugins/cache/test-mkt/demo/abc123/.claude-plugin/plugin.json",
        ".claude/plugins/cache/test-mkt/demo/abc123/.mcp.json",
        ".claude/skills/alpha/SKILL.md",
        "projects/alpha-repo/.mcp.json",
        "projects/alpha-repo/.claude/settings.json",
        "projects/alpha-repo/.claude/commands/ns/sub.md",
    ] {
        let path = fx.path(rel);
        let hash = snap
            .file_hashes
            .get(&path)
            .unwrap_or_else(|| panic!("no hash for {path}"));
        assert_eq!(hash.as_str().unwrap().len(), 64);
    }
    assert!(
        snap.file_hashes
            .get(&fx.path(".claude/plugins/cache/test-mkt/demo/old999/.claude-plugin/plugin.json"))
            .is_none(),
        "stale plugin dirs are not read"
    );
}

#[test]
fn home_without_claude_files_is_silent() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".codex")).unwrap();
    let snap = scan_all(&Homes::for_test(tmp.path()), &AppSettings::default());
    let claude = |agent: Agent| agent == Agent::Claude;
    assert!(snap.skills.iter().all(|s| !claude(s.agent)));
    assert!(snap.mcp_servers.iter().all(|s| !claude(s.agent)));
    assert!(snap.plugins.iter().all(|p| !claude(p.agent)));
    assert!(snap.subagents.iter().all(|a| !claude(a.agent)));
    assert!(snap.commands.iter().all(|c| !claude(c.agent)));
    assert!(snap.hooks.iter().all(|h| !claude(h.agent)));
    assert!(snap
        .projects
        .iter()
        .all(|p| !p.tracked_by.contains(&Agent::Claude)));
    let claude_warnings: Vec<_> = snap
        .warnings
        .iter()
        .filter(|w| w.file.as_deref().is_some_and(|f| f.contains("/.claude")))
        .collect();
    assert!(claude_warnings.is_empty(), "{claude_warnings:#?}");
}

#[test]
fn broken_json_yields_exactly_one_warning_and_scan_continues() {
    let fx = Fixture::build();
    let broken = fx
        .home
        .join("projects/beta-repo/.claude/settings.local.json");
    fs::write(&broken, "{ \"broken\": ").unwrap();
    let snap = fx.scan();
    assert_eq!(snap.warnings.len(), 1, "{:#?}", snap.warnings);
    let w = &snap.warnings[0];
    assert_eq!(w.file.as_deref(), Some(broken.to_string_lossy().as_ref()));
    assert!(w.message.contains("invalid JSON"), "{}", w.message);
    assert_eq!(snap.skills.len(), 5, "the rest of the scan still completes");
    assert_eq!(snap.mcp_servers.len(), 6);
}

/// Read-only sanity check against this machine's real config. Prints counts only, never values.
/// Run with: `cargo test --manifest-path src-tauri/Cargo.toml -- --ignored claude_real_machine --nocapture`
#[test]
#[ignore]
fn claude_real_machine() {
    let homes = Homes::detect().expect("home directory");
    let snap = scan_all(&homes, &AppSettings::default());
    let by_scope = |scopes: Vec<&Scope>| {
        let mut user = 0;
        let mut project = 0;
        let mut plugin = 0;
        let mut other = 0;
        for s in scopes {
            match s {
                Scope::User => user += 1,
                Scope::Project { .. } => project += 1,
                Scope::Plugin { .. } => plugin += 1,
                _ => other += 1,
            }
        }
        format!("user={user} project={project} plugin={plugin} other={other}")
    };
    let claude_skills: Vec<_> = snap
        .skills
        .iter()
        .filter(|s| s.agent == Agent::Claude)
        .collect();
    let claude_mcp: Vec<_> = snap
        .mcp_servers
        .iter()
        .filter(|s| s.agent == Agent::Claude)
        .collect();
    let claude_plugins: Vec<_> = snap
        .plugins
        .iter()
        .filter(|p| p.agent == Agent::Claude)
        .collect();
    let claude_agents: Vec<_> = snap
        .subagents
        .iter()
        .filter(|a| a.agent == Agent::Claude)
        .collect();
    let claude_commands: Vec<_> = snap
        .commands
        .iter()
        .filter(|c| c.agent == Agent::Claude)
        .collect();
    let claude_hooks: Vec<_> = snap
        .hooks
        .iter()
        .filter(|h| h.agent == Agent::Claude)
        .collect();

    println!("scan took {} ms", snap.scan_millis);
    println!(
        "skills: {} ({})",
        claude_skills.len(),
        by_scope(claude_skills.iter().map(|s| &s.scope).collect())
    );
    println!(
        "mcp servers: {} ({}); needs_auth={}; from_plugin={}",
        claude_mcp.len(),
        by_scope(claude_mcp.iter().map(|s| &s.scope).collect()),
        claude_mcp.iter().filter(|s| s.needs_auth).count(),
        claude_mcp
            .iter()
            .filter(|s| s.from_plugin.is_some())
            .count()
    );
    for s in &claude_mcp {
        println!(
            "  mcp {} scope={} enabled={:?} editable={} reason={:?}",
            s.name,
            s.scope.key(),
            s.enabled.enabled,
            s.enabled.editable,
            s.enabled.reason
        );
    }
    println!(
        "plugins: {} (installed={}, unresolved={}, in_use={})",
        claude_plugins.len(),
        claude_plugins.iter().filter(|p| p.installed).count(),
        claude_plugins
            .iter()
            .filter(|p| p.unresolved_marketplace)
            .count(),
        claude_plugins.iter().filter(|p| p.in_use).count()
    );
    for p in &claude_plugins {
        println!(
            "  plugin {} scope={} enabled={:?} stale_dirs={} contributions: skills={} agents={} commands={} mcp={}",
            p.id,
            p.scope.key(),
            p.enabled.enabled,
            p.stale_dirs.len(),
            p.contributions.skills.len(),
            p.contributions.agents.len(),
            p.contributions.commands.len(),
            p.contributions.mcp_servers.len()
        );
    }
    println!(
        "subagents: {} ({})",
        claude_agents.len(),
        by_scope(claude_agents.iter().map(|a| &a.scope).collect())
    );
    println!(
        "commands: {} ({})",
        claude_commands.len(),
        by_scope(claude_commands.iter().map(|c| &c.scope).collect())
    );
    println!(
        "hook events: {} (entries={})",
        claude_hooks.len(),
        claude_hooks.iter().map(|h| h.entries.len()).sum::<usize>()
    );
    println!("dup groups: {}", snap.dup_groups.len());
    for g in &snap.dup_groups {
        println!(
            "  {} {} members={} roots={}",
            g.kind.key(),
            g.name,
            g.member_ids.len(),
            g.project_roots.len()
        );
    }
    println!(
        "projects: {} (existing={})",
        snap.projects.len(),
        snap.projects.iter().filter(|p| p.exists).count()
    );
    println!("file hashes: {}", snap.file_hashes.len());
    println!("warnings: {}", snap.warnings.len());
    for w in &snap.warnings {
        println!("  {:?}: {}", w.file, w.message);
    }
}
