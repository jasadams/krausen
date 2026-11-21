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
    let claims = jwt::validate_token(&sess.access_token, SECRET).unwrap().claims;
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
    let mut claims = jwt::validate_token(&sess.access_token, SECRET).unwrap().claims;
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

/// KYO-571 port: an MCP JWT is refused by the browser extractor (`AuthUser`),
/// so it cannot reach any non-`/mcp` route: it has no session authority.
async fn mcp_token_is_refused_by_the_browser_extractor(db: &DbPool) {
    let u = user(db).await;
    let mcp = jwt::create_mcp_access_token_str(&u.user_id, SECRET, 15, Default::default()).unwrap();
    match authenticate(db, &mcp).await {
        Err(krausen_core::Error::Unauthorized(msg)) => {
            assert_eq!(msg, "Token is not a browser session")
        }
        Err(other) => panic!("expected Unauthorized, got {other:?}"),
        Ok(_) => panic!("an MCP token must not authenticate as a browser session"),
    }
    // The control: a real browser session for the same user is accepted.
    let sess = login(db, &u).await;
    assert!(authenticate(db, &sess.access_token).await.is_ok());
}

/// Even an MCP-purpose token that carries a browser session's claims (everything
/// else the extractor checks) is refused because the purpose claim means it has
/// no browser authority.
async fn mcp_purpose_with_browser_claims_is_still_refused(db: &DbPool) {
    let u = user(db).await;
    let sess = login(db, &u).await;
    let browser = jwt::validate_token(&sess.access_token, SECRET)
        .unwrap()
        .claims;
    let extra = browser.extra.clone();
    assert!(extra.contains_key("user_id"), "fixture carries a browser session's claims");
    let forged = jwt::create_mcp_access_token_str(&u.user_id, SECRET, 15, extra).unwrap();
    match authenticate(db, &forged).await {
        Err(krausen_core::Error::Unauthorized(msg)) => {
            assert_eq!(msg, "Token is not a browser session")
        }
        Err(other) => panic!("expected Unauthorized, got {other:?}"),
        Ok(_) => panic!("a purpose token must never authenticate as a browser session"),
    }
}

/// Signing out one browser session revokes its refresh-token family only: the
/// family cannot refresh and has no active row, while an access token already
/// issued authenticates until it expires (it is not bound to any stored record).
async fn single_session_logout_ends_refresh_not_access_token(db: &DbPool) {
    let u = user(db).await;
    let sess = login(db, &u).await;
    let hash = token_service::hash_refresh_token(&sess.refresh_token);
    #[derive(sqlx::FromRow)]
    struct Row {
        token_id: String,
        family_id: String,
    }
    let row = krausen_core::db_fetch_one!(
        db,
        Row,
        "SELECT token_id, family_id FROM refresh_tokens WHERE token_hash = $1",
        &hash
    )
    .unwrap();
    assert!(
        token_service::revoke_user_refresh_token(db, &u.user_id, &row.token_id)
            .await
            .unwrap()
    );
    assert!(
        token_refresh::refresh_tokens(db, SECRET, &sess.refresh_token, &device())
            .await
            .is_err(),
        "the revoked family cannot refresh"
    );
    let active = krausen_core::sql_compat::bool_true(db.is_postgres());
    let sql = format!(
        "SELECT COUNT(*) FROM refresh_tokens WHERE family_id = $1 AND is_active = {active}"
    );
    assert_eq!(
        krausen_core::db_fetch_scalar!(db, i64, &sql, &row.family_id).unwrap(),
        0,
        "the family has no active row"
    );
    assert!(
        authenticate(db, &sess.access_token).await.is_ok(),
        "the access token authenticates until it expires"
    );
    assert_eq!(cutoff(db, &u.user_id).await, None, "no cutoff was set");
}

/// Upgrade path: an access token minted before the credential removal carries
/// `iss`/`aud` claims and is refused by `validate_token` (InvalidAudience); a
/// refresh family that has no credential row (the pre-migration data is gone)
/// still rotates, and the new access token authenticates.
async fn legacy_iss_aud_token_is_rejected_and_a_bare_refresh_family_rotates(db: &DbPool) {
    let u = user(db).await;
    let mut extra = std::collections::HashMap::new();
    extra.insert("iss".to_string(), serde_json::json!("krausen-browser"));
    extra.insert("aud".to_string(), serde_json::json!("krausen-ui"));
    extra.insert("credential_id".to_string(), serde_json::json!("old-credential"));
    let legacy =
        jwt::create_session_access_token_str(&u.user_id, SECRET, 15, extra, u.sessions_valid_from)
            .unwrap();
    assert!(jwt::validate_token(&legacy, SECRET).is_err());
    assert!(matches!(
        authenticate(db, &legacy).await,
        Err(krausen_core::Error::Unauthorized(_))
    ));

    let raw = jwt::create_refresh_token();
    token_service::store_refresh_token(
        db,
        &u.user_id,
        &token_service::hash_refresh_token(&raw),
        chrono::Utc::now() + chrono::Duration::days(1),
        &device(),
        &token_service::generate_family_id(),
    )
    .await
    .unwrap();
    let refreshed = token_refresh::refresh_tokens(db, SECRET, &raw, &device())
        .await
        .unwrap();
    assert!(authenticate(db, &refreshed.access_token).await.is_ok());
}

/// Kyomi parity: the extractor only refuses MCP-purpose tokens (`require_session`),
/// so a `scope = passkey_recovery` token presented as a bearer authenticates like
/// any signed session token (and is exempt from the session cutoff, see
/// `passkey_recovery_scope_is_exempt_from_the_session_cutoff`). The recovery
/// token normally lives only in an HttpOnly cookie the extractor does not read.
async fn a_recovery_scope_token_authenticates_like_any_signed_session_token(db: &DbPool) {
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
    let auth = authenticate(db, &token)
        .await
        .expect("a signed recovery-scope token authenticates (Kyomi behaviour)");
    assert_eq!(auth.user_id, u.user_id);
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
    mcp_token_is_refused_by_the_browser_extractor,
    mcp_purpose_with_browser_claims_is_still_refused,
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
    single_session_logout_ends_refresh_not_access_token,
    legacy_iss_aud_token_is_rejected_and_a_bare_refresh_family_rotates,
    a_recovery_scope_token_authenticates_like_any_signed_session_token,
);
