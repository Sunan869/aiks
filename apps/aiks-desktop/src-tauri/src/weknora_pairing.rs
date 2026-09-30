//! Native-only WeKnora browser pairing for service_remote Desktop mode.
//! The scoped WeKnora API key never crosses into WebView state.
use crate::service_client::valid_id;
use rand::RngCore;
use reqwest::{Client, Response, StatusCode, Url};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{net::IpAddr, time::Duration};

const MAX_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Clone, Deserialize)]
pub(crate) struct DesktopBootstrap {
    pub tenant_id: u64,
    pub knowledge_base_id: String,
    pub knowledge_base: String,
    pub api_key: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct PendingPairing {
    attempt_id: String,
    verifier: String,
}

pub(crate) enum PairingExchange {
    Pending,
    Connected(DesktopBootstrap),
}

pub(crate) fn validate_origin(value: &str, allow_insecure_http: bool) -> Result<Url, String> {
    let url = Url::parse(value.trim()).map_err(|_| "weknora_web_origin_invalid".to_string())?;
    let loopback_http = url.scheme() == "http"
        && url
            .host_str()
            .and_then(|host| host.trim_matches(['[', ']']).parse::<IpAddr>().ok())
            .is_some_and(|ip| ip.is_loopback());
    let explicitly_allowed_http = allow_insecure_http && url.scheme() == "http";
    if !(url.scheme() == "https" || loopback_http || explicitly_allowed_http)
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err("weknora_web_origin_invalid".into());
    }
    Ok(url)
}

fn client() -> Result<Client, String> {
    Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| "weknora_login_unavailable".to_string())
}

fn valid_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn valid_secret(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 8192
        && value.bytes().all(|b| (0x21..=0x7e).contains(&b))
}

async fn bounded_json<T: DeserializeOwned>(mut response: Response) -> Result<T, String> {
    if response
        .content_length()
        .is_some_and(|len| len > MAX_RESPONSE_BYTES as u64)
    {
        return Err("weknora_login_response_too_large".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "weknora_login_unavailable".to_string())?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err("weknora_login_response_too_large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "weknora_login_response_invalid".to_string())
}

fn response_error(status: StatusCode) -> String {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::GONE => "weknora_login_expired".into(),
        StatusCode::FORBIDDEN => "weknora_login_forbidden".into(),
        StatusCode::CONFLICT => "weknora_bootstrap_conflict".into(),
        StatusCode::TOO_MANY_REQUESTS | StatusCode::REQUEST_TIMEOUT => {
            "weknora_login_retryable".into()
        }
        value if value.is_server_error() => "weknora_login_unavailable".into(),
        _ => "weknora_login_failed".into(),
    }
}

fn validate_authorize_url(origin: &Url, url: &Url, attempt_id: &str) -> Result<(), String> {
    if url.origin() != origin.origin()
        || url.path() != "/api/v1/aiks/desktop/connect/browser"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("weknora_login_response_invalid".into());
    }
    let pairs = url.query_pairs().collect::<Vec<_>>();
    if pairs.len() != 2 {
        return Err("weknora_login_response_invalid".into());
    }
    let attempt_ok = pairs.iter().filter(|(key, _)| key == "attempt_id").count() == 1
        && pairs
            .iter()
            .any(|(key, value)| key == "attempt_id" && value.as_ref() == attempt_id);
    let launch_ok = pairs.iter().filter(|(key, _)| key == "launch").count() == 1
        && pairs
            .iter()
            .any(|(key, value)| key == "launch" && valid_sha256_hex(value.as_ref()));
    if !attempt_ok || !launch_ok {
        return Err("weknora_login_response_invalid".into());
    }
    Ok(())
}

fn validate_bootstrap(value: &DesktopBootstrap) -> Result<(), String> {
    if value.tenant_id == 0
        || !valid_id(&value.knowledge_base_id)
        || value.knowledge_base.trim().is_empty()
        || value.knowledge_base.chars().any(char::is_control)
        || !valid_secret(&value.api_key)
        || !value.capabilities.iter().any(|capability| capability == "retrieve")
        || value.display_name.chars().any(char::is_control)
    {
        return Err("weknora_login_response_invalid".into());
    }
    Ok(())
}

