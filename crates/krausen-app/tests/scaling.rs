// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{ActorContext, equipment, recipe_editor, recipe_library, scaling};
use krausen_core::DbPool;
use krausen_types::{
    brewery::*,
    equipment::{ArchiveEquipment, SaveEquipment},
    recipe_editor::{RecipeReadOutcome, RecipeSaveOutcome},
    recipe_library::{ArchiveRecipe, LibraryOutcome},
    request_context::OperationIntent,
    scaling::{
        DuplicateOutcome, DuplicateRecipe, ScaleApplyOutcome, ScaleApplyRequest, ScaleOutcome,
        ScalePreviewRequest, ScaleProblem, ScaleWarning,
    },
};

async fn run(db: &DbPool, sql: &str) {
    match db {
        DbPool::Postgres(pool) => {
            sqlx::query(sql).execute(pool).await.unwrap();
        }
        DbPool::Sqlite(pool) => {
            sqlx::query(sql).execute(pool).await.unwrap();
        }
    }
}

fn settings() -> EquipmentSettingsV1 {
    EquipmentSettingsV1 {
        mash_efficiency_fraction: Some(0.75),
        boil_off_l_per_hour: Some(3.0),
        transfer_loss_l: Some(2.0),
        pre_boil_deadspace_l: Some(0.0),
        grain_absorption_l_per_kg: Some(0.8),
        kettle_capacity_l: Some(30.0),
        ..Default::default()
    }
}

fn document(profile_id: Option<&str>) -> RecipeDocumentV1 {
    let mut document = RecipeDocumentV1 {
        title: "Scaled ale".into(),
        batch_size_l: Some(20.0),
        boil_minutes: Some(60.0),
        equipment_profile_id: profile_id.map(str::to_owned),
        process: ProcessV1 {
            apparent_attenuation_fraction: Some(0.8),
            mash_steps: vec![ProcessStep {
                row_id: "s1".into(),
                name: Some("Saccharification".into()),
                time_minutes: Some(45.0),
                temperature_c: Some(66.0),
                volume_l: None,
                extensions: Default::default(),
            }],
            mash_water_l: Some(12.0),
            mash_water_method: Some("manual".into()),
            kettle_top_up_l: Some(2.0),
            ..Default::default()
        },
        ..Default::default()
    };
    document.fermentables.push(FermentableAddition {
        row_id: "g1".into(),
        amount_kg: Some(4.0),
        addition: AdditionStage {
            stage: "mash".into(),
            time_minutes: Some(60.0),
        },
        specification: FermentableSpecification {
            name: Some("Pale malt".into()),
            producer: None,
            kind: Some("grain".into()),
            yield_fraction: Some(0.8),
            colour_ebc: Some(7.0),
        },
        source: None,
    });
    document.hops.push(HopAddition {
        row_id: "h1".into(),
        amount_g: Some(30.0),
        stage: "boil".into(),
        contact_minutes: Some(60.0),
        temperature_c: Some(100.0),
        specification: HopSpecification {
            name: Some("Cascade".into()),
            producer: None,
            alpha_fraction: Some(0.055),
        },
        source: None,
    });
    document.yeast.push(IngredientAddition {
        row_id: "y1".into(),
        amount: Some(1.0),
        unit: Some("pkg".into()),
        specification: IngredientSpecification {
            name: Some("House yeast".into()),
            producer: None,
            attenuation_fraction: Some(0.8),
            extensions: Default::default(),
        },
        source: None,
    });
    document
}

async fn create_recipe(
    db: &DbPool,
    actor: &ActorContext,
    intent: &OperationIntent,
    document: &RecipeDocumentV1,
) -> String {
    let RecipeSaveOutcome::Saved { recipe_id, .. } = recipe_editor::save(
        db,
        actor,
        intent,
        None,
        None,
        document,
        uuid::Uuid::new_v4(),
    )
    .await
    else {
        panic!("create recipe");
    };
    recipe_id
}

