//! Read-only DingTalk adapter. Production origins are fixed and never follow redirects.
mod directory;
#[cfg(test)]
mod tests;
mod wire;

use crate::team::{config::ValidatedTeamSettings, secrets::SecretValue};
use aiks_core::team::{
    provider::ProviderFuture, DirectorySnapshot, DirectoryUser, ExternalLogin, IdentityProvider,
    TeamError,
};
use reqwest::Client;
use serde_json::{json, Value};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

struct CachedToken {
    value: Arc<String>,
    usable_until: Instant,
}

pub struct DingTalkClient {
    settings: ValidatedTeamSettings,
    secret: SecretValue,
    http: Client,
    api_origin: String,
    legacy_origin: String,
    app_token: Mutex<Option<CachedToken>>,
}

impl DingTalkClient {
    pub fn new(settings: ValidatedTeamSettings, secret: SecretValue) -> Result<Self, TeamError> {
        Self::build(
            settings,
            secret,
            "https://api.dingtalk.com",
            "https://oapi.dingtalk.com",
            Duration::from_secs(10),
        )
    }

    fn build(
        settings: ValidatedTeamSettings,
        secret: SecretValue,
        api_origin: &str,
        legacy_origin: &str,
        timeout: Duration,
    ) -> Result<Self, TeamError> {
        let http = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(timeout)
            .timeout(timeout)
            .build()
            .map_err(|_| TeamError::Unavailable)?;
        Ok(Self {
            settings,
            secret,
            http,
            api_origin: api_origin.into(),
            legacy_origin: legacy_origin.into(),
            app_token: Mutex::new(None),
        })
    }

    #[cfg(test)]
    fn with_test_origin(
        settings: ValidatedTeamSettings,
        secret: SecretValue,
        origin: &str,
        timeout: Duration,
    ) -> Result<Self, TeamError> {
        Self::build(settings, secret, origin, origin, timeout)
    }

    pub fn authorize_url(&self, state: &str) -> Result<String, TeamError> {
        if state.len() != 64 || !state.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(TeamError::InvalidInput);
        }
        let config = &self.settings.settings().dingtalk;
        let mut url = reqwest::Url::parse("https://login.dingtalk.com/oauth2/auth")
            .map_err(|_| TeamError::ConfigInvalid)?;
        url.query_pairs_mut()
            .append_pair("client_id", &config.client_id)
            .append_pair("redirect_uri", &config.redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", "openid corpid")
            .append_pair("prompt", "consent")
            .append_pair("state", state);
        Ok(url.into())
    }

    pub async fn exchange_code(&self, code: &str) -> Result<ExternalLogin, TeamError> {
        if !bounded_text(code, 4096) {
            return Err(TeamError::InvalidInput);
        }
        tokio::time::timeout(Duration::from_secs(45), self.exchange_inner(code))
            .await
            .map_err(|_| TeamError::Unavailable)?
    }

    async fn exchange_inner(&self, code: &str) -> Result<ExternalLogin, TeamError> {
        let config = &self.settings.settings().dingtalk;
        let response = self.execute(self.http.post(format!("{}/v1.0/oauth2/userAccessToken", self.api_origin))
            .json(&json!({"clientId":config.client_id,"clientSecret":self.secret.expose(),"code":code,"grantType":"authorization_code"}))).await?;
        if text(&response, "corpId", 256)? != config.corp_id {
            return Err(TeamError::Unauthorized);
        }
        let token = token_text(&response, "accessToken")?;
        let identity = self
            .execute(
                self.http
                    .get(format!("{}/v1.0/contact/users/me", self.api_origin))
                    .header("x-acs-dingtalk-access-token", token),
            )
            .await?;
        let union = text(&identity, "unionId", 512)?;
        let app_token = self.application_token().await?;
        let mapped = self
            .legacy(
                "/topapi/user/getbyunionid",
                app_token.as_str(),
                json!({"unionid":union}),
            )
            .await?;
        // External contacts and missing relationship fields are not employees.
        if mapped.get("contact_type").and_then(Value::as_i64) != Some(0) {
            return Err(TeamError::Unauthorized);
        }
        let user_id = text(&mapped, "userid", 512)?;
        let member = self.read_member(app_token.as_str(), &user_id).await?;
        if !member.active || member.union_id != union {
            return Err(TeamError::Unauthorized);
        }
        Ok(ExternalLogin {
            corp_id: config.corp_id.clone(),
            union_id: union,
            external_user_id: user_id,
            display_name: member.display_name,
        })
    }

    async fn application_token(&self) -> Result<Arc<String>, TeamError> {
        // This lock coalesces upstream refresh only; it is never a database lock.
        let mut cached = self.app_token.lock().await;
        if let Some(token) = cached.as_ref().filter(|t| t.usable_until > Instant::now()) {
            return Ok(token.value.clone());
        }
        let config = &self.settings.settings().dingtalk;
        let response = self.execute(self.http.post(format!("{}/v1.0/oauth2/{}/token", self.api_origin, config.corp_id))
            .json(&json!({"client_id":config.client_id,"client_secret":self.secret.expose(),"grant_type":"client_credentials"}))).await?;
        let value = Arc::new(token_text(&response, "access_token")?);
        let ttl = response
            .get("expires_in")
            .and_then(Value::as_u64)
            .filter(|n| *n > 0 && *n <= 86400)
            .ok_or(TeamError::Unavailable)?;
        *cached = Some(CachedToken {
            value: value.clone(),
            usable_until: Instant::now() + Duration::from_secs(ttl.saturating_sub(60)),
        });
        Ok(value)
    }

    async fn read_member(&self, token: &str, id: &str) -> Result<DirectoryUser, TeamError> {
        let response = self
            .legacy(
                "/topapi/v2/user/get",
                token,
                json!({"userid":id,"language":"zh_CN"}),
            )
            .await?;
        let member = member(&response)?;
        if member.external_user_id != id {
            return Err(TeamError::Unauthorized);
        }
        Ok(member)
    }
}

impl IdentityProvider for DingTalkClient {
    fn exchange_code<'a>(&'a self, code: &'a str) -> ProviderFuture<'a, ExternalLogin> {
        Box::pin(DingTalkClient::exchange_code(self, code))
    }
    fn directory<'a>(&'a self, scope: &'a [String]) -> ProviderFuture<'a, DirectorySnapshot> {
        Box::pin(DingTalkClient::directory(self, scope))
    }
}

fn bounded_text(value: &str, budget: usize) -> bool {
    !value.is_empty()
        && value.len() <= budget
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
fn text(value: &Value, field: &str, budget: usize) -> Result<String, TeamError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|v| bounded_text(v, budget))
        .map(str::to_owned)
        .ok_or(TeamError::Unavailable)
}
fn token_text(value: &Value, field: &str) -> Result<String, TeamError> {
    let token = text(value, field, 8192)?;
    if !token.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
        return Err(TeamError::Unavailable);
    }
    Ok(token)
}
fn member(value: &Value) -> Result<DirectoryUser, TeamError> {
    Ok(DirectoryUser {
        external_user_id: text(value, "userid", 512)?,
        union_id: text(value, "unionid", 512)?,
        display_name: text(value, "name", 4096)?,
        active: value
            .get("active")
            .and_then(Value::as_bool)
            .ok_or(TeamError::Unavailable)?,
    })
}
