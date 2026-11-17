// SPDX-License-Identifier: AGPL-3.0-or-later

//! Browser session revocation cutoff (`users.sessions_valid_from`), run on
//! SQLite and PostgreSQL: all-session revocation, signed issuance, stale-login
//! rejection, recovery eviction, refresh, fail-closed authentication.

use axum::extract::FromRequestParts;
use krausen_auth::{
    auth_service::{
        recovery_set_password_service, RecoverySetPasswordServiceResult,
    },
    jwt,
    middleware::{AuthState, AuthUser},
    session,
    token_refresh,
    token_service::{self, DeviceInfo},
    user_service,
};
use krausen_core::{DbPool, KVPool};

const SECRET: &str = "test-secret-at-least-32-bytes-long";

fn device() -> DeviceInfo {
    DeviceInfo {
        user_agent: None,
        ip_address: None,
        country_code: None,
        oauth_client_id: None,
    }
}

fn kv() -> KVPool {
    krausen_core::kv_store_memory::InMemoryKVStore::new_pool()
}

async fn user(db: &DbPool) -> krausen_core::models::User {
    let _ = krausen_core::constants::load_with_fallback();
    let email = format!("{}@example.test", uuid::Uuid::new_v4());
    let user = user_service::create_user(db, &email, Some("Brewer"), true)
        .await
        .unwrap();
    user_service::create_workspace_for_user(db, &user.user_id, Some("Brewer"), &email, None)
        .await
        .unwrap();
    user_service::get_user_by_id(db, &user.user_id)
        .await
        .unwrap()
        .unwrap()
}

async fn login(db: &DbPool, user: &krausen_core::models::User) -> session::AuthenticatedSession {
    session::create_authenticated_session(db, &kv(), SECRET, user, &device())
        .await
        .unwrap()
}

async fn authenticate(db: &DbPool, token: &str) -> krausen_core::Result<AuthUser> {
    let state = AuthState {
        jwt_secret: SECRET.into(),
        db: db.clone(),
    };
    let (mut parts, _) = axum::http::Request::builder()
        .uri("/x")
        .header("authorization", format!("Bearer {token}"))
        .body(())
        .unwrap()
        .into_parts();
    AuthUser::from_request_parts(&mut parts, &state).await
}

async fn cutoff(db: &DbPool, user_id: &str) -> Option<i64> {
    user_service::get_user_by_id(db, user_id)
        .await
        .unwrap()
        .unwrap()
        .sessions_valid_from
}

/// The KRA-10 credential row also dies on revoke-all, and the extractor checks it
/// before the cutoff. Un-revoking it leaves only the cutoff to refuse the token.
async fn revive_credentials(db: &DbPool, user_id: &str) {
    krausen_core::db_execute!(
        db,
        "UPDATE auth_credentials SET revoked_at = NULL WHERE user_id = $1",
        user_id
    )
    .unwrap();
}

/// Assert the extractor refuses `token` because of the cutoff alone.
async fn assert_refused_by_cutoff(db: &DbPool, token: &str) {
    match authenticate(db, token).await {
        Err(krausen_core::Error::Unauthorized(msg)) => assert!(
            msg.contains("Session revoked"),
            "expected the cutoff to refuse the token, got: {msg}"
        ),
        Err(other) => panic!("expected the cutoff to refuse the token, got: {other:?}"),
        Ok(_) => panic!("token issued before the cutoff must be refused"),
    }
}

/// New accounts start with no cutoff, so existing sessions are preserved.
async fn new_user_has_no_cutoff(db: &DbPool) {
    let u = user(db).await;
    assert_eq!(cutoff(db, &u.user_id).await, None);
}

/// Login mints a signed microsecond issuance and the session authenticates.
async fn login_signs_session_issuance_and_authenticates(db: &DbPool) {
    let u = user(db).await;
    let sess = login(db, &u).await;
    let claims = jwt::validate_browser_token(&sess.access_token, SECRET).unwrap().claims;
    assert!(claims.session_iat_us.is_some());
    assert!(authenticate(db, &sess.access_token).await.is_ok());
}

