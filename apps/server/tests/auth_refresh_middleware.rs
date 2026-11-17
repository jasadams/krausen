// SPDX-License-Identifier: AGPL-3.0-or-later
//! Transparent access-token refresh over real HTTP, on SQLite and PostgreSQL: a
//! request with a missing or expired access cookie and a valid refresh cookie is
//! served with a fresh session and receives the rotated cookies; failures fall
//! through to the canonical 401; valid access cookies and logout are untouched.

mod common;

use common::{Account, TestServer, account};
use krausen_auth::jwt;
use leptos::server_fn::ServerFn;
use reqwest::StatusCode;

fn protected_path() -> &'static str {
    <krausen_ui::server_fns::security::GetSessions as ServerFn>::PATH
}

fn expired_access(srv: &TestServer, who: &Account) -> String {
    jwt::create_session_access_token_str(
        &who.user_id,
        &srv.secret,
        -5,
        Default::default(),
        who.user.sessions_valid_from,
    )
    .unwrap()
}

async fn post(srv: &TestServer, path: &str, cookie: Option<String>, ip: Option<&str>) -> reqwest::Response {
    let mut request = reqwest::Client::new()
        .post(format!("{}{}", srv.base, path))
        .header("content-type", "application/x-www-form-urlencoded")
        .body("");
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    if let Some(ip) = ip {
        request = request.header("x-real-ip", ip);
    }
    request.send().await.unwrap()
}

fn set_cookie_value(response: &reqwest::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|v| v.strip_prefix(&format!("{name}=")))
        .and_then(|v| v.split(';').next())
        .map(str::to_owned)
}

