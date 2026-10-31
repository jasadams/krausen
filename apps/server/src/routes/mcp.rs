// SPDX-License-Identifier: AGPL-3.0-or-later

//! MCP Server endpoints — JSON-RPC over Streamable HTTP.
//!
//! Implements the MCP 2025-03-26 spec with an empty tool registry.
//! Domain-specific tools are added when domain code is written.
//!
//! ## Endpoints
//!
//! - `POST /mcp` — JSON-RPC 2.0 request/response (initialize, tools/list, tools/call)
//! - `GET  /mcp` — SSE stream for server-initiated notifications
//! - `DELETE /mcp` — Terminate an MCP session

use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{delete, get, post},
    Json, Router,
};
use futures_util::stream;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::state::AppState;

const MCP_PROTOCOL_VERSION: &str = "2025-03-26";
const MCP_SERVER_NAME: &str = "krausen-mcp";
const MCP_SERVER_VERSION: &str = "0.1.0";
const MCP_SESSION_ID_HEADER: &str = "mcp-session-id";

/// Comment-only heartbeat frames keep proxies from closing an idle stream.
const SSE_HEARTBEAT_SECONDS: u64 = 30;

/// Buffered server -> client notifications before the stream applies backpressure.
const SSE_CHANNEL_CAPACITY: usize = 32;

// ─────────────────────────────────────────────────────────────────────────────
// JSON-RPC 2.0 types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

impl JsonRpcResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self { jsonrpc: "2.0", id, result: Some(result), error: None }
    }

    fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError { code, message: message.into() }),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Routes
// ─────────────────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", post(handle_post))
        .route("/", get(handle_sse))
        .route("/", delete(handle_delete))
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /mcp — JSON-RPC request handler
// ─────────────────────────────────────────────────────────────────────────────

async fn handle_post(
    State(state): State<AppState>,
    _headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    let is_initialize = request.method == "initialize";

    let response = match request.method.as_str() {
        "initialize" => handle_initialize(request.id, request.params),
        "notifications/initialized" => {
            return (StatusCode::ACCEPTED, "").into_response();
        }
        "tools/list" => handle_tools_list(request.id),
        "tools/call" => handle_tools_call(request.id, request.params),
        "resources/list" => handle_resources_list(request.id),
        "ping" => JsonRpcResponse::success(request.id, json!({})),
        _ => JsonRpcResponse::error(request.id, -32601, format!("Method not found: {}", request.method)),
    };

    let mut resp_headers = HeaderMap::new();

    if is_initialize {
        let session_id = state.mcp_sessions.create_session("anonymous").await;
        if let Ok(val) = session_id.parse() {
            resp_headers.insert(MCP_SESSION_ID_HEADER, val);
        }
    }

    (StatusCode::OK, resp_headers, Json(response)).into_response()
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /mcp — SSE stream for server-initiated notifications
// ─────────────────────────────────────────────────────────────────────────────

/// Open the server -> client notification stream.
///
/// Requires an `Mcp-Session-Id` header, obtained from the `initialize` response.
/// A session the server no longer knows about (restart, eviction) is re-created
/// rather than rejected — the client would otherwise be stuck retrying a session
/// it cannot recover on its own.
///
/// The stream stays open and carries whatever the session manager publishes —
/// `notifications/tools/list_changed`, for instance — with periodic heartbeats
/// so intermediaries do not drop an idle connection.
async fn handle_sse(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let requested_session = headers
        .get(MCP_SESSION_ID_HEADER)
        .and_then(|v| v.to_str().ok());

    // A session id the server does not recognise is healed into a new one, so
    // refusing a request that carries none would be the stricter of the two —
    // presenting a bogus id would get further than presenting nothing. Both
    // mean the same thing here: the client has no usable session yet, and the
    // server can give it one.
    let session_id = match requested_session {
        Some(id) if state.mcp_sessions.validate_session(id).await.is_some() => id.to_string(),
        other => {
            let new_id = state.mcp_sessions.create_session("anonymous").await;
            tracing::info!(
                old_session_id = other.unwrap_or("<none>"),
                new_session_id = %new_id,
                "MCP SSE session issued"
            );
            new_id
        }
    };

    let (tx, rx) = mpsc::channel::<String>(SSE_CHANNEL_CAPACITY);
    state.mcp_sessions.set_sse_sender(&session_id, tx);

    tracing::info!(session_id = %session_id, "MCP SSE stream opened");

    // An SSE response's headers are not flushed until something is written, so
    // a stream that stays silent until the first notification leaves the client
    // waiting with no way to tell a live stream from a stalled one — and any
    // proxy in between free to decide the connection is idle. Lead with a ping
    // so the stream announces itself the moment it opens.
    let hello = stream::once(async {
        Ok::<_, Infallible>(Event::default().event("ping").data("{}"))
    });

    let notifications = stream::unfold(rx, |mut rx| async move {
        let msg = rx.recv().await?;
        Some((Ok::<_, Infallible>(Event::default().data(msg)), rx))
    });

    let sse = Sse::new(hello.chain(notifications))
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(SSE_HEARTBEAT_SECONDS)));

    let mut resp = sse.into_response();
    if let Ok(val) = session_id.parse() {
        resp.headers_mut().insert(MCP_SESSION_ID_HEADER, val);
    }
    resp
}

// ─────────────────────────────────────────────────────────────────────────────
// DELETE /mcp — session termination
// ─────────────────────────────────────────────────────────────────────────────

async fn handle_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(session_id) = headers.get(MCP_SESSION_ID_HEADER).and_then(|v| v.to_str().ok()) {
        state.mcp_sessions.remove_session(session_id).await;
    }
    StatusCode::NO_CONTENT
}

// ─────────────────────────────────────────────────────────────────────────────
// Method handlers
// ─────────────────────────────────────────────────────────────────────────────

fn handle_initialize(id: Option<Value>, _params: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(id, json!({
        "protocolVersion": MCP_PROTOCOL_VERSION,
        "serverInfo": {
            "name": MCP_SERVER_NAME,
            "version": MCP_SERVER_VERSION,
        },
        "capabilities": {
            "tools": { "listChanged": true },
            "resources": { "subscribe": false, "listChanged": false },
        },
    }))
}

fn handle_tools_list(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(id, json!({
        "tools": [
            {
                "name": "hello",
                "description": "Returns a greeting message. Use this to verify the MCP connection is working.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Name to greet"
                        }
                    }
                }
            }
        ]
    }))
}

fn handle_tools_call(id: Option<Value>, params: Option<Value>) -> JsonRpcResponse {
    let tool_name = params
        .as_ref()
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("unknown");

    match tool_name {
        "hello" => {
            let name = params
                .as_ref()
                .and_then(|p| p.get("arguments"))
                .and_then(|a| a.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("world");

            JsonRpcResponse::success(id, json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Hello, {name}! Krausen MCP server is running.")
                    }
                ]
            }))
        }
        _ => JsonRpcResponse::error(id, -32602, format!("Unknown tool: {tool_name}")),
    }
}

fn handle_resources_list(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(id, json!({ "resources": [] }))
}
