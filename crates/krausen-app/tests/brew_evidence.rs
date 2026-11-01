// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared brew evidence, selection, lifecycle and archive operations.
use krausen_app::{ActorContext, brew_evidence, brew_start, equipment, formulation, recipe_editor};
use krausen_core::DbPool;
use krausen_types::{
    brew::{
        ActualFormulationCommand, ActualFormulationHistoryRequest, ActualFormulationOutcome,
        ActualFormulationViewRequest, AmendActualFormulationRequest, ArchiveBrewRequest,
        BrewArchiveOutcome, BrewEvidenceCommand, BrewEvidenceOutcome, BrewEvidenceViewOutcome,
        BrewEvidenceViewRequest, BrewLifecycleOutcome, BrewTransitionV1, CorrectMeasurementRequest,
        CorrectNoteRequest, MeasurementCommand, MeasurementListRequest, MeasurementOutcome,
        NoteCommand, NoteListRequest, NoteOutcome, RecordMeasurementRequest, RecordNoteRequest,
        RetractMeasurementRequest, RetractNoteRequest, SelectMeasurementRequest,
        TransitionBrewRequest,
    },
    brewery::{
        AdditionStage, BrewLifecycleEvent, BrewPlanV1, BrewStatusV1, ChillingV1,
        EquipmentProvenance, EquipmentSettingsV1, FermentableAddition, FermentableSpecification,
        HopAddition, HopSpecification, ProcessV1, QUANTITIES, RecipeDocumentV1, StyleReference,
    },
    equipment::SaveEquipment,
    formulation::{Preview, PreviewOutcome, PreviewRequest},
    recipe_editor::RecipeSaveOutcome,
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

async fn preview_document(
    db: &DbPool,
    actor: &ActorContext,
    document: &RecipeDocumentV1,
) -> Preview {
    let outcome = formulation::preview(
        db,
        actor,
        PreviewRequest {
            draft_generation: "brew-evidence-test".into(),
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

async fn fixture(db: &DbPool) -> (ActorContext, String, BrewPlanV1) {
    let suffix = Uuid::new_v4().to_string();
    let user = format!("u-{suffix}");
    let workspace = format!("w-{suffix}");
    let credential = format!("c-{suffix}");
    let email = format!("{suffix}@example.test");
    macro_rules! run {
        ($sql:expr $(, $a:expr)*) => { match db {
            DbPool::Postgres(p) => sqlx::query($sql)$(.bind($a))*.execute(p).await.unwrap().rows_affected(),
            DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($a))*.execute(p).await.unwrap().rows_affected(),
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
    let mut profile = equipment_request(&profile_id);
    profile.intent = intent(&actor);
    profile.request_id = Uuid::new_v4().to_string();
    equipment::save(db, &actor, profile)
        .await
        .expect("equipment fixture");

    let document = complete_document(&profile_id, "Evidence brew");
    let RecipeSaveOutcome::Saved {
        recipe_id,
        revision,
    } = recipe_editor::save(
        db,
        &actor,
        &intent(&actor),
        None,
        None,
        &document,
        Uuid::new_v4(),
    )
    .await
    else {
        panic!("recipe fixture");
    };
    let preview = preview_document(db, &actor, &document).await;
    assert_eq!(preview.validation.state, Readiness::Ready);
    let digest = preview.context_digest.expect("context digest");
    let outcome = brew_start::start(
        db,
        &actor,
        krausen_types::brew::StartBrewRequest {
            intent: intent(&actor),
            recipe_id,
            selected_recipe_revision: revision,
            expected_head_revision: Some(revision),
            expected_calculation_context: digest,
            request_id: Uuid::new_v4().to_string(),
        },
    )
    .await;
    let krausen_types::brew::StartBrewOutcome::Started { brew_id, plan, .. } = outcome else {
        panic!("brew start fixture");
    };
    (actor, brew_id, plan)
}

fn record_request(
    actor: &ActorContext,
    brew_id: &str,
    quantity_id: &str,
    value: f64,
) -> RecordMeasurementRequest {
    RecordMeasurementRequest {
        intent: intent(actor),
        brew_id: brew_id.into(),
        quantity_id: quantity_id.into(),
        value,
        observed_at: None,
        source: "manual".into(),
        metadata: Default::default(),
        request_id: Uuid::new_v4().to_string(),
    }
}

fn correct_request(
    actor: &ActorContext,
    brew_id: &str,
    entry_id: &str,
    expected_selection_revision: Option<i64>,
    value: f64,
) -> CorrectMeasurementRequest {
    CorrectMeasurementRequest {
        intent: intent(actor),
        brew_id: brew_id.into(),
        entry_id: entry_id.into(),
        expected_selection_revision,
        value,
        source: "manual".into(),
        metadata: None,
        request_id: Uuid::new_v4().to_string(),
    }
}

fn retract_request(
    actor: &ActorContext,
    brew_id: &str,
    entry_id: &str,
    expected_selection_revision: Option<i64>,
) -> RetractMeasurementRequest {
    RetractMeasurementRequest {
        intent: intent(actor),
        brew_id: brew_id.into(),
        entry_id: entry_id.into(),
        expected_selection_revision,
        request_id: Uuid::new_v4().to_string(),
    }
}

fn select_request(
    actor: &ActorContext,
    brew_id: &str,
    root: Option<&str>,
    expected_selection_revision: i64,
) -> SelectMeasurementRequest {
    SelectMeasurementRequest {
        intent: intent(actor),
        brew_id: brew_id.into(),
        quantity_id: "gravity.original.v1".into(),
        root_observation_id: root.map(str::to_owned),
        expected_selection_revision,
        request_id: Uuid::new_v4().to_string(),
    }
}

async fn view(
    db: &DbPool,
    actor: &ActorContext,
    brew_id: &str,
) -> krausen_types::brew::BrewEvidenceView {
    let outcome = brew_evidence::view(
        db,
        actor,
        BrewEvidenceViewRequest {
            intent: intent(actor),
            brew_id: brew_id.into(),
            lifecycle_cursor: None,
        },
    )
    .await;
    let BrewEvidenceViewOutcome::View(view) = outcome else {
        panic!("view not available: {outcome:?}");
    };
    view
}

async fn record(
    db: &DbPool,
    actor: &ActorContext,
    request: RecordMeasurementRequest,
) -> MeasurementOutcome {
    brew_evidence::measurements(db, actor, MeasurementCommand::Record(request)).await
}

async fn amend_document(
    db: &DbPool,
    actor: &ActorContext,
    brew_id: &str,
    expected: Option<i64>,
    document: krausen_types::brewery::ActualFormulationV1,
) -> ActualFormulationOutcome {
    brew_evidence::actual(
        db,
        actor,
        ActualFormulationCommand::Amend(AmendActualFormulationRequest {
            intent: intent(actor),
            brew_id: brew_id.into(),
            expected_actual_revision: expected,
            document,
            reason: Some("focused validation".into()),
            request_id: Uuid::new_v4().to_string(),
        }),
    )
    .await
}

async fn exercise(db: DbPool) {
    let (actor, brew_id, plan) = fixture(&db).await;
    macro_rules! count {
        ($sql:expr $(, $a:expr)*) => { match &db {
            DbPool::Postgres(p) => sqlx::query_scalar::<_, i64>($sql)$(.bind($a))*.fetch_one(p).await.unwrap(),
            DbPool::Sqlite(p) => sqlx::query_scalar::<_, i64>($sql)$(.bind($a))*.fetch_one(p).await.unwrap(),
        } };
    }
    let original = view(&db, &actor, &brew_id).await;
    assert_eq!(original.measurements.len(), QUANTITIES.len());
    assert!(original.actual.is_none());
    assert!(original.notes.is_empty());
    assert_eq!(original.status, BrewStatusV1::Started);
    assert_eq!(original.revision, 1);
    assert_eq!(original.metadata_revision, 1);
    assert!(!original.archived);
    assert_eq!(original.plan, plan);

    // First reading selects its root atomically; a second reading does not
    // replace the primary.
    let first = record(
        &db,
        &actor,
        record_request(&actor, &brew_id, "gravity.original.v1", 1.050),
    )
    .await;
    let MeasurementOutcome::Recorded(first) = first else {
        panic!("first reading: {first:?}");
    };
    let after_first = view(&db, &actor, &brew_id).await;
    let slot = after_first
        .measurements
        .iter()
        .find(|slot| slot.quantity_id == "gravity.original.v1")
        .unwrap();
    assert_eq!(
        slot.selected_root_observation_id.as_deref(),
        Some(first.entry_id.as_str())
    );
    assert_eq!(slot.current.as_ref().unwrap().value, Some(1.050));

    let second = record(
        &db,
        &actor,
        record_request(&actor, &brew_id, "gravity.original.v1", 1.052),
    )
    .await;
    let MeasurementOutcome::Recorded(second) = second else {
        panic!("second reading: {second:?}");
    };
    let after_second = view(&db, &actor, &brew_id).await;
    let slot = after_second
        .measurements
        .iter()
        .find(|slot| slot.quantity_id == "gravity.original.v1")
        .unwrap();
    assert_eq!(
        slot.selected_root_observation_id.as_deref(),
        Some(first.entry_id.as_str())
    );
    assert_eq!(slot.current.as_ref().unwrap().value, Some(1.050));

    let selected = brew_evidence::measurements(
        &db,
        &actor,
        MeasurementCommand::Select(select_request(&actor, &brew_id, Some(&second.entry_id), 1)),
    )
    .await;
    let MeasurementOutcome::Selected(selected) = selected else {
        panic!("select second: {selected:?}");
    };
    assert_eq!(selected.revision, 2);
    assert_eq!(
        selected.selected_root_observation_id.as_deref(),
        Some(second.entry_id.as_str())
    );
    let after_select = view(&db, &actor, &brew_id).await;
    let slot = after_select
        .measurements
        .iter()
        .find(|slot| slot.quantity_id == "gravity.original.v1")
        .unwrap();
    assert_eq!(slot.current.as_ref().unwrap().value, Some(1.052));

    // Correcting the selected root requires the expected selection revision and
    // keeps the same observation root; the view resolves the successor.
    let corrected = brew_evidence::measurements(
        &db,
        &actor,
        MeasurementCommand::Correct(correct_request(
            &actor,
            &brew_id,
            &second.entry_id,
            Some(2),
            1.054,
        )),
    )
    .await;
    let MeasurementOutcome::Corrected(corrected) = corrected else {
        panic!("correct selected: {corrected:?}");
    };
    assert_eq!(corrected.root_observation_id, second.entry_id);
    assert_eq!(corrected.observed_at, second.observed_at);
    let after_correct = view(&db, &actor, &brew_id).await;
    let slot = after_correct
        .measurements
        .iter()
        .find(|slot| slot.quantity_id == "gravity.original.v1")
        .unwrap();
    assert_eq!(
        slot.selected_root_observation_id.as_deref(),
        Some(second.entry_id.as_str())
    );
    assert_eq!(slot.current.as_ref().unwrap().value, Some(1.054));

    // A stale selection revision is a typed conflict and writes nothing.
    let stale = brew_evidence::measurements(
        &db,
        &actor,
        MeasurementCommand::Correct(correct_request(
            &actor,
            &brew_id,
            &corrected.entry_id,
            Some(2),
            1.056,
        )),
    )
    .await;
    assert_eq!(
        stale,
        MeasurementOutcome::Conflict {
            expected: 2,
            actual: 3
        }
    );
    // Correcting an already superseded entry is an entry conflict.
    let superseded = brew_evidence::measurements(
        &db,
        &actor,
        MeasurementCommand::Correct(correct_request(
            &actor,
            &brew_id,
            &second.entry_id,
            None,
            1.057,
        )),
    )
    .await;
    assert_eq!(
        superseded,
        MeasurementOutcome::EntryConflict {
            expected: second.entry_id.clone(),
            actual: corrected.entry_id.clone()
        }
    );
    // A non-selected root can be corrected without touching selection.
    let non_selected = brew_evidence::measurements(
        &db,
        &actor,
        MeasurementCommand::Correct(correct_request(
            &actor,
            &brew_id,
            &first.entry_id,
            None,
            1.049,
        )),
    )
    .await;
    let MeasurementOutcome::Corrected(_) = non_selected else {
        panic!("correct first: {non_selected:?}");
    };
    let after_non_selected = view(&db, &actor, &brew_id).await;
    let slot = after_non_selected
        .measurements
        .iter()
        .find(|slot| slot.quantity_id == "gravity.original.v1")
        .unwrap();
    assert_eq!(slot.current.as_ref().unwrap().value, Some(1.054));

    // Retracting the selected current entry appends a value-less entry and
    // clears selection atomically; the lineage stays readable.
    let retracted = brew_evidence::measurements(
        &db,
        &actor,
        MeasurementCommand::Retract(retract_request(
            &actor,
            &brew_id,
            &corrected.entry_id,
            Some(3),
        )),
    )
    .await;
    let MeasurementOutcome::Retracted(retracted) = retracted else {
        panic!("retract selected: {retracted:?}");
    };
    assert!(retracted.is_retraction());
    let after_retract = view(&db, &actor, &brew_id).await;
    let slot = after_retract
        .measurements
        .iter()
        .find(|slot| slot.quantity_id == "gravity.original.v1")
        .unwrap();
    assert_eq!(slot.selected_root_observation_id, None);
    assert!(slot.current.is_none());

    let lineage = brew_evidence::measurements(
        &db,
        &actor,
        MeasurementCommand::List(MeasurementListRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            root_observation_id: Some(second.entry_id.clone()),
            cursor: None,
            limit: 1,
        }),
    )
    .await;
    let MeasurementOutcome::Page(page) = lineage else {
        panic!("lineage page: {lineage:?}");
    };
    assert_eq!(page.items.len(), 1);
    assert!(page.next_cursor.is_some());
    let foreign_lineage_cursor = page.next_cursor.clone();
    let mut lineage_ids = page
        .items
        .iter()
        .map(|entry| entry.entry_id.clone())
        .collect::<Vec<_>>();
    let mut cursor = page.next_cursor;
    while let Some(next) = cursor {
        let outcome = brew_evidence::measurements(
            &db,
            &actor,
            MeasurementCommand::List(MeasurementListRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                root_observation_id: Some(second.entry_id.clone()),
                cursor: Some(next),
                limit: 1,
            }),
        )
        .await;
        let MeasurementOutcome::Page(page) = outcome else {
            panic!("lineage continuation: {outcome:?}");
        };
        lineage_ids.extend(page.items.iter().map(|entry| entry.entry_id.clone()));
        cursor = page.next_cursor;
    }
    lineage_ids.sort();
    lineage_ids.dedup();
    assert_eq!(lineage_ids.len(), 3);
    // A continuation is bound to workspace, brew, operation, order and filter;
    // reusing it for another root is invalid rather than silently skipping.
    assert_eq!(
        brew_evidence::measurements(
            &db,
            &actor,
            MeasurementCommand::List(MeasurementListRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                root_observation_id: Some(first.entry_id.clone()),
                cursor: foreign_lineage_cursor.clone(),
                limit: 1,
            }),
        )
        .await,
        MeasurementOutcome::Invalid
    );
    // Public pages are 1..=100; the default is 50 and over-bound values are
    // rejected instead of silently clamped.
    assert_eq!(
        brew_evidence::measurements(
            &db,
            &actor,
            MeasurementCommand::List(MeasurementListRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                root_observation_id: None,
                cursor: None,
                limit: 101,
            }),
        )
        .await,
        MeasurementOutcome::Invalid
    );
    assert_eq!(
        brew_evidence::measurements(
            &db,
            &actor,
            MeasurementCommand::List(MeasurementListRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                root_observation_id: None,
                cursor: None,
                limit: 0,
            }),
        )
        .await,
        MeasurementOutcome::Invalid
    );
    assert_eq!(
        brew_evidence::measurements(
            &db,
            &actor,
            MeasurementCommand::Select(
                select_request(&actor, &brew_id, Some(&second.entry_id), 4,)
            ),
        )
        .await,
        MeasurementOutcome::Invalid
    );

    // A third reading never auto-selects after the slot was cleared; explicit
    // selection and clearing remain revision-checked.
    let third = record(
        &db,
        &actor,
        record_request(&actor, &brew_id, "gravity.original.v1", 1.060),
    )
    .await;
    let MeasurementOutcome::Recorded(third) = third else {
        panic!("third reading: {third:?}");
    };
    let after_third = view(&db, &actor, &brew_id).await;
    let slot = after_third
        .measurements
        .iter()
        .find(|slot| slot.quantity_id == "gravity.original.v1")
        .unwrap();
    assert_eq!(slot.selected_root_observation_id, None);
    let selected = brew_evidence::measurements(
        &db,
        &actor,
        MeasurementCommand::Select(select_request(&actor, &brew_id, Some(&third.entry_id), 4)),
    )
    .await;
    let MeasurementOutcome::Selected(selected) = selected else {
        panic!("select third: {selected:?}");
    };
    assert_eq!(selected.revision, 5);
    let cleared = brew_evidence::measurements(
        &db,
        &actor,
        MeasurementCommand::Select(select_request(&actor, &brew_id, None, 5)),
    )
    .await;
    let MeasurementOutcome::Selected(cleared) = cleared else {
        panic!("clear selection: {cleared:?}");
    };
    assert_eq!(cleared.revision, 6);
    assert_eq!(cleared.selected_root_observation_id, None);
    let selected = brew_evidence::measurements(
        &db,
        &actor,
        MeasurementCommand::Select(select_request(&actor, &brew_id, Some(&third.entry_id), 6)),
    )
    .await;
    let MeasurementOutcome::Selected(selected) = selected else {
        panic!("reselect third: {selected:?}");
    };
    assert_eq!(selected.revision, 7);

    // Notes are append-only revisions; retraction retains earlier text.
    let note = brew_evidence::notes(
        &db,
        &actor,
        NoteCommand::Record(RecordNoteRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            text: "first note".into(),
            request_id: Uuid::new_v4().to_string(),
        }),
    )
    .await;
    let NoteOutcome::Recorded(note) = note else {
        panic!("record note: {note:?}");
    };
    assert_eq!(note.document.text.as_deref(), Some("first note"));
    let corrected_note = brew_evidence::notes(
        &db,
        &actor,
        NoteCommand::Correct(CorrectNoteRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            note_id: note.note_id.clone(),
            expected_revision: 1,
            text: "second note".into(),
            reason: Some("clarify".into()),
            request_id: Uuid::new_v4().to_string(),
        }),
    )
    .await;
    let NoteOutcome::Corrected(corrected_note) = corrected_note else {
        panic!("correct note: {corrected_note:?}");
    };
    assert_eq!(corrected_note.revision, 2);
    assert_eq!(
        brew_evidence::notes(
            &db,
            &actor,
            NoteCommand::Correct(CorrectNoteRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                note_id: note.note_id.clone(),
                expected_revision: 1,
                text: "stale".into(),
                reason: None,
                request_id: Uuid::new_v4().to_string(),
            }),
        )
        .await,
        NoteOutcome::Conflict {
            expected: 1,
            actual: 2
        }
    );
    let retracted_note = brew_evidence::notes(
        &db,
        &actor,
        NoteCommand::Retract(RetractNoteRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            note_id: note.note_id.clone(),
            expected_revision: 2,
            reason: Some("wrong brew".into()),
            request_id: Uuid::new_v4().to_string(),
        }),
    )
    .await;
    let NoteOutcome::Retracted(retracted_note) = retracted_note else {
        panic!("retract note: {retracted_note:?}");
    };
    assert!(retracted_note.document.text.is_none());
    let history = brew_evidence::notes(
        &db,
        &actor,
        NoteCommand::List(NoteListRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            note_id: Some(note.note_id.clone()),
            cursor: None,
            limit: 50,
        }),
    )
    .await;
    let NoteOutcome::History(history) = history else {
        panic!("note history: {history:?}");
    };
    assert_eq!(history.items.len(), 3);
    assert_eq!(
        history.items[0].document.text.as_deref(),
        Some("first note")
    );
    assert_eq!(
        history.items[1].document.text.as_deref(),
        Some("second note")
    );
    assert!(history.items[2].document.text.is_none());

    // Note history continuations are note-bound; a continuation from another
    // note or another operation family is invalid.
    let first_note_history = brew_evidence::notes(
        &db,
        &actor,
        NoteCommand::List(NoteListRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            note_id: Some(note.note_id.clone()),
            cursor: None,
            limit: 1,
        }),
    )
    .await;
    let NoteOutcome::History(first_note_history) = first_note_history else {
        panic!("first note history: {first_note_history:?}");
    };
    let foreign_note_cursor = first_note_history.next_cursor.clone().unwrap();
    let other_note = brew_evidence::notes(
        &db,
        &actor,
        NoteCommand::Record(RecordNoteRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            text: "other note".into(),
            request_id: Uuid::new_v4().to_string(),
        }),
    )
    .await;
    let NoteOutcome::Recorded(other_note) = other_note else {
        panic!("other note: {other_note:?}");
    };
    assert_eq!(
        brew_evidence::notes(
            &db,
            &actor,
            NoteCommand::List(NoteListRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                note_id: Some(other_note.note_id.clone()),
                cursor: Some(foreign_note_cursor),
                limit: 1,
            }),
        )
        .await,
        NoteOutcome::Invalid
    );
    assert_eq!(
        brew_evidence::notes(
            &db,
            &actor,
            NoteCommand::List(NoteListRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                note_id: None,
                cursor: foreign_lineage_cursor.clone(),
                limit: 1,
            }),
        )
        .await,
        NoteOutcome::Invalid
    );
    assert_eq!(
        brew_evidence::notes(
            &db,
            &actor,
            NoteCommand::List(NoteListRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                note_id: None,
                cursor: None,
                limit: 101,
            }),
        )
        .await,
        NoteOutcome::Invalid
    );

    // Bounded note pagination follows an opaque cursor without duplicates.
    for index in 0..3 {
        let outcome = brew_evidence::notes(
            &db,
            &actor,
            NoteCommand::Record(RecordNoteRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                text: format!("paged note {index}"),
                request_id: Uuid::new_v4().to_string(),
            }),
        )
        .await;
        assert!(matches!(outcome, NoteOutcome::Recorded(_)));
    }
    let mut note_ids = Vec::new();
    let mut cursor = None;
    loop {
        let outcome = brew_evidence::notes(
            &db,
            &actor,
            NoteCommand::List(NoteListRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                note_id: None,
                cursor,
                limit: 1,
            }),
        )
        .await;
        let NoteOutcome::Page(page) = outcome else {
            panic!("note page: {outcome:?}");
        };
        note_ids.extend(page.items.iter().map(|item| item.note.note_id.clone()));
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    note_ids.sort();
    note_ids.dedup();
    assert_eq!(note_ids.len(), 5);

    // Actual formulation starts as the frozen plan projection and later
    // amendments are complete revision-checked documents.
    let effective = brew_evidence::actual(
        &db,
        &actor,
        ActualFormulationCommand::Get(ActualFormulationViewRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
        }),
    )
    .await;
    let ActualFormulationOutcome::View(effective) = effective else {
        panic!("actual view: {effective:?}");
    };
    assert_eq!(effective.source, "plan");
    assert_eq!(effective.revision, None);
    assert_eq!(effective.document.boil_minutes, Some(60.0));
    assert_eq!(effective.document.fermentables, plan.recipe.fermentables);

    // Physically invalid native values and duplicate row identities are
    // rejected before the transaction, so no actual/request/change writes
    // survive a rejected amendment.
    let actuals_before = count!(
        "SELECT COUNT(*) FROM brew_actual_formulations WHERE workspace_id = $1 AND brew_id = $2",
        &actor.workspace_id,
        &brew_id
    );
    let requests_before = count!(
        "SELECT COUNT(*) FROM app_requests WHERE workspace_id = $1 AND operation = 'brew.actual.amend.v1'",
        &actor.workspace_id
    );
    let changes_before = count!(
        "SELECT COUNT(*) FROM app_changes WHERE workspace_id = $1 AND operation = 'brew.actual.amend.v1'",
        &actor.workspace_id
    );
    let mut negative_yield = effective.document.clone();
    negative_yield.fermentables[0].specification.yield_fraction = Some(-0.1);
    assert_eq!(
        amend_document(&db, &actor, &brew_id, None, negative_yield).await,
        ActualFormulationOutcome::Invalid
    );
    let mut negative_colour = effective.document.clone();
    negative_colour.fermentables[0].specification.colour_ebc = Some(-5.0);
    assert_eq!(
        amend_document(&db, &actor, &brew_id, None, negative_colour).await,
        ActualFormulationOutcome::Invalid
    );
    let mut invalid_alpha = effective.document.clone();
    invalid_alpha.hops.push(HopAddition {
        row_id: "hop-1".into(),
        amount_g: Some(10.0),
        stage: "boil".into(),
        contact_minutes: Some(60.0),
        temperature_c: None,
        specification: HopSpecification {
            name: Some("Test hop".into()),
            producer: None,
            alpha_fraction: Some(2.0),
        },
        source: None,
    });
    assert_eq!(
        amend_document(&db, &actor, &brew_id, None, invalid_alpha).await,
        ActualFormulationOutcome::Invalid
    );
    let mut invalid_attenuation = effective.document.clone();
    invalid_attenuation.process.apparent_attenuation_fraction = Some(1.2);
    assert_eq!(
        amend_document(&db, &actor, &brew_id, None, invalid_attenuation).await,
        ActualFormulationOutcome::Invalid
    );
    let mut duplicate_rows = effective.document.clone();
    let mut duplicate = duplicate_rows.fermentables[0].clone();
    duplicate.row_id = duplicate_rows.fermentables[0].row_id.clone();
    duplicate_rows.fermentables.push(duplicate);
    assert_eq!(
        amend_document(&db, &actor, &brew_id, None, duplicate_rows).await,
        ActualFormulationOutcome::Invalid
    );
    assert_eq!(
        count!(
            "SELECT COUNT(*) FROM brew_actual_formulations WHERE workspace_id = $1 AND brew_id = $2",
            &actor.workspace_id,
            &brew_id
        ),
        actuals_before
    );
    assert_eq!(
        count!(
            "SELECT COUNT(*) FROM app_requests WHERE workspace_id = $1 AND operation = 'brew.actual.amend.v1'",
            &actor.workspace_id
        ),
        requests_before
    );
    assert_eq!(
        count!(
            "SELECT COUNT(*) FROM app_changes WHERE workspace_id = $1 AND operation = 'brew.actual.amend.v1'",
            &actor.workspace_id
        ),
        changes_before
    );

    let mut changed = effective.document.clone();
    changed.boil_minutes = Some(75.0);
    let amended = brew_evidence::actual(
        &db,
        &actor,
        ActualFormulationCommand::Amend(AmendActualFormulationRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            expected_actual_revision: None,
            document: changed.clone(),
            reason: Some("boiled longer".into()),
            request_id: Uuid::new_v4().to_string(),
        }),
    )
    .await;
    let ActualFormulationOutcome::Amended(amended) = amended else {
        panic!("amend actual: {amended:?}");
    };
    assert_eq!(amended.revision, 1);
    assert_eq!(amended.predecessor_revision, None);
    let effective = brew_evidence::actual(
        &db,
        &actor,
        ActualFormulationCommand::Get(ActualFormulationViewRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
        }),
    )
    .await;
    let ActualFormulationOutcome::View(effective) = effective else {
        panic!("actual view after amend: {effective:?}");
    };
    assert_eq!(effective.source, "actual");
    assert_eq!(effective.revision, Some(1));
    assert_eq!(effective.document.boil_minutes, Some(75.0));
    assert_eq!(
        view(&db, &actor, &brew_id).await.plan.recipe.boil_minutes,
        Some(60.0)
    );
    assert_eq!(
        brew_evidence::actual(
            &db,
            &actor,
            ActualFormulationCommand::Amend(AmendActualFormulationRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                expected_actual_revision: None,
                document: changed.clone(),
                reason: None,
                request_id: Uuid::new_v4().to_string(),
            }),
        )
        .await,
        ActualFormulationOutcome::Conflict {
            expected: None,
            actual: Some(1)
        }
    );
    let mut second_change = changed.clone();
    second_change.boil_minutes = Some(80.0);
    let amended = brew_evidence::actual(
        &db,
        &actor,
        ActualFormulationCommand::Amend(AmendActualFormulationRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            expected_actual_revision: Some(1),
            document: second_change,
            reason: None,
            request_id: Uuid::new_v4().to_string(),
        }),
    )
    .await;
    let ActualFormulationOutcome::Amended(amended) = amended else {
        panic!("second amend: {amended:?}");
    };
    assert_eq!(amended.revision, 2);
    assert_eq!(amended.predecessor_revision, Some(1));
    let history = brew_evidence::actual(
        &db,
        &actor,
        ActualFormulationCommand::History(ActualFormulationHistoryRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            cursor: None,
            limit: 50,
        }),
    )
    .await;
    let ActualFormulationOutcome::History(history) = history else {
        panic!("actual history: {history:?}");
    };
    assert_eq!(history.items.len(), 2);
    assert_eq!(history.items[0].revision, 1);
    assert_eq!(history.items[1].revision, 2);

    // Lifecycle: terminal transitions need no measurements, reopening and
    // terminal correction require a reason, and stale revisions conflict.
    let completed = brew_evidence::transition(
        &db,
        &actor,
        TransitionBrewRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            expected_revision: 1,
            transition: BrewTransitionV1::Complete,
            reason: None,
            request_id: Uuid::new_v4().to_string(),
        },
    )
    .await;
    assert_eq!(
        completed,
        BrewLifecycleOutcome::Transitioned {
            brew_id: brew_id.clone(),
            status: BrewStatusV1::Completed,
            revision: 2
        }
    );
    assert_eq!(
        brew_evidence::transition(
            &db,
            &actor,
            TransitionBrewRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                expected_revision: 2,
                transition: BrewTransitionV1::Reopen,
                reason: None,
                request_id: Uuid::new_v4().to_string(),
            },
        )
        .await,
        BrewLifecycleOutcome::Invalid
    );
    let reopened = brew_evidence::transition(
        &db,
        &actor,
        TransitionBrewRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            expected_revision: 2,
            transition: BrewTransitionV1::Reopen,
            reason: Some("needs more time".into()),
            request_id: Uuid::new_v4().to_string(),
        },
    )
    .await;
    assert_eq!(
        reopened,
        BrewLifecycleOutcome::Transitioned {
            brew_id: brew_id.clone(),
            status: BrewStatusV1::Started,
            revision: 3
        }
    );
    assert_eq!(
        brew_evidence::transition(
            &db,
            &actor,
            TransitionBrewRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                expected_revision: 2,
                transition: BrewTransitionV1::Complete,
                reason: None,
                request_id: Uuid::new_v4().to_string(),
            },
        )
        .await,
        BrewLifecycleOutcome::Conflict {
            expected: 2,
            actual: 3
        }
    );
    let cancelled = brew_evidence::transition(
        &db,
        &actor,
        TransitionBrewRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            expected_revision: 3,
            transition: BrewTransitionV1::Cancel,
            reason: None,
            request_id: Uuid::new_v4().to_string(),
        },
    )
    .await;
    assert_eq!(
        cancelled,
        BrewLifecycleOutcome::Transitioned {
            brew_id: brew_id.clone(),
            status: BrewStatusV1::Cancelled,
            revision: 4
        }
    );
    assert_eq!(
        brew_evidence::transition(
            &db,
            &actor,
            TransitionBrewRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                expected_revision: 4,
                transition: BrewTransitionV1::Complete,
                reason: None,
                request_id: Uuid::new_v4().to_string(),
            },
        )
        .await,
        BrewLifecycleOutcome::Invalid
    );
    let corrected_terminal = brew_evidence::transition(
        &db,
        &actor,
        TransitionBrewRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            expected_revision: 4,
            transition: BrewTransitionV1::Complete,
            reason: Some("terminal status correction".into()),
            request_id: Uuid::new_v4().to_string(),
        },
    )
    .await;
    assert_eq!(
        corrected_terminal,
        BrewLifecycleOutcome::Transitioned {
            brew_id: brew_id.clone(),
            status: BrewStatusV1::Completed,
            revision: 5
        }
    );
    assert_eq!(
        brew_evidence::transition(
            &db,
            &actor,
            TransitionBrewRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                expected_revision: 5,
                transition: BrewTransitionV1::Complete,
                reason: None,
                request_id: Uuid::new_v4().to_string(),
            },
        )
        .await,
        BrewLifecycleOutcome::InvalidTransition {
            from: BrewStatusV1::Completed,
            to: BrewStatusV1::Completed
        }
    );

    // Archive/unarchive is metadata-revision guarded and never prunes evidence.
    let archived = brew_evidence::archive(
        &db,
        &actor,
        ArchiveBrewRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            archived: true,
            expected_metadata_revision: 1,
            request_id: Uuid::new_v4().to_string(),
        },
    )
    .await;
    assert_eq!(
        archived,
        BrewArchiveOutcome::Archived {
            brew_id: brew_id.clone(),
            archived: true,
            metadata_revision: 2
        }
    );
    let archived_view = view(&db, &actor, &brew_id).await;
    assert!(archived_view.archived);
    assert_eq!(archived_view.metadata_revision, 2);
    assert_eq!(archived_view.measurements.len(), QUANTITIES.len());
    assert!(!archived_view.notes.is_empty());
    assert!(archived_view.actual.is_some());
    assert_eq!(
        record(
            &db,
            &actor,
            record_request(&actor, &brew_id, "gravity.final.v1", 1.010),
        )
        .await,
        MeasurementOutcome::Archived
    );
    let unarchived = brew_evidence::archive(
        &db,
        &actor,
        ArchiveBrewRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            archived: false,
            expected_metadata_revision: 2,
            request_id: Uuid::new_v4().to_string(),
        },
    )
    .await;
    assert_eq!(
        unarchived,
        BrewArchiveOutcome::Archived {
            brew_id: brew_id.clone(),
            archived: false,
            metadata_revision: 3
        }
    );
    assert_eq!(
        brew_evidence::archive(
            &db,
            &actor,
            ArchiveBrewRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                archived: true,
                expected_metadata_revision: 1,
                request_id: Uuid::new_v4().to_string(),
            },
        )
        .await,
        BrewArchiveOutcome::Conflict {
            expected: 1,
            actual: 3
        }
    );
    assert!(matches!(
        record(
            &db,
            &actor,
            record_request(&actor, &brew_id, "gravity.final.v1", 1.010),
        )
        .await,
        MeasurementOutcome::Recorded(_)
    ));

    // The lifecycle overview is bounded; a long reopen/correction history must
    // not be fetched in full by every view.
    {
        let mut tx = db.begin_write().await.unwrap();
        for revision in 100..=150 {
            krausen_brewery::brew_evidence_store::insert_lifecycle_event(
                &mut tx,
                &BrewLifecycleEvent {
                    workspace_id: actor.workspace_id.clone(),
                    brew_id: brew_id.clone(),
                    brew_revision: revision,
                    from_status: BrewStatusV1::Started,
                    to_status: BrewStatusV1::Completed,
                    reason: Some("synthetic bounded history".into()),
                    actor_id: actor.actor_id.clone(),
                    recorded_at: chrono::Utc::now().timestamp(),
                },
            )
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();
    }
    let bounded_lifecycle = view(&db, &actor, &brew_id).await;
    assert_eq!(bounded_lifecycle.lifecycle.len(), 50);
    let lifecycle_cursor = bounded_lifecycle
        .lifecycle_next_cursor
        .clone()
        .expect("bounded lifecycle continuation");
    let continuation = brew_evidence::view(
        &db,
        &actor,
        BrewEvidenceViewRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            lifecycle_cursor: Some(lifecycle_cursor),
        },
    )
    .await;
    let BrewEvidenceViewOutcome::View(continuation) = continuation else {
        panic!("lifecycle continuation: {continuation:?}");
    };
    assert_eq!(continuation.lifecycle.len(), 5);
    assert!(continuation.lifecycle_next_cursor.is_none());
    assert!(
        bounded_lifecycle.lifecycle.last().unwrap().brew_revision
            < continuation.lifecycle.first().unwrap().brew_revision
    );

    // Request identity replay returns the original entry; reuse with a changed
    // payload is a typed conflict and writes nothing.
    let replay = record_request(&actor, &brew_id, "volume.packaged.v1", 19.0);
    let replay_id = replay.request_id.clone();
    let MeasurementOutcome::Recorded(replayed) = record(&db, &actor, replay.clone()).await else {
        panic!("replay record");
    };
    let MeasurementOutcome::Recorded(same) = record(&db, &actor, replay).await else {
        panic!("replay result");
    };
    assert_eq!(replayed.entry_id, same.entry_id);
    let mut changed_payload = record_request(&actor, &brew_id, "volume.packaged.v1", 18.5);
    changed_payload.request_id = replay_id;
    assert_eq!(
        record(&db, &actor, changed_payload).await,
        MeasurementOutcome::RequestIdentityConflict
    );

    // Workspace scoping and current membership are rechecked even for a user
    // who belongs to more than one workspace.
    let second_workspace = format!("w2-{}", Uuid::new_v4());
    let second_credential = format!("c2-{}", Uuid::new_v4());
    macro_rules! run {
        ($sql:expr $(, $a:expr)*) => { match &db {
            DbPool::Postgres(p) => sqlx::query($sql)$(.bind($a))*.execute(p).await.unwrap().rows_affected(),
            DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($a))*.execute(p).await.unwrap().rows_affected(),
        } };
    }
    run!(
        "INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ($1,$2)",
        &second_workspace,
        &actor.actor_id
    );
    run!(
        "INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ($1,$2,'member')",
        &second_workspace,
        &actor.actor_id
    );
    run!(
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)",
        &second_credential,
        &actor.actor_id,
        chrono::Utc::now().timestamp() + 3600
    );
    let other_workspace_actor = ActorContext {
        actor_id: actor.actor_id.clone(),
        workspace_id: second_workspace,
        credential_id: second_credential,
        access_expires_at: chrono::Utc::now().timestamp() + 3600,
    };
    assert_eq!(
        brew_evidence::view(
            &db,
            &other_workspace_actor,
            BrewEvidenceViewRequest {
                intent: intent(&other_workspace_actor),
                brew_id: brew_id.clone(),
                lifecycle_cursor: None,
            },
        )
        .await,
        BrewEvidenceViewOutcome::NotFound
    );
    assert_eq!(
        record(
            &db,
            &other_workspace_actor,
            record_request(&other_workspace_actor, &brew_id, "gravity.final.v1", 1.010),
        )
        .await,
        MeasurementOutcome::NotFound
    );

    let outsider = format!("outsider-{}", Uuid::new_v4());
    let outsider_credential = format!("outsider-c-{}", Uuid::new_v4());
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2)",
        &outsider,
        format!("{outsider}@example.test")
    );
    run!(
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)",
        &outsider_credential,
        &outsider,
        chrono::Utc::now().timestamp() + 3600
    );
    let outsider_actor = ActorContext {
        actor_id: outsider,
        workspace_id: actor.workspace_id.clone(),
        credential_id: outsider_credential,
        access_expires_at: chrono::Utc::now().timestamp() + 3600,
    };
    assert_eq!(
        brew_evidence::view(
            &db,
            &outsider_actor,
            BrewEvidenceViewRequest {
                intent: intent(&outsider_actor),
                brew_id: brew_id.clone(),
                lifecycle_cursor: None,
            },
        )
        .await,
        BrewEvidenceViewOutcome::Forbidden
    );

    // The shared dispatcher exposes the same operation to UI and MCP adapters.
    let dispatched = brew_evidence::execute(
        &db,
        &actor,
        BrewEvidenceCommand::View(BrewEvidenceViewRequest {
            intent: intent(&actor),
            brew_id: brew_id.clone(),
            lifecycle_cursor: None,
        }),
    )
    .await;
    assert!(matches!(
        dispatched,
        BrewEvidenceOutcome::View(BrewEvidenceViewOutcome::View(_))
    ));
}

