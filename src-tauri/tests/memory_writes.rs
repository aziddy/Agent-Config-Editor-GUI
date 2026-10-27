//! The memory editor uses the shared raw-text write path, including linked instructions.
use agent_config_editor_lib::actions::plan_save_file_text;
use agent_config_editor_lib::error::ErrorCode;
use agent_config_editor_lib::fsutil::{self, BackupStore, Homes};
use agent_config_editor_lib::writeplan::{apply, ApplyCtx};
use std::fs;
use std::path::Path;

fn preview(
    homes: &Homes,
    path: &Path,
    original: &str,
    next: &str,
) -> agent_config_editor_lib::model::WritePlan {
    plan_save_file_text(
        homes,
        &fsutil::display(path),
        next.into(),
        Some(fsutil::sha256_hex(original.as_bytes())),
    )
    .unwrap()
}

#[test]
fn memory_save_preserves_bytes_mode_and_creates_backup() {
    let temp = tempfile::tempdir().unwrap();
    let homes = Homes::for_test(temp.path());
    let path = temp.path().join("MEMORY.md");
    let original = "---\r\nname: example\r\n---\r\nKeep\tthis.\r\n";
    fs::write(&path, original).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let backups = BackupStore::new(temp.path().join("backups"), 5);
    let ctx = ApplyCtx {
        homes: &homes,
        backups: &backups,
        on_written: &|_| {},
    };
    let plan = preview(&homes, &path, original, original);
    apply(&ctx, &plan).unwrap();
    assert_eq!(fs::read(&path).unwrap(), original.as_bytes());
    #[cfg(unix)]
    assert_eq!(fsutil::mode_of(&fs::metadata(&path).unwrap()), 0o600);
    let plan = preview(&homes, &path, original, "Updated memory\n");
    let result = apply(&ctx, &plan).unwrap();
    assert_eq!(
        fs::read_to_string(result.backup_path.unwrap()).unwrap(),
        original
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "Updated memory\n");
}

#[cfg(unix)]
#[test]
fn linked_instruction_updates_target_and_preserves_link() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let homes = Homes::for_test(temp.path());
    let target = temp.path().join("AGENTS.md");
    let alias = temp.path().join("CLAUDE.md");
    fs::write(&target, "Before\n").unwrap();
    symlink("AGENTS.md", &alias).unwrap();
    let plan = preview(&homes, &alias, "Before\n", "After\n");
    assert_eq!(Path::new(&plan.path), fs::canonicalize(&target).unwrap());
    let backups = BackupStore::new(temp.path().join("backups"), 5);
    let result = apply(
        &ApplyCtx {
            homes: &homes,
            backups: &backups,
            on_written: &|_| {},
        },
        &plan,
    )
    .unwrap();
    assert!(fs::symlink_metadata(&alias)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read_link(&alias).unwrap(), Path::new("AGENTS.md"));
    assert_eq!(fs::read_to_string(&alias).unwrap(), "After\n");
    assert_eq!(
        fs::read_to_string(result.backup_path.unwrap()).unwrap(),
        "Before\n"
    );
}

#[cfg(unix)]
#[test]
fn deleted_codex_config_in_linked_home_can_be_restored_with_private_mode() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let temp = tempfile::tempdir().unwrap();
    let homes = Homes::for_test(temp.path());
    let directory = temp.path().join("codex-state");
    fs::create_dir(&directory).unwrap();
    symlink(&directory, &homes.codex_home).unwrap();
    let path = homes.codex_home.join("config.toml");
    fs::write(&path, "# original\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let plan = preview(&homes, &path, "# original\n", "# edited\n");
    let backups = BackupStore::new(temp.path().join("backups"), 5);
    let ctx = ApplyCtx {
        homes: &homes,
        backups: &backups,
        on_written: &|_| {},
    };
    apply(&ctx, &plan).unwrap();
    fs::remove_file(&path).unwrap();
    let target = fsutil::resolve_write_path(&path).unwrap();
    let history = backups.list(&target).unwrap();
    assert_eq!(history.len(), 1);
    let text = fs::read_to_string(&history[0].path).unwrap();
    let restore = plan_save_file_text(&homes, &plan.path, text, None).unwrap();
    apply(&ctx, &restore).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "# original\n");
    assert_eq!(fsutil::mode_of(&fs::metadata(&path).unwrap()), 0o600);
    assert!(fs::symlink_metadata(&homes.codex_home)
        .unwrap()
        .is_symlink());
}

