// SPDX-License-Identifier: AGPL-3.0-or-later

//! Recipe storage.
//!
//! A recipe is stored as one document plus the few columns a list view needs.
//! Nothing queries an individual hop, so normalising the ingredients into rows
//! would buy joins and lose fidelity — this way a mash schedule, a water
//! profile and an exporter's own private elements all survive a round trip
//! through the database exactly as they arrived.
//!
//! No derived values are stored. OG, colour and bitterness are a pure function
//! of the recipe and an equipment profile, and a stored copy of a derived
//! number is a copy that can go stale.

use chrono::{DateTime, Utc};
use krausen_beerxml::Recipe as RecipeDocument;
use krausen_core::{DbPool, db_execute, db_fetch_all, db_fetch_optional, sql_compat};
use serde::{Deserialize, Serialize};

/// Where a recipe came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipeSource {
    /// Written in Krausen.
    Krausen,
    /// Imported from a BeerXML file.
    BeerXml,
}

impl RecipeSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Krausen => "krausen",
            Self::BeerXml => "beerxml",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "beerxml" => Self::BeerXml,
            _ => Self::Krausen,
        }
    }
}

/// A stored recipe.
#[derive(Debug, Clone, PartialEq)]
pub struct Recipe {
    pub recipe_id: String,
    pub workspace_id: String,
    pub name: String,
    pub style_name: Option<String>,
    pub batch_size_l: Option<f64>,
    pub boil_minutes: Option<i64>,
    /// The complete recipe.
    pub document: RecipeDocument,
    pub source: RecipeSource,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub native_migration_error: Option<String>,
}

/// A recipe as the list view needs it — everything but the document.
///
/// Listing a hundred recipes should not mean deserialising a hundred full
/// documents to show a hundred names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeSummary {
    pub recipe_id: String,
    pub name: String,
    pub style_name: Option<String>,
    pub batch_size_l: Option<f64>,
    pub boil_minutes: Option<i64>,
    pub source: RecipeSource,
    pub updated_at: DateTime<Utc>,
}

/// Recoverable legacy payload for a row whose native migration failed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct LegacyRecovery {
    pub document: String,
    pub native_migration_error: String,
}

pub async fn get_legacy_recovery(
    pool: &DbPool,
    workspace_id: &str,
    recipe_id: &str,
) -> krausen_core::Result<Option<LegacyRecovery>> {
    let sql = "SELECT document, native_migration_error FROM recipes WHERE workspace_id = $1 AND recipe_id = $2 AND native_head_revision = 0 AND native_migration_error IS NOT NULL";
    Ok(db_fetch_optional!(
        pool,
        LegacyRecovery,
        sql,
        workspace_id,
        recipe_id
    )?)
}

/// The row shape shared by both queries, before the document is parsed.
#[derive(sqlx::FromRow)]
struct RecipeRow {
    recipe_id: String,
    workspace_id: String,
    name: String,
    style_name: Option<String>,
    batch_size_l: Option<f64>,
    boil_minutes: Option<i64>,
    document: String,
    source: String,
    created_by: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    native_head_revision: i64,
    native_migration_error: Option<String>,
}

#[derive(sqlx::FromRow)]
struct SummaryRow {
    recipe_id: String,
    name: String,
    style_name: Option<String>,
    batch_size_l: Option<f64>,
    boil_minutes: Option<i64>,
    source: String,
    updated_at: DateTime<Utc>,
}

impl TryFrom<RecipeRow> for Recipe {
    type Error = krausen_core::Error;

    fn try_from(row: RecipeRow) -> Result<Self, Self::Error> {
        if row.native_head_revision > 0 {
            return Err(krausen_core::Error::Conflict(
                "This recipe uses native revisions and cannot be opened by the legacy editor"
                    .into(),
            ));
        }
        let document: RecipeDocument = serde_json::from_str(&row.document).map_err(|e| {
            krausen_core::Error::Internal(format!(
                "recipe {} has an unreadable document: {e}",
                row.recipe_id
            ))
        })?;

        Ok(Self {
            recipe_id: row.recipe_id,
            workspace_id: row.workspace_id,
            name: row.name,
            style_name: row.style_name,
            batch_size_l: row.batch_size_l,
            boil_minutes: row.boil_minutes,
            document,
            source: RecipeSource::from_str(&row.source),
            created_by: row.created_by,
            created_at: row.created_at,
            updated_at: row.updated_at,
            native_migration_error: row.native_migration_error,
        })
    }
}

