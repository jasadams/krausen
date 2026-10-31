// SPDX-License-Identifier: AGPL-3.0-or-later

//! Recipe CRUD against a real database.
//!
//! Runs on an in-memory SQLite database with the real migrations applied, so
//! these exercise the actual SQL rather than a stand-in for it.

use krausen_beerxml::Recipe as RecipeDocument;
use krausen_brewery::recipe_service::{
    create_recipe, delete_recipe, get_recipe, list_recipes, restore_recipe, search_recipes,
    update_recipe, RecipeSource,
};
use krausen_core::DbPool;

/// A database with the schema and one workspace to hang recipes off.
async fn test_db() -> DbPool {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");

    sqlx::migrate!("../../apps/server/migrations-sqlite")
        .run(&pool)
        .await
        .expect("migrations apply");

    // Workspaces need an owner, and recipes reference a creator.
    sqlx::query("INSERT INTO users (user_id, email) VALUES ('user-1', 'brewer@example.test')")
        .execute(&pool)
        .await
        .expect("user inserted");
    for (id, name) in [("ws-test", "Test Brewery"), ("ws-other", "Someone Else")] {
        sqlx::query(
            "INSERT INTO workspaces (workspace_id, name, owner_user_id) VALUES (?, ?, 'user-1')",
        )
        .bind(id)
        .bind(name)
        .execute(&pool)
        .await
        .expect("workspace inserted");
    }

    DbPool::Sqlite(pool)
}

/// A recipe document, built by parsing — the same path an import takes.
fn document(name: &str) -> RecipeDocument {
    let xml = format!(
        r#"<RECIPE>
             <NAME>{name}</NAME>
             <VERSION>1</VERSION>
             <TYPE>All Grain</TYPE>
             <BATCH_SIZE>20.5</BATCH_SIZE>
             <BOIL_TIME>60.0</BOIL_TIME>
             <EFFICIENCY>78.0</EFFICIENCY>
             <STYLE><NAME>Munich Helles</NAME><VERSION>1</VERSION></STYLE>
             <FERMENTABLES><FERMENTABLE>
               <NAME>Pilsner</NAME><AMOUNT>4.2</AMOUNT><YIELD>80.0</YIELD><COLOR>3.0</COLOR>
             </FERMENTABLE></FERMENTABLES>
             <HOPS><HOP>
               <NAME>Hallertau</NAME><AMOUNT>0.03</AMOUNT><ALPHA>3.8</ALPHA>
               <TIME>60.0</TIME><USE>Boil</USE>
             </HOP></HOPS>
             <YEASTS><YEAST><NAME>S-23</NAME><ATTENUATION>82.0</ATTENUATION></YEAST></YEASTS>
             <MASH><NAME>Single Infusion</NAME><MASH_STEPS>
               <MASH_STEP><NAME>Sacch</NAME><STEP_TEMP>67.0</STEP_TEMP><INFUSE_AMOUNT>15.26</INFUSE_AMOUNT></MASH_STEP>
             </MASH_STEPS></MASH>
             <GRAINFATHER_PRIVATE_FIELD>keep me</GRAINFATHER_PRIVATE_FIELD>
           </RECIPE>"#
    );
    krausen_beerxml::parse(&xml).expect("parses").recipes.remove(0)
}

// ─────────────────────────────────────────────────────────────────────────
// Create and read
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_created_recipe_reads_back() {
    let db = test_db().await;
    let created = create_recipe(
        &db,
        "ws-test",
        &document("Munich Helles"),
        RecipeSource::Krausen,
        Some("user-1"),
    )
    .await
    .expect("creates");

    assert!(created.recipe_id.starts_with("rcp-"));
    assert_eq!(created.name, "Munich Helles");
    assert_eq!(created.workspace_id, "ws-test");
    assert_eq!(created.source, RecipeSource::Krausen);
    assert_eq!(created.created_by.as_deref(), Some("user-1"));

    let fetched = get_recipe(&db, "ws-test", &created.recipe_id)
        .await
        .expect("reads")
        .expect("is there");
    assert_eq!(fetched, created);
}

#[tokio::test]
async fn the_document_survives_the_database_intact() {
    // The whole reason for storing a document rather than normalised rows.
    let db = test_db().await;
    let original = document("Fidelity Test");

    let created = create_recipe(&db, "ws-test", &original, RecipeSource::BeerXml, None)
        .await
        .expect("creates");

    assert_eq!(created.document, original, "the document changed in storage");

    // Including the parts Krausen does not use, and an element the standard
    // has never heard of.
    let mash = created.document.mash.as_ref().expect("mash kept");
    assert_eq!(mash.steps.len(), 1);
    assert_eq!(mash.steps[0].infuse_amount().copied(), Some(15.26));
    assert!(
        created
            .document
            .header
            .extensions
            .iter()
            .any(|e| e.name == "GRAINFATHER_PRIVATE_FIELD"),
        "a vendor's own element must survive"
    );
}

