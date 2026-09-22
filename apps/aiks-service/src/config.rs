use aiks_core::{
    ai::config::AiModelConfig, config::SiYuanConfig, pipeline::EmbeddingConfig,
    service::ServiceRuntimeConfig,
};
use serde::Deserialize;
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::PathBuf,
};

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServiceConfig {
    pub database: PathBuf,
    pub mode: String,
    pub listen: SocketAddr,
    pub ai: AiModelConfig,
    pub embedding: EmbeddingConfig,
    pub siyuan: SiYuanConfig,
}
impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            database: PathBuf::new(),
            mode: "personal".into(),
            listen: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            ai: AiModelConfig {
                enabled: false,
                ..Default::default()
            },
            embedding: EmbeddingConfig {
                enabled: false,
                ..Default::default()
            },
            siyuan: SiYuanConfig::default(),
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
pub(crate) fn allowed_ip(ip: IpAddr) -> bool {
    ip == IpAddr::V4(Ipv4Addr::LOCALHOST) || ip == IpAddr::V6(Ipv6Addr::LOCALHOST)
}
