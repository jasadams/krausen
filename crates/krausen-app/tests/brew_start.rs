// SPDX-License-Identifier: AGPL-3.0-or-later
//! Atomic brew-start acceptance through the public application operation.
use chrono::Utc;
use krausen_app::{
    ActorContext, brew_start, equipment, formulation, recipe_editor, recipe_library,
};
use krausen_core::DbPool;
use krausen_types::{
    brew::{StartBrewOutcome, StartBrewRequest},
    brewery::{
        AdditionStage, BrewStatusV1, ChillingV1, EquipmentProvenance, EquipmentSettingsV1,
        FermentableAddition, FermentableSpecification, ProcessV1, RecipeDocumentV1, StyleReference,
    },
    equipment::{ArchiveEquipment, SaveEquipment},
    formulation::{Preview, PreviewOutcome, PreviewRequest},
    recipe_editor::RecipeSaveOutcome,
    recipe_library::{ArchiveRecipe, LibraryOutcome},
    request_context::OperationIntent,
    validation::Readiness,
};
use uuid::Uuid;

/// Policy source identity of the pre-V2 formulation code at the KRA-20 base.
const V1_POLICY_SOURCE_HASH: &str =
    "c2195592083a8abe4b735d78aa22d7006adb773046d480b78b6462d2699d3bb9";

fn intent(actor: &ActorContext) -> OperationIntent {
    OperationIntent {
        account_id: actor.actor_id.clone(),
        workspace_id: actor.workspace_id.clone(),
    }
}

