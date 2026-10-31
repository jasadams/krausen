// SPDX-License-Identifier: AGPL-3.0-or-later
//! Installation-local MCP authorization server. No callback URLs are fetched.
use crate::state::AppState;
use axum::{
    Form, Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Query, State},
    http::Request,
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use krausen_auth::{
    middleware::AuthUser,
    oauth_service::{self, AuthorizationRequest, Principal},
};
use serde::Deserialize;
use serde_json::json;

pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/authorize", get(authorize))
        .route("/authorize/continue", get(continue_authorize))
        .route("/authorize/approve", post(approve))
        .route("/token", post(token))
        .route("/revoke", post(revoke))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(middleware::from_fn_with_state(state, secure_installation))
}
pub fn well_known_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/.well-known/oauth-authorization-server",
            get(authorization_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(resource_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(resource_metadata),
        )
        .layer(middleware::from_fn_with_state(state, secure_installation))
}
/// Check the configured external origin; request forwarding headers never set trust.
pub fn secure_external_origin(base: &str) -> bool {
    let Ok(url) = url::Url::parse(base) else {
        return false;
    };
    if !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    if url.scheme() == "https" {
        return url.host_str().is_some();
    }
    url.scheme() == "http" && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
}
pub fn secure_external_configuration(
    base: &str,
    secure_cookie: bool,
    http_only_cookie: bool,
) -> bool {
    secure_external_origin(base)
        && http_only_cookie
        && (url::Url::parse(base).is_ok_and(|url| url.scheme() != "https") || secure_cookie)
}
async fn secure_installation(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let cookies = &krausen_core::constants::get().cookies;
    if !secure_external_configuration(&state.config.base_url, cookies.secure, cookies.httponly) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    next.run(request).await
}
fn issuer(state: &AppState) -> String {
    state.config.base_url.trim_end_matches('/').to_owned()
}
fn resource(state: &AppState) -> String {
    format!("{}/mcp", issuer(state))
}
async fn authorization_metadata(State(state): State<AppState>) -> Json<serde_json::Value> {
    let base = issuer(&state);
    Json(
        json!({"issuer":base,"authorization_endpoint":format!("{base}/api/v1/oauth/authorize"),"token_endpoint":format!("{base}/api/v1/oauth/token"),"registration_endpoint":format!("{base}/api/v1/oauth/register"),"revocation_endpoint":format!("{base}/api/v1/oauth/revoke"),"response_types_supported":["code"],"grant_types_supported":["authorization_code","refresh_token"],"token_endpoint_auth_methods_supported":["none","client_secret_basic"],"code_challenge_methods_supported":["S256"]}),
    )
}
async fn resource_metadata(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(
        json!({"resource":resource(&state),"authorization_servers":[issuer(&state)],"bearer_methods_supported":["header"]}),
    )
}
#[derive(Deserialize)]
struct Registration {
    client_name: String,
    redirect_uris: Vec<String>,
    #[serde(default)]
    token_endpoint_auth_method: Option<String>,
}
async fn register(State(state): State<AppState>, Json(input): Json<Registration>) -> Response {
    let method = input
        .token_endpoint_auth_method
        .as_deref()
        .unwrap_or("client_secret_basic");
    match oauth_service::register_client_with_method(
        &state.db,
        &input.client_name,
        &input.redirect_uris,
        method,
    )
    .await
    {
        Ok(client) => (
            StatusCode::CREATED,
            [
                (header::CACHE_CONTROL, "no-store"),
                (header::PRAGMA, "no-cache"),
            ],
            Json(client),
        )
            .into_response(),
        Err(_) => oauth_error(StatusCode::BAD_REQUEST, "invalid_client_metadata"),
    }
}
async fn authorize(
    State(state): State<AppState>,
    Query(input): Query<AuthorizationRequest>,
) -> Response {
    match oauth_service::begin_authorization(&state.db, &input, &resource(&state)).await {
        Ok(pending) => Redirect::to(&format!("/api/v1/oauth/authorize/continue?state={pending}"))
            .into_response(),
        Err(_) => oauth_error(StatusCode::BAD_REQUEST, "invalid_request"),
    }
}
#[derive(Deserialize)]
struct PendingQuery {
    state: String,
}
async fn continue_authorize(
    State(state): State<AppState>,
    auth: Result<AuthUser, krausen_core::Error>,
    Query(input): Query<PendingQuery>,
) -> Response {
    if input.state.len() != 43
        || !input
            .state
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return oauth_error(StatusCode::BAD_REQUEST, "invalid_request");
    }
    let Ok(auth) = auth else {
        return Redirect::to(&format!("/login?oauth_continue={}", input.state)).into_response();
    };
    let principal = Principal {
        user_id: auth.user_id,
        credential_id: auth.credential_id,
        access_expires_at: auth.token_exp.unwrap_or(0),
    };
    match oauth_service::bind_pending(&state.db, &input.state, &principal).await {
        Ok(name) => {
            let safe_name = html_escape(&name);
            // State is generated by the server using the base64url alphabet.
            if !input
                .state
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                return oauth_error(StatusCode::BAD_REQUEST, "invalid_request");
            }
            ([(header::CACHE_CONTROL, "no-store"), (header::REFERRER_POLICY, "no-referrer")], Html(format!("<!doctype html><html><head><meta charset=\"utf-8\"><title>Authorize connection</title></head><body><main><h1>Authorize {safe_name}?</h1><p>This connection acts with your workspace permissions.</p><form method=\"post\" action=\"/api/v1/oauth/authorize/approve\"><input type=\"hidden\" name=\"state\" value=\"{}\"><button type=\"submit\">Authorize</button></form></main></body></html>", input.state))).into_response()
        }
        Err(_) => oauth_error(StatusCode::BAD_REQUEST, "invalid_request"),
    }
}
fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn same_origin(headers: &HeaderMap, state: &AppState) -> bool {
    let Ok(base) = url::Url::parse(&state.config.base_url) else {
        return false;
    };
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| url::Url::parse(v).ok())
    else {
        return false;
    };
    origin.origin() == base.origin()
}
async fn approve(
    State(state): State<AppState>,
    auth: Result<AuthUser, krausen_core::Error>,
    headers: HeaderMap,
    Form(input): Form<PendingQuery>,
) -> Response {
    if !same_origin(&headers, &state) {
        return oauth_error(StatusCode::FORBIDDEN, "invalid_request");
    }
    let Ok(auth) = auth else {
        return oauth_error(StatusCode::UNAUTHORIZED, "login_required");
    };
    let principal = Principal {
        user_id: auth.user_id,
        credential_id: auth.credential_id,
        access_expires_at: auth.token_exp.unwrap_or(0),
    };
    match oauth_service::approve(&state.db, &input.state, &principal).await {
        Ok((redirect, oauth_state, code)) => match url::Url::parse(&redirect) {
            Ok(mut target) => {
                target
                    .query_pairs_mut()
                    .append_pair("code", &code)
                    .append_pair("state", &oauth_state)
                    .append_pair("iss", &issuer(&state));
                Redirect::to(target.as_str()).into_response()
            }
            Err(_) => oauth_error(StatusCode::BAD_REQUEST, "invalid_request"),
        },
        Err(_) => oauth_error(StatusCode::BAD_REQUEST, "invalid_request"),
    }
}
#[derive(Deserialize)]
struct TokenRequest {
    grant_type: String,
    client_id: Option<String>,
    resource: String,
    code: Option<String>,
    redirect_uri: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
}
async fn token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(input): Form<TokenRequest>,
) -> Response {
    let Some((client_id, client_secret)) = client_auth(&headers, input.client_id.as_deref()) else {
        return invalid_client();
    };
    if let Err(error) =
        oauth_service::authenticate_client(&state.db, &client_id, client_secret.as_deref()).await
    {
        return auth_failure(error);
    }
    let base = issuer(&state);
    let expected = resource(&state);
    if input.resource != expected {
        return oauth_error(StatusCode::BAD_REQUEST, "invalid_target");
    }
    let result = match input.grant_type.as_str() {
        "authorization_code" => {
            oauth_service::exchange_code(
                &state.db,
                &base,
                &client_id,
                client_secret.as_deref(),
                input.redirect_uri.as_deref().unwrap_or(""),
                &input.resource,
                input.code.as_deref().unwrap_or(""),
                input.code_verifier.as_deref().unwrap_or(""),
            )
            .await
        }
        "refresh_token" => {
            oauth_service::refresh(
                &state.db,
                &base,
                &client_id,
                client_secret.as_deref(),
                &input.resource,
                input.refresh_token.as_deref().unwrap_or(""),
            )
            .await
        }
        _ => return oauth_error(StatusCode::BAD_REQUEST, "unsupported_grant_type"),
    };
    match result {
        Ok(tokens) => (
            [
                (header::CACHE_CONTROL, "no-store"),
                (header::PRAGMA, "no-cache"),
            ],
            Json(tokens),
        )
            .into_response(),
        Err(krausen_core::Error::Unauthorized(_) | krausen_core::Error::BadRequest(_)) => {
            oauth_error(StatusCode::BAD_REQUEST, "invalid_grant")
        }
        Err(error) => {
            tracing::error!(%error, "OAuth token grant failed");
            oauth_error(StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable")
        }
    }
}
fn auth_failure(error: krausen_core::Error) -> Response {
    if matches!(error, krausen_core::Error::Unauthorized(_)) {
        invalid_client()
    } else {
        tracing::error!(%error, "OAuth client authentication failed");
        oauth_error(StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable")
    }
}
fn invalid_client() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [
            (header::WWW_AUTHENTICATE, "Basic realm=\"oauth\""),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Json(json!({"error":"invalid_client"})),
    )
        .into_response()
}
fn oauth_error(status: StatusCode, error: &'static str) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({"error":error})),
    )
        .into_response()
}

