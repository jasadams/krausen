// SPDX-License-Identifier: AGPL-3.0-or-later

//! Shared both-backends harness for the HTTP-level OAuth/MCP tests: a
//! per-test SQLite or PostgreSQL database, the real router on an ephemeral
//! port, and signed-in accounts. Nothing here is mocked.

#![allow(dead_code)]

use std::ops::Deref;
use std::sync::Arc;

use krausen_auth::{session, token_service::DeviceInfo, user_service};
use krausen_core::{DbPool, KVPool};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

pub const SECRET: &str = "test-secret-at-least-32-bytes-long";

/// A per-test database; a PostgreSQL database created for the test is dropped
/// with it (also when the test panics).
pub struct TestDb {
    pool: DbPool,
    postgres_cleanup: Option<(PgConnectOptions, String)>,
}

impl Deref for TestDb {
    type Target = DbPool;
    fn deref(&self) -> &DbPool {
        &self.pool
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let Some((admin, name)) = self.postgres_cleanup.take() else {
            return;
        };
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("cleanup runtime");
            rt.block_on(async {
                let pool = PgPoolOptions::new()
                    .max_connections(1)
                    .connect_with(admin)
                    .await
                    .expect("connect to drop test database");
                sqlx::query(&format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"))
                    .execute(&pool)
                    .await
                    .expect("drop test database");
            });
        })
        .join()
        .expect("cleanup thread");
    }
}

pub async fn sqlite_db() -> TestDb {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("open in-memory sqlite");
    sqlx::migrate!("./migrations-sqlite")
        .run(&pool)
        .await
        .expect("sqlite migrations");
    TestDb {
        pool: DbPool::Sqlite(pool),
        postgres_cleanup: None,
    }
}

pub async fn postgres_db() -> TestDb {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let admin: PgConnectOptions = url.parse().expect("parse KRAUSEN_TEST_POSTGRES_URL");
    let name = format!("krausen_httptest_{}", uuid::Uuid::new_v4().simple());
    let admin_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(admin.clone())
        .await
        .expect("connect to PostgreSQL");
    sqlx::query(&format!("CREATE DATABASE \"{name}\""))
        .execute(&admin_pool)
        .await
        .expect("create per-test database");
    admin_pool.close().await;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect_with(admin.clone().database(&name))
        .await
        .expect("connect to per-test database");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("postgres migrations");
    TestDb {
        pool: DbPool::Postgres(pool),
        postgres_cleanup: Some((admin, name)),
    }
}

/// The real router served on an ephemeral localhost port.
pub struct TestServer {
    pub base: String,
    pub db: DbPool,
    pub kv: KVPool,
    pub secret: String,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub async fn start(db: &DbPool) -> TestServer {
    let _ = krausen_core::constants::load_with_fallback();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://localhost:{}", listener.local_addr().unwrap().port());
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
    let handle = tokio::spawn(async move {
        axum::serve(listener, krausen_server::build_router(state))
            .await
            .unwrap()
    });
    TestServer {
        base,
        db: db.clone(),
        kv,
        secret: SECRET.into(),
        handle,
    }
}

pub fn device() -> DeviceInfo {
    DeviceInfo {
        user_agent: None,
        ip_address: None,
        country_code: None,
        oauth_client_id: None,
    }
}

/// A signed-in account with a workspace.
pub struct Account {
    pub user: krausen_core::models::User,
    pub user_id: String,
    pub email: String,
    pub workspace_id: String,
    pub access_token: String,
    pub refresh_token: String,
}

pub async fn account(srv: &TestServer) -> Account {
    let email = format!("{}@example.test", uuid::Uuid::new_v4());
    let user = user_service::create_user(&srv.db, &email, Some("Test"), true)
        .await
        .unwrap();
    let workspace_id =
        user_service::create_workspace_for_user(&srv.db, &user.user_id, Some("Test"), &email, None)
            .await
            .unwrap();
    krausen_core::db_execute!(
        &srv.db,
        "UPDATE workspaces SET status = 'active' WHERE workspace_id = $1",
        &workspace_id
    )
    .unwrap();
    let browser = session::create_authenticated_session(&srv.db, &srv.kv, &srv.secret, &user, &device())
        .await
        .unwrap();
    Account {
        user_id: user.user_id.clone(),
        email,
        workspace_id,
        access_token: browser.access_token,
        refresh_token: browser.refresh_token,
        user,
    }
}

/// Register each `async fn(&TestServer)` scenario as a SQLite and a PostgreSQL test.
macro_rules! both_backends {
    ($($scenario:ident),+ $(,)?) => {
        $(
            mod $scenario {
                #[tokio::test]
                async fn sqlite() {
                    let db = $crate::common::sqlite_db().await;
                    let srv = $crate::common::start(&db).await;
                    super::$scenario(&srv).await;
                }

                #[tokio::test]
                async fn postgres() {
                    let db = $crate::common::postgres_db().await;
                    let srv = $crate::common::start(&db).await;
                    super::$scenario(&srv).await;
                }
            }
        )+
    };
}
pub(crate) use both_backends;
