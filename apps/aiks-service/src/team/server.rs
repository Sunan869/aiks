//! Single-company team server composition. TLS terminates at the local reverse proxy.
use super::{
    auth_http::{one_header, TeamHttpError},
    auth_routes::{auth_router, AuthService},
    business_routes::business_router,
    config::ValidatedTeamSettings,
    directory_worker::{DirectoryPolicy, DirectoryWorker},
};
use crate::{config::allowed_ip, ServiceConfig, ServiceRuntime};
use aiks_core::{
    storage::StateDb,
    team::{IdentityProvider, TeamError, TeamStore},
};
use axum::{
    extract::{Request, State},
    http::{header, HeaderValue},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;
use std::{net::IpAddr, sync::Arc, time::Duration};

#[derive(Clone)]
struct Perimeter {
    authority: String,
}

pub struct TeamServer {
    router: Router,
    runtime: Arc<ServiceRuntime>,
    directory: DirectoryWorker,
}

impl TeamServer {
    pub fn build(
        config: &ServiceConfig,
        settings: ValidatedTeamSettings,
        provider: Arc<dyn IdentityProvider>,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(config.mode == "team", "team mode required");
        anyhow::ensure!(
            allowed_ip(config.listen.ip()) && config.listen.port() != 0,
            "team listener must be a fixed numeric loopback address"
        );
        let expected = settings.settings();
        anyhow::ensure!(
            expected.enabled
                && expected.dingtalk.enabled
                && expected.dingtalk.corp_id == config.team.dingtalk.corp_id
                && expected.dingtalk.client_id == config.team.dingtalk.client_id
                && expected.public_base_url == config.team.public_base_url,
            "validated team settings mismatch"
        );
        let database = config.database.clone();
        let corp_id = expected.dingtalk.corp_id.clone();
        let client_id = expected.dingtalk.client_id.clone();
        let db = Arc::new(StateDb::open_exclusive(&database)?);
        let store = Arc::new(TeamStore::bind(db, &corp_id, &client_id)?);
        let runtime = Arc::new(ServiceRuntime::open_team_bound(
            store.clone(),
            config.runtime_config(),
        )?);
        let auth = Arc::new(AuthService::new(
            store.clone(),
            provider.clone(),
            settings.clone(),
        )?);
        let directory = DirectoryWorker::start(
            store,
            provider,
            DirectoryPolicy {
                scope: expected.directory.root_department_ids.clone(),
                refresh_seconds: expected.directory.refresh_interval_seconds,
                max_stale_seconds: expected.directory.max_stale_seconds,
            },
        )?;
        let router = router(runtime.clone(), auth, &settings)?;
        Ok(Self {
            router,
            runtime,
            directory,
        })
    }

    pub fn router(&self) -> Router {
        self.router.clone()
    }

    pub async fn shutdown(&self, grace: Duration) -> anyhow::Result<()> {
        self.directory
            .shutdown(grace)
            .await
            .map_err(|_| anyhow::anyhow!("directory shutdown failed"))?;
        self.runtime.shutdown(grace).await?;
        Ok(())
    }
}

fn router(
    runtime: Arc<ServiceRuntime>,
    auth: Arc<AuthService>,
    settings: &ValidatedTeamSettings,
) -> anyhow::Result<Router> {
    let authority = settings
        .settings()
        .public_base_url
        .strip_prefix("https://")
        .filter(|value| !value.is_empty() && !value.contains('/'))
        .ok_or_else(|| anyhow::anyhow!("team public authority required"))?
        .to_owned();
    let perimeter = Arc::new(Perimeter { authority });
    Ok(Router::new()
        .route(
            "/healthz",
            get(|| async { Json(json!({"status":"ok","mode":"team"})) }),
        )
        .merge(auth_router(auth.clone()))
        .merge(business_router(runtime, auth.session_store()))
        .layer(middleware::from_fn_with_state(perimeter, perimeter_guard)))
}

async fn perimeter_guard(
    State(state): State<Arc<Perimeter>>,
    request: Request,
    next: Next,
) -> Response {
    let result = perimeter_inner(&state, &request);
    let mut response = match result {
        Ok(()) => next.run(request).await,
        Err(error) => TeamHttpError::from(error).into_response(),
    };
    for (name, value) in [
        (header::CACHE_CONTROL, "no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}

fn perimeter_inner(state: &Perimeter, request: &Request) -> Result<(), TeamError> {
    if one_header(request.headers(), header::HOST.as_str()) != Some(state.authority.as_str())
        || request.headers().contains_key(header::ORIGIN)
        || request.headers().contains_key("forwarded")
        || request.headers().contains_key("x-forwarded-host")
        || request.headers().contains_key("x-forwarded-proto")
        || request.headers().contains_key("x-forwarded-for")
        || request.headers().contains_key("x-original-host")
    {
        return Err(TeamError::Unauthorized);
    }
    if request.uri().path().len() > 4096
        || request
            .uri()
            .query()
            .is_some_and(|query| query.len() > 8192)
    {
        return Err(TeamError::InvalidInput);
    }
    Ok(())
}

pub fn validate_team_content_origin(config: &ServiceConfig) -> anyhow::Result<()> {
    let url = reqwest::Url::parse(&config.siyuan.base_url)?;
    let ip = url
        .host_str()
        .and_then(|host| host.trim_matches(['[', ']']).parse::<IpAddr>().ok());
    anyhow::ensure!(
        url.scheme() == "http"
            && ip.is_some_and(allowed_ip)
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && matches!(url.path(), "" | "/"),
        "team content origin must be loopback-only"
    );
    Ok(())
}
