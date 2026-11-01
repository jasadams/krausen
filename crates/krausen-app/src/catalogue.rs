// SPDX-License-Identifier: AGPL-3.0-or-later
//! Authorized ingredient catalogue operations. Every write owns one DbTx.
//!
//! Starter entries are the explicit public read-only exception; workspace
//! custom entries are strictly scoped to the effective workspace, including
//! for actors who belong to several workspaces.
use crate::{
    ActorContext, Error, RequestIdentity, authorize_workspace, check_intent, record_change,
    record_request, recover_request,
};
use krausen_core::{DbPool, DbTx, canonical_json};
use krausen_types::{
    brewery::{IngredientSource, RecipeDocumentV1},
    catalogue::{
        ArchiveIngredientEntry, CatalogueScope, IngredientEntry, IngredientEntryDocumentV1,
        IngredientOutcome, IngredientPage, IngredientProvenance, INGREDIENT_ENTRY_KINDS,
        SaveIngredientEntry, SearchCatalogue,
    },
    request_context::OperationIntent,
};

fn validate(document: &IngredientEntryDocumentV1) -> Result<(), Error> {
    if document.schema_version != 1
        || document.kind != "fermentable"
        || !INGREDIENT_ENTRY_KINDS.contains(&document.kind.as_str())
    {
        return Err(Error::Invalid("ingredient entry kind"));
    }
    let name = document.specification.name.as_deref().unwrap_or("").trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(Error::Invalid("ingredient name"));
    }
    if let Some(producer) = document.specification.producer.as_deref()
        && producer.chars().count() > 100
    {
        return Err(Error::Invalid("ingredient producer"));
    }
    if document.category.trim().is_empty() || document.category.chars().count() > 64 {
        return Err(Error::Invalid("ingredient category"));
    }
    if !matches!(
        document.specification.kind.as_deref(),
        Some("grain" | "extract" | "sugar")
    ) {
        return Err(Error::Invalid("ingredient type"));
    }
    let Some(yield_fraction) = document.specification.yield_fraction else {
        return Err(Error::Invalid("ingredient yield"));
    };
    if !yield_fraction.is_finite() || yield_fraction <= 0.0 || yield_fraction > 1.0 {
        return Err(Error::Invalid("ingredient yield"));
    }
    let Some(colour_ebc) = document.specification.colour_ebc else {
        return Err(Error::Invalid("ingredient colour"));
    };
    if !colour_ebc.is_finite() || colour_ebc < 0.0 {
        return Err(Error::Invalid("ingredient colour"));
    }
    canonical_json::from_serializable(document)
        .map_err(|_| Error::Invalid("ingredient encoding"))?;
    Ok(())
}

fn digest<T: serde::Serialize>(value: &T) -> Result<String, Error> {
    let v =
        canonical_json::from_serializable(value).map_err(|_| Error::Invalid("request encoding"))?;
    canonical_json::sha256(&v).map_err(|_| Error::Invalid("request encoding"))
}

fn identifiers(entry: &str, request: &str) -> Result<(), Error> {
    uuid::Uuid::parse_str(entry).map_err(|_| Error::Invalid("entry id"))?;
    uuid::Uuid::parse_str(request).map_err(|_| Error::Invalid("request id"))?;
    Ok(())
}

/// Serialize writes and retries against this workspace, including creation.
async fn lock_workspace(tx: &mut DbTx<'_>, workspace: &str) -> Result<(), Error> {
    let query = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT workspace_id FROM workspaces WHERE workspace_id=$1 AND status='active' FOR UPDATE"
    } else {
        "SELECT workspace_id FROM workspaces WHERE workspace_id=$1 AND status='active'"
    };
    let found = match tx {
        DbTx::Postgres(t) => {
            sqlx::query_scalar::<_, String>(query)
                .bind(workspace)
                .fetch_optional(&mut **t)
                .await
        }
        DbTx::Sqlite(t) => {
            sqlx::query_scalar::<_, String>(query)
                .bind(workspace)
                .fetch_optional(&mut **t)
                .await
        }
    }
    .map_err(crate::db_error)?;
    if found.is_none() {
        return Err(Error::Forbidden);
    }
    Ok(())
}

