use agent_config_editor_lib::fsutil::{self, Homes};
use agent_config_editor_lib::model::{AppSettings, Skill};
use agent_config_editor_lib::scan::scan_all;
use std::fs;

fn scanned_skill(homes: &Homes) -> Skill {
    let snapshot = scan_all(homes, &AppSettings::default());
    assert_eq!(snapshot.skills.len(), 1);
    snapshot.skills.into_iter().next().unwrap()
}

#[test]
fn codex_scan_tracks_policy_defaults_errors_hashes_and_metadata_lifecycle() {
    let temp = tempfile::tempdir().unwrap();
    let homes = Homes::for_test(temp.path());
    let dir = homes.codex_home.join("skills/demo");
    fs::create_dir_all(&dir).unwrap();
    // Claude's flag must never be interpreted as the Codex policy.
    fs::write(
        dir.join("SKILL.md"),
        "---\nname: demo\ndisable-model-invocation: true\n---\nDemo\n",
    )
    .unwrap();
    let metadata = dir.join("agents/openai.yaml");
    assert_eq!(scanned_skill(&homes).allow_implicit_invocation, Some(true));
    assert!(!metadata.exists(), "scanning must not create metadata");
    fs::create_dir_all(metadata.parent().unwrap()).unwrap();

    for (text, expected) in [
        ("policy:\n  allow_implicit_invocation: false\n", Some(false)),
        ("policy: {allow_implicit_invocation: true}\n", Some(true)),
        ("interface: {display_name: Demo}\n", Some(true)),
        ("policy: {}\n", Some(true)),
        ("policy: null\n", Some(true)),
        ("", Some(true)),
        ("policy: {allow_implicit_invocation: 'false'}\n", None),
        ("policy: {allow_implicit_invocation: null}\n", None),
        ("policy: [false]\n", None),
        ("policy: [\n", None),
    ] {
        fs::write(&metadata, text).unwrap();
        let snapshot = scan_all(&homes, &AppSettings::default());
        assert_eq!(
            snapshot.skills[0].allow_implicit_invocation, expected,
            "{text}"
        );
        assert_eq!(
            snapshot.file_hashes[&fsutil::display(&metadata)],
            fsutil::sha256_hex(text.as_bytes())
        );
        assert_eq!(
            snapshot
                .warnings
                .iter()
                .any(|w| w.file.as_deref() == Some(&fsutil::display(&metadata))),
            expected.is_none(),
            "{text}"
        );
        let serialized = serde_json::to_value(&snapshot.skills[0]).unwrap();
        assert_eq!(
            serialized["allowImplicitInvocation"],
            serde_json::to_value(expected).unwrap()
        );
        assert_eq!(fs::read_to_string(&metadata).unwrap(), text);
    }

    fs::write(&metadata, [0xff, 0xfe]).unwrap();
    let snapshot = scan_all(&homes, &AppSettings::default());
    assert_eq!(snapshot.skills[0].allow_implicit_invocation, None);
    assert!(snapshot
        .warnings
        .iter()
        .any(|w| w.file.as_deref() == Some(&fsutil::display(&metadata))));

    fs::remove_file(&metadata).unwrap();
    let snapshot = scan_all(&homes, &AppSettings::default());
    assert_eq!(snapshot.skills[0].allow_implicit_invocation, Some(true));
    assert!(!snapshot
        .file_hashes
        .contains_key(&fsutil::display(&metadata)));
}

#[test]
fn claude_scan_inverts_frontmatter_and_retains_lenient_parsing() {
    let temp = tempfile::tempdir().unwrap();
    let homes = Homes::for_test(temp.path());
    let dir = homes.claude_dir.join("skills/demo");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("SKILL.md");
    for (text, expected) in [
        ("# Demo\n", Some(true)),
        ("---\nname: demo\n---\nDemo\n", Some(true)),
        ("---\ndisable-model-invocation: true\n---\n", Some(false)),
        ("---\ndisable-model-invocation: false\n---\n", Some(true)),
        ("---\ndisable-model-invocation: ' YES '\n---\n", Some(false)),
        ("---\ndisable-model-invocation: 'no'\n---\n", Some(true)),
        (
            "---\ndescription: Use when: needed\ndisable-model-invocation: true\n---\n",
            Some(false),
        ),
        (
            "---\nname: demo\ndisable-model-invocation: maybe\n---\n",
            None,
        ),
        ("---\ndisable-model-invocation: [true]\n---\n", None),
        ("---\ndisable-model-invocation: null\n---\n", None),
        ("---\ndisable-model-invocation: true\n", None),
        ("---\n[broken\n---\n", None),
    ] {
        fs::write(&path, text).unwrap();
        let snapshot = scan_all(&homes, &AppSettings::default());
        assert_eq!(snapshot.skills.len(), 1);
        assert_eq!(
            snapshot.skills[0].allow_implicit_invocation, expected,
            "{text}"
        );
        assert_eq!(snapshot.skills[0].enabled.enabled, None);
        assert_eq!(
            snapshot
                .warnings
                .iter()
                .any(|w| w.file.as_deref() == Some(&fsutil::display(&path))),
            expected.is_none(),
            "{text}"
        );
    }
}
