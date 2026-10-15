use agent_config_editor_lib::actions;
use agent_config_editor_lib::fsutil::{self, BackupStore, Homes};
use agent_config_editor_lib::writeplan::{self, ApplyCtx};
use std::fs;

#[test]
fn creates_metadata_only_after_apply_and_preserves_skill_markdown() {
    let dir = tempfile::tempdir().unwrap();
    let homes = Homes::for_test(dir.path());
    let skill = homes.codex_home.join("skills/demo/SKILL.md");
    fs::create_dir_all(skill.parent().unwrap()).unwrap();
    let markdown = "---\nname: demo\ndisable-model-invocation: true\n---\n# Keep body\n";
    fs::write(&skill, markdown).unwrap();
    let id = format!("codex:skill:{}", fsutil::display(&skill));
    let state = actions::get_codex_skill_invocation(&id).unwrap();
    assert!(
        state.allow_implicit_invocation,
        "Claude frontmatter is ignored by Codex"
    );
    let plan =
        actions::plan_set_codex_skill_invocation(&homes, &id, false, state.base_hash).unwrap();
    assert!(
        !std::path::Path::new(&plan.path).exists(),
        "preview must not create metadata"
    );
    let backups = BackupStore::new(dir.path().join("backups"), 5);
    writeplan::apply(
        &ApplyCtx {
            homes: &homes,
            backups: &backups,
            on_written: &|_| {},
        },
        &plan,
    )
    .unwrap();
    assert_eq!(fs::read_to_string(&skill).unwrap(), markdown);
    let state = actions::get_codex_skill_invocation(&id).unwrap();
    assert!(!state.allow_implicit_invocation);
    let no_op =
        actions::plan_set_codex_skill_invocation(&homes, &id, false, state.base_hash).unwrap();
    assert!(no_op.warnings.contains(&"no changes".to_string()));
}

#[test]
fn preserves_metadata_and_mode_and_rebases_unrelated_external_edits() {
    let dir = tempfile::tempdir().unwrap();
    let homes = Homes::for_test(dir.path());
    let skill = homes.codex_home.join("skills/demo/SKILL.md");
    fs::create_dir_all(skill.parent().unwrap()).unwrap();
    fs::write(&skill, "# Demo\n").unwrap();
    let metadata = skill.parent().unwrap().join("agents/openai.yaml");
    let original = "# keep\ninterface:\n  display_name: 'Demo'\npolicy:\n  allow_implicit_invocation: true # keep too\ndependencies: { tools: [] }\n";
    fsutil::atomic_write(&metadata, original, Some(0o600)).unwrap();
    let id = format!("codex:skill:{}", fsutil::display(&skill));
    let state = actions::get_codex_skill_invocation(&id).unwrap();
    let plan =
        actions::plan_set_codex_skill_invocation(&homes, &id, false, state.base_hash.clone())
            .unwrap();
    assert_eq!(
        plan.new_text.as_deref(),
        Some(
            original
                .replace("invocation: true", "invocation: false")
                .as_str()
        )
    );
    fs::write(&metadata, format!("{original}extra: preserved\n")).unwrap();
    assert!(actions::plan_set_codex_skill_invocation(&homes, &id, false, state.base_hash).is_err());
    let backups = BackupStore::new(dir.path().join("backups"), 5);
    writeplan::apply(
        &ApplyCtx {
            homes: &homes,
            backups: &backups,
            on_written: &|_| {},
        },
        &plan,
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(&metadata).unwrap(),
        format!(
            "{}extra: preserved\n",
            original.replace("invocation: true", "invocation: false")
        )
    );
    assert_eq!(fsutil::mode_of(&fs::metadata(&metadata).unwrap()), 0o600);
    assert!(actions::get_codex_skill_invocation("claude:skill:/test").is_err());
    let mut read_only = homes;
    read_only.read_only = true;
    assert!(actions::plan_set_codex_skill_invocation(&read_only, &id, true, None).is_err());
}