fn complete_document(profile_id: &str, title: &str) -> RecipeDocumentV1 {
    RecipeDocumentV1 {
        schema_version: 1,
        title: title.into(),
        style: Some(StyleReference {
            name: "Munich Helles".into(),
            edition: Some("BJCP 2021".into()),
            style_id: Some("4A".into()),
        }),
        batch_size_l: Some(20.0),
        boil_minutes: Some(60.0),
        equipment_profile_id: Some(profile_id.into()),
        fermentables: vec![FermentableAddition {
            row_id: "grain-1".into(),
            amount_kg: Some(4.2),
            addition: AdditionStage {
                stage: "mash".into(),
                time_minutes: None,
            },
            specification: FermentableSpecification {
                name: Some("Pilsner malt".into()),
                producer: Some("Weyermann".into()),
                kind: Some("grain".into()),
                yield_fraction: Some(0.8),
                colour_ebc: Some(5.0),
            },
            source: None,
        }],
        process: ProcessV1 {
            apparent_attenuation_fraction: Some(0.78),
            mash_water_l: Some(15.0),
            mash_water_method: Some("manual".into()),
            whirlpool_minutes: Some(15.0),
            kettle_top_up_l: Some(0.0),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn equipment_request(profile_id: &str) -> SaveEquipment {
    SaveEquipment {
        intent: OperationIntent {
            account_id: String::new(),
            workspace_id: String::new(),
        },
        profile_id: profile_id.into(),
        name: "20 L kettle".into(),
        settings: EquipmentSettingsV1 {
            mash_efficiency_fraction: Some(0.75),
            boil_off_l_per_hour: Some(3.0),
            transfer_loss_l: Some(2.0),
            pre_boil_deadspace_l: Some(0.0),
            grain_absorption_l_per_kg: Some(0.8),
            mash_thickness_l_per_kg: Some(2.5),
            kettle_capacity_l: Some(30.0),
            kettle_diameter_cm: Some(35.0),
            kettle_opening_cm: Some(30.0),
            boil_temperature_c: Some(100.0),
            chilling: Some(ChillingV1::Natural),
            ..Default::default()
        },
        provenance: EquipmentProvenance {
            source: "test fixture".into(),
            assumptions: vec!["brewhouse_loss_calibrated".into()],
        },
        expected_revision: 0,
        expected_metadata_revision: 0,
        request_id: String::new(),
    }
}

async fn create_profile(db: &DbPool, actor: &ActorContext, profile_id: &str) {
    let mut request = equipment_request(profile_id);
    request.intent = intent(actor);
    request.request_id = Uuid::new_v4().to_string();
    equipment::save(db, actor, request)
        .await
        .expect("equipment profile fixture");
}

async fn save_recipe(
    db: &DbPool,
    actor: &ActorContext,
    recipe_id: Option<&str>,
    expected: Option<i64>,
    document: &RecipeDocumentV1,
) -> RecipeSaveOutcome {
    recipe_editor::save(
        db,
        actor,
        &intent(actor),
        recipe_id,
        expected,
        document,
        Uuid::new_v4(),
    )
    .await
}

async fn preview_document(
    db: &DbPool,
    actor: &ActorContext,
    document: &RecipeDocumentV1,
) -> Preview {
    let outcome = formulation::preview(
        db,
        actor,
        PreviewRequest {
            draft_generation: "brew-start-test".into(),
            intent: intent(actor),
            draft: document.clone(),
            equipment_draft: Default::default(),
        },
    )
    .await
    .expect("preview call");
    let PreviewOutcome::Preview(preview) = outcome else {
        panic!("preview not available");
    };
    preview
}

fn start_request(
    actor: &ActorContext,
    recipe_id: &str,
    selected: i64,
    expected_head: Option<i64>,
    context: &str,
) -> StartBrewRequest {
    StartBrewRequest {
        intent: intent(actor),
        recipe_id: recipe_id.into(),
        selected_recipe_revision: selected,
        expected_head_revision: expected_head,
        expected_calculation_context: context.into(),
        request_id: Uuid::new_v4().to_string(),
    }
}

async fn exercise(db: DbPool) {
    let suffix = Uuid::new_v4().to_string();
    let user = format!("u-{suffix}");
    let workspace = format!("w-{suffix}");
    let email = format!("{suffix}@example.test");
    macro_rules! run {
        ($sql:expr $(, $a:expr)*) => { match &db {
            DbPool::Postgres(p) => sqlx::query($sql)$(.bind($a))*.execute(p).await.unwrap().rows_affected(),
            DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($a))*.execute(p).await.unwrap().rows_affected(),
        } };
    }
    macro_rules! count {
        ($sql:expr $(, $a:expr)*) => { match &db {
            DbPool::Postgres(p) => sqlx::query_scalar::<_, i64>($sql)$(.bind($a))*.fetch_one(p).await.unwrap(),
            DbPool::Sqlite(p) => sqlx::query_scalar::<_, i64>($sql)$(.bind($a))*.fetch_one(p).await.unwrap(),
        } };
    }
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
        credential_id: credential.clone(),
        access_expires_at: chrono::Utc::now().timestamp() + 3600,
    };

    let profile_id = Uuid::new_v4().to_string();
    create_profile(&db, &actor, &profile_id).await;
    let doc1 = complete_document(&profile_id, "Revision one");
    let RecipeSaveOutcome::Saved {
        recipe_id,
        revision: 1,
    } = save_recipe(&db, &actor, None, None, &doc1).await
    else {
        panic!("recipe fixture");
    };
    let preview1 = preview_document(&db, &actor, &doc1).await;
    assert_eq!(preview1.validation.state, Readiness::Ready);
    let digest1 = preview1.context_digest.clone().expect("context digest");

    let current = start_request(&actor, &recipe_id, 1, Some(1), &digest1);
    let replay = current.clone();
    let StartBrewOutcome::Started {
        brew_id,
        status,
        revision,
        selected_recipe_revision,
        plan,
        ..
    } = brew_start::start(&db, &actor, current).await
    else {
        panic!("current brew start");
    };
    assert_eq!(status, BrewStatusV1::Started);
    assert_eq!(revision, 1);
    assert_eq!(selected_recipe_revision, 1);
    assert_eq!(plan.recipe, doc1);
    assert_eq!(plan.recipe_revision, 1);
    assert_eq!(
        plan.equipment,
        preview1.context.as_ref().unwrap().effective_equipment
    );
    assert_eq!(plan.original_targets, preview1.targets);
    assert_eq!(plan.context_id, digest1);
    assert_eq!(plan.policy_version, "krausen-formulation-v2");
    assert_eq!(
        plan.calculation_context,
        preview1.context.clone().expect("context")
    );
    let stored = {
        let mut tx = db.begin_write().await.unwrap();
        let stored = krausen_brewery::brew_store::load(&mut tx, &workspace, &brew_id)
            .await
            .unwrap()
            .unwrap();
        tx.commit().await.unwrap();
        stored
    };
    assert_eq!(stored.status, BrewStatusV1::Started);
    assert_eq!(stored.plan, plan);
    assert_eq!(stored.plan_content_digest.len(), 64);
    assert_eq!(
        count!(
            "SELECT COUNT(*) FROM brews WHERE workspace_id = $1",
            &workspace
        ),
        1
    );

    // Historical revision while the head advances; no restore or implicit save.
    let doc2 = complete_document(&profile_id, "Revision two");
    assert_eq!(
        save_recipe(&db, &actor, Some(&recipe_id), Some(1), &doc2).await,
        RecipeSaveOutcome::Saved {
            recipe_id: recipe_id.clone(),
            revision: 2
        }
    );
    let historical = start_request(&actor, &recipe_id, 1, None, &digest1);
    let StartBrewOutcome::Started {
        selected_recipe_revision,
        plan,
        ..
    } = brew_start::start(&db, &actor, historical).await
    else {
        panic!("historical brew start");
    };
    assert_eq!(selected_recipe_revision, 1);
    assert_eq!(plan.recipe_revision, 1);
    assert_eq!(plan.recipe.title, "Revision one");
    assert!(
        matches!(recipe_editor::read(&db, &actor, &intent(&actor), &recipe_id).await, krausen_types::recipe_editor::RecipeReadOutcome::Found(snapshot) if snapshot.revision == 2)
    );

    // A start-current guard uses the locked head and creates nothing on conflict.
    let brews_before = count!(
        "SELECT COUNT(*) FROM brews WHERE workspace_id = $1",
        &workspace
    );
    assert!(matches!(
        brew_start::start(
            &db,
            &actor,
            start_request(&actor, &recipe_id, 1, Some(1), &digest1)
        )
        .await,
        StartBrewOutcome::HeadConflict {
            expected_head: 1,
            actual_head: 2,
            refreshed: Some(_)
        }
    ));
    assert_eq!(
        count!(
            "SELECT COUNT(*) FROM brews WHERE workspace_id = $1",
            &workspace
        ),
        brews_before
    );
    assert!(matches!(
        brew_start::start(
            &db,
            &actor,
            start_request(&actor, &recipe_id, 1, Some(2), &digest1)
        )
        .await,
        StartBrewOutcome::InvalidRequest
    ));

    let preview2 = preview_document(&db, &actor, &doc2).await;
    let digest2 = preview2.context_digest.clone().expect("context two");
    let StartBrewOutcome::Started { .. } = brew_start::start(
        &db,
        &actor,
        start_request(&actor, &recipe_id, 2, Some(2), &digest2),
    )
    .await
    else {
        panic!("current revision two start");
    };

    // A changed profile context conflicts with the old accepted digest.
    let mut updated = equipment_request(&profile_id);
    updated.intent = intent(&actor);
    updated.expected_revision = 1;
    updated.expected_metadata_revision = 1;
    updated.settings.boil_off_l_per_hour = Some(4.0);
    updated.request_id = Uuid::new_v4().to_string();
    equipment::save(&db, &actor, updated)
        .await
        .expect("profile update");
    assert!(matches!(
        brew_start::start(
            &db,
            &actor,
            start_request(&actor, &recipe_id, 2, Some(2), &digest2)
        )
        .await,
        StartBrewOutcome::ContextConflict { refreshed } if refreshed.context_digest.as_deref() != Some(digest2.as_str())
    ));
    // An arbitrary old-looking digest (including a v1 accept) is also refused.
    assert!(matches!(
        brew_start::start(
            &db,
            &actor,
            start_request(&actor, &recipe_id, 2, Some(2), &"0".repeat(64))
        )
        .await,
        StartBrewOutcome::ContextConflict { .. }
    ));
    let preview2_new = preview_document(&db, &actor, &doc2).await;
    let digest2_new = preview2_new.context_digest.clone().expect("new context");
    let StartBrewOutcome::Started { .. } = brew_start::start(
        &db,
        &actor,
        start_request(&actor, &recipe_id, 2, Some(2), &digest2_new),
    )
    .await
    else {
        panic!("refreshed current start");
    };
    // A real V1 context envelope (V1 policy version/source hash, no applied
    // defaults) is refused rather than silently accepted under V2.
    let mut v1_context = preview2_new.context.clone().expect("v1 source context");
    v1_context.policy_version = "krausen-formulation-v1".into();
    v1_context.policy_source_hash = V1_POLICY_SOURCE_HASH.into();
    v1_context.applied_defaults.clear();
    let mut v1_value = serde_json::to_value(&v1_context).unwrap();
    v1_value
        .as_object_mut()
        .expect("context object")
        .remove("applied_defaults");
    let v1_digest = krausen_core::canonical_json::from_serializable(&v1_value)
        .ok()
        .and_then(|value| krausen_core::canonical_json::sha256(&value).ok())
        .expect("v1 context digest");
    assert_ne!(v1_digest, digest2_new);
    assert!(matches!(
        brew_start::start(
            &db,
            &actor,
            start_request(&actor, &recipe_id, 2, Some(2), &v1_digest)
        )
        .await,
        StartBrewOutcome::ContextConflict { refreshed }
            if refreshed
                .context
                .as_ref()
                .is_some_and(|context| context.policy_version == "krausen-formulation-v2")
    ));

    // Engine fallbacks actually used are frozen explicitly, not left as nulls.
    let mut defaults_doc = complete_document(&profile_id, "Defaults");
    defaults_doc.process.kettle_top_up_l = None;
    defaults_doc.process.whirlpool_minutes = None;
    let RecipeSaveOutcome::Saved {
        recipe_id: defaults_recipe,
        ..
    } = save_recipe(&db, &actor, None, None, &defaults_doc).await
    else {
        panic!("defaults fixture");
    };
    let defaults_preview = preview_document(&db, &actor, &defaults_doc).await;
    let StartBrewOutcome::Started {
        plan: defaults_plan,
        ..
    } = brew_start::start(
        &db,
        &actor,
        start_request(
            &actor,
            &defaults_recipe,
            1,
            Some(1),
            &defaults_preview.context_digest.unwrap(),
        ),
    )
    .await
    else {
        panic!("defaults start");
    };
    assert!(
        defaults_plan
            .applied_defaults
            .iter()
            .any(|default| default.path == "process.kettle_top_up_l" && default.value == 0.0)
    );
    assert!(
        defaults_plan
            .applied_defaults
            .iter()
            .any(|default| default.path == "process.whirlpool_minutes" && default.value == 0.0)
    );
    assert_eq!(
        defaults_plan.calculation_context.applied_defaults,
        defaults_plan.applied_defaults
    );

    // Saved but Incomplete recipes do not start.
    let mut incomplete = complete_document(&profile_id, "Incomplete");
    incomplete.fermentables.clear();
    let RecipeSaveOutcome::Saved {
        recipe_id: incomplete_id,
        ..
    } = save_recipe(&db, &actor, None, None, &incomplete).await
    else {
        panic!("incomplete fixture");
    };
    let incomplete_preview = preview_document(&db, &actor, &incomplete).await;
    assert_eq!(incomplete_preview.validation.state, Readiness::Incomplete);
    let incomplete_digest = incomplete_preview.context_digest.clone().unwrap();
    assert!(matches!(
        brew_start::start(
            &db,
            &actor,
            start_request(&actor, &incomplete_id, 1, Some(1), &incomplete_digest)
        )
        .await,
        StartBrewOutcome::NotReady(validation) if validation.state == Readiness::Incomplete
    ));

    // A committed start is recoverable after head/context changes; a changed
    // payload for the same key is rejected and an expired result is not rerun.
    let StartBrewOutcome::Started {
        brew_id: first_brew,
        ..
    } = brew_start::start(&db, &actor, replay.clone()).await
    else {
        panic!("replay");
    };
    assert_eq!(first_brew, brew_id);
    let mut changed = replay.clone();
    changed.selected_recipe_revision = 2;
    changed.expected_head_revision = Some(2);
    changed.expected_calculation_context = digest2_new.clone();
    assert_eq!(
        brew_start::start(&db, &actor, changed).await,
        StartBrewOutcome::RequestIdentityConflict
    );
    run!(
        "UPDATE app_requests SET result_expires_at = 0 WHERE actor_id = $1 AND workspace_id = $2 AND operation = 'brew.start.v1' AND request_id = $3",
        &user,
        &workspace,
        &replay.request_id
    );
    assert_eq!(
        brew_start::start(&db, &actor, replay).await,
        StartBrewOutcome::ResultExpired {
            resource_id: Some(brew_id.clone())
        }
    );

    // A foreign recipe and a direct foreign profile reference are tenant-scoped.
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
        workspace_id: foreign_workspace.clone(),
        ..actor.clone()
    };
    let foreign_profile = Uuid::new_v4().to_string();
    create_profile(&db, &foreign_actor, &foreign_profile).await;
    let foreign_doc = complete_document(&foreign_profile, "Foreign");
    let RecipeSaveOutcome::Saved {
        recipe_id: foreign_recipe,
        ..
    } = save_recipe(&db, &foreign_actor, None, None, &foreign_doc).await
    else {
        panic!("foreign recipe fixture");
    };
    let foreign_preview = preview_document(&db, &foreign_actor, &foreign_doc).await;
    assert_eq!(
        brew_start::start(
            &db,
            &actor,
            start_request(
                &actor,
                &foreign_recipe,
                1,
                Some(1),
                &foreign_preview.context_digest.unwrap()
            )
        )
        .await,
        StartBrewOutcome::NotFound
    );
    let tampered_id = Uuid::new_v4().to_string();
    let mut tampered = complete_document(&profile_id, "Tampered");
    tampered.equipment_profile_id = Some(foreign_profile.clone());
    let mut tx = db.begin_write().await.unwrap();
    krausen_brewery::native_store::create(
        &mut tx,
        &workspace,
        &tampered_id,
        &tampered,
        &user,
        Utc::now(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert!(matches!(
        brew_start::start(
            &db,
            &actor,
            start_request(&actor, &tampered_id, 1, Some(1), &"0".repeat(64))
        )
        .await,
        StartBrewOutcome::UnresolvedEquipmentProfile
    ));

    // Rollback after the plan/change write leaves no brew and a retry succeeds.
    let rollback_doc = complete_document(&profile_id, "Rollback brew");
    let RecipeSaveOutcome::Saved {
        recipe_id: rollback_recipe,
        ..
    } = save_recipe(&db, &actor, None, None, &rollback_doc).await
    else {
        panic!("rollback fixture");
    };
    let rollback_preview = preview_document(&db, &actor, &rollback_doc).await;
    let rollback_request = start_request(
        &actor,
        &rollback_recipe,
        1,
        Some(1),
        &rollback_preview.context_digest.unwrap(),
    );
    let trigger_name = format!("brew_{}", suffix.replace('-', ""));
    if db.is_postgres() {
        run!(&format!(
            "CREATE FUNCTION {trigger_name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected brew change failure'; END $$"
        ));
        run!(&format!(
            "CREATE TRIGGER {trigger_name} BEFORE INSERT ON app_changes FOR EACH ROW WHEN (NEW.workspace_id = '{workspace}') EXECUTE FUNCTION {trigger_name}()"
        ));
    } else {
        run!(&format!(
            "CREATE TRIGGER {trigger_name} BEFORE INSERT ON app_changes WHEN NEW.workspace_id = '{workspace}' BEGIN SELECT RAISE(ABORT, 'injected brew change failure'); END"
        ));
    }
    assert_eq!(
        brew_start::start(&db, &actor, rollback_request.clone()).await,
        StartBrewOutcome::Unavailable
    );
    assert_eq!(
        count!(
            "SELECT COUNT(*) FROM brews WHERE workspace_id = $1 AND recipe_id = $2",
            &workspace,
            &rollback_recipe
        ),
        0
    );
    if db.is_postgres() {
        run!(&format!("DROP TRIGGER {trigger_name} ON app_changes"));
        run!(&format!("DROP FUNCTION {trigger_name}()"));
    } else {
        run!(&format!("DROP TRIGGER {trigger_name}"));
    }
    assert!(matches!(
        brew_start::start(&db, &actor, rollback_request).await,
        StartBrewOutcome::Started { .. }
    ));

    // Archived recipe/profile selection is rejected; prior plans remain readable.
    let profile_state = {
        let mut tx = db.begin_write().await.unwrap();
        let profile =
            krausen_brewery::equipment_store::load(&mut tx, &workspace, &profile_id, false)
                .await
                .unwrap()
                .unwrap();
        tx.commit().await.unwrap();
        profile
    };
    assert!(matches!(
        equipment::set_archived(
            &db,
            &actor,
            ArchiveEquipment {
                intent: intent(&actor),
                profile_id: profile_id.clone(),
                archived: true,
                expected_metadata_revision: profile_state.metadata_revision,
                request_id: Uuid::new_v4().to_string(),
            }
        )
        .await,
        Ok(profile) if profile.archived
    ));
    assert_eq!(
        brew_start::start(
            &db,
            &actor,
            start_request(&actor, &recipe_id, 2, Some(2), &digest2_new)
        )
        .await,
        StartBrewOutcome::EquipmentProfileArchived
    );
    assert!(matches!(
        recipe_library::archive(
            &db,
            &actor,
            ArchiveRecipe {
                intent: intent(&actor),
                recipe_id: recipe_id.clone(),
                archived: true,
                expected_metadata_revision: 0,
                request_id: Uuid::new_v4().to_string(),
            }
        )
        .await,
        LibraryOutcome::Archive { archived: true, .. }
    ));
    assert_eq!(
        brew_start::start(
            &db,
            &actor,
            start_request(&actor, &recipe_id, 2, Some(2), &digest2_new)
        )
        .await,
        StartBrewOutcome::RecipeArchived
    );
    let still_readable = {
        let mut tx = db.begin_write().await.unwrap();
        let stored = krausen_brewery::brew_store::load(&mut tx, &workspace, &brew_id)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        stored
    };
    assert!(still_readable.is_some());
}

#[tokio::test]
async fn sqlite_brew_start() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../apps/server/migrations-sqlite")
        .run(&pool)
        .await
        .unwrap();
    Box::pin(exercise(DbPool::Sqlite(pool))).await;
}

#[tokio::test]
async fn postgres_brew_start() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL")
        .expect("KRAUSEN_TEST_POSTGRES_URL is required for PostgreSQL tests");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    Box::pin(exercise(DbPool::Postgres(pool))).await;
}

