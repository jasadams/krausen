// SPDX-License-Identifier: AGPL-3.0-or-later

//! `/mcp` authentication over real HTTP, on SQLite and PostgreSQL: an MCP OAuth
//! access token (signed JWT with `purpose = mcp`) or a browser session is
//! accepted; everything else is a 401 carrying the RFC 6750 / RFC 9728
//! `WWW-Authenticate` challenge.

mod common;

use common::{Account, TestServer, account};
use krausen_auth::{jwt, token_service};
use reqwest::StatusCode;
use serde_json::json;

fn initialize_body() -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {"name": "mcp-authenticate-test", "version": "1"}
        }
    })
}

async fn post_mcp(base: &str, bearer: Option<&str>) -> reqwest::Response {
    let mut request = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .header("accept", "application/json, text/event-stream")
        .json(&initialize_body());
    if let Some(token) = bearer {
        request = request.bearer_auth(token);
    }
    request.send().await.unwrap()
}

fn mcp_token(srv: &TestServer, who: &Account) -> String {
    jwt::create_mcp_access_token_str(&who.user_id, &srv.secret, 15, Default::default()).unwrap()
}

/// The exact challenge the layer must add to every 401.
fn challenge(scheme: &str, base: &str) -> String {
    let host = base.trim_start_matches("http://");
    format!(
        r#"Bearer realm="OAuth", resource_metadata="{scheme}://{host}/.well-known/oauth-protected-resource", error="invalid_token", error_description="Missing or invalid access token""#
    )
}

async fn assert_unauthorized(srv: &TestServer, bearer: Option<&str>, why: &str) {
    let response = post_mcp(&srv.base, bearer).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{why}");
    assert_eq!(
        response.headers()["www-authenticate"].to_str().unwrap(),
        challenge("https", &srv.base),
        "{why}: 401 must carry the OAuth challenge"
    );
}

async fn no_token_is_401_with_the_oauth_challenge(srv: &TestServer) {
    assert_unauthorized(srv, None, "no token").await;
    assert_unauthorized(srv, Some("not-a-jwt"), "garbage bearer").await;

    // Behind a proxy the challenge names the scheme the client used.
    let response = reqwest::Client::new()
        .post(format!("{}/mcp", srv.base))
        .header("accept", "application/json, text/event-stream")
        .header("x-forwarded-proto", "http")
        .json(&initialize_body())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.headers()["www-authenticate"].to_str().unwrap(),
        challenge("http", &srv.base)
    );

    // Only 401s carry the challenge.
    let who = account(srv).await;
    let ok = post_mcp(&srv.base, Some(&mcp_token(srv, &who))).await;
    assert_eq!(ok.status(), StatusCode::OK);
    assert!(ok.headers().get("www-authenticate").is_none());
}

