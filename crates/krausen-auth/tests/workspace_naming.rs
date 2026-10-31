// SPDX-License-Identifier: AGPL-3.0-or-later

//! Every workspace has a name.
//!
//! The product addresses a workspace by name in several places — the switcher
//! lists it, settings shows it, and transferring ownership asks the owner to
//! type it back to confirm. An empty name is not a cosmetic problem there: the
//! confirmation cannot be satisfied. Workspaces used to be created with
//! `name = NULL`, so every account made by signup had one.

use krausen_auth::user_service::{create_user, create_workspace_for_user};
use krausen_core::DbPool;

async fn test_db() -> DbPool {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    sqlx::migrate!("../../apps/server/migrations-sqlite")
        .run(&pool)
        .await
        .expect("migrations apply");
    DbPool::Sqlite(pool)
}

async fn name_of(db: &DbPool, workspace_id: &str) -> Option<String> {
    let DbPool::Sqlite(pool) = db else { unreachable!() };
    sqlx::query_scalar::<_, Option<String>>("SELECT name FROM workspaces WHERE workspace_id = ?")
        .bind(workspace_id)
        .fetch_one(pool)
        .await
        .expect("workspace exists")
}

#[tokio::test]
async fn a_workspace_is_named_after_its_owner() {
    let db = test_db().await;
    let user = create_user(&db, "brewer@example.test", Some("Jason"), true)
        .await
        .expect("user created");

    let ws = create_workspace_for_user(&db, &user.user_id, Some("Jason"), "brewer@example.test", None)
        .await
        .expect("workspace created");

    assert_eq!(name_of(&db, &ws).await.as_deref(), Some("Jason's Workspace"));
}

#[tokio::test]
async fn a_workspace_is_named_even_without_a_display_name() {
    let db = test_db().await;
    let user = create_user(&db, "brewer@example.test", None, true)
        .await
        .expect("user created");

    for name in [None, Some(""), Some("   ")] {
        let ws = create_workspace_for_user(&db, &user.user_id, name, "brewer@example.test", None)
            .await
            .expect("workspace created");
        assert_eq!(
            name_of(&db, &ws).await.as_deref(),
            Some("My Workspace"),
            "a blank display name ({name:?}) must still produce a usable workspace name"
        );
    }
}

/// The migration that reconciles workspaces made before the fix.
#[tokio::test]
async fn workspaces_created_nameless_are_named_by_the_migration() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");

    // The migration runs against an empty table here, so it does nothing. Write
    // the rows the old code would have left behind, then run its SQL over them
    // directly — that is the case it exists for.
    sqlx::migrate!("../../apps/server/migrations-sqlite")
        .run(&pool)
        .await
        .expect("migrations apply");

    sqlx::query("INSERT INTO users (user_id, email, name) VALUES ('u-named', 'a@example.test', 'Jason')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (user_id, email) VALUES ('u-anon', 'b@example.test')")
        .execute(&pool)
        .await
        .unwrap();

    for (id, owner) in [("ws-null", "u-named"), ("ws-blank", "u-anon")] {
        sqlx::query("INSERT INTO workspaces (workspace_id, name, owner_user_id) VALUES (?, NULL, ?)")
            .bind(id)
            .bind(owner)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("UPDATE workspaces SET name = '' WHERE workspace_id = 'ws-blank'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces (workspace_id, name, owner_user_id) VALUES ('ws-kept', 'Brewhouse', 'u-named')")
        .execute(&pool)
        .await
        .unwrap();

    let migration = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/server/migrations-sqlite/20260912200000_name_existing_workspaces.sql"
    ))
    .expect("migration readable");
    sqlx::raw_sql(&migration)
        .execute(&pool)
        .await
        .expect("migration runs");

    let db = DbPool::Sqlite(pool);
    assert_eq!(
        name_of(&db, "ws-null").await.as_deref(),
        Some("Jason's Workspace"),
        "a NULL name takes the owner's"
    );
    assert_eq!(
        name_of(&db, "ws-blank").await.as_deref(),
        Some("My Workspace"),
        "an owner with no display name gets the default"
    );
    assert_eq!(
        name_of(&db, "ws-kept").await.as_deref(),
        Some("Brewhouse"),
        "a workspace that already had a name keeps it"
    );
}
