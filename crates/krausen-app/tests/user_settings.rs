// SPDX-License-Identifier: AGPL-3.0-or-later
//! KRA-74: a per-user default batch size that every new recipe starts with. Both backends.
use krausen_app::{ActorContext, Error, interchange, recipe_editor, user_settings};
use krausen_core::DbPool;
use krausen_types::{
    brewery::RecipeDocumentV1,
    interchange::{
        ColourUnit, EfficiencyChoice, ImportChoices, ImportCommitOutcome, ImportRequest,
    },
    recipe_editor::{RecipeReadOutcome, RecipeSaveOutcome},
    request_context::OperationIntent,
    user_settings::UserSettings,
};
use uuid::Uuid;

struct Account {
    actor: ActorContext,
    intent: OperationIntent,
}

async fn account(db: &DbPool, workspace: &str) -> Account {
    let user = format!("u-{}", Uuid::new_v4());
    macro_rules! run {($sql:expr $(,$arg:expr)*)=>{match db {DbPool::Sqlite(p)=>sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),DbPool::Postgres(p)=>sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected()}};}
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2)",
        &user,
        format!("{user}@example.test")
    );
    run!(
        "INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ($1,$2,'workspace_user')",
        workspace,
        &user
    );
    Account {
        actor: ActorContext {
            actor_id: user.clone(),
            workspace_id: workspace.into(),
            session_iat_us: Some(chrono::Utc::now().timestamp_micros()),
            access_expires_at: 4102444800,
        },
        intent: OperationIntent {
            account_id: user,
            workspace_id: workspace.into(),
        },
    }
}

async fn create(db: &DbPool, who: &Account, document: &RecipeDocumentV1) -> (String, Option<f64>) {
    let outcome = recipe_editor::save(db, &who.actor, &who.intent, None, None, document, Uuid::new_v4()).await;
    let RecipeSaveOutcome::Saved { recipe_id, revision: 1, default_batch_size_l, .. } = outcome else { panic!("create: {outcome:?}") };
    (recipe_id, default_batch_size_l)
}

async fn stored_batch(db: &DbPool, who: &Account, id: &str) -> Option<f64> {
    let RecipeReadOutcome::Found(found) = recipe_editor::read(db, &who.actor, &who.intent, id).await else { panic!("read") };
    found.document.batch_size_l
}

async fn set(db: &DbPool, who: &Account, litres: Option<f64>) -> Result<UserSettings, Error> {
    user_settings::save(db, &who.actor, &who.intent, &UserSettings { default_batch_size_l: litres }).await
}

