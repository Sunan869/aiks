//! Server-owned model credentials; no real secrets or process-global env edits.
use aiks_service::ServiceConfig;
use std::process::{Command, Stdio};

fn configured() -> ServiceConfig {
    let text = r#"
[ai]
enabled = true
base_url = "http://127.0.0.1:1/v1"
model = "synthetic-text-model"
[embedding]
enabled = true
base_url = "http://127.0.0.1:2/v1"
model = "synthetic-vector-model"
dimensions = 3
[model_credentials]
ai_api_key_env = "AIKS_AI_API_KEY"
embedding_api_key_env = "AIKS_EMBEDDING_API_KEY"
"#;
    toml::from_str(text).expect("server credential references should be supported")
}

#[test]
fn ai_and_embedding_resolve_independent_server_credentials() {
    let mut config = configured();
    config
        .resolve_model_credentials_with(|name| match name {
            "AIKS_AI_API_KEY" => Some("synthetic-text-secret".into()),
            "AIKS_EMBEDDING_API_KEY" => Some("synthetic-vector-secret".into()),
            _ => None,
        })
        .unwrap();
    let runtime = config.runtime_config();
    assert_eq!(runtime.ai.api_key.as_deref(), Some("synthetic-text-secret"));
    assert_eq!(
        runtime.embedding.api_key.as_deref(),
        Some("synthetic-vector-secret")
    );
    assert_eq!(runtime.ai.model, "synthetic-text-model");
    assert_eq!(runtime.embedding.dimensions, Some(3));
}

#[test]
fn disabled_models_do_not_read_environment_or_inherit_private_defaults() {
    let mut config: ServiceConfig = toml::from_str(
        "[model_credentials]\nai_api_key_env='AIKS_AI_API_KEY'\nembedding_api_key_env='AIKS_EMBEDDING_API_KEY'\n",
    )
    .unwrap();
    config
        .resolve_model_credentials_with(|_| panic!("disabled models must not read secrets"))
        .unwrap();
    assert!(!config.ai.enabled);
    assert!(!config.embedding.enabled);
    assert!(config.ai.base_url.is_empty());
    assert!(config.ai.model.is_empty());
    assert!(config.ai.api_key.is_none());
    assert!(config.embedding.api_key.is_none());
}

#[test]
fn failed_resolution_is_atomic_and_errors_never_echo_secret_values() {
    let mut config = configured();
    let error = config
        .resolve_model_credentials_with(|name| {
            (name == "AIKS_AI_API_KEY").then(|| "synthetic-do-not-echo".to_owned())
        })
        .unwrap_err();
    assert_eq!(error.field, "model_credentials.embedding_api_key_env");
    assert_eq!(error.code, "secret_missing");
    assert!(!format!("{error:?} {error}").contains("synthetic-do-not-echo"));
    assert!(config.ai.api_key.is_none());
    assert!(config.embedding.api_key.is_none());
}

#[test]
fn ambiguous_inline_and_environment_credentials_are_rejected() {
    let mut config = configured();
    config.ai.api_key = Some("synthetic-inline-secret".into());
    let error = config
        .resolve_model_credentials_with(|_| panic!("ambiguity must be checked before lookup"))
        .unwrap_err();
    assert_eq!(error.code, "ambiguous_secret_source");
    assert!(!error.to_string().contains("synthetic-inline-secret"));
}

#[test]
fn secret_value_and_reference_budgets_are_enforced() {
    for value in [
        String::new(),
        " value".into(),
        "value ".into(),
        "value\r\n".into(),
        "value\0other".into(),
        "value\tother".into(),
        "value other".into(),
        "x".repeat(8193),
    ] {
        let mut config = configured();
        config.embedding.enabled = false;
        let error = config
            .resolve_model_credentials_with(|_| Some(value.clone()))
            .unwrap_err();
        assert_eq!(error.code, "secret_invalid");
        assert_eq!(error.field, "model_credentials.ai_api_key_env");
    }
    for name in ["9BAD", "BAD-NAME", " NAME", "KEY=VALUE", "KEY\nVALUE"] {
        let mut config = configured();
        config.model_credentials.ai_api_key_env = name.into();
        let error = config
            .resolve_model_credentials_with(|_| panic!("invalid names must not be looked up"))
            .unwrap_err();
        assert_eq!(error.code, "invalid_environment_reference");
    }
}

#[test]
fn legacy_personal_model_configuration_is_not_replaced() {
    let mut config: ServiceConfig = toml::from_str(
        "[ai]\nenabled=true\nbase_url='http://127.0.0.1:1/v1'\nmodel='personal'\napi_key='synthetic-existing-key'\n",
    )
    .unwrap();
    config
        .resolve_model_credentials_with(|_| panic!("no server reference was configured"))
        .unwrap();
    assert_eq!(config.ai.api_key.as_deref(), Some("synthetic-existing-key"));
    assert!(!config.embedding.enabled);
}

#[test]
fn local_models_without_authentication_need_no_api_key() {
    let mut config: ServiceConfig = toml::from_str(
        "[ai]\nenabled=true\nbase_url='http://127.0.0.1:1/v1'\nmodel='local-text'\n[embedding]\nenabled=true\nbase_url='http://127.0.0.1:2/v1'\nmodel='local-vector'\ndimensions=3\n",
    )
    .unwrap();
    config
        .resolve_model_credentials_with(|_| panic!("local no-auth models need no secret lookup"))
        .unwrap();
    assert!(config.ai.api_key.is_none());
    assert!(config.embedding.api_key.is_none());
}

#[test]
fn unknown_credential_fields_fail_instead_of_silently_ignoring_a_typo() {
    assert!(toml::from_str::<ServiceConfig>("[model_credentials]\napi_secret='value'\n").is_err());
}

#[test]
fn missing_server_key_prevents_business_database_creation() {
    let root = tempfile::tempdir().unwrap();
    let db = root.path().join("not-created.db");
    let path = root.path().join("service.toml");
    std::fs::write(
        &path,
        format!(
            "database={}\n[ai]\nenabled=true\nbase_url='http://127.0.0.1:1/v1'\nmodel='synthetic'\n[model_credentials]\nai_api_key_env='AIKS_SYNTHETIC_MISSING_KEY'\n",
            serde_json::to_string(&db).unwrap()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_aiks-service"))
        .arg("--config")
        .arg(path)
        .arg("--bootstrap-stdin")
        .env_remove("AIKS_SYNTHETIC_MISSING_KEY")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!db.exists());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("model_credentials.ai_api_key_env: secret_missing"));
}