fn normalize_query(query: &str) -> Result<String, Error> {
    let trimmed = query.trim();
    if trimmed.chars().count() > 200 {
        return Err(Error::Invalid("search query"));
    }
    Ok(trimmed.to_owned())
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchCursor {
    version: u8,
    workspace_id: String,
    filter: String,
    query: String,
    order: String,
    include_archived: bool,
    created_at: i64,
    entry_id: String,
}

fn valid_entry_cursor_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Bounded search with an opaque cursor bound to workspace, filter, query and
/// order. Starter results follow the explicit global-read exception.
pub async fn search(
    pool: &DbPool,
    actor: &ActorContext,
    request: &SearchCatalogue,
) -> Result<IngredientPage, Error> {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    if !(1..=100).contains(&request.limit) {
        return Err(Error::Invalid("search limit"));
    }
    let query = normalize_query(&request.query)?;
    let filter = request.filter;
    let mut tx = pool.begin_write().await.map_err(crate::boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, &request.intent)?;
    let (created_at, after_id) = if let Some(encoded) = request.cursor.as_deref() {
        if encoded.len() > 1024 {
            return Err(Error::Invalid("cursor"));
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| Error::Invalid("cursor"))?;
        let cursor: SearchCursor =
            serde_json::from_slice(&bytes).map_err(|_| Error::Invalid("cursor"))?;
        if cursor.version != 1
            || cursor.workspace_id != actor.workspace_id
            || cursor.filter != filter.as_str()
            || cursor.query != query
            || cursor.order != "created_at,id"
            || cursor.include_archived != request.include_archived
            || !valid_entry_cursor_id(&cursor.entry_id)
        {
            return Err(Error::Invalid("cursor"));
        }
        (cursor.created_at, cursor.entry_id)
    } else {
        (i64::MIN, String::new())
    };
    let mut items = krausen_brewery::catalogue_store::search(
        &mut tx,
        &actor.workspace_id,
        filter,
        &query,
        created_at,
        &after_id,
        request.include_archived,
        request.limit + 1,
    )
    .await
    .map_err(crate::boundary_error)?;
    let has_more = items.len() > request.limit as usize;
    items.truncate(request.limit as usize);
    let next_cursor = if has_more {
        let last = items.last().ok_or(Error::Invalid("cursor"))?;
        let next = SearchCursor {
            version: 1,
            workspace_id: actor.workspace_id.clone(),
            filter: filter.as_str().to_owned(),
            query: query.clone(),
            order: "created_at,id".into(),
            include_archived: request.include_archived,
            created_at: last.created_at,
            entry_id: last.entry_id.clone(),
        };
        Some(URL_SAFE_NO_PAD.encode(serde_json::to_vec(&next)?))
    } else {
        None
    };
    tx.commit().await.map_err(crate::boundary_error)?;
    Ok(IngredientPage {
        items,
        next_cursor,
    })
}

/// Read one entry: workspace-scoped first, then the starter exception.
pub async fn get(
    pool: &DbPool,
    actor: &ActorContext,
    intent: &OperationIntent,
    entry_id: &str,
) -> Result<Option<IngredientEntry>, Error> {
    if !valid_entry_cursor_id(entry_id) {
        return Err(Error::Invalid("entry id"));
    }
    let mut tx = pool.begin_write().await.map_err(crate::boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, intent)?;
    let entry = krausen_brewery::catalogue_store::load_entry(
        &mut tx,
        &actor.workspace_id,
        entry_id,
        false,
    )
    .await
    .map_err(crate::boundary_error)?;
    let entry = match entry {
        Some(entry) => Some(entry),
        None => {
            krausen_brewery::catalogue_store::load_starter(&mut tx, entry_id)
                .await
                .map_err(crate::boundary_error)?
        }
    };
    tx.commit().await.map_err(crate::boundary_error)?;
    Ok(entry)
}

/// Trim identity fields once so search metadata, duplicate keys and stored
/// documents always agree; absent-after-trim optional fields become `None`.
fn normalize(document: &mut IngredientEntryDocumentV1) {
    document.specification.name = document
        .specification
        .name
        .take()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty());
    document.specification.producer = document
        .specification
        .producer
        .take()
        .map(|producer| producer.trim().to_owned())
        .filter(|producer| !producer.is_empty());
    document.category = document.category.trim().to_owned();
}

