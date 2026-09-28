//! Static service configuration checks. Legacy team runtime is intentionally retired.
use super::config::ConfigIssue;
use crate::ServiceConfig;

impl ServiceConfig {
    pub fn check_configuration_with(
        &self,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<(), ConfigIssue> {
        let issue = |field, code| ConfigIssue { field, code };
        if !self.database.is_absolute() {
            return Err(issue("database", "absolute_path_required"));
        }

        match self.mode.as_str() {
            "personal" => {
                self.validate()
                    .map_err(|_| issue("service", "invalid_personal_configuration"))?;
                crate::weknora::check_settings_with(&self.weknora, &lookup)?;
                let mut candidate = self.clone();
                candidate
                    .resolve_model_credentials_with(&lookup)
                    .map_err(|error| issue(error.field, error.code))?;
                candidate.resolve_siyuan_credentials_with(lookup)?;
            }
            "collector" => {
                if self.listen.port() == 0 {
                    return Err(issue("listen", "port_required"));
                }
                if !self.weknora.enabled {
                    return Err(issue("weknora.enabled", "weknora_required"));
                }
                if !self.weknora.dynamic_targets {
                    return Err(issue(
                        "weknora.dynamic_targets",
                        "dynamic_workspace_routing_required",
                    ));
                }
                if self.ai.enabled {
                    return Err(issue("ai.enabled", "unsupported_in_collector"));
                }
                if self.embedding.enabled {
                    return Err(issue("embedding.enabled", "unsupported_in_collector"));
                }
                let _token = self.collector.resolve_token_with(&lookup)?;
                crate::weknora::check_settings_with(&self.weknora, lookup)?;
            }
            _ => return Err(issue("mode", "unsupported_mode")),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    fn collector_config() -> ServiceConfig {
        let mut config = ServiceConfig::default();
        config.mode = "collector".into();
        config.database = std::env::temp_dir().join("aiks-collector-config-test.db");
        config.listen = "0.0.0.0:28082".parse::<SocketAddr>().unwrap();
        config.weknora.enabled = true;
        config.weknora.base_url = "https://weknora.example.test".into();
        config.weknora.dynamic_targets = true;
        config
    }

    fn lookup(name: &str) -> Option<String> {
        match name {
            "AIKS_COLLECTOR_TOKEN" => Some("a".repeat(64)),
            "AIKS_WEKNORA_API_KEY" => Some("synthetic-platform-key".into()),
            _ => None,
        }
    }

    #[test]
    fn collector_accepts_only_transport_and_weknora_configuration() {
        let config = collector_config();
        assert!(config.check_configuration_with(lookup).is_ok());
    }

    #[test]
    fn legacy_team_mode_is_no_longer_a_service_runtime_mode() {
        let mut config = collector_config();
        config.mode = "team".into();
        let error = config.check_configuration_with(lookup).unwrap_err();
        assert_eq!(error.field, "mode");
        assert_eq!(error.code, "unsupported_mode");
    }

    #[test]
    fn collector_rejects_local_model_capabilities() {
        let mut ai = collector_config();
        ai.ai.enabled = true;
        let error = ai.check_configuration_with(lookup).unwrap_err();
        assert_eq!(error.field, "ai.enabled");
        assert_eq!(error.code, "unsupported_in_collector");

        let mut embedding = collector_config();
        embedding.embedding.enabled = true;
        let error = embedding.check_configuration_with(lookup).unwrap_err();
        assert_eq!(error.field, "embedding.enabled");
        assert_eq!(error.code, "unsupported_in_collector");
    }
}