/// Revoking all sessions invalidates already-issued access tokens at once,
/// kills refresh tokens, and a login afterwards works.
async fn revoke_all_sessions_invalidates_access_and_refresh(db: &DbPool) {
    let u = user(db).await;
    let old = login(db, &u).await;
    let revoked = token_service::revoke_all_user_sessions(db, &u.user_id)
        .await
        .unwrap();
    assert_eq!(revoked, 1, "one refresh token was active");
    assert!(cutoff(db, &u.user_id).await.is_some());

    revive_credentials(db, &u.user_id).await;
    assert_refused_by_cutoff(db, &old.access_token).await;
    assert!(
        token_refresh::refresh_tokens(db, SECRET, &old.refresh_token, &device())
            .await
            .is_err(),
        "the revoked refresh token must not refresh"
    );

    let fresh_user = user_service::get_user_by_id(db, &u.user_id)
        .await
        .unwrap()
        .unwrap();
    let new = login(db, &fresh_user).await;
    assert!(authenticate(db, &new.access_token).await.is_ok());
}

/// The cutoff strictly advances on repeated events and with no tokens to revoke.
async fn cutoff_advances_strictly_on_repeated_events(db: &DbPool) {
    let u = user(db).await;
    assert_eq!(token_service::revoke_all_user_sessions(db, &u.user_id).await.unwrap(), 0);
    let first = cutoff(db, &u.user_id).await.unwrap();
    token_service::revoke_all_user_sessions(db, &u.user_id).await.unwrap();
    let second = cutoff(db, &u.user_id).await.unwrap();
    assert!(second > first, "{second} must be after {first}");
}

/// A session whose signed issuance is not after the current cutoff cannot be
/// persisted: a pre-event mint cannot survive the event.
async fn stale_issuance_cannot_store_a_refresh_token(db: &DbPool) {
    let u = user(db).await;
    token_service::revoke_all_user_sessions(db, &u.user_id).await.unwrap();
    let c = cutoff(db, &u.user_id).await.unwrap();
    let hash = token_service::hash_refresh_token("rt_stale");
    let result = token_service::store_session_refresh_token(
        db,
        &u.user_id,
        &hash,
        chrono::Utc::now() + chrono::Duration::days(1),
        &device(),
        &token_service::generate_family_id(),
        c,
    )
    .await;
    assert!(matches!(result, Err(krausen_core::Error::Unauthorized(_))));
}

/// Password recovery evicts every prior session and leaves the recovered one.
async fn password_recovery_evicts_prior_sessions(db: &DbPool) {
    let u = user(db).await;
    let before = login(db, &u).await;
    let kv = kv();
    let recovery_id = format!("rec-{}", uuid::Uuid::new_v4());
    krausen_auth::redis_ops::store_recovery_session(&kv, &recovery_id, &u.user_id)
        .await
        .unwrap();

    let result = recovery_set_password_service(
        db,
        &kv,
        SECRET,
        &recovery_id,
        "a-brand-new-password",
        &device(),
    )
    .await
    .unwrap();
    let RecoverySetPasswordServiceResult::Success(after) = result else {
        panic!("recovery should succeed")
    };

    revive_credentials(db, &u.user_id).await;
    assert_refused_by_cutoff(db, &before.access_token).await;
    assert!(
        token_refresh::refresh_tokens(db, SECRET, &before.refresh_token, &device())
            .await
            .is_err()
    );
    assert!(authenticate(db, &after.access_token).await.is_ok());
    assert!(
        token_refresh::refresh_tokens(db, SECRET, &after.refresh_token, &device())
            .await
            .is_ok(),
        "the recovered session keeps a working refresh token"
    );
}

/// A refresh after a revocation mints access tokens after the cutoff.
async fn refreshed_access_token_follows_the_cutoff(db: &DbPool) {
    let u = user(db).await;
    let sess = login(db, &u).await;
    let refreshed = token_refresh::refresh_tokens(db, SECRET, &sess.refresh_token, &device())
        .await
        .unwrap();
    assert!(authenticate(db, &refreshed.access_token).await.is_ok());
    token_service::revoke_all_user_sessions(db, &u.user_id).await.unwrap();
    revive_credentials(db, &u.user_id).await;
    assert_refused_by_cutoff(db, &refreshed.access_token).await;
}

/// A DB error while loading the workspace membership fails the request closed.
async fn membership_lookup_error_fails_closed(db: &DbPool) {
    let u = user(db).await;
    let sess = login(db, &u).await;
    assert!(authenticate(db, &sess.access_token).await.is_ok());
    krausen_core::db_execute!(db, "ALTER TABLE workspace_users RENAME TO workspace_users_gone")
        .unwrap();
    let result = authenticate(db, &sess.access_token).await;
    krausen_core::db_execute!(db, "ALTER TABLE workspace_users_gone RENAME TO workspace_users")
        .unwrap();
    assert!(
        matches!(result, Err(krausen_core::Error::Internal(_))),
        "a database error must fail closed, not fall through with a default context"
    );
}