async fn mcp_jwt_for_an_active_user_is_accepted(srv: &TestServer) {
    let who = account(srv).await;
    let token = mcp_token(srv, &who);
    assert_eq!(post_mcp(&srv.base, Some(&token)).await.status(), StatusCode::OK);

    // The Bearer scheme is case-insensitive (RFC 7235).
    let response = reqwest::Client::new()
        .post(format!("{}/mcp", srv.base))
        .header("accept", "application/json, text/event-stream")
        .header("authorization", format!("bearer {token}"))
        .json(&initialize_body())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn mcp_jwt_for_a_deactivated_user_is_401(srv: &TestServer) {
    let who = account(srv).await;
    let token = mcp_token(srv, &who);
    assert_eq!(post_mcp(&srv.base, Some(&token)).await.status(), StatusCode::OK);
    krausen_core::db_execute!(
        &srv.db,
        "UPDATE users SET active = $1 WHERE user_id = $2",
        &false,
        &who.user_id
    )
    .unwrap();
    assert_unauthorized(srv, Some(&token), "deactivated user").await;
}

async fn mcp_jwt_for_an_unknown_user_is_401(srv: &TestServer) {
    let token =
        jwt::create_mcp_access_token_str("user-does-not-exist", &srv.secret, 15, Default::default())
            .unwrap();
    assert_unauthorized(srv, Some(&token), "unknown user").await;
}

async fn mcp_jwt_signed_with_the_wrong_secret_is_401(srv: &TestServer) {
    let who = account(srv).await;
    let forged = jwt::create_mcp_access_token_str(
        &who.user_id,
        "another-secret-at-least-32-bytes-long",
        15,
        Default::default(),
    )
    .unwrap();
    assert_unauthorized(srv, Some(&forged), "wrong secret").await;
}

async fn expired_mcp_jwt_is_401(srv: &TestServer) {
    let who = account(srv).await;
    // Expired well beyond the validator's clock-skew leeway.
    let expired =
        jwt::create_mcp_access_token_str(&who.user_id, &srv.secret, -5, Default::default()).unwrap();
    assert_unauthorized(srv, Some(&expired), "expired MCP token").await;
}

/// A validly signed token without a purpose is a browser session: a valid access
/// token is enough (there is no credential row to bind it to).
async fn signed_session_token_is_accepted_without_a_credential_row(srv: &TestServer) {
    let who = account(srv).await;
    let unbound =
        jwt::create_access_token_str(&who.user_id, &srv.secret, 15, Default::default()).unwrap();
    assert_eq!(post_mcp(&srv.base, Some(&unbound)).await.status(), StatusCode::OK);
    let session = jwt::create_session_access_token_str(
        &who.user_id,
        &srv.secret,
        15,
        Default::default(),
        None,
    )
    .unwrap();
    assert_eq!(post_mcp(&srv.base, Some(&session)).await.status(), StatusCode::OK);
}

async fn browser_access_token_is_accepted(srv: &TestServer) {
    let who = account(srv).await;
    assert_eq!(
        post_mcp(&srv.base, Some(&who.access_token)).await.status(),
        StatusCode::OK
    );
}

/// Revoke-all ends browser sessions on `/mcp`; the OAuth grant has its own
/// lifecycle (refresh-token revocation, account deactivation) and is unaffected.
async fn revoke_all_sessions_ends_browser_access_but_not_the_mcp_grant(srv: &TestServer) {
    let who = account(srv).await;
    let mcp = mcp_token(srv, &who);
    assert_eq!(post_mcp(&srv.base, Some(&who.access_token)).await.status(), StatusCode::OK);
    assert_eq!(post_mcp(&srv.base, Some(&mcp)).await.status(), StatusCode::OK);

    token_service::revoke_all_user_sessions(&srv.db, &who.user_id)
        .await
        .unwrap();

    assert_unauthorized(srv, Some(&who.access_token), "browser session after revoke-all").await;
    assert_eq!(post_mcp(&srv.base, Some(&mcp)).await.status(), StatusCode::OK);
}

/// Another endpoint's secret-bearing material is not an MCP credential: a bare
/// session cookie, no Authorization header, is how the browser authenticates
/// and is accepted; an MCP token in the cookie jar is not a session.
async fn browser_cookie_is_accepted_and_mcp_token_in_a_cookie_is_not(srv: &TestServer) {
    let who = account(srv).await;
    let cookie_name = &krausen_core::constants::get().cookies.access_token_name;
    let with_cookie = |token: String| {
        let base = srv.base.clone();
        let cookie = format!("{cookie_name}={token}");
        async move {
            reqwest::Client::new()
                .post(format!("{base}/mcp"))
                .header("accept", "application/json, text/event-stream")
                .header("cookie", cookie)
                .json(&initialize_body())
                .send()
                .await
                .unwrap()
        }
    };
    assert_eq!(with_cookie(who.access_token.clone()).await.status(), StatusCode::OK);
    let mcp = mcp_token(srv, &who);
    assert_eq!(with_cookie(mcp).await.status(), StatusCode::UNAUTHORIZED);
}

/// Tokens issued before MCP OAuth moved to signed JWTs were opaque random
/// strings (the old `mcp_`-prefixed credential id or the bare secret). They are
/// not JWTs, so they are refused with the OAuth challenge and the client
/// re-registers.
async fn old_opaque_bearer_tokens_are_401_with_the_challenge(srv: &TestServer) {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let secret = URL_SAFE_NO_PAD.encode(uuid::Uuid::new_v4().as_bytes());
    assert_unauthorized(srv, Some(&secret), "old opaque bearer").await;
    assert_unauthorized(srv, Some(&format!("mcp_{secret}")), "old opaque credential id").await;
}

/// A backend failure while resolving the caller is a 500 with the shared JSON
/// error body, never a 401: a transient database error must not make a client
/// discard a valid token.
async fn backend_failure_is_500_with_a_json_error_not_401(srv: &TestServer) {
    let who = account(srv).await;
    let mcp = mcp_token(srv, &who);
    krausen_core::db_execute!(&srv.db, "ALTER TABLE users RENAME TO users_unavailable").unwrap();
    for (token, why) in [(mcp.as_str(), "MCP token"), (who.access_token.as_str(), "browser token")] {
        let response = post_mcp(&srv.base, Some(token)).await;
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR, "{why}");
        assert!(
            response.headers().get("www-authenticate").is_none(),
            "{why}: a 500 carries no token challenge"
        );
        assert_eq!(
            response.json::<serde_json::Value>().await.unwrap(),
            json!({"detail": "internal server error"}),
            "{why}"
        );
    }
}