#[cfg(unix)]
#[test]
fn retargeting_link_after_preview_rejects_even_identical_content() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let homes = Homes::for_test(temp.path());
    let first = temp.path().join("first.md");
    let second = temp.path().join("second.md");
    let alias = temp.path().join("CLAUDE.md");
    fs::write(&first, "Same\n").unwrap();
    fs::write(&second, "Same\n").unwrap();
    symlink(&first, &alias).unwrap();
    let plan = preview(&homes, &alias, "Same\n", "Edit\n");
    fs::remove_file(&alias).unwrap();
    symlink(&second, &alias).unwrap();
    let backups = BackupStore::new(temp.path().join("backups"), 5);
    let error = apply(
        &ApplyCtx {
            homes: &homes,
            backups: &backups,
            on_written: &|_| {},
        },
        &plan,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::ExternalChange);
    assert_eq!(fs::read_to_string(&first).unwrap(), "Same\n");
    assert_eq!(fs::read_to_string(&second).unwrap(), "Same\n");
}

#[test]
fn stale_or_removed_memory_cannot_be_saved() {
    let temp = tempfile::tempdir().unwrap();
    let homes = Homes::for_test(temp.path());
    let path = temp.path().join("MEMORY.md");
    fs::write(&path, "Before\n").unwrap();
    let plan = preview(&homes, &path, "Before\n", "Edit\n");
    fs::write(&path, "External\n").unwrap();
    let backups = BackupStore::new(temp.path().join("backups"), 5);
    let error = apply(
        &ApplyCtx {
            homes: &homes,
            backups: &backups,
            on_written: &|_| {},
        },
        &plan,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::ExternalChange);
    fs::remove_file(&path).unwrap();
    let error = plan_save_file_text(
        &homes,
        &fsutil::display(&path),
        "Edit\n".into(),
        Some(fsutil::sha256_hex(b"Before\n")),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::ExternalChange);
    assert!(!path.exists());
}

#[test]
fn read_only_is_enforced_at_preview_and_apply() {
    let temp = tempfile::tempdir().unwrap();
    let mut homes = Homes::for_test(temp.path());
    let path = temp.path().join("MEMORY.md");
    fs::write(&path, "Before\n").unwrap();
    let plan = preview(&homes, &path, "Before\n", "Edit\n");
    homes.read_only = true;
    assert!(plan_save_file_text(&homes, &fsutil::display(&path), "Edit\n".into(), None).is_err());
    let backups = BackupStore::new(temp.path().join("backups"), 5);
    let error = apply(
        &ApplyCtx {
            homes: &homes,
            backups: &backups,
            on_written: &|_| {},
        },
        &plan,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::ReadOnly);
    assert_eq!(fs::read_to_string(&path).unwrap(), "Before\n");
}

#[cfg(unix)]
#[test]
fn sandbox_rejects_links_and_parent_traversal_outside_home() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("sandbox");
    fs::create_dir(&home).unwrap();
    let mut homes = Homes::for_test(&home);
    homes.allow_project_writes = false;
    let outside = temp.path().join("outside.md");
    fs::write(&outside, "Private\n").unwrap();
    let alias = home.join("CLAUDE.md");
    symlink(&outside, &alias).unwrap();
    for path in [
        &alias,
        &home.join("../outside.md"),
        &home.join("missing/../../outside.md"),
    ] {
        assert_eq!(
            plan_save_file_text(&homes, &fsutil::display(path), "Edit\n".into(), None)
                .unwrap_err()
                .code,
            ErrorCode::ReadOnly
        );
    }
    let directory_alias = home.join("linked-dir");
    symlink(temp.path(), &directory_alias).unwrap();
    assert!(plan_save_file_text(
        &homes,
        &fsutil::display(&directory_alias.join("new.md")),
        "Edit\n".into(),
        None
    )
    .is_err());
    assert_eq!(
        plan_save_file_text(
            &homes,
            &fsutil::display(&home.join("missing/../linked-dir/new.md")),
            "Edit\n".into(),
            None,
        )
        .unwrap_err()
        .code,
        ErrorCode::ReadOnly,
    );
    assert_eq!(fs::read_to_string(&outside).unwrap(), "Private\n");
    assert!(!temp.path().join("new.md").exists());
}
