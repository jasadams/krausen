// SPDX-License-Identifier: AGPL-3.0-or-later
//! Authorized equipment profile operations. Every write owns one DbTx.
use crate::{
    ActorContext, Error, RequestIdentity, authorize_workspace, check_intent, record_change,
    record_request, recover_request,
};
use krausen_core::{DbPool, DbTx, canonical_json};
use krausen_types::{
    brewery::{EquipmentSettingsV1, RecipeDocumentV1},
    equipment::{ArchiveEquipment, EquipmentOutcome, EquipmentProfile, SaveEquipment},
    request_context::OperationIntent,
    validation::Readiness,
};

fn validate(settings: &EquipmentSettingsV1) -> Result<(), Error> {
    if settings.schema_version != 1 {
        return Err(Error::Invalid("equipment schema"));
    }
    let result =
        krausen_brewery::validation::validate_draft(&RecipeDocumentV1::default(), settings);
    if result
        .diagnostics
        .iter()
        .any(|d| d.path.starts_with("equipment.") && d.severity == Readiness::Invalid)
    {
        return Err(Error::Invalid("equipment settings"));
    }
    canonical_json::from_serializable(settings)
        .map_err(|_| Error::Invalid("equipment encoding"))?;
    Ok(())
}
fn digest<T: serde::Serialize>(value: &T) -> Result<String, Error> {
    let v =
        canonical_json::from_serializable(value).map_err(|_| Error::Invalid("request encoding"))?;
    canonical_json::sha256(&v).map_err(|_| Error::Invalid("request encoding"))
}
fn identifiers(profile: &str, request: &str) -> Result<(), Error> {
    uuid::Uuid::parse_str(profile).map_err(|_| Error::Invalid("profile id"))?;
    uuid::Uuid::parse_str(request).map_err(|_| Error::Invalid("request id"))?;
    Ok(())
}
/// Serialize writes and retries against this workspace, including first creation.
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
/// Return an authorized profile, including archived history when requested.
pub async fn get(
    pool: &DbPool,
    actor: &ActorContext,
    intent: &OperationIntent,
    id: &str,
) -> Result<Option<EquipmentProfile>, Error> {
    let mut tx = pool.begin_write().await.map_err(crate::boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, intent)?;
    let profile = krausen_brewery::equipment_store::load(&mut tx, &actor.workspace_id, id, false)
        .await
        .map_err(crate::boundary_error)?;
    tx.commit().await.map_err(crate::boundary_error)?;
    Ok(profile)
}
/// Stable bounded list; callers retain the last profile ID as cursor.
pub async fn list(
    pool: &DbPool,
    actor: &ActorContext,
    intent: &OperationIntent,
    after: &str,
    limit: i64,
    include_archived: bool,
) -> Result<Vec<EquipmentProfile>, Error> {
    if !(1..=100).contains(&limit) {
        return Err(Error::Invalid("list limit"));
    }
    let mut tx = pool.begin_write().await.map_err(crate::boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, intent)?;
    let profiles = krausen_brewery::equipment_store::list(
        &mut tx,
        &actor.workspace_id,
        after,
        limit,
        include_archived,
    )
    .await
    .map_err(crate::boundary_error)?;
    tx.commit().await.map_err(crate::boundary_error)?;
    Ok(profiles)
}
/// Create at revision zero or append a guarded complete settings revision.
pub async fn save(
    pool: &DbPool,
    actor: &ActorContext,
    request: SaveEquipment,
) -> Result<EquipmentProfile, Error> {
    identifiers(&request.profile_id, &request.request_id)?;
    let name = request.name.trim();
    let hash = digest(&(
        &request.intent,
        &request.profile_id,
        name,
        &request.settings,
        &request.provenance,
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
        operation: "equipment.save.v1",
        request_id: &request.request_id,
        payload_digest: &hash,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        let p = serde_json::from_value(result)?;
        tx.commit().await.map_err(crate::boundary_error)?;
        return Ok(p);
    }
    validate(&request.settings)?;
    if request.provenance.source.trim().is_empty()
        || request.provenance.source.chars().count() > 255
        || request.provenance.assumptions.len() > 64
        || request
            .provenance
            .assumptions
            .iter()
            .any(|s| s.trim().is_empty() || s.chars().count() > 255)
    {
        return Err(Error::Invalid("equipment provenance"));
    }
    if name.is_empty()
        || name.chars().count() > 255
        || request.expected_revision < 0
        || request.expected_metadata_revision < 0
    {
        return Err(Error::Invalid("profile name or revision"));
    }
    let current = krausen_brewery::equipment_store::load(
        &mut tx,
        &actor.workspace_id,
        &request.profile_id,
        true,
    )
    .await
    .map_err(crate::boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    let now = chrono::Utc::now().timestamp();
    let mut changed = true;
    match current {
        None if request.expected_revision == 0 && request.expected_metadata_revision == 0 => {
            krausen_brewery::equipment_store::create(
                &mut tx,
                &actor.workspace_id,
                &request.profile_id,
                name,
                &request.settings,
                &request.provenance,
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
        Some(p) if p.archived => return Err(Error::EquipmentArchived),
        Some(p) if p.revision != request.expected_revision => {
            return Err(Error::Conflict {
                expected: request.expected_revision,
                actual: p.revision,
            });
        }
        Some(p) if p.metadata_revision != request.expected_metadata_revision => {
            return Err(Error::Conflict {
                expected: request.expected_metadata_revision,
                actual: p.metadata_revision,
            });
        }
        Some(p)
            if p.name == name
                && p.settings == request.settings
                && p.provenance == request.provenance =>
        {
            changed = false;
        }
        Some(_) => {
            if !krausen_brewery::equipment_store::append(
                &mut tx,
                &actor.workspace_id,
                &request.profile_id,
                name,
                &request.settings,
                &request.provenance,
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
    let profile = krausen_brewery::equipment_store::load(
        &mut tx,
        &actor.workspace_id,
        &request.profile_id,
        false,
    )
    .await
    .map_err(crate::boundary_error)?
    .ok_or(Error::Retryable)?;
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&profile)?,
        Some(&profile.profile_id),
    )
    .await?;
    if changed {
        record_change(
            &mut tx,
            actor,
            "equipment.save.v1",
            &profile.profile_id,
            profile.revision,
        )
        .await?;
    }
    tx.commit().await.map_err(crate::boundary_error)?;
    Ok(profile)
}
/// Archive/unarchive is guarded and appends retained metadata history.
pub async fn set_archived(
    pool: &DbPool,
    actor: &ActorContext,
    request: ArchiveEquipment,
) -> Result<EquipmentProfile, Error> {
    identifiers(&request.profile_id, &request.request_id)?;
    let hash = digest(&request)?;
    let mut tx = pool.begin_write().await.map_err(crate::boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, &request.intent)?;
    lock_workspace(&mut tx, &actor.workspace_id).await?;
    authorize_workspace(&mut tx, actor).await?;
    let key = RequestIdentity {
        actor,
        operation: "equipment.archive.v1",
        request_id: &request.request_id,
        payload_digest: &hash,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        let p = serde_json::from_value(result)?;
        tx.commit().await.map_err(crate::boundary_error)?;
        return Ok(p);
    }
    if request.expected_metadata_revision < 1 {
        return Err(Error::Invalid("metadata revision"));
    }
    let current = krausen_brewery::equipment_store::load(
        &mut tx,
        &actor.workspace_id,
        &request.profile_id,
        true,
    )
    .await
    .map_err(crate::boundary_error)?
    .ok_or(Error::Invalid("profile unavailable"))?;
    authorize_workspace(&mut tx, actor).await?;
    if current.metadata_revision != request.expected_metadata_revision {
        return Err(Error::Conflict {
            expected: request.expected_metadata_revision,
            actual: current.metadata_revision,
        });
    }
    let changed = current.archived != request.archived;
    if changed {
        if !krausen_brewery::equipment_store::archive(
            &mut tx,
            &actor.workspace_id,
            &request.profile_id,
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
    let profile = krausen_brewery::equipment_store::load(
        &mut tx,
        &actor.workspace_id,
        &request.profile_id,
        false,
    )
    .await
    .map_err(crate::boundary_error)?
    .ok_or(Error::Retryable)?;
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&profile)?,
        Some(&profile.profile_id),
    )
    .await?;
    if changed {
        record_change(
            &mut tx,
            actor,
            "equipment.archive.v1",
            &profile.profile_id,
            profile.metadata_revision,
        )
        .await?;
    }
    tx.commit().await.map_err(crate::boundary_error)?;
    Ok(profile)
}
/// Convert errors once for UI and MCP; never reveal foreign profile existence.
pub fn project(result: Result<EquipmentProfile, Error>) -> EquipmentOutcome {
    match result {
        Ok(p) => EquipmentOutcome::Profile(p),
        Err(Error::Forbidden) => EquipmentOutcome::Forbidden,
        Err(Error::ContextChanged) => EquipmentOutcome::ContextChanged,
        Err(Error::EquipmentArchived) => EquipmentOutcome::Archived,
        Err(Error::Conflict { expected, actual }) => {
            EquipmentOutcome::Conflict { expected, actual }
        }
        Err(Error::RequestIdentityConflict) => EquipmentOutcome::RequestIdentityConflict,
        Err(Error::ResultExpired { .. }) => EquipmentOutcome::ResultExpired,
        Err(Error::Invalid(_)) => EquipmentOutcome::Invalid,
        Err(e) => {
            tracing::warn!(error=%e,"equipment operation unavailable");
            EquipmentOutcome::Unavailable
        }
    }
}

/// Read a retained historical profile revision under current workspace authority.
pub async fn get_revision(
    pool: &DbPool,
    actor: &ActorContext,
    intent: &OperationIntent,
    id: &str,
    revision: i64,
) -> Result<Option<krausen_types::equipment::EquipmentRevision>, Error> {
    if revision < 1 {
        return Err(Error::Invalid("profile revision"));
    }
    let mut tx = pool.begin_write().await.map_err(crate::boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, intent)?;
    let result =
        krausen_brewery::equipment_store::load_revision(&mut tx, &actor.workspace_id, id, revision)
            .await
            .map_err(crate::boundary_error)?;
    tx.commit().await.map_err(crate::boundary_error)?;
    Ok(result)
}

pub fn project_get(result: Result<Option<EquipmentProfile>, Error>) -> EquipmentOutcome {
    match result {
        Ok(Some(profile)) => EquipmentOutcome::Profile(profile),
        Ok(None) => EquipmentOutcome::NotFound,
        Err(error) => project(Err(error)),
    }
}
pub fn project_list(result: Result<Vec<EquipmentProfile>, Error>) -> EquipmentOutcome {
    match result {
        Ok(profiles) => EquipmentOutcome::Profiles(profiles),
        Err(error) => project(Err(error)),
    }
}
pub fn project_revision(
    result: Result<Option<krausen_types::equipment::EquipmentRevision>, Error>,
) -> EquipmentOutcome {
    match result {
        Ok(Some(revision)) => EquipmentOutcome::Revision(revision),
        Ok(None) => EquipmentOutcome::NotFound,
        Err(error) => project(Err(error)),
    }
}
