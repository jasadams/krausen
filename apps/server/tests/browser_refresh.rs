// SPDX-License-Identifier: AGPL-3.0-or-later
//! Real-browser cross-tab refresh and stale-cookie adapter regression.
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::Html,
    routing::get,
};
use krausen_auth::{session, token_service::DeviceInfo, user_service};
use krausen_core::DbPool;
use std::{
    io::Write,
    process::{Command, Stdio},
    sync::Arc,
};

async fn page() -> Html<String> {
    let index = include_str!("../../../crates/krausen-ui/index.html");
    let script = index
        .split("<script type=\"module\">")
        .nth(1)
        .unwrap()
        .split("</script>")
        .next()
        .unwrap();
    Html(format!(
        "<!doctype html><script type=\"module\">{script}</script>"
    ))
}
async fn probe(
    State(state): State<krausen_server::state::AppState>,
    headers: HeaderMap,
) -> StatusCode {
    let Some(token) = krausen_auth::cookies::get_cookie_value(&headers, "access_token") else {
        return StatusCode::UNAUTHORIZED;
    };
    if krausen_auth::jwt::validate_browser_token(token, &state.config.jwt_secret).is_ok() {
        StatusCode::OK
    } else {
        StatusCode::UNAUTHORIZED
    }
}

#[tokio::test]
async fn authorized_workspace_preference_is_reflected_by_browser_refresh() {
    let _ = krausen_core::constants::load_with_fallback();
    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations-sqlite").run(&pool).await.unwrap();
    let db = DbPool::Sqlite(pool.clone());
    let kv = krausen_core::kv_store_memory::InMemoryKVStore::new_pool();
    let user = user_service::create_user(&db, "switch@example.test", Some("Switcher"), true).await.unwrap();
    user_service::create_workspace_for_user(&db, &user.user_id, Some("Switcher"), &user.email, None).await.unwrap();
    sqlx::query("INSERT INTO workspaces (workspace_id,owner_user_id,status) VALUES ('second',$1,'active')").bind(&user.user_id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO workspace_users (workspace_id,user_id,role,active) VALUES ('second',$1,'workspace_user',TRUE)").bind(&user.user_id).execute(&pool).await.unwrap();
    let device = DeviceInfo { user_agent: None, ip_address: None, country_code: None, oauth_client_id: None };
    let secret = "test-secret-at-least-32-bytes-long";
    let session = session::create_authenticated_session(&db, &kv, secret, &user, &device).await.unwrap();
    let token = krausen_auth::jwt::validate_browser_token(&session.access_token, secret).unwrap();
    let actor = krausen_app::ActorContext { actor_id: user.user_id.clone(), credential_id: token.claims.extra["credential_id"].as_str().unwrap().into(), access_expires_at: token.claims.exp, workspace_id: "second".into() };
    let account = krausen_app::invitation::AccountActor { actor_id: actor.actor_id.clone(), credential_id: actor.credential_id.clone(), access_expires_at: actor.access_expires_at };
    let revision = krausen_app::account_read::profile(&db, &account).await.unwrap().settings_revision;
    krausen_app::account_update::workspace(&db, &actor, revision).await.unwrap();
    let refreshed = krausen_auth::token_refresh::refresh_tokens(&db, secret, &session.refresh_token, &device).await.unwrap();
    let new_token = krausen_auth::jwt::validate_browser_token(&refreshed.access_token, secret).unwrap();
    assert_eq!(new_token.claims.extra["workspace_id"].as_str(), Some("second"));
    assert_ne!(token.claims.extra["workspace_id"].as_str(), Some("second"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_expiry_and_browser_tabs_remain_usable() {
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
    let router = krausen_server::build_router(state.clone())
        .route("/test-refresh", get(page))
        .route("/test-probe", get(probe).with_state(state));
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let email = format!("{}@example.test", uuid::Uuid::new_v4());
    let user = user_service::create_user(&db, &email, Some("test"), true)
        .await
        .unwrap();
    user_service::create_workspace_for_user(&db, &user.user_id, Some("test"), &email, None)
        .await
        .unwrap();
    let device = DeviceInfo {
        user_agent: None,
        ip_address: None,
        country_code: None,
        oauth_client_id: None,
    };
    let session = session::create_authenticated_session(
        &db,
        &kv,
        "test-secret-at-least-32-bytes-long",
        &user,
        &device,
    )
    .await
    .unwrap();
    let http = reqwest::Client::new();
    let cookie = format!("refresh_token={}", session.refresh_token);
    let (first, second) = tokio::join!(
        http.get(format!("{base}/test-probe"))
            .header(header::COOKIE, &cookie)
            .send(),
        http.get(format!("{base}/test-probe"))
            .header(header::COOKIE, &cookie)
            .send()
    );
    assert_eq!(first.unwrap().status(), StatusCode::UNAUTHORIZED);
    assert_eq!(second.unwrap().status(), StatusCode::UNAUTHORIZED);
    let mut child = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/browser_refresh.py"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "{}",
        serde_json::json!({"base":base, "refresh":session.refresh_token})
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "browser regression failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let active: i64 = match &db {
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM auth_credentials WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(&user.user_id)
        .fetch_one(p)
        .await
        .unwrap(),
        _ => unreachable!(),
    };
    assert_eq!(active, 1);
    server.abort();
}
