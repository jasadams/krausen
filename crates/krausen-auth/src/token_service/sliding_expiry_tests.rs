// SPDX-License-Identifier: AGPL-3.0-or-later

//! Persistence and browser regressions for the MCP inactivity timeout,
//! ported from Kyomi's `token_sliding_expiry_tests.rs` and run on SQLite and
//! PostgreSQL through the shared both-backends harness.
//!
//! Differences forced by Krausen: a browser refresh needs a real durable
//! credential family (`auth_credentials`), so the browser half logs in through
//! `session::create_authenticated_session` instead of storing a bare token; and
//! revocation goes through `revoke_user_refresh_token` (Krausen has no
//! per-token-id-only revoke; every stored fixture is its own family).

use chrono::{DateTime, Duration, Utc};
use krausen_core::DbPool;

use super::{DeviceInfo, RefreshTokenVerifyResult};
use crate::{
    auth_service::signup_tests::{both_backends, email, postgres_db, sqlite_db},
    jwt, session, token_service, user_service,
};

#[derive(sqlx::FromRow)]
struct TokenState {
    expires_at: DateTime<Utc>,
    replaced_at: Option<DateTime<Utc>>,
    is_active: bool,
}

#[derive(sqlx::FromRow)]
struct TokenIdRow {
    token_id: String,
}

async fn state(db: &DbPool, id: &str) -> TokenState {
    krausen_core::db_fetch_one!(
        db,
        TokenState,
        "SELECT expires_at, replaced_at, is_active FROM refresh_tokens WHERE token_id = $1",
        id
    )
    .unwrap()
}

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap()
}

fn device(client: Option<&str>) -> DeviceInfo {
    DeviceInfo {
        user_agent: None,
        ip_address: None,
        country_code: None,
        oauth_client_id: client.map(str::to_string),
    }
}

async fn new_user(db: &DbPool, tag: &str) -> krausen_core::models::User {
    let _ = krausen_core::constants::load_with_fallback();
    user_service::create_user(db, &email(tag), None, true)
        .await
        .unwrap()
}

async fn stored(
    db: &DbPool,
    user: &str,
    client: Option<&str>,
    expiry: DateTime<Utc>,
) -> (String, String) {
    let raw = jwt::create_refresh_token();
    let id = token_service::store_refresh_token(
        db,
        user,
        &token_service::hash_refresh_token(&raw),
        expiry,
        &device(client),
        &token_service::generate_family_id(),
    )
    .await
    .unwrap();
    (id, raw)
}

async fn renew(db: &DbPool, id: &str, client: &str, deadline: DateTime<Utc>) -> bool {
    token_service::renew_oauth_refresh_token(db, id, client, deadline)
        .await
        .unwrap()
}

async fn persistence_guards(db: &DbPool) {
    let user = new_user(db, "sliding-guards").await;
    let original = now() + Duration::hours(1);
    let later = now() + Duration::days(7);
    let (id, raw) = stored(db, &user.user_id, Some("sliding-client"), original).await;
    // Verification itself must not globally introduce sliding browser expiry.
    assert!(matches!(
        token_service::verify_refresh_token(db, &raw).await.unwrap(),
        RefreshTokenVerifyResult::Valid(_)
    ));
    assert_eq!(state(db, &id).await.expires_at, original);
    assert!(renew(db, &id, "sliding-client", later).await);
    assert_eq!(state(db, &id).await.expires_at, later);
    // An older request completing last cannot shorten the newer deadline.
    assert!(renew(db, &id, "sliding-client", original).await);
    assert_eq!(state(db, &id).await.expires_at, later);
    // A different client cannot renew the grant.
    assert!(!renew(db, &id, "wrong-client", later + Duration::days(1)).await);
    assert_eq!(state(db, &id).await.expires_at, later);
    assert!(!renew(db, "unknown", "sliding-client", later).await);

    // Verify before revocation, then resume renewal only after revocation commits.
    let verified = token_service::verify_refresh_token(db, &raw).await.unwrap();
    let RefreshTokenVerifyResult::Valid(data) = verified else {
        panic!("valid fixture")
    };
    assert!(
        token_service::revoke_user_refresh_token(db, &user.user_id, &id)
            .await
            .unwrap()
    );
    assert!(!renew(db, &data.token_id, "sliding-client", later + Duration::days(1)).await);
    assert!(!state(db, &id).await.is_active);
    assert_eq!(state(db, &id).await.expires_at, later);

    // An expired OAuth grant and a browser (non-OAuth) token are never renewed.
    for client in [Some("sliding-client"), None] {
        let expiry = if client.is_some() {
            now() - Duration::hours(1)
        } else {
            original
        };
        let (id, _) = stored(db, &user.user_id, client, expiry).await;
        assert!(!renew(db, &id, "sliding-client", later).await);
        assert_eq!(state(db, &id).await.expires_at, expiry);
    }
    // A revoked timestamp must also fence renewal even if is_active is
    // inconsistent; a stale verifier must never clear revocation state.
    let (revoked_id, _) = stored(db, &user.user_id, Some("sliding-client"), original).await;
    krausen_core::db_execute!(
        db,
        "UPDATE refresh_tokens SET revoked_at = $1 WHERE token_id = $2",
        &now(),
        &revoked_id
    )
    .unwrap();
    assert!(!renew(db, &revoked_id, "sliding-client", later).await);
    assert_eq!(state(db, &revoked_id).await.expires_at, original);
    // A deactivated account cannot renew.
    let (inactive_id, _) = stored(db, &user.user_id, Some("sliding-client"), original).await;
    krausen_core::db_execute!(
        db,
        "UPDATE users SET active = $1 WHERE user_id = $2",
        &false,
        &user.user_id
    )
    .unwrap();
    assert!(!renew(db, &inactive_id, "sliding-client", later).await);
    assert_eq!(state(db, &inactive_id).await.expires_at, original);
}

