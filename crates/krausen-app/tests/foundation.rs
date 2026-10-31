// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{ActorContext, Error, rename_workspace};
use krausen_core::DbPool;
use uuid::Uuid;

async fn exercise(db: DbPool) {
    let suffix = Uuid::new_v4().to_string();
    let user = format!("u-{suffix}");
    let workspace = format!("w-{suffix}");
    let email = format!("{suffix}@example.test");
    macro_rules! run { ($sql:expr $(, $a:expr)*) => { match &db {
        DbPool::Postgres(p) => sqlx::query($sql)$(.bind($a))*.execute(p).await.unwrap().rows_affected(),
        DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($a))*.execute(p).await.unwrap().rows_affected(),
    } }; }
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2)",
        &user,
        &email
    );
    run!(
        "INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ($1,$2)",
        &workspace,
        &user
    );
    run!(
        "INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ($1,$2,'workspace_admin')",
        &workspace,
        &user
    );
    let credential = format!("c-{suffix}");
    run!(
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)",
        &credential,
        &user,
        chrono::Utc::now().timestamp() + 3600
    );
    let actor = ActorContext {
        actor_id: user.clone(),
        workspace_id: workspace.clone(),
        credential_id: credential,
        access_expires_at: chrono::Utc::now().timestamp() + 3600,
    };
    let expired_actor = ActorContext {
        access_expires_at: chrono::Utc::now().timestamp() - 1,
        ..actor.clone()
    };
    assert!(matches!(
        krausen_app::workspace_snapshot(&db, &expired_actor).await,
        Err(krausen_app::Error::Forbidden)
    ));
    let key = Uuid::new_v4();
    let first = rename_workspace(&db, &actor, "New name", 0, key)
        .await
        .unwrap();
    assert_eq!(first.revision, 1);
    let snapshot = krausen_app::workspace_snapshot(&db, &actor).await.unwrap();
    assert_eq!(
        (snapshot.name.as_deref(), snapshot.revision),
        (Some("New name"), 1)
    );
    assert_eq!(
        rename_workspace(&db, &actor, "New name", 0, key)
            .await
            .unwrap(),
        first
    );
    assert!(matches!(
        rename_workspace(&db, &actor, "Other", 0, key).await,
        Err(Error::RequestIdentityConflict)
    ));
    assert!(matches!(
        rename_workspace(&db, &actor, "Other", 0, Uuid::new_v4()).await,
        Err(Error::Conflict {
            expected: 0,
            actual: 1
        })
    ));
    run!(
        "UPDATE app_requests SET result_expires_at = 0 WHERE workspace_id = $1 AND request_id = $2",
        &workspace,
        key.to_string()
    );
    assert!(
        matches!(rename_workspace(&db, &actor, "New name", 0, key).await,
        Err(Error::ResultExpired { resource_id: Some(id) }) if id == workspace)
    );
    let second_key = Uuid::new_v4();
    let (one, two) = tokio::join!(
        rename_workspace(&db, &actor, "Concurrent", 1, second_key),
        rename_workspace(&db, &actor, "Concurrent", 1, second_key)
    );
    assert_eq!(one.unwrap(), two.unwrap());
    let snapshot = krausen_app::workspace_snapshot(&db, &actor).await.unwrap();
    assert_eq!(
        (snapshot.name.as_deref(), snapshot.revision),
        (Some("Concurrent"), 2)
    );
    // Fail the final change write after the entity, sync and request writes.
    // The whole command must roll back on each real backend.
    if db.is_postgres() {
        run!(
            "CREATE OR REPLACE FUNCTION kra7_reject_change() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected failure'; END $$"
        );
        run!(
            "CREATE TRIGGER kra7_reject_change BEFORE INSERT ON app_changes FOR EACH ROW EXECUTE FUNCTION kra7_reject_change()"
        );
    } else {
        run!(
            "CREATE TRIGGER kra7_reject_change BEFORE INSERT ON app_changes BEGIN SELECT RAISE(ABORT, 'injected failure'); END"
        );
    }
    let failed_key = Uuid::new_v4();
    assert!(
        rename_workspace(&db, &actor, "Failed", 2, failed_key)
            .await
            .is_err()
    );
    if db.is_postgres() {
        run!("DROP TRIGGER kra7_reject_change ON app_changes");
        run!("DROP FUNCTION kra7_reject_change()");
    } else {
        run!("DROP TRIGGER kra7_reject_change");
    }
    let snapshot = krausen_app::workspace_snapshot(&db, &actor).await.unwrap();
    assert_eq!(
        (snapshot.name.as_deref(), snapshot.revision),
        (Some("Concurrent"), 2)
    );
    let failed_count: i64 = match &db {
        DbPool::Postgres(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM app_requests WHERE request_id = $1")
                .bind(failed_key.to_string())
                .fetch_one(p)
                .await
                .unwrap()
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM app_requests WHERE request_id = $1")
                .bind(failed_key.to_string())
                .fetch_one(p)
                .await
                .unwrap()
        }
    };
    assert_eq!(failed_count, 0);
    let sync_count: i64 = match &db {
        DbPool::Postgres(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM sync_log WHERE workspace_id = $1")
                .bind(&workspace)
                .fetch_one(p)
                .await
                .unwrap()
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM sync_log WHERE workspace_id = $1")
                .bind(&workspace)
                .fetch_one(p)
                .await
                .unwrap()
        }
    };
    assert_eq!(sync_count, 2);
    let false_value = if db.is_postgres() { "FALSE" } else { "0" };
    // A revocation that holds the authority lock must order before a waiting command.
    let mut revoke = db.begin_write().await.unwrap();
    let revoke_sql = format!(
        "UPDATE workspace_users SET active = {false_value} WHERE workspace_id = $1 AND user_id = $2"
    );
    match &mut revoke {
        krausen_core::DbTx::Postgres(t) => {
            sqlx::query(&revoke_sql)
                .bind(&workspace)
                .bind(&user)
                .execute(&mut **t)
                .await
                .unwrap();
        }
        krausen_core::DbTx::Sqlite(t) => {
            sqlx::query(&revoke_sql)
                .bind(&workspace)
                .bind(&user)
                .execute(&mut **t)
                .await
                .unwrap();
        }
    }
    let waiting_db = db.clone();
    let waiting_actor = actor.clone();
    let mut waiting = tokio::spawn(async move {
        rename_workspace(&waiting_db, &waiting_actor, "New name", 0, key).await
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut waiting)
            .await
            .is_err()
    );
    revoke.commit().await.unwrap();
    assert!(matches!(waiting.await.unwrap(), Err(Error::Forbidden)));
    assert!(matches!(
        rename_workspace(&db, &actor, "New name", 0, key).await,
        Err(Error::Forbidden)
    ));
    let true_value = if db.is_postgres() { "TRUE" } else { "1" };
    run!(
        &format!(
            "UPDATE workspace_users SET active = {true_value} WHERE workspace_id = $1 AND user_id = $2"
        ),
        &workspace,
        &user
    );
    let mut revocation = db.begin_write().await.unwrap();
    match &mut revocation {
        krausen_core::DbTx::Postgres(t) => {
            sqlx::query("SELECT user_id FROM users WHERE user_id = $1 FOR SHARE")
                .bind(&user)
                .fetch_one(&mut **t)
                .await
                .unwrap();
            sqlx::query("UPDATE auth_credentials SET revoked_at = $1 WHERE credential_id = $2")
                .bind(chrono::Utc::now().timestamp())
                .bind(&actor.credential_id)
                .execute(&mut **t)
                .await
                .unwrap();
        }
        krausen_core::DbTx::Sqlite(t) => {
            sqlx::query("UPDATE auth_credentials SET revoked_at = $1 WHERE credential_id = $2")
                .bind(chrono::Utc::now().timestamp())
                .bind(&actor.credential_id)
                .execute(&mut **t)
                .await
                .unwrap();
        }
    }
    let waiting_db = db.clone();
    let waiting_actor = actor.clone();
    let mut waiting = tokio::spawn(async move {
        rename_workspace(&waiting_db, &waiting_actor, "Blocked", 2, Uuid::new_v4()).await
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut waiting)
            .await
            .is_err()
    );
    revocation.commit().await.unwrap();
    assert!(matches!(waiting.await.unwrap(), Err(Error::Forbidden)));
    run!(
        &format!("UPDATE users SET active = {false_value} WHERE user_id = $1"),
        &user
    );
    assert!(matches!(
        rename_workspace(&db, &actor, "New name", 0, key).await,
        Err(Error::Forbidden)
    ));
    let mut tx = db.begin_write().await.unwrap();
    let outbox_id = krausen_app::enqueue_outbox(
        &mut tx,
        &workspace,
        "test",
        &serde_json::json!({"value": 1}),
    )
    .await
    .unwrap();
    match &mut tx {
        krausen_core::DbTx::Postgres(t) => {
            sqlx::query("UPDATE workspaces SET name = 'rolled back' WHERE workspace_id = $1")
                .bind(&workspace)
                .execute(&mut **t)
                .await
                .unwrap();
        }
        krausen_core::DbTx::Sqlite(t) => {
            sqlx::query("UPDATE workspaces SET name = 'rolled back' WHERE workspace_id = $1")
                .bind(&workspace)
                .execute(&mut **t)
                .await
                .unwrap();
        }
    }
    tx.rollback().await.unwrap();
    let outbox_count: i64 = match &db {
        DbPool::Postgres(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM app_outbox WHERE outbox_id = $1")
                .bind(&outbox_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM app_outbox WHERE outbox_id = $1")
                .bind(&outbox_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
    };
    assert_eq!(outbox_count, 0);
    let name: String = match &db {
        DbPool::Postgres(p) => {
            sqlx::query_scalar("SELECT name FROM workspaces WHERE workspace_id = $1")
                .bind(&workspace)
                .fetch_one(p)
                .await
                .unwrap()
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar("SELECT name FROM workspaces WHERE workspace_id = $1")
                .bind(&workspace)
                .fetch_one(p)
                .await
                .unwrap()
        }
    };
    assert_eq!(name, "Concurrent");
}

#[tokio::test]
async fn sqlite_foundation() {
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
async fn postgres_foundation() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL")
        .expect("KRAUSEN_TEST_POSTGRES_URL is required for PostgreSQL tests");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}

#[test]
fn shared_projection_preserves_conflicts_and_redacts_internal_errors() {
    use krausen_types::workspace::WorkspaceRenameOutcome as Outcome;
    assert_eq!(
        krausen_app::project_rename_result(Err(Error::Conflict {
            expected: 1,
            actual: 2
        })),
        Outcome::Conflict {
            expected: 1,
            actual: 2
        }
    );
    assert_eq!(
        krausen_app::project_rename_result(Err(Error::Database(krausen_core::Error::Internal(
            "private detail".into()
        )))),
        Outcome::Unavailable
    );
}
