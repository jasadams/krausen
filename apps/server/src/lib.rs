// SPDX-License-Identifier: AGPL-3.0-or-later

//! krausen-server — Axum HTTP server for the Krausen backend.

pub mod leptos_frontend;
pub mod middleware;
pub mod routes;
pub mod state;

use std::net::SocketAddr;

use axum::Router;
use tower::Layer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::normalize_path::NormalizePathLayer;
use tower_http::trace::TraceLayer;

use krausen_core::db::DbPool;
use krausen_core::sql_compat;

/// Build the axum Router with all core routes and middleware.
pub fn build_router(state: state::AppState) -> Router {
    use axum::extract::FromRef;

    Router::new()
        .route("/api/health", axum::routing::get(health_handler))
        .route("/health", axum::routing::get(health_handler))
        .nest("/api/v1/auth", routes::auth_token::routes())
        .nest("/api/v1/auth", routes::auth_google_oauth::routes())
        .nest("/mcp", routes::mcp::routes(state.clone()))
        .merge(routes::oauth::well_known_routes(state.clone()))
        .nest("/api/v1/oauth", routes::oauth::routes(state.clone()))
        .route("/ws/{user_id}", axum::routing::get(routes::websocket::ws_handler))
        // Leptos server functions
        .route("/leptos-api/{*fn_name}", axum::routing::post({
            let server_ctx = krausen_ui::server_fns::ServerContext {
                db: state.db.clone(),
                config: state.config.clone(),
                auth_state: krausen_auth::middleware::AuthState::from_ref(&state),
                encryption_key: Some(state.encryption_key.clone()),
                kv: Some(state.kv.clone()),
                redis: state.redis.clone(),
                webauthn: Some(state.webauthn.clone()),
                ws_manager: Some(state.ws_manager.clone()),
                mcp_sessions: Some(state.mcp_sessions.clone()),
            };
            move |req: axum::http::Request<axum::body::Body>| {
                let ctx = server_ctx.clone();
                async move {
                    leptos_axum::handle_server_fns_with_context(
                        move || {
                            leptos::prelude::provide_context(ctx.clone());
                        },
                        req,
                    )
                    .await
                }
            }
        }))
        .route("/login", axum::routing::get(leptos_frontend::serve_leptos_shell))
        .route("/signup", axum::routing::get(leptos_frontend::serve_leptos_shell))
        .fallback(leptos_frontend::serve_asset_or_shell)
        .with_state(state)
        .layer(axum::middleware::from_fn(middleware::security_headers))
        .layer(middleware::cors_layer())
        .layer(TraceLayer::new_for_http().make_span_with(|request: &axum::http::Request<_>| {
            tracing::info_span!("http_request", method = %request.method(), path = %request.uri().path())
        }))
        .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024))
}

async fn health_handler() -> &'static str {
    "ok"
}

/// Wrap a completed Router with path normalization and connect-info extraction.
pub fn wrap_service(
    router: Router,
) -> axum::extract::connect_info::IntoMakeServiceWithConnectInfo<
    tower_http::normalize_path::NormalizePath<Router>,
    SocketAddr,
> {
    let app = NormalizePathLayer::trim_trailing_slash().layer(router);
    axum::ServiceExt::<axum::extract::Request>::into_make_service_with_connect_info::<SocketAddr>(
        app,
    )
}

/// One-time local operator initialization; never reachable over HTTP.
pub async fn bootstrap_owner(
    db: &DbPool,
    email: &str,
    password: &str,
) -> Result<(), krausen_core::Error> {
    let email = email.trim().to_lowercase();
    if !email.contains('@') || email.len() > 255 || password.len() < 12 {
        return Err(krausen_core::Error::BadRequest(
            "A valid email and password of at least 12 bytes are required".into(),
        ));
    }
    let hash = krausen_auth::password::hash_password(password)?;
    let user_id = format!("user-{}", uuid::Uuid::new_v4());
    let workspace_id = format!("ws-{}", uuid::Uuid::new_v4());
    let mut tx = db.begin_write().await?;
    if db.is_postgres() {
        krausen_core::dbtx_execute!(&mut tx, "LOCK TABLE users IN EXCLUSIVE MODE")?;
    }
    let count = krausen_core::dbtx_fetch_scalar!(&mut tx, i64, "SELECT COUNT(*) FROM users")?;
    if count != 0 {
        return Err(krausen_core::Error::Conflict(
            "Installation already initialized".into(),
        ));
    }
    let truth = sql_compat::bool_true(db.is_postgres());
    let sql =
        format!("INSERT INTO users (user_id,email,verified,active) VALUES ($1,$2,{truth},{truth})");
    krausen_core::dbtx_execute!(&mut tx, &sql, &user_id, &email)?;
    let auth_data = serde_json::json!({"hash": hash}).to_string();
    let sql = if db.is_postgres() {
        "INSERT INTO user_auth_methods (user_id,auth_type,auth_data) VALUES ($1,'password',CAST($2 AS JSON))"
    } else {
        "INSERT INTO user_auth_methods (user_id,auth_type,auth_data) VALUES ($1,'password',$2)"
    };
    krausen_core::dbtx_execute!(&mut tx, sql, &user_id, &auth_data)?;
    krausen_core::dbtx_execute!(
        &mut tx,
        "INSERT INTO workspaces (workspace_id,name,owner_user_id,status) VALUES ($1,'My Workspace',$2,'active')",
        &workspace_id,
        &user_id
    )?;
    let sql = format!(
        "INSERT INTO workspace_users (workspace_id,user_id,role,active) VALUES ($1,$2,'workspace_admin',{truth})"
    );
    krausen_core::dbtx_execute!(&mut tx, &sql, &workspace_id, &user_id)?;
    tx.commit().await?;
    Ok(())
}

