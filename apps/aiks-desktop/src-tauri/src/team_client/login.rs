use super::connection::{valid_token, TeamEndpoint, TeamIdentity};
use crate::service_client::{valid_id, ClientError, ClientResult};
use reqwest::{Client, Method, StatusCode};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{future::Future, pin::Pin, time::Duration};

pub type AuthFuture<'a, T> = Pin<Box<dyn Future<Output = ClientResult<T>> + Send + 'a>>;

#[derive(Clone, Deserialize)]
pub struct AuthStart {
    pub attempt_id: String,
    pub authorize_url: String,
    pub expires_at: u64,
}

#[derive(Clone, Deserialize)]
pub struct AuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub identity: TeamIdentity,
}
impl AuthTokens {
    pub fn validate(&self) -> ClientResult<()> {
        if !valid_token(&self.access_token)
            || !valid_token(&self.refresh_token)
            || self.expires_in == 0
            || self.expires_in > 3600
        {
            return Err(ClientError::InvalidResponse);
        }
        self.identity.validate()
    }
}

#[derive(Clone)]
pub struct PendingLogin {
    pub attempt_id: String,
    pub(super) verifier: String,
    pub authorize_url: String,
}
impl PendingLogin {
    pub fn verifier(&self) -> &str {
        &self.verifier
    }
}

pub trait TeamAuthTransport: Send + Sync {
    fn start<'a>(
        &'a self,
        endpoint: &'a TeamEndpoint,
        verifier_hash: &'a str,
    ) -> AuthFuture<'a, AuthStart>;
    fn exchange<'a>(
        &'a self,
        endpoint: &'a TeamEndpoint,
        attempt_id: &'a str,
        verifier: &'a str,
    ) -> AuthFuture<'a, AuthTokens>;
    fn refresh<'a>(
        &'a self,
        endpoint: &'a TeamEndpoint,
        refresh_token: &'a str,
    ) -> AuthFuture<'a, AuthTokens>;
    fn logout<'a>(
        &'a self,
        endpoint: &'a TeamEndpoint,
        access_token: &'a str,
    ) -> AuthFuture<'a, ()>;
}

pub struct ReqwestTeamAuthTransport {
    client: Client,
}
impl ReqwestTeamAuthTransport {
    pub fn new() -> ClientResult<Self> {
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| ClientError::InvalidInput)?;
        Ok(Self { client })
    }

    async fn json<T: DeserializeOwned>(
        &self,
        endpoint: &TeamEndpoint,
        method: Method,
        path: &str,
        body: Option<Vec<u8>>,
        bearer: Option<&str>,
    ) -> ClientResult<T> {
        let url = endpoint.url(path)?;
        let mut request = self.client.request(method, url);
        if let Some(token) = bearer {
            if !valid_token(token) {
                return Err(ClientError::Unauthorized);
            }
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request
                .header("Content-Type", "application/json")
                .body(body);
        }
        let mut response = request.send().await.map_err(|_| ClientError::Retryable)?;
        if response.status() == StatusCode::ACCEPTED {
            return Err(ClientError::LoginPending);
        }
        if !response.status().is_success() {
            return Err(classify(response.status()));
        }
        let mut bytes = Vec::new();
        const MAX: usize = 64 * 1024;
        while let Some(chunk) = response.chunk().await.map_err(|_| ClientError::Retryable)? {
            if bytes.len().saturating_add(chunk.len()) > MAX {
                return Err(ClientError::TooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| ClientError::InvalidResponse)
    }
}
impl TeamAuthTransport for ReqwestTeamAuthTransport {
    fn start<'a>(
        &'a self,
        endpoint: &'a TeamEndpoint,
        verifier_hash: &'a str,
    ) -> AuthFuture<'a, AuthStart> {
        Box::pin(async move {
            if !valid_token(verifier_hash) {
                return Err(ClientError::InvalidInput);
            }
            let body = serde_json::to_vec(&json!({"verifier_hash":verifier_hash}))
                .map_err(|_| ClientError::InvalidInput)?;
            let value: AuthStart = self
                .json(
                    endpoint,
                    Method::POST,
                    "/api/v1/auth/dingtalk/start",
                    Some(body),
                    None,
                )
                .await?;
            if !valid_id(&value.attempt_id) || value.expires_at == 0 {
                return Err(ClientError::InvalidResponse);
            }
            endpoint.validate_browser_handoff(&value.authorize_url)?;
            Ok(value)
        })
    }
    fn exchange<'a>(
        &'a self,
        endpoint: &'a TeamEndpoint,
        attempt_id: &'a str,
        verifier: &'a str,
    ) -> AuthFuture<'a, AuthTokens> {
        Box::pin(async move {
            if !valid_id(attempt_id) || !valid_token(verifier) {
                return Err(ClientError::InvalidInput);
            }
            let body = serde_json::to_vec(&json!({"attempt_id":attempt_id,"verifier":verifier}))
                .map_err(|_| ClientError::InvalidInput)?;
            let value: AuthTokens = self
                .json(
                    endpoint,
                    Method::POST,
                    "/api/v1/auth/dingtalk/exchange",
                    Some(body),
                    None,
                )
                .await?;
            value.validate()?;
            Ok(value)
        })
    }
    fn refresh<'a>(
        &'a self,
        endpoint: &'a TeamEndpoint,
        refresh_token: &'a str,
    ) -> AuthFuture<'a, AuthTokens> {
        Box::pin(async move {
            if !valid_token(refresh_token) {
                return Err(ClientError::Unauthorized);
            }
            let body = serde_json::to_vec(&json!({"refresh_token":refresh_token}))
                .map_err(|_| ClientError::InvalidInput)?;
            let value: AuthTokens = self
                .json(
                    endpoint,
                    Method::POST,
                    "/api/v1/auth/refresh",
                    Some(body),
                    None,
                )
                .await?;
            value.validate()?;
            Ok(value)
        })
    }
    fn logout<'a>(
        &'a self,
        endpoint: &'a TeamEndpoint,
        access_token: &'a str,
    ) -> AuthFuture<'a, ()> {
        Box::pin(async move {
            let url = endpoint.url("/api/v1/auth/logout")?;
            if !valid_token(access_token) {
                return Err(ClientError::Unauthorized);
            }
            let response = self
                .client
                .post(url)
                .bearer_auth(access_token)
                .send()
                .await
                .map_err(|_| ClientError::Retryable)?;
            if response.status() == StatusCode::NO_CONTENT {
                Ok(())
            } else {
                Err(classify(response.status()))
            }
        })
    }
}

pub fn verifier() -> ClientResult<(String, String)> {
    let mut bytes = [0u8; 32];
    use rand::RngCore;
    rand::rngs::OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| ClientError::Storage)?;
    let verifier = hex::encode(bytes);
    let hash = hex::encode(Sha256::digest(verifier.as_bytes()));
    Ok((verifier, hash))
}

fn classify(status: StatusCode) -> ClientError {
    match status {
        StatusCode::UNAUTHORIZED => ClientError::Unauthorized,
        StatusCode::FORBIDDEN => ClientError::Forbidden,
        StatusCode::NOT_FOUND => ClientError::NotFound,
        StatusCode::CONFLICT => ClientError::Conflict,
        StatusCode::PAYLOAD_TOO_LARGE => ClientError::TooLarge,
        StatusCode::TOO_MANY_REQUESTS | StatusCode::REQUEST_TIMEOUT => ClientError::Retryable,
        value if value.is_server_error() => ClientError::Retryable,
        _ => ClientError::InvalidResponse,
    }
}
