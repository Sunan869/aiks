use aiks_service::ServiceConfig;

#[test]
fn partial_ai_configuration_never_inherits_a_legacy_endpoint_or_enables_itself() {
    let root = tempfile::tempdir().unwrap();
    let database = serde_json::to_string(&root.path().join("state.db")).unwrap();
    for ai in [
        "",
        "[ai]",
        "[ai]\ntimeout_seconds=3",
        "[ai]\nmodel='explicit-model'",
    ] {
        let config: ServiceConfig =
            toml::from_str(&format!("database={database}\n{ai}\n")).unwrap();
        assert!(!config.ai.enabled, "AI requires an explicit opt-in");
        assert!(
            config.ai.base_url.is_empty(),
            "no inherited private endpoint"
        );
        config.validate().unwrap();
    }
    let config = ServiceConfig::personal(root.path().join("other.db"));
    assert!(!config.ai.enabled);
    assert!(config.ai.base_url.is_empty());
}

#[test]
fn enabled_ai_requires_explicit_endpoint_and_model_before_starting_the_service() {
    let root = tempfile::tempdir().unwrap();
    let database = serde_json::to_string(&root.path().join("state.db")).unwrap();
    for extra in [
        "",
        "model='explicit-model'",
        "base_url='http://127.0.0.1:1/v1'",
    ] {
        let config: ServiceConfig = toml::from_str(&format!(
            "database={database}\n[ai]\nenabled=true\n{extra}\n"
        ))
        .unwrap();
        assert!(
            config.validate().is_err(),
            "partially configured AI must not call inherited defaults"
        );
    }
    let config: ServiceConfig = toml::from_str(&format!("database={database}\n[ai]\nenabled=true\nmodel='explicit-model'\nbase_url='http://127.0.0.1:1/v1'\n")).unwrap();
    config.validate().unwrap();
    assert!(config.ai.enabled);
}
