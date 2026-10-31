// SPDX-License-Identifier: AGPL-3.0-or-later

//! WebSocket endpoint for real-time server -> client messages.
//!
//! `GET /ws/{user_id}?token={jwt}`
//!
//! The connection carries messages produced by
//! [`krausen_auth::websocket::helpers`] — workspace invitations, ownership
//! transfers, member changes and sync actions. Clients send nothing but
//! `ping`; everything else is server-initiated.

use axum::{
    extract::{ws, Path, Query, State},
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;

use krausen_auth::{jwt, user_service};

use crate::state::AppState;

/// Query parameters for WebSocket authentication.
#[derive(Debug, Deserialize)]
pub struct WsAuthParams {
    /// JWT access token. Browsers cannot set headers on a WebSocket handshake,
    /// so the token travels as a query parameter.
    token: Option<String>,
}

const CLOSE_AUTH_REQUIRED: u16 = 4001;
const CLOSE_FORBIDDEN: u16 = 4003;
const CLOSE_TOO_MANY_CONNECTIONS: u16 = 4029;

/// Maximum size of a single client message (64 KB). Clients only ever send
/// small JSON pings, so this is generous and bounds a trivial DoS.
const MAX_MESSAGE_SIZE: usize = 64 * 1024;

/// Ping every 45s. Keeps the connection alive through proxies that drop idle
/// sockets (Cloudflare at 100s, common nginx defaults at 60s).
const PING_INTERVAL_SECS: u64 = 45;

/// WebSocket upgrade handler.
///
/// 1. Validate the JWT from `?token=`
/// 2. Verify the path `user_id` matches the token subject
/// 3. Verify the user exists and is active
/// 4. Register with the [`WebSocketManager`](krausen_auth::websocket::WebSocketManager)
/// 5. Pump manager messages out, pings out, and client frames in
pub async fn ws_handler(
    ws: ws::WebSocketUpgrade,
    State(state): State<AppState>,
    Path(path_user_id): Path<String>,
    Query(params): Query<WsAuthParams>,
) -> impl IntoResponse {
    ws.max_message_size(MAX_MESSAGE_SIZE)
        .on_upgrade(move |socket| handle_authenticated_ws(socket, state, path_user_id, params))
}

async fn handle_authenticated_ws(
    socket: ws::WebSocket,
    state: AppState,
    path_user_id: String,
    params: WsAuthParams,
) {
    let token = match params.token {
        Some(t) if !t.is_empty() => t,
        _ => {
            tracing::warn!("WebSocket rejected: no token provided");
            close_with_code(socket, CLOSE_AUTH_REQUIRED, "Authentication required").await;
            return;
        }
    };

    let claims = match jwt::validate_token(&token, &state.config.jwt_secret) {
        Ok(token_data) => token_data.claims,
        Err(e) => {
            tracing::warn!(error = %e, "WebSocket rejected: JWT validation failed");
            close_with_code(socket, CLOSE_AUTH_REQUIRED, "Authentication failed").await;
            return;
        }
    };

    let user_id = claims.sub.clone();

    // A valid token must not grant a socket for somebody else's user id.
    if path_user_id != user_id {
        tracing::warn!(
            path = %path_user_id,
            jwt = %user_id,
            "WebSocket rejected: user_id mismatch"
        );
        close_with_code(socket, CLOSE_FORBIDDEN, "User ID mismatch").await;
        return;
    }

    match user_service::get_user_by_id(&state.db, &user_id).await {
        Ok(Some(user)) if user.active => {}
        Ok(Some(_)) => {
            tracing::warn!(user_id = %user_id, "WebSocket rejected: account disabled");
            close_with_code(socket, CLOSE_FORBIDDEN, "Account disabled").await;
            return;
        }
        Ok(None) => {
            tracing::warn!(user_id = %user_id, "WebSocket rejected: user not found");
            close_with_code(socket, CLOSE_FORBIDDEN, "User not found").await;
            return;
        }
        Err(e) => {
            tracing::error!(error = %e, "WebSocket rejected: user lookup failed");
            close_with_code(socket, CLOSE_AUTH_REQUIRED, "Authentication failed").await;
            return;
        }
    }

    let (connection_id, mut manager_rx) = match state.ws_manager.connect(&user_id) {
        Ok(conn) => conn,
        Err(e) => {
            tracing::warn!(user_id = %user_id, reason = %e, "WebSocket rejected: connection limit");
            close_with_code(socket, CLOSE_TOO_MANY_CONNECTIONS, "Too many connections").await;
            return;
        }
    };
    tracing::info!(user_id = %user_id, connection_id, "WebSocket connected");

    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Outbound: forward everything the manager publishes, plus keepalive pings.
    let send_task = tokio::spawn(async move {
        let mut ping_interval =
            tokio::time::interval(std::time::Duration::from_secs(PING_INTERVAL_SECS));
        ping_interval.tick().await; // the first tick fires immediately — skip it

        loop {
            tokio::select! {
                msg = manager_rx.recv() => {
                    match msg {
                        Some(json) => {
                            if ws_sender.send(ws::Message::text(json)).await.is_err() {
                                break;
                            }
                        }
                        None => break, // manager dropped the sender
                    }
                }
                _ = ping_interval.tick() => {
                    if ws_sender.send(ws::Message::Ping(Vec::new().into())).await.is_err() {
                        break;
                    }
                }
            }
        }

        let _ = ws_sender.close().await;
    });

    // Inbound: the protocol is server-push, so we only watch for pings and close.
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            match msg {
                ws::Message::Close(_) => break,
                // Axum answers protocol-level Ping frames itself. This is the
                // application-level `{"type":"ping"}` some clients send instead.
                ws::Message::Text(_) | ws::Message::Pong(_) => {}
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = send_task => {}
        _ = recv_task => {}
    }

    state.ws_manager.disconnect(&user_id, connection_id);
    tracing::info!(user_id = %user_id, connection_id, "WebSocket disconnected");
}

/// Close the socket with an application close code before it is ever used.
async fn close_with_code(socket: ws::WebSocket, code: u16, reason: &str) {
    let (mut sender, _) = socket.split();
    let close_frame = ws::CloseFrame {
        code,
        reason: reason.to_string().into(),
    };
    let _ = sender.send(ws::Message::Close(Some(close_frame))).await;
}
