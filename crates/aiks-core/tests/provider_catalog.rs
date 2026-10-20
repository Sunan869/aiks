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
fn catalog_has_twenty_four_unique_stable_keys_and_keeps_old_provider_ids() {
    let mut config = Config::default();
    // A fresh install is opt-in; exercise registry coverage with all sources
    // explicitly enabled rather than assuming defaults are enabled.
    config.providers.claude.enabled = true;
    config.providers.codex.enabled = true;
    config.providers.gemini.enabled = true;
    config.providers.opencode.enabled = true;
    config.providers.workbuddy.enabled = true;
    config.providers.antigravity.enabled = true;
    config.providers.cursor.enabled = true;
    config.providers.cursor_agent.enabled = true;
    config.providers.cline.enabled = true;
    config.providers.roo_code.enabled = true;
    config.providers.kilo_code.enabled = true;
    config.providers.github_copilot.enabled = true;
    config.providers.kimi_code.enabled = true;
    config.providers.qwen_code.enabled = true;
    config.providers.continue_dev.enabled = true;
    config.providers.aider.enabled = true;
    let catalog = descriptors(&config, &[]);
    let keys: std::collections::HashSet<_> = catalog.iter().map(|s| s.key).collect();
    assert_eq!(keys.len(), 24);
    assert_eq!(catalog.iter().filter(|d| d.configurable).count(), 16);
    for key in [
        "chatgpt_share",
        "claude_share",
        "gemini_share",
        "deepseek_share",
        "doubao_share",
        "kimi_share",
        "yuanbao_share",
        "qwen_share",
    ] {
        assert!(patch_provider_toml("", key, true, &[]).is_err());
    }
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
