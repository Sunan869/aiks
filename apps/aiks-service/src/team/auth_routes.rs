//! Auth-only routes. Production team startup composes these with the authorized business router.
use super::{
    auth_http::{bearer, one_header, LoginLimiter, TeamHttpError},
    config::ValidatedTeamSettings,
    dingtalk::authorize_url,
};
use aiks_core::team::{
    AuthPolicy, IdentityProvider, LoginStore, SessionStore, SessionTokens, TeamError, TeamStore,
};
use axum::{
    extract::{
        rejection::{JsonRejection, QueryRejection},
        ConnectInfo, DefaultBodyLimit, Query, Request, State,
    },
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Semaphore;

const COOKIE: &str = "__Host-aiks-login";
pub struct AuthService {
    login: Arc<LoginStore>,
    pub(crate) sessions: Arc<SessionStore>,
    provider: Arc<dyn IdentityProvider>,
    settings: ValidatedTeamSettings,
    limiter: LoginLimiter,
    inflight: Semaphore,
}
impl AuthService {
    pub fn new(
        store: Arc<TeamStore>,
        provider: Arc<dyn IdentityProvider>,
        settings: ValidatedTeamSettings,
    ) -> Result<Self, TeamError> {
        let config = settings.settings();
        if store.corp_id() != config.dingtalk.corp_id
            || store.client_id() != config.dingtalk.client_id
        {
            return Err(TeamError::ConfigInvalid);
        }
        let policy = AuthPolicy {
            login_ttl: config.sessions.login_attempt_ttl_seconds,
            access_ttl: config.sessions.access_token_ttl_seconds,
            refresh_ttl: config.sessions.refresh_token_ttl_seconds,
            directory_max_age: config.directory.max_stale_seconds,
        };
        Ok(Self {
            login: Arc::new(LoginStore::new(store.clone(), policy)?),
            sessions: Arc::new(SessionStore::new(store, policy)?),
            provider,
            settings,
            limiter: LoginLimiter::default(),
            inflight: Semaphore::new(16),
        })
    }
    pub fn session_store(&self) -> Arc<SessionStore> {
        self.sessions.clone()
    }

    async fn blocking<T, F>(&self, action: F) -> Result<T, TeamHttpError>
    where
        T: Send + 'static,
        F: FnOnce(&LoginStore, &SessionStore, u64) -> Result<T, TeamError> + Send + 'static,
    {
        let (login, sessions) = (self.login.clone(), self.sessions.clone());
        let result = tokio::task::spawn_blocking(move || action(&login, &sessions, now()?))
            .await
            .map_err(|_| TeamError::Storage)??;
        Ok(result)
    }
}

pub fn auth_router(auth: Arc<AuthService>) -> Router {
    Router::new()
        .route("/api/v1/auth/dingtalk/start", post(start))
        .route("/api/v1/auth/dingtalk/browser", get(browser))
        .route("/api/v1/auth/dingtalk/callback", get(callback))
        .route("/api/v1/auth/dingtalk/exchange", post(exchange))
        .route("/api/v1/auth/refresh", post(refresh))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/me", get(me))
        .route("/api/v1/workspace/tickets", post(workspace_ticket))
        .route(
            "/api/v1/internal/workspace/tickets/consume",
            post(consume_workspace_ticket),
        )
        .method_not_allowed_fallback(|| async { TeamHttpError::from(TeamError::InvalidInput) })
        .layer(DefaultBodyLimit::max(8192))
        .layer(middleware::from_fn_with_state(auth.clone(), guard))
        .with_state(auth)
}
async fn guard(State(auth): State<Arc<AuthService>>, request: Request, next: Next) -> Response {
    let response = guard_inner(auth, request, next).await;
    let mut response = match response {
        Ok(r) => r,
        Err(e) => e.into_response(),
    };
    for (header, value) in [
        ("cache-control", "no-store"),
        ("referrer-policy", "no-referrer"),
        ("x-content-type-options", "nosniff"),
        (
            "content-security-policy",
            "default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
        ),
    ] {
        response
            .headers_mut()
            .insert(header, HeaderValue::from_static(value));
    }
    response
}
async fn guard_inner(
    auth: Arc<AuthService>,
    request: Request,
    next: Next,
) -> Result<Response, TeamHttpError> {
    let authority = auth
        .settings
        .settings()
        .public_base_url
        .strip_prefix("https://")
        .ok_or(TeamError::ConfigInvalid)?;
    if one_header(request.headers(), "host") != Some(authority)
        || request.headers().contains_key(header::ORIGIN)
    {
        return Err(TeamError::Unauthorized.into());
    }
    if request.uri().path().len() > 4096
        || request.uri().query().is_some_and(|q| q.len() > 8192)
        || request.headers().contains_key(header::CONTENT_ENCODING)
    {
        return Err(TeamError::InvalidInput.into());
    }
    if request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
        .is_some_and(|v| v > 8192)
    {
        return Err(TeamHttpError::TooLarge);
    }
    if let Some(query) = request.uri().query() {
        let values = serde_urlencoded::from_str::<Vec<(String, String)>>(query)
            .map_err(|_| TeamError::InvalidInput)?;
        if values.iter().any(|(key, _)| {
            matches!(
                key.to_ascii_lowercase().as_str(),
                "access_token" | "refresh_token" | "token" | "authorization"
            )
        }) {
            return Err(TeamError::InvalidInput.into());
        }
    }
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .ok_or(TeamError::Unavailable)?
        .0
        .ip();
    auth.limiter
        .check(peer, request.uri().path() == "/api/v1/auth/dingtalk/start")?;
    let _permit = auth
        .inflight
        .try_acquire()
        .map_err(|_| TeamHttpError::Limited)?;
    tokio::time::timeout(Duration::from_secs(60), next.run(request))
        .await
        .map_err(|_| TeamError::Unavailable.into())
}
fn body<T: DeserializeOwned>(value: Result<Json<T>, JsonRejection>) -> Result<T, TeamHttpError> {
    value.map(|Json(v)| v).map_err(|e| {
        if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
            TeamHttpError::TooLarge
        } else {
            TeamError::InvalidInput.into()
        }
    })
}
fn query<T: DeserializeOwned>(value: Result<Query<T>, QueryRejection>) -> Result<T, TeamHttpError> {
    value
        .map(|Query(v)| v)
        .map_err(|_| TeamError::InvalidInput.into())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartInput {
    verifier_hash: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserInput {
    attempt_id: String,
    launch: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CallbackInput {
    state: String,
    #[serde(rename = "authCode")]
    code: Option<String>,
    error: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExchangeInput {
    attempt_id: String,
    verifier: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RefreshInput {
    refresh_token: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceTicketInput {
    ticket: String,
}

async fn start(
    State(auth): State<Arc<AuthService>>,
    input: Result<Json<StartInput>, JsonRejection>,
) -> Result<Json<Value>, TeamHttpError> {
    let input = body(input)?;
    let start = auth
        .blocking(move |login, _, at| login.start(&input.verifier_hash, at))
        .await?;
    let mut url = reqwest::Url::parse(&format!(
        "{}/api/v1/auth/dingtalk/browser",
        auth.settings.settings().public_base_url
    ))
    .map_err(|_| TeamError::ConfigInvalid)?;
    url.query_pairs_mut()
        .append_pair("attempt_id", &start.attempt_id)
        .append_pair("launch", start.launch_token.expose());
    Ok(Json(
        json!({"attempt_id":start.attempt_id,"authorize_url":url.as_str(),"expires_at":start.expires_at}),
    ))
}
async fn browser(
    State(auth): State<Arc<AuthService>>,
    input: Result<Query<BrowserInput>, QueryRejection>,
) -> Result<Response, TeamHttpError> {
    let input = query(input)?;
    let browser = auth
        .blocking(move |login, _, at| login.open_browser(&input.attempt_id, &input.launch, at))
        .await?;
    let url = authorize_url(&auth.settings, browser.state.expose())?;
    let mut response = Redirect::to(&url).into_response();
    let cookie = format!(
        "{COOKIE}={}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age={}",
        browser.nonce.expose(),
        browser.expires_at.saturating_sub(now()?)
    );
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(|_| TeamError::Unavailable)?,
    );
    Ok(response)
}
async fn callback(
    State(auth): State<Arc<AuthService>>,
    headers: HeaderMap,
    input: Result<Query<CallbackInput>, QueryRejection>,
) -> Result<Response, TeamHttpError> {
    let input = query(input)?;
    if input.code.is_some() == input.error.is_some() {
        return Err(TeamError::InvalidInput.into());
    }
    let cookie = one_header(&headers, "cookie").ok_or(TeamError::Unauthorized)?;
    let mut nonces = cookie
        .split(';')
        .filter_map(|c| c.trim().split_once('='))
        .filter(|(name, _)| *name == COOKIE);
    let nonce = nonces.next().ok_or(TeamError::Unauthorized)?.1.to_owned();
    if nonces.next().is_some() {
        return Err(TeamError::Unauthorized.into());
    }
    let state = input.state;
    let code = input.code;
    let claim_code = code.clone();
    let claim = auth
        .blocking(move |login, _, at| {
            login.claim_callback(&state, &nonce, claim_code.as_deref(), at)
        })
        .await?;
    let cancelled = code.is_none();
    if let Some(code) = code {
        let result =
            tokio::time::timeout(Duration::from_secs(45), auth.provider.exchange_code(&code))
                .await
                .unwrap_or(Err(TeamError::Unavailable));
        match result {
            Ok(external) => {
                auth.blocking(move |login, _, at| login.finish_callback(&claim, external, at))
                    .await?;
            }
            Err(error) => {
                auth.blocking(move |login, _, at| login.fail_callback(&claim, at))
                    .await?;
                return Err(error.into());
            }
        }
    } else {
        auth.blocking(move |login, _, at| login.fail_callback(&claim, at))
            .await?;
    }
    // No scripts, third-party resources, supplied return URL, identity or token.
    let message = if cancelled {
        "登录已取消，可以关闭此页面。"
    } else {
        "钉钉授权已完成，请返回 AIKS 桌面程序。"
    };
    let mut response=Html(format!("<!doctype html><html lang=\"zh-CN\"><meta charset=\"utf-8\"><title>AIKS 登录</title><body><p>{message}</p></body></html>")).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static(
            "__Host-aiks-login=; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=0",
        ),
    );
    Ok(response)
}
fn tokens_response(tokens: SessionTokens) -> Json<Value> {
    Json(
        json!({"access_token":tokens.access_token.expose(),"refresh_token":tokens.refresh_token.expose(),"expires_in":tokens.expires_in,"identity":tokens.identity}),
    )
}
async fn exchange(
    State(auth): State<Arc<AuthService>>,
    input: Result<Json<ExchangeInput>, JsonRejection>,
) -> Result<Json<Value>, TeamHttpError> {
    let input = body(input)?;
    Ok(tokens_response(
        auth.blocking(move |login, _, at| login.exchange(&input.attempt_id, &input.verifier, at))
            .await?,
    ))
}
async fn refresh(
    State(auth): State<Arc<AuthService>>,
    input: Result<Json<RefreshInput>, JsonRejection>,
) -> Result<Json<Value>, TeamHttpError> {
    let input = body(input)?;
    Ok(tokens_response(
        auth.blocking(move |_, sessions, at| sessions.refresh(&input.refresh_token, at))
            .await?,
    ))
}
async fn logout(
    State(auth): State<Arc<AuthService>>,
    headers: HeaderMap,
) -> Result<StatusCode, TeamHttpError> {
    let token = bearer(&headers)?;
    auth.blocking(move |_, sessions, at| sessions.logout(&token, at))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn me(
    State(auth): State<Arc<AuthService>>,
    headers: HeaderMap,
) -> Result<Json<Value>, TeamHttpError> {
    let token = bearer(&headers)?;
    let identity = auth
        .blocking(move |_, sessions, at| {
            let ctx = sessions.authenticate(&token, at)?;
            sessions.identity(&ctx, at)
        })
        .await?;
    Ok(Json(json!(identity)))
}

async fn workspace_ticket(
    State(auth): State<Arc<AuthService>>,
    headers: HeaderMap,
) -> Result<Json<Value>, TeamHttpError> {
    let token = bearer(&headers)?;
    let ticket = auth
        .blocking(move |_, sessions, at| sessions.issue_workspace_ticket(&token, at))
        .await?;
    Ok(Json(json!({
        "ticket": ticket.ticket.expose(),
        "expires_at": ticket.expires_at
    })))
}

async fn consume_workspace_ticket(
    State(auth): State<Arc<AuthService>>,
    input: Result<Json<WorkspaceTicketInput>, JsonRejection>,
) -> Result<Json<Value>, TeamHttpError> {
    let input = body(input)?;
    let principal = auth
        .blocking(move |_, sessions, at| {
            sessions.consume_workspace_ticket(&input.ticket, at)
        })
        .await?;
    Ok(Json(json!(principal)))
}
pub(crate) fn now() -> Result<u64, TeamError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| TeamError::Unavailable)
}
