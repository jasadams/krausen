// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared process-summary acceptance through the public application operation.
use krausen_app::{ActorContext, equipment, process_summary, recipe_editor};
use krausen_core::DbPool;
use krausen_types::{
    brewery::{
        AdditionStage, ChillingV1, EquipmentProvenance, EquipmentSettingsV1, FermentableAddition,
        FermentableSpecification, HopAddition, HopSpecification, IngredientAddition,
        IngredientSpecification, ProcessStep, ProcessV1, RecipeDocumentV1, StyleReference,
    },
    equipment::SaveEquipment,
    process_summary::{
        ProcessSummaryOutcome, ProcessSummaryRequest, ProcessSummarySelection, SummarySource,
    },
    recipe_editor::{RecipeReadOutcome, RecipeSaveOutcome},
    request_context::OperationIntent,
    validation::Readiness,
};
use uuid::Uuid;

fn intent(actor: &ActorContext) -> OperationIntent {
    OperationIntent {
        account_id: actor.actor_id.clone(),
        workspace_id: actor.workspace_id.clone(),
    }
}

fn summary_request(
    actor: &ActorContext,
    selection: ProcessSummarySelection,
) -> ProcessSummaryRequest {
    ProcessSummaryRequest {
        intent: intent(actor),
        selection,
    }
}

