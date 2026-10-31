// SPDX-License-Identifier: AGPL-3.0-or-later
//! Authorized native recipe reads and revision-checked explicit saves.
use crate::{
    ActorContext, Error, RequestIdentity, authorize_workspace, boundary_error, check_intent,
    db_error, record_change, record_request, recover_request,
};
use krausen_brewery::{native_store, validation::validate_draft};
use krausen_core::{DbPool, DbTx, canonical_json};
use krausen_types::{
    brewery::RecipeDocumentV1,
    recipe_editor::{RecipeReadOutcome, RecipeSaveOutcome, RecipeSnapshot},
    request_context::OperationIntent,
};

/// Read a current, non-archived recipe under current workspace authority.
pub async fn read(
    pool: &DbPool,
    actor: &ActorContext,
    intent: &OperationIntent,
    recipe_id: &str,
) -> RecipeReadOutcome {
    match read_inner(pool, actor, intent, recipe_id).await {
        Ok(Some(snapshot)) => RecipeReadOutcome::Found(snapshot),
        Ok(None) => RecipeReadOutcome::NotFound,
        Err(Error::Forbidden) => RecipeReadOutcome::Forbidden,
        Err(Error::ContextChanged) => RecipeReadOutcome::ContextChanged,
        Err(e) => {
            tracing::error!(error = %e, "recipe read failed");
            RecipeReadOutcome::Unavailable
        }
    }
}
async fn read_inner(
    pool: &DbPool,
    actor: &ActorContext,
    intent: &OperationIntent,
    recipe_id: &str,
) -> Result<Option<RecipeSnapshot>, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, intent)?;
    let revision = scalar!(&mut tx, i64, "SELECT native_head_revision FROM recipes WHERE workspace_id = $1 AND recipe_id = $2 AND deleted_at IS NULL AND native_head_revision > 0 AND EXISTS (SELECT 1 FROM workspaces w WHERE w.workspace_id = recipes.workspace_id AND w.status = 'active')", &actor.workspace_id, recipe_id).map_err(db_error)?;
    let Some(revision) = revision else {
        tx.commit().await.map_err(boundary_error)?;
        return Ok(None);
    };
    let document = native_store::load_document(&mut tx, &actor.workspace_id, recipe_id, revision)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "native recipe revision cannot be read");
            Error::Invalid("unsupported recipe revision")
        })?
        .ok_or(Error::Invalid("missing recipe revision"))?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(Some(RecipeSnapshot {
        recipe_id: recipe_id.to_owned(),
        revision,
        document,
    }))
}

// A stored copied specification may retain its existing provenance. Until the
// catalogue resolver exists, callers cannot introduce or change source IDs.
fn source_references_preserved(
    draft: &RecipeDocumentV1,
    previous: Option<&RecipeDocumentV1>,
) -> bool {
    macro_rules! preserved {
        ($field:ident) => {
            draft.$field.iter().all(|row| {
                row.source.is_none()
                    || previous.is_some_and(|old| {
                        old.$field
                            .iter()
                            .any(|prior| prior.row_id == row.row_id && prior.source == row.source)
                    })
            })
        };
    }
    preserved!(fermentables)
        && preserved!(hops)
        && preserved!(yeast)
        && preserved!(other_ingredients)
}

