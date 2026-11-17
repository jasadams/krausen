// SPDX-License-Identifier: AGPL-3.0-or-later

//! Real MCP refresh grants earn a sliding inactivity window after validation.
//! Ported from Kyomi's `contract_mcp_refresh_expiry.rs` and run against the real
//! router on SQLite and PostgreSQL.
//!
//! Differences from Kyomi, all forced by Krausen: the in-process test server and
//! accounts come from `common` (DbPool/DbTx on both backends), and the
//! persistence-failure case injects its failing trigger on each backend
//! (Kyomi's runs on PostgreSQL only).

mod common;

use chrono::{DateTime, Duration, Utc};
use common::{TestServer, account};
use krausen_auth::{jwt, token_service};
use krausen_core::DbPool;
use serde_json::{Value, json};

#[derive(sqlx::FromRow)]
struct Expiry {
    expires_at: DateTime<Utc>,
    replaced_at: Option<DateTime<Utc>>,
}

async fn registered(srv: &TestServer) -> String {
    let response = reqwest::Client::new()
        .post(format!("{}/api/v1/oauth/register", srv.base))
        .json(&json!({"redirect_uris": ["https://example.com/callback"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    response.json::<Value>().await.unwrap()["client_id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn token(
    srv: &TestServer,
    user_id: &str,
    client: Option<&str>,
    expiry: DateTime<Utc>,
) -> (String, String) {
    let raw = jwt::create_refresh_token();
    let mut device = common::device();
    device.oauth_client_id = client.map(str::to_string);
    let id = token_service::store_refresh_token(
        &srv.db,
        user_id,
        &token_service::hash_refresh_token(&raw),
        expiry,
        &device,
        &token_service::generate_family_id(),
    )
    .await
    .unwrap();
    (id, raw)
}

async fn expiry(srv: &TestServer, id: &str) -> Expiry {
    krausen_core::db_fetch_one!(
        &srv.db,
        Expiry,
        "SELECT expires_at, replaced_at FROM refresh_tokens WHERE token_id = $1",
        id
    )
    .unwrap()
}

async fn refresh(srv: &TestServer, client: &str, raw: &str) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{}/api/v1/oauth/token", srv.base))
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client),
            ("refresh_token", raw),
        ])
        .send()
        .await
        .unwrap()
}

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap()
}

async fn active_mcp_refresh_keeps_raw_token_and_renews_inactivity_timeout(srv: &TestServer) {
    let who = account(srv).await;
    let client = registered(srv).await;
    let original = now() + Duration::hours(1);
    let (id, raw) = token(srv, &who.user_id, Some(&client), original).await;
    for _ in 0..2 {
        let before = Utc::now();
        let response = refresh(srv, &client, &raw).await;
        assert_eq!(response.status(), 200);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["refresh_token"], raw);
        assert_eq!(body["scope"], "mcp");
        let signed = jwt::validate_token(body["access_token"].as_str().unwrap(), &srv.secret).unwrap();
        assert_eq!(signed.claims.sub, who.user_id);
        let saved = expiry(srv, &id).await;
        let ttl = Duration::days(krausen_core::constants::get().jwt.refresh_token_expire_days);
        assert!(saved.expires_at >= before + ttl - Duration::seconds(1));
        assert!(saved.expires_at <= Utc::now() + ttl + Duration::seconds(1));
        assert!(
            saved.expires_at > original,
            "the grant survives its original deadline"
        );
        assert!(saved.replaced_at.is_none(), "MCP must never rotate");
    }
    // Explicit inactivity fixture beyond the renewed deadline: no multi-day sleep.
    let inactive = now() - Duration::days(1);
    krausen_core::db_execute!(
        &srv.db,
        "UPDATE refresh_tokens SET expires_at = $1 WHERE token_id = $2",
        &inactive,
        &id
    )
    .unwrap();
    let response = refresh(srv, &client, &raw).await;
    assert_eq!(response.status(), 400);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"error": "invalid_grant: refresh token invalid or expired"})
    );
    assert_eq!(expiry(srv, &id).await.expires_at, inactive);
}

