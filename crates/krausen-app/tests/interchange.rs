// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{ActorContext, interchange, recipe_editor};
use krausen_core::DbPool;
use krausen_types::{
    brewery::EquipmentSettingsV1,
    interchange::{
        ColourUnit, EfficiencyChoice, ExportOutcome, ExportRequest, ExportSelection, ImportChoices,
        ImportCommitOutcome, ImportPreviewOutcome, ImportRequest, OriginalOutcome,
    },
    recipe_editor::{RecipeReadOutcome, RecipeSaveOutcome},
    request_context::OperationIntent,
};
use uuid::Uuid;

async fn exercise(db: DbPool) {
    let suffix = Uuid::new_v4().to_string();
    let user = format!("user-{suffix}");
    let workspace = format!("workspace-{suffix}");
    let other = format!("other-{suffix}");
    let credential = format!("credential-{suffix}");
    macro_rules! run { ($sql:expr $(, $a:expr)*) => { match &db {
        DbPool::Postgres(p) => sqlx::query($sql)$(.bind($a))*.execute(p).await.unwrap().rows_affected(),
        DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($a))*.execute(p).await.unwrap().rows_affected(),
    } }; }
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2)",
        &user,
        format!("{suffix}@example.test")
    );
    for w in [&workspace, &other] {
        run!(
            "INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ($1,$2)",
            w,
            &user
        );
        run!(
            "INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ($1,$2,'member')",
            w,
            &user
        );
    }
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
    let intent = OperationIntent {
        account_id: user,
        workspace_id: workspace,
    };
    let xml = r#"<RECIPES vendor="x"><RECIPE><NAME>Original</NAME><BOIL_TIME>60.5</BOIL_TIME><EFFICIENCY>75</EFFICIENCY><FERMENTABLES><FERMENTABLE id="grain"><NAME>Malt</NAME><COLOR>3</COLOR></FERMENTABLE></FERMENTABLES></RECIPE></RECIPES>"#;
    let mut request = ImportRequest {
        intent: intent.clone(),
        xml: xml.into(),
        recipe_index: 0,
        choices: ImportChoices::default(),
        equipment_profile_id: None,
        expected_profile_revision: None,
        expected_profile_metadata_revision: None,
        equipment_draft: EquipmentSettingsV1::default(),
        draft_generation: "generation-1".into(),
    };
    assert!(matches!(
        interchange::preview(&db, &actor, &request).await,
        ImportPreviewOutcome::Preview(_)
    ));
    let mut too_large = request.clone();
    too_large.xml = "x".repeat(krausen_types::interchange::MAX_BEERXML_BYTES + 1);
    assert_eq!(
        interchange::preview(&db, &actor, &too_large).await,
        ImportPreviewOutcome::InvalidSource
    );
    assert_eq!(
        interchange::commit(&db, &actor, &too_large, Uuid::new_v4()).await,
        ImportCommitOutcome::InvalidSource
    );
    assert!(matches!(
        interchange::commit(&db, &actor, &request, Uuid::new_v4()).await,
        ImportCommitOutcome::ChoicesRequired(_)
    ));
    request.choices = ImportChoices {
        colour: Some(ColourUnit::Srm),
        efficiency: Some(EfficiencyChoice::RecipeOverride),
    };
    let key = Uuid::new_v4();
    let result = interchange::commit(&db, &actor, &request, key).await;
    let ImportCommitOutcome::Imported {
        recipe_id,
        revision: 1,
        source_sha256,
        ..
    } = result.clone()
    else {
        panic!("{result:?}");
    };
    assert_eq!(
        source_sha256,
        format!("{:x}", sha2::Sha256::digest(xml.as_bytes()))
    );
    let source_record: String = match &db {
        DbPool::Postgres(p) => sqlx::query_scalar("SELECT source_record FROM recipe_import_provenance WHERE workspace_id = $1 AND recipe_id = $2 AND revision = 1").bind(&intent.workspace_id).bind(&recipe_id).fetch_one(p).await.unwrap(),
        DbPool::Sqlite(p) => sqlx::query_scalar("SELECT source_record FROM recipe_import_provenance WHERE workspace_id = $1 AND recipe_id = $2 AND revision = 1").bind(&intent.workspace_id).bind(&recipe_id).fetch_one(p).await.unwrap(),
    };
    let metadata: serde_json::Value = serde_json::from_str(&source_record).unwrap();
    assert_eq!(metadata["choices"]["colour"], "srm");
    assert_eq!(metadata["source_recipe_index"], 0);
    assert!(
        metadata["source_elements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|element| element["row_id"] == "fermentable-1")
    );
    let mapping_version: String = match &db {
        DbPool::Postgres(p) => sqlx::query_scalar("SELECT mapping_version FROM recipe_import_provenance WHERE workspace_id = $1 AND recipe_id = $2 AND revision = 1").bind(&intent.workspace_id).bind(&recipe_id).fetch_one(p).await.unwrap(),
        DbPool::Sqlite(p) => sqlx::query_scalar("SELECT mapping_version FROM recipe_import_provenance WHERE workspace_id = $1 AND recipe_id = $2 AND revision = 1").bind(&intent.workspace_id).bind(&recipe_id).fetch_one(p).await.unwrap(),
    };
    assert_eq!(mapping_version, krausen_beerxml::native::MAPPING_VERSION);
    assert_eq!(
        interchange::commit(&db, &actor, &request, key).await,
        result
    );
    let mut changed = request.clone();
    changed.xml.push(' ');
    assert_eq!(
        interchange::commit(&db, &actor, &changed, key).await,
        ImportCommitOutcome::RequestIdentityConflict
    );
    assert_eq!(
        interchange::original(&db, &actor, &intent, &recipe_id).await,
        OriginalOutcome::Original {
            bytes: xml.as_bytes().to_vec(),
            sha256: format!("{:x}", sha2::Sha256::digest(xml.as_bytes()))
        }
    );
    let RecipeReadOutcome::Found(snapshot) =
        recipe_editor::read(&db, &actor, &intent, &recipe_id).await
    else {
        panic!("imported revision missing");
    };
    assert_eq!(snapshot.document.boil_minutes, Some(60.5));
    let summary: Option<i64> = match &db {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT boil_minutes::BIGINT FROM recipes WHERE workspace_id = $1 AND recipe_id = $2",
        )
        .bind(&intent.workspace_id)
        .bind(&recipe_id)
        .fetch_one(p)
        .await
        .unwrap(),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT boil_minutes FROM recipes WHERE workspace_id = $1 AND recipe_id = $2",
        )
        .bind(&intent.workspace_id)
        .bind(&recipe_id)
        .fetch_one(p)
        .await
        .unwrap(),
    };
    assert_eq!(
        summary, None,
        "fractional native minutes must not be silently truncated in legacy metadata"
    );
    let fractional_export = interchange::export(
        &db,
        &actor,
        &ExportRequest {
            intent: intent.clone(),
            selection: ExportSelection::Saved {
                recipe_id: recipe_id.clone(),
                revision: 1,
            },
            acknowledge_loss: true,
        },
    )
    .await;
    assert!(
        matches!(fractional_export, ExportOutcome::Exported { xml, .. } if xml.contains("<BOIL_TIME>60.5</BOIL_TIME>"))
    );
    let mut edited = snapshot.document;
    edited.title = "Edited".into();
    edited.boil_minutes = Some(2_147_483_648.0);
    assert!(matches!(
        recipe_editor::save(
            &db,
            &actor,
            &intent,
            Some(&recipe_id),
            Some(1),
            &edited,
            Uuid::new_v4()
        )
        .await,
        RecipeSaveOutcome::Saved { revision: 2, .. }
    ));
    let RecipeReadOutcome::Found(saved) =
        recipe_editor::read(&db, &actor, &intent, &recipe_id).await
    else {
        panic!("updated revision missing");
    };
    assert_eq!(saved.document.boil_minutes, Some(2_147_483_648.0));
    let summary: Option<i64> = match &db {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT boil_minutes::BIGINT FROM recipes WHERE workspace_id = $1 AND recipe_id = $2",
        )
        .bind(&intent.workspace_id)
        .bind(&recipe_id)
        .fetch_one(p)
        .await
        .unwrap(),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT boil_minutes FROM recipes WHERE workspace_id = $1 AND recipe_id = $2",
        )
        .bind(&intent.workspace_id)
        .bind(&recipe_id)
        .fetch_one(p)
        .await
        .unwrap(),
    };
    assert_eq!(
        summary, None,
        "out-of-range native minutes must not be saturated in legacy metadata"
    );
    let mut export = ExportRequest {
        intent: intent.clone(),
        selection: ExportSelection::Saved {
            recipe_id: recipe_id.clone(),
            revision: 2,
        },
        acknowledge_loss: false,
    };
    assert!(matches!(
        interchange::export(&db, &actor, &export).await,
        ExportOutcome::LossAcknowledgementRequired(_)
    ));
    export.acknowledge_loss = true;
    let ExportOutcome::Exported { xml: rendered, .. } =
        interchange::export(&db, &actor, &export).await
    else {
        panic!("export failed");
    };
    assert!(rendered.contains("<NAME>Edited</NAME>"));
    assert!(rendered.contains("<BOIL_TIME>2147483648.0</BOIL_TIME>"));
    assert!(!rendered.contains("<NAME>Original</NAME>"));
    let foreign = ActorContext {
        workspace_id: other.clone(),
        ..actor.clone()
    };
    let foreign_intent = OperationIntent {
        workspace_id: other,
        ..intent.clone()
    };
    assert_eq!(
        interchange::preview(&db, &foreign, &request).await,
        ImportPreviewOutcome::ContextChanged
    );
    assert_eq!(
        interchange::commit(&db, &foreign, &request, Uuid::new_v4()).await,
        ImportCommitOutcome::ContextChanged
    );
    let expired = ActorContext {
        access_expires_at: chrono::Utc::now().timestamp() - 1,
        ..actor.clone()
    };
    assert_eq!(
        interchange::preview(&db, &expired, &request).await,
        ImportPreviewOutcome::Forbidden
    );
    assert_eq!(
        interchange::commit(&db, &expired, &request, Uuid::new_v4()).await,
        ImportCommitOutcome::Forbidden
    );
    assert_eq!(
        interchange::original(&db, &foreign, &foreign_intent, &recipe_id).await,
        OriginalOutcome::NotFound
    );
    let foreign_export = ExportRequest {
        intent: foreign_intent.clone(),
        selection: ExportSelection::Saved {
            recipe_id: recipe_id.clone(),
            revision: 2,
        },
        acknowledge_loss: true,
    };
    assert_eq!(
        interchange::export(&db, &foreign, &foreign_export).await,
        ExportOutcome::NotFound
    );
    assert_eq!(
        interchange::original(&db, &foreign, &intent, &recipe_id).await,
        OriginalOutcome::ContextChanged
    );
    let profile_id = Uuid::new_v4().to_string();
    let mut profile = krausen_types::equipment::SaveEquipment {
        intent: intent.clone(),
        profile_id: profile_id.clone(),
        name: "Import equipment".into(),
        settings: EquipmentSettingsV1::default(),
        provenance: krausen_types::equipment::EquipmentProvenance {
            source: "manual".into(),
            assumptions: vec![],
        },
        expected_revision: 0,
        expected_metadata_revision: 0,
        request_id: Uuid::new_v4().to_string(),
    };
    let created = krausen_app::equipment::save(&db, &actor, profile.clone())
        .await
        .unwrap();
    let mut with_profile = request.clone();
    with_profile.equipment_profile_id = Some(profile_id);
    with_profile.expected_profile_revision = Some(created.revision);
    with_profile.expected_profile_metadata_revision = Some(created.metadata_revision);
    let ImportPreviewOutcome::Preview(profile_preview) =
        interchange::preview(&db, &actor, &with_profile).await
    else {
        panic!("profile preview failed");
    };
    assert_eq!(
        profile_preview.selected_profile_revision,
        Some(created.revision)
    );
    assert_eq!(
        profile_preview.selected_profile_metadata_revision,
        Some(created.metadata_revision)
    );
    let mut unknown_style = with_profile.clone();
    unknown_style.xml = unknown_style.xml.replace(
        "</RECIPE>",
        "<STYLE><NAME>Unknown style</NAME></STYLE></RECIPE>",
    );
    let ImportPreviewOutcome::Preview(unresolved) =
        interchange::preview(&db, &actor, &unknown_style).await
    else {
        panic!("unresolved style preview failed");
    };
    assert!(matches!(
        unresolved.calculation,
        krausen_types::formulation::PreviewOutcome::UnresolvedStyle
    ));
    assert_eq!(unresolved.selected_profile_revision, Some(created.revision));
    profile.expected_revision = created.revision;
    profile.expected_metadata_revision = created.metadata_revision;
    profile.request_id = Uuid::new_v4().to_string();
    profile.settings.boil_off_l_per_hour = Some(2.0);
    let changed_profile = krausen_app::equipment::save(&db, &actor, profile)
        .await
        .unwrap();
    assert_eq!(
        interchange::commit(&db, &actor, &with_profile, Uuid::new_v4()).await,
        ImportCommitOutcome::ProfileChanged
    );
    with_profile.expected_profile_revision = Some(changed_profile.revision);
    with_profile.expected_profile_metadata_revision = Some(changed_profile.metadata_revision);
    assert!(matches!(
        interchange::commit(&db, &actor, &with_profile, Uuid::new_v4()).await,
        ImportCommitOutcome::Imported { revision: 1, .. }
    ));
    let trigger = format!("kra17_{}", suffix.replace('-', ""));
    if db.is_postgres() {
        run!(&format!(
            "CREATE FUNCTION {trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected failure'; END $$"
        ));
        run!(&format!(
            "CREATE TRIGGER {trigger} BEFORE INSERT ON app_changes FOR EACH ROW WHEN (NEW.workspace_id = '{}') EXECUTE FUNCTION {trigger}()",
            intent.workspace_id
        ));
    } else {
        run!(&format!(
            "CREATE TRIGGER {trigger} BEFORE INSERT ON app_changes WHEN NEW.workspace_id = '{}' BEGIN SELECT RAISE(ABORT, 'injected failure'); END",
            intent.workspace_id
        ));
    }
    let failed_key = Uuid::new_v4();
    assert_eq!(
        interchange::commit(&db, &actor, &request, failed_key).await,
        ImportCommitOutcome::Unavailable
    );
    let attachments: i64 = match &db {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM recipe_import_provenance WHERE workspace_id = $1",
        )
        .bind(&intent.workspace_id)
        .fetch_one(p)
        .await
        .unwrap(),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM recipe_import_provenance WHERE workspace_id = $1",
        )
        .bind(&intent.workspace_id)
        .fetch_one(p)
        .await
        .unwrap(),
    };
    assert_eq!(
        attachments, 2,
        "failed import must not leave an attachment or partial revision"
    );
    if db.is_postgres() {
        run!(&format!("DROP TRIGGER {trigger} ON app_changes"));
        run!(&format!("DROP FUNCTION {trigger}()"));
    } else {
        run!(&format!("DROP TRIGGER {trigger}"));
    }
    assert!(matches!(
        interchange::commit(&db, &actor, &request, failed_key).await,
        ImportCommitOutcome::Imported { revision: 1, .. }
    ));
}

use sha2::Digest;

#[tokio::test]
async fn sqlite_interchange() {
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
async fn postgres_interchange() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL")
        .expect("KRAUSEN_TEST_POSTGRES_URL is required for PostgreSQL tests");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