/// Create or save through one transaction. `recipe_id = None` creates revision 1;
/// otherwise the observed head is required. Reusing a request id recovers its result.
pub async fn save(
    pool: &DbPool,
    actor: &ActorContext,
    intent: &OperationIntent,
    recipe_id: Option<&str>,
    expected_revision: Option<i64>,
    document: &RecipeDocumentV1,
    request_id: uuid::Uuid,
) -> RecipeSaveOutcome {
    match save_inner(
        pool,
        actor,
        intent,
        recipe_id,
        expected_revision,
        document,
        request_id,
    )
    .await
    {
        Ok(result) => result,
        Err(Error::Forbidden) => RecipeSaveOutcome::Forbidden,
        Err(Error::ContextChanged) => RecipeSaveOutcome::ContextChanged,
        Err(Error::Invalid(_)) => RecipeSaveOutcome::InvalidRequest,
        Err(Error::RequestIdentityConflict) => RecipeSaveOutcome::RequestIdentityConflict,
        Err(Error::ResultExpired { resource_id }) => {
            RecipeSaveOutcome::ResultExpired { resource_id }
        }
        Err(Error::Retryable) => RecipeSaveOutcome::Retryable,
        Err(e) => {
            tracing::error!(error = %e, "recipe save failed");
            RecipeSaveOutcome::Unavailable
        }
    }
}
async fn save_inner(
    pool: &DbPool,
    actor: &ActorContext,
    intent: &OperationIntent,
    recipe_id: Option<&str>,
    expected: Option<i64>,
    document: &RecipeDocumentV1,
    request_id: uuid::Uuid,
) -> Result<RecipeSaveOutcome, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, intent)?;
    // Serialize request identity across both creation and edits before retry lookup.
    let ws_sql = if matches!(&tx, DbTx::Postgres(_)) {
        "SELECT revision FROM workspaces WHERE workspace_id = $1 AND status = 'active' FOR UPDATE"
    } else {
        "SELECT revision FROM workspaces WHERE workspace_id = $1 AND status = 'active'"
    };
    let workspace = scalar!(&mut tx, i64, ws_sql, &actor.workspace_id).map_err(db_error)?;
    // The row lock can wait past access or credential expiry. Recheck under
    // the same transaction before returning a stored result or writing.
    authorize_workspace(&mut tx, actor).await?;
    if workspace.is_none() {
        return Err(Error::Forbidden);
    }
    let validation = validate_draft(document, &document.equipment_overrides);
    let value = match canonical_json::from_serializable(&(recipe_id, expected, document)) {
        Ok(value) => value,
        Err(_) if !validation.saveable() => return Ok(RecipeSaveOutcome::Invalid(validation)),
        Err(_) => return Err(Error::Invalid("recipe document")),
    };
    let digest = canonical_json::sha256(&value).map_err(|_| Error::Invalid("recipe document"))?;
    let request_id = request_id.to_string();
    let key = RequestIdentity {
        actor,
        operation: "recipe.save.v1",
        request_id: &request_id,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        return Ok(serde_json::from_value(result)?);
    }
    if document.equipment_profile_id.is_some() {
        return Ok(RecipeSaveOutcome::UnresolvedEquipmentProfile);
    }
    if !validation.saveable() {
        return Ok(RecipeSaveOutcome::Invalid(validation));
    }
    let (id, revision, changed) = match (recipe_id, expected) {
        (None, None) => {
            if document.import_provenance_id.is_some() {
                return Ok(RecipeSaveOutcome::InvalidProvenance);
            }
            if !source_references_preserved(document, None) {
                return Ok(RecipeSaveOutcome::UnresolvedIngredientSource);
            }
            let id = uuid::Uuid::new_v4().to_string();
            native_store::create(
                &mut tx,
                &actor.workspace_id,
                &id,
                document,
                &actor.actor_id,
                chrono::Utc::now(),
            )
            .await
            .map_err(boundary_error)?;
            (id, 1, true)
        }
        (Some(id), Some(expected)) if expected > 0 => {
            let sql = if matches!(&tx, DbTx::Postgres(_)) {
                "SELECT native_head_revision FROM recipes WHERE workspace_id = $1 AND recipe_id = $2 AND deleted_at IS NULL FOR UPDATE"
            } else {
                "SELECT native_head_revision FROM recipes WHERE workspace_id = $1 AND recipe_id = $2 AND deleted_at IS NULL"
            };
            let actual = scalar!(&mut tx, i64, sql, &actor.workspace_id, id).map_err(db_error)?;
            authorize_workspace(&mut tx, actor).await?;
            let Some(actual) = actual else {
                return Ok(RecipeSaveOutcome::NotFound);
            };
            if actual != expected {
                return Ok(RecipeSaveOutcome::Conflict { expected, actual });
            }
            let prior = native_store::load_document(&mut tx, &actor.workspace_id, id, actual)
                .await
                .map_err(|e| {
                    tracing::error!(error = ?e, "native recipe revision cannot be read");
                    Error::Invalid("unsupported recipe revision")
                })?
                .ok_or(Error::Invalid("missing recipe revision"))?;
            if document.import_provenance_id != prior.import_provenance_id {
                return Ok(RecipeSaveOutcome::InvalidProvenance);
            }
            if !source_references_preserved(document, Some(&prior)) {
                return Ok(RecipeSaveOutcome::UnresolvedIngredientSource);
            }
            let saved = native_store::save(
                &mut tx,
                &actor.workspace_id,
                id,
                expected,
                document,
                &actor.actor_id,
                chrono::Utc::now(),
                None,
            )
            .await
            .map_err(boundary_error)?;
            match saved {
                native_store::SaveResult::Unchanged(revision) => (id.to_owned(), revision, false),
                native_store::SaveResult::Appended(revision) => (id.to_owned(), revision, true),
            }
        }
        _ => return Err(Error::Invalid("recipe identity and expected revision")),
    };
    let result = RecipeSaveOutcome::Saved {
        recipe_id: id.clone(),
        revision,
    };
    record_request(&mut tx, &key, &serde_json::to_value(&result)?, Some(&id)).await?;
    if changed {
        record_change(&mut tx, actor, "recipe.save.v1", &id, revision).await?;
    }
    tx.commit().await.map_err(boundary_error)?;
    Ok(result)
}
