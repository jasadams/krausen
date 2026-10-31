// SPDX-License-Identifier: AGPL-3.0-or-later
//! The combined binary must upgrade either independently shipped schema.
use krausen_core::DbPool;
use sqlx::migrate::Migrator;
use std::borrow::Cow;
const NATIVE: i64 = 20261031010000;
const CREDENTIALS: i64 = 20261031020000;
fn without(all: &Migrator, skip: Option<i64>) -> Migrator {
    Migrator {
        migrations: Cow::Owned(
            all.iter()
                .filter(|m| Some(m.version) != skip)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    }
}
async fn exercise(db: DbPool, all: &Migrator, skip: Option<i64>) {
    let first = without(all, skip);
    match &db {
        DbPool::Postgres(p) => first.run(p).await.unwrap(),
        DbPool::Sqlite(p) => first.run(p).await.unwrap(),
    }
    macro_rules! run {
        ($sql:expr) => {
            match &db {
                DbPool::Postgres(p) => sqlx::query($sql).execute(p).await.unwrap().rows_affected(),
                DbPool::Sqlite(p) => sqlx::query($sql).execute(p).await.unwrap().rows_affected(),
            }
        };
    }
    run!("INSERT INTO users (user_id,email) VALUES ('u','migration@example.test')");
    run!("INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ('w','u')");
    run!(
        "INSERT INTO recipes (workspace_id,recipe_id,name,document) VALUES ('w','legacy','Legacy','not-json')"
    );
    match &db {
        DbPool::Postgres(p) => all.run(p).await.unwrap(),
        DbPool::Sqlite(p) => all.run(p).await.unwrap(),
    }
    let report = krausen_app::recipe_migration::backfill_legacy_recipes(&db)
        .await
        .unwrap();
    assert_eq!(report.failed, vec![("w".into(), "legacy".into())]);
    let count: i64 = match &db {
        DbPool::Postgres(p) => sqlx::query_scalar("SELECT COUNT(*) FROM auth_credentials")
            .fetch_one(p)
            .await
            .unwrap(),
        DbPool::Sqlite(p) => sqlx::query_scalar("SELECT COUNT(*) FROM auth_credentials")
            .fetch_one(p)
            .await
            .unwrap(),
    };
    assert_eq!(count, 0);
}
#[tokio::test]
async fn sqlite_empty_and_both_upgrade_orders() {
    let all = sqlx::migrate!("./migrations-sqlite");
    for skip in [None, Some(NATIVE), Some(CREDENTIALS)] {
        let p = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        exercise(DbPool::Sqlite(p), &all, skip).await;
    }
}
#[tokio::test]
async fn postgres_empty_and_both_upgrade_orders() {
    let base =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let admin = sqlx::PgPool::connect(&base).await.unwrap();
    let all = sqlx::migrate!("./migrations");
    for skip in [None, Some(NATIVE), Some(CREDENTIALS)] {
        let name = format!("krausen_migration_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&admin)
            .await
            .unwrap();
        let mut url = url::Url::parse(&base).unwrap();
        url.set_path(&name);
        let pool = sqlx::PgPool::connect(url.as_str()).await.unwrap();
        exercise(DbPool::Postgres(pool.clone()), &all, skip).await;
        pool.close().await;
        sqlx::query(&format!("DROP DATABASE {name} WITH (FORCE)"))
            .execute(&admin)
            .await
            .unwrap();
    }
}
