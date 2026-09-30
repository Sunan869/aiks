use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendMode {
    #[default]
    Legacy,
    ServiceLocal,
    ServiceRemote,
}
impl BackendMode {
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "legacy" => Ok(Self::Legacy),
            "service_local" => Ok(Self::ServiceLocal),
            "service_remote" => Ok(Self::ServiceRemote),
            _ => anyhow::bail!("backend.mode must be legacy, service_local or service_remote"),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BackendConfig {
    pub mode: BackendMode,
    pub collector_url: String,
    pub collector_token_env: String,
    /// Allow plain HTTP for remote collector/WeKnora during explicit development or LAN testing.
    /// Production keeps this false so non-loopback remote endpoints require HTTPS.
    pub allow_insecure_http: bool,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            mode: BackendMode::Legacy,
            collector_url: String::new(),
            collector_token_env: "AIKS_COLLECTOR_TOKEN".into(),
            allow_insecure_http: false,
        }
    }
}
