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