#[tokio::test]
async fn a_stored_recipe_still_exports_as_beerxml() {
    // Round trip all the way out: parse -> store -> load -> write.
    let db = test_db().await;
    let created = create_recipe(&db, "ws-test", &document("Exportable"), RecipeSource::BeerXml, None)
        .await
        .expect("creates");

    let xml = krausen_beerxml::write_recipe_document(&created.document);
    let reparsed = krausen_beerxml::parse(&xml).expect("re-parses");
    assert_eq!(reparsed.recipes[0], created.document);
    assert!(xml.contains("GRAINFATHER_PRIVATE_FIELD"));
}

#[tokio::test]
async fn a_stored_recipe_derives() {
    let db = test_db().await;
    let created = create_recipe(&db, "ws-test", &document("Derivable"), RecipeSource::Krausen, None)
        .await
        .expect("creates");

    let converted = krausen_beerxml::convert::to_calc(&created.document);
    let equipment = converted.equipment.unwrap_or_default();
    let derived = krausen_calc::derive(&converted.recipe, &equipment).expect("derives");

    assert!(derived.original_gravity.points() > 0.0);
    assert!(derived.ibu_total > 0.0);
    // Mash water came from the infusion step.
    assert!((derived.volumes.mash_water_l - 15.26).abs() < 1e-9);
}

#[tokio::test]
async fn the_indexed_columns_come_from_the_document() {
    let db = test_db().await;
    let created = create_recipe(&db, "ws-test", &document("Indexed"), RecipeSource::Krausen, None)
        .await
        .expect("creates");

    assert_eq!(created.name, "Indexed");
    assert_eq!(created.style_name.as_deref(), Some("Munich Helles"));
    assert_eq!(created.batch_size_l, Some(20.5));
    assert_eq!(created.boil_minutes, Some(60));
}

#[tokio::test]
async fn a_nameless_recipe_still_gets_a_usable_name() {
    let db = test_db().await;
    let doc = krausen_beerxml::parse("<RECIPE><BATCH_SIZE>20.0</BATCH_SIZE></RECIPE>")
        .expect("parses")
        .recipes
        .remove(0);

    let created = create_recipe(&db, "ws-test", &doc, RecipeSource::Krausen, None)
        .await
        .expect("creates");
    assert_eq!(created.name, "Untitled recipe", "a list needs something to show");
}

#[tokio::test]
async fn an_unknown_id_reads_as_absent() {
    let db = test_db().await;
    assert!(get_recipe(&db, "ws-test", "rcp-nope")
        .await
        .expect("queries")
        .is_none());
}

// ─────────────────────────────────────────────────────────────────────────
// List and search
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn listing_returns_the_workspaces_recipes_newest_first() {
    let db = test_db().await;
    for name in ["First", "Second", "Third"] {
        create_recipe(&db, "ws-test", &document(name), RecipeSource::Krausen, None)
            .await
            .expect("creates");
    }

    let listed = list_recipes(&db, "ws-test").await.expect("lists");
    assert_eq!(listed.len(), 3);
    let names: Vec<&str> = listed.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"First") && names.contains(&"Third"));
}

#[tokio::test]
async fn search_matches_on_name_case_insensitively() {
    let db = test_db().await;
    for name in ["Munich Helles", "Dry Stout", "Helles Bock"] {
        create_recipe(&db, "ws-test", &document(name), RecipeSource::Krausen, None)
            .await
            .expect("creates");
    }

    let found = search_recipes(&db, "ws-test", "helles").await.expect("searches");
    assert_eq!(found.len(), 2, "found {:?}", found.iter().map(|r| &r.name).collect::<Vec<_>>());

    assert!(search_recipes(&db, "ws-test", "porter")
        .await
        .expect("searches")
        .is_empty());
}

// ─────────────────────────────────────────────────────────────────────────
// Update
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_update_replaces_the_document_and_reindexes() {
    let db = test_db().await;
    let created = create_recipe(&db, "ws-test", &document("Before"), RecipeSource::Krausen, None)
        .await
        .expect("creates");

    let updated = update_recipe(&db, "ws-test", &created.recipe_id, &document("After"))
        .await
        .expect("updates")
        .expect("found it");

    assert_eq!(updated.recipe_id, created.recipe_id, "same recipe");
    assert_eq!(updated.name, "After", "the indexed column follows the document");
    assert_eq!(updated.document.name(), "After");
    assert_eq!(updated.created_at, created.created_at, "creation time is not touched");
}

