// SPDX-License-Identifier: AGPL-3.0-or-later
//! A WebSocket upgrade honours the session revocation cutoff
//! (`users.sessions_valid_from`): a session issued before the cutoff is closed
//! with "Session revoked", while a current session stays connected.
//!
//! The upgrade is driven over a raw TCP stream (HTTP upgrade request, then
//! frames), so no extra client dependency is needed.
use krausen_auth::{session, token_service::DeviceInfo, user_service};
use krausen_core::DbPool;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const SECRET: &str = "test-secret-at-least-32-bytes-long";

/// Open a WebSocket for `user_id` with `token` and return the first server frame's
/// close code and reason, or `None` when the server keeps the socket open.
async fn connect(addr: std::net::SocketAddr, user_id: &str, token: &str) -> Option<(u16, String)> {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "GET /ws/{user_id}?token={token} HTTP/1.1\r\nHost: {addr}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();

    // Read the HTTP response head.
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).await.unwrap();
        head.push(byte[0]);
    }
    assert!(
        String::from_utf8_lossy(&head).starts_with("HTTP/1.1 101"),
        "upgrade must be accepted: {}",
        String::from_utf8_lossy(&head)
    );

    // Read frames for a short while: a connected socket greets with a text frame
    // and then stays quiet; a refused one sends a close frame.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(900);
    loop {
        let mut header = [0u8; 2];
        let read =
            tokio::time::timeout_at(deadline, stream.read_exact(&mut header)).await;
        let Ok(read) = read else { return None };
        read.unwrap();
        let opcode = header[0] & 0x0f;
        let mut len = (header[1] & 0x7f) as usize;
        if len == 126 {
            let mut ext = [0u8; 2];
            stream.read_exact(&mut ext).await.unwrap();
            len = u16::from_be_bytes(ext) as usize;
        }
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).await.unwrap();
        if opcode == 0x8 {
            let code = u16::from_be_bytes([payload[0], payload[1]]);
            return Some((code, String::from_utf8_lossy(&payload[2..]).into_owned()));
        }
    }
}

async fn exercise(db: DbPool) {
    let _ = krausen_core::constants::load_with_fallback();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://localhost:{}", addr.port());
    let mut config = krausen_core::Config::test_config();
    config.base_url = base.clone();
    config.frontend_url = base.clone();
    config.jwt_secret = SECRET.into();
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

    let email = format!("{}@example.test", uuid::Uuid::new_v4());
    let user = user_service::create_user(&db, &email, Some("Test"), true)
        .await
        .unwrap();
    user_service::create_workspace_for_user(&db, &user.user_id, Some("Test"), &email, None)
        .await
        .unwrap();
    let device = DeviceInfo {
        user_agent: None,
        ip_address: None,
        country_code: None,
        oauth_client_id: None,
    };
    let sess = session::create_authenticated_session(&db, &kv, SECRET, &user, &device)
        .await
        .unwrap();

    assert_eq!(
        connect(addr, &user.user_id, &sess.access_token).await,
        None,
        "a current session stays connected"
    );

    // Move the cutoff past this session without touching its credential, so the
    // cutoff itself is what refuses the socket.
    let cutoff = chrono::Utc::now().timestamp_micros() + 10_000_000;
    match &db {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE users SET sessions_valid_from = $1 WHERE user_id = $2")
                .bind(cutoff)
                .bind(&user.user_id)
                .execute(p)
                .await
                .unwrap();
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE users SET sessions_valid_from = $1 WHERE user_id = $2")
                .bind(cutoff)
                .bind(&user.user_id)
                .execute(p)
                .await
                .unwrap();
        }
    }
    assert_eq!(
        connect(addr, &user.user_id, &sess.access_token).await,
        Some((4001, "Session revoked".to_string())),
        "a session issued before the cutoff is closed"
    );
    server.abort();
}

#[tokio::test]
async fn sqlite_websocket_honours_the_session_cutoff() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations-sqlite")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Sqlite(pool)).await;
}

#[tokio::test]
async fn postgres_websocket_honours_the_session_cutoff() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let admin = sqlx::PgPool::connect(&url).await.unwrap();
    let name = format!("krausen_wscutoff_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(&admin)
        .await
        .unwrap();
    let mut db_url = url::Url::parse(&url).unwrap();
    db_url.set_path(&name);
    let pool = sqlx::PgPool::connect(db_url.as_str()).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    exercise(DbPool::Postgres(pool.clone())).await;
    pool.close().await;
    sqlx::query(&format!("DROP DATABASE {name} WITH (FORCE)"))
        .execute(&admin)
        .await
        .unwrap();
}
