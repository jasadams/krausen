// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_auth::{
    jwt, session,
    token_refresh::{self, RefreshedTokens},
    token_service::{self, DeviceInfo},
    user_service,
};
use krausen_core::DbPool;

const REGISTER_CREDENTIAL_JSON: &str = r#"
    {
        "id":"0xYE4bQ_HZM51-XYwp7WHJu8RfeA2Oz3_9HnNIZAKqRTz9gsUlF3QO7EqcJ0pgLSwDcq6cL1_aQpTtKLeGu6Ig",
        "rawId":"0xYE4bQ_HZM51-XYwp7WHJu8RfeA2Oz3_9HnNIZAKqRTz9gsUlF3QO7EqcJ0pgLSwDcq6cL1_aQpTtKLeGu6Ig",
        "response":{
             "attestationObject":"o2NmbXRoZmlkby11MmZnYXR0U3RtdKJjc2lnWEcwRQIhALjRb43YFcbJ3V9WiYPpIrZkhgzAM6KTR8KIjwCXejBCAiAO5Lvp1VW4dYBhBDv7HZIrxZb1SwKKYOLfFRXykRxMqGN4NWOBWQLBMIICvTCCAaWgAwIBAgIEGKxGwDANBgkqhkiG9w0BAQsFADAuMSwwKgYDVQQDEyNZdWJpY28gVTJGIFJvb3QgQ0EgU2VyaWFsIDQ1NzIwMDYzMTAgFw0xNDA4MDEwMDAwMDBaGA8yMDUwMDkwNDAwMDAwMFowbjELMAkGA1UEBhMCU0UxEjAQBgNVBAoMCVl1YmljbyBBQjEiMCAGA1UECwwZQXV0aGVudGljYXRvciBBdHRlc3RhdGlvbjEnMCUGA1UEAwweWXViaWNvIFUyRiBFRSBTZXJpYWwgNDEzOTQzNDg4MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEeeo7LHxJcBBiIwzSP-tg5SkxcdSD8QC-hZ1rD4OXAwG1Rs3Ubs_K4-PzD4Hp7WK9Jo1MHr03s7y-kqjCrutOOqNsMGowIgYJKwYBBAGCxAoCBBUxLjMuNi4xLjQuMS40MTQ4Mi4xLjcwEwYLKwYBBAGC5RwCAQEEBAMCBSAwIQYLKwYBBAGC5RwBAQQEEgQQy2lIHo_3QDmT7AonKaFUqDAMBgNVHRMBAf8EAjAAMA0GCSqGSIb3DQEBCwUAA4IBAQCXnQOX2GD4LuFdMRx5brr7Ivqn4ITZurTGG7tX8-a0wYpIN7hcPE7b5IND9Nal2bHO2orh_tSRKSFzBY5e4cvda9rAdVfGoOjTaCW6FZ5_ta2M2vgEhoz5Do8fiuoXwBa1XCp61JfIlPtx11PXm5pIS2w3bXI7mY0uHUMGvxAzta74zKXLslaLaSQibSKjWKt9h-SsXy4JGqcVefOlaQlJfXL1Tga6wcO0QTu6Xq-Uw7ZPNPnrpBrLauKDd202RlN4SP7ohL3d9bG6V5hUz_3OusNEBZUn5W3VmPj1ZnFavkMB3RkRMOa58MZAORJT4imAPzrvJ0vtv94_y71C6tZ5aGF1dGhEYXRhWMQSyhe0mvIolDbzA-AWYDCiHlJdJm4gkmdDOAGo_UBxoEEAAAAAAAAAAAAAAAAAAAAAAAAAAABA0xYE4bQ_HZM51-XYwp7WHJu8RfeA2Oz3_9HnNIZAKqRTz9gsUlF3QO7EqcJ0pgLSwDcq6cL1_aQpTtKLeGu6IqUBAgMmIAEhWCCe1KvqpcVWN416_QZc8vJynt3uo3_WeJ2R4uj6kJbaiiJYIDC5ssxxummKviGgLoP9ZLFb836A9XfRO7op18QY3i5m",
             "clientDataJSON":"eyJjaGFsbGVuZ2UiOiJBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBIiwiY2xpZW50RXh0ZW5zaW9ucyI6e30sImhhc2hBbGdvcml0aG0iOiJTSEEtMjU2Iiwib3JpZ2luIjoiaHR0cDovLzEyNy4wLjAuMTo4MDgwIiwidHlwZSI6IndlYmF1dGhuLmNyZWF0ZSJ9"
        },
        "type":"public-key"}
    "#;

