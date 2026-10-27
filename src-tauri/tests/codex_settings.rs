use agent_config_editor_lib::actions::{plan_save_codex_settings, plan_save_file_text};
use agent_config_editor_lib::codex::settings;
use agent_config_editor_lib::error::ErrorCode;
use agent_config_editor_lib::fsutil::{self, BackupStore, Homes};
use agent_config_editor_lib::model::{CodexSettingChange, WritePlan};
use agent_config_editor_lib::writeplan::{apply, ApplyCtx};
use serde_json::{json, Value};
use std::fs;

const ORIGINAL: &str = r#"# retain root comment
model = 'custom-model' # retain inline comment
model_reasoning_effort = "future-effort"
service_tier = "fast"

[features]
memories = true
hooks = false

[memories]
generate_memories = true
use_memories = false

[model_providers."custom.provider"]
base_url = 'https://example.invalid'
experimental_bearer_token = 'fictional-private-value'

[mcp_servers."server.with.dots"]
command = 'keep literal \n spelling'

[empty]
# retain empty table
"#;

fn setup(text: &str) -> (tempfile::TempDir, Homes) {
    let temp = tempfile::tempdir().unwrap();
    let mut homes = Homes::for_test(temp.path());
    homes.allow_project_writes = false;
    fs::create_dir_all(&homes.codex_home).unwrap();
    let config = homes.codex_home.join("config.toml");
    fs::write(&config, text).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    }
    (temp, homes)
}

fn change(path: &[&str], value: Value) -> CodexSettingChange {
    CodexSettingChange {
        path: path.iter().map(|s| (*s).into()).collect(),
        value: if value.is_null() { None } else { Some(value) },
    }
}

fn preview(homes: &Homes, changes: &[CodexSettingChange]) -> WritePlan {
    let data = settings::read(homes).unwrap();
    plan_save_codex_settings(homes, changes, &data.hash).unwrap()
}

fn apply_plan(homes: &Homes, plan: &WritePlan) {
    let backups = BackupStore::new(homes.home.join("backups"), 5);
    apply(
        &ApplyCtx {
            homes,
            backups: &backups,
            on_written: &|_| {},
        },
        plan,
    )
    .unwrap();
}

#[test]
fn projection_excludes_provider_secrets_and_keeps_custom_values() {
    let (_temp, homes) = setup(ORIGINAL);
    let data = settings::read(&homes).unwrap();
    assert_eq!(data.provider_names, vec!["custom.provider"]);
    assert_eq!(data.values["model"], "custom-model");
    assert_eq!(data.values["model_reasoning_effort"], "future-effort");
    assert_eq!(data.values["features"]["hooks"], false);
    assert_eq!(data.mode, 0o600);
    assert!(data.unsupported_paths.is_empty());
    let serialized = serde_json::to_string(&data).unwrap();
    assert!(!serialized.contains("fictional-private-value"));
    assert!(!serialized.contains("experimental_bearer_token"));
}

#[test]
fn batch_save_changes_only_selected_keys_and_keeps_backups_mode_and_comments() {
    let (_temp, homes) = setup(ORIGINAL);
    let path = homes.codex_home.join("config.toml");
    let plan = preview(
        &homes,
        &[
            change(&["model"], json!("next-model")),
            change(&["features", "memories"], json!(false)),
            change(&["service_tier"], Value::Null),
        ],
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        ORIGINAL,
        "preview does not write"
    );
    let text = plan.new_text.as_ref().unwrap();
    assert!(text.contains("model = \"next-model\" # retain inline comment"));
    assert!(text.contains("memories = false"));
    assert!(!text.contains("service_tier"));
    assert!(text.ends_with(&ORIGINAL[ORIGINAL.find("[model_providers.").unwrap()..]));
    assert!(text.contains("hooks = false"));
    apply_plan(&homes, &plan);
    assert_eq!(fs::read_to_string(&path).unwrap(), *text);
    assert_eq!(fsutil::mode_of(&fs::metadata(&path).unwrap()), 0o600);
    let backups = BackupStore::new(homes.home.join("backups"), 5);
    let history = backups.list(std::path::Path::new(&plan.path)).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(fs::read_to_string(&history[0].path).unwrap(), ORIGINAL);
}

