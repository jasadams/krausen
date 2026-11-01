// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{
    ActorContext, brew_evidence, brew_history, brew_start, equipment, formulation, recipe_editor,
    recipe_library,
};
use krausen_core::DbPool;
use krausen_types::{
    brew::{
        ActualFormulationCommand, ActualFormulationOutcome, AmendActualFormulationRequest,
        ArchiveBrewRequest, BrewArchiveOutcome, BrewEvidenceViewOutcome, BrewEvidenceViewRequest,
        MeasurementCommand, MeasurementOutcome, RecordMeasurementRequest, StartBrewOutcome,
        StartBrewRequest,
    },
    brew_history::{
        BrewChangeKind, BrewComparisonOutcome, BrewComparisonRequest, BrewFeedbackApplyOutcome,
        BrewFeedbackApplyRequest, BrewFeedbackPreviewOutcome, BrewFeedbackPreviewRequest,
        BrewPrintOutcome, BrewPrintRequest, RecipeBrewHistoryOutcome, RecipeBrewHistoryQuery,
    },
    brewery::{
        ActualFormulationV1, AdditionStage, BrewPlanV1, EquipmentProvenance, EquipmentSettingsV1,
        FermentableAddition, FermentableSpecification, ProcessV1, RecipeDocumentV1, StyleReference,
    },
    equipment::SaveEquipment,
    formulation::{PreviewOutcome, PreviewRequest},
    recipe_editor::{RecipeReadOutcome, RecipeSaveOutcome},
    recipe_library::{ArchiveRecipe, LibraryOutcome},
    request_context::OperationIntent,
};
use uuid::Uuid;

fn intent(actor: &ActorContext) -> OperationIntent {
    OperationIntent {
        account_id: actor.actor_id.clone(),
        workspace_id: actor.workspace_id.clone(),
    }
}

fn document(profile_id: &str, title: &str) -> RecipeDocumentV1 {
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

async fn fixture(db: &DbPool) -> (ActorContext, String, BrewPlanV1, RecipeDocumentV1) {
    let suffix = Uuid::new_v4().to_string();
    let user = format!("u-{suffix}");
    let workspace = format!("w-{suffix}");
    let credential = format!("c-{suffix}");
    macro_rules! run {
        ($sql:expr $(,$arg:expr)*) => { match db {
            DbPool::Postgres(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
            DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
        } };
    }
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2)",
        &user,
        format!("{suffix}@example.test")
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
    run!(
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)",
        &credential,
        &user,
        chrono::Utc::now().timestamp() + 3600
    );
    let actor = ActorContext {
        actor_id: user,
        workspace_id: workspace,
        credential_id: credential,
        access_expires_at: chrono::Utc::now().timestamp() + 3600,
    };
    let profile_id = Uuid::new_v4().to_string();
    equipment::save(
        db,
        &actor,
        SaveEquipment {
            intent: intent(&actor),
            profile_id: profile_id.clone(),
            name: "20 L kettle".into(),
            settings: EquipmentSettingsV1 {
                mash_efficiency_fraction: Some(0.75),
                boil_off_l_per_hour: Some(3.0),
                transfer_loss_l: Some(2.0),
                pre_boil_deadspace_l: Some(0.0),
                grain_absorption_l_per_kg: Some(0.8),
                mash_thickness_l_per_kg: Some(2.5),
                kettle_capacity_l: Some(30.0),
                ..Default::default()
            },
            provenance: EquipmentProvenance {
                source: "test".into(),
                assumptions: vec!["fixture".into()],
            },
            expected_revision: 0,
            expected_metadata_revision: 0,
            request_id: Uuid::new_v4().to_string(),
        },
    )
    .await
    .expect("equipment");
    let recipe = document(&profile_id, "History brew");
    let RecipeSaveOutcome::Saved { recipe_id, .. } = recipe_editor::save(
        db,
        &actor,
        &intent(&actor),
        None,
        None,
        &recipe,
        Uuid::new_v4(),
    )
    .await
    else {
        panic!("recipe");
    };
    let preview = formulation::preview(
        db,
        &actor,
        PreviewRequest {
            draft_generation: "brew-history".into(),
            intent: intent(&actor),
            draft: recipe.clone(),
            equipment_draft: Default::default(),
        },
    )
    .await
    .unwrap();
    let PreviewOutcome::Preview(preview) = preview else {
        panic!("preview")
    };
    let StartBrewOutcome::Started { brew_id, plan, .. } = brew_start::start(
        db,
        &actor,
        StartBrewRequest {
            intent: intent(&actor),
            recipe_id,
            selected_recipe_revision: 1,
            expected_head_revision: Some(1),
            expected_calculation_context: preview.context_digest.unwrap(),
            request_id: Uuid::new_v4().to_string(),
        },
    )
    .await
    else {
        panic!("start");
    };
    (actor, brew_id, plan, recipe)
}