async fn exercise(db: DbPool) {
    let _ = krausen_core::constants::load_with_fallback();
    let suffix = uuid::Uuid::new_v4();
    let email = format!("{suffix}@example.test");
    let user = user_service::create_user(&db, &email, Some("Brewer"), true)
        .await
        .unwrap();
    let ws =
        user_service::create_workspace_for_user(&db, &user.user_id, Some("Brewer"), &email, None)
            .await
            .unwrap();
    krausen_core::db_execute!(
        &db,
        "UPDATE workspaces SET status = 'active' WHERE workspace_id = $1",
        &ws
    )
    .unwrap();
    let device = DeviceInfo {
        user_agent: None,
        ip_address: None,
        country_code: None,
        oauth_client_id: None,
    };
    let kv = krausen_core::kv_store_memory::InMemoryKVStore::new_pool();
    // Neither claimed email nor an old unbound challenge can enroll a passkey.
    let webauthn = krausen_auth::webauthn::build_webauthn(
        "localhost",
        "test",
        &url::Url::parse("http://localhost").unwrap(),
    )
    .unwrap();
    // A challenge with no purpose, however plausible its user_id, cannot be
    // completed through the authenticated add-device path.
    krausen_auth::redis_ops::store_webauthn_challenge(
        &kv,
        "old-unbound",
        &serde_json::json!({"user_id":user.user_id,"email":email}),
    )
    .await
    .unwrap();
    let credential: webauthn_rs::prelude::RegisterPublicKeyCredential =
        serde_json::from_str(REGISTER_CREDENTIAL_JSON).unwrap();
    let rejected = krausen_auth::security_service::complete_passkey_registration(
        &db,
        &kv,
        &webauthn,
        &user.user_id,
        "old-unbound",
        &credential,
    )
    .await;
    assert!(
        matches!(rejected, Err(krausen_core::Error::Internal(ref m)) if m == "Invalid or expired challenge"),
        "an unbound challenge must be rejected as invalid or expired"
    );
    assert!(
        user_service::get_passkey_credentials(&db, &user.user_id)
            .await
            .unwrap()
            .is_empty()
    );

    let issued = session::create_authenticated_session(
        &db,
        &kv,
        "test-secret-at-least-32-bytes-long",
        &user,
        &device,
    )
    .await
    .unwrap();
    let claims = jwt::validate_token(&issued.access_token, "test-secret-at-least-32-bytes-long")
        .unwrap()
        .claims;
    let actor = krausen_app::ActorContext {
        actor_id: user.user_id.clone(),
        workspace_id: ws,
        session_iat_us: Some(claims.session_iat_us.unwrap()),
        access_expires_at: chrono::Utc::now().timestamp() + 3600,
    };
    assert!(krausen_app::workspace_snapshot(&db, &actor).await.is_ok());
    let rotated = token_refresh::refresh_tokens(
        &db,
        "test-secret-at-least-32-bytes-long",
        &issued.refresh_token,
        &device,
    )
    .await
    .unwrap();
    assert!(
        jwt::validate_token(&rotated.access_token, "test-secret-at-least-32-bytes-long").is_ok()
    );
    // Inside the grace period the replaced token is still accepted (it rotates again).
    token_refresh::refresh_tokens(
        &db,
        "test-secret-at-least-32-bytes-long",
        &issued.refresh_token,
        &device,
    )
    .await
    .unwrap();
    // Reuse AFTER the grace period is theft: it is rejected and revokes the family.
    let old_hash = krausen_auth::token_service::hash_refresh_token(&issued.refresh_token);
    age_replaced_token(&db, &old_hash, grace_seconds() + 1).await;
    assert!(
        token_refresh::refresh_tokens(
            &db,
            "test-secret-at-least-32-bytes-long",
            &issued.refresh_token,
            &device
        )
        .await
        .is_err()
    );
    assert!(
        token_refresh::refresh_tokens(
            &db,
            "test-secret-at-least-32-bytes-long",
            &rotated.raw_refresh_token,
            &device
        )
        .await
        .is_err()
    );
    // Theft revokes the refresh-token family, not access tokens already issued.
    assert!(krausen_app::workspace_snapshot(&db, &actor).await.is_ok());
    // A real revocation waits for an authorized mutation's held authority
    // lock, and all later operations observe the new session cutoff.
    let second = session::create_authenticated_session(
        &db,
        &kv,
        "test-secret-at-least-32-bytes-long",
        &user,
        &device,
    )
    .await
    .unwrap();
    let second_claims =
        jwt::validate_token(&second.access_token, "test-secret-at-least-32-bytes-long")
            .unwrap()
            .claims;
    let second_actor = krausen_app::ActorContext {
        actor_id: actor.actor_id.clone(),
        workspace_id: actor.workspace_id.clone(),
        session_iat_us: second_claims.session_iat_us,
        access_expires_at: second_claims.exp,
    };
    let mut held = db.begin_write().await.unwrap();
    krausen_app::authorize_workspace(&mut held, &second_actor)
        .await
        .unwrap();
    krausen_core::dbtx_execute!(
        &mut held,
        "UPDATE workspaces SET name = $1 WHERE workspace_id = $2",
        "committed before revocation",
        &second_actor.workspace_id
    )
    .unwrap();
    let other_db = db.clone();
    let revoked_user = second_actor.actor_id.clone();
    let mut revoke = tokio::spawn(async move {
        krausen_auth::token_service::revoke_all_user_sessions(&other_db, &revoked_user).await
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut revoke)
            .await
            .is_err()
    );
    held.commit().await.unwrap();
    revoke.await.unwrap().unwrap();
    assert!(matches!(
        krausen_app::workspace_snapshot(&db, &second_actor).await,
        Err(krausen_app::Error::Forbidden)
    ));
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
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    DbPool::Postgres(pool)
}