#[test]
fn noop_save_is_byte_identical_even_with_explicit_unchanged_values() {
    let (_temp, homes) = setup(ORIGINAL);
    let plan = preview(
        &homes,
        &[
            change(&["model"], json!("custom-model")),
            change(&["model_reasoning_effort"], json!("future-effort")),
            change(&["features", "hooks"], json!(false)),
            change(&["web_search"], Value::Null),
        ],
    );
    assert!(plan.warnings.iter().any(|w| w == "no changes"));
    assert_eq!(plan.new_text.as_deref(), Some(ORIGINAL));
    assert!(plan.unified_diff.is_empty());
    apply_plan(&homes, &plan);
    assert_eq!(
        fs::read(homes.codex_home.join("config.toml")).unwrap(),
        ORIGINAL.as_bytes()
    );
}

#[test]
fn expanded_settings_round_trip_without_defaults_or_unrelated_changes() {
    let (_temp, homes) = setup(ORIGINAL);
    let plan = preview(
        &homes,
        &[
            change(&["model_auto_compact_token_limit"], json!(120000)),
            change(
                &["model_auto_compact_token_limit_scope"],
                json!("body_after_prefix"),
            ),
            change(&["agents", "max_concurrent_threads_per_session"], json!(6)),
            change(
                &["agents", "default_subagent_model"],
                json!("custom-agent-model"),
            ),
            change(&["history", "persistence"], json!("none")),
            change(&["history", "max_bytes"], json!(0)),
            change(&["analytics", "enabled"], json!(false)),
            change(&["feedback", "enabled"], json!(false)),
            change(&["project_doc_fallback_filenames"], json!(["TEAM.md"])),
            change(&["skills", "max_context_tokens"], json!(10000)),
            change(&["memories", "max_unused_days"], json!(0)),
            change(&["memories", "extract_model"], json!("custom-memory-model")),
            change(&["features", "goals"], json!(false)),
            change(&["tui", "notification_condition"], json!("always")),
        ],
    );
    let text = plan.new_text.as_ref().unwrap();
    assert!(text.contains("max_concurrent_threads_per_session = 6"));
    assert!(!text.contains("model_context_window"));
    assert!(!text.contains("fast_mode"));
    assert!(text.contains("model = 'custom-model' # retain inline comment"));
    assert!(text.contains("experimental_bearer_token = 'fictional-private-value'"));
    apply_plan(&homes, &plan);
    let data = settings::read(&homes).unwrap();
    assert!(data.unsupported_paths.is_empty());
    assert_eq!(
        data.values["agents"]["max_concurrent_threads_per_session"],
        6
    );
    assert_eq!(data.values["history"]["max_bytes"], 0);
    assert_eq!(data.values["analytics"]["enabled"], false);
    assert_eq!(data.values["memories"]["max_unused_days"], 0);
    assert_eq!(data.values["skills"]["max_context_tokens"], 10000);
    let noop = preview(
        &homes,
        &[
            change(&["agents", "max_concurrent_threads_per_session"], json!(6)),
            change(&["history", "max_bytes"], json!(0)),
            change(&["features", "goals"], json!(false)),
        ],
    );
    assert!(noop.unified_diff.is_empty());
    assert_eq!(noop.new_text, plan.new_text);
    let remove = preview(
        &homes,
        &[change(
            &["agents", "max_concurrent_threads_per_session"],
            Value::Null,
        )],
    );
    apply_plan(&homes, &remove);
    assert!(settings::read(&homes).unwrap().values["agents"]
        .get("max_concurrent_threads_per_session")
        .is_none());
}