/// Claim only the exact, uncredentialed account created by the retired personal
/// mode. This is a local operator command, not a remote account recovery flow.
pub async fn claim_legacy_personal_owner(
    db: &DbPool,
    email: &str,
    password: &str,
) -> Result<(), krausen_core::Error> {
    let email = email.trim().to_lowercase();
    if !email.contains('@') || email.len() > 255 || password.len() < 12 {
        return Err(krausen_core::Error::BadRequest(
            "A valid email and password of at least 12 bytes are required".into(),
        ));
    }
    let hash = krausen_auth::password::hash_password(password)?;
    let mut tx = db.begin_write().await?;
    if db.is_postgres() {
        krausen_core::dbtx_execute!(
            &mut tx,
            "LOCK TABLE users, workspaces, workspace_users, user_auth_methods, refresh_tokens, auth_credentials, api_tokens, verification_tokens, oauth_states, oauth_clients, mcp_oauth_clients, mcp_oauth_pending IN EXCLUSIVE MODE"
        )?;
    }
    let truth = sql_compat::bool_true(db.is_postgres());
    let exact_user = format!(
        "SELECT COUNT(*) FROM users WHERE user_id = 'user-local' AND email = 'local@localhost' AND name = 'Local User' AND oauth_data IS NULL AND active = {truth} AND verified = {truth}"
    );
    let exact_membership = format!(
        "SELECT COUNT(*) FROM workspace_users WHERE workspace_id = 'workspace-local' AND user_id = 'user-local' AND role = 'workspace_admin' AND active = {truth}"
    );
    for (sql, expected) in [
        ("SELECT COUNT(*) FROM users", 1),
        (exact_user.as_str(), 1),
        ("SELECT COUNT(*) FROM workspaces", 1),
        (
            "SELECT COUNT(*) FROM workspaces WHERE workspace_id = 'workspace-local' AND owner_user_id = 'user-local'",
            1,
        ),
        ("SELECT COUNT(*) FROM workspace_users", 1),
        (exact_membership.as_str(), 1),
        ("SELECT COUNT(*) FROM user_auth_methods", 0),
        ("SELECT COUNT(*) FROM refresh_tokens", 0),
        ("SELECT COUNT(*) FROM auth_credentials", 0),
        ("SELECT COUNT(*) FROM api_tokens", 0),
        ("SELECT COUNT(*) FROM verification_tokens", 0),
        ("SELECT COUNT(*) FROM oauth_states", 0),
        ("SELECT COUNT(*) FROM oauth_clients", 0),
        ("SELECT COUNT(*) FROM mcp_oauth_clients", 0),
        ("SELECT COUNT(*) FROM mcp_oauth_pending", 0),
    ] {
        if krausen_core::dbtx_fetch_scalar!(&mut tx, i64, sql)? != expected {
            return Err(krausen_core::Error::Conflict(
                "Legacy installation is ambiguous or already credentialed".into(),
            ));
        }
    }
    let auth_data = serde_json::json!({"hash": hash}).to_string();
    let insert = if db.is_postgres() {
        "INSERT INTO user_auth_methods (user_id,auth_type,auth_data) VALUES ('user-local','password',CAST($1 AS JSON))"
    } else {
        "INSERT INTO user_auth_methods (user_id,auth_type,auth_data) VALUES ('user-local','password',$1)"
    };
    krausen_core::dbtx_execute!(&mut tx, insert, &auth_data)?;
    krausen_core::dbtx_execute!(
        &mut tx,
        "UPDATE users SET email = $1 WHERE user_id = 'user-local' AND email = 'local@localhost'",
        &email
    )?;
    tx.commit().await?;
    Ok(())
}
