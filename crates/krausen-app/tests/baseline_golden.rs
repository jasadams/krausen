// SPDX-License-Identifier: AGPL-3.0-or-later
//! Baseline-generated v1 context, numeric parity, and explicit v1 refusal.
use krausen_app::{ActorContext, brew_start, formulation, recipe_editor};
use krausen_core::{DbPool, canonical_json};
use krausen_types::{
    brew::{StartBrewOutcome, StartBrewRequest},
    brewery::RecipeDocumentV1,
    formulation::{CalculationContext, PreviewOutcome, PreviewRequest},
    recipe_editor::RecipeSaveOutcome,
    request_context::OperationIntent,
};
use serde_json::Value;
use uuid::Uuid;

const CONTEXT: &str =
    include_str!("fixtures/kra20_baseline_4fc8313/baseline_4fc8313_context.pretty.json");
const DIGEST: &str = include_str!("fixtures/kra20_baseline_4fc8313/baseline_4fc8313_digest.txt");
const FIXTURE: &str = include_str!("fixtures/kra20_baseline_4fc8313/baseline_4fc8313_fixture.json");
const TARGETS: &str = include_str!("fixtures/kra20_baseline_4fc8313/baseline_4fc8313_targets.json");
const VALIDATION: &str =
    include_str!("fixtures/kra20_baseline_4fc8313/baseline_4fc8313_validation.json");
const STYLE: &str =
    include_str!("fixtures/kra20_baseline_4fc8313/baseline_4fc8313_style_comparison.json");

#[tokio::test]
async fn baseline_v1_golden_refused_and_v2_matches_targets() {
    // Hash the raw v1 JSON before current-type deserialization adds defaults.
    let raw: Value = serde_json::from_str(CONTEXT).unwrap();
    assert!(raw.get("applied_defaults").is_none());
    assert_eq!(raw["policy_version"], "krausen-formulation-v1");
    assert_eq!(
        raw["policy_source_hash"],
        "c2195592083a8abe4b735d78aa22d7006adb773046d480b78b6462d2699d3bb9"
    );
    let raw_digest =
        canonical_json::sha256(&canonical_json::from_serializable(&raw).unwrap()).unwrap();
    assert_eq!(raw_digest, DIGEST.trim());
    let v1: CalculationContext = serde_json::from_value(raw.clone()).unwrap();
    assert!(v1.applied_defaults.is_empty());

    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let profile = &fixture["profile"];
    let document: RecipeDocumentV1 = serde_json::from_value(fixture["document"].clone()).unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../apps/server/migrations-sqlite")
        .run(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (user_id,email) VALUES ('kra20-baseline-user','kra20-baseline-user@example.test')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ('kra20-baseline-workspace','kra20-baseline-user')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ('kra20-baseline-workspace','kra20-baseline-user','workspace_member')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ('kra20-baseline-credential','kra20-baseline-user','browser','test','test',4102444800)").execute(&pool).await.unwrap();
    let profile_id = profile["profile_id"].as_str().unwrap();
    sqlx::query("INSERT INTO equipment_profiles (workspace_id,profile_id,name,current_revision,metadata_revision,archived) VALUES ($1,$2,$3,1,1,0)")
        .bind("kra20-baseline-workspace").bind(profile_id).bind(profile["name"].as_str().unwrap()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO equipment_revisions (workspace_id,profile_id,revision,name,schema_version,settings_json,provenance_json,content_digest,actor_id,recorded_at) VALUES ($1,$2,1,$3,1,$4,$5,$6,$7,$8)")
        .bind("kra20-baseline-workspace").bind(profile_id).bind(profile["name"].as_str().unwrap())
        .bind(serde_json::to_string(&profile["settings"]).unwrap()).bind(serde_json::to_string(&profile["provenance"]).unwrap())
        .bind("baseline-fixture").bind("kra20-baseline-user").bind(profile["saved_at"].as_i64().unwrap()).execute(&pool).await.unwrap();
    let db = DbPool::Sqlite(pool);
    let actor = ActorContext {
        actor_id: "kra20-baseline-user".into(),
        workspace_id: "kra20-baseline-workspace".into(),
        credential_id: "kra20-baseline-credential".into(),
        access_expires_at: 4102444800,
    };
    let intent = OperationIntent {
        account_id: actor.actor_id.clone(),
        workspace_id: actor.workspace_id.clone(),
    };
    let RecipeSaveOutcome::Saved {
        recipe_id,
        revision,
    } = recipe_editor::save(&db, &actor, &intent, None, None, &document, Uuid::new_v4()).await
    else {
        panic!("save golden recipe")
    };
    let PreviewOutcome::Preview(preview) = formulation::preview(
        &db,
        &actor,
        PreviewRequest {
            draft_generation: "kra20-baseline-golden".into(),
            intent: intent.clone(),
            draft: document,
            equipment_draft: Default::default(),
        },
    )
    .await
    .unwrap() else {
        panic!("v2 preview")
    };
    assert_eq!(
        serde_json::to_value(&preview.targets).unwrap(),
        serde_json::from_str::<Value>(TARGETS).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&preview.validation).unwrap(),
        serde_json::from_str::<Value>(VALIDATION).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&preview.style_comparison).unwrap(),
        serde_json::from_str::<Value>(STYLE).unwrap()
    );
    let context = preview.context.as_ref().unwrap();
    assert_eq!(context.equipment_profile, v1.equipment_profile);
    assert!(
        serde_json::to_value(context)
            .unwrap()
            .get("applied_defaults")
            .is_some()
    );
    assert_eq!(context.policy_version, "krausen-formulation-v2");
    let request = |digest: String| StartBrewRequest {
        intent: intent.clone(),
        recipe_id: recipe_id.clone(),
        selected_recipe_revision: revision,
        expected_head_revision: Some(revision),
        expected_calculation_context: digest,
        request_id: Uuid::new_v4().to_string(),
    };
    let StartBrewOutcome::ContextConflict { refreshed } =
        brew_start::start(&db, &actor, request(raw_digest)).await
    else {
        panic!("baseline v1 must conflict")
    };
    assert_eq!(
        refreshed.context.as_ref().unwrap().policy_version,
        "krausen-formulation-v2"
    );
    assert_eq!(refreshed.targets, preview.targets);
    assert_eq!(refreshed.context_digest, preview.context_digest);
    let StartBrewOutcome::Started { plan, .. } =
        brew_start::start(&db, &actor, request(refreshed.context_digest.unwrap())).await
    else {
        panic!("refreshed v2 start")
    };
    assert_eq!(plan.original_targets, preview.targets);
    assert_eq!(&plan.calculation_context, context);
}