/// Create at revision zero or append a guarded complete specification revision.
/// Starter entries are immutable and never accepted here.
pub async fn save(
    pool: &DbPool,
    actor: &ActorContext,
    mut request: SaveIngredientEntry,
) -> Result<IngredientEntry, Error> {
    identifiers(&request.entry_id, &request.request_id)?;
    if request.expected_revision < 0 || request.expected_metadata_revision < 0 {
        return Err(Error::Invalid("entry revision"));
    }
    normalize(&mut request.document);
    validate(&request.document)?;
    let hash = digest(&(
        &request.intent,
        &request.entry_id,
        &request.document,
        request.expected_revision,
        request.expected_metadata_revision,
    ))?;
    let mut tx = pool.begin_write().await.map_err(crate::boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, &request.intent)?;
    lock_workspace(&mut tx, &actor.workspace_id).await?;
    authorize_workspace(&mut tx, actor).await?;
    let key = RequestIdentity {
        actor,
        operation: "ingredient.save.v1",
        request_id: &request.request_id,
        payload_digest: &hash,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        let entry: IngredientEntry = serde_json::from_value(result)?;
        tx.commit().await.map_err(crate::boundary_error)?;
        return Ok(entry);
    }
    let name = request
        .document
        .specification
        .name
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_owned();
    let producer = request
        .document
        .specification
        .producer
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_owned();
    let current = krausen_brewery::catalogue_store::load_entry(
        &mut tx,
        &actor.workspace_id,
        &request.entry_id,
        true,
    )
    .await
    .map_err(crate::boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    let now = chrono::Utc::now().timestamp();
    let mut changed = true;
    match current {
        None if request.expected_revision == 0 && request.expected_metadata_revision == 0 => {
            if krausen_brewery::catalogue_store::entry_id_in_use(&mut tx, &request.entry_id)
                .await
                .map_err(crate::boundary_error)?
            {
                return Err(Error::Invalid("entry id"));
            }
            let dup_key =
                krausen_brewery::catalogue_store::duplicate_key(&name, &producer);
            if krausen_brewery::catalogue_store::duplicate_key_exists(
                &mut tx,
                &actor.workspace_id,
                &dup_key,
                &request.entry_id,
            )
            .await
            .map_err(crate::boundary_error)?
            {
                return Err(Error::DuplicateIngredient);
            }
            let provenance = IngredientProvenance::workspace_custom();
            krausen_brewery::catalogue_store::create(
                &mut tx,
                &actor.workspace_id,
                &request.entry_id,
                &request.document,
                &provenance,
                &actor.actor_id,
                now,
            )
            .await
            .map_err(crate::boundary_error)?
        }
        None => {
            return Err(Error::Conflict {
                expected: request.expected_revision,
                actual: 0,
            });
        }
        Some(entry) if entry.archived => return Err(Error::IngredientArchived),
        Some(entry) if entry.revision != request.expected_revision => {
            return Err(Error::Conflict {
                expected: request.expected_revision,
                actual: entry.revision,
            });
        }
        Some(entry) if entry.metadata_revision != request.expected_metadata_revision => {
            return Err(Error::Conflict {
                expected: request.expected_metadata_revision,
                actual: entry.metadata_revision,
            });
        }
        Some(entry) if entry.document == request.document => {
            changed = false;
        }
        Some(entry) => {
            let dup_key = krausen_brewery::catalogue_store::duplicate_key(&name, &producer);
            if krausen_brewery::catalogue_store::duplicate_key_exists(
                &mut tx,
                &actor.workspace_id,
                &dup_key,
                &request.entry_id,
            )
            .await
            .map_err(crate::boundary_error)?
            {
                return Err(Error::DuplicateIngredient);
            }
            let provenance = entry.provenance.clone();
            if !krausen_brewery::catalogue_store::append(
                &mut tx,
                &actor.workspace_id,
                &request.entry_id,
                &request.document,
                &provenance,
                &actor.actor_id,
                now,
                request.expected_revision,
            )
            .await
            .map_err(crate::boundary_error)?
            {
                return Err(Error::Retryable);
            }
        }
    }
    let entry = krausen_brewery::catalogue_store::load_entry(
        &mut tx,
        &actor.workspace_id,
        &request.entry_id,
        false,
    )
    .await
    .map_err(crate::boundary_error)?
    .ok_or(Error::Retryable)?;
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&entry)?,
        Some(&entry.entry_id),
    )
    .await?;
    if changed {
        record_change(
            &mut tx,
            actor,
            "ingredient.save.v1",
            &entry.entry_id,
            entry.revision,
        )
        .await?;
    }
    tx.commit().await.map_err(crate::boundary_error)?;
    Ok(entry)
}