#[tokio::test]
async fn sqlite_brew_evidence() {
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
async fn postgres_brew_evidence() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL")
        .expect("KRAUSEN_TEST_POSTGRES_URL is required for PostgreSQL tests");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    Box::pin(exercise(DbPool::Postgres(pool))).await;
}

async fn exercise_acceptance_cases(db: DbPool) {
    macro_rules! count {
        ($sql:expr $(, $a:expr)*) => { match &db {
            DbPool::Postgres(p) => sqlx::query_scalar::<_, i64>($sql)$(.bind($a))*.fetch_one(p).await.unwrap(),
            DbPool::Sqlite(p) => sqlx::query_scalar::<_, i64>($sql)$(.bind($a))*.fetch_one(p).await.unwrap(),
        } };
    }
    macro_rules! run {
        ($sql:expr $(, $a:expr)*) => { match &db {
            DbPool::Postgres(p) => sqlx::query($sql)$(.bind($a))*.execute(p).await.unwrap().rows_affected(),
            DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($a))*.execute(p).await.unwrap().rows_affected(),
        } };
    }

    // Two corrections from the same predecessor serialize on the brew lock:
    // one succeeds and the loser gets a typed conflict with no extra evidence.
    {
        let (actor, brew_id, _) = fixture(&db).await;
        let MeasurementOutcome::Recorded(first) = record(
            &db,
            &actor,
            record_request(&actor, &brew_id, "gravity.original.v1", 1.050),
        )
        .await
        else {
            panic!("race fixture reading");
        };
        let before = count!(
            "SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = $1 AND brew_id = $2 AND root_observation_id = $3",
            &actor.workspace_id,
            &brew_id,
            &first.entry_id
        );
        let (left, right) = tokio::join!(
            brew_evidence::measurements(
                &db,
                &actor,
                MeasurementCommand::Correct(correct_request(
                    &actor,
                    &brew_id,
                    &first.entry_id,
                    Some(1),
                    1.051,
                )),
            ),
            brew_evidence::measurements(
                &db,
                &actor,
                MeasurementCommand::Correct(correct_request(
                    &actor,
                    &brew_id,
                    &first.entry_id,
                    Some(1),
                    1.052,
                )),
            ),
        );
        let successes = [&left, &right]
            .into_iter()
            .filter(|outcome| matches!(outcome, MeasurementOutcome::Corrected(_)))
            .count();
        assert_eq!(successes, 1, "exactly one correction must win");
        let loser = if matches!(&left, MeasurementOutcome::Corrected(_)) {
            &right
        } else {
            &left
        };
        assert!(matches!(
            loser,
            MeasurementOutcome::EntryConflict { .. } | MeasurementOutcome::Conflict { .. }
        ));
        assert_eq!(
            count!(
                "SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = $1 AND brew_id = $2 AND root_observation_id = $3",
                &actor.workspace_id,
                &brew_id,
                &first.entry_id
            ),
            before + 1,
            "the losing correction must roll back its evidence row"
        );
        let selected = view(&db, &actor, &brew_id).await;
        let slot = selected
            .measurements
            .iter()
            .find(|slot| slot.quantity_id == "gravity.original.v1")
            .unwrap();
        let value = slot.current.as_ref().and_then(|entry| entry.value);
        assert!(value == Some(1.051) || value == Some(1.052));
    }

    // Two explicit selections from the same slot revision serialize: one
    // success, one typed conflict, one new selection event.
    {
        let (actor, brew_id, _) = fixture(&db).await;
        let MeasurementOutcome::Recorded(first) = record(
            &db,
            &actor,
            record_request(&actor, &brew_id, "gravity.original.v1", 1.050),
        )
        .await
        else {
            panic!("selection race fixture");
        };
        let MeasurementOutcome::Recorded(second) = record(
            &db,
            &actor,
            record_request(&actor, &brew_id, "gravity.original.v1", 1.052),
        )
        .await
        else {
            panic!("second selection race fixture");
        };
        let MeasurementOutcome::Recorded(third) = record(
            &db,
            &actor,
            record_request(&actor, &brew_id, "gravity.original.v1", 1.054),
        )
        .await
        else {
            panic!("third selection race fixture");
        };
        assert_ne!(first.entry_id, second.entry_id);
        assert_ne!(second.entry_id, third.entry_id);
        let (left, right) = tokio::join!(
            brew_evidence::measurements(
                &db,
                &actor,
                MeasurementCommand::Select(select_request(
                    &actor,
                    &brew_id,
                    Some(&second.entry_id),
                    1,
                )),
            ),
            brew_evidence::measurements(
                &db,
                &actor,
                MeasurementCommand::Select(select_request(
                    &actor,
                    &brew_id,
                    Some(&third.entry_id),
                    1,
                )),
            ),
        );
        let successes = [&left, &right]
            .into_iter()
            .filter(|outcome| matches!(outcome, MeasurementOutcome::Selected(_)))
            .count();
        assert_eq!(successes, 1, "exactly one selection must win");
        let loser = if matches!(&left, MeasurementOutcome::Selected(_)) {
            &right
        } else {
            &left
        };
        assert!(matches!(
            loser,
            MeasurementOutcome::Conflict {
                expected: 1,
                actual: 2
            }
        ));
        assert_eq!(
            count!(
                "SELECT COUNT(*) FROM brew_actual_selection_events WHERE workspace_id = $1 AND brew_id = $2 AND quantity_id = 'gravity.original.v1' AND revision = 2",
                &actor.workspace_id,
                &brew_id
            ),
            1
        );
    }

    // Two actual amendments from the same expected revision serialize: one
    // immutable actual revision and one typed conflict.
    {
        let (actor, brew_id, _) = fixture(&db).await;
        let effective = brew_evidence::actual(
            &db,
            &actor,
            ActualFormulationCommand::Get(ActualFormulationViewRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
            }),
        )
        .await;
        let ActualFormulationOutcome::View(effective) = effective else {
            panic!("actual race view");
        };
        let mut left_document = effective.document.clone();
        left_document.boil_minutes = Some(70.0);
        let mut right_document = effective.document.clone();
        right_document.boil_minutes = Some(71.0);
        let (left, right) = tokio::join!(
            amend_document(&db, &actor, &brew_id, None, left_document),
            amend_document(&db, &actor, &brew_id, None, right_document),
        );
        let successes = [&left, &right]
            .into_iter()
            .filter(|outcome| matches!(outcome, ActualFormulationOutcome::Amended(_)))
            .count();
        assert_eq!(successes, 1, "exactly one actual amendment must win");
        let loser = if matches!(&left, ActualFormulationOutcome::Amended(_)) {
            &right
        } else {
            &left
        };
        assert!(matches!(
            loser,
            ActualFormulationOutcome::Conflict {
                expected: None,
                actual: Some(1)
            }
        ));
        assert_eq!(
            count!(
                "SELECT COUNT(*) FROM brew_actual_formulations WHERE workspace_id = $1 AND brew_id = $2",
                &actor.workspace_id,
                &brew_id
            ),
            1
        );
    }

    // An injected failure after the evidence, selection, request and change
    // writes rolls the whole transaction back on both real backends.
    {
        let (actor, brew_id, _) = fixture(&db).await;
        let MeasurementOutcome::Recorded(_) = record(
            &db,
            &actor,
            record_request(&actor, &brew_id, "gravity.original.v1", 1.050),
        )
        .await
        else {
            panic!("rollback fixture reading");
        };
        let entries = count!(
            "SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = $1 AND brew_id = $2",
            &actor.workspace_id,
            &brew_id
        );
        let requests = count!(
            "SELECT COUNT(*) FROM app_requests WHERE workspace_id = $1 AND operation = 'brew.measurement.record.v1'",
            &actor.workspace_id
        );
        let changes = count!(
            "SELECT COUNT(*) FROM app_changes WHERE workspace_id = $1 AND operation = 'brew.measurement.record.v1'",
            &actor.workspace_id
        );
        let selection_before = view(&db, &actor, &brew_id).await;
        let selection_before = selection_before
            .measurements
            .iter()
            .find(|slot| slot.quantity_id == "gravity.original.v1")
            .unwrap();
        let selection_revision_before = selection_before.selection_revision;
        let selected_root_before = selection_before.selected_root_observation_id.clone();
        let lineage_before = count!(
            "SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = $1 AND brew_id = $2 AND root_observation_id = $3",
            &actor.workspace_id,
            &brew_id,
            selected_root_before.as_deref().unwrap()
        );
        let workspace = actor.workspace_id.as_str();
        if db.is_postgres() {
            run!("DROP TRIGGER IF EXISTS kra21_reject_change ON app_changes");
            let function = format!(
                "CREATE OR REPLACE FUNCTION kra21_reject_change() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.workspace_id = '{workspace}' THEN RAISE EXCEPTION 'injected failure'; END IF; RETURN NEW; END $$"
            );
            run!(function.as_str());
            run!(
                "CREATE TRIGGER kra21_reject_change BEFORE INSERT ON app_changes FOR EACH ROW EXECUTE FUNCTION kra21_reject_change()"
            );
        } else {
            run!("DROP TRIGGER IF EXISTS kra21_reject_change");
            let trigger = format!(
                "CREATE TRIGGER kra21_reject_change BEFORE INSERT ON app_changes WHEN NEW.workspace_id = '{workspace}' BEGIN SELECT RAISE(ABORT, 'injected failure'); END"
            );
            run!(trigger.as_str());
        }
        let outcome = record(
            &db,
            &actor,
            record_request(&actor, &brew_id, "gravity.original.v1", 1.052),
        )
        .await;
        if db.is_postgres() {
            run!("DROP TRIGGER kra21_reject_change ON app_changes");
            run!("DROP FUNCTION kra21_reject_change()");
        } else {
            run!("DROP TRIGGER kra21_reject_change");
        }
        assert_eq!(outcome, MeasurementOutcome::Unavailable);
        assert_eq!(
            count!(
                "SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = $1 AND brew_id = $2",
                &actor.workspace_id,
                &brew_id
            ),
            entries,
            "evidence must roll back"
        );
        assert_eq!(
            count!(
                "SELECT COUNT(*) FROM app_requests WHERE workspace_id = $1 AND operation = 'brew.measurement.record.v1'",
                &actor.workspace_id
            ),
            requests,
            "request result must roll back"
        );
        assert_eq!(
            count!(
                "SELECT COUNT(*) FROM app_changes WHERE workspace_id = $1 AND operation = 'brew.measurement.record.v1'",
                &actor.workspace_id
            ),
            changes,
            "change record must roll back"
        );
        let selection_after = view(&db, &actor, &brew_id).await;
        let selection_after = selection_after
            .measurements
            .iter()
            .find(|slot| slot.quantity_id == "gravity.original.v1")
            .unwrap();
        assert_eq!(
            selection_after.selection_revision, selection_revision_before,
            "selection revision must roll back"
        );
        assert_eq!(
            selection_after.selected_root_observation_id, selected_root_before,
            "selected root must roll back"
        );
        assert_eq!(
            count!(
                "SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = $1 AND brew_id = $2 AND root_observation_id = $3",
                &actor.workspace_id,
                &brew_id,
                selected_root_before.as_deref().unwrap()
            ),
            lineage_before,
            "lineage history must roll back"
        );
    }

    // Expired and revoked credentials fail before recovery; a fresh credential
    // can replay the same request and produce the original result.
    {
        let (mut actor, brew_id, _) = fixture(&db).await;
        let request = record_request(&actor, &brew_id, "gravity.final.v1", 1.010);
        let MeasurementOutcome::Recorded(original) = record(&db, &actor, request.clone()).await
        else {
            panic!("expiry fixture reading");
        };
        let entries = count!(
            "SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = $1 AND brew_id = $2",
            &actor.workspace_id,
            &brew_id
        );
        let requests = count!(
            "SELECT COUNT(*) FROM app_requests WHERE workspace_id = $1 AND operation = 'brew.measurement.record.v1'",
            &actor.workspace_id
        );
        let changes = count!(
            "SELECT COUNT(*) FROM app_changes WHERE workspace_id = $1 AND operation = 'brew.measurement.record.v1'",
            &actor.workspace_id
        );
        let past = chrono::Utc::now().timestamp() - 1;
        run!(
            "UPDATE auth_credentials SET expires_at = $3 WHERE credential_id = $1 AND user_id = $2",
            &actor.credential_id,
            &actor.actor_id,
            past
        );
        actor.access_expires_at = past;
        assert_eq!(
            record(&db, &actor, request.clone()).await,
            MeasurementOutcome::Forbidden
        );
        assert_eq!(
            count!(
                "SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = $1 AND brew_id = $2",
                &actor.workspace_id,
                &brew_id
            ),
            entries
        );
        let future = chrono::Utc::now().timestamp() + 3600;
        run!(
            "UPDATE auth_credentials SET expires_at = $3, revoked_at = NULL WHERE credential_id = $1 AND user_id = $2",
            &actor.credential_id,
            &actor.actor_id,
            future
        );
        actor.access_expires_at = future;
        let MeasurementOutcome::Recorded(replayed) = record(&db, &actor, request.clone()).await
        else {
            panic!("expiry replay");
        };
        assert_eq!(replayed.entry_id, original.entry_id);
        run!(
            "UPDATE auth_credentials SET revoked_at = $3 WHERE credential_id = $1 AND user_id = $2",
            &actor.credential_id,
            &actor.actor_id,
            chrono::Utc::now().timestamp()
        );
        assert_eq!(
            record(&db, &actor, request.clone()).await,
            MeasurementOutcome::Forbidden,
            "revocation must deny replay of the same committed request"
        );
        assert_eq!(
            count!(
                "SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = $1 AND brew_id = $2",
                &actor.workspace_id,
                &brew_id
            ),
            entries,
            "revoked replay must not add evidence"
        );
        assert_eq!(
            count!(
                "SELECT COUNT(*) FROM app_requests WHERE workspace_id = $1 AND operation = 'brew.measurement.record.v1'",
                &actor.workspace_id
            ),
            requests
        );
        assert_eq!(
            count!(
                "SELECT COUNT(*) FROM app_changes WHERE workspace_id = $1 AND operation = 'brew.measurement.record.v1'",
                &actor.workspace_id
            ),
            changes
        );
    }

    // A retained result that expired returns the resource tombstone rather than
    // replaying a stale success or re-writing evidence.
    {
        let (actor, brew_id, _) = fixture(&db).await;
        let request = record_request(&actor, &brew_id, "gravity.final.v1", 1.010);
        let MeasurementOutcome::Recorded(_) = record(&db, &actor, request.clone()).await else {
            panic!("result expiry fixture");
        };
        let entries = count!(
            "SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = $1 AND brew_id = $2",
            &actor.workspace_id,
            &brew_id
        );
        let requests = count!(
            "SELECT COUNT(*) FROM app_requests WHERE workspace_id = $1 AND operation = 'brew.measurement.record.v1'",
            &actor.workspace_id
        );
        let changes = count!(
            "SELECT COUNT(*) FROM app_changes WHERE workspace_id = $1 AND operation = 'brew.measurement.record.v1'",
            &actor.workspace_id
        );
        let selection_before = view(&db, &actor, &brew_id).await;
        let selection_before = selection_before
            .measurements
            .iter()
            .find(|slot| slot.quantity_id == "gravity.final.v1")
            .unwrap();
        let selection_revision = selection_before.selection_revision;
        let selected_root = selection_before.selected_root_observation_id.clone();
        run!(
            "UPDATE app_requests SET result_expires_at = $4 WHERE workspace_id = $1 AND actor_id = $2 AND operation = 'brew.measurement.record.v1' AND request_id = $3",
            &actor.workspace_id,
            &actor.actor_id,
            &request.request_id,
            chrono::Utc::now().timestamp() - 1
        );
        assert_eq!(
            record(&db, &actor, request).await,
            MeasurementOutcome::ResultExpired {
                resource_id: Some(brew_id.clone())
            }
        );
        assert_eq!(
            count!(
                "SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = $1 AND brew_id = $2",
                &actor.workspace_id,
                &brew_id
            ),
            entries,
            "expired result replay must not re-execute evidence writes"
        );
        assert_eq!(
            count!(
                "SELECT COUNT(*) FROM app_requests WHERE workspace_id = $1 AND operation = 'brew.measurement.record.v1'",
                &actor.workspace_id
            ),
            requests
        );
        assert_eq!(
            count!(
                "SELECT COUNT(*) FROM app_changes WHERE workspace_id = $1 AND operation = 'brew.measurement.record.v1'",
                &actor.workspace_id
            ),
            changes
        );
        let selection_after = view(&db, &actor, &brew_id).await;
        let selection_after = selection_after
            .measurements
            .iter()
            .find(|slot| slot.quantity_id == "gravity.final.v1")
            .unwrap();
        assert_eq!(selection_after.selection_revision, selection_revision);
        assert_eq!(selection_after.selected_root_observation_id, selected_root);
    }

    // Complete and cancel do not require any measurements.
    for (transition, expected_status, expected_event) in [
        (
            BrewTransitionV1::Complete,
            BrewStatusV1::Completed,
            "completed",
        ),
        (
            BrewTransitionV1::Cancel,
            BrewStatusV1::Cancelled,
            "cancelled",
        ),
    ] {
        let (actor, brew_id, _) = fixture(&db).await;
        assert_eq!(
            count!(
                "SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = $1 AND brew_id = $2",
                &actor.workspace_id,
                &brew_id
            ),
            0
        );
        let outcome = brew_evidence::transition(
            &db,
            &actor,
            TransitionBrewRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                expected_revision: 1,
                transition,
                reason: None,
                request_id: Uuid::new_v4().to_string(),
            },
        )
        .await;
        assert_eq!(
            outcome,
            BrewLifecycleOutcome::Transitioned {
                brew_id: brew_id.clone(),
                status: expected_status,
                revision: 2
            }
        );
        assert_eq!(
            count!(
                "SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = $1 AND brew_id = $2",
                &actor.workspace_id,
                &brew_id
            ),
            0,
            "terminal transition must not invent measurements"
        );
        assert_eq!(
            count!(
                "SELECT COUNT(*) FROM brew_lifecycle_events WHERE workspace_id = $1 AND brew_id = $2 AND brew_revision = 2 AND to_status = $3",
                &actor.workspace_id,
                &brew_id,
                expected_event
            ),
            1
        );
    }

    // Evidence corrections and actual amendments remain available after a
    // terminal status; the frozen plan is unchanged.
    {
        let (actor, brew_id, plan) = fixture(&db).await;
        let MeasurementOutcome::Recorded(first) = record(
            &db,
            &actor,
            record_request(&actor, &brew_id, "gravity.original.v1", 1.050),
        )
        .await
        else {
            panic!("terminal evidence fixture");
        };
        let completed = brew_evidence::transition(
            &db,
            &actor,
            TransitionBrewRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                expected_revision: 1,
                transition: BrewTransitionV1::Complete,
                reason: None,
                request_id: Uuid::new_v4().to_string(),
            },
        )
        .await;
        assert!(matches!(
            completed,
            BrewLifecycleOutcome::Transitioned {
                status: BrewStatusV1::Completed,
                revision: 2,
                ..
            }
        ));
        let MeasurementOutcome::Corrected(corrected) = brew_evidence::measurements(
            &db,
            &actor,
            MeasurementCommand::Correct(correct_request(
                &actor,
                &brew_id,
                &first.entry_id,
                Some(1),
                1.051,
            )),
        )
        .await
        else {
            panic!("terminal correction");
        };
        assert_eq!(corrected.value, Some(1.051));
        let actual = amend_document(
            &db,
            &actor,
            &brew_id,
            None,
            krausen_types::brewery::ActualFormulationV1 {
                boil_minutes: Some(75.0),
                fermentables: plan.recipe.fermentables.clone(),
                hops: plan.recipe.hops.clone(),
                yeast: plan.recipe.yeast.clone(),
                other_ingredients: plan.recipe.other_ingredients.clone(),
                process: plan.recipe.process.clone(),
                notes: plan.recipe.notes.clone(),
                extensions: plan.recipe.extensions.clone(),
                schema_version: 1,
            },
        )
        .await;
        assert!(matches!(actual, ActualFormulationOutcome::Amended(_)));
        let note = brew_evidence::notes(
            &db,
            &actor,
            NoteCommand::Record(RecordNoteRequest {
                intent: intent(&actor),
                brew_id: brew_id.clone(),
                text: "terminal note".into(),
                request_id: Uuid::new_v4().to_string(),
            }),
        )
        .await;
        assert!(matches!(note, NoteOutcome::Recorded(_)));
        let after = view(&db, &actor, &brew_id).await;
        assert_eq!(after.status, BrewStatusV1::Completed);
        assert_eq!(after.plan, plan);
        assert_eq!(after.actual.unwrap().document.boil_minutes, Some(75.0));
    }

    // A root from one quantity cannot be selected for another quantity.
    {
        let (actor, brew_id, _) = fixture(&db).await;
        let MeasurementOutcome::Recorded(root) = record(
            &db,
            &actor,
            record_request(&actor, &brew_id, "gravity.original.v1", 1.050),
        )
        .await
        else {
            panic!("cross-quantity fixture");
        };
        let outcome = brew_evidence::measurements(
            &db,
            &actor,
            MeasurementCommand::Select(SelectMeasurementRequest {
                intent: intent(&actor),
                brew_id,
                quantity_id: "gravity.final.v1".into(),
                root_observation_id: Some(root.entry_id),
                expected_selection_revision: 0,
                request_id: Uuid::new_v4().to_string(),
            }),
        )
        .await;
        assert_eq!(outcome, MeasurementOutcome::Invalid);
    }
}

#[tokio::test]
async fn sqlite_brew_evidence_acceptance_cases() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../apps/server/migrations-sqlite")
        .run(&pool)
        .await
        .unwrap();
    Box::pin(exercise_acceptance_cases(DbPool::Sqlite(pool))).await;
}

#[tokio::test]
async fn postgres_brew_evidence_acceptance_cases() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL")
        .expect("KRAUSEN_TEST_POSTGRES_URL is required for PostgreSQL tests");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    Box::pin(exercise_acceptance_cases(DbPool::Postgres(pool))).await;
}