#[test]
fn invalid_numeric_limits_and_empty_lists_are_rejected_without_writes() {
    let (_temp, homes) = setup(ORIGINAL);
    for edit in [
        change(&["model_context_window"], json!(1.5)),
        change(&["model_context_window"], json!("1000")),
        change(&["model_context_window"], json!(9_007_199_254_740_992_i64)),
        change(&["agents", "max_concurrent_threads_per_session"], json!(0)),
        change(&["history", "max_bytes"], json!(-1)),
        change(&["memories", "max_unused_days"], json!(366)),
        change(&["memories", "min_rollout_idle_hours"], json!(0)),
        change(
            &["memories", "min_rate_limit_remaining_percent"],
            json!(101),
        ),
        change(&["skills", "max_context_tokens"], json!(10001)),
        change(&["project_doc_fallback_filenames"], json!([""])),
        change(&["tui", "notifications"], json!([" "])),
    ] {
        let data = settings::read(&homes).unwrap();
        assert_eq!(
            plan_save_codex_settings(&homes, &[edit], &data.hash)
                .unwrap_err()
                .code,
            ErrorCode::InvalidInput
        );
    }
    assert_eq!(
        fs::read_to_string(homes.codex_home.join("config.toml")).unwrap(),
        ORIGINAL
    );
}

#[test]
fn notification_shapes_and_array_comments_survive_edits() {
    let original = "model = 'keep'\n[tui]\nnotifications = ['agent-turn-complete'] # keep event comment\nstatus_line = [\n  'model', # keep element comment\n  'context-used'\n] # keep footer comment\n";
    let (_temp, homes) = setup(original);
    let plan = preview(
        &homes,
        &[
            change(&["tui", "notifications"], json!(["approval-requested"])),
            change(
                &["tui", "status_line"],
                json!(["model", "context-remaining"]),
            ),
        ],
    );
    assert!(plan
        .new_text
        .as_ref()
        .unwrap()
        .contains("# keep event comment"));
    assert!(plan
        .new_text
        .as_ref()
        .unwrap()
        .contains("# keep footer comment"));
    assert!(plan
        .new_text
        .as_ref()
        .unwrap()
        .contains("'model', # keep element comment"));
    apply_plan(&homes, &plan);
    let plan = preview(&homes, &[change(&["tui", "notifications"], json!(false))]);
    assert!(plan
        .new_text
        .as_ref()
        .unwrap()
        .contains("notifications = false # keep event comment"));
    apply_plan(&homes, &plan);
    assert_eq!(
        settings::read(&homes).unwrap().values["tui"]["notifications"],
        false
    );
}

#[test]
fn environment_filter_edits_preserve_unchanged_quoting_comments_and_unrelated_values() {
    let original = "[shell_environment_policy]\ninherit = 'all'\n[shell_environment_policy.filters]\n'PATH' = 'include' # leave this byte-identical\n\"SECRET.*\" = 'exclude' # update this\nOLD = 'include'\n[shell_environment_policy.set]\nPRIVATE_TOKEN = 'fictional-private-value'\n";
    let (_temp, homes) = setup(original);
    let data = settings::read(&homes).unwrap();
    assert_eq!(
        data.values["shell_environment_policy"]["filters"]["SECRET.*"],
        "exclude"
    );
    assert!(!serde_json::to_string(&data)
        .unwrap()
        .contains("fictional-private-value"));
    let plan = preview(
        &homes,
        &[change(
            &["shell_environment_policy", "filters"],
            json!({"PATH":"include", "SECRET.*":"include", "__proto__":"exclude"}),
        )],
    );
    let text = plan.new_text.as_ref().unwrap();
    assert!(text.contains("'PATH' = 'include' # leave this byte-identical"));
    assert!(text.contains("\"SECRET.*\" = \"include\" # update this"));
    assert!(!text.contains("OLD ="));
    assert!(text
        .ends_with("[shell_environment_policy.set]\nPRIVATE_TOKEN = 'fictional-private-value'\n"));
    apply_plan(&homes, &plan);
    let data = settings::read(&homes).unwrap();
    assert_eq!(
        data.values["shell_environment_policy"]["filters"]["__proto__"],
        "exclude"
    );
    let noop = preview(
        &homes,
        &[change(
            &["shell_environment_policy", "filters"],
            json!({"__proto__":"exclude", "SECRET.*":"include", "PATH":"include"}),
        )],
    );
    assert!(noop.unified_diff.is_empty());
    assert_eq!(noop.new_text, plan.new_text);
    let reset = preview(
        &homes,
        &[change(
            &["shell_environment_policy", "filters"],
            Value::Null,
        )],
    );
    apply_plan(&homes, &reset);
    assert!(
        settings::read(&homes).unwrap().values["shell_environment_policy"]
            .get("filters")
            .is_none()
    );
}

