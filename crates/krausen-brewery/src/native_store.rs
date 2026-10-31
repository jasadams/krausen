// SPDX-License-Identifier: AGPL-3.0-or-later

//! Transaction-participating native recipe persistence. The caller owns DbTx.

use crate::native_recipe::{encode_document, prepare_backfill, read_document};
use chrono::{DateTime, Utc};
use krausen_core::{DbTx, Error, Result};
use krausen_types::brewery::RecipeDocumentV1;

// The legacy summary column is INTEGER on PostgreSQL and SQLite. The native
// revision retains full precision; do not publish a truncated or saturated
// summary when the source cannot be represented by both backends.
fn legacy_whole_minutes(value: Option<f64>) -> Option<i64> {
    value
        .filter(|v| {
            v.is_finite() && v.fract() == 0.0 && *v >= i32::MIN as f64 && *v <= i32::MAX as f64
        })
        .map(|v| v as i64)
}

macro_rules! execute {
    ($tx:expr, $sql:expr $(, $arg:expr)* $(,)?) => {{
        match &mut *$tx {
            DbTx::Postgres(inner) => sqlx::query($sql)$(.bind($arg))*.execute(&mut **inner).await?.rows_affected(),
            DbTx::Sqlite(inner) => sqlx::query($sql)$(.bind($arg))*.execute(&mut **inner).await?.rows_affected(),
        }
    }};
}

#[derive(sqlx::FromRow)]
struct LockedRecipe {
    native_head_revision: i64,
    metadata_revision: i64,
    document: String,
    deleted_at: Option<DateTime<Utc>>,
}

async fn lock_recipe(
    tx: &mut DbTx<'_>,
    workspace: &str,
    recipe: &str,
) -> Result<Option<LockedRecipe>> {
    let sql = "SELECT native_head_revision, metadata_revision, document, deleted_at FROM recipes WHERE workspace_id = $1 AND recipe_id = $2";
    Ok(match &mut *tx {
        DbTx::Postgres(inner) => {
            sqlx::query_as::<_, LockedRecipe>(&format!("{sql} FOR UPDATE"))
                .bind(workspace)
                .bind(recipe)
                .fetch_optional(&mut **inner)
                .await?
        }
        DbTx::Sqlite(inner) => {
            sqlx::query_as::<_, LockedRecipe>(sql)
                .bind(workspace)
                .bind(recipe)
                .fetch_optional(&mut **inner)
                .await?
        }
    })
}