async fn summary(
    db: &DbPool,
    actor: &ActorContext,
    selection: ProcessSummarySelection,
) -> ProcessSummaryOutcome {
    process_summary::read(db, actor, summary_request(actor, selection)).await
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

fn complete_draft(profile_id: &str) -> RecipeDocumentV1 {
    RecipeDocumentV1 {
        schema_version: 1,
        title: "Saved title".into(),
        description: Some("A clean lager.".into()),
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
        hops: vec![
            HopAddition {
                row_id: "hop-1".into(),
                amount_g: Some(30.0),
                stage: "boil".into(),
                contact_minutes: Some(60.0),
                temperature_c: None,
                specification: HopSpecification {
                    name: Some("Hallertauer".into()),
                    producer: None,
                    alpha_fraction: Some(0.05),
                },
                source: None,
            },
            HopAddition {
                row_id: "hop-dry".into(),
                amount_g: Some(50.0),
                stage: "dry_hop".into(),
                contact_minutes: Some(1440.0),
                temperature_c: Some(18.0),
                specification: HopSpecification {
                    name: Some("Hallertauer".into()),
                    producer: None,
                    alpha_fraction: Some(0.05),
                },
                source: None,
            },
        ],
        yeast: vec![IngredientAddition {
            row_id: "yeast-1".into(),
            amount: Some(1.0),
            unit: Some("pack".into()),
            specification: IngredientSpecification {
                name: Some("German Lager".into()),
                producer: Some("Wyeast 2308".into()),
                attenuation_fraction: Some(0.78),
                extensions: Default::default(),
            },
            source: None,
        }],
        process: ProcessV1 {
            apparent_attenuation_fraction: Some(0.78),
            mash_steps: vec![ProcessStep {
                row_id: "mash-1".into(),
                name: Some("Mash in".into()),
                time_minutes: Some(60.0),
                temperature_c: Some(63.0),
                volume_l: Some(15.0),
                extensions: Default::default(),
            }],
            fermentation_steps: vec![ProcessStep {
                row_id: "ferment-1".into(),
                name: Some("Primary".into()),
                time_minutes: Some(14.0 * 24.0 * 60.0),
                temperature_c: Some(10.0),
                volume_l: None,
                extensions: Default::default(),
            }],
            whirlpool_minutes: Some(15.0),
            whirlpool_temperature_c: Some(95.0),
            mash_water_l: Some(15.0),
            mash_water_method: Some("manual".into()),
            kettle_top_up_l: Some(0.0),
            unsupported: Default::default(),
        },
        notes: Some("Pitch cool.".into()),
        ..Default::default()
    }
}

fn equipment_request(name: &str) -> SaveEquipment {
    SaveEquipment {
        intent: OperationIntent {
            account_id: String::new(),
            workspace_id: String::new(),
        },
        profile_id: String::new(),
        name: name.into(),
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

async fn create_equipment(db: &DbPool, actor: &ActorContext, profile_id: &str, name: &str) {
    let mut request = equipment_request(name);
    request.intent = intent(actor);
    request.profile_id = profile_id.into();
    request.request_id = Uuid::new_v4().to_string();
    equipment::save(db, actor, request)
        .await
        .expect("equipment profile fixture");
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
    create_equipment(&db, &actor, &profile_id, "20 L kettle").await;
    let mut draft = complete_draft(&profile_id);
    let RecipeSaveOutcome::Saved {
        recipe_id,
        revision: 1,
    } = save_recipe(&db, &actor, None, None, &draft).await
    else {
        panic!("initial recipe save failed");
    };
    draft.title = "Saved revision two".into();
    assert_eq!(
        save_recipe(&db, &actor, Some(&recipe_id), Some(1), &draft).await,
        RecipeSaveOutcome::Saved {
            recipe_id: recipe_id.clone(),
            revision: 2
        }
    );

    let ProcessSummaryOutcome::Summary(saved) = summary(
        &db,
        &actor,
        ProcessSummarySelection::Saved {
            recipe_id: recipe_id.clone(),
            revision: 2,
        },
    )
    .await
    else {
        panic!("saved summary unavailable");
    };
    assert_eq!(
        saved.source,
        SummarySource::Saved {
            recipe_id: recipe_id.clone(),
            revision: 2
        }
    );
    assert_eq!(saved.title, "Saved revision two");
    assert_eq!(saved.batch_size_l, Some(20.0));
    assert_eq!(saved.readiness, Readiness::Ready);
    assert_eq!(
        saved.equipment.profile_id.as_deref(),
        Some(profile_id.as_str())
    );
    assert_eq!(saved.equipment.provenance, "saved_profile");
    assert_eq!(
        saved.equipment.settings.mash_efficiency_fraction,
        Some(0.75)
    );
    assert!(saved.context.digest.is_some());
    assert_eq!(
        saved
            .style_ranges
            .as_ref()
            .map(|style| style.style_id.as_str()),
        Some("4A")
    );
    assert_eq!(saved.fermentables[0].unit.as_deref(), Some("kg"));
    assert_eq!(
        saved
            .hops
            .iter()
            .find(|hop| hop.row_id == "hop-dry")
            .and_then(|hop| hop.time_minutes),
        Some(1440.0)
    );
    assert!(
        saved
            .assumptions
            .iter()
            .any(|assumption| assumption.starts_with("profile:"))
    );

    // An explicitly selected older revision keeps its own identity and content.
    let ProcessSummaryOutcome::Summary(first) = summary(
        &db,
        &actor,
        ProcessSummarySelection::Saved {
            recipe_id: recipe_id.clone(),
            revision: 1,
        },
    )
    .await
    else {
        panic!("saved revision 1 summary unavailable");
    };
    assert_eq!(first.title, "Saved title");
    assert_eq!(
        first.source,
        SummarySource::Saved {
            recipe_id: recipe_id.clone(),
            revision: 1
        }
    );

    let revisions_before = count!(
        "SELECT COUNT(*) FROM recipe_revisions WHERE workspace_id = $1 AND recipe_id = $2",
        &workspace,
        &recipe_id
    );
    let mut unsaved = draft.clone();
    unsaved.title = "Unsaved draft title".into();
    let ProcessSummaryOutcome::Summary(draft_summary) = summary(
        &db,
        &actor,
        ProcessSummarySelection::Draft {
            recipe_id: Some(recipe_id.clone()),
            base_revision: Some(2),
            document: unsaved.clone(),
        },
    )
    .await
    else {
        panic!("draft summary unavailable");
    };
    assert_eq!(draft_summary.title, "Unsaved draft title");
    assert_eq!(
        draft_summary.source,
        SummarySource::UnsavedDraft {
            recipe_id: Some(recipe_id.clone()),
            base_revision: Some(2)
        }
    );
    assert_eq!(
        count!(
            "SELECT COUNT(*) FROM recipe_revisions WHERE workspace_id = $1 AND recipe_id = $2",
            &workspace,
            &recipe_id
        ),
        revisions_before,
        "printing a draft must not persist a revision"
    );
    assert!(
        matches!(recipe_editor::read(&db, &actor, &intent(&actor), &recipe_id).await, RecipeReadOutcome::Found(snapshot) if snapshot.document.title == "Saved revision two")
    );

    // Draft source identity is scoped and checked; historical bases remain valid.
    let ProcessSummaryOutcome::Summary(historical) = summary(
        &db,
        &actor,
        ProcessSummarySelection::Draft {
            recipe_id: Some(recipe_id.clone()),
            base_revision: Some(1),
            document: unsaved.clone(),
        },
    )
    .await
    else {
        panic!("historical base draft summary unavailable");
    };
    assert_eq!(
        historical.source,
        SummarySource::UnsavedDraft {
            recipe_id: Some(recipe_id.clone()),
            base_revision: Some(1)
        }
    );
    for selection in [
        ProcessSummarySelection::Draft {
            recipe_id: Some(recipe_id.clone()),
            base_revision: None,
            document: unsaved.clone(),
        },
        ProcessSummarySelection::Draft {
            recipe_id: None,
            base_revision: Some(1),
            document: unsaved.clone(),
        },
        ProcessSummarySelection::Draft {
            recipe_id: Some(recipe_id.clone()),
            base_revision: Some(0),
            document: unsaved.clone(),
        },
    ] {
        assert_eq!(
            summary(&db, &actor, selection).await,
            ProcessSummaryOutcome::InvalidRequest
        );
    }
    assert_eq!(
        summary(
            &db,
            &actor,
            ProcessSummarySelection::Draft {
                recipe_id: Some(recipe_id.clone()),
                base_revision: Some(99),
                document: unsaved.clone(),
            },
        )
        .await,
        ProcessSummaryOutcome::NotFound
    );
    let mut other = complete_draft(&profile_id);
    other.title = "Other recipe".into();
    let RecipeSaveOutcome::Saved {
        recipe_id: other_recipe,
        ..
    } = save_recipe(&db, &actor, None, None, &other).await
    else {
        panic!("other recipe fixture");
    };
    other.title = "Other recipe two".into();
    assert_eq!(
        save_recipe(&db, &actor, Some(&other_recipe), Some(1), &other).await,
        RecipeSaveOutcome::Saved {
            recipe_id: other_recipe.clone(),
            revision: 2
        }
    );
    other.title = "Other recipe three".into();
    assert_eq!(
        save_recipe(&db, &actor, Some(&other_recipe), Some(2), &other).await,
        RecipeSaveOutcome::Saved {
            recipe_id: other_recipe.clone(),
            revision: 3
        }
    );
    assert_eq!(
        summary(
            &db,
            &actor,
            ProcessSummarySelection::Draft {
                recipe_id: Some(recipe_id.clone()),
                base_revision: Some(3),
                document: unsaved.clone(),
            },
        )
        .await,
        ProcessSummaryOutcome::NotFound,
        "a revision from another recipe must not label this draft"
    );

    // Incomplete drafts still produce an honest sheet with diagnostics and no invented targets.
    let incomplete = RecipeDocumentV1 {
        title: "Incomplete draft".into(),
        ..Default::default()
    };
    let ProcessSummaryOutcome::Summary(incomplete) = summary(
        &db,
        &actor,
        ProcessSummarySelection::Draft {
            recipe_id: None,
            base_revision: None,
            document: incomplete,
        },
    )
    .await
    else {
        panic!("incomplete summary unavailable");
    };
    assert_eq!(incomplete.readiness, Readiness::Incomplete);
    assert!(!incomplete.diagnostics.is_empty());
    assert!(incomplete.targets.original_sg.is_none());

    let mut invalid = complete_draft(&profile_id);
    invalid.batch_size_l = Some(-1.0);
    let ProcessSummaryOutcome::Summary(invalid) = summary(
        &db,
        &actor,
        ProcessSummarySelection::Draft {
            recipe_id: None,
            base_revision: None,
            document: invalid,
        },
    )
    .await
    else {
        panic!("invalid summary unavailable");
    };
    assert_eq!(invalid.readiness, Readiness::Invalid);
    assert!(!invalid.diagnostics.is_empty());

    let unresolved_style = RecipeDocumentV1 {
        title: "Unknown style".into(),
        style: Some(StyleReference {
            name: "Not a supported style".into(),
            edition: Some("BJCP 2021".into()),
            style_id: Some("99Z".into()),
        }),
        ..Default::default()
    };
    assert_eq!(
        summary(
            &db,
            &actor,
            ProcessSummarySelection::Draft {
                recipe_id: None,
                base_revision: None,
                document: unresolved_style,
            },
        )
        .await,
        ProcessSummaryOutcome::UnresolvedStyle
    );

    // A second workspace the same user belongs to must not supply linked data.
    let foreign_workspace = format!("foreign-w-{suffix}");
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
    create_equipment(&db, &foreign_actor, &foreign_profile, "Foreign kettle").await;
    let foreign_draft = RecipeDocumentV1 {
        title: "Foreign recipe".into(),
        ..Default::default()
    };
    let RecipeSaveOutcome::Saved {
        recipe_id: foreign_recipe,
        ..
    } = save_recipe(&db, &foreign_actor, None, None, &foreign_draft).await
    else {
        panic!("foreign recipe fixture");
    };
    assert_eq!(
        summary(
            &db,
            &actor,
            ProcessSummarySelection::Saved {
                recipe_id: foreign_recipe.clone(),
                revision: 1,
            },
        )
        .await,
        ProcessSummaryOutcome::NotFound
    );
    assert_eq!(
        summary(
            &db,
            &actor,
            ProcessSummarySelection::Draft {
                recipe_id: Some(foreign_recipe),
                base_revision: Some(1),
                document: draft.clone(),
            },
        )
        .await,
        ProcessSummaryOutcome::NotFound,
        "a foreign draft source identity must not be labelled"
    );
    let cross_workspace_profile = RecipeDocumentV1 {
        title: "Cross-workspace profile".into(),
        equipment_profile_id: Some(foreign_profile),
        ..Default::default()
    };
    assert_eq!(
        summary(
            &db,
            &actor,
            ProcessSummarySelection::Draft {
                recipe_id: None,
                base_revision: None,
                document: cross_workspace_profile,
            },
        )
        .await,
        ProcessSummaryOutcome::UnresolvedEquipmentProfile
    );
    assert_eq!(
        process_summary::read(
            &db,
            &actor,
            ProcessSummaryRequest {
                intent: intent(&foreign_actor),
                selection: ProcessSummarySelection::Draft {
                    recipe_id: None,
                    base_revision: None,
                    document: draft.clone(),
                },
            },
        )
        .await,
        ProcessSummaryOutcome::ContextChanged
    );
    assert_eq!(
        summary(
            &db,
            &actor,
            ProcessSummarySelection::Saved {
                recipe_id: recipe_id.clone(),
                revision: 0,
            },
        )
        .await,
        ProcessSummaryOutcome::InvalidRequest
    );
    let mut wrong_schema = draft.clone();
    wrong_schema.schema_version = 2;
    assert_eq!(
        summary(
            &db,
            &actor,
            ProcessSummarySelection::Draft {
                recipe_id: None,
                base_revision: None,
                document: wrong_schema,
            },
        )
        .await,
        ProcessSummaryOutcome::InvalidRequest
    );

    // A stored revision with an unsupported native schema is a typed state, not
    // a storage failure and not a generic invalid request.
    let unsupported_recipe = format!("unsupported-{suffix}");
    run!(
        "INSERT INTO recipes (workspace_id,recipe_id,name,document,native_head_revision) VALUES ($1,$2,'Unsupported','{}',1)",
        &workspace,
        &unsupported_recipe
    );
    run!(
        "INSERT INTO recipe_revisions (workspace_id,recipe_id,revision,schema_version,document,content_digest) VALUES ($1,$2,1,2,'{}','digest')",
        &workspace,
        &unsupported_recipe
    );
    assert_eq!(
        summary(
            &db,
            &actor,
            ProcessSummarySelection::Saved {
                recipe_id: unsupported_recipe,
                revision: 1,
            },
        )
        .await,
        ProcessSummaryOutcome::UnsupportedRevision { schema_version: 2 }
    );

    run!(
        "UPDATE auth_credentials SET revoked_at = $1 WHERE credential_id = $2",
        chrono::Utc::now().timestamp(),
        &credential
    );
    assert_eq!(
        summary(
            &db,
            &actor,
            ProcessSummarySelection::Draft {
                recipe_id: None,
                base_revision: None,
                document: draft.clone(),
            },
        )
        .await,
        ProcessSummaryOutcome::Forbidden
    );
    run!(
        "UPDATE auth_credentials SET revoked_at = NULL WHERE credential_id = $1",
        &credential
    );
    let expired = ActorContext {
        access_expires_at: chrono::Utc::now().timestamp() - 1,
        ..actor.clone()
    };
    assert_eq!(
        summary(
            &db,
            &expired,
            ProcessSummarySelection::Draft {
                recipe_id: None,
                base_revision: None,
                document: draft,
            },
        )
        .await,
        ProcessSummaryOutcome::Forbidden
    );
}

#[tokio::test]
async fn postgres_preview_rechecks_authority_after_profile_lock_wait() {
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
    create_equipment(&db, &actor, &profile_id, "Locked kettle").await;
    let draft = complete_draft(&profile_id);
    let RecipeSaveOutcome::Saved { recipe_id, .. } =
        save_recipe(&db, &actor, None, None, &draft).await
    else {
        panic!("lock-wait recipe fixture");
    };

    // Hold the profile aggregate lock so the second authorization in the shared
    // preview happens after the verified access window has elapsed.
    let mut lock = control.begin().await.unwrap();
    sqlx::query(
        "SELECT profile_id FROM equipment_profiles WHERE workspace_id=$1 AND profile_id=$2 FOR UPDATE",
    )
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
    let task_workspace = workspace.clone();
    let task_recipe = recipe_id.clone();
    let handle = tokio::spawn(async move {
        process_summary::read(
            &task_db,
            &task_actor,
            ProcessSummaryRequest {
                intent: OperationIntent {
                    account_id: task_actor.actor_id.clone(),
                    workspace_id: task_workspace,
                },
                selection: ProcessSummarySelection::Saved {
                    recipe_id: task_recipe,
                    revision: 1,
                },
            },
        )
        .await
    });
    tokio::time::sleep(Duration::from_secs(4)).await;
    lock.rollback().await.unwrap();
    assert_eq!(handle.await.unwrap(), ProcessSummaryOutcome::Forbidden);
}

#[tokio::test]
async fn sqlite_process_summary() {
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
async fn postgres_process_summary() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL")
        .expect("KRAUSEN_TEST_POSTGRES_URL is required for PostgreSQL tests");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
