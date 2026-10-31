// SPDX-License-Identifier: AGPL-3.0-or-later

//! Google OAuth login endpoint.
//!
//! Only the flow *entry point* lives here. The callback is handled by the
//! `google_oauth_callback` Leptos server function, which needs to set session
//! cookies on the response.
//!
//! - `GET /google/login` — build the Google authorization URL and stash a CSRF
//!   state token so the callback can verify it.

use axum::{extract::{Query, State}, routing::get, Json, Router};
use serde::Deserialize;

use krausen_auth::{google_oauth, redis_ops};

use crate::state::AppState;

/// Google OAuth routes. Mounted at `/api/v1/auth`.
pub fn routes() -> Router<AppState> {
    Router::new().route("/google/login", get(google_login))
}

#[derive(Deserialize)]
struct GoogleLoginQuery {
    /// Opaque state forwarded from an in-progress MCP OAuth flow, resumed
    /// after Google sign-in completes.
    oauth_continue: Option<String>,
}

/// Build the Google authorization URL.
///
/// Returns `{ "authorization_url": ..., "state": ... }`. The client redirects
/// the browser to `authorization_url`.
async fn google_login(
    State(state): State<AppState>,
    Query(query): Query<GoogleLoginQuery>,
) -> Result<Json<serde_json::Value>, krausen_core::Error> {
    let client_id = state
        .config
        .google_oauth_client_id
        .clone()
        .ok_or_else(|| {
            krausen_core::Error::BadRequest("Google OAuth is not configured on this server".into())
        })?;

    let csrf_state = redis_ops::generate_token();

    let redirect_uri = format!(
        "{}/auth/google/callback",
        state.config.frontend_url.trim_end_matches('/')
    );

    let authorization_url = google_oauth::build_authorization_url(
        &client_id,
        &redirect_uri,
        &csrf_state,
        google_oauth::LOGIN_SCOPES,
        false, // don't force the consent screen on plain sign-in
        false, // no offline access — we never use a refresh token here
    );

    let mut state_data = serde_json::json!({
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    if let Some(ref oauth_continue) = query.oauth_continue {
        state_data["oauth_continue"] = serde_json::json!(oauth_continue);
    }
    redis_ops::store_oauth_state(&state.kv, "google", &csrf_state, &state_data).await?;

    Ok(Json(serde_json::json!({
        "authorization_url": authorization_url,
        "state": csrf_state,
    })))
}