impl From<SummaryRow> for RecipeSummary {
    fn from(row: SummaryRow) -> Self {
        Self {
            recipe_id: row.recipe_id,
            name: row.name,
            style_name: row.style_name,
            batch_size_l: row.batch_size_l,
            boil_minutes: row.boil_minutes,
            source: RecipeSource::from_str(&row.source),
            updated_at: row.updated_at,
        }
    }
}

/// The columns pulled out of the document so the list view can sort and search.
struct Indexed {
    name: String,
    style_name: Option<String>,
    batch_size_l: Option<f64>,
    boil_minutes: Option<i64>,
}

/// Derive the indexed columns from a document.
///
/// One place, so a recipe saved and a recipe imported always index the same
/// way — and so an edit that renames a recipe updates the column with it.
fn index(document: &RecipeDocument) -> Indexed {
    let name = match document.name().trim() {
        "" => "Untitled recipe".to_string(),
        n => n.to_string(),
    };

    Indexed {
        name,
        style_name: document
            .style
            .as_ref()
            .and_then(|s| s.name().cloned())
            .filter(|s| !s.trim().is_empty()),
        batch_size_l: document.header.batch_size().copied(),
        boil_minutes: document.header.boil_time().map(|m| *m as i64),
    }
}

fn new_recipe_id() -> String {
    format!("rcp-{}", uuid::Uuid::new_v4())
}

// ─────────────────────────────────────────────────────────────────────────
// Create
// ─────────────────────────────────────────────────────────────────────────

/// Store a new recipe, returning it as stored.
pub async fn create_recipe(
    pool: &DbPool,
    workspace_id: &str,
    document: &RecipeDocument,
    source: RecipeSource,
    created_by: Option<&str>,
) -> krausen_core::Result<Recipe> {
    let recipe_id = new_recipe_id();
    let indexed = index(document);
    let encoded = serde_json::to_string(document)
        .map_err(|e| krausen_core::Error::Internal(format!("could not encode recipe: {e}")))?;

    let sql = "INSERT INTO recipes \
        (recipe_id, workspace_id, name, style_name, batch_size_l, boil_minutes, \
         document, source, created_by) \
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)";

    db_execute!(
        pool,
        sql,
        &recipe_id,
        workspace_id,
        &indexed.name,
        &indexed.style_name,
        indexed.batch_size_l,
        indexed.boil_minutes,
        &encoded,
        source.as_str(),
        created_by
    )?;

    get_recipe(pool, workspace_id, &recipe_id)
        .await?
        .ok_or_else(|| krausen_core::Error::Internal("recipe vanished after insert".into()))
}

// ─────────────────────────────────────────────────────────────────────────
// Read
// ─────────────────────────────────────────────────────────────────────────

/// Fetch one recipe.
///
/// Scoped by workspace: a recipe id from another workspace reads as absent
/// rather than as a permission error, which is the same answer a caller who
/// should not know it exists would get for one that genuinely does not.
pub async fn get_recipe(
    pool: &DbPool,
    workspace_id: &str,
    recipe_id: &str,
) -> krausen_core::Result<Option<Recipe>> {
    let sql = "SELECT recipe_id, workspace_id, name, style_name, batch_size_l, \
                      boil_minutes, document, source, created_by, created_at, updated_at, native_head_revision, native_migration_error \
               FROM recipes \
               WHERE recipe_id = $1 AND workspace_id = $2 AND deleted_at IS NULL";

    let row = db_fetch_optional!(pool, RecipeRow, sql, recipe_id, workspace_id)?;
    row.map(Recipe::try_from).transpose()
}

/// List a workspace's recipes, most recently updated first.
pub async fn list_recipes(
    pool: &DbPool,
    workspace_id: &str,
) -> krausen_core::Result<Vec<RecipeSummary>> {
    let sql = "SELECT recipe_id, name, style_name, batch_size_l, boil_minutes, \
                      source, updated_at \
               FROM recipes \
               WHERE workspace_id = $1 AND deleted_at IS NULL \
               ORDER BY updated_at DESC";

    let rows = db_fetch_all!(pool, SummaryRow, sql, workspace_id)?;
    Ok(rows.into_iter().map(RecipeSummary::from).collect())
}

/// Search a workspace's recipes by name.
pub async fn search_recipes(
    pool: &DbPool,
    workspace_id: &str,
    query: &str,
) -> krausen_core::Result<Vec<RecipeSummary>> {
    let is_pg = pool.is_postgres();
    let name_like = sql_compat::ilike(is_pg, "name", "$2");
    let sql = format!(
        "SELECT recipe_id, name, style_name, batch_size_l, boil_minutes, \
                source, updated_at \
         FROM recipes \
         WHERE workspace_id = $1 AND deleted_at IS NULL AND {name_like} \
         ORDER BY updated_at DESC"
    );

    let pattern = format!("%{query}%");
    let rows = db_fetch_all!(pool, SummaryRow, &sql, workspace_id, &pattern)?;
    Ok(rows.into_iter().map(RecipeSummary::from).collect())
}