/// Backfill one legacy row. The legacy source is retained verbatim in immutable
/// provenance; absent source file bytes and actors are not invented.
pub async fn backfill(
    tx: &mut DbTx<'_>,
    workspace: &str,
    recipe: &str,
    migrated_at: DateTime<Utc>,
) -> Result<bool> {
    let row = lock_recipe(tx, workspace, recipe)
        .await?
        .ok_or_else(|| Error::NotFound("recipe".into()))?;
    if row.native_head_revision != 0 {
        return Ok(false);
    }
    let provenance_id = format!("imp-{}", uuid::Uuid::new_v4());
    let prepared = prepare_backfill(&row.document, &provenance_id)
        .map_err(|e| Error::BadRequest(format!("legacy recipe requires recovery: {e}")))?;
    let document = read_document(&prepared.document_json)
        .map_err(|e| Error::Internal(format!("prepared native recipe is unreadable: {e:?}")))?;
    execute!(
        tx,
        "INSERT INTO recipe_revisions (workspace_id, recipe_id, revision, schema_version, document, content_digest, migrated_at) VALUES ($1, $2, 1, 1, $3, $4, $5)",
        workspace,
        recipe,
        &prepared.document_json,
        &prepared.content_digest,
        migrated_at
    );
    execute!(
        tx,
        "INSERT INTO recipe_import_provenance (workspace_id, recipe_id, revision, provenance_id, format, source_record, mapping_version, diagnostics) VALUES ($1, $2, 1, $3, 'legacy-beerxml-record', $4, 'legacy-native-v2', $5)",
        workspace,
        recipe,
        &provenance_id,
        &prepared.source_record_json,
        &prepared.diagnostics_json
    );
    execute!(
        tx,
        "UPDATE recipes SET native_head_revision = 1, native_migration_error = NULL, name = $3, style_name = $4, batch_size_l = $5, boil_minutes = $6 WHERE workspace_id = $1 AND recipe_id = $2 AND native_head_revision = 0",
        workspace,
        recipe,
        &document.title,
        document.style.as_ref().map(|s| s.name.as_str()),
        document.batch_size_l,
        legacy_whole_minutes(document.boil_minutes)
    );
    Ok(true)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveResult {
    Unchanged(i64),
    Appended(i64),
}

/// Create the aggregate and its first native revision in the caller's transaction.
pub async fn create(
    tx: &mut DbTx<'_>,
    workspace: &str,
    recipe: &str,
    document: &RecipeDocumentV1,
    actor: &str,
    saved_at: DateTime<Utc>,
) -> Result<()> {
    let (json, digest) = encode_document(document).map_err(|e| Error::BadRequest(e.to_string()))?;
    let style = document.style.as_ref().map(|s| s.name.as_str());
    let boil = legacy_whole_minutes(document.boil_minutes);
    execute!(
        tx,
        "INSERT INTO recipes (workspace_id, recipe_id, name, style_name, batch_size_l, boil_minutes, document, source, created_by, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, '{}', 'krausen', $7, $8, $8)",
        workspace,
        recipe,
        &document.title,
        style,
        document.batch_size_l,
        boil,
        actor,
        saved_at
    );
    execute!(
        tx,
        "INSERT INTO recipe_revisions (workspace_id, recipe_id, revision, schema_version, document, content_digest, saved_by, saved_at) VALUES ($1, $2, 1, 1, $3, $4, $5, $6)",
        workspace,
        recipe,
        &json,
        &digest,
        actor,
        saved_at
    );
    execute!(
        tx,
        "UPDATE recipes SET native_head_revision = 1 WHERE workspace_id = $1 AND recipe_id = $2 AND native_head_revision = 0",
        workspace,
        recipe
    );
    Ok(())
}

/// Create an imported native recipe and retain its exact source in the same
/// transaction. The parser's source record and diagnostics are also retained.
pub async fn create_imported(
    tx: &mut DbTx<'_>,
    workspace: &str,
    recipe: &str,
    mut imported: krausen_beerxml::native::NativeMigration,
    actor: &str,
    saved_at: DateTime<Utc>,
) -> Result<RecipeDocumentV1> {
    let source = imported
        .original_source
        .take()
        .ok_or_else(|| Error::BadRequest("original source is required for an import".into()))?;
    if !source.has_valid_hash() {
        return Err(Error::BadRequest(
            "source hash does not match original bytes".into(),
        ));
    }
    let provenance_id = format!("imp-{}", uuid::Uuid::new_v4());
    imported.document.import_provenance_id = Some(provenance_id.clone());
    create(tx, workspace, recipe, &imported.document, actor, saved_at).await?;
    // The complete original bytes remain authoritative provenance; this index
    // records exact source locations (including attributes and envelope nodes)
    // alongside the parsed legacy projection and the user's mapping choices.
    let record = serde_json::to_string(&serde_json::json!({
        "legacy_record": imported.legacy_record,
        "source_elements": imported.source_elements,
        "choices": imported.choices,
        "source_recipe_index": imported.source_recipe_index,
        "source_estimates": imported.source_estimates,
    }))?;
    let diagnostics = serde_json::to_string(&imported.warnings)?;
    execute!(
        tx,
        "INSERT INTO recipe_import_provenance (workspace_id, recipe_id, revision, provenance_id, format, original_bytes, original_hash, source_record, mapping_version, diagnostics) VALUES ($1, $2, 1, $3, 'BeerXML', $4, $5, $6, $7, $8)",
        workspace,
        recipe,
        &provenance_id,
        &source.bytes,
        &source.sha256,
        &record,
        krausen_beerxml::native::MAPPING_VERSION,
        &diagnostics
    );
    Ok(imported.document)
}

/// Retrieve the original versioned JSON; pass it to `read_document` to obtain
/// typed unsupported-version errors without silently rewriting old records.
pub async fn load_raw(
    tx: &mut DbTx<'_>,
    workspace: &str,
    recipe: &str,
    revision: i64,
) -> Result<Option<String>> {
    let sql = "SELECT document FROM recipe_revisions WHERE workspace_id = $1 AND recipe_id = $2 AND revision = $3";
    Ok(match &mut *tx {
        DbTx::Postgres(inner) => {
            sqlx::query_scalar(sql)
                .bind(workspace)
                .bind(recipe)
                .bind(revision)
                .fetch_optional(&mut **inner)
                .await?
        }
        DbTx::Sqlite(inner) => {
            sqlx::query_scalar(sql)
                .bind(workspace)
                .bind(recipe)
                .bind(revision)
                .fetch_optional(&mut **inner)
                .await?
        }
    })
}

#[derive(Debug)]
pub enum StoredReadError {
    Storage(krausen_core::Error),
    UnsupportedVersion(i32),
    Document(crate::native_recipe::ReadError),
}

/// Dispatch using the stored schema before reading the embedded document.
pub async fn load_document(
    tx: &mut DbTx<'_>,
    workspace: &str,
    recipe: &str,
    revision: i64,
) -> std::result::Result<Option<RecipeDocumentV1>, StoredReadError> {
    let sql = "SELECT schema_version, document FROM recipe_revisions WHERE workspace_id = $1 AND recipe_id = $2 AND revision = $3";
    let row: Option<(i32, String)> = match &mut *tx {
        DbTx::Postgres(inner) => {
            sqlx::query_as(sql)
                .bind(workspace)
                .bind(recipe)
                .bind(revision)
                .fetch_optional(&mut **inner)
                .await
        }
        DbTx::Sqlite(inner) => {
            sqlx::query_as(sql)
                .bind(workspace)
                .bind(recipe)
                .bind(revision)
                .fetch_optional(&mut **inner)
                .await
        }
    }
    .map_err(|e| StoredReadError::Storage(e.into()))?;
    let Some((version, json)) = row else {
        return Ok(None);
    };
    if version != 1 {
        return Err(StoredReadError::UnsupportedVersion(version));
    }
    read_document(&json)
        .map(Some)
        .map_err(StoredReadError::Document)
}

#[derive(Debug, sqlx::FromRow)]
pub struct ImportProvenance {
    pub provenance_id: String,
    pub original_bytes: Option<Vec<u8>>,
    pub original_hash: Option<String>,
    pub source_record: Option<String>,
    pub mapping_version: String,
    pub diagnostics: String,
}

pub async fn load_provenance(
    tx: &mut DbTx<'_>,
    workspace: &str,
    recipe: &str,
    revision: i64,
) -> Result<Option<ImportProvenance>> {
    let sql = "SELECT provenance_id, original_bytes, original_hash, source_record, mapping_version, diagnostics FROM recipe_import_provenance WHERE workspace_id = $1 AND recipe_id = $2 AND revision = $3";
    Ok(match &mut *tx {
        DbTx::Postgres(inner) => {
            sqlx::query_as(sql)
                .bind(workspace)
                .bind(recipe)
                .bind(revision)
                .fetch_optional(&mut **inner)
                .await?
        }
        DbTx::Sqlite(inner) => {
            sqlx::query_as(sql)
                .bind(workspace)
                .bind(recipe)
                .bind(revision)
                .fetch_optional(&mut **inner)
                .await?
        }
    })
}

/// Append a document under the held transaction and expected head. A deliberate
/// restore creates a revision even when its contents match the current head.
pub async fn save(
    tx: &mut DbTx<'_>,
    workspace: &str,
    recipe: &str,
    expected_head: i64,
    document: &RecipeDocumentV1,
    actor: &str,
    saved_at: DateTime<Utc>,
    restored_from: Option<i64>,
) -> Result<SaveResult> {
    let row = lock_recipe(tx, workspace, recipe)
        .await?
        .ok_or_else(|| Error::NotFound("recipe".into()))?;
    if row.native_head_revision != expected_head || expected_head < 1 || row.deleted_at.is_some() {
        return Err(Error::Conflict("recipe head changed".into()));
    }
    let (json, digest) = encode_document(document).map_err(|e| Error::BadRequest(e.to_string()))?;
    let sql = "SELECT content_digest FROM recipe_revisions WHERE workspace_id = $1 AND recipe_id = $2 AND revision = $3";
    let previous: String = match &mut *tx {
        DbTx::Postgres(inner) => {
            sqlx::query_scalar(sql)
                .bind(workspace)
                .bind(recipe)
                .bind(expected_head)
                .fetch_one(&mut **inner)
                .await?
        }
        DbTx::Sqlite(inner) => {
            sqlx::query_scalar(sql)
                .bind(workspace)
                .bind(recipe)
                .bind(expected_head)
                .fetch_one(&mut **inner)
                .await?
        }
    };
    if let Some(source) = restored_from {
        if source < 1 || source > expected_head {
            return Err(Error::BadRequest("invalid restore revision".into()));
        }
        let source_json = load_raw(tx, workspace, recipe, source)
            .await?
            .ok_or_else(|| Error::NotFound("source revision".into()))?;
        let source_document = read_document(&source_json)
            .map_err(|e| Error::BadRequest(format!("source revision cannot be read: {e:?}")))?;
        let (_, source_digest) =
            encode_document(&source_document).map_err(|e| Error::BadRequest(e.to_string()))?;
        if source_digest != digest {
            return Err(Error::BadRequest(
                "restore content differs from source revision".into(),
            ));
        }
    }
    if restored_from.is_none() && previous == digest {
        return Ok(SaveResult::Unchanged(expected_head));
    }
    let next = expected_head
        .checked_add(1)
        .ok_or_else(|| Error::Conflict("revision overflow".into()))?;
    execute!(
        tx,
        "INSERT INTO recipe_revisions (workspace_id, recipe_id, revision, schema_version, document, content_digest, saved_by, saved_at, restored_from_revision) VALUES ($1, $2, $3, 1, $4, $5, $6, $7, $8)",
        workspace,
        recipe,
        next,
        &json,
        &digest,
        actor,
        saved_at,
        restored_from
    );
    let style = document.style.as_ref().map(|s| s.name.as_str());
    let boil = legacy_whole_minutes(document.boil_minutes);
    let updated = execute!(
        tx,
        "UPDATE recipes SET native_head_revision = $1, name = $2, style_name = $3, batch_size_l = $4, boil_minutes = $5, updated_at = $6 WHERE workspace_id = $7 AND recipe_id = $8 AND native_head_revision = $9 AND deleted_at IS NULL",
        next,
        &document.title,
        style,
        document.batch_size_l,
        boil,
        saved_at,
        workspace,
        recipe,
        expected_head
    );
    if updated != 1 {
        return Err(Error::Conflict(
            "recipe head changed or recipe archived".into(),
        ));
    }
    Ok(SaveResult::Appended(next))
}

/// Archive or restore under the independently guarded metadata revision.
pub async fn set_archived(
    tx: &mut DbTx<'_>,
    workspace: &str,
    recipe: &str,
    expected_metadata: i64,
    archived_at: Option<DateTime<Utc>>,
) -> Result<i64> {
    let row = lock_recipe(tx, workspace, recipe)
        .await?
        .ok_or_else(|| Error::NotFound("recipe".into()))?;
    if row.native_head_revision < 1 || row.metadata_revision != expected_metadata {
        return Err(Error::Conflict("recipe metadata changed".into()));
    }
    if row.deleted_at.is_some() == archived_at.is_some() {
        return Ok(expected_metadata);
    }
    let next = expected_metadata
        .checked_add(1)
        .ok_or_else(|| Error::Conflict("revision overflow".into()))?;
    let updated = execute!(
        tx,
        "UPDATE recipes SET deleted_at = $1, metadata_revision = $2 WHERE workspace_id = $3 AND recipe_id = $4 AND metadata_revision = $5",
        archived_at,
        next,
        workspace,
        recipe,
        expected_metadata
    );
    if updated != 1 {
        return Err(Error::Conflict("recipe metadata changed".into()));
    }
    Ok(next)
}