#[test]
fn empty_and_inline_filter_maps_are_editable_and_removable() {
    for original in [
        "[shell_environment_policy.filters]\n",
        "shell_environment_policy = { filters = { PATH = 'include' }, inherit = 'core' }\n",
    ] {
        let (_temp, homes) = setup(original);
        let data = settings::read(&homes).unwrap();
        let noop = preview(
            &homes,
            &[change(
                &["shell_environment_policy", "filters"],
                data.values["shell_environment_policy"]["filters"].clone(),
            )],
        );
        assert_eq!(noop.new_text.as_deref(), Some(original));
        let plan = preview(
            &homes,
            &[change(
                &["shell_environment_policy", "filters"],
                json!({"NODE_*":"exclude"}),
            )],
        );
        apply_plan(&homes, &plan);
        assert_eq!(
            settings::read(&homes).unwrap().values["shell_environment_policy"]["filters"],
            json!({"NODE_*":"exclude"})
        );
        let empty = preview(
            &homes,
            &[change(&["shell_environment_policy", "filters"], json!({}))],
        );
        apply_plan(&homes, &empty);
        assert_eq!(
            settings::read(&homes).unwrap().values["shell_environment_policy"]["filters"],
            json!({})
        );
        let reset = preview(
            &homes,
            &[change(
                &["shell_environment_policy", "filters"],
                Value::Null,
            )],
        );
        apply_plan(&homes, &reset);
        assert!(!fs::read_to_string(homes.codex_home.join("config.toml"))
            .unwrap()
            .contains("filters"));
    }
}

#[test]
fn invalid_filter_maps_and_legacy_conflicts_cannot_produce_invalid_config() {
    let (_temp, homes) = setup(ORIGINAL);
    for value in [
        json!({"":"include"}),
        json!({"PATH":"invalid"}),
        json!({"PATH":"include", "path":"exclude"}),
        json!({"PATH":true}),
        json!(["PATH"]),
    ] {
        let data = settings::read(&homes).unwrap();
        assert_eq!(
            plan_save_codex_settings(
                &homes,
                &[change(&["shell_environment_policy", "filters"], value)],
                &data.hash
            )
            .unwrap_err()
            .code,
            ErrorCode::InvalidInput
        );
    }
    for legacy in ["exclude", "include_only"] {
        let original =
            format!("model = 'keep'\n[shell_environment_policy]\n{legacy} = ['KEY_*']\n");
        let (_temp, homes) = setup(&original);
        let data = settings::read(&homes).unwrap();
        assert!(plan_save_codex_settings(
            &homes,
            &[change(
                &["shell_environment_policy", "filters"],
                json!({"PATH":"include"})
            )],
            &data.hash
        )
        .is_err());
        assert!(preview(&homes, &[change(&["model"], json!("unrelated"))])
            .new_text
            .unwrap()
            .contains(&format!("{legacy} = ['KEY_*']")));
        assert_eq!(
            fs::read_to_string(homes.codex_home.join("config.toml")).unwrap(),
            original
        );
    }
    let (_temp, homes) = setup("[agents]\nmax_threads = 3\n");
    let data = settings::read(&homes).unwrap();
    assert!(plan_save_codex_settings(
        &homes,
        &[change(
            &["agents", "max_concurrent_threads_per_session"],
            json!(4)
        )],
        &data.hash
    )
    .is_err());
}