// ─────────────────────────────────────────────────────────────────────────
// Update
// ─────────────────────────────────────────────────────────────────────────

/// Replace a recipe's document.
///
/// The whole document is replaced rather than patched: a recipe is edited as a
/// whole, and a partial update is a way to end up with a hop schedule that
/// belongs to one version and a grain bill that belongs to another.
pub async fn update_recipe(
    pool: &DbPool,
    workspace_id: &str,
    recipe_id: &str,
    document: &RecipeDocument,
) -> krausen_core::Result<Option<Recipe>> {
    let indexed = index(document);
    let encoded = serde_json::to_string(document)
        .map_err(|e| krausen_core::Error::Internal(format!("could not encode recipe: {e}")))?;

    let is_pg = pool.is_postgres();
    let now = sql_compat::now(is_pg);
    let sql = format!(
        "UPDATE recipes SET \
            name = $1, style_name = $2, batch_size_l = $3, boil_minutes = $4, \
            document = $5, updated_at = {now} \
         WHERE recipe_id = $6 AND workspace_id = $7 AND deleted_at IS NULL AND native_head_revision = 0 AND native_migration_error IS NULL"
    );

    let affected = db_execute!(
        pool,
        &sql,
        &indexed.name,
        &indexed.style_name,
        indexed.batch_size_l,
        indexed.boil_minutes,
        &encoded,
        recipe_id,
        workspace_id
    )?;

    if affected.rows_affected() == 0 {
        reject_native_legacy_write(pool, workspace_id, recipe_id).await?;
        return Ok(None);
    }
    get_recipe(pool, workspace_id, recipe_id).await
}

// ─────────────────────────────────────────────────────────────────────────
// Delete
// ─────────────────────────────────────────────────────────────────────────

/// Soft-delete a recipe. Returns whether one was deleted.
///
/// Soft, because a recipe is the record of something a person made and a
/// mis-click should not be the end of it.
pub async fn delete_recipe(
    pool: &DbPool,
    workspace_id: &str,
    recipe_id: &str,
) -> krausen_core::Result<bool> {
    let is_pg = pool.is_postgres();
    let now = sql_compat::now(is_pg);
    let sql = format!(
        "UPDATE recipes SET deleted_at = {now} \
         WHERE recipe_id = $1 AND workspace_id = $2 AND deleted_at IS NULL AND native_head_revision = 0 AND native_migration_error IS NULL"
    );

    let affected = db_execute!(pool, &sql, recipe_id, workspace_id)?;
    if affected.rows_affected() == 0 {
        reject_native_legacy_write(pool, workspace_id, recipe_id).await?;
    }
    Ok(affected.rows_affected() > 0)
}

/// Bring a soft-deleted recipe back. Returns whether one was restored.
pub async fn restore_recipe(
    pool: &DbPool,
    workspace_id: &str,
    recipe_id: &str,
) -> krausen_core::Result<bool> {
    let sql = "UPDATE recipes SET deleted_at = NULL \
               WHERE recipe_id = $1 AND workspace_id = $2 AND deleted_at IS NOT NULL AND native_head_revision = 0 AND native_migration_error IS NULL";

    let affected = db_execute!(pool, sql, recipe_id, workspace_id)?;
    if affected.rows_affected() == 0 {
        reject_native_legacy_write(pool, workspace_id, recipe_id).await?;
    }
    Ok(affected.rows_affected() > 0)
}

#[derive(sqlx::FromRow)]
struct NativeHead {
    native_head_revision: i64,
    native_migration_error: Option<String>,
}

/// The legacy editor cannot update or archive a native revisioned recipe.
async fn reject_native_legacy_write(
    pool: &DbPool,
    workspace_id: &str,
    recipe_id: &str,
) -> krausen_core::Result<()> {
    let row = db_fetch_optional!(
        pool,
        NativeHead,
        "SELECT native_head_revision, native_migration_error FROM recipes WHERE workspace_id = $1 AND recipe_id = $2",
        workspace_id,
        recipe_id
    )?;
    if row.is_some_and(|r| r.native_head_revision > 0 || r.native_migration_error.is_some()) {
        return Err(krausen_core::Error::Conflict(
            "This recipe uses native revisions and is read-only in the legacy editor".into(),
        ));
    }
    Ok(())
}