/// Archive/unarchive is guarded and appends a retained immutable event.
/// Starter entries are never archived through workspace operations.
pub async fn set_archived(
    pool: &DbPool,
    actor: &ActorContext,
    request: ArchiveIngredientEntry,
) -> Result<IngredientEntry, Error> {
    identifiers(&request.entry_id, &request.request_id)?;
    let hash = digest(&request)?;
    let mut tx = pool.begin_write().await.map_err(crate::boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, &request.intent)?;
    lock_workspace(&mut tx, &actor.workspace_id).await?;
    authorize_workspace(&mut tx, actor).await?;
    let key = RequestIdentity {
        actor,
        operation: "ingredient.archive.v1",
        request_id: &request.request_id,
        payload_digest: &hash,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        let entry: IngredientEntry = serde_json::from_value(result)?;
        tx.commit().await.map_err(crate::boundary_error)?;
        return Ok(entry);
    }
    if request.expected_metadata_revision < 1 {
        return Err(Error::Invalid("metadata revision"));
    }
    let current = krausen_brewery::catalogue_store::load_entry(
        &mut tx,
        &actor.workspace_id,
        &request.entry_id,
        true,
    )
    .await
    .map_err(crate::boundary_error)?
    .ok_or(Error::Invalid("ingredient entry unavailable"))?;
    authorize_workspace(&mut tx, actor).await?;
    if current.metadata_revision != request.expected_metadata_revision {
        return Err(Error::Conflict {
            expected: request.expected_metadata_revision,
            actual: current.metadata_revision,
        });
    }
    let changed = current.archived != request.archived;
    if changed {
        if !krausen_brewery::catalogue_store::archive(
            &mut tx,
            &actor.workspace_id,
            &request.entry_id,
            request.archived,
            current.metadata_revision,
            &actor.actor_id,
            chrono::Utc::now().timestamp(),
        )
        .await
        .map_err(crate::boundary_error)?
        {
            return Err(Error::Retryable);
        }
    }
    let entry = krausen_brewery::catalogue_store::load_entry(
        &mut tx,
        &actor.workspace_id,
        &request.entry_id,
        false,
    )
    .await
    .map_err(crate::boundary_error)?
    .ok_or(Error::Retryable)?;
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&entry)?,
        Some(&entry.entry_id),
    )
    .await?;
    if changed {
        record_change(
            &mut tx,
            actor,
            "ingredient.archive.v1",
            &entry.entry_id,
            entry.metadata_revision,
        )
        .await?;
    }
    tx.commit().await.map_err(crate::boundary_error)?;
    Ok(entry)
}