#[tokio::test]
async fn postgres_start_rechecks_authority_after_profile_lock_wait() {
    use std::time::Duration;

    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL")
        .expect("KRAUSEN_TEST_POSTGRES_URL is required for PostgreSQL tests");
    let control = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&control)
        .await
        .unwrap();
    let suffix = Uuid::new_v4().to_string();
    let user = format!("lock-u-{suffix}");
    let workspace = format!("lock-w-{suffix}");
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
    let credential = format!("lock-c-{suffix}");
    sqlx::query("INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)")
        .bind(&credential)
        .bind(&user)
        .bind(chrono::Utc::now().timestamp() + 3600)
        .execute(&control)
        .await
        .unwrap();
    let actor = ActorContext {
        actor_id: user,
        workspace_id: workspace.clone(),
        credential_id: credential,
        access_expires_at: chrono::Utc::now().timestamp() + 60,
    };
    let db = DbPool::Postgres(control.clone());
    let profile_id = Uuid::new_v4().to_string();
    create_profile(&db, &actor, &profile_id).await;
    let document = complete_document(&profile_id, "Lock brew");
    let RecipeSaveOutcome::Saved { recipe_id, .. } =
        save_recipe(&db, &actor, None, None, &document).await
    else {
        panic!("lock recipe fixture");
    };
    let preview = preview_document(&db, &actor, &document).await;
    let request = start_request(
        &actor,
        &recipe_id,
        1,
        Some(1),
        &preview.context_digest.unwrap(),
    );
    let mut lock = control.begin().await.unwrap();
    sqlx::query("SELECT profile_id FROM equipment_profiles WHERE workspace_id=$1 AND profile_id=$2 FOR UPDATE")
        .bind(&workspace)
        .bind(&profile_id)
        .fetch_optional(&mut *lock)
        .await
        .unwrap();
    let task_actor = ActorContext {
        access_expires_at: chrono::Utc::now().timestamp() + 3,
        ..actor.clone()
    };
    let task_db = DbPool::Postgres(control.clone());
    let handle =
        tokio::spawn(async move { brew_start::start(&task_db, &task_actor, request).await });
    tokio::time::sleep(Duration::from_secs(4)).await;
    lock.rollback().await.unwrap();
    assert_eq!(handle.await.unwrap(), StartBrewOutcome::Forbidden);
    let brews: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM brews WHERE workspace_id=$1")
        .bind(&workspace)
        .fetch_one(&control)
        .await
        .unwrap();
    assert_eq!(brews, 0);
}
