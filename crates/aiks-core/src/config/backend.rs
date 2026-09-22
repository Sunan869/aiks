use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendMode {
    #[default]
    Legacy,
    ServiceLocal,
}
impl BackendMode {
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "legacy" => Ok(Self::Legacy),
            "service_local" => Ok(Self::ServiceLocal),
            _ => anyhow::bail!("backend.mode must be legacy or service_local"),
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BackendConfig {
    pub mode: BackendMode,
}