async fn exercise(db: DbPool) {
    let suffix = uuid::Uuid::new_v4();
    let user = format!("u-{suffix}");
    let a = format!("a-{suffix}");
    let b = format!("b-{suffix}");
    let credential = format!("c-{suffix}");
    macro_rules! run {
        ($sql:expr $(,$arg:expr)*) => {
            match &db {
                DbPool::Postgres(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
                DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
            }
        };
    }
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2)",
        &user,
        format!("{suffix}@example.test")
    );
    for workspace in [&a, &b] {
        run!(
            "INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ($1,$2)",
            workspace,
            &user
        );
        run!(
            "INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ($1,$2,'member')",
            workspace,
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
    let profile = equipment::save(
        &db,
        &actor,
        SaveEquipment {
            intent: intent.clone(),
            profile_id: uuid::Uuid::new_v4().to_string(),
            name: "20 L kettle".into(),
            settings: settings(),
            provenance: EquipmentProvenance {
                source: "test".into(),
                assumptions: vec!["test profile".into()],
            },
            expected_revision: 0,
            expected_metadata_revision: 0,
            request_id: uuid::Uuid::new_v4().to_string(),
        },
    )
    .await
    .unwrap();

    let recipe_id = create_recipe(&db, &actor, &intent, &document(Some(&profile.profile_id))).await;
    let preview = match scaling::preview(
        &db,
        &actor,
        ScalePreviewRequest {
            intent: intent.clone(),
            recipe_id: recipe_id.clone(),
            target_batch_size_l: 40.0,
        },
    )
    .await
    {
        ScaleOutcome::Preview(preview) => preview,
        other => panic!("scale preview: {other:?}"),
    };
    assert_eq!(preview.source_revision, 1);
    assert_eq!(preview.factor, 2.0);
    assert_eq!(preview.scaled_document.batch_size_l, Some(40.0));
    assert_eq!(preview.scaled_document.fermentables[0].amount_kg, Some(8.0));
    assert_eq!(preview.scaled_document.hops[0].amount_g, Some(60.0));
    assert_eq!(preview.scaled_document.process.mash_water_l, Some(24.0));
    assert_eq!(preview.scaled_document.process.kettle_top_up_l, Some(4.0));
    assert_eq!(preview.scaled_document.hops[0].contact_minutes, Some(60.0));
    assert_eq!(preview.scaled_document.yeast[0].amount, Some(1.0));
    assert!(
        preview
            .warnings
            .iter()
            .any(|warning| matches!(warning, ScaleWarning::KettleCapacityExceeded { .. }))
    );
    assert!(
        preview
            .warnings
            .iter()
            .any(|warning| matches!(warning, ScaleWarning::FixedLossesRetained { .. }))
    );
    assert_ne!(
        preview.source_targets.post_boil_l, preview.targets.post_boil_l,
        "fixed transfer loss makes the scaled post-boil target differ"
    );
    assert_ne!(
        preview.source_targets.original_sg, preview.targets.original_sg,
        "fixed losses change the scaled target rather than matching the source"
    );

    let request_id = uuid::Uuid::new_v4().to_string();
    let apply = ScaleApplyRequest {
        intent: intent.clone(),
        recipe_id: recipe_id.clone(),
        expected_revision: 1,
        target_batch_size_l: 40.0,
        accepted: preview.accepted.clone(),
        request_id: request_id.clone(),
    };
    assert_eq!(
        scaling::apply(&db, &actor, apply.clone()).await,
        ScaleApplyOutcome::Applied {
            recipe_id: recipe_id.clone(),
            revision: 2,
            source_revision: 1,
        }
    );
    let RecipeReadOutcome::Found(snapshot) =
        recipe_editor::read(&db, &actor, &intent, &recipe_id).await
    else {
        panic!("read scaled recipe");
    };
    assert_eq!(snapshot.revision, 2);
    assert_eq!(snapshot.document.batch_size_l, Some(40.0));
    assert_eq!(snapshot.document.fermentables[0].amount_kg, Some(8.0));
    assert_eq!(snapshot.document.hops[0].amount_g, Some(60.0));
    assert_eq!(
        snapshot.document.process.mash_steps[0].time_minutes,
        Some(45.0)
    );
    assert_eq!(
        snapshot.document.equipment_profile_id.as_deref(),
        Some(profile.profile_id.as_str())
    );

    assert_eq!(
        scaling::apply(&db, &actor, apply.clone()).await,
        ScaleApplyOutcome::Applied {
            recipe_id: recipe_id.clone(),
            revision: 2,
            source_revision: 1,
        },
        "identical retry recovers the original result"
    );
    let mut stale_head = apply.clone();
    stale_head.request_id = uuid::Uuid::new_v4().to_string();
    stale_head.expected_revision = 1;
    assert_eq!(
        scaling::apply(&db, &actor, stale_head).await,
        ScaleApplyOutcome::Conflict {
            expected: 1,
            actual: 2,
        }
    );
    let mut conflict_request = apply.clone();
    conflict_request.target_batch_size_l = 30.0;
    assert_eq!(
        scaling::apply(&db, &actor, conflict_request).await,
        ScaleApplyOutcome::RequestIdentityConflict
    );

    let unchanged = match scaling::preview(
        &db,
        &actor,
        ScalePreviewRequest {
            intent: intent.clone(),
            recipe_id: recipe_id.clone(),
            target_batch_size_l: 40.0,
        },
    )
    .await
    {
        ScaleOutcome::Preview(preview) => preview,
        other => panic!("preview unchanged: {other:?}"),
    };
    assert_eq!(unchanged.factor, 1.0);
    let accepted_at_head_two = unchanged.accepted.clone();
    assert_eq!(
        scaling::apply(
            &db,
            &actor,
            ScaleApplyRequest {
                intent: intent.clone(),
                recipe_id: recipe_id.clone(),
                expected_revision: 2,
                target_batch_size_l: 40.0,
                accepted: unchanged.accepted,
                request_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .await,
        ScaleApplyOutcome::Unchanged {
            recipe_id: recipe_id.clone(),
            revision: 2,
        }
    );

    // Same accepted digest, tampered component: head passes, context compare fails.
    let mut tampered = accepted_at_head_two;
    tampered.targets.original_sg = Some(1.123456);
    let tampered_outcome = scaling::apply(
        &db,
        &actor,
        ScaleApplyRequest {
            intent: intent.clone(),
            recipe_id: recipe_id.clone(),
            expected_revision: 2,
            target_batch_size_l: 40.0,
            accepted: tampered,
            request_id: uuid::Uuid::new_v4().to_string(),
        },
    )
    .await;
    match tampered_outcome {
        ScaleApplyOutcome::ContextConflict { fresh } => {
            assert_eq!(fresh.source_revision, 2);
            assert_eq!(fresh.factor, 1.0);
        }
        other => panic!("expected context conflict, got {other:?}"),
    }

    // Reproduce the editor-created document exactly: no equipment profile,
    // no hops, no kettle top-up and no process steps. This is the browser
    // fixture shape that exposed an apply context conflict.
    let mut browser_document = document(None);
    browser_document.hops.clear();
    browser_document.yeast.clear();
    browser_document.other_ingredients.clear();
    browser_document.process.mash_steps.clear();
    browser_document.process.kettle_top_up_l = None;
    browser_document.process.mash_water_l = Some(15.0);
    browser_document.equipment_overrides = EquipmentSettingsV1 {
        mash_efficiency_fraction: Some(0.75),
        boil_off_l_per_hour: Some(3.0),
        transfer_loss_l: Some(2.0),
        pre_boil_deadspace_l: Some(0.0),
        grain_absorption_l_per_kg: Some(0.8),
        kettle_capacity_l: Some(30.0),
        ..Default::default()
    };
    let browser_id = create_recipe(&db, &actor, &intent, &browser_document).await;
    let browser_preview = match scaling::preview(
        &db,
        &actor,
        ScalePreviewRequest {
            intent: intent.clone(),
            recipe_id: browser_id.clone(),
            target_batch_size_l: 40.0,
        },
    )
    .await
    {
        ScaleOutcome::Preview(preview) => preview,
        other => panic!("browser-shape preview: {other:?}"),
    };
    assert_eq!(
        browser_preview.validation.state,
        krausen_types::validation::Readiness::Ready
    );
    let browser_outcome = scaling::apply(
        &db,
        &actor,
        ScaleApplyRequest {
            intent: intent.clone(),
            recipe_id: browser_id.clone(),
            expected_revision: 1,
            target_batch_size_l: 40.0,
            accepted: browser_preview.accepted.clone(),
            request_id: uuid::Uuid::new_v4().to_string(),
        },
    )
    .await;
    match browser_outcome {
        ScaleApplyOutcome::Applied { revision: 2, .. } => {}
        ScaleApplyOutcome::ContextConflict { fresh } => {
            let accepted = &browser_preview.accepted;
            let current = &fresh.accepted;
            assert_eq!(accepted.context.draft, current.context.draft, "draft");
            assert_eq!(
                accepted.context.equipment_draft, current.context.equipment_draft,
                "equipment_draft"
            );
            assert_eq!(
                accepted.context.effective_equipment, current.context.effective_equipment,
                "effective_equipment"
            );
            assert_eq!(
                accepted.context.equipment_provenance, current.context.equipment_provenance,
                "equipment_provenance"
            );
            assert_eq!(
                accepted.context.equipment_profile, current.context.equipment_profile,
                "equipment_profile"
            );
            assert_eq!(
                accepted.context.assumptions, current.context.assumptions,
                "assumptions"
            );
            assert_eq!(
                accepted.context.engine_version, current.context.engine_version,
                "engine_version"
            );
            assert_eq!(
                accepted.context.engine_build, current.context.engine_build,
                "engine_build"
            );
            assert_eq!(
                accepted.context.engine_source_hash, current.context.engine_source_hash,
                "engine_source_hash"
            );
            assert_eq!(
                accepted.context.policy_source_hash, current.context.policy_source_hash,
                "policy_source_hash"
            );
            assert_eq!(accepted.context.style, current.context.style, "style");
            assert_eq!(accepted.targets, current.targets, "targets");
            panic!("server-produced accepted context equals fresh preview but apply rejected it");
        }
        other => panic!("browser-shape apply: {other:?}"),
    }

    let duplicate = scaling::duplicate(
        &db,
        &actor,
        DuplicateRecipe {
            intent: intent.clone(),
            source_recipe_id: recipe_id.clone(),
            expected_revision: 2,
            title: Some("Independent copy".into()),
            request_id: uuid::Uuid::new_v4().to_string(),
        },
    )
    .await;
    let DuplicateOutcome::Duplicated {
        recipe_id: duplicate_id,
        revision: 1,
        source_revision: 2,
    } = duplicate
    else {
        panic!("duplicate: {duplicate:?}");
    };
    assert_ne!(duplicate_id, recipe_id);
    let RecipeReadOutcome::Found(copy) =
        recipe_editor::read(&db, &actor, &intent, &duplicate_id).await
    else {
        panic!("read duplicate");
    };
    assert_eq!(copy.document.title, "Independent copy");
    assert_eq!(copy.document.batch_size_l, Some(40.0));
    assert_eq!(copy.document.fermentables[0].amount_kg, Some(8.0));
    assert_eq!(copy.document.import_provenance_id, None);

    assert_eq!(
        scaling::duplicate(
            &db,
            &actor,
            DuplicateRecipe {
                intent: intent.clone(),
                source_recipe_id: recipe_id.clone(),
                expected_revision: 1,
                title: None,
                request_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .await,
        DuplicateOutcome::Conflict {
            expected: 1,
            actual: 2,
        }
    );

    let other_actor = ActorContext {
        workspace_id: b.clone(),
        ..actor.clone()
    };
    let other_intent = OperationIntent {
        account_id: user.clone(),
        workspace_id: b.clone(),
    };
    assert_eq!(
        scaling::preview(
            &db,
            &other_actor,
            ScalePreviewRequest {
                intent: other_intent,
                recipe_id: recipe_id.clone(),
                target_batch_size_l: 40.0,
            },
        )
        .await,
        ScaleOutcome::NotFound,
        "another workspace cannot resolve the source recipe"
    );

    let mut missing_batch = document(Some(&profile.profile_id));
    missing_batch.batch_size_l = None;
    let missing_id = create_recipe(&db, &actor, &intent, &missing_batch).await;
    assert_eq!(
        scaling::preview(
            &db,
            &actor,
            ScalePreviewRequest {
                intent: intent.clone(),
                recipe_id: missing_id,
                target_batch_size_l: 40.0,
            },
        )
        .await,
        ScaleOutcome::Unscalable(ScaleProblem::MissingSourceVolume)
    );

    let mut unknown_style = document(Some(&profile.profile_id));
    unknown_style.style = Some(StyleReference {
        name: "Not a style".into(),
        edition: None,
        style_id: None,
    });
    let unknown_style_id = create_recipe(&db, &actor, &intent, &unknown_style).await;
    assert_eq!(
        scaling::preview(
            &db,
            &actor,
            ScalePreviewRequest {
                intent: intent.clone(),
                recipe_id: unknown_style_id,
                target_batch_size_l: 40.0,
            },
        )
        .await,
        ScaleOutcome::UnresolvedStyle
    );

    assert_eq!(
        recipe_library::archive(
            &db,
            &actor,
            ArchiveRecipe {
                intent: intent.clone(),
                recipe_id: recipe_id.clone(),
                expected_metadata_revision: 0,
                archived: true,
                request_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .await,
        LibraryOutcome::Archive {
            recipe_id: recipe_id.clone(),
            metadata_revision: 1,
            archived: true,
        }
    );
    assert_eq!(
        scaling::preview(
            &db,
            &actor,
            ScalePreviewRequest {
                intent: intent.clone(),
                recipe_id: recipe_id.clone(),
                target_batch_size_l: 40.0,
            },
        )
        .await,
        ScaleOutcome::Archived
    );
    assert_eq!(
        scaling::apply(
            &db,
            &actor,
            ScaleApplyRequest {
                intent: intent.clone(),
                recipe_id: recipe_id.clone(),
                expected_revision: 2,
                target_batch_size_l: 40.0,
                accepted: preview.accepted.clone(),
                request_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .await,
        ScaleApplyOutcome::Archived
    );
    assert_eq!(
        scaling::duplicate(
            &db,
            &actor,
            DuplicateRecipe {
                intent: intent.clone(),
                source_recipe_id: recipe_id.clone(),
                expected_revision: 2,
                title: None,
                request_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .await,
        DuplicateOutcome::Archived
    );

    let rollback_id =
        create_recipe(&db, &actor, &intent, &document(Some(&profile.profile_id))).await;
    let rollback_preview = match scaling::preview(
        &db,
        &actor,
        ScalePreviewRequest {
            intent: intent.clone(),
            recipe_id: rollback_id.clone(),
            target_batch_size_l: 40.0,
        },
    )
    .await
    {
        ScaleOutcome::Preview(preview) => preview,
        other => panic!("rollback preview: {other:?}"),
    };
    match &db {
        DbPool::Postgres(_) => {
            run(&db, "CREATE OR REPLACE FUNCTION kra18_reject_revision() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$").await;
            run(&db, "CREATE TRIGGER kra18_reject_revision BEFORE INSERT ON recipe_revisions FOR EACH ROW EXECUTE FUNCTION kra18_reject_revision()").await;
        }
        DbPool::Sqlite(_) => {
            run(&db, "CREATE TRIGGER kra18_reject_revision BEFORE INSERT ON recipe_revisions BEGIN SELECT RAISE(ABORT, 'injected'); END").await;
        }
    }
    let rollback_request = ScaleApplyRequest {
        intent: intent.clone(),
        recipe_id: rollback_id.clone(),
        expected_revision: 1,
        target_batch_size_l: 40.0,
        accepted: rollback_preview.accepted,
        request_id: uuid::Uuid::new_v4().to_string(),
    };
    let failed = scaling::apply(&db, &actor, rollback_request.clone()).await;
    assert!(matches!(
        failed,
        ScaleApplyOutcome::Unavailable | ScaleApplyOutcome::Retryable
    ));
    let RecipeReadOutcome::Found(unchanged_head) =
        recipe_editor::read(&db, &actor, &intent, &rollback_id).await
    else {
        panic!("read rollback recipe");
    };
    assert_eq!(unchanged_head.revision, 1);
    match &db {
        DbPool::Postgres(_) => {
            run(
                &db,
                "DROP TRIGGER kra18_reject_revision ON recipe_revisions",
            )
            .await;
            run(&db, "DROP FUNCTION kra18_reject_revision()").await;
        }
        DbPool::Sqlite(_) => {
            run(&db, "DROP TRIGGER kra18_reject_revision").await;
        }
    }
    assert!(matches!(
        scaling::apply(&db, &actor, rollback_request).await,
        ScaleApplyOutcome::Applied { revision: 2, .. }
    ));

    equipment::set_archived(
        &db,
        &actor,
        ArchiveEquipment {
            intent: intent.clone(),
            profile_id: profile.profile_id,
            archived: true,
            expected_metadata_revision: 1,
            request_id: uuid::Uuid::new_v4().to_string(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        scaling::preview(
            &db,
            &actor,
            ScalePreviewRequest {
                intent,
                recipe_id: rollback_id,
                target_batch_size_l: 40.0,
            },
        )
        .await,
        ScaleOutcome::UnresolvedProfile
    );
}

#[tokio::test]
async fn sqlite_scaling() {
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
async fn postgres_scaling() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
