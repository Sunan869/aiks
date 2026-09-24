//! Static checks and secret resolution, with no DB, socket or upstream request.
use super::{
    config::{validate_static, ConfigIssue},
    secrets::resolve_secret_with,
};
use crate::{config::allowed_ip, ServiceConfig};
use std::net::IpAddr;

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
            "personal" => self
                .validate()
                .map_err(|_| issue("service", "invalid_personal_configuration"))?,
            "team" => {
                let validated = validate_static(&self.team).map_err(|issues| issues[0])?;
                if !allowed_ip(self.listen.ip()) {
                    return Err(issue("listen", "loopback_required"));
                }
                let content = reqwest::Url::parse(&self.siyuan.base_url)
                    .map_err(|_| issue("siyuan.base_url", "invalid_internal_origin"))?;
                let internal_ip = content
                    .host_str()
                    .and_then(|host| host.trim_matches(['[', ']']).parse::<IpAddr>().ok());
                if content.scheme() != "http"
                    || internal_ip.is_none_or(|ip| !allowed_ip(ip))
                    || !content.username().is_empty()
                    || content.password().is_some()
                    || content.query().is_some()
                    || content.fragment().is_some()
                    || !matches!(content.path(), "" | "/")
                {
                    return Err(issue("siyuan.base_url", "internal_loopback_required"));
                }
                if self.ai.enabled
                    && (self.ai.base_url.trim().is_empty() || self.ai.model.trim().is_empty())
                {
                    return Err(issue("ai", "explicit_endpoint_and_model_required"));
                }
                if self.embedding.enabled
                    && (self.embedding.base_url.trim().is_empty()
                        || self.embedding.model.trim().is_empty())
                {
                    return Err(issue("embedding", "explicit_endpoint_and_model_required"));
                }
                // A successful static check does not enable team startup.
                let _secret = resolve_secret_with(validated.secret_source(), &lookup)?;
            }
            _ => return Err(issue("mode", "unsupported_mode")),
        }
        let mut candidate = self.clone();
        candidate
            .resolve_model_credentials_with(&lookup)
            .map_err(|error| issue(error.field, error.code))?;
        candidate.resolve_siyuan_credentials_with(lookup)?;
        Ok(())
    }
}