async fn exercise(db: DbPool) {
    let ws = format!("w-{}", Uuid::new_v4());
    let owner = format!("o-{}", Uuid::new_v4());
    macro_rules! run {($sql:expr $(,$arg:expr)*)=>{match &db {DbPool::Sqlite(p)=>sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),DbPool::Postgres(p)=>sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected()}};}
    run!("INSERT INTO users (user_id,email) VALUES ($1,$2)", &owner, format!("{owner}@example.test"));
    run!("INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ($1,$2)", &ws, &owner);
    let jason = account(&db, &ws).await;
    let other = account(&db, &ws).await;
    let blank = |title: &str| RecipeDocumentV1 { title: title.into(), ..Default::default() };

    // The theme preference lives in the same JSON; settings must never disturb it.
    match &db {
        DbPool::Postgres(p) => sqlx::query("UPDATE users SET extra_metadata = CAST($1 AS json) WHERE user_id = $2").bind(r#"{"theme":"dark"}"#).bind(&jason.actor.actor_id).execute(p).await.unwrap().rows_affected(),
        DbPool::Sqlite(p) => sqlx::query("UPDATE users SET extra_metadata = $1 WHERE user_id = $2").bind(r#"{"theme":"dark"}"#).bind(&jason.actor.actor_id).execute(p).await.unwrap().rows_affected(),
    };

    // With no default set, a new recipe behaves as today: no batch size, nothing applied.
    assert_eq!(user_settings::get(&db, &jason.actor, &jason.intent).await.unwrap(), UserSettings::default());
    let (none_id, applied) = create(&db, &jason, &blank("No default")).await;
    assert_eq!((stored_batch(&db, &jason, &none_id).await, applied), (None, None));

    // Set: a blank new recipe starts with it, and the result says so.
    let saved = set(&db, &jason, Some(23.0)).await.unwrap();
    assert_eq!(saved.default_batch_size_l, Some(23.0));
    assert_eq!(user_settings::get(&db, &jason.actor, &jason.intent).await.unwrap().default_batch_size_l, Some(23.0));
    let request = Uuid::new_v4();
    let first = recipe_editor::save(&db, &jason.actor, &jason.intent, None, None, &blank("Defaulted"), request).await;
    let RecipeSaveOutcome::Saved { recipe_id: defaulted_id, default_batch_size_l, .. } = first.clone() else { panic!("{first:?}") };
    assert_eq!(default_batch_size_l, Some(23.0));
    assert_eq!(stored_batch(&db, &jason, &defaulted_id).await, Some(23.0));

    // A recipe that names its own batch size keeps it and reports nothing applied.
    let own = RecipeDocumentV1 { title: "Own".into(), batch_size_l: Some(18.0), ..Default::default() };
    let (own_id, applied) = create(&db, &jason, &own).await;
    assert_eq!((stored_batch(&db, &jason, &own_id).await, applied), (Some(18.0), None));

    // Change: new recipes take the new value; existing recipes keep theirs; a retry after the
    // change recovers the original result; editing a saved recipe never applies the default.
    set(&db, &jason, Some(20.0)).await.unwrap();
    assert_eq!(recipe_editor::save(&db, &jason.actor, &jason.intent, None, None, &blank("Defaulted"), request).await, first);
    assert_eq!(stored_batch(&db, &jason, &defaulted_id).await, Some(23.0));
    let (changed_id, applied) = create(&db, &jason, &blank("After change")).await;
    assert_eq!((stored_batch(&db, &jason, &changed_id).await, applied), (Some(20.0), Some(20.0)));
    let RecipeReadOutcome::Found(found) = recipe_editor::read(&db, &jason.actor, &jason.intent, &none_id).await else { panic!("read") };
    let edited = RecipeDocumentV1 { title: "No default, edited".into(), ..found.document };
    let outcome = recipe_editor::save(&db, &jason.actor, &jason.intent, Some(&none_id), Some(found.revision), &edited, Uuid::new_v4()).await;
    let RecipeSaveOutcome::Saved { default_batch_size_l: edit_applied, .. } = outcome else { panic!("{outcome:?}") };
    assert_eq!((edit_applied, stored_batch(&db, &jason, &none_id).await), (None, None));

    // The default belongs to the user: another account in the same workspace is unaffected.
    assert_eq!(user_settings::get(&db, &other.actor, &other.intent).await.unwrap(), UserSettings::default());
    let (other_id, applied) = create(&db, &other, &blank("Other's")).await;
    assert_eq!((stored_batch(&db, &other, &other_id).await, applied), (None, None));

    // A BeerXML import keeps what the file says (here: no batch size), not the default.
    let xml = r#"<RECIPES vendor="x"><RECIPE><NAME>Imported</NAME><BOIL_TIME>60</BOIL_TIME><EFFICIENCY>75</EFFICIENCY><FERMENTABLES><FERMENTABLE id="grain"><NAME>Malt</NAME><COLOR>3</COLOR></FERMENTABLE></FERMENTABLES></RECIPE></RECIPES>"#;
    let import = ImportRequest {
        intent: jason.intent.clone(),
        xml: xml.into(),
        recipe_index: 0,
        choices: ImportChoices { colour: Some(ColourUnit::Srm), efficiency: Some(EfficiencyChoice::RecipeOverride) },
        equipment_profile_id: None,
        expected_profile_revision: None,
        expected_profile_metadata_revision: None,
        expected_profile_id: None,
        expected_no_profile: false,
        equipment_draft: Default::default(),
        draft_generation: "generation-1".into(),
    };
    let ImportCommitOutcome::Imported { recipe_id: imported_id, .. } = interchange::commit(&db, &jason.actor, &import, Uuid::new_v4()).await else { panic!("import") };
    assert_eq!(stored_batch(&db, &jason, &imported_id).await, None);

    // Invalid values are refused and leave the stored default alone.
    for bad in [0.0, -5.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(set(&db, &jason, Some(bad)).await, Err(Error::Invalid(_))), "{bad}");
    }
    assert_eq!(user_settings::get(&db, &jason.actor, &jason.intent).await.unwrap().default_batch_size_l, Some(20.0));
    // A stale or foreign context is refused.
    let stale = OperationIntent { workspace_id: "elsewhere".into(), ..jason.intent.clone() };
    assert!(matches!(user_settings::save(&db, &jason.actor, &stale, &UserSettings { default_batch_size_l: Some(9.0) }).await, Err(Error::ContextChanged)));

    // Unset: back to today's behaviour; existing recipes keep their value.
    assert_eq!(set(&db, &jason, None).await.unwrap(), UserSettings::default());
    assert_eq!(user_settings::get(&db, &jason.actor, &jason.intent).await.unwrap(), UserSettings::default());
    let (after_unset, applied) = create(&db, &jason, &blank("After unset")).await;
    assert_eq!((stored_batch(&db, &jason, &after_unset).await, applied), (None, None));
    assert_eq!(stored_batch(&db, &jason, &changed_id).await, Some(20.0));

    // The theme preference sat through all of it.
    let metadata: String = match &db {
        DbPool::Sqlite(p) => sqlx::query_scalar("SELECT CAST(extra_metadata AS TEXT) FROM users WHERE user_id = $1").bind(&jason.actor.actor_id).fetch_one(p).await.unwrap(),
        DbPool::Postgres(p) => sqlx::query_scalar("SELECT CAST(extra_metadata AS TEXT) FROM users WHERE user_id = $1").bind(&jason.actor.actor_id).fetch_one(p).await.unwrap(),
    };
    let metadata: serde_json::Value = serde_json::from_str(&metadata).unwrap();
    assert_eq!(metadata["theme"], "dark");
    assert!(metadata.get("default_batch_size_l").is_none());
}

#[tokio::test]
async fn sqlite_user_settings() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("../../apps/server/migrations-sqlite").run(&pool).await.unwrap();
    exercise(DbPool::Sqlite(pool)).await;
}
#[tokio::test]
async fn postgres_user_settings() {
    let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations").run(&pool).await.unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
