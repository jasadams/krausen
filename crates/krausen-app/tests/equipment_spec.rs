// SPDX-License-Identifier: AGPL-3.0-or-later
//! KRA-70: the equipment profile is exactly Jason's spec. Both backends.
use krausen_app::{ActorContext, Error, equipment, formulation};
use krausen_core::DbPool;
use krausen_types::{
    brewery::*,
    equipment::{ArchiveEquipment, EquipmentProvenance, SaveEquipment},
    formulation::{PreviewOutcome, PreviewRequest, Vessel},
    recipe_editor::{RecipeReadOutcome, RecipeSaveOutcome},
    request_context::OperationIntent,
};

/// The Grainfather-style example from the spec, with the worked-case losses of
/// `docs/CALCULATIONS.md` ("Equipment profile: water chain").
fn spec_settings() -> EquipmentSettingsV1 {
    EquipmentSettingsV1 {
        mash_tun_volume_l: Some(30.0),
        mash_efficiency_fraction: Some(0.75),
        mash_tun_deadspace_l: Some(3.5),
        mash_thickness_l_per_kg: Some(2.8),
        mash_tun_loss_l: Some(0.5),
        mash_grain_absorption_l_per_kg: Some(0.8),
        sparge_vessel_volume_l: Some(19.0),
        sparge_grain_absorption_l_per_kg: Some(0.1),
        sparge_deadspace_l: Some(1.0),
        boil_volume_l: Some(30.0),
        wort_shrinkage_fraction: Some(0.04),
        boil_loss_l_per_hour: Some(3.0),
        trub_and_chiller_loss_l: Some(2.0),
        kettle_diameter_cm: Some(30.0),
        cooling_method: Some(CoolingMethodV1::CounterflowChiller),
        cooling_flow_rate_l_per_minute: Some(1.0),
        ..Default::default()
    }
}
fn spec_about() -> EquipmentAboutV1 {
    EquipmentAboutV1 {
        brand: Some("Grainfather".into()),
        description: Some("All in one".into()),
        measurement: Some("metric".into()),
        equipment_type: Some("all_grain".into()),
        controller_type: Some("Grainfather Bluetooth Controller".into()),
        recirculation_system: Some("RIMS".into()),
        vessel_count: Some(1),
        batch_size_l: Some(20.0),
        sparge_water_alert: true,
        sparge_progress_counter: true,
        boil_power_control: true,
        notes: Some("Calibrated 2026-11".into()),
    }
}
fn provenance() -> EquipmentProvenance {
    EquipmentProvenance {
        source: "manual".into(),
        assumptions: vec![],
    }
}
fn save_request(
    intent: &OperationIntent,
    id: &str,
    name: &str,
    is_default: Option<bool>,
    expected: (i64, i64),
) -> SaveEquipment {
    SaveEquipment {
        intent: intent.clone(),
        profile_id: id.into(),
        name: name.into(),
        settings: spec_settings(),
        about: spec_about(),
        is_default,
        provenance: provenance(),
        expected_revision: expected.0,
        expected_metadata_revision: expected.1,
        request_id: uuid::Uuid::new_v4().to_string(),
    }
}
/// 4.2 kg of mashed grain at yield 0.8, mash thickness mode: points
/// 4.2 × 0.8 × 385.6745 × 0.75 = 971.9 points·L.
fn recipe(profile_id: &str) -> RecipeDocumentV1 {
    let mut d = RecipeDocumentV1::default();
    d.batch_size_l = Some(20.0);
    d.boil_minutes = Some(60.0);
    d.equipment_profile_id = Some(profile_id.into());
    d.process.mash_water_method = Some("profile_thickness".into());
    d.process.apparent_attenuation_fraction = Some(0.8);
    d.fermentables.push(FermentableAddition {
        row_id: "g".into(),
        amount_kg: Some(4.2),
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
    d
}

async fn exercise(db: DbPool) {
    let suffix = uuid::Uuid::new_v4();
    let user = format!("u-{suffix}");
    let ws = format!("w-{suffix}");
    macro_rules! run {($sql:expr $(,$arg:expr)*)=>{match &db {DbPool::Sqlite(p)=>sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),DbPool::Postgres(p)=>sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected()}};}
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2)",
        &user,
        format!("{suffix}@example.test")
    );
    run!(
        "INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ($1,$2)",
        &ws,
        &user
    );
    run!(
        "INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ($1,$2,'workspace_user')",
        &ws,
        &user
    );
    let actor = ActorContext {
        actor_id: user.clone(),
        workspace_id: ws.clone(),
        session_iat_us: Some(chrono::Utc::now().timestamp_micros()),
        access_expires_at: 4102444800,
    };
    let intent = OperationIntent {
        account_id: user.clone(),
        workspace_id: ws.clone(),
    };

    // The whole spec saves, reads back and is the workspace default.
    let first_id = uuid::Uuid::new_v4().to_string();
    let first = equipment::save(
        &db,
        &actor,
        save_request(
            &intent,
            &first_id,
            "Grainfather (220V) - Bluetooth Controller",
            Some(true),
            (0, 0),
        ),
    )
    .await
    .unwrap();
    assert_eq!(first.settings, spec_settings());
    assert_eq!(first.about, spec_about());
    assert!(first.is_default);
    let read = equipment::get(&db, &actor, &intent, &first_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read, first);
    let revision = equipment::get_revision(&db, &actor, &intent, &first_id, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(revision.about, spec_about());
    assert_eq!(revision.settings, spec_settings());

    // New recipes pick the default up.
    let page = equipment::list(&db, &actor, &intent, None, 20, false)
        .await
        .unwrap();
    let default = page.default_profile.expect("default on the list page");
    assert_eq!(default.profile_id, first_id);
    let seed = default.new_recipe_seed();
    assert_eq!(seed.equipment_profile_id.as_deref(), Some(first_id.as_str()));
    assert_eq!(seed.batch_size_l, None, "the profile's batch size does not seed new recipes");

    // Every creation path takes the default, inside the save transaction: a recipe naming no
    // profile gets it, one naming a profile keeps its own, and a retry with the same request id
    // after the default moved recovers the original result.
    let request = uuid::Uuid::new_v4();
    let blank = RecipeDocumentV1 { title: "Defaulted".into(), ..Default::default() };
    let created = krausen_app::recipe_editor::save(&db, &actor, &intent, None, None, &blank, request).await;
    let RecipeSaveOutcome::Saved { recipe_id, revision: 1 } = created.clone() else { panic!("create: {created:?}") };
    let read = |id: String| {
        let (db, actor, intent) = (db.clone(), actor.clone(), intent.clone());
        async move {
            let RecipeReadOutcome::Found(found) = krausen_app::recipe_editor::read(&db, &actor, &intent, &id).await else { panic!("read") };
            found.document.equipment_profile_id
        }
    };
    assert_eq!(read(recipe_id.clone()).await.as_deref(), Some(first_id.as_str()));
    // Move the default, then retry the identical request.
    let moved_id = uuid::Uuid::new_v4().to_string();
    equipment::save(&db, &actor, save_request(&intent, &moved_id, "Moved default", Some(true), (0, 0))).await.unwrap();
    assert_eq!(
        krausen_app::recipe_editor::save(&db, &actor, &intent, None, None, &blank, request).await,
        created,
        "a retry after the default changed recovers the original result"
    );
    let named = RecipeDocumentV1 { title: "Named".into(), equipment_profile_id: Some(first_id.clone()), ..Default::default() };
    let RecipeSaveOutcome::Saved { recipe_id: named_id, .. } = krausen_app::recipe_editor::save(&db, &actor, &intent, None, None, &named, uuid::Uuid::new_v4()).await else { panic!("named") };
    assert_eq!(read(named_id).await.as_deref(), Some(first_id.as_str()));
    let fresh = RecipeDocumentV1 { title: "Fresh".into(), ..Default::default() };
    let RecipeSaveOutcome::Saved { recipe_id: fresh_id, .. } = krausen_app::recipe_editor::save(&db, &actor, &intent, None, None, &fresh, uuid::Uuid::new_v4()).await else { panic!("fresh") };
    assert_eq!(read(fresh_id).await.as_deref(), Some(moved_id.as_str()), "a new create takes the current default");
    // An actor outside the workspace stays Forbidden, not Unavailable.
    let stranger = ActorContext { actor_id: "stranger".into(), ..actor.clone() };
    assert_eq!(
        krausen_app::recipe_editor::save(&db, &stranger, &intent, None, None, &fresh, uuid::Uuid::new_v4()).await,
        RecipeSaveOutcome::Forbidden
    );
    // Put the default back for the checks below.
    equipment::save(&db, &actor, save_request(&intent, &first_id, "Grainfather (220V) - Bluetooth Controller", Some(true), (1, 1))).await.unwrap();
    equipment::save(&db, &actor, save_request(&intent, &moved_id, "Moved default", Some(false), (1, 1))).await.unwrap();

    // A save that does not mention the default leaves it alone.
    let mut again = save_request(&intent, &first_id, "Renamed", None, (1, 1));
    again.about.notes = Some("Edited".into());
    let renamed = equipment::save(&db, &actor, again).await.unwrap();
    assert!(renamed.is_default);
    assert_eq!(renamed.revision, 2);

    // Only one default per workspace: choosing another moves it.
    let second_id = uuid::Uuid::new_v4().to_string();
    let second = equipment::save(
        &db,
        &actor,
        save_request(&intent, &second_id, "Second", Some(true), (0, 0)),
    )
    .await
    .unwrap();
    assert!(second.is_default);
    let first_now = equipment::get(&db, &actor, &intent, &first_id)
        .await
        .unwrap()
        .unwrap();
    assert!(!first_now.is_default);
    // Switching it off clears it.
    let cleared = equipment::save(
        &db,
        &actor,
        save_request(
            &intent,
            &second_id,
            "Second",
            Some(false),
            (second.revision, second.metadata_revision),
        ),
    )
    .await
    .unwrap();
    assert!(!cleared.is_default);
    let page = equipment::list(&db, &actor, &intent, None, 20, false)
        .await
        .unwrap();
    assert!(page.default_profile.is_none());

    // Archiving the default clears it; an archived profile cannot be made default.
    let made = equipment::save(
        &db,
        &actor,
        save_request(
            &intent,
            &second_id,
            "Second",
            Some(true),
            (cleared.revision, cleared.metadata_revision),
        ),
    )
    .await
    .unwrap();
    let archived = equipment::set_archived(
        &db,
        &actor,
        ArchiveEquipment {
            intent: intent.clone(),
            profile_id: second_id.clone(),
            archived: true,
            expected_metadata_revision: made.metadata_revision,
            request_id: uuid::Uuid::new_v4().to_string(),
        },
    )
    .await
    .unwrap();
    assert!(archived.archived && !archived.is_default);
    assert!(
        equipment::list(&db, &actor, &intent, None, 20, true)
            .await
            .unwrap()
            .default_profile
            .is_none()
    );
    assert!(matches!(
        equipment::save(
            &db,
            &actor,
            save_request(
                &intent,
                &second_id,
                "Second",
                Some(true),
                (archived.revision, archived.metadata_revision),
            ),
        )
        .await,
        Err(Error::EquipmentArchived)
    ));

    // Brewhouse efficiency is a recipe-level override, never a profile field.
    let mut with_override = save_request(&intent, &first_id, "Renamed", None, (2, 2));
    with_override.settings.efficiency_override_fraction = Some(0.7);
    assert!(matches!(
        equipment::save(&db, &actor, with_override).await,
        Err(Error::Invalid(_))
    ));
    // Spec bounds: shrinkage is below one; the about block is validated.
    let mut bad_shrinkage = save_request(&intent, &first_id, "Renamed", None, (2, 2));
    bad_shrinkage.settings.wort_shrinkage_fraction = Some(1.0);
    assert!(matches!(
        equipment::save(&db, &actor, bad_shrinkage).await,
        Err(Error::Invalid(_))
    ));
    let mut bad_about = save_request(&intent, &first_id, "Renamed", None, (2, 2));
    bad_about.about.measurement = Some("imperial".into());
    assert!(matches!(
        equipment::save(&db, &actor, bad_about).await,
        Err(Error::Invalid(_))
    ));

    // The estimates use the stages' volumes and losses: the worked case of
    // docs/CALCULATIONS.md.
    let PreviewOutcome::Preview(preview) = formulation::preview(
        &db,
        &actor,
        PreviewRequest {
            draft_generation: "g".into(),
            intent: intent.clone(),
            draft: recipe(&first_id),
            equipment_draft: EquipmentSettingsV1::default(),
        },
    )
    .await
    .unwrap() else {
        panic!("preview")
    };
    let t = &preview.targets;
    let close = |a: Option<f64>, b: f64| assert!((a.unwrap() - b).abs() < 1e-9, "{a:?} vs {b}");
    close(t.post_boil_l, 22.0);
    close(t.pre_boil_l, 24.88);
    close(t.grain_absorption_l, 3.78);
    close(t.total_water_l, 33.66);
    close(t.mash_water_l, 11.76);
    close(t.sparge_water_l, 21.9);
    // Sparge vessel (19 L) is overfilled by the 21.9 L sparge; mash tun and
    // kettle (pre-boil hot 25.9167 L) are within their volumes.
    assert_eq!(t.vessel_warnings.len(), 1);
    assert_eq!(t.vessel_warnings[0].vessel, Vessel::SpargeVessel);
    assert!((t.vessel_warnings[0].required_l - 21.9).abs() < 1e-9);
    assert_eq!(t.vessel_warnings[0].capacity_l, 19.0);
    let context = preview.context.unwrap();
    assert!(context.assumptions.contains(&"kettle_uncovered".to_string()));
    assert!(
        context
            .assumptions
            .contains(&"boil_temperature_100c".to_string())
    );
    assert!(!context.equipment_profile.unwrap().is_default);

    // Profile data from before the spec still loads: old names are read aliases,
    // dropped fields are ignored and a profile-level Brewhouse efficiency is
    // dropped (no migration). The stored revision is not rewritten.
    let legacy_id = uuid::Uuid::new_v4().to_string();
    let legacy_json = r#"{"schema_version":1,"efficiency_override_fraction":0.6,"mash_efficiency_fraction":0.7,"boil_off_l_per_hour":3.5,"transfer_loss_l":1.5,"pre_boil_deadspace_l":0.5,"grain_absorption_l_per_kg":0.9,"mash_thickness_l_per_kg":3.0,"kettle_capacity_l":40.0,"kettle_diameter_cm":32.0,"kettle_opening_cm":30.0,"boil_temperature_c":99.0,"chilling":{"method":"forced","rate_per_minute":0.5,"plateau_c":20.0},"extensions":{}}"#;
    run!(
        "INSERT INTO equipment_profiles (workspace_id,profile_id,name,current_revision,metadata_revision,archived) VALUES ($1,$2,'Legacy',1,1,FALSE)",
        &ws,
        &legacy_id
    );
    run!(
        "INSERT INTO equipment_revisions (workspace_id,profile_id,revision,name,schema_version,settings_json,provenance_json,content_digest,actor_id,recorded_at) VALUES ($1,$2,1,'Legacy',1,$3,'{\"source\":\"manual\",\"assumptions\":[]}','digest',$4,1)",
        &ws,
        &legacy_id,
        legacy_json,
        &user
    );
    let legacy = equipment::get(&db, &actor, &intent, &legacy_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(legacy.settings.efficiency_override_fraction, None);
    assert_eq!(legacy.settings.mash_efficiency_fraction, Some(0.7));
    assert_eq!(legacy.settings.boil_loss_l_per_hour, Some(3.5));
    assert_eq!(legacy.settings.trub_and_chiller_loss_l, Some(1.5));
    assert_eq!(legacy.settings.mash_tun_deadspace_l, Some(0.5));
    assert_eq!(legacy.settings.mash_grain_absorption_l_per_kg, Some(0.9));
    assert_eq!(legacy.settings.boil_volume_l, Some(40.0));
    assert_eq!(legacy.settings.cooling_method, None);
    assert_eq!(legacy.about, EquipmentAboutV1::default());
    assert!(!legacy.is_default);
    let stored: String = match &db {
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT settings_json FROM equipment_revisions WHERE profile_id=$1",
        )
        .bind(&legacy_id)
        .fetch_one(p)
        .await
        .unwrap(),
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT settings_json FROM equipment_revisions WHERE profile_id=$1",
        )
        .bind(&legacy_id)
        .fetch_one(p)
        .await
        .unwrap(),
    };
    assert_eq!(stored, legacy_json);
}