#[test]
fn unsupported_parents_and_unsafe_integers_remain_readonly() {
    let (_temp, homes) = setup("model = 'keep'\nmodel_context_window = 9007199254740992\nagents = true\n[features.network_proxy]\nenabled = true\n[history]\nmax_bytes = 1.5\n");
    let data = settings::read(&homes).unwrap();
    for path in [
        vec!["model_context_window"],
        vec!["agents", "enabled"],
        vec!["features", "network_proxy"],
        vec!["history", "max_bytes"],
    ] {
        assert!(data
            .unsupported_paths
            .contains(&path.iter().map(|s| (*s).into()).collect()));
    }
    assert!(!data.values.contains_key("model_context_window"));
    assert!(preview(&homes, &[change(&["model"], json!("changed"))])
        .new_text
        .unwrap()
        .contains("max_bytes = 1.5"));
}

#[test]
fn stale_files_are_rejected_before_preview_and_apply() {
    let (_temp, homes) = setup(ORIGINAL);
    let path = homes.codex_home.join("config.toml");
    let base = settings::read(&homes).unwrap();
    let edits = [change(&["model"], json!("edited"))];
    let plan = preview(&homes, &edits);
    fs::write(&path, format!("{ORIGINAL}\n# external edit\n")).unwrap();
    assert_eq!(
        plan_save_codex_settings(&homes, &edits, &base.hash)
            .unwrap_err()
            .code,
        ErrorCode::ExternalChange
    );
    let backups = BackupStore::new(homes.home.join("backups"), 5);
    assert_eq!(
        apply(
            &ApplyCtx {
                homes: &homes,
                backups: &backups,
                on_written: &|_| {}
            },
            &plan
        )
        .unwrap_err()
        .code,
        ErrorCode::ExternalChange
    );
    assert!(backups.list(&path).unwrap().is_empty());
}

#[test]
fn invalid_types_options_and_unexposed_paths_cannot_change_the_file() {
    let (_temp, homes) = setup(ORIGINAL);
    for edit in [
        change(&["features", "hooks"], json!("false")),
        change(&["web_search"], json!("fictional-secret-option")),
        change(&["tui", "status_line"], json!([true])),
        change(&["mcp_servers", "x", "command"], json!("new")),
        change(&["features"], json!({})),
    ] {
        let data = settings::read(&homes).unwrap();
        let error = plan_save_codex_settings(&homes, &[edit], &data.hash).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput);
        assert!(!error.message.contains("fictional-secret-option"));
    }
    assert_eq!(
        fs::read_to_string(homes.codex_home.join("config.toml")).unwrap(),
        ORIGINAL
    );
}

#[test]
fn custom_toml_shapes_are_readonly_without_blocking_other_fields() {
    let text = "model = 'original'\napproval_policy = { granular = { rules = true } }\n[features.code_mode]\nenabled = true\n";
    let (_temp, homes) = setup(text);
    let data = settings::read(&homes).unwrap();
    assert!(data
        .unsupported_paths
        .contains(&vec!["approval_policy".into()]));
    assert!(data
        .unsupported_paths
        .contains(&vec!["features".into(), "code_mode".into()]));
    assert!(!data.values.contains_key("approval_policy"));
    assert!(plan_save_codex_settings(
        &homes,
        &[change(&["approval_policy"], json!("never"))],
        &data.hash
    )
    .is_err());
    let plan = preview(&homes, &[change(&["model"], json!("changed"))]);
    assert!(plan
        .new_text
        .unwrap()
        .ends_with(&text[text.find("approval_policy").unwrap()..]));
}

#[test]
fn adding_and_unsetting_nested_settings_preserves_unrelated_sections() {
    let (_temp, homes) = setup(ORIGINAL);
    let plan = preview(
        &homes,
        &[
            change(&["tui", "status_line"], json!([])),
            change(&["memories", "generate_memories"], Value::Null),
            change(&["model_verbosity"], json!("low")),
        ],
    );
    apply_plan(&homes, &plan);
    let data = settings::read(&homes).unwrap();
    assert_eq!(data.values["tui"]["status_line"], json!([]));
    assert!(data.values["memories"].get("generate_memories").is_none());
    assert_eq!(data.values["memories"]["use_memories"], false);
    assert_eq!(data.values["model_verbosity"], "low");
}

