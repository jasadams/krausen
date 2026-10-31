// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{ActorContext, Error, formulation::preview};
use krausen_core::DbPool;
use krausen_types::{
    brewery::*,
    formulation::{PreviewOutcome, PreviewRequest},
    validation::Readiness,
};

fn request() -> PreviewRequest {
    let mut d = RecipeDocumentV1::default();
    d.title = "Reference".into();
    d.batch_size_l = Some(20.0);
    d.boil_minutes = Some(60.0);
    d.fermentables.push(FermentableAddition {
        row_id: "one".into(),
        amount_kg: Some(4.2),
        addition: AdditionStage {
            stage: "mash".into(),
            time_minutes: None,
        },
        specification: FermentableSpecification {
            name: Some("Malt".into()),
            producer: None,
            kind: Some("grain".into()),
            yield_fraction: Some(300.0 / 385.674500456),
            colour_ebc: Some(5.0),
        },
        source: None,
    });
    d.process.apparent_attenuation_fraction = Some(0.8);
    d.process.mash_water_method = Some("manual".into());
    d.process.mash_water_l = Some(15.0);
    let mut e = EquipmentSettingsV1::default();
    e.mash_efficiency_fraction = Some(0.75);
    e.boil_off_l_per_hour = Some(3.0);
    e.transfer_loss_l = Some(2.0);
    e.pre_boil_deadspace_l = Some(0.0);
    e.grain_absorption_l_per_kg = Some(0.8);
    PreviewRequest {
        draft_generation: "gen-1".into(),
        draft: d,
        equipment_draft: e,
    }
}
async fn exercise(db: DbPool) {
    let id = uuid::Uuid::new_v4().to_string();
    let user = format!("u-{id}");
    let ws = format!("w-{id}");
    macro_rules! run {($sql:expr $(,$arg:expr)*)=>{match &db {DbPool::Sqlite(p)=>sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),DbPool::Postgres(p)=>sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected()}};}
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2)",
        &user,
        format!("{id}@example.test")
    );
    run!(
        "INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ($1,$2)",
        &ws,
        &user
    );
    run!(
        "INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ($1,$2,'workspace_member')",
        &ws,
        &user
    );
    let actor = ActorContext {
        actor_id: user,
        workspace_id: ws,
    };
    let choices = krausen_app::formulation::list_styles(&db, &actor)
        .await
        .unwrap();
    assert_eq!(choices.len(), 3);
    assert_eq!(choices[0].style_id.as_deref(), Some("4A"));
    let req = request();
    let PreviewOutcome::Preview(first) = preview(&db, &actor, req.clone()).await.unwrap() else {
        panic!("preview")
    };
    assert_eq!(first.validation.state, Readiness::Ready);
    assert_eq!(first.draft_generation, "gen-1");
    assert!((first.targets.original_sg.unwrap() - 1.0429545454545455).abs() < 1e-9);
    assert_eq!(first.targets.ibu, Some(0.0));
    assert!(first.context_digest.is_some());
    let mut styled = req.clone();
    styled.draft.style = Some(StyleReference {
        name: "Munich Helles".into(),
        edition: Some("BJCP 2021".into()),
        style_id: Some("4A".into()),
    });
    let PreviewOutcome::Preview(with_style) = preview(&db, &actor, styled.clone()).await.unwrap()
    else {
        panic!("style preview")
    };
    let ranges = with_style.style.unwrap();
    assert_eq!(ranges.reference_version, "bjcp-2021-initial-v1");
    assert_eq!(
        (ranges.original_sg.min, ranges.original_sg.max),
        (1.044, 1.048)
    );
    assert_ne!(with_style.context_digest, first.context_digest);
    styled.draft.style.as_mut().unwrap().edition = Some("old".into());
    assert!(matches!(
        preview(&db, &actor, styled).await.unwrap(),
        PreviewOutcome::UnresolvedStyle
    ));
    let mut generation_only = req.clone();
    generation_only.draft_generation = "new-generation".into();
    let PreviewOutcome::Preview(correlated) = preview(&db, &actor, generation_only).await.unwrap()
    else {
        panic!("preview")
    };
    assert_eq!(first.context_digest, correlated.context_digest);
    assert_eq!(correlated.draft_generation, "new-generation");
    let mut changed = req.clone();
    changed.equipment_draft.boil_off_l_per_hour = Some(4.0);
    changed.draft_generation = "gen-2".into();
    let PreviewOutcome::Preview(second) = preview(&db, &actor, changed).await.unwrap() else {
        panic!("preview")
    };
    assert_ne!(first.context_digest, second.context_digest);
    let mut missing = req.clone();
    missing.draft.process.apparent_attenuation_fraction = None;
    let PreviewOutcome::Preview(partial) = preview(&db, &actor, missing).await.unwrap() else {
        panic!("preview")
    };
    assert!(partial.targets.original_sg.is_some());
    assert!(partial.targets.total_water_l.is_some());
    assert_eq!(partial.targets.final_sg, None);
    assert_eq!(partial.targets.abv_percent, None);
    let mut missing_hop = req.clone();
    missing_hop.draft.hops.push(HopAddition {
        row_id: "hop".into(),
        amount_g: Some(20.0),
        stage: "boil".into(),
        contact_minutes: Some(30.0),
        temperature_c: None,
        specification: HopSpecification {
            name: None,
            producer: None,
            alpha_fraction: None,
        },
        source: None,
    });
    let PreviewOutcome::Preview(partial_hop) = preview(&db, &actor, missing_hop).await.unwrap()
    else {
        panic!("preview")
    };
    assert!(partial_hop.targets.original_sg.is_some());
    assert!(partial_hop.targets.total_water_l.is_some());
    assert!(partial_hop.targets.ibu.is_none());
    let mut missing_yield = req.clone();
    missing_yield.draft.fermentables[0]
        .specification
        .yield_fraction = None;
    let PreviewOutcome::Preview(partial_yield) = preview(&db, &actor, missing_yield).await.unwrap()
    else {
        panic!("preview")
    };
    assert!(partial_yield.targets.total_water_l.is_some());
    assert!(partial_yield.targets.original_sg.is_none());
    assert!(partial_yield.targets.colour_ebc.is_some());
    assert_eq!(partial_yield.targets.ibu, Some(0.0));
    let mut extreme = req.clone();
    extreme.draft.fermentables[0].amount_kg = Some(1e300);
    let PreviewOutcome::Preview(extreme_result) = preview(&db, &actor, extreme).await.unwrap()
    else {
        panic!("preview")
    };
    assert_eq!(extreme_result.validation.state, Readiness::Invalid);
    assert!(extreme_result.targets.original_sg.is_none());
    let mut profile = req.clone();
    profile.draft.equipment_profile_id = Some("foreign-or-unresolved".into());
    assert!(matches!(
        preview(&db, &actor, profile).await.unwrap(),
        PreviewOutcome::UnresolvedProfile
    ));
    let stranger = ActorContext {
        actor_id: "stranger".into(),
        workspace_id: actor.workspace_id.clone(),
    };
    assert!(matches!(
        krausen_app::formulation::list_styles(&db, &stranger).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        preview(&db, &stranger, req).await,
        Err(Error::Forbidden)
    ));
}
#[tokio::test]
async fn sqlite_preview() {
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
async fn postgres_preview() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
