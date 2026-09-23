use std::time::Duration;
use aiks_core::team::TeamError;
use reqwest::{header, RequestBuilder, StatusCode};
use serde_json::Value;
use super::DingTalkClient;

const RESPONSE_LIMIT: usize = 2 * 1024 * 1024;

impl DingTalkClient {
    pub(super) async fn execute(&self, request: RequestBuilder) -> Result<Value, TeamError> {
        for attempt in 0..3 {
            let mut response = request.try_clone().ok_or(TeamError::Unavailable)?.send().await.map_err(|_| TeamError::Unavailable)?;
            if response.status() == StatusCode::TOO_MANY_REQUESTS && attempt < 2 {
                let delay = match response.headers().get(header::RETRY_AFTER) {
                    Some(value) => {
                        let seconds=value.to_str().ok().and_then(|v|v.parse::<u64>().ok()).filter(|n|*n<=10).ok_or(TeamError::Unavailable)?;
                        Duration::from_secs(seconds)
                    }
                    None => Duration::from_millis(100 * (1 << attempt)),
                };
                drop(response);
                tokio::time::sleep(delay).await;
                continue;
            }
            if !response.status().is_success() || response.content_length().is_some_and(|n|n>RESPONSE_LIMIT as u64) {
                return Err(TeamError::Unavailable);
            }
            let mut bytes=Vec::new();
            while let Some(chunk)=response.chunk().await.map_err(|_|TeamError::Unavailable)? {
                if bytes.len().saturating_add(chunk.len())>RESPONSE_LIMIT {return Err(TeamError::Unavailable);}
                bytes.extend_from_slice(&chunk);
            }
            return serde_json::from_slice(&bytes).map_err(|_|TeamError::Unavailable);
        }
        Err(TeamError::Unavailable)
    }

    pub(super) async fn legacy(&self, path: &str, token: &str, body: Value) -> Result<Value, TeamError> {
        // The legacy official read APIs require the app token as a query field.
        // This helper never logs a URL, response body or upstream error string.
        let result=self.execute(self.http.post(format!("{}{path}",self.legacy_origin))
            .query(&[("access_token",token)]).json(&body)).await?;
        match result.get("errcode").and_then(Value::as_i64) {
            Some(0) => result.get("result").cloned().ok_or(TeamError::Unavailable),
            Some(40014 | 42001) => {
                *self.app_token.lock().await=None;
                Err(TeamError::Unavailable)
            }
            _ => Err(TeamError::Unavailable),
        }
    }
}
