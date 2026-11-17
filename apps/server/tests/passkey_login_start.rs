// SPDX-License-Identifier: AGPL-3.0-or-later
//! The passkey login challenge is minted by a server function; this drives it over
//! HTTP and reads the stored blob back, so a mint that drops or mistypes the
//! WebAuthn `purpose` fails here (every consumer test hand-builds its challenge).
use krausen_core::DbPool;
use leptos::server_fn::ServerFn;
use std::sync::Arc;

#[tokio::test]
async fn passkey_login_start_stores_a_challenge_bound_to_the_login_purpose() {
    let _ = krausen_core::constants::load_with_fallback();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations-sqlite")
        .run(&pool)
        .await
        .unwrap();
    let db = DbPool::Sqlite(pool);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://localhost:{}", listener.local_addr().unwrap().port());
    let mut config = krausen_core::Config::test_config();
    config.base_url = base.clone();
    config.frontend_url = base.clone();
    config.jwt_secret = "test-secret-at-least-32-bytes-long".into();
    let kv = krausen_core::kv_store_memory::InMemoryKVStore::new_pool();
    let webauthn = krausen_auth::webauthn::build_webauthn(
        "localhost",
        "test",
        &url::Url::parse(&base).unwrap(),
    )
    .unwrap();
    let state = krausen_server::state::AppState {
        db: db.clone(),
        kv: kv.clone(),
        redis: None,
        config: Arc::new(config),
        encryption_key: Arc::new([0; 32]),
        webauthn: Arc::new(webauthn),
        ws_manager: krausen_auth::websocket::WebSocketManager::new(None, db.clone()),
        mcp_sessions: krausen_auth::mcp_session_manager::MCPSessionManager::new(kv.clone()),
    };
    let server = tokio::spawn(async move {
        axum::serve(listener, krausen_server::build_router(state))
            .await
            .unwrap()
    });

    let url = format!(
        "{base}{}",
        krausen_ui::server_fns::auth::PasskeyLoginStart::PATH
    );
    let response = reqwest::Client::new().post(url).send().await.unwrap();
    assert!(response.status().is_success(), "HTTP {}", response.status());
    let body: serde_json::Value = response.json().await.unwrap();
    let challenge_id = body["challenge_id"].as_str().expect("challenge_id").to_string();

    let stored = krausen_auth::redis_ops::get_webauthn_challenge(&kv, &challenge_id)
        .await
        .unwrap()
        .expect("the challenge was stored");
    assert_eq!(
        stored.get("purpose").and_then(|v| v.as_str()),
        Some(krausen_auth::webauthn_challenge_purpose::PASSKEY_LOGIN),
        "a login challenge must be minted with the login purpose"
    );
    server.abort();
}
