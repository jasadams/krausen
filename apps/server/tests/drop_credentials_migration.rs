// SPDX-License-Identifier: AGPL-3.0-or-later
//! The forward migration that removes browser credentials and the opaque MCP
//! OAuth tables works on an installation that already holds rows in every one of
//! them (foreign keys included), keeps the accounts, and leaves the tables the
//! signed-token MCP model uses.
use krausen_core::DbPool;
use sqlx::migrate::Migrator;
use std::borrow::Cow;

const DROP_CREDENTIALS: i64 = 20261102020000;

const SEED: &[&str] = &[
    "INSERT INTO users (user_id,email) VALUES ('u','drop@example.test')",
    "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ('c-browser','u','browser','krausen-browser','krausen-ui',4102444800)",
    "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ('c-mcp','u','mcp','issuer','audience',4102444800)",
    "INSERT INTO mcp_oauth_clients (client_id,client_name,redirect_uris,auth_method,created_at) VALUES ('client','Client','[]','none',1)",
    "INSERT INTO mcp_oauth_authorization_codes (code_hash,client_id,user_id,initiating_credential_id,redirect_uri,resource,code_challenge,expires_at) VALUES ('code','client','u','c-browser','https://example.test/cb','https://example.test/mcp','challenge',4102444800)",
    "INSERT INTO mcp_oauth_refresh_tokens (token_hash,credential_id,client_id,expires_at) VALUES ('refresh','c-mcp','client',4102444800)",
    "INSERT INTO mcp_oauth_access_tokens (token_hash,credential_id,resource,expires_at) VALUES ('access','c-mcp','https://example.test/mcp',4102444800)",
    "INSERT INTO mcp_oauth_pending (pending_hash,client_id,redirect_uri,resource,state,code_challenge,user_id,credential_id,expires_at) VALUES ('pending','client','https://example.test/cb','https://example.test/mcp','state','challenge','u','c-browser',4102444800)",
];

const DROPPED: &[&str] = &[
    "auth_credentials",
    "mcp_oauth_clients",
    "mcp_oauth_authorization_codes",
    "mcp_oauth_refresh_tokens",
    "mcp_oauth_access_tokens",
    "mcp_oauth_pending",
];

fn before_drop(all: &Migrator) -> Migrator {
    Migrator {
        migrations: Cow::Owned(
            all.iter()
                .filter(|m| m.version != DROP_CREDENTIALS)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    }
}

async fn count(db: &DbPool, sql: &str, table: &str) -> i64 {
    match db {
        DbPool::Postgres(p) => sqlx::query_scalar(sql)
            .bind(table)
            .fetch_one(p)
            .await
            .unwrap(),
        DbPool::Sqlite(p) => sqlx::query_scalar(sql)
            .bind(table)
            .fetch_one(p)
            .await
            .unwrap(),
    }
}

async fn table_exists(db: &DbPool, table: &str) -> bool {
    let sql = if db.is_postgres() {
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = current_schema() AND table_name = $1"
    } else {
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = $1"
    };
    count(db, sql, table).await == 1
}

async fn exercise(db: DbPool, all: &Migrator) {
    let old = before_drop(all);
    match &db {
        DbPool::Postgres(p) => old.run(p).await.unwrap(),
        DbPool::Sqlite(p) => old.run(p).await.unwrap(),
    }
    for sql in SEED {
        match &db {
            DbPool::Postgres(p) => sqlx::query(sql).execute(p).await.unwrap().rows_affected(),
            DbPool::Sqlite(p) => sqlx::query(sql).execute(p).await.unwrap().rows_affected(),
        };
    }
    for table in DROPPED {
        assert!(table_exists(&db, table).await, "{table} exists before");
    }

    match &db {
        DbPool::Postgres(p) => all.run(p).await.unwrap(),
        DbPool::Sqlite(p) => all.run(p).await.unwrap(),
    }

    for table in DROPPED {
        assert!(!table_exists(&db, table).await, "{table} is dropped");
    }
    for table in ["users", "oauth_clients", "refresh_tokens"] {
        assert!(table_exists(&db, table).await, "{table} is kept");
    }
    let users: i64 = match &db {
        DbPool::Postgres(p) => sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE user_id = 'u'")
            .fetch_one(p)
            .await
            .unwrap(),
        DbPool::Sqlite(p) => sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE user_id = 'u'")
            .fetch_one(p)
            .await
            .unwrap(),
    };
    assert_eq!(users, 1, "accounts are retained");
}

#[tokio::test]
async fn sqlite_drops_populated_credential_tables() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    exercise(DbPool::Sqlite(pool), &sqlx::migrate!("./migrations-sqlite")).await;
}

#[tokio::test]
async fn postgres_drops_populated_credential_tables() {
    let base =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let admin = sqlx::PgPool::connect(&base).await.unwrap();
    let name = format!("krausen_drop_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(&admin)
        .await
        .unwrap();
    let mut url = url::Url::parse(&base).unwrap();
    url.set_path(&name);
    let pool = sqlx::PgPool::connect(url.as_str()).await.unwrap();
    exercise(
        DbPool::Postgres(pool.clone()),
        &sqlx::migrate!("./migrations"),
    )
    .await;
    pool.close().await;
    sqlx::query(&format!("DROP DATABASE {name} WITH (FORCE)"))
        .execute(&admin)
        .await
        .unwrap();
}
