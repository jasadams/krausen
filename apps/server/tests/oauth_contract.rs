// SPDX-License-Identifier: AGPL-3.0-or-later

//! HTTP contract tests for the MCP OAuth endpoints under `/api/v1/oauth`,
//! ported from Kyomi's `contract_oauth_client.rs` and `contract_mcp_oauth_scope.rs`
//! and run against the real router on SQLite and PostgreSQL. Every error path
//! asserts the exact status and body.
//!
//! Differences from Kyomi, all forced by Krausen:
//! - browser sessions are Krausen credentialed sessions
//!   (`session::create_authenticated_session`);
//! - there is no access-token auto-refresh middleware, so Kyomi's "a refreshed
//!   access JWT preserves consent" case becomes "an expired access cookie is
//!   refused with 401 Invalid session";
//! - "ordinary REST" is the authenticated server function `get_sessions`;
//! - a browser refresh needs a real credential family, so the "unbound refresh
//!   token" is a real browser login's refresh token.

mod common;

use common::{Account, TestServer, account, device};
use krausen_auth::{jwt, redis_ops, token_service};
use leptos::server_fn::ServerFn;
use serde_json::{Value, json};

const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const CALLBACK: &str = "https://example.com/callback?existing=1";

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

fn access_cookie(token: &str) -> String {
    format!(
        "{}={token}",
        krausen_core::constants::get().cookies.access_token_name
    )
}

fn authorize_url(srv: &TestServer) -> String {
    format!("{}/api/v1/oauth/authorize", srv.base)
}

fn token_url(srv: &TestServer) -> String {
    format!("{}/api/v1/oauth/token", srv.base)
}

