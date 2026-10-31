// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{ActorContext, Error, equipment, formulation};
use krausen_core::DbPool;
use krausen_types::{
    brewery::*,
    equipment::{ArchiveEquipment, SaveEquipment},
    formulation::{PreviewOutcome, PreviewRequest},
    request_context::OperationIntent,
};

fn settings() -> EquipmentSettingsV1 {
    EquipmentSettingsV1 {
        mash_efficiency_fraction: Some(0.75),
        boil_off_l_per_hour: Some(3.0),
        transfer_loss_l: Some(2.0),
        pre_boil_deadspace_l: Some(0.0),
        grain_absorption_l_per_kg: Some(0.8),
        ..Default::default()
    }
}
fn draft(intent: OperationIntent, id: &str) -> PreviewRequest {
    let mut d = RecipeDocumentV1::default();
    d.batch_size_l = Some(20.0);
    d.boil_minutes = Some(60.0);
    d.equipment_profile_id = Some(id.into());
    d.process.mash_water_method = Some("manual".into());
    d.process.mash_water_l = Some(12.0);
    d.process.apparent_attenuation_fraction = Some(0.8);
    d.fermentables.push(FermentableAddition {
        row_id: "g".into(),
        amount_kg: Some(4.0),
        addition: AdditionStage {
            stage: "mash".into(),
            time_minutes: None,
        },
        specification: FermentableSpecification {
            name: None,
            producer: None,
            kind: Some("grain".into()),
            yield_fraction: Some(0.8),
            colour_ebc: Some(5.0),
        },
        source: None,
    });
    PreviewRequest {
        draft_generation: "g1".into(),
        intent,
        draft: d,
        equipment_draft: EquipmentSettingsV1::default(),
    }
}
async fn exercise(db: DbPool) {
    let suffix = uuid::Uuid::new_v4();
    let user = format!("u-{suffix}");
    let a = format!("a-{suffix}");
    let b = format!("b-{suffix}");
    let cred = format!("c-{suffix}");
    macro_rules! run {($sql:expr $(,$arg:expr)*)=>{match &db {DbPool::Sqlite(p)=>sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),DbPool::Postgres(p)=>sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected()}};}
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
            "INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ($1,$2,'workspace_user')",
            ws,
            &user
        );
    }
    run!(
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)",
        &cred,
        &user,
        4102444800_i64
    );
    let actor = ActorContext {
        actor_id: user.clone(),
        workspace_id: a.clone(),
        credential_id: cred,
        access_expires_at: 4102444800,
    };
    let intent = OperationIntent {
        account_id: user.clone(),
        workspace_id: a.clone(),
    };
    let id = uuid::Uuid::new_v4().to_string();
    let req = SaveEquipment {
        intent: intent.clone(),
        profile_id: id.clone(),
        name: "Test system".into(),
        settings: settings(),
        provenance: krausen_types::equipment::EquipmentProvenance {
            source: "manual".into(),
            assumptions: vec!["measured boil-off".into()],
        },
        expected_revision: 0,
        expected_metadata_revision: 0,
        request_id: uuid::Uuid::new_v4().to_string(),
    };
    let created = equipment::save(&db, &actor, req.clone()).await.unwrap();
    assert_eq!((created.revision, created.metadata_revision), (1, 1));
    assert_eq!(
        equipment::save(&db, &actor, req.clone()).await.unwrap(),
        created
    );
    let mut equivalent = req.clone();
    equivalent.name = "  Test system  ".into();
    assert_eq!(
        equipment::save(&db, &actor, equivalent).await.unwrap(),
        created
    );
    let mut reused = req.clone();
    reused.name = "Different".into();
    assert!(matches!(
        equipment::save(&db, &actor, reused).await,
        Err(Error::RequestIdentityConflict)
    ));
    let PreviewOutcome::Preview(first) =
        formulation::preview(&db, &actor, draft(intent.clone(), &id))
            .await
            .unwrap()
    else {
        panic!("preview")
    };
    let context = first.context.as_ref().unwrap();
    assert_eq!(context.equipment_provenance, "saved_profile");
    assert!(
        context
            .assumptions
            .contains(&"profile:measured boil-off".to_string())
    );
    assert_eq!(context.equipment_profile.as_ref().unwrap().revision, 1);
    assert_eq!(context.effective_equipment.boil_off_l_per_hour, Some(3.0));
    let mut forged = draft(intent.clone(), &id);
    forged.equipment_draft.boil_off_l_per_hour = Some(999.0);
    let PreviewOutcome::Preview(forged_result) =
        formulation::preview(&db, &actor, forged).await.unwrap()
    else {
        panic!("preview")
    };
    assert_eq!(first.context_digest, forged_result.context_digest);
    assert_eq!(
        forged_result
            .context
            .unwrap()
            .effective_equipment
            .boil_off_l_per_hour,
        Some(3.0)
    );
    let mut next = req.clone();
    next.expected_revision = 1;
    next.expected_metadata_revision = created.metadata_revision;
    next.request_id = uuid::Uuid::new_v4().to_string();
    next.settings.boil_off_l_per_hour = Some(4.0);
    let updated = equipment::save(&db, &actor, next).await.unwrap();
    assert_eq!(updated.revision, 2);
    let PreviewOutcome::Preview(second) =
        formulation::preview(&db, &actor, draft(intent.clone(), &id))
            .await
            .unwrap()
    else {
        panic!("preview")
    };
    assert_ne!(first.context_digest, second.context_digest);
    assert_eq!(
        first
            .context
            .unwrap()
            .effective_equipment
            .boil_off_l_per_hour,
        Some(3.0)
    );
    assert_eq!(
        second
            .context
            .unwrap()
            .effective_equipment
            .boil_off_l_per_hour,
        Some(4.0)
    );
    let foreign_actor = ActorContext {
        workspace_id: b.clone(),
        ..actor.clone()
    };
    let foreign_intent = OperationIntent {
        account_id: user,
        workspace_id: b,
    };
    assert!(
        equipment::get(&db, &foreign_actor, &foreign_intent, &id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        formulation::preview(&db, &foreign_actor, draft(foreign_intent, &id))
            .await
            .unwrap(),
        PreviewOutcome::UnresolvedProfile
    ));
    let archive = ArchiveEquipment {
        intent: intent.clone(),
        profile_id: id.clone(),
        archived: true,
        expected_metadata_revision: updated.metadata_revision,
        request_id: uuid::Uuid::new_v4().to_string(),
    };
    let archived = equipment::set_archived(&db, &actor, archive.clone())
        .await
        .unwrap();
    assert!(archived.archived);
    let original = equipment::get_revision(&db, &actor, &intent, &id, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(original.name, "Test system");
    assert_eq!(original.settings.boil_off_l_per_hour, Some(3.0));
    assert_eq!(
        equipment::set_archived(&db, &actor, archive).await.unwrap(),
        archived
    );
    assert!(
        equipment::list(&db, &actor, &intent, "", 20, false)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        formulation::preview(&db, &actor, draft(intent.clone(), &id))
            .await
            .unwrap(),
        PreviewOutcome::UnresolvedProfile
    ));
    let archived_list = equipment::list(&db, &actor, &intent, "", 20, true)
        .await
        .unwrap();
    assert!(
        archived_list
            .iter()
            .any(|profile| profile.profile_id == id && profile.archived)
    );
    let restored = equipment::set_archived(
        &db,
        &actor,
        ArchiveEquipment {
            intent: intent.clone(),
            profile_id: id.clone(),
            archived: false,
            expected_metadata_revision: archived.metadata_revision,
            request_id: uuid::Uuid::new_v4().to_string(),
        },
    )
    .await
    .unwrap();
    assert!(!restored.archived);
    assert_eq!(restored.revision, 2);
    let PreviewOutcome::Preview(after_restore) =
        formulation::preview(&db, &actor, draft(intent.clone(), &id))
            .await
            .unwrap()
    else {
        panic!("preview")
    };
    assert_ne!(after_restore.context_digest, second.context_digest);
    let mut overridden = draft(intent.clone(), &id);
    overridden
        .draft
        .equipment_overrides
        .efficiency_override_fraction = Some(0.6);
    let PreviewOutcome::Preview(with_override) =
        formulation::preview(&db, &actor, overridden).await.unwrap()
    else {
        panic!("preview")
    };
    let override_context = with_override.context.unwrap();
    assert_eq!(
        override_context
            .effective_equipment
            .efficiency_override_fraction,
        Some(0.6)
    );
    assert_eq!(
        override_context
            .equipment_profile
            .unwrap()
            .settings
            .mash_efficiency_fraction,
        Some(0.75)
    );
    assert_ne!(with_override.context_digest, after_restore.context_digest);
    let mut left = req.clone();
    left.expected_revision = 2;
    left.expected_metadata_revision = restored.metadata_revision;
    left.request_id = uuid::Uuid::new_v4().to_string();
    left.name = "Left".into();
    let mut right = left.clone();
    right.request_id = uuid::Uuid::new_v4().to_string();
    right.name = "Right".into();
    let (left, right, concurrent_preview) = tokio::join!(
        equipment::save(&db, &actor, left),
        equipment::save(&db, &actor, right),
        formulation::preview(&db, &actor, draft(intent.clone(), &id))
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    // A preview racing an edit must capture one complete persisted revision,
    // never a mixed identity/settings snapshot.
    let PreviewOutcome::Preview(concurrent_result) = concurrent_preview.unwrap() else {
        panic!("concurrent preview")
    };
    let concurrent_context = concurrent_result.context.as_ref().unwrap();
    let concurrent_profile = concurrent_context.equipment_profile.as_ref().unwrap();
    let stored_revision =
        equipment::get_revision(&db, &actor, &intent, &id, concurrent_profile.revision)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(concurrent_profile.name, stored_revision.name);
    assert_eq!(concurrent_profile.settings, stored_revision.settings);
    assert_eq!(
        concurrent_context.equipment_draft,
        concurrent_profile.settings
    );
    assert!(matches!(
        left.as_ref().err().or(right.as_ref().err()),
        Some(Error::Conflict { actual: 3, .. })
    ));
    // A required change-record failure must roll back the aggregate, immutable
    // revision and retry result; the same request can then commit once.
    if db.is_postgres() {
        run!(
            "CREATE OR REPLACE FUNCTION kra15_reject_change() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$"
        );
        run!(
            "CREATE TRIGGER kra15_reject_change BEFORE INSERT ON app_changes FOR EACH ROW EXECUTE FUNCTION kra15_reject_change()"
        );
    } else {
        run!(
            "CREATE TRIGGER kra15_reject_change BEFORE INSERT ON app_changes BEGIN SELECT RAISE(ABORT, 'injected'); END"
        );
    }
    let mut rollback = req.clone();
    rollback.expected_revision = 3;
    rollback.expected_metadata_revision = restored.metadata_revision + 1;
    rollback.request_id = uuid::Uuid::new_v4().to_string();
    rollback.name = "After rollback".into();
    assert!(
        equipment::save(&db, &actor, rollback.clone())
            .await
            .is_err()
    );
    if db.is_postgres() {
        run!("DROP TRIGGER kra15_reject_change ON app_changes");
        run!("DROP FUNCTION kra15_reject_change()");
    } else {
        run!("DROP TRIGGER kra15_reject_change");
    }
    assert_eq!(
        equipment::get(&db, &actor, &intent, &id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        3
    );
    assert!(
        equipment::get_revision(&db, &actor, &intent, &id, 4)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        equipment::save(&db, &actor, rollback.clone())
            .await
            .unwrap()
            .revision,
        4
    );
    let mut stale = req;
    stale.expected_revision = 1;
    stale.request_id = uuid::Uuid::new_v4().to_string();
    assert!(matches!(
        equipment::save(&db, &actor, stale).await,
        Err(Error::Conflict { actual: 4, .. })
    ));
    run!(
        "UPDATE auth_credentials SET revoked_at=$1 WHERE credential_id=$2",
        chrono::Utc::now().timestamp(),
        &actor.credential_id
    );
    assert!(matches!(
        equipment::get(&db, &actor, &intent, &id).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        equipment::save(&db, &actor, rollback).await,
        Err(Error::Forbidden)
    ));
}
#[tokio::test]
async fn sqlite_profiles() {
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
async fn postgres_profiles() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
