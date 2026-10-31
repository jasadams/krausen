// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{
    ActorContext, Error,
    recipe_migration::{backfill_legacy_recipes, recover_legacy_recipe, reject_legacy_authoring},
};
use krausen_core::DbPool;

async fn exercise(db: DbPool) {
    macro_rules! run { ($sql:expr $(, $a:expr)*) => { match &db {
        DbPool::Postgres(p) => sqlx::query($sql)$(.bind($a))*.execute(p).await.unwrap().rows_affected(),
        DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($a))*.execute(p).await.unwrap().rows_affected(),
    } }; }
    run!("INSERT INTO users (user_id,email) VALUES ('migration-u','migration@example.test')");
    run!(
        "INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ('migration-w','migration-u')"
    );
    run!(
        "INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ('migration-w','migration-u','member')"
    );
    run!(
        "INSERT INTO recipes (workspace_id,recipe_id,name,document) VALUES ('migration-w','migration-good','Good','{}'), ('migration-w','migration-bad','Broken','not-json')"
    );
    let report = backfill_legacy_recipes(&db).await.unwrap();
    assert_eq!(report.migrated, 1);
    assert!(
        report
            .failed
            .contains(&("migration-w".to_owned(), "migration-bad".to_owned()))
    );
    run!(
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ('migration-c','migration-u','browser','test','test',4102444800)"
    );
    let actor = ActorContext {
        actor_id: "migration-u".into(),
        workspace_id: "migration-w".into(),
        credential_id: "migration-c".into(),
        access_expires_at: 4102444800,
    };
    assert!(matches!(
        reject_legacy_authoring(&db, &actor).await,
        Err(Error::LegacyAuthoringUnavailable)
    ));
    let count: i64 = match &db {
        DbPool::Postgres(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM recipes WHERE workspace_id = 'migration-w'")
                .fetch_one(p)
                .await
                .unwrap()
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM recipes WHERE workspace_id = 'migration-w'")
                .fetch_one(p)
                .await
                .unwrap()
        }
    };
    assert_eq!(count, 2);
    let recovered = recover_legacy_recipe(&db, &actor, "migration-bad")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.0, "not-json");
    assert!(recovered.1.contains("recovery"));
    assert!(
        recover_legacy_recipe(&db, &actor, "migration-good")
            .await
            .unwrap()
            .is_none()
    );
    let wrong = ActorContext {
        actor_id: "migration-u".into(),
        workspace_id: "wrong-workspace".into(),
        credential_id: "migration-c".into(),
        access_expires_at: 4102444800,
    };
    assert!(matches!(
        reject_legacy_authoring(&db, &wrong).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        recover_legacy_recipe(&db, &wrong, "migration-bad").await,
        Err(Error::Forbidden)
    ));
    let report = backfill_legacy_recipes(&db).await.unwrap();
    assert_eq!(report.migrated, 0);
    assert!(
        report
            .failed
            .contains(&("migration-w".to_owned(), "migration-bad".to_owned()))
    );
    run!(
        "UPDATE workspace_users SET active = FALSE WHERE workspace_id = 'migration-w' AND user_id = 'migration-u'"
    );
    assert!(matches!(
        recover_legacy_recipe(&db, &actor, "migration-bad").await,
        Err(Error::Forbidden)
    ));
}

#[tokio::test]
async fn sqlite_migration_recovery() {
    let p = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../apps/server/migrations-sqlite")
        .run(&p)
        .await
        .unwrap();
    exercise(DbPool::Sqlite(p)).await;
}

#[tokio::test]
async fn sqlite_upgrade_backfills_preexisting_rows() {
    use std::borrow::Cow;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let all = sqlx::migrate!("../../apps/server/migrations-sqlite");
    let prior = sqlx::migrate::Migrator {
        migrations: Cow::Owned(
            all.iter()
                .filter(|m| m.version < 20261031010000)
                .cloned()
                .collect(),
        ),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    };
    prior.run(&pool).await.unwrap();
    sqlx::query("INSERT INTO users (user_id,email) VALUES ('upgrade-u','upgrade@example.test')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ('upgrade-w','upgrade-u')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO recipes (workspace_id,recipe_id,name,document) VALUES ('upgrade-w','upgrade-r','Before upgrade','{}')").execute(&pool).await.unwrap();
    all.run(&pool).await.unwrap();
    let report = backfill_legacy_recipes(&DbPool::Sqlite(pool.clone()))
        .await
        .unwrap();
    assert_eq!(report.migrated, 1);
    assert!(report.failed.is_empty());
    let row: (i64, Option<String>, Option<String>) = sqlx::query_as("SELECT revision, saved_by, source_record FROM recipe_revisions JOIN recipe_import_provenance USING (workspace_id, recipe_id, revision) WHERE recipe_id = 'upgrade-r'").fetch_one(&pool).await.unwrap();
    assert_eq!(row, (1, None, Some("{}".into())));
}

#[tokio::test]
#[ignore = "requires an isolated KRAUSEN_TEST_POSTGRES_URL database"]
async fn postgres_migration_recovery() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("isolated PostgreSQL URL");
    let p = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&p)
        .await
        .unwrap();
    exercise(DbPool::Postgres(p)).await;
}

#[tokio::test]
#[ignore = "requires an isolated KRAUSEN_TEST_POSTGRES_URL database with CREATEDB"]
async fn postgres_upgrade_backfills_preexisting_rows() {
    use std::{borrow::Cow, str::FromStr};
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("isolated PostgreSQL URL");
    let options = sqlx::postgres::PgConnectOptions::from_str(&url).unwrap();
    let admin = sqlx::PgPool::connect_with(options.clone()).await.unwrap();
    let name = format!("kra6_upgrade_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(&admin)
        .await
        .unwrap();
    let pool = sqlx::PgPool::connect_with(options.database(&name))
        .await
        .unwrap();
    let all = sqlx::migrate!("../../apps/server/migrations");
    let prior = sqlx::migrate::Migrator {
        migrations: Cow::Owned(
            all.iter()
                .filter(|m| m.version < 20261031010000)
                .cloned()
                .collect(),
        ),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    };
    prior.run(&pool).await.unwrap();
    sqlx::query("INSERT INTO users (user_id,email) VALUES ('upgrade-u','upgrade@example.test')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ('upgrade-w','upgrade-u')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO recipes (workspace_id,recipe_id,name,document) VALUES ('upgrade-w','upgrade-r','Before upgrade','{}')").execute(&pool).await.unwrap();
    all.run(&pool).await.unwrap();
    let report = backfill_legacy_recipes(&DbPool::Postgres(pool.clone()))
        .await
        .unwrap();
    assert_eq!(report.migrated, 1);
    assert!(report.failed.is_empty());
    let row: (i64, Option<String>, Option<String>) = sqlx::query_as("SELECT revision, saved_by, source_record FROM recipe_revisions JOIN recipe_import_provenance USING (workspace_id, recipe_id, revision) WHERE recipe_id = 'upgrade-r'").fetch_one(&pool).await.unwrap();
    assert_eq!(row, (1, None, Some("{}".into())));
    pool.close().await;
    sqlx::query(&format!("DROP DATABASE {name}"))
        .execute(&admin)
        .await
        .unwrap();
}
