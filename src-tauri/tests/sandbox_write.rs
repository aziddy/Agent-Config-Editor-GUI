//! End-to-end write checks against a sandbox home (`ACE_HOME`). Ignored by default:
//! `ACE_HOME=$PWD/../.sandbox-home cargo test --test sandbox_write -- --ignored --nocapture`.
//! Refuses to run unless the resolved home is a sandbox.

use agent_config_editor_lib::actions::{self, McpTarget};
use agent_config_editor_lib::fsutil::{self, BackupStore, Homes};
use agent_config_editor_lib::model::{Agent, AppSettings, Scope};
use agent_config_editor_lib::{scan, writeplan};
use std::fs;

fn apply(homes: &Homes, backups: &BackupStore, plan: &agent_config_editor_lib::model::WritePlan) {
    let ctx = writeplan::ApplyCtx {
        homes,
        backups,
        on_written: &|_| {},
    };
    writeplan::apply(&ctx, plan).expect("apply");
}

#[test]
#[ignore]
fn codex_toggle_round_trips_byte_identical() {
    let homes = Homes::detect().unwrap();
    assert!(homes.sandbox, "refusing to run outside ACE_HOME sandbox");
    let snapshot = scan::scan_all(&homes, &AppSettings::default());
    let config = homes.codex_home.join("config.toml");
    let server = snapshot
        .mcp_servers
        .iter()
        .find(|m| {
            m.agent == Agent::Codex
                && m.scope == Scope::User
                && m.enabled.enabled == Some(true)
                && m.origin.file == fsutil::display(&config)
        })
        .expect("an enabled user-scope Codex server");
    println!("toggling codex server `{}`", server.name);
    let original = fs::read_to_string(&config).unwrap();
    let mode_before = fsutil::mode_of(&fs::metadata(&config).unwrap());
    let tmp = tempfile::tempdir().unwrap();
    let backups = BackupStore::new(tmp.path().to_path_buf(), 5);
    let target = McpTarget {
        agent: Agent::Codex,
        scope: Scope::User,
        file: server.origin.file.clone(),
        name: server.name.clone(),
    };

    let plan = actions::plan_set_mcp_enabled(&homes, &target, false, None).unwrap();
    println!("--- disable diff ---\n{}", plan.unified_diff);
    apply(&homes, &backups, &plan);
    let disabled = fs::read_to_string(&config).unwrap();
    let added: Vec<&str> = plan
        .unified_diff
        .lines()
        .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
        .collect();
    let removed: Vec<&str> = plan
        .unified_diff
        .lines()
        .filter(|l| l.starts_with('-') && !l.starts_with("---"))
        .collect();
    assert_eq!(added, vec!["+enabled = false"], "exactly one line added");
    assert!(removed.is_empty(), "nothing removed");
    assert_eq!(
        fsutil::mode_of(&fs::metadata(&config).unwrap()),
        mode_before,
        "mode preserved"
    );
    assert!(
        disabled.contains("# cmux-codex-hooks-feature"),
        "sentinel comments intact"
    );
    let rescanned = scan::scan_all(&homes, &AppSettings::default());
    let after = rescanned
        .mcp_servers
        .iter()
        .find(|m| m.id == server.id)
        .unwrap();
    assert_eq!(after.enabled.enabled, Some(false), "rescan sees the toggle");

    let plan = actions::plan_set_mcp_enabled(&homes, &target, true, None).unwrap();
    apply(&homes, &backups, &plan);
    let restored = fs::read_to_string(&config).unwrap();
    assert_eq!(
        restored, original,
        "re-enabling restores the file byte-for-byte"
    );
    println!("codex round-trip OK, mode {:o}", mode_before);
}

#[test]
#[ignore]
fn claude_project_toggle_round_trips() {
    let homes = Homes::detect().unwrap();
    assert!(homes.sandbox, "refusing to run outside ACE_HOME sandbox");
    let snapshot = scan::scan_all(&homes, &AppSettings::default());
    let server = snapshot
        .mcp_servers
        .iter()
        .find(|m| m.agent == Agent::Claude && m.scope == Scope::User)
        .expect("a user-scope Claude server");
    let project = snapshot
        .projects
        .iter()
        .find(|p| p.exists && p.tracked_by.contains(&Agent::Claude))
        .expect("a project");
    println!(
        "toggling claude server `{}` in {}",
        server.name, project.root
    );
    let original = fs::read_to_string(&homes.claude_state).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let backups = BackupStore::new(tmp.path().to_path_buf(), 5);
    let target = McpTarget {
        agent: Agent::Claude,
        scope: Scope::User,
        file: server.origin.file.clone(),
        name: server.name.clone(),
    };
    let already_disabled = server
        .extra
        .get("disabledInProjects")
        .and_then(|v| v.as_array())
        .is_some_and(|a| a.iter().any(|r| r.as_str() == Some(project.root.as_str())));

    let plan =
        actions::plan_set_mcp_enabled(&homes, &target, already_disabled, Some(&project.root))
            .unwrap();
    println!("--- toggle diff ---\n{}", plan.unified_diff);
    apply(&homes, &backups, &plan);
    let changed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&homes.claude_state).unwrap()).unwrap();
    let list = changed["projects"][&project.root]["disabledMcpServers"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        list.iter()
            .any(|v| v.as_str() == Some(server.name.as_str())),
        !already_disabled
    );

    let plan =
        actions::plan_set_mcp_enabled(&homes, &target, !already_disabled, Some(&project.root))
            .unwrap();
    apply(&homes, &backups, &plan);
    let restored = fs::read_to_string(&homes.claude_state).unwrap();
    let a: serde_json::Value = serde_json::from_str(&original).unwrap();
    let b: serde_json::Value = serde_json::from_str(&restored).unwrap();
    assert_eq!(a, b, "semantically identical after round trip");
    println!(
        "claude round-trip OK; bytes identical: {} (original {} bytes, restored {} bytes)",
        restored == original,
        original.len(),
        restored.len()
    );
    if restored != original {
        let diff = writeplan::unified_diff(&homes.claude_state, &original, &restored);
        println!(
            "formatting differences:\n{}",
            diff.lines().take(30).collect::<Vec<_>>().join("\n")
        );
    }
}
