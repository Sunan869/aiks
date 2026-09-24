//! Thin bearer-to-TeamContext bridge for authenticated business routes.
use super::{
    auth_http::{bearer, TeamHttpError},
    auth_routes::now,
};
use aiks_core::team::SessionStore;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

pub(crate) async fn require_session(
    State(sessions): State<Arc<SessionStore>>,
    mut request: Request,
    next: Next,
) -> Response {
    let result = async {
        let token = bearer(request.headers())?;
        let at = now()?;
        let store = sessions.clone();
        let ctx = tokio::task::spawn_blocking(move || store.authenticate(&token, at))
            .await
            .map_err(|_| aiks_core::team::TeamError::Storage)??;
        request.extensions_mut().insert(ctx);
        Ok::<_, TeamHttpError>(next.run(request).await)
    }
    .await;
    match result {
        Ok(response) => response,
        Err(error) => error.into_response(),
    }
}
