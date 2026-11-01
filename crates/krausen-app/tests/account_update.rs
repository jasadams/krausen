// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{Error, account_read, account_update, invitation::AccountActor};
use krausen_core::DbPool;

async fn exercise(db: DbPool) {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user = format!("u-{suffix}");
    let credential = format!("c-{suffix}");
    macro_rules! run { ($sql:expr $(, $arg:expr)*) => { match &db {
        DbPool::Postgres(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
        DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
    } }; }
    run!(
        "INSERT INTO users (user_id,email,extra_metadata) VALUES ($1,$2,$3)",
        &user,
        format!("{suffix}@example.test"),
        sqlx::types::Json(serde_json::json!({"other":"preserved"}))
    );
    run!(
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',4102444800)",
        &credential,
        &user
    );
    let actor = AccountActor {
        actor_id: user.clone(),
        credential_id: credential.clone(),
        access_expires_at: 4102444800,
    };
    account_update::name(&db, &actor, 0, "  Brewer  ")
        .await
        .unwrap();
    assert!(matches!(
        account_update::theme(&db, &actor, 0, "dark").await,
        Err(Error::Conflict { .. })
    ));
    account_update::theme(&db, &actor, 1, "dark").await.unwrap();
    let profile = account_read::profile(&db, &actor).await.unwrap();
    assert_eq!(profile.name.as_deref(), Some("Brewer"));
    assert_eq!(profile.theme, "dark");
    assert_eq!(profile.settings_revision, 2);
    assert!(matches!(
        account_update::name(&db, &actor, 2, "  ").await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        account_update::theme(&db, &actor, 2, "other").await,
        Err(Error::Invalid(_))
    ));
    let value: sqlx::types::Json<serde_json::Value> = match &db {
        DbPool::Postgres(p) => {
            sqlx::query_scalar("SELECT extra_metadata FROM users WHERE user_id=$1")
                .bind(&user)
                .fetch_one(p)
                .await
                .unwrap()
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar("SELECT extra_metadata FROM users WHERE user_id=$1")
                .bind(&user)
                .fetch_one(p)
                .await
                .unwrap()
        }
    };
    assert_eq!(value.0["other"], "preserved");
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
    assert_eq!(
        count, 2,
        "stale and invalid writes must not create audit entries"
    );
    run!(
        "INSERT INTO account_settings_changes (change_id,account_id,actor_id,operation,revision,recorded_at) VALUES ($1,$2,$2,'test',3,0)",
        format!("block-{suffix}"),
        &user
    );
    assert!(
        account_update::name(&db, &actor, 2, "Must roll back")
            .await
            .is_err()
    );
    let after = account_read::profile(&db, &actor).await.unwrap();
    assert_eq!(after.name.as_deref(), Some("Brewer"));
    assert_eq!(after.settings_revision, 2);
    run!(
        "UPDATE auth_credentials SET revoked_at=$1 WHERE credential_id=$2",
        chrono::Utc::now().timestamp(),
        &credential
    );
    assert!(matches!(
        account_update::name(&db, &actor, 2, "Denied").await,
        Err(Error::Forbidden)
    ));
}

#[tokio::test]
async fn sqlite_account_update() {
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
async fn postgres_account_update() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