#[derive(Deserialize)]
struct RevokeRequest {
    client_id: Option<String>,
    token: String,
}
async fn revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(input): Form<RevokeRequest>,
) -> Response {
    let Some((client_id, client_secret)) = client_auth(&headers, input.client_id.as_deref()) else {
        return invalid_client();
    };
    if let Err(error) =
        oauth_service::authenticate_client(&state.db, &client_id, client_secret.as_deref()).await
    {
        return auth_failure(error);
    }
    match oauth_service::revoke(
        &state.db,
        &client_id,
        client_secret.as_deref(),
        &input.token,
    )
    .await
    {
        Ok(()) => StatusCode::OK.into_response(),
        Err(_) => oauth_error(StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"),
    }
}

fn client_auth(headers: &HeaderMap, body_id: Option<&str>) -> Option<(String, Option<String>)> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    if let Some(header) = headers.get(header::AUTHORIZATION) {
        let encoded = header.to_str().ok()?.strip_prefix("Basic ")?;
        let decoded = String::from_utf8(STANDARD.decode(encoded).ok()?).ok()?;
        let (id, secret) = decoded.split_once(':')?;
        if id.is_empty() || secret.is_empty() || body_id.is_some_and(|body| body != id) {
            return None;
        }
        return Some((id.to_owned(), Some(secret.to_owned())));
    }
    let id = body_id.filter(|id| !id.is_empty())?;
    Some((id.to_owned(), None))
}

#[cfg(test)]
mod security_tests {
    use super::{secure_external_configuration, secure_external_origin};
    #[test]
    fn external_origin_requires_https_except_loopback_development() {
        assert!(secure_external_origin("https://brew.example"));
        assert!(secure_external_origin("http://127.0.0.1:3000"));
        assert!(secure_external_origin("http://localhost:3000"));
        assert!(!secure_external_origin("http://brew.example"));
        assert!(!secure_external_origin("http://192.168.1.2"));
        assert!(!secure_external_origin("http://localhost.evil.example"));
        assert!(!secure_external_origin("https://user@brew.example"));
        assert!(!secure_external_origin("https://brew.example/path"));
        assert!(secure_external_configuration(
            "https://brew.example",
            true,
            true
        ));
        assert!(!secure_external_configuration(
            "https://brew.example",
            false,
            true
        ));
        assert!(!secure_external_configuration(
            "https://brew.example",
            true,
            false
        ));
    }
}