#[tokio::test]
async fn sqlite_browser_refresh_lifecycle() {
    exercise(sqlite().await).await;
}

#[tokio::test]
async fn postgres_browser_refresh_lifecycle() {
    exercise(postgres().await).await;
}

/// Pretend the refresh token with this hash was replaced `seconds` ago.
async fn age_replaced_token(db: &DbPool, hash: &str, seconds: i64) {
    let sql = if db.is_postgres() {
        format!(
            "UPDATE refresh_tokens SET replaced_at = NOW() - INTERVAL '{seconds} seconds' WHERE token_hash = $1"
        )
    } else {
        format!(
            "UPDATE refresh_tokens SET replaced_at = datetime('now', '-{seconds} seconds') WHERE token_hash = $1"
        )
    };
    krausen_core::db_execute!(db, &sql, hash).unwrap();
}

// Refresh grace period (Kyomi parity): reuse inside it rotates again; reuse after it revokes.
const SECRET: &str = "test-secret-at-least-32-bytes-long";

fn grace_seconds() -> i64 {
    krausen_core::constants::get()
        .jwt
        .refresh_token_grace_period_seconds
}

struct Fixture {
    db: DbPool,
    device: DeviceInfo,
    refresh_token: String,
    user_id: String,
}

async fn fixture(db: DbPool) -> Fixture {
    let _ = krausen_core::constants::load_with_fallback();
    let email = format!("{}@example.test", uuid::Uuid::new_v4());
    let user = user_service::create_user(&db, &email, Some("Brewer"), true)
        .await
        .unwrap();
    user_service::create_workspace_for_user(&db, &user.user_id, Some("Brewer"), &email, None)
        .await
        .unwrap();
    let device = DeviceInfo {
        user_agent: None,
        ip_address: None,
        country_code: None,
        oauth_client_id: None,
    };
    let kv = krausen_core::kv_store_memory::InMemoryKVStore::new_pool();
    let issued = session::create_authenticated_session(&db, &kv, SECRET, &user, &device)
        .await
        .unwrap();
    Fixture {
        db,
        device,
        refresh_token: issued.refresh_token,
        user_id: user.user_id,
    }
}

impl Fixture {
    async fn refresh(&self, raw: &str) -> krausen_core::Result<RefreshedTokens> {
        token_refresh::refresh_tokens(&self.db, SECRET, raw, &self.device).await
    }

    /// Pretend the token was replaced `seconds` ago.
    async fn replaced_ago(&self, raw: &str, seconds: i64) {
        age_replaced_token(&self.db, &token_service::hash_refresh_token(raw), seconds).await;
    }

    /// Whether the user has no active refresh token left (the family is revoked).
    async fn family_revoked(&self) -> bool {
        let active = krausen_core::sql_compat::bool_true(self.db.is_postgres());
        let sql = format!(
            "SELECT COUNT(*) FROM refresh_tokens WHERE user_id = $1 AND is_active = {active}"
        );
        krausen_core::db_fetch_scalar!(&self.db, i64, &sql, &self.user_id).unwrap() == 0
    }
}

fn is_unauthorized<T>(result: &krausen_core::Result<T>) -> bool {
    matches!(result, Err(krausen_core::Error::Unauthorized(_)))
}

