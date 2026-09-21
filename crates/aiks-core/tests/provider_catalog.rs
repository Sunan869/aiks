use aiks_core::{
    model::SourceKind,
    providers::{
        build_registry,
        catalog::{descriptors, ALL_SOURCES},
        settings::patch_provider_toml,
    },
    Config,
};

#[test]
fn catalog_has_nineteen_unique_stable_keys_and_keeps_old_provider_ids() {
    let config = Config::default();
    let catalog = descriptors(&config, &[]);
    let keys: std::collections::HashSet<_> = catalog.iter().map(|s| s.key).collect();
    assert_eq!(keys.len(), 19);
    assert_eq!(catalog.iter().filter(|d| d.configurable).count(), 16);
    assert!(patch_provider_toml("", "chatgpt_share", true, &[]).is_err());
    for source in ALL_SOURCES {
        assert_eq!(SourceKind::from_str(source.as_str()), Some(source));
        assert!(build_registry(&config).get(source).is_some());
    }
    for key in [
        "claude_code",
        "codex",
        "gemini_cli",
        "opencode",
        "workbuddy",
    ] {
        assert!(keys.contains(key));
    }
    assert_eq!(
        catalog
            .iter()
            .find(|p| p.key == "workbuddy")
            .unwrap()
            .display_name,
        "WorkBuddy"
    );
    assert_eq!(SourceKind::from_str("continue"), Some(SourceKind::Continue));
}
#[test]
fn configuration_edit_preserves_unknown_and_unrelated_values() {
    let old = "[ai]\nmodel='preserved-model'\n[future]\nvalue=42\n[providers.codex]\ninclude_archived_sessions=false\n[providers.continue]\ncustom_future='retained'\n";
    let next = patch_provider_toml(
        old,
        "continue",
        false,
        &["/example/a".into(), "/example/b".into()],
    )
    .unwrap();
    let value: toml::Value = toml::from_str(&next).unwrap();
    assert_eq!(value["ai"]["model"].as_str(), Some("preserved-model"));
    assert_eq!(value["future"]["value"].as_integer(), Some(42));
    assert_eq!(
        value["providers"]["codex"]["include_archived_sessions"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["providers"]["continue"]["custom_future"].as_str(),
        Some("retained")
    );
    let config: Config = toml::from_str(&next).unwrap();
    assert!(!config.providers.continue_dev.enabled);
    assert_eq!(config.providers.continue_dev.paths.len(), 2);
}
#[test]
fn provider_edits_reject_unknown_keys_and_do_not_change_old_serde_keys() {
    assert!(patch_provider_toml("", "../../ai", true, &[]).is_err());
    assert!(patch_provider_toml("broken", "continue", true, &[]).is_err());
    assert!(patch_provider_toml("", "codex", true, &["a".into(), "b".into()]).is_err());
    assert_eq!(
        serde_json::to_value(SourceKind::WorkBuddy).unwrap(),
        "work_buddy"
    );
    assert_eq!(
        serde_json::to_value(SourceKind::OpenCode).unwrap(),
        "open_code"
    );
}
