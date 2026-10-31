// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{ActorContext, recipe_editor};
use krausen_core::DbPool;
use krausen_types::{
    brewery::RecipeDocumentV1,
    recipe_editor::{RecipeReadOutcome, RecipeSaveOutcome},
};
use uuid::Uuid;

async fn read(db: &DbPool, actor: &ActorContext, recipe_id: &str) -> RecipeReadOutcome {
    let intent = krausen_types::request_context::OperationIntent {
        account_id: actor.actor_id.clone(),
        workspace_id: actor.workspace_id.clone(),
    };
    recipe_editor::read(db, actor, &intent, recipe_id).await
}
async fn save(
    db: &DbPool,
    actor: &ActorContext,
    recipe_id: Option<&str>,
    expected: Option<i64>,
    document: &RecipeDocumentV1,
    request_id: Uuid,
) -> RecipeSaveOutcome {
    let intent = krausen_types::request_context::OperationIntent {
        account_id: actor.actor_id.clone(),
        workspace_id: actor.workspace_id.clone(),
    };
    recipe_editor::save(
        db, actor, &intent, recipe_id, expected, document, request_id,
    )
    .await
}

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
    let original_intent = krausen_types::request_context::OperationIntent {
        account_id: actor.actor_id.clone(),
        workspace_id: actor.workspace_id.clone(),
    };
    assert_eq!(
        recipe_editor::read(&db, &foreign_actor, &original_intent, recipe_id).await,
        RecipeReadOutcome::ContextChanged
    );
    assert_eq!(
        recipe_editor::save(
            &db,
            &foreign_actor,
            &original_intent,
            None,
            None,
            &draft,
            Uuid::new_v4()
        )
        .await,
        RecipeSaveOutcome::ContextChanged
    );
    let other_user = format!("other-{suffix}");
    let other_email = format!("other-{suffix}@example.test");
    let other_credential = format!("other-c-{suffix}");
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2)",
        &other_user,
        &other_email
    );
    run!(
        "INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ($1,$2,'member')",
        &workspace,
        &other_user
    );
    run!(
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)",
        &other_credential,
        &other_user,
        chrono::Utc::now().timestamp() + 3600
    );
    let other_actor = ActorContext {
        actor_id: other_user,
        workspace_id: workspace.clone(),
        credential_id: other_credential,
        access_expires_at: chrono::Utc::now().timestamp() + 3600,
    };
    assert_eq!(
        recipe_editor::read(&db, &other_actor, &original_intent, recipe_id).await,
        RecipeReadOutcome::ContextChanged
    );
    assert_eq!(
        recipe_editor::save(
            &db,
            &other_actor,
            &original_intent,
            None,
            None,
            &draft,
            Uuid::new_v4()
        )
        .await,
        RecipeSaveOutcome::ContextChanged
    );
    let wrong_account = krausen_types::request_context::OperationIntent {
        account_id: "different-account".into(),
        workspace_id: actor.workspace_id.clone(),
    };
    assert_eq!(
        recipe_editor::read(&db, &actor, &wrong_account, recipe_id).await,
        RecipeReadOutcome::ContextChanged
    );
    assert_eq!(
        recipe_editor::save(
            &db,
            &actor,
            &wrong_account,
            None,
            None,
            &draft,
            Uuid::new_v4()
        )
        .await,
        RecipeSaveOutcome::ContextChanged
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

// A save must not recover a stored result or write a revision if its authority
// expires while PostgreSQL is waiting for either resource lock.
#[tokio::test]
async fn postgres_resource_lock_wait_rechecks_both_expiries() {
    use std::str::FromStr;
    use std::time::Duration;

    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL")
        .expect("KRAUSEN_TEST_POSTGRES_URL is required for PostgreSQL tests");
    let control = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&control)
        .await
        .unwrap();
    let suffix = Uuid::new_v4().to_string();
    let user = format!("wait-u-{suffix}");
    let workspace = format!("wait-w-{suffix}");
    let credential = format!("wait-c-{suffix}");
    sqlx::query("INSERT INTO users (user_id,email) VALUES ($1,$2)")
        .bind(&user)
        .bind(format!("{suffix}@example.test"))
        .execute(&control)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ($1,$2)")
        .bind(&workspace)
        .bind(&user)
        .execute(&control)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ($1,$2,'member')")
        .bind(&workspace)
        .bind(&user)
        .execute(&control)
        .await
        .unwrap();
    sqlx::query("INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)")
        .bind(&credential).bind(&user).bind(chrono::Utc::now().timestamp() + 3600).execute(&control).await.unwrap();
    let mut actor = ActorContext {
        actor_id: user,
        workspace_id: workspace.clone(),
        credential_id: credential.clone(),
        access_expires_at: chrono::Utc::now().timestamp() + 3600,
    };
    let pool = DbPool::Postgres(control.clone());
    let draft = RecipeDocumentV1 {
        title: "Lock wait".into(),
        ..Default::default()
    };
    let replay_key = Uuid::new_v4();
    let RecipeSaveOutcome::Saved {
        recipe_id,
        revision: 1,
    } = save(&pool, &actor, None, None, &draft, replay_key).await
    else {
        panic!("initial recipe creation failed");
    };

    for resource in ["workspace", "recipe"] {
        for expiry in ["access", "credential"] {
            let expires_at = chrono::Utc::now().timestamp() + 8;
            let credential_expiry = if expiry == "credential" {
                expires_at
            } else {
                expires_at + 3600
            };
            actor.access_expires_at = if expiry == "access" {
                expires_at
            } else {
                expires_at + 3600
            };
            sqlx::query("UPDATE auth_credentials SET expires_at = $1 WHERE credential_id = $2")
                .bind(credential_expiry)
                .bind(&credential)
                .execute(&control)
                .await
                .unwrap();
            let mut lock = control.begin().await.unwrap();
            if resource == "workspace" {
                sqlx::query(
                    "SELECT workspace_id FROM workspaces WHERE workspace_id = $1 FOR UPDATE",
                )
                .bind(&workspace)
                .fetch_one(&mut *lock)
                .await
                .unwrap();
            } else {
                sqlx::query("SELECT recipe_id FROM recipes WHERE workspace_id = $1 AND recipe_id = $2 FOR UPDATE")
                    .bind(&workspace).bind(&recipe_id).fetch_one(&mut *lock).await.unwrap();
            }
            let app_name = format!("kra12-wait-{}", Uuid::new_v4().simple());
            let options = sqlx::postgres::PgConnectOptions::from_str(&url)
                .unwrap()
                .application_name(&app_name);
            let worker = sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .connect_with(options)
                .await
                .unwrap();
            let task_actor = actor.clone();
            let task_draft = draft.clone();
            let task_recipe = recipe_id.clone();
            let task = tokio::spawn(async move {
                let db = DbPool::Postgres(worker);
                if resource == "workspace" {
                    // An existing request must not be recovered after expiry.
                    save(&db, &task_actor, None, None, &task_draft, replay_key).await
                } else {
                    let mut changed = task_draft;
                    changed.title = "Must not be written".into();
                    save(
                        &db,
                        &task_actor,
                        Some(&task_recipe),
                        Some(1),
                        &changed,
                        Uuid::new_v4(),
                    )
                    .await
                }
            });
            tokio::time::timeout(Duration::from_secs(6), async {
                loop {
                    let waiting: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE application_name = $1 AND wait_event_type = 'Lock')")
                        .bind(&app_name).fetch_one(&control).await.unwrap();
                    if waiting { break; }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }).await.expect("save did not reach the resource lock before expiry");
            while chrono::Utc::now().timestamp() <= expires_at {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            lock.commit().await.unwrap();
            assert_eq!(
                task.await.unwrap(),
                RecipeSaveOutcome::Forbidden,
                "{resource} lock / {expiry} expiry"
            );
            let head: i64 =
                sqlx::query_scalar("SELECT native_head_revision FROM recipes WHERE recipe_id = $1")
                    .bind(&recipe_id)
                    .fetch_one(&control)
                    .await
                    .unwrap();
            assert_eq!(head, 1, "expired write changed recipe head");
        }
    }
}
