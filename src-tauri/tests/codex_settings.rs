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
