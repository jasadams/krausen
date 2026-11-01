// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{Error, account_update, invitation::AccountActor};
use krausen_auth::user_service;
use krausen_core::DbPool;

async fn exercise(db: DbPool) {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user = format!("entry-{suffix}");
    let credential = format!("credential-{suffix}");
    macro_rules! run { ($sql:expr $(, $arg:expr)*) => { match &db {
        DbPool::Postgres(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
        DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
    } }; }
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2)",
        &user,
        format!("{suffix}@example.test")
    );
    run!(
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',4102444800)",
        &credential,
        &user
    );
    let actor = AccountActor {
        actor_id: user.clone(),
        credential_id: credential,
        access_expires_at: 4102444800,
    };
    assert!(
        user_service::update_user_name(&db, &user, "Entry name")
            .await
            .unwrap()
    );
    assert!(matches!(
        account_update::theme(&db, &actor, 0, "dark").await,
        Err(Error::Conflict { actual: 1, .. })
    ));
    let rows: Vec<(String, i64)> = match &db {
        DbPool::Postgres(p) => sqlx::query_as(
            "SELECT actor_id,revision FROM account_settings_changes WHERE account_id=$1",
        )
        .bind(&user)
        .fetch_all(p)
        .await
        .unwrap(),
        DbPool::Sqlite(p) => sqlx::query_as(
            "SELECT actor_id,revision FROM account_settings_changes WHERE account_id=$1",
        )
        .bind(&user)
        .fetch_all(p)
        .await
        .unwrap(),
    };
    assert_eq!(rows, vec![(user, 1)]);
}

#[tokio::test]
async fn sqlite_entry_invalidates_settings_revision() {
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
async fn postgres_entry_invalidates_settings_revision() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