/// A bare access token with no workspace-independent signed issuance and a
/// cutoff in the future of its legacy second is rejected (legacy second rule).
async fn legacy_token_without_issuance_is_rejected_by_whole_second(db: &DbPool) {
    let u = user(db).await;
    let sess = login(db, &u).await;
    let mut claims = jwt::validate_browser_token(&sess.access_token, SECRET).unwrap().claims;
    claims.session_iat_us = None;
    let cutoff_us = (claims.iat + 1) * 1_000_000;
    assert!(claims.require_current_session(Some(cutoff_us)).is_err());
    assert!(claims.require_current_session(Some((claims.iat - 1) * 1_000_000)).is_ok());
    assert!(claims.require_current_session(None).is_ok());
}

/// Concurrent revocations each advance the persisted cutoff.
async fn concurrent_revocations_each_advance_the_cutoff(db: &DbPool) {
    let u = user(db).await;
    token_service::revoke_all_user_sessions(db, &u.user_id).await.unwrap();
    let first = cutoff(db, &u.user_id).await.unwrap();
    let (a, b) = tokio::join!(
        token_service::revoke_all_user_sessions(db, &u.user_id),
        token_service::revoke_all_user_sessions(db, &u.user_id)
    );
    a.unwrap();
    b.unwrap();
    let last = cutoff(db, &u.user_id).await.unwrap();
    assert!(last >= first + 2, "{last} must be at least two past {first}");
}

/// Session creation reloads the user: a stale pre-event user value must not
/// mint a session that the event already revoked.
async fn session_creation_reloads_a_stale_user(db: &DbPool) {
    let u = user(db).await;
    token_service::revoke_all_user_sessions(db, &u.user_id).await.unwrap();
    // `u` still carries the pre-event (None) cutoff.
    assert_eq!(u.sessions_valid_from, None);
    let sess = login(db, &u).await;
    assert!(authenticate(db, &sess.access_token).await.is_ok());
    assert!(
        token_refresh::refresh_tokens(db, SECRET, &sess.refresh_token, &device())
            .await
            .is_ok()
    );
}

/// Restricted recovery tokens keep their independent cutoff behaviour.
async fn passkey_recovery_scope_is_exempt_from_the_session_cutoff(db: &DbPool) {
    let u = user(db).await;
    let token = jwt::create_access_token_str(
        &u.user_id,
        SECRET,
        15,
        [("scope".into(), serde_json::json!("passkey_recovery"))]
            .into_iter()
            .collect(),
    )
    .unwrap();
    let claims = jwt::validate_token(&token, SECRET).unwrap().claims;
    assert!(claims.session_iat_us.is_none());
    token_service::revoke_all_user_sessions(db, &u.user_id).await.unwrap();
    let c = cutoff(db, &u.user_id).await;
    claims.require_current_session(c).unwrap();
}

/// A session minted against a future cutoff is a strict successor of it.
async fn minted_issuance_is_a_strict_successor_of_the_cutoff(db: &DbPool) {
    let _ = db;
    let future = chrono::Utc::now().timestamp_micros() + 60_000_000;
    let token = jwt::create_session_access_token_str(
        "user-1",
        SECRET,
        15,
        Default::default(),
        Some(future),
    )
    .unwrap();
    let claims = jwt::validate_token(&token, SECRET).unwrap().claims;
    assert_eq!(claims.session_iat_us, Some(future + 1));
    assert!(claims.require_current_session(Some(future)).is_ok());
    assert!(claims.require_current_session(Some(future + 1)).is_err());
}

/// Grace re-rotation of a token that predates a revoke-all is rejected.
async fn grace_rerotation_after_revoke_all_is_rejected(db: &DbPool) {
    let u = user(db).await;
    let sess = login(db, &u).await;
    let first = token_refresh::refresh_tokens(db, SECRET, &sess.refresh_token, &device())
        .await
        .unwrap();
    // The original token is now replaced but inside the grace period: it would
    // rotate again...
    token_service::revoke_all_user_sessions(db, &u.user_id).await.unwrap();
    for raw in [&sess.refresh_token, &first.raw_refresh_token] {
        assert!(
            token_refresh::refresh_tokens(db, SECRET, raw, &device())
                .await
                .is_err(),
            "no token of a revoked session may re-rotate, grace or not"
        );
    }
    revive_credentials(db, &u.user_id).await;
    assert_refused_by_cutoff(db, &first.access_token).await;
}