async fn expired_access_with_a_valid_refresh_is_refreshed_transparently(srv: &TestServer) {
    let who = account(srv).await;
    let cookie = format!(
        "access_token={}; refresh_token={}",
        expired_access(srv, &who),
        who.refresh_token
    );
    let response = post(srv, protected_path(), Some(cookie), None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let access = set_cookie_value(&response, "access_token").expect("new access cookie");
    let refresh = set_cookie_value(&response, "refresh_token").expect("rotated refresh cookie");
    assert_ne!(refresh, who.refresh_token, "the refresh token rotated");
    assert!(jwt::validate_token(&access, &srv.secret).is_ok());
    // The rotated pair keeps working.
    let again = post(
        srv,
        protected_path(),
        Some(format!("access_token={access}; refresh_token={refresh}")),
        None,
    )
    .await;
    assert_eq!(again.status(), StatusCode::OK);
    assert!(set_cookie_value(&again, "access_token").is_none(), "valid access passes through");
}

async fn missing_access_cookie_with_a_valid_refresh_is_refreshed(srv: &TestServer) {
    let who = account(srv).await;
    let response = post(
        srv,
        protected_path(),
        Some(format!("refresh_token={}", who.refresh_token)),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(set_cookie_value(&response, "access_token").is_some());
}

async fn a_valid_access_cookie_passes_through_without_new_cookies(srv: &TestServer) {
    let who = account(srv).await;
    let response = post(
        srv,
        protected_path(),
        Some(format!(
            "access_token={}; refresh_token={}",
            who.access_token, who.refresh_token
        )),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get("set-cookie").is_none());
}

async fn an_unusable_refresh_token_falls_through_to_the_canonical_401(srv: &TestServer) {
    let who = account(srv).await;
    let response = post(
        srv,
        protected_path(),
        Some(format!(
            "access_token={}; refresh_token=rt_not-a-real-token",
            expired_access(srv, &who)
        )),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(response.headers().get("set-cookie").is_none());

    let none = post(srv, protected_path(), None, None).await;
    assert_eq!(none.status(), StatusCode::UNAUTHORIZED);
}

/// The access cookie is expired, so the middleware really attempts the refresh;
/// a refresh token revoked by sign-out-everywhere, by a single-session logout or
/// by theft detection must not be revived by it.
async fn a_revoked_refresh_token_is_not_revived_by_the_middleware(srv: &TestServer) {
    // Sign out everywhere.
    let all = account(srv).await;
    krausen_auth::token_service::revoke_all_user_sessions(&srv.db, &all.user_id)
        .await
        .unwrap();
    // Single-session logout (family revoked).
    let single = account(srv).await;
    let hash = krausen_auth::token_service::hash_refresh_token(&single.refresh_token);
    #[derive(sqlx::FromRow)]
    struct Row {
        token_id: String,
    }
    let row = krausen_core::db_fetch_one!(
        &srv.db,
        Row,
        "SELECT token_id FROM refresh_tokens WHERE token_hash = $1",
        &hash
    )
    .unwrap();
    assert!(
        krausen_auth::token_service::revoke_user_refresh_token(&srv.db, &single.user_id, &row.token_id)
            .await
            .unwrap()
    );
    for (who, why) in [(&all, "revoke-all"), (&single, "family revoked")] {
        let response = post(
            srv,
            protected_path(),
            Some(format!(
                "access_token={}; refresh_token={}",
                expired_access(srv, who),
                who.refresh_token
            )),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{why}");
        assert!(response.headers().get("set-cookie").is_none(), "{why}");
    }
}

/// A still-valid access token issued before sign-out-everywhere takes the
/// middleware's fast path and is refused by the session cutoff.
async fn a_cut_off_access_token_is_refused_by_the_cutoff(srv: &TestServer) {
    let who = account(srv).await;
    krausen_auth::token_service::revoke_all_user_sessions(&srv.db, &who.user_id)
        .await
        .unwrap();
    let response = post(
        srv,
        protected_path(),
        Some(format!(
            "access_token={}; refresh_token={}",
            who.access_token, who.refresh_token
        )),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(response.headers().get("set-cookie").is_none());
}

async fn logout_is_never_refreshed(srv: &TestServer) {
    let who = account(srv).await;
    let logout = <krausen_ui::server_fns::security::Logout as ServerFn>::PATH;
    let response = post(
        srv,
        logout,
        Some(format!(
            "access_token={}; refresh_token={}",
            expired_access(srv, &who),
            who.refresh_token
        )),
        None,
    )
    .await;
    assert!(
        set_cookie_value(&response, "access_token").is_none_or(|v| v.is_empty()),
        "logout must not receive a fresh access token"
    );
    // The refresh token was not rotated.
    let hash = krausen_auth::token_service::hash_refresh_token(&who.refresh_token);
    let replaced: i64 = krausen_core::db_fetch_scalar!(
        &srv.db,
        i64,
        "SELECT COUNT(*) FROM refresh_tokens WHERE token_hash = $1 AND replaced_at IS NOT NULL",
        &hash
    )
    .unwrap();
    assert_eq!(replaced, 0);
}

async fn concurrent_requests_with_one_expired_session_all_succeed(srv: &TestServer) {
    let who = account(srv).await;
    let cookie = format!(
        "access_token={}; refresh_token={}",
        expired_access(srv, &who),
        who.refresh_token
    );
    let (a, b, c) = tokio::join!(
        post(srv, protected_path(), Some(cookie.clone()), None),
        post(srv, protected_path(), Some(cookie.clone()), None),
        post(srv, protected_path(), Some(cookie.clone()), None),
    );
    for response in [a, b, c] {
        assert_eq!(response.status(), StatusCode::OK, "grace period covers concurrent refresh");
    }
}

async fn the_refresh_records_the_request_device_info(srv: &TestServer) {
    let who = account(srv).await;
    let response = post(
        srv,
        protected_path(),
        Some(format!("refresh_token={}", who.refresh_token)),
        Some("203.0.113.9"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let rotated = set_cookie_value(&response, "refresh_token").unwrap();
    let hash = krausen_auth::token_service::hash_refresh_token(&rotated);
    #[derive(sqlx::FromRow)]
    struct Row {
        ip_address: Option<String>,
    }
    let row = krausen_core::db_fetch_one!(
        &srv.db,
        Row,
        "SELECT ip_address FROM refresh_tokens WHERE token_hash = $1",
        &hash
    )
    .unwrap();
    assert_eq!(row.ip_address.as_deref(), Some("203.0.113.9"));
}

common::both_backends!(
    expired_access_with_a_valid_refresh_is_refreshed_transparently,
    missing_access_cookie_with_a_valid_refresh_is_refreshed,
    a_valid_access_cookie_passes_through_without_new_cookies,
    an_unusable_refresh_token_falls_through_to_the_canonical_401,
    a_revoked_refresh_token_is_not_revived_by_the_middleware,
    a_cut_off_access_token_is_refused_by_the_cutoff,
    logout_is_never_refreshed,
    concurrent_requests_with_one_expired_session_all_succeed,
    the_refresh_records_the_request_device_info,
);
