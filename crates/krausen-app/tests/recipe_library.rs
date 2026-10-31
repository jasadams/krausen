// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{ActorContext, recipe_editor, recipe_library};
use krausen_core::DbPool;
use krausen_types::{
    brewery::RecipeDocumentV1,
    recipe_editor::RecipeSaveOutcome,
    recipe_library::{
        ArchiveRecipe, HistoryQuery, LibraryOutcome, LibraryQuery, RecoveryQuery, RevisionQuery,
    },
    request_context::OperationIntent,
};

async fn exercise(db: DbPool) {
    let suffix = uuid::Uuid::new_v4();
    let user = format!("u-{suffix}");
    let a = format!("a-{suffix}");
    let b = format!("b-{suffix}");
    let credential = format!("c-{suffix}");
    macro_rules! run { ($sql:expr $(,$arg:expr)*) => { match &db { DbPool::Postgres(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(), DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected() } }; }
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2)",
        &user,
        format!("{suffix}@example.test")
    );
    for ws in [&a, &b] {
        run!(
            "INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ($1,$2)",
            ws,
            &user
        );
        run!(
            "INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ($1,$2,'member')",
            ws,
            &user
        );
    }
    run!(
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)",
        &credential,
        &user,
        4102444800_i64
    );
    let actor = ActorContext {
        actor_id: user.clone(),
        workspace_id: a.clone(),
        credential_id: credential,
        access_expires_at: 4102444800,
    };
    let intent = OperationIntent {
        account_id: user.clone(),
        workspace_id: a.clone(),
    };
    let other = ActorContext {
        workspace_id: b.clone(),
        ..actor.clone()
    };
    let other_intent = OperationIntent {
        account_id: user,
        workspace_id: b,
    };
    let mut ids = Vec::new();
    for i in 0..102 {
        let doc = RecipeDocumentV1 {
            title: format!("Recipe {i:03}"),
            ..Default::default()
        };
        let RecipeSaveOutcome::Saved {
            recipe_id,
            revision: 1,
        } = recipe_editor::save(&db, &actor, &intent, None, None, &doc, uuid::Uuid::new_v4()).await
        else {
            panic!("create")
        };
        ids.push(recipe_id);
    }
    let query = LibraryQuery {
        intent: intent.clone(),
        search: String::new(),
        archived: false,
        cursor: None,
        limit: 50,
    };
    let LibraryOutcome::Recipes(first) = recipe_library::list(&db, &actor, query.clone()).await
    else {
        panic!("list")
    };
    assert_eq!(first.items.len(), 50);
    let token = first.next_cursor.clone().unwrap();
    let mut foreign_query = query.clone();
    foreign_query.intent = other_intent.clone();
    foreign_query.cursor = Some(token.clone());
    assert_eq!(
        recipe_library::list(&db, &other, foreign_query).await,
        LibraryOutcome::Invalid
    );
    let mut wrong_filter = query.clone();
    wrong_filter.archived = true;
    wrong_filter.cursor = Some(token.clone());
    assert_eq!(
        recipe_library::list(&db, &actor, wrong_filter).await,
        LibraryOutcome::Invalid
    );
    let mut second_query = query.clone();
    second_query.cursor = Some(token);
    let LibraryOutcome::Recipes(second) = recipe_library::list(&db, &actor, second_query).await
    else {
        panic!("second page")
    };
    let mut third_query = query.clone();
    third_query.cursor = second.next_cursor.clone();
    let LibraryOutcome::Recipes(third) = recipe_library::list(&db, &actor, third_query).await
    else {
        panic!("third page")
    };
    let all: Vec<_> = first
        .items
        .into_iter()
        .chain(second.items)
        .chain(third.items)
        .collect();
    assert_eq!(all.len(), 102);
    assert!(
        all.windows(2)
            .all(|p| (&p[0].created_at, &p[0].recipe_id) < (&p[1].created_at, &p[1].recipe_id))
    );
    let mut searched = query.clone();
    searched.search = "Recipe 010".into();
    let LibraryOutcome::Recipes(found) = recipe_library::list(&db, &actor, searched).await else {
        panic!("search")
    };
    assert_eq!(found.items.len(), 1);
    // Failed legacy backfills remain discoverable and recoverable under the
    // same current workspace authority after replacing the old library.
    let legacy_id = format!("legacy-{suffix}");
    run!(
        "INSERT INTO recipes (workspace_id,recipe_id,name,document,native_migration_error) VALUES ($1,$2,$3,$4,$5)",
        &a,
        &legacy_id,
        "Legacy recover",
        "original source",
        "cannot map"
    );
    let mut recovery_search = query.clone();
    recovery_search.search = "Legacy recover".into();
    assert!(
        matches!(recipe_library::list(&db, &actor, recovery_search).await, LibraryOutcome::Recipes(page) if page.items.len() == 1 && page.items[0].needs_recovery)
    );
    let recovery_query = RecoveryQuery {
        intent: intent.clone(),
        recipe_id: legacy_id.clone(),
    };
    assert!(
        matches!(recipe_library::recover(&db, &actor, recovery_query.clone()).await, LibraryOutcome::LegacySource { source, .. } if source == "original source")
    );
    let foreign_recovery = RecoveryQuery {
        intent: other_intent.clone(),
        ..recovery_query
    };
    assert_eq!(
        recipe_library::recover(&db, &other, foreign_recovery).await,
        LibraryOutcome::NotFound
    );
    let id = ids[0].clone();
    let archive = ArchiveRecipe {
        intent: intent.clone(),
        recipe_id: id.clone(),
        expected_metadata_revision: 0,
        archived: true,
        request_id: uuid::Uuid::new_v4().to_string(),
    };
    let result = recipe_library::archive(&db, &actor, archive.clone()).await;
    assert_eq!(
        result,
        LibraryOutcome::Archive {
            recipe_id: id.clone(),
            metadata_revision: 1,
            archived: true
        }
    );
    assert_eq!(
        recipe_library::archive(&db, &actor, archive.clone()).await,
        result
    );
    let mut mismatch = archive.clone();
    mismatch.archived = false;
    assert_eq!(
        recipe_library::archive(&db, &actor, mismatch).await,
        LibraryOutcome::RequestIdentityConflict
    );
    let mut foreign_archive = archive.clone();
    foreign_archive.intent = other_intent.clone();
    foreign_archive.request_id = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        recipe_library::archive(&db, &other, foreign_archive).await,
        LibraryOutcome::NotFound
    );
    let history = HistoryQuery {
        intent: intent.clone(),
        recipe_id: id.clone(),
        cursor: None,
        limit: 50,
    };
    assert!(
        matches!(recipe_library::history(&db, &actor, history).await, LibraryOutcome::History(page) if page.items.len() == 1)
    );
    let revision = RevisionQuery {
        intent: intent.clone(),
        recipe_id: id.clone(),
        revision: 1,
    };
    assert!(
        matches!(recipe_library::revision(&db, &actor, revision).await, LibraryOutcome::Revision(doc) if doc.title == "Recipe 000")
    );
    let mut archived_query = query.clone();
    archived_query.archived = true;
    assert!(
        matches!(recipe_library::list(&db, &actor, archived_query).await, LibraryOutcome::Recipes(page) if page.items.len() == 1 && page.items[0].recipe_id == id)
    );
    let restore = ArchiveRecipe {
        archived: false,
        expected_metadata_revision: 1,
        request_id: uuid::Uuid::new_v4().to_string(),
        ..archive
    };
    assert_eq!(
        recipe_library::archive(&db, &actor, restore).await,
        LibraryOutcome::Archive {
            recipe_id: id.clone(),
            metadata_revision: 2,
            archived: false
        }
    );
    let count: i64 = match &db {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM recipe_archive_events WHERE workspace_id=$1 AND recipe_id=$2",
        )
        .bind(&a)
        .bind(&id)
        .fetch_one(p)
        .await
        .unwrap(),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM recipe_archive_events WHERE workspace_id=$1 AND recipe_id=$2",
        )
        .bind(&a)
        .bind(&id)
        .fetch_one(p)
        .await
        .unwrap(),
    };
    assert_eq!(count, 2);
    let immutable = match &db {
        DbPool::Postgres(p) => sqlx::query("UPDATE recipe_archive_events SET archived=FALSE WHERE workspace_id=$1 AND recipe_id=$2").bind(&a).bind(&id).execute(p).await.is_err(),
        DbPool::Sqlite(p) => sqlx::query("UPDATE recipe_archive_events SET archived=0 WHERE workspace_id=$1 AND recipe_id=$2").bind(&a).bind(&id).execute(p).await.is_err(),
    };
    assert!(immutable);
    if db.is_postgres() {
        run!(
            "CREATE OR REPLACE FUNCTION kra16_reject_event() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$"
        );
        run!(
            "CREATE TRIGGER kra16_reject_event BEFORE INSERT ON recipe_archive_events FOR EACH ROW EXECUTE FUNCTION kra16_reject_event()"
        );
    } else {
        run!(
            "CREATE TRIGGER kra16_reject_event BEFORE INSERT ON recipe_archive_events BEGIN SELECT RAISE(ABORT, 'injected'); END"
        );
    }
    let retry = ArchiveRecipe {
        intent: intent.clone(),
        recipe_id: id.clone(),
        expected_metadata_revision: 2,
        archived: true,
        request_id: uuid::Uuid::new_v4().to_string(),
    };
    assert_eq!(
        recipe_library::archive(&db, &actor, retry.clone()).await,
        LibraryOutcome::Unavailable
    );
    if db.is_postgres() {
        run!("DROP TRIGGER kra16_reject_event ON recipe_archive_events");
        run!("DROP FUNCTION kra16_reject_event()");
    } else {
        run!("DROP TRIGGER kra16_reject_event");
    }
    assert_eq!(
        recipe_library::archive(&db, &actor, retry).await,
        LibraryOutcome::Archive {
            recipe_id: id,
            metadata_revision: 3,
            archived: true
        }
    );

    run!(
        "UPDATE auth_credentials SET revoked_at=$1 WHERE credential_id=$2",
        chrono::Utc::now().timestamp(),
        &actor.credential_id
    );
    assert_eq!(
        recipe_library::list(&db, &actor, query).await,
        LibraryOutcome::Forbidden
    );
}
#[tokio::test]
async fn sqlite_library() {
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
async fn postgres_library() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