/// Reuse of a replaced token inside the grace period succeeds and rotates again.
async fn reuse_inside_grace_succeeds_and_rotates_again(db: DbPool) {
    let f = fixture(db).await;
    let first = f.refresh(&f.refresh_token).await.unwrap();
    let second = f.refresh(&f.refresh_token).await.unwrap();
    assert_ne!(first.raw_refresh_token, second.raw_refresh_token);
    assert!(jwt::validate_token(&second.access_token, SECRET).is_ok());
    // Each tab holds its own fresh token and both stay usable.
    f.refresh(&first.raw_refresh_token).await.unwrap();
    f.refresh(&second.raw_refresh_token).await.unwrap();
    assert!(!f.family_revoked().await);
}

/// Reuse after the grace period is theft: the family is revoked.
async fn reuse_after_grace_revokes_the_family(db: DbPool) {
    let f = fixture(db).await;
    let first = f.refresh(&f.refresh_token).await.unwrap();
    f.replaced_ago(&f.refresh_token, grace_seconds() + 1).await;
    let reuse = f.refresh(&f.refresh_token).await;
    assert!(
        is_unauthorized(&reuse),
        "reuse after grace must be rejected"
    );
    assert!(f.family_revoked().await, "the family is revoked");
    // The token rotated from it is part of the revoked family too.
    assert!(is_unauthorized(&f.refresh(&first.raw_refresh_token).await));
}

/// The grace period boundary: replaced 29 s ago is accepted, 31 s ago is theft.
async fn grace_boundary(db: DbPool) {
    let f = fixture(db).await;
    f.refresh(&f.refresh_token).await.unwrap();
    f.replaced_ago(&f.refresh_token, grace_seconds() - 1).await;
    f.refresh(&f.refresh_token).await.unwrap();
    assert!(!f.family_revoked().await);
    f.replaced_ago(&f.refresh_token, grace_seconds() + 1).await;
    assert!(is_unauthorized(&f.refresh(&f.refresh_token).await));
    assert!(f.family_revoked().await);
}

/// The rotation committed but its response was dropped: retrying with the old token
/// inside the grace period succeeds, so a lost response never signs the user out.
async fn lost_response_replay_succeeds(db: DbPool) {
    let f = fixture(db).await;
    drop(f.refresh(&f.refresh_token).await.unwrap());
    let retry = f.refresh(&f.refresh_token).await.unwrap();
    assert!(jwt::validate_token(&retry.access_token, SECRET).is_ok());
    assert!(!f.family_revoked().await);
}

/// A revoked session or an expired refresh token is rejected even inside the grace period.
async fn revoked_or_expired_token_is_rejected_inside_grace(db: DbPool) {
    let revoked = fixture(db.clone()).await;
    revoked.refresh(&revoked.refresh_token).await.unwrap();
    token_service::revoke_all_user_sessions(&revoked.db, &revoked.user_id)
        .await
        .unwrap();
    assert!(is_unauthorized(
        &revoked.refresh(&revoked.refresh_token).await
    ));

    let expired = fixture(db).await;
    expired.refresh(&expired.refresh_token).await.unwrap();
    let past = chrono::Utc::now() - chrono::Duration::seconds(10);
    krausen_core::db_execute!(
        &expired.db,
        "UPDATE refresh_tokens SET expires_at = $1 WHERE user_id = $2",
        &past,
        &expired.user_id
    )
    .unwrap();
    assert!(is_unauthorized(
        &expired.refresh(&expired.refresh_token).await
    ));
}

/// Two simultaneous rotations of the same current token both succeed with distinct tokens.
async fn concurrent_rotations_of_one_token_both_succeed(db: DbPool) {
    let f = fixture(db).await;
    let (a, b) = tokio::join!(f.refresh(&f.refresh_token), f.refresh(&f.refresh_token));
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.raw_refresh_token, b.raw_refresh_token);
    assert!(!f.family_revoked().await);
}

macro_rules! on_both_backends {
    ($($scenario:ident => $sqlite:ident, $postgres:ident;)+) => {$(
        #[tokio::test]
        async fn $sqlite() { $scenario(sqlite().await).await; }
        #[tokio::test]
        async fn $postgres() { $scenario(postgres().await).await; }
    )+};
}

on_both_backends! {
    reuse_inside_grace_succeeds_and_rotates_again => reuse_inside_grace_sqlite, reuse_inside_grace_postgres;
    reuse_after_grace_revokes_the_family => reuse_after_grace_sqlite, reuse_after_grace_postgres;
    grace_boundary => grace_boundary_sqlite, grace_boundary_postgres;
    lost_response_replay_succeeds => lost_response_replay_sqlite, lost_response_replay_postgres;
    concurrent_rotations_of_one_token_both_succeed => concurrent_sqlite, concurrent_postgres;
    revoked_or_expired_token_is_rejected_inside_grace => revoked_or_expired_sqlite, revoked_or_expired_postgres;
}