/// A DB error loading the workspace itself also fails closed.
async fn workspace_lookup_error_fails_closed(db: &DbPool) {
    let u = user(db).await;
    let sess = login(db, &u).await;
    krausen_core::db_execute!(db, "ALTER TABLE workspaces RENAME TO workspaces_gone").unwrap();
    let result = authenticate(db, &sess.access_token).await;
    krausen_core::db_execute!(db, "ALTER TABLE workspaces_gone RENAME TO workspaces").unwrap();
    assert!(
        matches!(result, Err(krausen_core::Error::Internal(_))),
        "a workspace lookup error must fail closed, got {result:?}"
    );
}

/// A DB error loading the user fails closed.
async fn user_lookup_error_fails_closed(db: &DbPool) {
    let u = user(db).await;
    let sess = login(db, &u).await;
    krausen_core::db_execute!(db, "ALTER TABLE users RENAME TO users_gone").unwrap();
    let result = authenticate(db, &sess.access_token).await;
    krausen_core::db_execute!(db, "ALTER TABLE users_gone RENAME TO users").unwrap();
    assert!(
        matches!(result, Err(krausen_core::Error::Internal(_))),
        "a user lookup error must fail closed, got {result:?}"
    );
}

/// A recovery-session token carries no browser issuer, audience or credential,
/// so the extractor refuses it as an invalid browser credential. (The refusal
/// comes from the browser-token validation, not from the `scope` claim.)
async fn a_recovery_session_token_is_refused_as_a_browser_credential(db: &DbPool) {
    let u = user(db).await;
    let token = jwt::create_access_token_str(
        &u.user_id,
        SECRET,
        15,
        [
            ("scope".into(), serde_json::json!("passkey_recovery")),
            ("email".into(), serde_json::json!(u.email)),
        ]
        .into_iter()
        .collect(),
    )
    .unwrap();
    match authenticate(db, &token).await {
        Err(krausen_core::Error::Unauthorized(msg)) => {
            assert_eq!(msg, "Invalid browser credential")
        }
        Err(other) => panic!("expected Unauthorized, got {other:?}"),
        Ok(_) => panic!("a recovery-session token must not authenticate as a browser session"),
    }
}

async fn sqlite() -> DbPool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../apps/server/migrations-sqlite")
        .run(&pool)
        .await
        .unwrap();
    DbPool::Sqlite(pool)
}

async fn postgres() -> DbPool {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let admin = sqlx::PgPool::connect(&url).await.unwrap();
    let name = format!("krausen_cutoff_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(&admin)
        .await
        .unwrap();
    let mut db_url = url::Url::parse(&url).unwrap();
    db_url.set_path(&name);
    let pool = sqlx::PgPool::connect(db_url.as_str()).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    DbPool::Postgres(pool)
}

macro_rules! on_both_backends {
    ($($scenario:ident),+ $(,)?) => {$(
        mod $scenario {
            #[tokio::test]
            async fn sqlite() { super::$scenario(&super::sqlite().await).await; }
            #[tokio::test]
            async fn postgres() { super::$scenario(&super::postgres().await).await; }
        }
    )+};
}

on_both_backends!(
    new_user_has_no_cutoff,
    login_signs_session_issuance_and_authenticates,
    revoke_all_sessions_invalidates_access_and_refresh,
    cutoff_advances_strictly_on_repeated_events,
    stale_issuance_cannot_store_a_refresh_token,
    password_recovery_evicts_prior_sessions,
    refreshed_access_token_follows_the_cutoff,
    membership_lookup_error_fails_closed,
    legacy_token_without_issuance_is_rejected_by_whole_second,
    concurrent_revocations_each_advance_the_cutoff,
    session_creation_reloads_a_stale_user,
    passkey_recovery_scope_is_exempt_from_the_session_cutoff,
    minted_issuance_is_a_strict_successor_of_the_cutoff,
    grace_rerotation_after_revoke_all_is_rejected,
    workspace_lookup_error_fails_closed,
    user_lookup_error_fails_closed,
    a_recovery_session_token_is_refused_as_a_browser_credential,
);