#[tokio::test]
async fn sqlite_equipment_spec() {
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
async fn postgres_equipment_spec() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}

/// Equipment saved before the spec migration keeps its stored revision untouched,
/// gains an empty about block and reads back through the current code.
async fn upgrade(db: DbPool, migrator: sqlx::migrate::Migrator) {
    let before = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            migrator
                .iter()
                .filter(|m| m.version != 20261128000000)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    macro_rules! run {($sql:expr $(,$arg:expr)*)=>{match &db {DbPool::Sqlite(p)=>sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),DbPool::Postgres(p)=>sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected()}};}
    match &db {
        DbPool::Sqlite(p) => before.run(p).await.unwrap(),
        DbPool::Postgres(p) => before.run(p).await.unwrap(),
    }
    let suffix = uuid::Uuid::new_v4();
    let (user, ws, id) = (format!("u-{suffix}"), format!("w-{suffix}"), uuid::Uuid::new_v4().to_string());
    run!("INSERT INTO users (user_id,email) VALUES ($1,$2)", &user, format!("{suffix}@example.test"));
    run!("INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ($1,$2)", &ws, &user);
    run!("INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ($1,$2,'workspace_user')", &ws, &user);
    let old = r#"{"schema_version":1,"mash_efficiency_fraction":0.7,"boil_off_l_per_hour":3.5,"kettle_opening_cm":30.0,"extensions":{}}"#;
    run!("INSERT INTO equipment_profiles (workspace_id,profile_id,name,current_revision,metadata_revision,archived) VALUES ($1,$2,'Old',1,1,FALSE)", &ws, &id);
    run!("INSERT INTO equipment_revisions (workspace_id,profile_id,revision,name,schema_version,settings_json,provenance_json,content_digest,actor_id,recorded_at) VALUES ($1,$2,1,'Old',1,$3,'{\"source\":\"manual\",\"assumptions\":[]}','digest',$4,1)", &ws, &id, old, &user);
    match &db {
        DbPool::Sqlite(p) => migrator.run(p).await.unwrap(),
        DbPool::Postgres(p) => migrator.run(p).await.unwrap(),
    }
    let actor = ActorContext {
        actor_id: user.clone(),
        workspace_id: ws.clone(),
        session_iat_us: Some(chrono::Utc::now().timestamp_micros()),
        access_expires_at: 4102444800,
    };
    let intent = OperationIntent { account_id: user.clone(), workspace_id: ws.clone() };
    let profile = equipment::get(&db, &actor, &intent, &id).await.unwrap().unwrap();
    assert_eq!(profile.about, EquipmentAboutV1::default());
    assert!(!profile.is_default);
    assert_eq!(profile.settings.boil_loss_l_per_hour, Some(3.5));
    let revision = equipment::get_revision(&db, &actor, &intent, &id, 1).await.unwrap().unwrap();
    assert_eq!(revision.settings.mash_efficiency_fraction, Some(0.7));
    // A new save on top of the old revision appends revision 2 with its about block.
    let saved = equipment::save(&db, &actor, SaveEquipment { about: spec_about(), settings: spec_settings(), ..save_request(&intent, &id, "Old", Some(true), (1, 1)) }).await.unwrap();
    assert_eq!((saved.revision, saved.is_default), (2, true));
}
#[tokio::test]
async fn sqlite_upgrade_keeps_old_equipment_readable() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    upgrade(DbPool::Sqlite(pool), sqlx::migrate!("../../apps/server/migrations-sqlite")).await;
}
#[tokio::test]
async fn postgres_upgrade_keeps_old_equipment_readable() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL required");
    // The upgrade needs a database that has not had the spec migration applied yet.
    let admin = sqlx::PgPool::connect(&url).await.unwrap();
    let name = format!("upgrade_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {name}")).execute(&admin).await.unwrap();
    let options = url
        .parse::<sqlx::postgres::PgConnectOptions>()
        .unwrap()
        .database(&name);
    let pool = sqlx::PgPool::connect_with(options).await.unwrap();
    upgrade(DbPool::Postgres(pool.clone()), sqlx::migrate!("../../apps/server/migrations")).await;
    pool.close().await;
    sqlx::query(&format!("DROP DATABASE {name}")).execute(&admin).await.unwrap();
}
