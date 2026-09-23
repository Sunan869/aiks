//! Pure, bounded validation. This module never reads secrets, files or networks.
use std::{collections::HashSet, fmt, path::PathBuf};

use reqwest::Url;
use serde::Deserialize;

pub const CALLBACK_PATH: &str = "/api/v1/auth/dingtalk/callback";

#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TeamSettings {
    pub enabled: bool,
    pub public_base_url: String,
    pub dingtalk: DingTalkSettings,
    pub directory: DirectorySettings,
    pub sessions: SessionSettings,
}

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DingTalkSettings {
    pub enabled: bool,
    pub corp_id: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub client_secret_env: String,
    pub client_secret_file: String,
}
impl Default for DingTalkSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            corp_id: String::new(),
            client_id: String::new(),
            redirect_uri: String::new(),
            client_secret_env: "AIKS_DINGTALK_CLIENT_SECRET".into(),
            client_secret_file: String::new(),
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DirectorySettings {
    pub root_department_ids: Vec<String>,
    pub refresh_interval_seconds: u64,
    pub max_stale_seconds: u64,
}
impl Default for DirectorySettings {
    fn default() -> Self {
        Self {
            root_department_ids: Vec::new(),
            refresh_interval_seconds: 300,
            max_stale_seconds: 900,
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SessionSettings {
    pub login_attempt_ttl_seconds: u64,
    pub access_token_ttl_seconds: u64,
    pub refresh_token_ttl_seconds: u64,
}
impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            login_attempt_ttl_seconds: 300,
            access_token_ttl_seconds: 900,
            refresh_token_ttl_seconds: 604800,
        }
    }
}

#[derive(Clone)]
pub enum SecretSource {
    Environment(String),
    File(PathBuf),
}

#[derive(Clone)]
pub struct ValidatedTeamSettings {
    settings: TeamSettings,
    secret_source: SecretSource,
}
impl ValidatedTeamSettings {
    pub fn settings(&self) -> &TeamSettings {
        &self.settings
    }
    pub fn secret_source(&self) -> &SecretSource {
        &self.secret_source
    }
}
impl fmt::Debug for ValidatedTeamSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ValidatedTeamSettings { configuration: [REDACTED] }")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfigIssue {
    pub field: &'static str,
    pub code: &'static str,
}
impl fmt::Display for ConfigIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.code)
    }
}
impl std::error::Error for ConfigIssue {}

pub fn valid_environment_name(name: &str) -> bool {
    name.len() <= 128
        && name
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

pub fn validate_static(settings: &TeamSettings) -> Result<ValidatedTeamSettings, Vec<ConfigIssue>> {
    if !settings.enabled {
        return Err(vec![ConfigIssue {
            field: "team.enabled",
            code: "team_disabled",
        }]);
    }
    let mut issues = Vec::new();
    let mut issue = |field, code| issues.push(ConfigIssue { field, code });
    if !settings.dingtalk.enabled {
        issue("team.dingtalk.enabled", "dingtalk_disabled");
    }
    let base = &settings.public_base_url;
    let parsed = Url::parse(base).ok();
    if !base.starts_with("https://") {
        issue("team.public_base_url", "https_required");
    }
    let valid_origin = base.len() <= 2048
        && parsed.as_ref().is_some_and(|url| {
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && url.path() == "/"
                && url.origin().ascii_serialization() == *base
        });
    if !valid_origin {
        issue("team.public_base_url", "invalid_origin");
    }
    if !valid_origin || settings.dingtalk.redirect_uri != format!("{base}{CALLBACK_PATH}") {
        issue("team.dingtalk.redirect_uri", "callback_mismatch");
    }
    if !identity(&settings.dingtalk.corp_id) {
        issue("team.dingtalk.corp_id", "invalid_identity");
    }
    if !identity(&settings.dingtalk.client_id) {
        issue("team.dingtalk.client_id", "invalid_identity");
    }
    let environment = &settings.dingtalk.client_secret_env;
    let file = &settings.dingtalk.client_secret_file;
    let source = match (environment.is_empty(), file.is_empty()) {
        (false, false) => {
            issue("team.dingtalk.client_secret", "ambiguous_secret_source");
            None
        }
        (true, true) => {
            issue("team.dingtalk.client_secret", "secret_source_missing");
            None
        }
        (false, true) => {
            if !valid_environment_name(environment) {
                issue(
                    "team.dingtalk.client_secret_env",
                    "invalid_environment_reference",
                );
            }
            Some(SecretSource::Environment(environment.clone()))
        }
        (true, false) => {
            let path = PathBuf::from(file);
            if file.len() > 4096
                || file.trim() != file
                || file.chars().any(char::is_control)
                || !path.is_absolute()
            {
                issue("team.dingtalk.client_secret_file", "invalid_secret_path");
            }
            Some(SecretSource::File(path))
        }
    };
    let directory = &settings.directory;
    if directory.root_department_ids.is_empty() {
        issue(
            "team.directory.root_department_ids",
            "directory_scope_required",
        );
    } else {
        let mut seen = HashSet::new();
        if directory.root_department_ids.len() > 100
            || directory.root_department_ids.iter().any(|id| {
                !id.parse::<i64>()
                    .is_ok_and(|n| n > 0 && n.to_string() == *id)
                    || !seen.insert(id)
            })
        {
            issue(
                "team.directory.root_department_ids",
                "directory_scope_invalid",
            );
        }
    }
    if !(60..=900).contains(&directory.refresh_interval_seconds) {
        issue(
            "team.directory.refresh_interval_seconds",
            "invalid_duration",
        );
    }
    if directory.max_stale_seconds < directory.refresh_interval_seconds
        || directory.max_stale_seconds > 3600
    {
        issue("team.directory.max_stale_seconds", "invalid_duration");
    }
    for (field, valid) in [
        (
            "team.sessions.login_attempt_ttl_seconds",
            (60..=600).contains(&settings.sessions.login_attempt_ttl_seconds),
        ),
        (
            "team.sessions.access_token_ttl_seconds",
            (60..=3600).contains(&settings.sessions.access_token_ttl_seconds),
        ),
        (
            "team.sessions.refresh_token_ttl_seconds",
            (3600..=2592000).contains(&settings.sessions.refresh_token_ttl_seconds),
        ),
    ] {
        if !valid {
            issue(field, "invalid_duration");
        }
    }
    if !issues.is_empty() {
        return Err(issues);
    }
    let Some(secret_source) = source else {
        return Err(vec![ConfigIssue {
            field: "team.dingtalk.client_secret",
            code: "secret_source_missing",
        }]);
    };
    Ok(ValidatedTeamSettings {
        settings: settings.clone(),
        secret_source,
    })
}
