use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::PathBuf,
};

use aiks_core::{
    ai::config::AiModelConfig, config::SiYuanConfig, pipeline::EmbeddingConfig,
    service::ServiceRuntimeConfig,
};
use serde::Deserialize;

use crate::model_credentials::ModelCredentials;

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServiceConfig {
    pub database: PathBuf,
    pub mode: String,
    pub listen: SocketAddr,
    #[serde(deserialize_with = "deserialize_service_ai")]
    pub ai: AiModelConfig,
    pub embedding: EmbeddingConfig,
    pub siyuan: SiYuanConfig,
    pub model_credentials: ModelCredentials,
    pub team: crate::team::config::TeamSettings,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            database: PathBuf::new(),
            mode: "personal".into(),
            listen: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            ai: service_ai_default(),
            embedding: EmbeddingConfig {
                enabled: false,
                ..Default::default()
            },
            siyuan: SiYuanConfig::default(),
            model_credentials: ModelCredentials::default(),
            team: crate::team::config::TeamSettings::default(),
        }
    }
}

impl ServiceConfig {
    pub fn personal(database: PathBuf) -> Self {
        Self {
            database,
            ..Default::default()
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.mode == "personal" && allowed_ip(self.listen.ip()),
            "S1 supports numeric loopback personal mode only"
        );
        anyhow::ensure!(
            self.database.is_absolute(),
            "An absolute database path is required"
        );
        anyhow::ensure!(
            !self.ai.enabled
                || (!self.ai.base_url.trim().is_empty() && !self.ai.model.trim().is_empty()),
            "Enabled AI requires an explicit endpoint and model"
        );
        Ok(())
    }

    pub fn runtime_config(&self) -> ServiceRuntimeConfig {
        ServiceRuntimeConfig {
            database: self.database.clone(),
            ai: self.ai.clone(),
            embedding: self.embedding.clone(),
            siyuan: self.siyuan.clone(),
        }
    }
}

fn service_ai_default() -> AiModelConfig {
    AiModelConfig {
        enabled: false,
        base_url: String::new(),
        model: String::new(),
        ..Default::default()
    }
}

fn deserialize_service_ai<'de, D>(deserializer: D) -> Result<AiModelConfig, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // Reuse the Core parameter types, but a partial service [ai] table must
    // never inherit the old Desktop's enabled flag or private deployment URL.
    let mut values = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
    values
        .entry("enabled".to_owned())
        .or_insert(serde_json::Value::Bool(false));
    for key in ["base_url", "model"] {
        values
            .entry(key.to_owned())
            .or_insert(serde_json::Value::String(String::new()));
    }
    serde_json::from_value(serde_json::Value::Object(values)).map_err(serde::de::Error::custom)
}

pub(crate) fn allowed_ip(ip: IpAddr) -> bool {
    ip == IpAddr::V4(Ipv4Addr::LOCALHOST) || ip == IpAddr::V6(Ipv6Addr::LOCALHOST)
}
