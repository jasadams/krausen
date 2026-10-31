// SPDX-License-Identifier: AGPL-3.0-or-later

//! Native helpers run inside the application's existing transaction on both DBs.
use chrono::Utc;
use krausen_brewery::{
    native_recipe::read_document,
    native_store::{self, SaveResult},
    recipe_service::{self, RecipeSource},
};
use krausen_core::{DbPool, Error};
use krausen_types::brewery::RecipeDocumentV1;
use std::time::Duration;

async fn exercise(db: DbPool) {
    let mut first = RecipeDocumentV1::default();
    first.title = "Native".into();
    first.batch_size_l = Some(20.0);
    let now = Utc::now();
    let mut tx = db.begin_write().await.unwrap();
    tokio::time::timeout(
        Duration::from_secs(3),
        native_store::create(&mut tx, "native-ws", "native-r", &first, "native-user", now),
    )
    .await
    .expect("helper must not acquire another connection")
    .unwrap();
    tx.commit().await.unwrap();

    let source = include_str!("../../krausen-beerxml/tests/fixtures/synthetic.xml");
    let imported = krausen_beerxml::native::from_xml(source).unwrap().remove(0);
    let mut tx = db.begin_write().await.unwrap();
    let imported_document = native_store::create_imported(
        &mut tx,
        "native-ws",
        "import-r",
        imported,
        "native-user",
        now,
    )
    .await
    .unwrap();
    let provenance = native_store::load_provenance(&mut tx, "native-ws", "import-r", 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        provenance.original_bytes.as_deref(),
        Some(source.as_bytes())
    );
    assert_eq!(
        imported_document.import_provenance_id.as_deref(),
        Some(provenance.provenance_id.as_str())
    );
    assert!(
        native_store::load_provenance(&mut tx, "wrong-ws", "import-r", 1)
            .await
            .unwrap()
            .is_none()
    );
    tx.commit().await.unwrap();

    let mut tx = db.begin_write().await.unwrap();
    let json = native_store::load_raw(&mut tx, "native-ws", "native-r", 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read_document(&json).unwrap(), first);
    assert_eq!(
        native_store::load_document(&mut tx, "native-ws", "native-r", 1)
            .await
            .unwrap(),
        Some(first.clone())
    );
    let unsupported_sql = "INSERT INTO recipe_revisions (workspace_id, recipe_id, revision, schema_version, document, content_digest) VALUES ('native-ws', 'native-r', 99, 2, '{\"schema_version\":2}', 'future')";
    match &mut tx {
        krausen_core::DbTx::Postgres(inner) => {
            sqlx::query(unsupported_sql)
                .execute(&mut **inner)
                .await
                .unwrap();
        }
        krausen_core::DbTx::Sqlite(inner) => {
            sqlx::query(unsupported_sql)
                .execute(&mut **inner)
                .await
                .unwrap();
        }
    }
    assert!(matches!(
        native_store::load_document(&mut tx, "native-ws", "native-r", 99).await,
        Err(native_store::StoredReadError::UnsupportedVersion(2))
    ));
    assert!(
        native_store::load_raw(&mut tx, "wrong-ws", "native-r", 1)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        native_store::save(
            &mut tx,
            "native-ws",
            "native-r",
            1,
            &first,
            "native-user",
            now,
            None
        )
        .await
        .unwrap(),
        SaveResult::Unchanged(1)
    );
    tx.commit().await.unwrap();

    let mut changed = first.clone();
    changed.title = "Updated".into();
    let mut tx = db.begin_write().await.unwrap();
    assert_eq!(
        native_store::save(
            &mut tx,
            "native-ws",
            "native-r",
            1,
            &changed,
            "native-user",
            now,
            None
        )
        .await
        .unwrap(),
        SaveResult::Appended(2)
    );
    tx.commit().await.unwrap();
    let mut tx = db.begin_write().await.unwrap();
    assert!(matches!(
        native_store::save(
            &mut tx,
            "native-ws",
            "native-r",
            1,
            &first,
            "native-user",
            now,
            None
        )
        .await,
        Err(Error::Conflict(_))
    ));
    tx.rollback().await.unwrap();

    let mut tx = db.begin_write().await.unwrap();
    assert_eq!(
        native_store::save(
            &mut tx,
            "native-ws",
            "native-r",
            2,
            &first,
            "native-user",
            now,
            Some(1)
        )
        .await
        .unwrap(),
        SaveResult::Appended(3)
    );
    tx.commit().await.unwrap();

    // A second writer with the same expected head must lose after the first
    // commits, even when it was already attempting to begin its transaction.
    let mut tx = db.begin_write().await.unwrap();
    assert_eq!(
        native_store::save(
            &mut tx,
            "native-ws",
            "native-r",
            3,
            &changed,
            "native-user",
            now,
            None
        )
        .await
        .unwrap(),
        SaveResult::Appended(4)
    );
    let competing_db = db.clone();
    let competing_document = first.clone();
    let (started, ready) = tokio::sync::oneshot::channel();
    let competing = tokio::spawn(async move {
        started.send(()).unwrap();
        let mut other = competing_db.begin_write().await.unwrap();
        let result = native_store::save(
            &mut other,
            "native-ws",
            "native-r",
            3,
            &competing_document,
            "native-user",
            now,
            None,
        )
        .await;
        other.rollback().await.unwrap();
        result
    });
    ready.await.unwrap();
    tx.commit().await.unwrap();
    assert!(matches!(competing.await.unwrap(), Err(Error::Conflict(_))));

    let mut tx = db.begin_write().await.unwrap();
    assert_eq!(
        native_store::set_archived(&mut tx, "native-ws", "native-r", 0, Some(now))
            .await
            .unwrap(),
        1
    );
    tx.commit().await.unwrap();
    let mut tx = db.begin_write().await.unwrap();
    assert!(matches!(
        native_store::save(
            &mut tx,
            "native-ws",
            "native-r",
            4,
            &first,
            "native-user",
            now,
            None
        )
        .await,
        Err(Error::Conflict(_))
    ));
    tx.rollback().await.unwrap();
    let mut tx = db.begin_write().await.unwrap();
    assert_eq!(
        native_store::set_archived(&mut tx, "native-ws", "native-r", 1, None)
            .await
            .unwrap(),
        2
    );
    tx.commit().await.unwrap();

    let mut tx = db.begin_write().await.unwrap();
    native_store::create(
        &mut tx,
        "native-ws",
        "rollback-r",
        &first,
        "native-user",
        now,
    )
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    let mut tx = db.begin_write().await.unwrap();
    assert!(
        native_store::load_raw(&mut tx, "native-ws", "rollback-r", 1)
            .await
            .unwrap()
            .is_none()
    );
    tx.rollback().await.unwrap();

    let legacy =
        krausen_beerxml::parse("<RECIPE><NAME>Legacy</NAME><VENDOR>retained</VENDOR></RECIPE>")
            .unwrap()
            .recipes
            .remove(0);
    let old = recipe_service::create_recipe(&db, "native-ws", &legacy, RecipeSource::BeerXml, None)
        .await
        .unwrap();
    let mut tx = db.begin_write().await.unwrap();
    assert!(
        native_store::backfill(&mut tx, "native-ws", &old.recipe_id, now)
            .await
            .unwrap()
    );
    assert!(
        !native_store::backfill(&mut tx, "native-ws", &old.recipe_id, now)
            .await
            .unwrap()
    );
    let native = native_store::load_raw(&mut tx, "native-ws", &old.recipe_id, 1)
        .await
        .unwrap()
        .unwrap();
    let document = read_document(&native).unwrap();
    assert_eq!(document.title, "Legacy");
    let provenance = native_store::load_provenance(&mut tx, "native-ws", &old.recipe_id, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        document.import_provenance_id.as_deref(),
        Some(provenance.provenance_id.as_str())
    );
    assert!(provenance.original_bytes.is_none());
    assert!(
        provenance
            .diagnostics
            .contains("original_bytes_unavailable")
    );
    let sql = "SELECT saved_by IS NULL AND saved_at IS NULL AND migrated_at IS NOT NULL FROM recipe_revisions WHERE workspace_id = $1 AND recipe_id = $2 AND revision = 1";
    let honest_history: bool = match &mut tx {
        krausen_core::DbTx::Postgres(inner) => sqlx::query_scalar(sql)
            .bind("native-ws")
            .bind(&old.recipe_id)
            .fetch_one(&mut **inner)
            .await
            .unwrap(),
        krausen_core::DbTx::Sqlite(inner) => sqlx::query_scalar(sql)
            .bind("native-ws")
            .bind(&old.recipe_id)
            .fetch_one(&mut **inner)
            .await
            .unwrap(),
    };
    assert!(honest_history);
    tx.commit().await.unwrap();
    assert!(matches!(
        recipe_service::get_recipe(&db, "native-ws", &old.recipe_id).await,
        Err(Error::Conflict(_))
    ));
}

#[tokio::test]
async fn sqlite_native_transactions() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../apps/server/migrations-sqlite")
        .run(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (user_id, email) VALUES ('native-user', 'native@example.test')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces (workspace_id, name, owner_user_id) VALUES ('native-ws', 'Native', 'native-user')").execute(&pool).await.unwrap();
    exercise(DbPool::Sqlite(pool)).await;
}

#[tokio::test]
#[ignore = "requires an isolated KRAUSEN_TEST_POSTGRES_URL database"]
async fn postgres_native_transactions() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("isolated PostgreSQL URL");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (user_id, email) VALUES ('native-user', 'native@example.test')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces (workspace_id, name, owner_user_id) VALUES ('native-ws', 'Native', 'native-user')").execute(&pool).await.unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
