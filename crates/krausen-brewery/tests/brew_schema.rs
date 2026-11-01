// SPDX-License-Identifier: AGPL-3.0-or-later
//! Brew plan/source identity is immutable and retained by database constraints.
use std::str::FromStr;

async fn exercise_sqlite() {
    let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::migrate!("../../apps/server/migrations-sqlite")
        .run(&pool)
        .await
        .unwrap();
    seed_sqlite(&pool).await;
    exercise_sqlite_constraints(&pool).await;
}

async fn seed_sqlite(pool: &sqlx::SqlitePool) {
    sqlx::query("INSERT INTO users (user_id,email) VALUES ('u','brew@example.test')")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces (workspace_id,name,owner_user_id) VALUES ('w','W','u')")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO recipes (workspace_id,recipe_id,name,document,native_head_revision) VALUES ('w','r','Recipe','{}',1)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO recipe_revisions (workspace_id,recipe_id,revision,schema_version,document,content_digest) VALUES ('w','r',1,1,'{}','recipe-digest')")
        .execute(pool)
        .await
        .unwrap();
    let head: i64 = sqlx::query_scalar(
        "SELECT native_head_revision FROM recipes WHERE workspace_id='w' AND recipe_id='r'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        head, 1,
        "fixture's native head must match its saved revision"
    );
    sqlx::query(
        "INSERT INTO brews (workspace_id,brew_id,recipe_id,selected_recipe_revision,status,revision,started_at,started_by,plan_schema_version,plan_json,plan_content_digest) VALUES ('w','b','r',1,'started',1,1,'u',1,'{\"plan\":true}','plan-digest')",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn exercise_sqlite_constraints(pool: &sqlx::SqlitePool) {
    assert!(
        sqlx::query("UPDATE brews SET status='completed', revision=2 WHERE brew_id='b'")
            .execute(pool)
            .await
            .is_ok()
    );
    assert!(
        sqlx::query("UPDATE brews SET plan_json='{\"changed\":true}' WHERE brew_id='b'")
            .execute(pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE brews SET recipe_id='other' WHERE brew_id='b'")
            .execute(pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE brews SET started_by='other' WHERE brew_id='b'")
            .execute(pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE brews SET plan_content_digest='changed' WHERE brew_id='b'")
            .execute(pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM brews WHERE brew_id='b'")
            .execute(pool)
            .await
            .is_err()
    );
    assert!(sqlx::query(
        "INSERT INTO brews (workspace_id,brew_id,recipe_id,selected_recipe_revision,status,revision,started_at,started_by,plan_schema_version,plan_json,plan_content_digest) VALUES ('w','b2','r',2,'started',1,1,'u',1,'{}','digest')"
    )
    .execute(pool)
    .await
    .is_err());
}

#[tokio::test]
async fn sqlite_brew_plan_is_immutable() {
    exercise_sqlite().await;
}

#[tokio::test]
#[ignore = "requires an isolated KRAUSEN_TEST_POSTGRES_URL database"]
async fn postgres_brew_plan_is_immutable() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("isolated PostgreSQL URL");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (user_id,email) VALUES ('brew-pg-u','brew-pg@example.test')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces (workspace_id,name,owner_user_id) VALUES ('brew-pg-w','W','brew-pg-u')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO recipes (workspace_id,recipe_id,name,document,native_head_revision) VALUES ('brew-pg-w','r','Recipe','{}',1)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO recipe_revisions (workspace_id,recipe_id,revision,schema_version,document,content_digest) VALUES ('brew-pg-w','r',1,1,'{}','recipe-digest')")
        .execute(&pool)
        .await
        .unwrap();
    let head: i64 = sqlx::query_scalar(
        "SELECT native_head_revision FROM recipes WHERE workspace_id='brew-pg-w' AND recipe_id='r'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        head, 1,
        "fixture's native head must match its saved revision"
    );
    sqlx::query(
        "INSERT INTO brews (workspace_id,brew_id,recipe_id,selected_recipe_revision,status,revision,started_at,started_by,plan_schema_version,plan_json,plan_content_digest) VALUES ('brew-pg-w','b','r',1,'started',1,1,'brew-pg-u',1,'{\"plan\":true}','plan-digest')",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        sqlx::query("UPDATE brews SET status='completed', revision=2 WHERE brew_id='b'")
            .execute(&pool)
            .await
            .is_ok()
    );
    for sql in [
        "UPDATE brews SET plan_json='{\"changed\":true}' WHERE brew_id='b'",
        "UPDATE brews SET recipe_id='other' WHERE brew_id='b'",
        "UPDATE brews SET started_by='other' WHERE brew_id='b'",
        "UPDATE brews SET plan_content_digest='changed' WHERE brew_id='b'",
        "DELETE FROM brews WHERE brew_id='b'",
    ] {
        assert!(sqlx::query(sql).execute(&pool).await.is_err(), "{sql}");
    }
    assert!(sqlx::query(
        "INSERT INTO brews (workspace_id,brew_id,recipe_id,selected_recipe_revision,status,revision,started_at,started_by,plan_schema_version,plan_json,plan_content_digest) VALUES ('brew-pg-w','b2','r',2,'started',1,1,'brew-pg-u',1,'{}','digest')"
    )
    .execute(&pool)
    .await
    .is_err());
}