fn measurement(
    actor: &ActorContext,
    brew_id: &str,
    quantity: &str,
    value: f64,
) -> RecordMeasurementRequest {
    RecordMeasurementRequest {
        intent: intent(actor),
        brew_id: brew_id.into(),
        quantity_id: quantity.into(),
        value,
        observed_at: None,
        source: "manual".into(),
        metadata: krausen_types::brewery::MeasurementMetadataV1 {
            reference_temperature_c: Some(20.0),
            ..Default::default()
        },
        request_id: Uuid::new_v4().to_string(),
    }
}

fn actual(doc: &RecipeDocumentV1) -> ActualFormulationV1 {
    let mut fermentables = doc.fermentables.clone();
    fermentables[0].amount_kg = Some(4.5);
    ActualFormulationV1 {
        schema_version: 1,
        boil_minutes: Some(70.0),
        fermentables,
        hops: doc.hops.clone(),
        yeast: doc.yeast.clone(),
        other_ingredients: doc.other_ingredients.clone(),
        process: doc.process.clone(),
        notes: Some("Actual brew notes".into()),
        extensions: Default::default(),
    }
}

async fn exercise(db: DbPool) {
    let (actor, brew_id, _plan, recipe) = fixture(&db).await;

    let view = brew_evidence::view(
        &db,
        &actor,
        BrewEvidenceViewRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            lifecycle_cursor: None,
        },
    )
    .await;
    let BrewEvidenceViewOutcome::View(view) = view else {
        panic!("view")
    };
    let recipe_id = view.recipe_id.clone();
    let stored_plan = view.plan.clone();
    let comparison = brew_history::comparison(
        &db,
        &actor,
        BrewComparisonRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
        },
    )
    .await;
    let BrewComparisonOutcome::Comparison(comparison) = comparison else {
        panic!("comparison")
    };
    assert!(comparison.changes.is_empty());
    assert!(
        comparison
            .measurements
            .iter()
            .any(|row| row.quantity_id == "gravity.original.v1" && row.target.is_some())
    );
    assert!(
        comparison
            .measurements
            .iter()
            .any(|row| row.quantity_id == "volume.packaged.v1" && !row.comparable)
    );

    let MeasurementOutcome::Recorded(_entry) = brew_evidence::measurements(
        &db,
        &actor,
        MeasurementCommand::Record(measurement(&actor, &brew_id, "gravity.original.v1", 1.050)),
    )
    .await
    else {
        panic!("measurement")
    };
    let comparison = brew_history::comparison(
        &db,
        &actor,
        BrewComparisonRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
        },
    )
    .await;
    let BrewComparisonOutcome::Comparison(comparison) = comparison else {
        panic!("comparison after measurement")
    };
    let slot = comparison
        .measurements
        .iter()
        .find(|row| row.quantity_id == "gravity.original.v1")
        .unwrap();
    assert_eq!(slot.selected.as_ref().unwrap().value, Some(1.050));
    assert!(slot.comparable);
    assert!(slot.delta.is_some());
    let hot = brew_evidence::measurements(
        &db,
        &actor,
        MeasurementCommand::Record(RecordMeasurementRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            quantity_id: "volume.post_boil.v1".into(),
            value: 22.0,
            observed_at: None,
            source: "manual".into(),
            metadata: krausen_types::brewery::MeasurementMetadataV1 {
                sample_temperature_c: Some(95.0),
                ..Default::default()
            },
            request_id: Uuid::new_v4().to_string(),
        }),
    )
    .await;
    assert!(matches!(hot, MeasurementOutcome::Recorded(_)));
    let comparison = brew_history::comparison(
        &db,
        &actor,
        BrewComparisonRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
        },
    )
    .await;
    let BrewComparisonOutcome::Comparison(comparison) = comparison else {
        panic!("hot comparison")
    };
    let hot_slot = comparison
        .measurements
        .iter()
        .find(|row| row.quantity_id == "volume.post_boil.v1")
        .unwrap();
    assert!(
        !hot_slot.comparable,
        "hot volume must not be a like-for-like delta"
    );
    assert_eq!(hot_slot.delta, None);
    assert_eq!(
        hot_slot.warning.as_deref(),
        Some("unlike_sample_temperature")
    );

    let doc = actual(&recipe);
    let ActualFormulationOutcome::Amended(actual_revision) = brew_evidence::actual(
        &db,
        &actor,
        ActualFormulationCommand::Amend(AmendActualFormulationRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            expected_actual_revision: None,
            document: doc,
            reason: Some("brew day".into()),
            request_id: Uuid::new_v4().to_string(),
        }),
    )
    .await
    else {
        panic!("amend")
    };
    assert_eq!(actual_revision.revision, 1);

    let comparison = brew_history::comparison(
        &db,
        &actor,
        BrewComparisonRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
        },
    )
    .await;
    let BrewComparisonOutcome::Comparison(comparison) = comparison else {
        panic!("comparison actual")
    };
    let ids: Vec<String> = comparison
        .changes
        .iter()
        .filter(|change| {
            change.kind == BrewChangeKind::Changed
                && (change.id == "fermentables:grain-1" || change.id == "process:boil_minutes")
        })
        .map(|change| change.id.clone())
        .collect();
    assert_eq!(
        ids.len(),
        2,
        "expected amount and boil changes: {:?}",
        comparison.changes
    );

    let preview = brew_history::feedback_preview(
        &db,
        &actor,
        BrewFeedbackPreviewRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            expected_actual_revision: Some(1),
            change_ids: ids.clone(),
        },
    )
    .await;
    let BrewFeedbackPreviewOutcome::Preview(preview) = preview else {
        panic!("feedback preview: {preview:?}")
    };
    let context = preview.preview.context_digest.clone().unwrap();
    let request = BrewFeedbackApplyRequest {
        intent: intent(&actor),
        brew_id: brew_id.clone(),
        expected_actual_revision: Some(1),
        change_ids: ids.clone(),
        expected_recipe_head: preview.base_recipe_head,
        expected_calculation_context: context.clone(),
        request_id: Uuid::new_v4().to_string(),
    };
    let conflict = brew_history::feedback_apply(
        &db,
        &actor,
        BrewFeedbackApplyRequest {
            expected_calculation_context: "stale".into(),
            request_id: Uuid::new_v4().to_string(),
            ..request.clone()
        },
    )
    .await;
    assert!(matches!(
        conflict,
        BrewFeedbackApplyOutcome::ContextConflict { .. }
    ));
    assert_eq!(
        brew_history::feedback_apply(&db, &actor, request.clone()).await,
        BrewFeedbackApplyOutcome::Applied {
            brew_id: brew_id.clone(),
            recipe_id: recipe_id.clone(),
            source_actual_revision: 1,
            recipe_revision: 2,
        }
    );
    assert_eq!(
        brew_history::feedback_apply(&db, &actor, request.clone()).await,
        BrewFeedbackApplyOutcome::Applied {
            brew_id: brew_id.clone(),
            recipe_id: recipe_id.clone(),
            source_actual_revision: 1,
            recipe_revision: 2,
        },
        "identical retry recovers the first result"
    );
    let RecipeReadOutcome::Found(updated) =
        recipe_editor::read(&db, &actor, &intent(&actor), &recipe_id).await
    else {
        panic!("read recipe")
    };
    assert_eq!(updated.revision, 2);
    assert_eq!(updated.document.fermentables[0].amount_kg, Some(4.5));
    assert_eq!(updated.document.boil_minutes, Some(70.0));
    let head_document = updated.document.clone();

    let stale = brew_history::feedback_apply(
        &db,
        &actor,
        BrewFeedbackApplyRequest {
            expected_recipe_head: 1,
            request_id: Uuid::new_v4().to_string(),
            ..request.clone()
        },
    )
    .await;
    assert!(matches!(
        stale,
        BrewFeedbackApplyOutcome::HeadConflict {
            expected: 1,
            actual: 2
        }
    ));

    let plan_view = brew_evidence::view(
        &db,
        &actor,
        BrewEvidenceViewRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            lifecycle_cursor: None,
        },
    )
    .await;
    let BrewEvidenceViewOutcome::View(plan_view) = plan_view else {
        panic!("plan view")
    };
    assert_eq!(
        plan_view.plan, stored_plan,
        "plan is immutable after feedback"
    );

    // Complete canonical deviations: an actual spec-only change and an
    // extension-only change must be visible and explicitly unapplied.
    let mut spec_actual = actual(&recipe);
    spec_actual.fermentables[0].amount_kg = Some(4.5);
    spec_actual.fermentables[0].specification.yield_fraction = Some(0.82);
    spec_actual
        .extensions
        .insert("fixture".into(), serde_json::json!(true));
    let ActualFormulationOutcome::Amended(spec_revision) = brew_evidence::actual(
        &db,
        &actor,
        ActualFormulationCommand::Amend(AmendActualFormulationRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            expected_actual_revision: Some(1),
            document: spec_actual,
            reason: Some("spec and extension evidence".into()),
            request_id: Uuid::new_v4().to_string(),
        }),
    )
    .await
    else {
        panic!("spec amendment")
    };
    assert_eq!(spec_revision.revision, 2);
    let comparison = brew_history::comparison(
        &db,
        &actor,
        BrewComparisonRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
        },
    )
    .await;
    let BrewComparisonOutcome::Comparison(comparison) = comparison else {
        panic!("spec comparison")
    };
    assert!(
        comparison
            .changes
            .iter()
            .any(|change| change.id == "fermentables:grain-1")
    );
    assert!(
        comparison
            .changes
            .iter()
            .any(|change| change.id == "extensions")
    );
    let spec_preview = brew_history::feedback_preview(
        &db,
        &actor,
        BrewFeedbackPreviewRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            expected_actual_revision: Some(2),
            change_ids: vec!["fermentables:grain-1".into()],
        },
    )
    .await;
    let BrewFeedbackPreviewOutcome::NoChanges { diagnostics } = spec_preview else {
        panic!("spec-only preview should be explicit no-change: {spec_preview:?}")
    };
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("specification preserved"))
    );
    let extension_preview = brew_history::feedback_preview(
        &db,
        &actor,
        BrewFeedbackPreviewRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            expected_actual_revision: Some(2),
            change_ids: vec!["extensions".into()],
        },
    )
    .await;
    let BrewFeedbackPreviewOutcome::NoChanges { diagnostics } = extension_preview else {
        panic!("extension-only preview should be explicit no-change: {extension_preview:?}")
    };
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("extensions"))
    );

    let print = brew_history::historical_print(
        &db,
        &actor,
        BrewPrintRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
        },
    )
    .await;
    let BrewPrintOutcome::Print(print) = print else {
        panic!("print")
    };
    assert_eq!(print.summary.title, "History brew");
    assert_eq!(print.summary.boil_minutes, Some(60.0));
    assert!(print.actual.is_some());
    assert!(
        print
            .measurements
            .iter()
            .any(|row| row.quantity_id == "gravity.original.v1")
    );

    // Two brews give a real keyset continuation; the cursor is bound to its
    // recipe parent, so reusing it for another recipe is rejected.
    let head_preview = formulation::preview(
        &db,
        &actor,
        PreviewRequest {
            draft_generation: "second-brew".into(),
            intent: intent(&actor),
            draft: head_document,
            equipment_draft: Default::default(),
        },
    )
    .await
    .unwrap();
    let PreviewOutcome::Preview(head_preview) = head_preview else {
        panic!("second brew preview")
    };
    let StartBrewOutcome::Started {
        brew_id: second_brew,
        ..
    } = brew_start::start(
        &db,
        &actor,
        StartBrewRequest {
            intent: intent(&actor),
            recipe_id: recipe_id.clone(),
            selected_recipe_revision: 2,
            expected_head_revision: Some(2),
            expected_calculation_context: head_preview.context_digest.unwrap(),
            request_id: Uuid::new_v4().to_string(),
        },
    )
    .await
    else {
        panic!("second brew")
    };
    let first_page = brew_history::history(
        &db,
        &actor,
        RecipeBrewHistoryQuery {
            intent: intent(&actor),
            recipe_id: recipe_id.clone(),
            include_archived: true,
            cursor: None,
            limit: 1,
        },
    )
    .await;
    let RecipeBrewHistoryOutcome::History(first_page) = first_page else {
        panic!("first page")
    };
    assert_eq!(first_page.items.len(), 1);
    let cursor = first_page.next_cursor.expect("continuation");
    let second_page = brew_history::history(
        &db,
        &actor,
        RecipeBrewHistoryQuery {
            intent: intent(&actor),
            recipe_id: recipe_id.clone(),
            include_archived: true,
            cursor: Some(cursor.clone()),
            limit: 1,
        },
    )
    .await;
    let RecipeBrewHistoryOutcome::History(second_page) = second_page else {
        panic!("second page")
    };
    assert_eq!(second_page.items.len(), 1);
    assert_ne!(second_page.items[0].brew_id, first_page.items[0].brew_id);
    let mut second_recipe = recipe.clone();
    second_recipe.title = "Other history recipe".into();
    let RecipeSaveOutcome::Saved {
        recipe_id: other_recipe,
        ..
    } = recipe_editor::save(
        &db,
        &actor,
        &intent(&actor),
        None,
        None,
        &second_recipe,
        Uuid::new_v4(),
    )
    .await
    else {
        panic!("second recipe")
    };
    assert_eq!(
        brew_history::history(
            &db,
            &actor,
            RecipeBrewHistoryQuery {
                intent: intent(&actor),
                recipe_id: other_recipe,
                include_archived: true,
                cursor: Some(cursor),
                limit: 1,
            },
        )
        .await,
        RecipeBrewHistoryOutcome::InvalidRequest,
        "a cursor must not cross parent recipes"
    );

    // Archive the recipe; immutable plan/history/print stay available.
    let archived = recipe_library::archive(
        &db,
        &actor,
        ArchiveRecipe {
            intent: intent(&actor),
            recipe_id: recipe_id.clone(),
            expected_metadata_revision: 0,
            archived: true,
            request_id: Uuid::new_v4().to_string(),
        },
    )
    .await;
    assert!(matches!(
        archived,
        LibraryOutcome::Archive { archived: true, .. }
    ));
    let archived_history = brew_history::history(
        &db,
        &actor,
        RecipeBrewHistoryQuery {
            intent: intent(&actor),
            recipe_id: recipe_id.clone(),
            include_archived: true,
            cursor: None,
            limit: 50,
        },
    )
    .await;
    let RecipeBrewHistoryOutcome::History(page) = archived_history else {
        panic!("archived history")
    };
    assert_eq!(page.items.len(), 2);
    assert!(page.items.iter().any(|item| item.brew_id == brew_id));
    assert_eq!(
        brew_history::historical_print(
            &db,
            &actor,
            BrewPrintRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
            },
        )
        .await,
        BrewPrintOutcome::Print(print)
    );

    // Archive the brew: explicit history includes it, ordinary filtering excludes it.
    let metadata = plan_view.metadata_revision;
    let archived_brew = brew_evidence::archive(
        &db,
        &actor,
        ArchiveBrewRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            archived: true,
            expected_metadata_revision: metadata,
            request_id: Uuid::new_v4().to_string(),
        },
    )
    .await;
    assert!(matches!(archived_brew, BrewArchiveOutcome::Archived { .. }));
    let active = brew_history::history(
        &db,
        &actor,
        RecipeBrewHistoryQuery {
            intent: intent(&actor),
            recipe_id: recipe_id.clone(),
            include_archived: false,
            cursor: None,
            limit: 50,
        },
    )
    .await;
    let RecipeBrewHistoryOutcome::History(active) = active else {
        panic!("active history")
    };
    assert_eq!(active.items.len(), 1);
    assert_eq!(active.items[0].brew_id, second_brew);

    let restored = recipe_library::archive(
        &db,
        &actor,
        ArchiveRecipe {
            intent: intent(&actor),
            recipe_id,
            expected_metadata_revision: 1,
            archived: false,
            request_id: Uuid::new_v4().to_string(),
        },
    )
    .await;
    assert!(matches!(
        restored,
        LibraryOutcome::Archive {
            archived: false,
            ..
        }
    ));
}

#[tokio::test]
async fn sqlite_brew_history() {
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
async fn postgres_brew_history() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("PostgreSQL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
