use std::process::Command;

use aiks_service::{
    team::secrets::{resolve_secret_with, SecretSource},
    ServiceConfig,
};

#[test]
fn server_secret_resolution_is_injected_bounded_and_redacted() {
    let source = SecretSource::Environment("AIKS_TEST_SECRET".into());
    let value = resolve_secret_with(&source, |name| {
        assert_eq!(name, "AIKS_TEST_SECRET");
        Some("SYNTHETIC_PRIVATE_VALUE".into())
    })
    .unwrap();
    assert_eq!(value.expose(), "SYNTHETIC_PRIVATE_VALUE");
    assert_eq!(format!("{value:?}"), "[REDACTED]");
    assert_eq!(
        resolve_secret_with(&source, |_| None).unwrap_err().code,
        "secret_missing"
    );
    for invalid in [
        String::new(),
        "a\nb".into(),
        "a\0b".into(),
        " leading".into(),
        "trailing ".into(),
        "x".repeat(8193),
    ] {
        let issue = resolve_secret_with(&source, |_| Some(invalid.clone())).unwrap_err();
        assert_eq!(issue.code, "secret_invalid");
    }
    let invalid = SecretSource::Environment("1invalid".into());
    assert!(resolve_secret_with(&invalid, |_| panic!(
        "invalid reference must not be looked up"
    ))
    .is_err());
    let missing = SecretSource::File(std::env::temp_dir().join("not-created-secret"));
    assert_eq!(
        resolve_secret_with(&missing, |_| panic!("file is not env"))
            .unwrap_err()
            .code,
        "secret_missing"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("secret");
        std::fs::write(&path, "SYNTHETIC_FILE_SECRET").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let file = SecretSource::File(path.clone());
        assert_eq!(
            resolve_secret_with(&file, |_| panic!("file is not env"))
                .unwrap()
                .expose(),
            "SYNTHETIC_FILE_SECRET"
        );
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            resolve_secret_with(&file, |_| panic!("file is not env"))
                .unwrap_err()
                .code,
            "secret_file_permissions"
        );
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(root.path().join("target"), &path).unwrap();
        std::fs::write(root.path().join("target"), "SYNTHETIC_FILE_SECRET").unwrap();
        assert_eq!(
            resolve_secret_with(&file, |_| panic!("file is not env"))
                .unwrap_err()
                .code,
            "secret_file_unsafe"
        );
    }
}

#[test]
fn personal_check_ignores_dingtalk_and_does_not_fall_back_from_team() {
    let mut config = ServiceConfig::personal(std::env::temp_dir().join("not-created-business.db"));
    config
        .check_configuration_with(|_| panic!("personal mode needs no DingTalk credentials"))
        .unwrap();
    config.mode = "team".into();
    assert!(config
        .check_configuration_with(|_| panic!("disabled team must not look up secrets"))
        .is_err());
    config.mode = "unknown".into();
    assert!(config.check_configuration_with(|_| None).is_err());
}

#[test]
fn team_siyuan_secret_is_server_owned_and_internal_origin_is_loopback_only() {
    let database = std::env::temp_dir().join("team-siyuan-not-created.db");
    let mut config: ServiceConfig = toml::from_str(&format!(
        r#"
mode = "team"
database = {}
listen = "127.0.0.1:28081"
[team]
enabled = true
public_base_url = "https://aiks.example.com"
[team.dingtalk]
enabled = true
corp_id = "synthetic-corp"
client_id = "synthetic-client"
redirect_uri = "https://aiks.example.com/api/v1/auth/dingtalk/callback"
client_secret_env = "AIKS_DINGTALK_CLIENT_SECRET"
[team.directory]
root_department_ids = ["1"]
[siyuan]
base_url = "http://127.0.0.1:6806"
token_env = "AIKS_SIYUAN_TOKEN"
"#,
        serde_json::to_string(&database).unwrap()
    ))
    .unwrap();
    config
        .check_configuration_with(|name| match name {
            "AIKS_DINGTALK_CLIENT_SECRET" => Some("SYNTHETIC_DINGTALK".into()),
            "AIKS_SIYUAN_TOKEN" => Some("SYNTHETIC_SIYUAN".into()),
            _ => None,
        })
        .unwrap();
    config
        .resolve_siyuan_credentials_with(|name| {
            (name == "AIKS_SIYUAN_TOKEN").then(|| "SYNTHETIC_SIYUAN".into())
        })
        .unwrap();
    assert_eq!(config.runtime_config().siyuan.token, "SYNTHETIC_SIYUAN");

    let mut inline = config.clone();
    inline.siyuan.token_env.clear();
    inline.siyuan.token = "INLINE_FORBIDDEN".into();
    assert_eq!(
        inline
            .check_configuration_with(|name| {
                (name == "AIKS_DINGTALK_CLIENT_SECRET").then(|| "SYNTHETIC_DINGTALK".into())
            })
            .unwrap_err()
            .code,
        "inline_secret_forbidden"
    );

    let mut remote = config;
    remote.siyuan.token.clear();
    remote.siyuan.base_url = "https://siyuan.example.com".into();
    assert_eq!(
        remote
            .check_configuration_with(|name| match name {
                "AIKS_DINGTALK_CLIENT_SECRET" => Some("SYNTHETIC_DINGTALK".into()),
                "AIKS_SIYUAN_TOKEN" => Some("SYNTHETIC_SIYUAN".into()),
                _ => None,
            })
            .unwrap_err()
            .code,
        "internal_loopback_required"
    );
}

#[test]
fn config_check_neither_requires_bootstrap_nor_opens_database_or_port() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("untouched.db");
    let path = root.path().join("service.toml");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let text = format!(
        r#"
mode = "team"
database = {}
listen = "{}"
[team]
enabled = true
public_base_url = "https://aiks.example.com"
[team.dingtalk]
enabled = true
corp_id = "synthetic-corp"
client_id = "synthetic-client"
redirect_uri = "https://aiks.example.com/api/v1/auth/dingtalk/callback"
client_secret_env = "AIKS_TEST_CHECK_SECRET"
[team.directory]
root_department_ids = ["1"]
"#,
        serde_json::to_string(&database).unwrap(),
        listener.local_addr().unwrap()
    );
    std::fs::write(&path, &text).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_aiks-service"))
        .args(["--check-config", "--config"])
        .arg(&path)
        .env("AIKS_TEST_CHECK_SECRET", "SYNTHETIC_CHECK_VALUE")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("configuration_valid"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("SYNTHETIC_CHECK_VALUE"));
    assert!(!database.exists());
    let output = Command::new(env!("CARGO_BIN_EXE_aiks-service"))
        .args(["--config"])
        .arg(&path)
        .arg("--check-config")
        .env_remove("AIKS_TEST_CHECK_SECRET")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("secret_missing"));
    assert!(error.contains("team.dingtalk.client_secret"));
    assert!(!database.exists());
    std::fs::write(
        &path,
        format!("database={}\n", serde_json::to_string(&database).unwrap()),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_aiks-service"))
        .args(["--config"])
        .arg(&path)
        .arg("--check-config")
        .env_remove("AIKS_TEST_CHECK_SECRET")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!database.exists());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}
