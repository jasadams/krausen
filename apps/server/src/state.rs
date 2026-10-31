// SPDX-License-Identifier: AGPL-3.0-or-later

//! Shared application state passed to all axum handlers.

use axum::extract::FromRef;
use krausen_auth::mcp_session_manager::MCPSessionManager;
use krausen_auth::middleware::AuthState;
use krausen_auth::websocket::WebSocketManager;
use webauthn_rs::Webauthn;

/// Application-wide shared state.
#[derive(Clone)]
pub struct AppState {
    pub db: krausen_core::DbPool,
    pub kv: krausen_core::KVPool,
    pub redis: Option<krausen_core::RedisPool>,
    pub config: std::sync::Arc<krausen_core::Config>,
    pub encryption_key: std::sync::Arc<[u8; 32]>,
    pub webauthn: std::sync::Arc<Webauthn>,
    pub ws_manager: WebSocketManager,
    pub mcp_sessions: MCPSessionManager,
}

impl FromRef<AppState> for AuthState {
    fn from_ref(state: &AppState) -> Self {
        AuthState {
            jwt_secret: state.config.jwt_secret.clone(),
            db: state.db.clone(),
            is_personal: state.config.is_personal(),
        }
    }
}
