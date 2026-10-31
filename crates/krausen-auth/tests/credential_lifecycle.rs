// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_auth::{jwt, session, token_refresh, token_service::DeviceInfo, user_service};
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

#[tokio::test]
async fn sqlite_browser_credential_lifecycle() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../apps/server/migrations-sqlite")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Sqlite(pool)).await;
}

#[tokio::test]
async fn postgres_browser_credential_lifecycle() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
