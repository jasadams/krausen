// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{Error, account_update, invitation::AccountActor};
use krausen_core::DbPool;

async fn exercise(db: DbPool) {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let user = format!("race-{id}");
    let credential = format!("credential-{id}");
    macro_rules! run { ($sql:expr $(, $arg:expr)*) => { match &db {
        DbPool::Postgres(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
        DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
    } }; }
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2)",
        &user,
        format!("{id}@example.test")
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
    let (name, theme) = tokio::join!(
        account_update::name(&db, &actor, 0, "New name"),
        account_update::theme(&db, &actor, 0, "dark")
    );
    assert_eq!(usize::from(name.is_ok()) + usize::from(theme.is_ok()), 1);
    assert!(
        matches!(name, Err(Error::Conflict { .. })) || matches!(theme, Err(Error::Conflict { .. }))
    );
    let count: i64 = match &db {
        DbPool::Postgres(p) => {
            sqlx::query_scalar("SELECT count(*) FROM account_settings_changes WHERE account_id=$1")
                .bind(&user)
                .fetch_one(p)
                .await
                .unwrap()
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar("SELECT count(*) FROM account_settings_changes WHERE account_id=$1")
                .bind(&user)
                .fetch_one(p)
                .await
                .unwrap()
        }
    };
    assert_eq!(count, 1);
}

#[tokio::test]
async fn sqlite_account_settings_race() {
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
async fn postgres_account_settings_race() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