/// Resolve every new or changed ingredient source in the caller's transaction.
///
/// Unchanged rows keep their historical copies without consulting the
/// catalogue, so later edits/archives never invalidate saved recipes. A new or
/// changed source must resolve inside the effective workspace (or the starter
/// exception), be non-archived, and never claim a version newer than the entry.
pub async fn resolve_draft_sources(
    tx: &mut DbTx<'_>,
    workspace: &str,
    draft: &RecipeDocumentV1,
    previous: Option<&RecipeDocumentV1>,
) -> Result<bool, Error> {
    macro_rules! resolve_rows {
        ($field:ident) => {
            for row in &draft.$field {
                let Some(source) = &row.source else { continue };
                let preserved = previous.is_some_and(|old| {
                    old.$field.iter().any(|prior| {
                        prior.row_id == row.row_id && prior.source.as_ref() == Some(source)
                    })
                });
                if preserved {
                    continue;
                }
                if !resolve_one(tx, workspace, source).await? {
                    return Ok(false);
                }
            }
        };
    }
    resolve_rows!(fermentables);
    resolve_rows!(hops);
    resolve_rows!(yeast);
    resolve_rows!(other_ingredients);
    Ok(true)
}

async fn resolve_one(
    tx: &mut DbTx<'_>,
    workspace: &str,
    source: &IngredientSource,
) -> Result<bool, Error> {
    // Identity checks first: the declared catalogue/edition and a real
    // version must match the scope. Recipe rows keep their own copied
    // specifications; only identity is validated here, never spec equality,
    // so deliberate recipe-specific edits and historical copies survive.
    match source.scope.as_str() {
        "starter" => {
            if source.catalogue_id.as_deref() != Some("krausen-starter-catalogue-v1") {
                return Ok(false);
            }
        }
        "workspace" => {
            if source.catalogue_id.is_some() {
                return Ok(false);
            }
        }
        _ => return Ok(false),
    }
    let Some(version) = source.version else {
        return Ok(false);
    };
    let Ok(version) = i64::try_from(version) else {
        return Ok(false);
    };
    if version < 1 {
        return Ok(false);
    }
    let entry = krausen_brewery::catalogue_store::resolve_source(tx, workspace, source)
        .await
        .map_err(crate::boundary_error)?;
    let Some(entry) = entry else {
        return Ok(false);
    };
    if entry.archived {
        return Ok(false);
    }
    if entry.scope == CatalogueScope::Workspace && entry.workspace_id.as_deref() != Some(workspace) {
        return Ok(false);
    }
    if version > entry.revision {
        return Ok(false);
    }
    Ok(true)
}

/// Convert errors once for UI and MCP; never reveal foreign entry existence.
pub fn project(result: Result<IngredientEntry, Error>) -> IngredientOutcome {
    match result {
        Ok(entry) => IngredientOutcome::Entry(entry),
        Err(Error::Forbidden) => IngredientOutcome::Forbidden,
        Err(Error::ContextChanged) => IngredientOutcome::ContextChanged,
        Err(Error::IngredientArchived) => IngredientOutcome::Archived,
        Err(Error::DuplicateIngredient) => IngredientOutcome::Duplicate,
        Err(Error::Conflict { expected, actual }) => {
            IngredientOutcome::Conflict { expected, actual }
        }
        Err(Error::RequestIdentityConflict) => IngredientOutcome::RequestIdentityConflict,
        Err(Error::ResultExpired { .. }) => IngredientOutcome::ResultExpired,
        Err(Error::Invalid(_)) => IngredientOutcome::Invalid,
        Err(e) => {
            tracing::warn!(error = %e, "ingredient catalogue operation unavailable");
            IngredientOutcome::Unavailable
        }
    }
}

pub fn project_get(result: Result<Option<IngredientEntry>, Error>) -> IngredientOutcome {
    match result {
        Ok(Some(entry)) => IngredientOutcome::Entry(entry),
        Ok(None) => IngredientOutcome::NotFound,
        Err(error) => project(Err(error)),
    }
}

pub fn project_page(result: Result<IngredientPage, Error>) -> IngredientOutcome {
    match result {
        Ok(page) => IngredientOutcome::Entries(page),
        Err(error) => project(Err(error)),
    }
}