async fn concurrent_renewal_and_revocation(db: &DbPool) {
    let user = new_user(db, "sliding-concurrent").await;
    let expiry = now() + Duration::hours(1);
    let (id, _) = stored(db, &user.user_id, Some("sliding-client"), expiry).await;
    let early = now() + Duration::days(7);
    let late = early + Duration::hours(1);
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(3));
    let mut workers = Vec::new();
    for deadline in [early, late] {
        let (db, id, barrier) = (db.clone(), id.clone(), barrier.clone());
        workers.push(tokio::spawn(async move {
            barrier.wait().await;
            token_service::renew_oauth_refresh_token(&db, &id, "sliding-client", deadline)
                .await
                .unwrap()
        }));
    }
    barrier.wait().await;
    for worker in workers {
        assert!(worker.await.unwrap());
    }
    // Whichever finished last, the later deadline wins.
    assert_eq!(state(db, &id).await.expires_at, late);
    // Either lock order is safe: a successful renewal cannot undo revocation.
    let (renewed, revoked) = tokio::join!(
        token_service::renew_oauth_refresh_token(
            db,
            &id,
            "sliding-client",
            late + Duration::hours(1)
        ),
        token_service::revoke_user_refresh_token(db, &user.user_id, &id)
    );
    renewed.unwrap();
    assert!(revoked.unwrap());
    assert!(!state(db, &id).await.is_active);
    assert!(!renew(db, &id, "sliding-client", late + Duration::days(1)).await);
}

async fn browser_rotation_and_replaced_oauth_grace(db: &DbPool) {
    let user = new_user(db, "sliding-browser").await;
    let secret = "sliding-browser-secret";
    let kv = krausen_core::kv_store_memory::InMemoryKVStore::new_pool();
    let login = session::create_authenticated_session(db, &kv, secret, &user, &device(None))
        .await
        .unwrap();
    let raw = login.refresh_token.clone();
    let old_id = krausen_core::db_fetch_one!(
        db,
        TokenIdRow,
        "SELECT token_id FROM refresh_tokens WHERE token_hash = $1",
        &token_service::hash_refresh_token(&raw)
    )
    .unwrap()
    .token_id;
    let expiry = state(db, &old_id).await.expires_at;

    let rotated = crate::token_refresh::refresh_tokens(db, secret, &raw, &device(None))
        .await
        .unwrap();
    assert_ne!(rotated.raw_refresh_token, raw);
    assert!(jwt::validate_browser_token(&rotated.access_token, secret).is_ok());
    let old = state(db, &old_id).await;
    assert_eq!(old.expires_at, expiry, "browser rotation must not slide expiry");
    let replaced = old.replaced_at.unwrap();
    assert!(matches!(
        token_service::verify_refresh_token(db, &raw).await.unwrap(),
        RefreshTokenVerifyResult::GracePeriod(_)
    ));

    // Model an OAuth-marked replaced token: it may be accepted during grace,
    // but neither expiry nor replaced_at may advance.
    krausen_core::db_execute!(
        db,
        "UPDATE refresh_tokens SET oauth_client_id = $1 WHERE token_id = $2",
        "sliding-client",
        &old_id
    )
    .unwrap();
    let late = now() + Duration::days(30);
    assert!(renew(db, &old_id, "sliding-client", late).await);
    assert_eq!(state(db, &old_id).await.expires_at, expiry);
    assert_eq!(state(db, &old_id).await.replaced_at, Some(replaced));

    // Past the grace period the replaced token is refused and reuse is theft.
    let past_grace = now()
        - Duration::seconds(
            krausen_core::constants::get()
                .jwt
                .refresh_token_grace_period_seconds
                + 5,
        );
    krausen_core::db_execute!(
        db,
        "UPDATE refresh_tokens SET replaced_at = $1 WHERE token_id = $2",
        &past_grace,
        &old_id
    )
    .unwrap();
    assert!(!renew(db, &old_id, "sliding-client", late).await);
    assert!(matches!(
        token_service::verify_refresh_token(db, &raw).await.unwrap(),
        RefreshTokenVerifyResult::TheftDetected { .. }
    ));
    assert!(matches!(
        token_service::verify_refresh_token(db, &rotated.raw_refresh_token)
            .await
            .unwrap(),
        RefreshTokenVerifyResult::Invalid
    ));
}

both_backends!(
    persistence_guards,
    concurrent_renewal_and_revocation,
    browser_rotation_and_replaced_oauth_grace,
);