#[test]
fn named_permissions_are_exposed_without_combining_legacy_sandbox_settings() {
    let (_temp, homes) =
        setup("default_permissions = 'team'\n[permissions.team]\ndescription = 'Team'\n");
    let data = settings::read(&homes).unwrap();
    assert!(data.named_permissions);
    assert_eq!(data.permission_profiles, vec!["team"]);
    assert!(plan_save_codex_settings(
        &homes,
        &[change(&["sandbox_mode"], json!("workspace-write"))],
        &data.hash
    )
    .is_err());
    let plan = preview(
        &homes,
        &[change(&["default_permissions"], json!(":read-only"))],
    );
    assert!(!plan.new_text.unwrap().contains("sandbox_mode"));
}

#[test]
fn readonly_is_enforced_during_preview_and_apply() {
    let (_temp, mut homes) = setup(ORIGINAL);
    let plan = preview(&homes, &[change(&["model"], json!("changed"))]);
    let hash = settings::read(&homes).unwrap().hash;
    homes.read_only = true;
    assert_eq!(
        plan_save_codex_settings(&homes, &[], &hash)
            .unwrap_err()
            .code,
        ErrorCode::ReadOnly
    );
    let backups = BackupStore::new(homes.home.join("backups"), 5);
    assert_eq!(
        apply(
            &ApplyCtx {
                homes: &homes,
                backups: &backups,
                on_written: &|_| {}
            },
            &plan
        )
        .unwrap_err()
        .code,
        ErrorCode::ReadOnly
    );
}

#[test]
fn missing_and_malformed_files_are_reported_without_leaking_source_text() {
    let (_temp, homes) = setup("model = 'fictional-secret-value' broken\n");
    let error = settings::read(&homes).unwrap_err();
    assert_eq!(error.code, ErrorCode::Parse);
    assert!(!error.message.contains("fictional-secret-value"));
    let path = homes.codex_home.join("config.toml");
    fs::remove_file(&path).unwrap();
    assert!(settings::read(&homes).is_err());
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn symlink_targets_are_preserved_and_retargeting_after_preview_is_rejected() {
    use std::os::unix::fs::symlink;
    let (_temp, homes) = setup(ORIGINAL);
    let path = homes.codex_home.join("config.toml");
    let target = homes.home.join("actual.toml");
    fs::rename(&path, &target).unwrap();
    symlink(&target, &path).unwrap();
    let plan = preview(&homes, &[change(&["model"], json!("changed"))]);
    apply_plan(&homes, &plan);
    assert!(fs::symlink_metadata(&path).unwrap().is_symlink());
    assert!(fs::read_to_string(&target).unwrap().contains("changed"));
    let plan = preview(&homes, &[change(&["model"], json!("next"))]);
    let other = homes.home.join("other.toml");
    fs::copy(&target, &other).unwrap();
    fs::remove_file(&path).unwrap();
    symlink(&other, &path).unwrap();
    let backups = BackupStore::new(homes.home.join("backups"), 5);
    assert_eq!(
        apply(
            &ApplyCtx {
                homes: &homes,
                backups: &backups,
                on_written: &|_| {}
            },
            &plan
        )
        .unwrap_err()
        .code,
        ErrorCode::ExternalChange
    );
}

#[cfg(unix)]
#[test]
fn sandbox_refuses_a_config_link_outside_its_home() {
    use std::os::unix::fs::symlink;
    let (_temp, homes) = setup(ORIGINAL);
    let outside = tempfile::NamedTempFile::new().unwrap();
    fs::write(outside.path(), ORIGINAL).unwrap();
    let path = homes.codex_home.join("config.toml");
    fs::remove_file(&path).unwrap();
    symlink(outside.path(), &path).unwrap();
    let hash = settings::read(&homes).unwrap().hash;
    assert_eq!(
        plan_save_codex_settings(&homes, &[change(&["model"], json!("changed"))], &hash)
            .unwrap_err()
            .code,
        ErrorCode::ReadOnly
    );
    // The existing raw editor still observes the same sandbox boundary.
    assert!(
        plan_save_file_text(&homes, &fsutil::display(&path), ORIGINAL.into(), Some(hash)).is_err()
    );
}