async fn rejected_mcp_grants_never_earn_expiry_extension(srv: &TestServer) {
    let who = account(srv).await;
    let client = registered(srv).await;
    let other = registered(srv).await;
    let original = now() + Duration::hours(1);
    let (id, raw) = token(srv, &who.user_id, Some(&client), original).await;
    for rejected_client in [&other, "unknown-client"] {
        assert_eq!(refresh(srv, rejected_client, &raw).await.status(), 400);
        assert_eq!(expiry(srv, &id).await.expires_at, original);
    }
    // The active-client gate runs before the refresh grant can be renewed.
    krausen_core::db_execute!(
        &srv.db,
        "UPDATE oauth_clients SET active = $1 WHERE client_id = $2",
        &false,
        &client
    )
    .unwrap();
    assert_eq!(refresh(srv, &client, &raw).await.status(), 400);
    assert_eq!(expiry(srv, &id).await.expires_at, original);
    krausen_core::db_execute!(
        &srv.db,
        "UPDATE oauth_clients SET active = $1 WHERE client_id = $2",
        &true,
        &client
    )
    .unwrap();
    assert_eq!(refresh(srv, &client, "unknown-refresh").await.status(), 400);
    let (browser_id, browser_raw) = token(srv, &who.user_id, None, original).await;
    assert_eq!(refresh(srv, &client, &browser_raw).await.status(), 400);
    assert_eq!(expiry(srv, &browser_id).await.expires_at, original);
    krausen_core::db_execute!(
        &srv.db,
        "UPDATE users SET active = $1 WHERE user_id = $2",
        &false,
        &who.user_id
    )
    .unwrap();
    assert_eq!(refresh(srv, &client, &raw).await.status(), 400);
    assert_eq!(expiry(srv, &id).await.expires_at, original);
    krausen_core::db_execute!(
        &srv.db,
        "UPDATE users SET active = $1, last_workspace_id = NULL WHERE user_id = $2",
        &true,
        &who.user_id
    )
    .unwrap();
    krausen_core::db_execute!(
        &srv.db,
        "UPDATE workspace_users SET active = $1 WHERE user_id = $2",
        &false,
        &who.user_id
    )
    .unwrap();
    let missing_workspace = refresh(srv, &client, &raw).await;
    assert_eq!(missing_workspace.status(), 400);
    assert_eq!(
        missing_workspace.json::<Value>().await.unwrap(),
        json!({"error": "invalid_grant: no workspace access"})
    );
    assert_eq!(expiry(srv, &id).await.expires_at, original);
    token_service::revoke_user_refresh_token(&srv.db, &who.user_id, &id)
        .await
        .unwrap();
    assert_eq!(refresh(srv, &client, &raw).await.status(), 400);
    assert_eq!(expiry(srv, &id).await.expires_at, original);
}

async fn grace_mcp_refresh_preserves_rotation_deadlines_and_theft_detection(srv: &TestServer) {
    let who = account(srv).await;
    let client = registered(srv).await;
    let original = now() + Duration::hours(1);
    let (id, raw) = token(srv, &who.user_id, Some(&client), original).await;
    let replaced = now();
    krausen_core::db_execute!(
        &srv.db,
        "UPDATE refresh_tokens SET replaced_at = $1 WHERE token_id = $2",
        &replaced,
        &id
    )
    .unwrap();
    let response = refresh(srv, &client, &raw).await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.json::<Value>().await.unwrap()["refresh_token"],
        raw
    );
    assert_eq!(expiry(srv, &id).await.expires_at, original);
    assert_eq!(expiry(srv, &id).await.replaced_at, Some(replaced));
    let past_grace = now()
        - Duration::seconds(
            krausen_core::constants::get()
                .jwt
                .refresh_token_grace_period_seconds
                + 5,
        );
    krausen_core::db_execute!(
        &srv.db,
        "UPDATE refresh_tokens SET replaced_at = $1 WHERE token_id = $2",
        &past_grace,
        &id
    )
    .unwrap();
    let response = refresh(srv, &client, &raw).await;
    assert_eq!(response.status(), 400);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"error": "invalid_grant: refresh token revoked"})
    );
    assert_eq!(expiry(srv, &id).await.expires_at, original);
    assert!(matches!(
        token_service::verify_refresh_token(&srv.db, &raw)
            .await
            .unwrap(),
        token_service::RefreshTokenVerifyResult::Invalid
    ));
}

async fn failed_expiry_persistence_cannot_return_a_successful_grant(srv: &TestServer) {
    let who = account(srv).await;
    let client = registered(srv).await;
    let original = now() + Duration::hours(1);
    let (id, raw) = token(srv, &who.user_id, Some(&client), original).await;
    // Test-generated identifiers and token ids only; the trigger rejects just
    // this fixture's expiry update, leaving verification's last_used write intact.
    let name = format!("k900_{}", uuid::Uuid::new_v4().simple());
    match &srv.db {
        DbPool::Postgres(pg) => {
            sqlx::query(&format!(
                "CREATE FUNCTION {name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'controlled expiry persistence failure'; END $$"
            ))
            .execute(pg)
            .await
            .unwrap();
            sqlx::query(&format!(
                "CREATE TRIGGER {name} BEFORE UPDATE OF expires_at ON refresh_tokens FOR EACH ROW WHEN (OLD.token_id = '{id}') EXECUTE FUNCTION {name}()"
            ))
            .execute(pg)
            .await
            .unwrap();
        }
        DbPool::Sqlite(sqlite) => {
            sqlx::query(&format!(
                "CREATE TRIGGER {name} BEFORE UPDATE OF expires_at ON refresh_tokens FOR EACH ROW WHEN OLD.token_id = '{id}' BEGIN SELECT RAISE(ABORT, 'controlled expiry persistence failure'); END"
            ))
            .execute(sqlite)
            .await
            .unwrap();
        }
    }
    let response = refresh(srv, &client, &raw).await;
    match &srv.db {
        DbPool::Postgres(pg) => {
            sqlx::query(&format!("DROP FUNCTION {name}() CASCADE"))
                .execute(pg)
                .await
                .unwrap();
        }
        DbPool::Sqlite(sqlite) => {
            sqlx::query(&format!("DROP TRIGGER {name}"))
                .execute(sqlite)
                .await
                .unwrap();
        }
    }
    assert_eq!(response.status(), 500);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"error": "internal_error"})
    );
    assert_eq!(expiry(srv, &id).await.expires_at, original);
}

common::both_backends!(
    active_mcp_refresh_keeps_raw_token_and_renews_inactivity_timeout,
    rejected_mcp_grants_never_earn_expiry_extension,
    grace_mcp_refresh_preserves_rotation_deadlines_and_theft_detection,
    failed_expiry_persistence_cannot_return_a_successful_grant,
);
