// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{ActorContext, Error, account_update};
use krausen_core::DbPool;

async fn exercise(db: DbPool) {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user = format!("u-{suffix}");
    let owner = format!("o-{suffix}");
    let allowed = format!("a-{suffix}");
    let foreign = format!("f-{suffix}");
    let inactive = format!("i-{suffix}");
    let credential = format!("c-{suffix}");
    macro_rules! run { ($sql:expr $(, $arg:expr)*) => { match &db {
        DbPool::Postgres(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
        DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
    } }; }
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2),($3,$4)",
        &user,
        format!("u-{suffix}@example.test"),
        &owner,
        format!("o-{suffix}@example.test")
    );
    for workspace in [&allowed, &foreign, &inactive] {
        run!(
            "INSERT INTO workspaces (workspace_id,owner_user_id,status) VALUES ($1,$2,'active')",
            workspace,
            &owner
        );
    }
    run!(
        "INSERT INTO workspace_users (workspace_id,user_id,role,active) VALUES ($1,$2,'workspace_user',TRUE),($3,$2,'workspace_user',FALSE)",
        &allowed,
        &user,
        &inactive
    );
    run!(
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',4102444800)",
        &credential,
        &user
    );
    let actor = ActorContext {
        actor_id: user.clone(),
        credential_id: credential,
        workspace_id: allowed.clone(),
        access_expires_at: 4102444800,
    };
    account_update::workspace(&db, &actor, 0).await.unwrap();
    for denied in [&foreign, &inactive] {
        let attempt = ActorContext {
            workspace_id: denied.clone(),
            ..actor.clone()
        };
        assert!(matches!(
            account_update::workspace(&db, &attempt, 1).await,
            Err(Error::Forbidden)
        ));
    }
    let stored: Option<String> = match &db {
        DbPool::Postgres(p) => {
            sqlx::query_scalar("SELECT last_workspace_id FROM users WHERE user_id=$1")
                .bind(&user)
                .fetch_one(p)
                .await
                .unwrap()
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar("SELECT last_workspace_id FROM users WHERE user_id=$1")
                .bind(&user)
                .fetch_one(p)
                .await
                .unwrap()
        }
    };
    assert_eq!(stored.as_deref(), Some(allowed.as_str()));
}

#[tokio::test]
async fn sqlite_workspace_selection() {
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
async fn postgres_workspace_selection() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