/// A token that names a workspace is refused at authentication once its user is
/// removed from that workspace (401 + challenge), as for a browser session. A
/// token naming no workspace still authenticates, and `authorize_workspace`
/// then refuses each call against a workspace the user does not belong to.
async fn removed_member_with_an_mcp_token_is_refused(srv: &TestServer) {
    use rmcp::{
        model::{CallToolRequestParams, ClientConfig, ProtocolVersion},
        service::{ClientLifecycleMode, ClientServiceExt},
        transport::{
            StreamableHttpClientTransport,
            streamable_http_client::StreamableHttpClientTransportConfig,
        },
    };
    let who = account(srv).await;
    let mut extra = serde_json::Map::new();
    extra.insert("workspace_id".into(), json!(who.workspace_id));
    let named =
        jwt::create_mcp_access_token_str(&who.user_id, &srv.secret, 15, extra.into_iter().collect())
            .unwrap();
    let unnamed = mcp_token(srv, &who);
    let client = ClientConfig::default()
        .with_protocol_version(ProtocolVersion::V_2025_11_25)
        .serve_with_lifecycle(
            StreamableHttpClientTransport::from_config(
                StreamableHttpClientTransportConfig::with_uri(format!("{}/mcp", srv.base))
                    .auth_header(unnamed.clone()),
            ),
            ClientLifecycleMode::Initialize,
        )
        .await
        .unwrap();
    let args = json!({
        "workspace_id": who.workspace_id,
        "intent": {"account_id": who.user_id, "workspace_id": who.workspace_id},
    })
    .as_object()
    .unwrap()
    .clone();

    // The MCP-token principal (no browser credential) drives a real tool.
    let allowed = client
        .call_tool(CallToolRequestParams::new("formulation_styles").with_arguments(args.clone()))
        .await
        .unwrap();
    assert_ne!(
        allowed.structured_content.as_ref().unwrap()["outcome"],
        "Forbidden",
        "a member's MCP token reaches the operation"
    );
    assert_eq!(post_mcp(&srv.base, Some(&named)).await.status(), StatusCode::OK);

    krausen_core::db_execute!(
        &srv.db,
        "UPDATE workspace_users SET active = $1 WHERE workspace_id = $2 AND user_id = $3",
        &false,
        &who.workspace_id,
        &who.user_id
    )
    .unwrap();

    assert_unauthorized(srv, Some(&named), "removed member, token names the workspace").await;

    assert_eq!(post_mcp(&srv.base, Some(&unnamed)).await.status(), StatusCode::OK);
    let denied = client
        .call_tool(CallToolRequestParams::new("formulation_styles").with_arguments(args))
        .await
        .unwrap();
    assert_eq!(denied.is_error, Some(true));
    assert_eq!(denied.structured_content.unwrap()["outcome"], "Forbidden");
}

common::both_backends!(
    no_token_is_401_with_the_oauth_challenge,
    mcp_jwt_for_an_active_user_is_accepted,
    mcp_jwt_for_a_deactivated_user_is_401,
    mcp_jwt_for_an_unknown_user_is_401,
    mcp_jwt_signed_with_the_wrong_secret_is_401,
    expired_mcp_jwt_is_401,
    signed_session_token_is_accepted_without_a_credential_row,
    browser_access_token_is_accepted,
    revoke_all_sessions_ends_browser_access_but_not_the_mcp_grant,
    browser_cookie_is_accepted_and_mcp_token_in_a_cookie_is_not,
    old_opaque_bearer_tokens_are_401_with_the_challenge,
    backend_failure_is_500_with_a_json_error_not_401,
    removed_member_with_an_mcp_token_is_refused,
);
