// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{
    ActorContext,
    recipe_editor::{read, save},
};
use krausen_core::DbPool;
use krausen_types::{
    brewery::RecipeDocumentV1,
    recipe_editor::{RecipeReadOutcome, RecipeSaveOutcome},
};
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
        "INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ($1,$2,'member')",
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
    let expired = ActorContext {
        access_expires_at: chrono::Utc::now().timestamp() - 1,
        ..actor.clone()
    };
    assert_eq!(
        read(&db, &expired, "missing").await,
        RecipeReadOutcome::Forbidden
    );
    run!(
        "UPDATE auth_credentials SET revoked_at = $1 WHERE credential_id = $2",
        chrono::Utc::now().timestamp(),
        &actor.credential_id
    );
    assert_eq!(
        read(&db, &actor, "missing").await,
        RecipeReadOutcome::Forbidden
    );
    run!(
        "UPDATE auth_credentials SET revoked_at = NULL WHERE credential_id = $1",
        &actor.credential_id
    );
    let mut draft = RecipeDocumentV1 {
        title: "An incomplete recipe".into(),
        ..Default::default()
    };
    let key = Uuid::new_v4();
    let created = save(&db, &actor, None, None, &draft, key).await;
    let RecipeSaveOutcome::Saved {
        recipe_id,
        revision: 1,
    } = &created
    else {
        panic!("{created:?}");
    };
    assert_eq!(save(&db, &actor, None, None, &draft, key).await, created);
    let refreshed_credential = format!("refresh-{suffix}");
    run!(
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)",
        &refreshed_credential,
        &user,
        chrono::Utc::now().timestamp() + 3600
    );
    let refreshed_actor = ActorContext {
        credential_id: refreshed_credential,
        access_expires_at: chrono::Utc::now().timestamp() + 3600,
        ..actor.clone()
    };
    assert_eq!(
        save(&db, &refreshed_actor, None, None, &draft, key).await,
        created
    );

    draft.title = "Revised".into();
    assert_eq!(
        save(&db, &actor, None, None, &draft, key).await,
        RecipeSaveOutcome::RequestIdentityConflict
    );
    let saved = save(
        &db,
        &actor,
        Some(recipe_id),
        Some(1),
        &draft,
        Uuid::new_v4(),
    )
    .await;
    assert_eq!(
        saved,
        RecipeSaveOutcome::Saved {
            recipe_id: recipe_id.clone(),
            revision: 2
        }
    );
    assert_eq!(
        save(
            &db,
            &actor,
            Some(recipe_id),
            Some(1),
            &draft,
            Uuid::new_v4()
        )
        .await,
        RecipeSaveOutcome::Conflict {
            expected: 1,
            actual: 2
        }
    );
    assert!(
        matches!(read(&db, &actor, recipe_id).await, RecipeReadOutcome::Found(snapshot) if snapshot.document == draft && snapshot.revision == 2)
    );

    let foreign_workspace = format!("foreign-{suffix}");
    run!(
        "INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ($1,$2)",
        &foreign_workspace,
        &user
    );
    run!(
        "INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ($1,$2,'member')",
        &foreign_workspace,
        &user
    );
    let foreign_actor = ActorContext {
        actor_id: user.clone(),
        workspace_id: foreign_workspace,
        ..actor.clone()
    };
    assert_eq!(
        read(&db, &foreign_actor, recipe_id).await,
        RecipeReadOutcome::NotFound
    );
    assert_eq!(
        save(
            &db,
            &foreign_actor,
            Some(recipe_id),
            Some(2),
            &draft,
            Uuid::new_v4()
        )
        .await,
        RecipeSaveOutcome::NotFound
    );
    let mut forged = draft.clone();
    forged.import_provenance_id = Some("foreign-provenance".into());
    assert_eq!(
        save(
            &db,
            &actor,
            Some(recipe_id),
            Some(2),
            &forged,
            Uuid::new_v4()
        )
        .await,
        RecipeSaveOutcome::InvalidProvenance
    );
    forged.import_provenance_id = None;
    forged
        .fermentables
        .push(krausen_types::brewery::FermentableAddition {
            row_id: "new-row".into(),
            amount_kg: Some(1.0),
            addition: krausen_types::brewery::AdditionStage {
                stage: "mash".into(),
                time_minutes: None,
            },
            specification: krausen_types::brewery::FermentableSpecification {
                name: Some("malt".into()),
                producer: None,
                kind: Some("grain".into()),
                yield_fraction: Some(0.8),
                colour_ebc: Some(4.0),
            },
            source: Some(krausen_types::brewery::IngredientSource {
                scope: "workspace".into(),
                catalogue_id: None,
                entry_id: "foreign-entry".into(),
                version: Some(1),
            }),
        });
    assert_eq!(
        save(
            &db,
            &actor,
            Some(recipe_id),
            Some(2),
            &forged,
            Uuid::new_v4()
        )
        .await,
        RecipeSaveOutcome::UnresolvedIngredientSource
    );
    draft.batch_size_l = Some(-1.0);
    assert!(matches!(
        save(
            &db,
            &actor,
            Some(recipe_id),
            Some(2),
            &draft,
            Uuid::new_v4()
        )
        .await,
        RecipeSaveOutcome::Invalid(_)
    ));
    draft.batch_size_l = None;
    draft.title = "Concurrent edit".into();
    let concurrent_key = Uuid::new_v4();
    let (first, second) = tokio::join!(
        save(
            &db,
            &actor,
            Some(recipe_id),
            Some(2),
            &draft,
            concurrent_key
        ),
        save(
            &db,
            &actor,
            Some(recipe_id),
            Some(2),
            &draft,
            concurrent_key
        ),
    );
    assert_eq!(
        first,
        RecipeSaveOutcome::Saved {
            recipe_id: recipe_id.clone(),
            revision: 3
        }
    );
    assert_eq!(second, first);
    run!(
        "UPDATE app_requests SET result_expires_at = 0 WHERE workspace_id = $1 AND request_id = $2",
        &workspace,
        concurrent_key.to_string()
    );
    assert_eq!(
        save(
            &db,
            &actor,
            Some(recipe_id),
            Some(2),
            &draft,
            concurrent_key
        )
        .await,
        RecipeSaveOutcome::ResultExpired {
            resource_id: Some(recipe_id.clone())
        }
    );
    let trigger_name = format!("kra12_{}", suffix.replace('-', ""));
    if db.is_postgres() {
        run!(&format!(
            "CREATE FUNCTION {trigger_name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected change failure'; END $$"
        ));
        run!(&format!(
            "CREATE TRIGGER {trigger_name} BEFORE INSERT ON app_changes FOR EACH ROW WHEN (NEW.workspace_id = '{workspace}') EXECUTE FUNCTION {trigger_name}()"
        ));
    } else {
        run!(&format!(
            "CREATE TRIGGER {trigger_name} BEFORE INSERT ON app_changes WHEN NEW.workspace_id = '{workspace}' BEGIN SELECT RAISE(ABORT, 'injected change failure'); END"
        ));
    }
    let failed_key = Uuid::new_v4();
    draft.title = "Rollback test".into();
    assert_eq!(
        save(&db, &actor, Some(recipe_id), Some(3), &draft, failed_key).await,
        RecipeSaveOutcome::Unavailable
    );
    if db.is_postgres() {
        run!(&format!("DROP TRIGGER {trigger_name} ON app_changes"));
        run!(&format!("DROP FUNCTION {trigger_name}()"));
    } else {
        run!(&format!("DROP TRIGGER {trigger_name}"));
    }
    assert!(
        matches!(read(&db, &actor, recipe_id).await, RecipeReadOutcome::Found(snapshot) if snapshot.revision == 3)
    );
    assert_eq!(
        save(&db, &actor, Some(recipe_id), Some(3), &draft, failed_key).await,
        RecipeSaveOutcome::Saved {
            recipe_id: recipe_id.clone(),
            revision: 4
        }
    );
    run!(
        "UPDATE workspace_users SET active = FALSE WHERE workspace_id = $1 AND user_id = $2",
        &workspace,
        &user
    );
    assert_eq!(
        read(&db, &actor, recipe_id).await,
        RecipeReadOutcome::Forbidden
    );
    assert_eq!(
        save(&db, &actor, None, None, &draft, key).await,
        RecipeSaveOutcome::Forbidden
    );
}

#[tokio::test]
async fn sqlite_editor() {
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
async fn postgres_editor() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL")
        .expect("KRAUSEN_TEST_POSTGRES_URL is required for PostgreSQL tests");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
