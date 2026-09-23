use aiks_service::team::config::{validate_static, TeamSettings};

fn configured() -> TeamSettings {
    toml::from_str(
        r#"
        enabled = true
        public_base_url = "https://aiks.example.com"
        [dingtalk]
        enabled = true
        corp_id = "synthetic-corp"
        client_id = "synthetic-client"
        redirect_uri = "https://aiks.example.com/api/v1/auth/dingtalk/callback"
        client_secret_env = "AIKS_DINGTALK_CLIENT_SECRET"
        [directory]
        root_department_ids = ["1"]
        "#,
    )
    .unwrap()
}

#[test]
fn team_configuration_requires_explicit_enablement() {
    let settings = TeamSettings::default();
    assert!(!settings.enabled);
    assert!(!settings.dingtalk.enabled);
    assert_eq!(
        validate_static(&settings).unwrap_err()[0].code,
        "team_disabled"
    );
    assert!(validate_static(&configured()).is_ok());
}

#[test]
fn identity_scope_callback_and_origin_must_be_explicit_and_consistent() {
    for origin in [
        "http://aiks.example.com",
        "https://user@aiks.example.com",
        "https://aiks.example.com/prefix",
        "https://aiks.example.com?token=anything",
        "https://aiks.example.com/#fragment",
        "https://aiks.example.com/../",
        " https://aiks.example.com",
        "https://aiks.example.com\\path",
    ] {
        let mut settings = configured();
        settings.public_base_url = origin.into();
        assert!(validate_static(&settings).is_err(), "{origin}");
    }
    for callback in [
        "https://other.example.com/api/v1/auth/dingtalk/callback",
        "https://aiks.example.com/api/v1/auth/dingtalk/callback?next=elsewhere",
        "https://aiks.example.com/elsewhere",
        "https://aiks.example.com/api/v1/auth/dingtalk/%63allback",
    ] {
        let mut settings = configured();
        settings.dingtalk.redirect_uri = callback.into();
        assert!(validate_static(&settings)
            .unwrap_err()
            .iter()
            .any(|e| e.code == "callback_mismatch"));
    }
    let mut settings = configured();
    settings.dingtalk.corp_id.clear();
    settings.dingtalk.client_id = " invalid ".into();
    settings.directory.root_department_ids.clear();
    let issues = validate_static(&settings).unwrap_err();
    assert!(issues.iter().any(|e| e.code == "invalid_identity"));
    assert!(issues.iter().any(|e| e.code == "directory_scope_required"));
}

#[test]
fn configuration_does_not_silently_choose_between_secret_sources() {
    let mut settings = configured();
    settings.dingtalk.client_secret_file = "/run/secrets/dingtalk".into();
    assert!(validate_static(&settings)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "ambiguous_secret_source"));
    settings.dingtalk.client_secret_file.clear();
    settings.dingtalk.client_secret_env.clear();
    assert!(validate_static(&settings)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "secret_source_missing"));
}

#[test]
fn expiry_ranges_and_unknown_keys_cannot_weaken_defaults() {
    for (refresh, stale) in [(59, 900), (901, 901), (300, 299), (300, 3601)] {
        let mut settings = configured();
        settings.directory.refresh_interval_seconds = refresh;
        settings.directory.max_stale_seconds = stale;
        assert!(validate_static(&settings).is_err());
    }
    for ttl in [0, 59, 601, u64::MAX] {
        let mut settings = configured();
        settings.sessions.login_attempt_ttl_seconds = ttl;
        assert!(validate_static(&settings).is_err());
    }
    for text in [
        "enabeld = true",
        "[dingtalk]\nclient_secret = 'not-an-accepted-field'",
        "[directory]\nroots = ['1']",
        "[sessions]\naccess_ttl = 1",
    ] {
        assert!(toml::from_str::<TeamSettings>(text).is_err());
    }
}
