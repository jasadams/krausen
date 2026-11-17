// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_auth::{
    jwt, session,
    token_refresh::{self, RefreshedTokens},
    token_service::{self, DeviceInfo},
    user_service,
};
use krausen_core::DbPool;

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
    for claimed_email in [email.as_str(), "new-unverified@example.test"] {
        let result = krausen_auth::auth_service::passkey_register_start_service(
            krausen_auth::auth_service::PasskeyRegisterStartParams {
                db: &db,
                kv: &kv,
                webauthn: &webauthn,
                email: claimed_email,
                name: "Claimed",
                device_name: "Device",
                ip: "test",
                self_hosted: true,
                smtp_configured: false,
                frontend_url: "http://localhost",
            },
        )
        .await;
        assert!(result.is_err());
    }
    assert!(
        user_service::get_user_by_email(&db, "new-unverified@example.test")
            .await
            .unwrap()
            .is_none()
    );
    krausen_auth::redis_ops::store_webauthn_challenge(
        &kv,
        "old-unbound",
        &serde_json::json!({"user_id":user.user_id,"email":email}),
    )
    .await
    .unwrap();
    assert!(
        krausen_auth::auth_service::passkey_register_complete_service(
            &db,
            &kv,
            "test-secret-at-least-32-bytes-long",
            &webauthn,
            "old-unbound",
            "{}",
            &device
        )
        .await
        .is_err()
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
    let claims =
        jwt::validate_browser_token(&issued.access_token, "test-secret-at-least-32-bytes-long")
            .unwrap()
            .claims;
    let credential = claims
        .extra
        .get("credential_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let actor = krausen_app::ActorContext {
        actor_id: user.user_id.clone(),
        workspace_id: ws,
        credential_id: credential,
        access_expires_at: chrono::Utc::now().timestamp() + 3600,
    };
    assert!(krausen_app::workspace_snapshot(&db, &actor).await.is_ok());
    exercise_oauth(&db, &actor).await;
    let rotated = token_refresh::refresh_tokens(
        &db,
        "test-secret-at-least-32-bytes-long",
        &issued.refresh_token,
        &device,
    )
    .await
    .unwrap();
    assert!(
        jwt::validate_browser_token(&rotated.access_token, "test-secret-at-least-32-bytes-long")
            .is_ok()
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
    assert!(matches!(
        krausen_app::workspace_snapshot(&db, &actor).await,
        Err(krausen_app::Error::Forbidden)
    ));
    // A real revocation waits for an authorized mutation's held authority
    // lock, and all later operations observe the revoked family.
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
        jwt::validate_browser_token(&second.access_token, "test-secret-at-least-32-bytes-long")
            .unwrap()
            .claims;
    let second_actor = krausen_app::ActorContext {
        actor_id: actor.actor_id.clone(),
        workspace_id: actor.workspace_id.clone(),
        credential_id: second_claims.extra["credential_id"]
            .as_str()
            .unwrap()
            .into(),
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
    let family = second_actor.credential_id.clone();
    let mut revoke = tokio::spawn(async move {
        krausen_auth::token_service::revoke_token_family(&other_db, &family).await
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
    let legacy = jwt::create_access_token_str(
        &user.user_id,
        "test-secret-at-least-32-bytes-long",
        10,
        Default::default(),
    )
    .unwrap();
    assert!(jwt::validate_browser_token(&legacy, "test-secret-at-least-32-bytes-long").is_err());
}

async fn exercise_oauth(db: &DbPool, actor: &krausen_app::ActorContext) {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use krausen_auth::oauth_service::{self, AuthorizationRequest, Principal};
    use sha2::{Digest, Sha256};
    let principal = Principal {
        user_id: actor.actor_id.clone(),
        credential_id: actor.credential_id.clone(),
        access_expires_at: actor.access_expires_at,
    };
    assert!(
        oauth_service::register_client(db, "bad", &["http://example.test/callback".into()])
            .await
            .is_err()
    );
    let redirect = "http://127.0.0.1:34567/callback";
    let client = oauth_service::register_client(db, "client", &[redirect.into()])
        .await
        .unwrap();
    let verifier = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let request = AuthorizationRequest {
        client_id: client.client_id.clone(),
        redirect_uri: redirect.into(),
        resource: "https://example.test/mcp".into(),
        state: "opaque-state".into(),
        code_challenge: challenge,
        code_challenge_method: "S256".into(),
        response_type: "code".into(),
    };
    let pending = oauth_service::begin_authorization(db, &request, "https://example.test/mcp")
        .await
        .unwrap();
    assert!(
        oauth_service::approve(db, &pending, &principal)
            .await
            .is_err()
    );
    oauth_service::bind_pending(db, &pending, &principal)
        .await
        .unwrap();
    let (_, state, code) = oauth_service::approve(db, &pending, &principal)
        .await
        .unwrap();
    assert_eq!(state, "opaque-state");
    assert!(
        oauth_service::approve(db, &pending, &principal)
            .await
            .is_err()
    );
    assert!(
        oauth_service::exchange_code(
            db,
            "https://example.test",
            &client.client_id,
            None,
            redirect,
            "https://example.test/mcp",
            &code,
            "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB"
        )
        .await
        .is_err()
    );
    let token = oauth_service::exchange_code(
        db,
        "https://example.test",
        &client.client_id,
        None,
        redirect,
        "https://example.test/mcp",
        &code,
        verifier,
    )
    .await
    .unwrap();
    assert!(
        oauth_service::exchange_code(
            db,
            "https://example.test",
            &client.client_id,
            None,
            redirect,
            "https://example.test/mcp",
            &code,
            verifier
        )
        .await
        .is_err()
    );
    assert!(
        oauth_service::verify_access(
            db,
            "https://example.test",
            "https://other.test/mcp",
            &token.access_token
        )
        .await
        .is_err()
    );
    let agent = oauth_service::verify_access(
        db,
        "https://example.test",
        "https://example.test/mcp",
        &token.access_token,
    )
    .await
    .unwrap();
    let agent_actor = krausen_app::ActorContext {
        actor_id: agent.user_id,
        workspace_id: actor.workspace_id.clone(),
        credential_id: agent.credential_id,
        access_expires_at: agent.access_expires_at,
    };
    assert!(
        krausen_app::workspace_snapshot(db, &agent_actor)
            .await
            .is_ok()
    );
    let rotated = oauth_service::refresh(
        db,
        "https://example.test",
        &client.client_id,
        None,
        "https://example.test/mcp",
        &token.refresh_token,
    )
    .await
    .unwrap();
    assert!(
        oauth_service::refresh(
            db,
            "https://example.test",
            &client.client_id,
            None,
            "https://example.test/mcp",
            &token.refresh_token
        )
        .await
        .is_err()
    );
    assert!(
        oauth_service::verify_access(
            db,
            "https://example.test",
            "https://example.test/mcp",
            &rotated.access_token
        )
        .await
        .is_err()
    );
    assert!(matches!(
        krausen_app::workspace_snapshot(db, &agent_actor).await,
        Err(krausen_app::Error::Forbidden)
    ));
    let confidential = oauth_service::register_client_with_method(
        db,
        "confidential",
        &[redirect.into()],
        "client_secret_basic",
    )
    .await
    .unwrap();
    let mut request = request;
    request.client_id = confidential.client_id.clone();
    let pending = oauth_service::begin_authorization(db, &request, "https://example.test/mcp")
        .await
        .unwrap();
    oauth_service::bind_pending(db, &pending, &principal)
        .await
        .unwrap();
    let (_, _, code) = oauth_service::approve(db, &pending, &principal)
        .await
        .unwrap();
    assert!(
        oauth_service::exchange_code(
            db,
            "https://example.test",
            &confidential.client_id,
            None,
            redirect,
            "https://example.test/mcp",
            &code,
            verifier
        )
        .await
        .is_err()
    );
    assert!(
        oauth_service::exchange_code(
            db,
            "https://example.test",
            &confidential.client_id,
            Some("incorrect"),
            redirect,
            "https://example.test/mcp",
            &code,
            verifier
        )
        .await
        .is_err()
    );
    let secret = confidential.client_secret.as_deref().unwrap();
    let issued = oauth_service::exchange_code(
        db,
        "https://example.test",
        &confidential.client_id,
        Some(secret),
        redirect,
        "https://example.test/mcp",
        &code,
        verifier,
    )
    .await
    .unwrap();
    assert!(
        oauth_service::refresh(
            db,
            "https://example.test",
            &confidential.client_id,
            Some("incorrect"),
            "https://example.test/mcp",
            &issued.refresh_token
        )
        .await
        .is_err()
    );
    assert!(
        oauth_service::refresh(
            db,
            "https://example.test",
            &client.client_id,
            None,
            "https://example.test/mcp",
            &issued.refresh_token
        )
        .await
        .is_err()
    );
    let next = oauth_service::refresh(
        db,
        "https://example.test",
        &confidential.client_id,
        Some(secret),
        "https://example.test/mcp",
        &issued.refresh_token,
    )
    .await
    .unwrap();
    oauth_service::revoke(
        db,
        &confidential.client_id,
        Some(secret),
        &next.refresh_token,
    )
    .await
    .unwrap();
    assert!(
        oauth_service::verify_access(
            db,
            "https://example.test",
            "https://example.test/mcp",
            &next.access_token
        )
        .await
        .is_err()
    );
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
async fn sqlite_browser_credential_lifecycle() {
    exercise(sqlite().await).await;
}

#[tokio::test]
async fn postgres_browser_credential_lifecycle() {
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
    credential_id: String,
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
    let credential_id = jwt::validate_browser_token(&issued.access_token, SECRET)
        .unwrap()
        .claims
        .extra["credential_id"]
        .as_str()
        .unwrap()
        .to_owned();
    Fixture {
        db,
        device,
        refresh_token: issued.refresh_token,
        credential_id,
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

    async fn credential_revoked(&self) -> bool {
        #[derive(sqlx::FromRow)]
        struct Row {
            revoked: i64,
        }
        let row = krausen_core::db_fetch_optional!(
            &self.db,
            Row,
            "SELECT CAST(CASE WHEN revoked_at IS NULL THEN 0 ELSE 1 END AS BIGINT) AS revoked FROM auth_credentials WHERE credential_id = $1",
            &self.credential_id
        )
        .unwrap()
        .unwrap();
        row.revoked == 1
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
    assert!(jwt::validate_browser_token(&second.access_token, SECRET).is_ok());
    // Each tab holds its own fresh token and both stay usable.
    f.refresh(&first.raw_refresh_token).await.unwrap();
    f.refresh(&second.raw_refresh_token).await.unwrap();
    assert!(!f.credential_revoked().await);
}

/// Reuse after the grace period is theft: the family and credential are revoked.
async fn reuse_after_grace_revokes_the_family(db: DbPool) {
    let f = fixture(db).await;
    let first = f.refresh(&f.refresh_token).await.unwrap();
    f.replaced_ago(&f.refresh_token, grace_seconds() + 1).await;
    let reuse = f.refresh(&f.refresh_token).await;
    assert!(
        is_unauthorized(&reuse),
        "reuse after grace must be rejected"
    );
    assert!(f.credential_revoked().await, "the credential is revoked");
    // The token rotated from it is part of the revoked family too.
    assert!(is_unauthorized(&f.refresh(&first.raw_refresh_token).await));
}

/// The grace period boundary: replaced 29 s ago is accepted, 31 s ago is theft.
async fn grace_boundary(db: DbPool) {
    let f = fixture(db).await;
    f.refresh(&f.refresh_token).await.unwrap();
    f.replaced_ago(&f.refresh_token, grace_seconds() - 1).await;
    f.refresh(&f.refresh_token).await.unwrap();
    assert!(!f.credential_revoked().await);
    f.replaced_ago(&f.refresh_token, grace_seconds() + 1).await;
    assert!(is_unauthorized(&f.refresh(&f.refresh_token).await));
    assert!(f.credential_revoked().await);
}

/// The rotation committed but its response was dropped: retrying with the old token
/// inside the grace period succeeds, so a lost response never signs the user out.
async fn lost_response_replay_succeeds(db: DbPool) {
    let f = fixture(db).await;
    drop(f.refresh(&f.refresh_token).await.unwrap());
    let retry = f.refresh(&f.refresh_token).await.unwrap();
    assert!(jwt::validate_browser_token(&retry.access_token, SECRET).is_ok());
    assert!(!f.credential_revoked().await);
}

/// A revoked or expired credential is rejected even inside the grace period.
async fn revoked_or_expired_credential_is_rejected_inside_grace(db: DbPool) {
    let revoked = fixture(db.clone()).await;
    revoked.refresh(&revoked.refresh_token).await.unwrap();
    krausen_core::db_execute!(
        &revoked.db,
        "UPDATE auth_credentials SET revoked_at = $1 WHERE credential_id = $2",
        chrono::Utc::now().timestamp(),
        &revoked.credential_id
    )
    .unwrap();
    assert!(is_unauthorized(
        &revoked.refresh(&revoked.refresh_token).await
    ));

    let expired = fixture(db).await;
    expired.refresh(&expired.refresh_token).await.unwrap();
    krausen_core::db_execute!(
        &expired.db,
        "UPDATE auth_credentials SET expires_at = $1 WHERE credential_id = $2",
        chrono::Utc::now().timestamp() - 10,
        &expired.credential_id
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
    assert!(!f.credential_revoked().await);
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
    revoked_or_expired_credential_is_rejected_inside_grace => revoked_or_expired_sqlite, revoked_or_expired_postgres;
}