#[tokio::test]
async fn updating_an_unknown_recipe_reports_it_rather_than_creating_one() {
    let db = test_db().await;
    assert!(update_recipe(&db, "ws-test", "rcp-nope", &document("Ghost"))
        .await
        .expect("queries")
        .is_none());
    assert!(list_recipes(&db, "ws-test").await.expect("lists").is_empty());
}

// ─────────────────────────────────────────────────────────────────────────
// Delete
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_deleted_recipe_disappears_from_reads_and_lists() {
    let db = test_db().await;
    let created = create_recipe(&db, "ws-test", &document("Doomed"), RecipeSource::Krausen, None)
        .await
        .expect("creates");

    assert!(delete_recipe(&db, "ws-test", &created.recipe_id).await.expect("deletes"));

    assert!(get_recipe(&db, "ws-test", &created.recipe_id)
        .await
        .expect("queries")
        .is_none());
    assert!(list_recipes(&db, "ws-test").await.expect("lists").is_empty());
}

#[tokio::test]
async fn deleting_is_soft_so_a_mis_click_is_recoverable() {
    let db = test_db().await;
    let created = create_recipe(&db, "ws-test", &document("Rescued"), RecipeSource::Krausen, None)
        .await
        .expect("creates");

    delete_recipe(&db, "ws-test", &created.recipe_id).await.expect("deletes");
    assert!(restore_recipe(&db, "ws-test", &created.recipe_id).await.expect("restores"));

    let back = get_recipe(&db, "ws-test", &created.recipe_id)
        .await
        .expect("queries")
        .expect("is back");
    assert_eq!(back.document, created.document, "and it came back whole");
}

#[tokio::test]
async fn deleting_twice_reports_that_there_was_nothing_to_do() {
    let db = test_db().await;
    let created = create_recipe(&db, "ws-test", &document("Once"), RecipeSource::Krausen, None)
        .await
        .expect("creates");

    assert!(delete_recipe(&db, "ws-test", &created.recipe_id).await.expect("deletes"));
    assert!(!delete_recipe(&db, "ws-test", &created.recipe_id).await.expect("queries"));
}

// ─────────────────────────────────────────────────────────────────────────
// Workspace isolation
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_recipe_is_invisible_from_another_workspace() {
    let db = test_db().await;
    let created = create_recipe(&db, "ws-test", &document("Private"), RecipeSource::Krausen, None)
        .await
        .expect("creates");

    assert!(
        get_recipe(&db, "ws-other", &created.recipe_id)
            .await
            .expect("queries")
            .is_none(),
        "knowing the id must not be enough"
    );
    assert!(list_recipes(&db, "ws-other").await.expect("lists").is_empty());
}

#[tokio::test]
async fn another_workspace_cannot_update_or_delete_it() {
    let db = test_db().await;
    let created = create_recipe(&db, "ws-test", &document("Protected"), RecipeSource::Krausen, None)
        .await
        .expect("creates");

    assert!(update_recipe(&db, "ws-other", &created.recipe_id, &document("Hijacked"))
        .await
        .expect("queries")
        .is_none());
    assert!(!delete_recipe(&db, "ws-other", &created.recipe_id).await.expect("queries"));

    // And the original is untouched.
    let still = get_recipe(&db, "ws-test", &created.recipe_id)
        .await
        .expect("queries")
        .expect("still there");
    assert_eq!(still.name, "Protected");
}

// ─────────────────────────────────────────────────────────────────────────
// Import
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_whole_beerxml_file_imports() {
    const EXAMPLES: &str = include_str!("../../krausen-beerxml/tests/fixtures/synthetic.xml");

    let db = test_db().await;
    let doc = krausen_beerxml::parse(EXAMPLES).expect("parses");
    for recipe in &doc.recipes {
        create_recipe(&db, "ws-test", recipe, RecipeSource::BeerXml, Some("user-1"))
            .await
            .expect("creates");
    }

    let listed = list_recipes(&db, "ws-test").await.expect("lists");
    assert_eq!(listed.len(), doc.len());
    assert!(listed.iter().all(|r| r.source == RecipeSource::BeerXml));
    assert!(
        listed.iter().all(|r| !r.name.is_empty()),
        "every imported recipe should be named"
    );
}