pub(crate) async fn start(
    base_url: &str,
    allow_insecure_http: bool,
) -> Result<(PendingPairing, String), String> {
    let origin = validate_origin(base_url, allow_insecure_http)?;
    let mut random = [0u8; 32];
    rand::rngs::OsRng
        .try_fill_bytes(&mut random)
        .map_err(|_| "weknora_login_unavailable".to_string())?;
    let verifier = hex::encode(random);
    let verifier_hash = hex::encode(Sha256::digest(verifier.as_bytes()));

    let url = origin
        .join("/api/v1/aiks/desktop/connect/start")
        .map_err(|_| "weknora_web_origin_invalid".to_string())?;
    let response = client()?
        .post(url)
        .json(&json!({"verifier_hash": verifier_hash}))
        .send()
        .await
        .map_err(|_| "weknora_login_unavailable".to_string())?;
    if !response.status().is_success() {
        return Err(response_error(response.status()));
    }

    #[derive(Deserialize)]
    struct StartResponse {
        attempt_id: String,
        authorize_path: String,
        expires_at: i64,
    }
    let value: StartResponse = bounded_json(response).await?;
    if !valid_id(&value.attempt_id)
        || value.attempt_id.len() > 256
        || value.expires_at <= 0
        || !value.authorize_path.starts_with('/')
    {
        return Err("weknora_login_response_invalid".into());
    }
    let authorize_url = origin
        .join(&value.authorize_path)
        .map_err(|_| "weknora_login_response_invalid".to_string())?;
    validate_authorize_url(&origin, &authorize_url, &value.attempt_id)?;

    Ok((
        PendingPairing {
            attempt_id: value.attempt_id,
            verifier,
        },
        authorize_url.to_string(),
    ))
}

pub(crate) async fn exchange(
    base_url: &str,
    allow_insecure_http: bool,
    pending: &PendingPairing,
) -> Result<PairingExchange, String> {
    let origin = validate_origin(base_url, allow_insecure_http)?;
    let url = origin
        .join("/api/v1/aiks/desktop/connect/exchange")
        .map_err(|_| "weknora_web_origin_invalid".to_string())?;
    let response = client()?
        .post(url)
        .json(&json!({
            "attempt_id": pending.attempt_id,
            "verifier": pending.verifier
        }))
        .send()
        .await
        .map_err(|_| "weknora_login_unavailable".to_string())?;
    if response.status() == StatusCode::ACCEPTED {
        return Ok(PairingExchange::Pending);
    }
    if !response.status().is_success() {
        return Err(response_error(response.status()));
    }

    #[derive(Deserialize)]
    struct ExchangeResponse {
        state: String,
        credential: Option<DesktopBootstrap>,
    }
    let value: ExchangeResponse = bounded_json(response).await?;
    if value.state != "connected" {
        return Err("weknora_login_response_invalid".into());
    }
    let credential = value
        .credential
        .ok_or_else(|| "weknora_login_response_invalid".to_string())?;
    validate_bootstrap(&credential)?;
    Ok(PairingExchange::Connected(credential))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_http_requires_explicit_opt_in() {
        assert!(validate_origin("http://203.0.113.10:8080", false).is_err());
        assert!(validate_origin("http://203.0.113.10:8080", true).is_ok());
        assert!(validate_origin("http://127.0.0.1:8080", false).is_ok());
        assert!(validate_origin("https://weknora.example.com", false).is_ok());
    }

    #[test]
    fn bootstrap_requires_scoped_retrieve_credential() {
        let valid = DesktopBootstrap {
            tenant_id: 1,
            knowledge_base_id: "kb-1".into(),
            knowledge_base: "AIKS Sessions".into(),
            api_key: "sk-example".into(),
            display_name: "AIKS User".into(),
            capabilities: vec!["retrieve".into()],
        };
        assert!(validate_bootstrap(&valid).is_ok());
        let mut missing_scope = valid.clone();
        missing_scope.capabilities.clear();
        assert!(validate_bootstrap(&missing_scope).is_err());
    }
}
