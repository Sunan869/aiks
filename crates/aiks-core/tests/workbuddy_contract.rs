use std::path::PathBuf;

use aiks_core::{config::Config, model::SourceKind};

// RED contract for the WorkBuddy provider integration.
#[test]
fn workbuddy_source_kind_has_stable_identity() {
    assert_eq!(SourceKind::WorkBuddy.as_str(), "workbuddy");
    assert_eq!(SourceKind::WorkBuddy.display_name(), "WorkBuddy");
    assert_eq!(SourceKind::from_str("workbuddy"), Some(SourceKind::WorkBuddy));
    assert_eq!(SourceKind::from_str("work_buddy"), Some(SourceKind::WorkBuddy));
}

#[test]
fn workbuddy_provider_config_defaults_enabled_and_accepts_path_override() {
    let config: Config = toml::from_str(
        r#"
[providers.workbuddy]
enabled = true
path = "C:/Users/test/.workbuddy"
"#,
    )
    .unwrap();

    assert!(config.providers.workbuddy.enabled);
    assert_eq!(
        config.workbuddy_path(),
        Some(PathBuf::from("C:/Users/test/.workbuddy"))
    );
}
