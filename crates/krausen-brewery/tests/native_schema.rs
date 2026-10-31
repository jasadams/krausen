// SPDX-License-Identifier: AGPL-3.0-or-later

//! The real SQLite migration enforces native revision lineage and immutability.

#[tokio::test]
async fn revisions_are_workspace_scoped_and_append_only() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("../../apps/server/migrations-sqlite")
        .run(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (user_id, email) VALUES ('u', 'u@example.test')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces (workspace_id, name, owner_user_id) VALUES ('a', 'A', 'u'), ('b', 'B', 'u')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO recipes (workspace_id, recipe_id, name, document) VALUES ('a', 'r', 'Recipe', '{}')").execute(&pool).await.unwrap();
    let insert = "INSERT INTO recipe_revisions (workspace_id, recipe_id, revision, schema_version, document, content_digest) VALUES (?, 'r', 1, 1, '{}', 'digest')";
    sqlx::query(insert).bind("a").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO recipe_import_provenance (workspace_id, recipe_id, revision, format, mapping_version, diagnostics, source_record) VALUES ('a', 'r', 1, 'BeerXML', 'v1', '[]', '{\"raw\":true}')").execute(&pool).await.unwrap();
    assert!(
        sqlx::query("UPDATE recipe_import_provenance SET diagnostics = 'changed'")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(sqlx::query(insert).bind("b").execute(&pool).await.is_err());
    assert!(
        sqlx::query("UPDATE recipe_revisions SET document = 'changed'")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM recipe_revisions")
            .execute(&pool)
            .await
            .is_err()
    );
    let value: String = sqlx::query_scalar("SELECT document FROM recipe_revisions WHERE workspace_id = 'a' AND recipe_id = 'r' AND revision = 1").fetch_one(&pool).await.unwrap();
    assert_eq!(value, "{}");
}

/// Run against an isolated disposable database with the matching URL supplied.
#[tokio::test]
#[ignore = "requires an isolated KRAUSEN_TEST_POSTGRES_URL database"]
async fn postgres_revisions_are_workspace_scoped_and_append_only() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("isolated PostgreSQL URL");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO users (user_id, email) VALUES ('native-test-u', 'native-test@example.test')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO workspaces (workspace_id, name, owner_user_id) VALUES ('native-test-a', 'A', 'native-test-u'), ('native-test-b', 'B', 'native-test-u')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO recipes (workspace_id, recipe_id, name, document) VALUES ('native-test-a', 'native-test-r', 'Recipe', '{}')").execute(&pool).await.unwrap();
    let insert = "INSERT INTO recipe_revisions (workspace_id, recipe_id, revision, schema_version, document, content_digest) VALUES ($1, 'native-test-r', 1, 1, '{}', 'digest')";
    sqlx::query(insert)
        .bind("native-test-a")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO recipe_import_provenance (workspace_id, recipe_id, revision, format, mapping_version, diagnostics, source_record) VALUES ('native-test-a', 'native-test-r', 1, 'BeerXML', 'v1', '[]', '{\"raw\":true}')").execute(&pool).await.unwrap();
    assert!(sqlx::query("UPDATE recipe_import_provenance SET diagnostics = 'changed' WHERE recipe_id = 'native-test-r'").execute(&pool).await.is_err());
    assert!(
        sqlx::query(insert)
            .bind("native-test-b")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query(
            "UPDATE recipe_revisions SET document = 'changed' WHERE recipe_id = 'native-test-r'"
        )
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM recipe_revisions WHERE recipe_id = 'native-test-r'")
            .execute(&pool)
            .await
            .is_err()
    );
    let value: String = sqlx::query_scalar("SELECT document FROM recipe_revisions WHERE workspace_id = 'native-test-a' AND recipe_id = 'native-test-r' AND revision = 1").fetch_one(&pool).await.unwrap();
    assert_eq!(value, "{}");
}