async fn register(srv: &TestServer, redirect_uris: &[&str]) -> String {
    let resp = client()
        .post(format!("{}/api/v1/oauth/register", srv.base))
        .json(&json!({ "redirect_uris": redirect_uris }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "fixture: client registration");
    resp.json::<Value>().await.unwrap()["client_id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn hidden_value(page: &str, name: &str) -> String {
    let marker = format!("name=\"{name}\" value=\"");
    page.split(&marker)
        .nth(1)
        .unwrap_or_else(|| panic!("consent form field {name} in {page}"))
        .split('"')
        .next()
        .unwrap()
        .to_owned()
}

fn consent_callback(response: &reqwest::Response) -> url::Url {
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    let refresh = response.headers()["refresh"].to_str().unwrap();
    url::Url::parse(refresh.strip_prefix("0; url=").expect("immediate navigation")).unwrap()
}

/// GET the consent page as `who`; returns (transaction, csrf, consent cookie pair).
async fn consent_page(
    srv: &TestServer,
    who: &Account,
    client_id: &str,
) -> (String, String, String) {
    let resp = client()
        .get(authorize_url(srv))
        .header("cookie", access_cookie(&who.access_token))
        .query(&[
            ("client_id", client_id),
            ("redirect_uri", CALLBACK),
            ("response_type", "code"),
            ("code_challenge", CHALLENGE),
            ("code_challenge_method", "S256"),
            ("state", "client state & more"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers()["cache-control"], "no-store");
    let set_cookie = resp.headers()["set-cookie"].to_str().unwrap().to_owned();
    for attribute in ["Max-Age=300", "Path=/api/v1/oauth/authorize", "SameSite=Lax", "HttpOnly"] {
        assert!(set_cookie.contains(attribute), "{attribute} in {set_cookie}");
    }
    let consent_cookie = set_cookie.split(';').next().unwrap().to_owned();
    let page = resp.text().await.unwrap();
    assert!(page.contains("Allow MCP access?"));
    assert!(!page.contains("code="));
    let transaction = hidden_value(&page, "transaction");
    assert!(
        consent_cookie.starts_with(&format!("oauth_consent_{transaction}=")),
        "the nonce cookie is bound to this transaction"
    );
    (transaction, hidden_value(&page, "csrf"), consent_cookie)
}

async fn decide(
    srv: &TestServer,
    cookie: String,
    transaction: &str,
    csrf: &str,
    decision: &str,
) -> reqwest::Response {
    client()
        .post(authorize_url(srv))
        .header("cookie", cookie)
        .form(&[("transaction", transaction), ("csrf", csrf), ("decision", decision)])
        .send()
        .await
        .unwrap()
}

fn query_value(url: &url::Url, key: &str) -> String {
    url.query_pairs()
        .find(|(k, _)| k == key)
        .unwrap_or_else(|| panic!("{key} in {url}"))
        .1
        .to_string()
}

/// Approve through the consent page and return the authorization code.
async fn approved_code(srv: &TestServer, who: &Account, client_id: &str) -> String {
    let (transaction, csrf, cookie) = consent_page(srv, who, client_id).await;
    let resp = decide(
        srv,
        format!("{}; {cookie}", access_cookie(&who.access_token)),
        &transaction,
        &csrf,
        "allow",
    )
    .await;
    query_value(&consent_callback(&resp), "code")
}

async fn exchange(srv: &TestServer, client_id: &str, code: &str) -> Value {
    let resp = client()
        .post(token_url(srv))
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("redirect_uri", CALLBACK),
            ("code", code),
            ("code_verifier", VERIFIER),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    resp.json().await.unwrap()
}

async fn error_of(resp: reqwest::Response, status: u16) -> Value {
    assert_eq!(resp.status(), status);
    resp.json().await.unwrap()
}

/// Store an OAuth-bound refresh token (what the code exchange stores).
async fn store_bound_refresh(srv: &TestServer, user_id: &str, client_id: Option<&str>) -> String {
    let raw = jwt::create_refresh_token();
    let mut device = device();
    device.oauth_client_id = client_id.map(str::to_string);
    token_service::store_refresh_token(
        &srv.db,
        user_id,
        &token_service::hash_refresh_token(&raw),
        chrono::Utc::now() + chrono::Duration::days(1),
        &device,
        &token_service::generate_family_id(),
    )
    .await
    .unwrap();
    raw
}

fn sessions_path() -> &'static str {
    <krausen_ui::server_fns::security::GetSessions as ServerFn>::PATH
}

/// "Ordinary REST": an authenticated server function.
async fn rest_status(srv: &TestServer, token: &str) -> u16 {
    client()
        .post(format!("{}{}", srv.base, sessions_path()))
        .header("content-type", "application/x-www-form-urlencoded")
        .bearer_auth(token)
        .body("")
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

// ===========================================================================
// 1. authorize
// ===========================================================================

async fn authorize_rejects_unsupported_response_type(srv: &TestServer) {
    // response_type is checked before client/redirect_uri validation.
    let resp = client()
        .get(authorize_url(srv))
        .query(&[
            ("client_id", "does-not-matter"),
            ("redirect_uri", "https://example.com/callback"),
            ("response_type", "token"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(
        error_of(resp, 400).await,
        json!({"error": "Only response_type=code is supported"})
    );
}

async fn authorize_rejects_unknown_client_id(srv: &TestServer) {
    let resp = client()
        .get(authorize_url(srv))
        .query(&[
            ("client_id", "unknown-client-does-not-exist"),
            ("redirect_uri", "https://example.com/callback"),
            ("response_type", "code"),
            ("code_challenge", CHALLENGE),
            ("code_challenge_method", "S256"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(
        error_of(resp, 400).await,
        json!({"error": "Unknown client_id: unknown-client-does-not-exist"})
    );
}

async fn authorize_rejects_invalid_redirect_uri(srv: &TestServer) {
    let client_id = register(srv, &["https://example.com/callback"]).await;
    let resp = client()
        .get(authorize_url(srv))
        .query(&[
            ("client_id", client_id.as_str()),
            ("redirect_uri", "https://evil.example/steal"),
            ("response_type", "code"),
            ("code_challenge", CHALLENGE),
            ("code_challenge_method", "S256"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(error_of(resp, 400).await, json!({"error": "Invalid redirect_uri"}));
}

async fn authorize_rejects_unsupported_scope(srv: &TestServer) {
    let client_id = register(srv, &[CALLBACK]).await;
    let resp = client()
        .get(authorize_url(srv))
        .query(&[
            ("client_id", client_id.as_str()),
            ("redirect_uri", CALLBACK),
            ("scope", "admin"),
            ("code_challenge", CHALLENGE),
            ("code_challenge_method", "S256"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(error_of(resp, 400).await, json!({"error": "Unsupported scope"}));
}

// ===========================================================================
// 2. authorize/continue
// ===========================================================================

async fn authorize_continue_rejects_missing_session_cookie(srv: &TestServer) {
    let resp = client()
        .get(format!("{}/api/v1/oauth/authorize/continue", srv.base))
        .query(&[("state", "some-pending-state")])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    assert_eq!(resp.text().await.unwrap(), "Not logged in");
}

async fn authorize_continue_rejects_invalid_session_cookie(srv: &TestServer) {
    let resp = client()
        .get(format!("{}/api/v1/oauth/authorize/continue", srv.base))
        .query(&[("state", "some-pending-state")])
        .header("cookie", access_cookie("not-a-real-jwt"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    assert_eq!(resp.text().await.unwrap(), "Invalid session");
}

async fn authorize_continue_rejects_an_unknown_pending_state(srv: &TestServer) {
    let who = account(srv).await;
    let resp = client()
        .get(format!("{}/api/v1/oauth/authorize/continue", srv.base))
        .query(&[("state", "never-issued")])
        .header("cookie", access_cookie(&who.access_token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    assert_eq!(resp.text().await.unwrap(), "Authorization expired");
}

// ===========================================================================
// 3. token
// ===========================================================================

async fn token_rejects_unsupported_grant_type(srv: &TestServer) {
    let client_id = register(srv, &["https://example.com/callback"]).await;
    let resp = client()
        .post(token_url(srv))
        .form(&[("grant_type", "client_credentials"), ("client_id", &client_id)])
        .send()
        .await
        .unwrap();
    assert_eq!(
        error_of(resp, 400).await,
        json!({"error": "Unsupported grant_type: client_credentials"})
    );
}

async fn token_rejects_unknown_client_id(srv: &TestServer) {
    let resp = client()
        .post(token_url(srv))
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", "unknown-client-does-not-exist"),
            ("code", "irrelevant-because-client-lookup-fails-first"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(
        error_of(resp, 400).await,
        json!({"error": "Unknown client_id: unknown-client-does-not-exist"})
    );
}

// ===========================================================================
// 4. register
// ===========================================================================

async fn register_rejects_empty_redirect_uris(srv: &TestServer) {
    let resp = client()
        .post(format!("{}/api/v1/oauth/register", srv.base))
        .json(&json!({ "redirect_uris": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        error_of(resp, 400).await,
        json!({"error": "redirect_uris is required and must not be empty"})
    );
}

async fn registration_rejects_unsafe_callbacks_and_authorize_requires_s256(srv: &TestServer) {
    for uri in [
        "javascript:alert(1)",
        "https://user:pass@example.com/callback",
        "https://example.com/callback#fragment",
        "http://example.com/callback",
    ] {
        let resp = client()
            .post(format!("{}/api/v1/oauth/register", srv.base))
            .json(&json!({"redirect_uris": [uri]}))
            .send()
            .await
            .unwrap();
        assert_eq!(error_of(resp, 400).await, json!({"error": "Invalid redirect_uri"}), "{uri}");
    }
    let id = register(srv, &["http://127.0.0.1:8000/callback"]).await;
    let resp = client()
        .get(authorize_url(srv))
        .query(&[("client_id", id.as_str()), ("redirect_uri", "http://127.0.0.1:8000/callback")])
        .send()
        .await
        .unwrap();
    assert_eq!(
        error_of(resp, 400).await,
        json!({"error": "PKCE S256 code_challenge required"})
    );
}

// ===========================================================================
// 5. consent
// ===========================================================================

async fn logged_in_get_requires_post_consent_and_pkce_to_issue_code(srv: &TestServer) {
    let who = account(srv).await;
    let id = register(srv, &[CALLBACK]).await;
    let (transaction, csrf, consent_cookie) = consent_page(srv, &who, &id).await;
    let cookie = format!("{}; {consent_cookie}", access_cookie(&who.access_token));
    let resp = decide(srv, cookie.clone(), &transaction, &csrf, "allow").await;
    let url = consent_callback(&resp);
    assert_eq!(query_value(&url, "existing"), "1");
    assert_eq!(query_value(&url, "state"), "client state & more");
    let code = query_value(&url, "code");

    // The transaction is single-use.
    let replay = decide(srv, cookie, &transaction, &csrf, "allow").await;
    assert_eq!(replay.status(), 400);
    assert_eq!(replay.text().await.unwrap(), "Consent expired or used");

    let wrong_pkce = client()
        .post(token_url(srv))
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", id.as_str()),
            ("redirect_uri", CALLBACK),
            ("code", code.as_str()),
            ("code_verifier", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(
        error_of(wrong_pkce, 400).await["error"],
        "invalid_grant: PKCE verification failed"
    );
}

async fn consent_denial_csrf_and_browser_binding(srv: &TestServer) {
    let who = account(srv).await;
    let id = register(srv, &[CALLBACK]).await;

    let (transaction, csrf, consent_cookie) = consent_page(srv, &who, &id).await;
    let denied = decide(
        srv,
        format!("{}; {consent_cookie}", access_cookie(&who.access_token)),
        &transaction,
        &csrf,
        "deny",
    )
    .await;
    let location = consent_callback(&denied).to_string();
    assert!(location.contains("error=access_denied"));
    assert!(!location.contains("code="));

    // A wrong CSRF value is refused, and still consumes the transaction.
    let (transaction, csrf, consent_cookie) = consent_page(srv, &who, &id).await;
    let cookie = format!("{}; {consent_cookie}", access_cookie(&who.access_token));
    let csrf_failure = decide(srv, cookie.clone(), &transaction, "bad", "allow").await;
    assert_eq!(csrf_failure.status(), 403);
    assert_eq!(csrf_failure.text().await.unwrap(), "Consent session mismatch");
    let after_failure = decide(srv, cookie, &transaction, &csrf, "allow").await;
    assert_eq!(after_failure.status(), 400, "a failed attempt consumes the transaction");

    // The consent cookie binds the transaction to the browser that rendered it.
    let (transaction, csrf, _) = consent_page(srv, &who, &id).await;
    let wrong_browser =
        decide(srv, access_cookie(&who.access_token), &transaction, &csrf, "allow").await;
    assert_eq!(wrong_browser.status(), 403);

    // A nonce cookie from a different transaction does not satisfy this one.
    let (first, first_csrf, _) = consent_page(srv, &who, &id).await;
    let (_, _, other_cookie) = consent_page(srv, &who, &id).await;
    let swapped = decide(
        srv,
        format!("{}; {other_cookie}", access_cookie(&who.access_token)),
        &first,
        &first_csrf,
        "allow",
    )
    .await;
    assert_eq!(swapped.status(), 403);

    // An invalid decision value is a 400.
    let (transaction, csrf, consent_cookie) = consent_page(srv, &who, &id).await;
    let invalid = decide(
        srv,
        format!("{}; {consent_cookie}", access_cookie(&who.access_token)),
        &transaction,
        &csrf,
        "maybe",
    )
    .await;
    assert_eq!(invalid.status(), 400);
    assert_eq!(invalid.text().await.unwrap(), "Invalid decision");

    // Krausen has no cookie auto-refresh: an expired access token is not a session.
    let (transaction, csrf, consent_cookie) = consent_page(srv, &who, &id).await;
    let expired = jwt::create_access_token_str(&who.user_id, &srv.secret, -5, Default::default())
        .unwrap();
    let expired_session = decide(
        srv,
        format!("{}; {consent_cookie}", access_cookie(&expired)),
        &transaction,
        &csrf,
        "allow",
    )
    .await;
    assert_eq!(expired_session.status(), 401);
    assert_eq!(expired_session.text().await.unwrap(), "Invalid session");
}

async fn consent_decision_requires_a_browser_session(srv: &TestServer) {
    let resp = client()
        .post(authorize_url(srv))
        .form(&[("transaction", "t"), ("csrf", "c"), ("decision", "allow")])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    assert_eq!(resp.text().await.unwrap(), "Not logged in");

    // An MCP access token is not a browser session.
    let who = account(srv).await;
    let mcp = jwt::create_mcp_access_token_str(&who.user_id, &srv.secret, 15, Default::default())
        .unwrap();
    let resp = client()
        .post(authorize_url(srv))
        .header("cookie", access_cookie(&mcp))
        .form(&[("transaction", "t"), ("csrf", "c"), ("decision", "allow")])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    assert_eq!(resp.text().await.unwrap(), "Invalid session");
}

async fn login_continuation_only_renders_consent(srv: &TestServer) {
    let who = account(srv).await;
    let id = register(srv, &[CALLBACK]).await;
    let initial = client()
        .get(authorize_url(srv))
        .query(&[
            ("client_id", id.as_str()),
            ("redirect_uri", CALLBACK),
            ("code_challenge", CHALLENGE),
            ("code_challenge_method", "S256"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(initial.status(), 303);
    let login = url::Url::parse(initial.headers()["location"].to_str().unwrap()).unwrap();
    assert_eq!(login.path(), "/login");
    let state = query_value(&login, "oauth_continue");
    let continued = client()
        .get(format!("{}/api/v1/oauth/authorize/continue", srv.base))
        .header("cookie", access_cookie(&who.access_token))
        .query(&[("state", state.as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(continued.status(), 200);
    assert_eq!(continued.headers()["cache-control"], "no-store");
    assert!(continued.headers().get("set-cookie").is_some(), "consent nonce cookie");
    let page = continued.text().await.unwrap();
    assert!(page.contains("Allow MCP access?"));
    assert!(!page.contains("code="));

    // The pending state is single-use.
    let again = client()
        .get(format!("{}/api/v1/oauth/authorize/continue", srv.base))
        .header("cookie", access_cookie(&who.access_token))
        .query(&[("state", state.as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), 400);
}

/// The consent pages send a scriptless Content-Security-Policy
/// (`style-src 'self'`, `font-src 'self'`, `form-action 'self'`, `frame-ancestors 'none'`).
async fn consent_pages_send_a_scriptless_content_security_policy(srv: &TestServer) {
    let who = account(srv).await;
    let id = register(srv, &[CALLBACK]).await;
    let resp = client()
        .get(authorize_url(srv))
        .header("cookie", access_cookie(&who.access_token))
        .query(&[
            ("client_id", id.as_str()),
            ("redirect_uri", CALLBACK),
            ("code_challenge", CHALLENGE),
            ("code_challenge_method", "S256"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let csp = resp
        .headers()
        .get("content-security-policy")
        .expect("consent page must send a Content-Security-Policy")
        .to_str()
        .unwrap();
    assert!(csp.contains("style-src 'self'"), "{csp}");
    assert!(csp.contains("form-action 'self'"), "{csp}");
    assert!(csp.contains("frame-ancestors 'none'"), "{csp}");

    // The page's fonts are same-origin /public/fonts files (local-fonts.css is
    // imported into the stylesheet), so the policy must allow exactly 'self'.
    let directive = |name: &str| {
        csp.split(';')
            .map(str::trim)
            .find_map(|d| d.strip_prefix(name).map(str::trim))
            .map(str::to_owned)
    };
    assert_eq!(directive("font-src").as_deref(), Some("'self'"), "{csp}");
    assert!(!csp.contains("gstatic"), "no external font host: {csp}");
    let fonts = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/krausen-ui/style/local-fonts.css"
    ))
    .unwrap();
    let urls: Vec<&str> = fonts
        .split("url(")
        .skip(1)
        .filter_map(|rest| rest.split(')').next())
        .map(|url| url.trim().trim_matches(|c| c == '\'' || c == '"'))
        .collect();
    assert!(!urls.is_empty(), "local-fonts.css declares fonts");
    for url in urls {
        assert!(
            url.starts_with("/public/fonts/"),
            "font {url} must be same-origin so font-src 'self' permits it"
        );
    }
}

async fn different_user_cannot_approve_another_users_transaction(srv: &TestServer) {
    let owner = account(srv).await;
    let other = account(srv).await;
    let id = register(srv, &[CALLBACK]).await;
    let (transaction, csrf, consent_cookie) = consent_page(srv, &owner, &id).await;
    let response = decide(
        srv,
        format!("{}; {consent_cookie}", access_cookie(&other.access_token)),
        &transaction,
        &csrf,
        "allow",
    )
    .await;
    assert_eq!(response.status(), 403);
}

// ===========================================================================
// 6. code exchange and refresh
// ===========================================================================

async fn code_exchange_requires_exact_redirect_and_correct_pkce(srv: &TestServer) {
    let who = account(srv).await;
    let id = register(srv, &[CALLBACK]).await;
    let code = approved_code(srv, &who, &id).await;

    let missing_redirect = client()
        .post(token_url(srv))
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", id.as_str()),
            ("code", code.as_str()),
            ("code_verifier", VERIFIER),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(
        error_of(missing_redirect, 400).await["error"],
        "redirect_uri required"
    );

    let tokens = exchange(srv, &id, &code).await;
    assert!(tokens["access_token"].as_str().is_some());
    assert_eq!(tokens["scope"], "mcp");
    assert_eq!(tokens["token_type"], "Bearer");
    let refresh = tokens["refresh_token"].as_str().unwrap();

    // The code was consumed by the exchange.
    let reused = client()
        .post(token_url(srv))
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", id.as_str()),
            ("redirect_uri", CALLBACK),
            ("code", code.as_str()),
            ("code_verifier", VERIFIER),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(
        error_of(reused, 400).await["error"],
        "invalid_grant: code expired or invalid"
    );

    let other_id = register(srv, &["https://example.com/other"]).await;
    let cross_client = client()
        .post(token_url(srv))
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", other_id.as_str()),
            ("refresh_token", refresh),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(
        error_of(cross_client, 400).await,
        json!({"error": "invalid_grant: client_id mismatch"})
    );
    let same_client = client()
        .post(token_url(srv))
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", id.as_str()),
            ("refresh_token", refresh),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(same_client.status(), 200);
    let refreshed: Value = same_client.json().await.unwrap();
    assert!(refreshed["access_token"].as_str().is_some());
    assert_eq!(refreshed["refresh_token"], refresh, "the refresh token does not rotate");
}

async fn code_exchange_refuses_another_client_and_a_mismatched_redirect(srv: &TestServer) {
    let who = account(srv).await;
    let id = register(srv, &[CALLBACK]).await;
    let other = register(srv, &[CALLBACK]).await;

    let code = approved_code(srv, &who, &id).await;
    let wrong_client = client()
        .post(token_url(srv))
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", other.as_str()),
            ("redirect_uri", CALLBACK),
            ("code", code.as_str()),
            ("code_verifier", VERIFIER),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(
        error_of(wrong_client, 400).await,
        json!({"error": "invalid_grant: client_id mismatch"})
    );

    let code = approved_code(srv, &who, &id).await;
    let wrong_redirect = client()
        .post(token_url(srv))
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", id.as_str()),
            ("redirect_uri", "https://example.com/callback"),
            ("code", code.as_str()),
            ("code_verifier", VERIFIER),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(
        error_of(wrong_redirect, 400).await,
        json!({"error": "invalid_grant: redirect_uri mismatch"})
    );
}

async fn refresh_grant_is_bound_to_issuing_client(srv: &TestServer) {
    let who = account(srv).await;
    let client_a = register(srv, &[CALLBACK]).await;
    let client_b = register(srv, &["https://example.com/other"]).await;
    let refresh = store_bound_refresh(srv, &who.user_id, Some(&client_a)).await;

    let wrong_client = client()
        .post(token_url(srv))
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_b.as_str()),
            ("refresh_token", refresh.as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(
        error_of(wrong_client, 400).await,
        json!({"error": "invalid_grant: client_id mismatch"})
    );

    // A rejected cross-client attempt must leave A's grant usable; the same
    // refresh token is returned on every successful refresh.
    for _ in 0..2 {
        let same_client = client()
            .post(token_url(srv))
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", client_a.as_str()),
                ("refresh_token", refresh.as_str()),
            ])
            .send()
            .await
            .unwrap();
        assert_eq!(same_client.status(), 200);
        let body: Value = same_client.json().await.unwrap();
        assert!(body["access_token"].as_str().is_some());
        assert_eq!(body["refresh_token"], refresh);
    }

    krausen_core::db_execute!(
        &srv.db,
        "UPDATE oauth_clients SET active = $1 WHERE client_id = $2",
        &false,
        &client_a
    )
    .unwrap();
    let inactive_client = client()
        .post(token_url(srv))
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_a.as_str()),
            ("refresh_token", refresh.as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(
        error_of(inactive_client, 400).await["error"],
        format!("Unknown client_id: {client_a}")
    );
}

async fn refresh_grant_slides_the_inactivity_window_and_honours_revocation(srv: &TestServer) {
    #[derive(sqlx::FromRow)]
    struct Expiry {
        expires_at: chrono::DateTime<chrono::Utc>,
    }
    let who = account(srv).await;
    let id = register(srv, &[CALLBACK]).await;
    let refresh = store_bound_refresh(srv, &who.user_id, Some(&id)).await;
    let hash = token_service::hash_refresh_token(&refresh);
    let before = krausen_core::db_fetch_one!(
        &srv.db,
        Expiry,
        "SELECT expires_at FROM refresh_tokens WHERE token_hash = $1",
        &hash
    )
    .unwrap()
    .expires_at;

    let refresh_with = |client_id: String, token: String| {
        let url = token_url(srv);
        async move {
            client()
                .post(url)
                .form(&[
                    ("grant_type", "refresh_token"),
                    ("client_id", client_id.as_str()),
                    ("refresh_token", token.as_str()),
                ])
                .send()
                .await
                .unwrap()
        }
    };
    assert_eq!(refresh_with(id.clone(), refresh.clone()).await.status(), 200);
    let after = krausen_core::db_fetch_one!(
        &srv.db,
        Expiry,
        "SELECT expires_at FROM refresh_tokens WHERE token_hash = $1",
        &hash
    )
    .unwrap()
    .expires_at;
    assert!(after > before, "a successful refresh renews the expiry ({before} -> {after})");

    // A revoked grant cannot be revived by presenting it again.
    token_service::revoke_all_user_sessions(&srv.db, &who.user_id)
        .await
        .unwrap();
    let revoked = refresh_with(id.clone(), refresh.clone()).await;
    assert_eq!(
        error_of(revoked, 400).await["error"],
        "invalid_grant: refresh token invalid or expired"
    );
    // ... and an unknown token is refused the same way.
    let unknown = refresh_with(id, "rt_not-a-token".into()).await;
    assert_eq!(
        error_of(unknown, 400).await["error"],
        "invalid_grant: refresh token invalid or expired"
    );
}

async fn oauth_refresh_rejects_unbound_token_but_browser_refresh_succeeds(srv: &TestServer) {
    let who = account(srv).await;
    let client_id = register(srv, &[CALLBACK]).await;
    // The browser login's own refresh token carries no OAuth client marker.
    let oauth = client()
        .post(token_url(srv))
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id.as_str()),
            ("refresh_token", who.refresh_token.as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(
        error_of(oauth, 400).await,
        json!({"error": "invalid_grant: refresh token is not an OAuth client token"})
    );

    let refresh_cookie = format!(
        "{}={}",
        krausen_core::constants::get().cookies.refresh_token_name,
        who.refresh_token
    );
    let browser = client()
        .post(format!("{}/api/v1/auth/refresh", srv.base))
        .header("cookie", refresh_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(browser.status(), 200);
    let body: Value = browser.json().await.unwrap();
    assert!(body["access_token"].as_str().is_some());
}

// ===========================================================================
// 7. the MCP token's scope
// ===========================================================================

async fn assert_mcp_only(srv: &TestServer, who: &Account, token: &str) {
    let mcp = client()
        .post(format!("{}/mcp", srv.base))
        .header("accept", "application/json, text/event-stream")
        .bearer_auth(token)
        .json(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-03-26", "capabilities": {},
                       "clientInfo": {"name": "oauth-contract", "version": "1"}}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(mcp.status(), 200, "MCP OAuth token must authorize MCP initialize");
    assert_eq!(
        rest_status(srv, token).await,
        401,
        "MCP OAuth token must not authorize ordinary REST"
    );
    let browser = client()
        .get(format!("{}/api/v1/oauth/authorize/continue?state=unused", srv.base))
        .header("cookie", access_cookie(token))
        .send()
        .await
        .unwrap();
    assert_eq!(
        browser.status(),
        401,
        "MCP OAuth token must not become a browser session"
    );
    assert_eq!(
        rest_status(srv, &who.access_token).await,
        200,
        "the normal session still authorizes REST"
    );
}

async fn exchanged_and_refreshed_mcp_tokens_cannot_authorize_browser_or_rest(srv: &TestServer) {
    let who = account(srv).await;
    let client_id = register(srv, &["https://example.com/callback"]).await;

    // Seed a valid authorization code so the exchange is exercised on its own.
    let code = redis_ops::generate_token();
    redis_ops::store_oauth_state(
        &srv.kv,
        "oauth_code",
        &code,
        &json!({
            "user_id": who.user_id,
            "workspace_id": who.workspace_id,
            "client_id": client_id,
            "redirect_uri": "https://example.com/callback",
            "code_challenge": CHALLENGE,
        }),
    )
    .await
    .unwrap();
    let exchanged: Value = client()
        .post(token_url(srv))
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", client_id.as_str()),
            ("code", code.as_str()),
            ("redirect_uri", "https://example.com/callback"),
            ("code_verifier", VERIFIER),
        ])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(exchanged["scope"], "mcp");
    let issued = exchanged["access_token"].as_str().unwrap();
    let refresh = exchanged["refresh_token"].as_str().unwrap();
    let claims = jwt::validate_token(issued, &srv.secret).unwrap().claims;
    assert_eq!(claims.purpose, Some(jwt::TokenPurpose::Mcp));
    assert_eq!(claims.sub, who.user_id);
    assert_eq!(claims.extra["workspace_id"], who.workspace_id.as_str());
    assert_mcp_only(srv, &who, issued).await;

    // An MCP refresh token must not be exchangeable at the browser refresh route.
    let browser_refresh = client()
        .post(format!("{}/api/v1/auth/refresh", srv.base))
        .header(
            "cookie",
            format!(
                "{}={refresh}",
                krausen_core::constants::get().cookies.refresh_token_name
            ),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(browser_refresh.status(), 401);

    let refreshed: Value = client()
        .post(token_url(srv))
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id.as_str()),
            ("refresh_token", refresh),
        ])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(refreshed["scope"], "mcp");
    assert_mcp_only(srv, &who, refreshed["access_token"].as_str().unwrap()).await;
}

// ===========================================================================
// 8. session cutoff
// ===========================================================================

async fn logout_all_clears_cookies_and_revokes_browser_oauth_authority(srv: &TestServer) {
    let who = account(srv).await;
    let client_id = register(srv, &[CALLBACK]).await;
    let other = krausen_auth::session::create_authenticated_session(
        &srv.db,
        &srv.kv,
        &srv.secret,
        &who.user,
        &device(),
    )
    .await
    .unwrap();
    let (transaction, csrf, consent_cookie) = consent_page(srv, &who, &client_id).await;

    let path = <krausen_ui::server_fns::security::LogoutAllSessions as ServerFn>::PATH;
    let logout = client()
        .post(format!("{}{path}", srv.base))
        .header("content-type", "application/x-www-form-urlencoded")
        .header(
            "cookie",
            format!(
                "{}={}; {}={}",
                krausen_core::constants::get().cookies.access_token_name,
                who.access_token,
                krausen_core::constants::get().cookies.refresh_token_name,
                who.refresh_token
            ),
        )
        .body("")
        .send()
        .await
        .unwrap();
    assert_eq!(logout.status(), 200, "the actual logout-all server function succeeds");
    let cookies: Vec<_> = logout
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|header| header.to_str().unwrap().to_owned())
        .collect();
    for name in [
        &krausen_core::constants::get().cookies.access_token_name,
        &krausen_core::constants::get().cookies.refresh_token_name,
    ] {
        assert!(
            cookies
                .iter()
                .any(|c| c.starts_with(&format!("{name}=")) && c.contains("Max-Age=0")),
            "logout-all clears {name}: {cookies:?}"
        );
    }
    for token in [&who.access_token, &other.access_token] {
        assert_eq!(
            rest_status(srv, token).await,
            401,
            "all pre-event signed browser access tokens must fail"
        );
    }
    for refresh in [&who.refresh_token, &other.refresh_token] {
        assert!(
            krausen_auth::token_refresh::refresh_tokens(&srv.db, &srv.secret, refresh, &device())
                .await
                .is_err()
        );
    }

    let authorize = client()
        .get(authorize_url(srv))
        .header("cookie", access_cookie(&who.access_token))
        .query(&[
            ("client_id", client_id.as_str()),
            ("redirect_uri", CALLBACK),
            ("response_type", "code"),
            ("code_challenge", CHALLENGE),
            ("code_challenge_method", "S256"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(
        authorize.status(),
        303,
        "a revoked cookie must return to login rather than consent"
    );
    assert!(authorize.headers()["location"].to_str().unwrap().contains("/login"));

    let continuation = client()
        .get(format!("{}/api/v1/oauth/authorize/continue?state=unused", srv.base))
        .header("cookie", access_cookie(&who.access_token))
        .send()
        .await
        .unwrap();
    assert_eq!(continuation.status(), 401, "a revoked cookie cannot resume OAuth");

    let consent = decide(
        srv,
        format!("{}; {consent_cookie}", access_cookie(&who.access_token)),
        &transaction,
        &csrf,
        "allow",
    )
    .await;
    assert_eq!(
        consent.status(),
        401,
        "pre-event consent cannot grant OAuth access after logout-all"
    );

    let fresh = krausen_auth::session::create_authenticated_session(
        &srv.db,
        &srv.kv,
        &srv.secret,
        &who.user,
        &device(),
    )
    .await
    .unwrap();
    assert_eq!(
        rest_status(srv, &fresh.access_token).await,
        200,
        "a fresh login works immediately after logout-all"
    );
    krausen_auth::token_refresh::refresh_tokens(&srv.db, &srv.secret, &fresh.refresh_token, &device())
        .await
        .unwrap();
}

common::both_backends!(
    authorize_rejects_unsupported_response_type,
    authorize_rejects_unknown_client_id,
    authorize_rejects_invalid_redirect_uri,
    authorize_rejects_unsupported_scope,
    authorize_continue_rejects_missing_session_cookie,
    authorize_continue_rejects_invalid_session_cookie,
    authorize_continue_rejects_an_unknown_pending_state,
    token_rejects_unsupported_grant_type,
    token_rejects_unknown_client_id,
    register_rejects_empty_redirect_uris,
    registration_rejects_unsafe_callbacks_and_authorize_requires_s256,
    logged_in_get_requires_post_consent_and_pkce_to_issue_code,
    consent_denial_csrf_and_browser_binding,
    consent_decision_requires_a_browser_session,
    login_continuation_only_renders_consent,
    consent_pages_send_a_scriptless_content_security_policy,
    different_user_cannot_approve_another_users_transaction,
    code_exchange_requires_exact_redirect_and_correct_pkce,
    code_exchange_refuses_another_client_and_a_mismatched_redirect,
    refresh_grant_is_bound_to_issuing_client,
    refresh_grant_slides_the_inactivity_window_and_honours_revocation,
    oauth_refresh_rejects_unbound_token_but_browser_refresh_succeeds,
    exchanged_and_refreshed_mcp_tokens_cannot_authorize_browser_or_rest,
    logout_all_clears_cookies_and_revokes_browser_oauth_authority,
);
