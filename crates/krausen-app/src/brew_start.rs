// SPDX-License-Identifier: AGPL-3.0-or-later
//! Atomic, authorized brew start with an immutable frozen plan.
use crate::{
    ActorContext, Error, RequestIdentity, authorize_workspace, boundary_error, check_intent,
    db_error, record_change, record_request, recover_request,
};
use krausen_brewery::{
    brew_store::{self, BREW_PLAN_SCHEMA_VERSION},
    native_store::{self, StoredReadError},
};
use krausen_core::{DbPool, DbTx, canonical_json};
use krausen_types::{
    brew::{StartBrewOutcome, StartBrewRequest},
    brewery::{BrewPlanV1, BrewStatusV1},
    formulation::{PreviewOutcome, PreviewRequest},
    request_context::OperationIntent,
};
use uuid::Uuid;

pub async fn start(
    pool: &DbPool,
    actor: &ActorContext,
    request: StartBrewRequest,
) -> StartBrewOutcome {
    match start_inner(pool, actor, request).await {
        Ok(outcome) => outcome,
        Err(Error::Forbidden) => StartBrewOutcome::Forbidden,
        Err(Error::ContextChanged) => StartBrewOutcome::ContextChanged,
        Err(Error::Invalid(_)) => StartBrewOutcome::InvalidRequest,
        Err(Error::RequestIdentityConflict) => StartBrewOutcome::RequestIdentityConflict,
        Err(Error::ResultExpired { resource_id }) => {
            StartBrewOutcome::ResultExpired { resource_id }
        }
        Err(Error::Retryable) => StartBrewOutcome::Retryable,
        Err(error) => {
            tracing::warn!(%error, "brew start unavailable");
            StartBrewOutcome::Unavailable
        }
    }
}

async fn start_inner(
    pool: &DbPool,
    actor: &ActorContext,
    request: StartBrewRequest,
) -> Result<StartBrewOutcome, Error> {
    if request.selected_recipe_revision <= 0
        || request.expected_calculation_context.len() != 64
        || !request
            .expected_calculation_context
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Ok(StartBrewOutcome::InvalidRequest);
    }
    if let Some(expected) = request.expected_head_revision {
        if expected <= 0 || expected != request.selected_recipe_revision {
            return Ok(StartBrewOutcome::InvalidRequest);
        }
    }
    let Ok(_request_id) = Uuid::parse_str(&request.request_id) else {
        return Ok(StartBrewOutcome::InvalidRequest);
    };

    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, &request.intent)?;
    lock_workspace(&mut tx, &actor.workspace_id).await?;
    // The workspace lock may have waited across the credential boundary.
    authorize_workspace(&mut tx, actor).await?;

    let payload = canonical_json::from_serializable(&(
        &request.intent,
        &request.recipe_id,
        request.selected_recipe_revision,
        request.expected_head_revision,
        &request.expected_calculation_context,
    ))
    .map_err(|_| Error::Invalid("start request"))?;
    let payload_digest =
        canonical_json::sha256(&payload).map_err(|_| Error::Invalid("start request"))?;
    let key = RequestIdentity {
        actor,
        operation: "brew.start.v1",
        request_id: &request.request_id,
        payload_digest: &payload_digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        return Ok(serde_json::from_value(result)?);
    }

    let workspace_id = actor.workspace_id.clone();
    let recipe_id = request.recipe_id.clone();
    let selected = request.selected_recipe_revision;
    let expected_head = request.expected_head_revision;
    let intent = OperationIntent {
        account_id: actor.actor_id.clone(),
        workspace_id: workspace_id.clone(),
    };

    let Some(head) = recipe_head(&mut tx, &workspace_id, &recipe_id, false).await? else {
        return Ok(StartBrewOutcome::NotFound);
    };
    if recipe_archived(&mut tx, &workspace_id, &recipe_id).await? {
        return Ok(StartBrewOutcome::RecipeArchived);
    }
    if let Some(expected) = expected_head {
        if head != expected {
            return head_conflict(&mut tx, actor, &workspace_id, &recipe_id, expected, head).await;
        }
    }

    let document = match native_store::load_document(&mut tx, &workspace_id, &recipe_id, selected)
        .await
    {
        Ok(Some(document)) => document,
        Ok(None) => return Ok(StartBrewOutcome::NotFound),
        Err(StoredReadError::UnsupportedVersion(_)) => return Ok(StartBrewOutcome::InvalidRequest),
        Err(StoredReadError::Storage(error)) => return Err(Error::Database(error)),
        Err(StoredReadError::Document(error)) => {
            tracing::error!(error = ?error, "selected brew recipe revision cannot be decoded");
            return Err(Error::Database(krausen_core::Error::Internal(
                "selected brew recipe revision cannot be decoded".into(),
            )));
        }
    };

    if let Some(profile_id) = &document.equipment_profile_id {
        match krausen_brewery::equipment_store::load(&mut tx, &workspace_id, profile_id, true)
            .await
            .map_err(crate::boundary_error)?
        {
            Some(profile) if profile.archived => {
                return Ok(StartBrewOutcome::EquipmentProfileArchived);
            }
            Some(_) => {}
            None => return Ok(StartBrewOutcome::UnresolvedEquipmentProfile),
        }
        // The profile lock may have waited across the credential boundary.
        authorize_workspace(&mut tx, actor).await?;
    }

    // Lock the recipe only after the profile, matching the existing save lock
    // order. The selected revision itself is immutable.
    let Some(head) = recipe_head(&mut tx, &workspace_id, &recipe_id, true).await? else {
        return Ok(StartBrewOutcome::NotFound);
    };
    if recipe_archived(&mut tx, &workspace_id, &recipe_id).await? {
        return Ok(StartBrewOutcome::RecipeArchived);
    }
    authorize_workspace(&mut tx, actor).await?;
    if let Some(expected) = expected_head {
        if head != expected {
            return head_conflict(&mut tx, actor, &workspace_id, &recipe_id, expected, head).await;
        }
    }

    let preview = crate::formulation_kernel::resolve_in_tx(
        &mut tx,
        actor,
        PreviewRequest {
            draft_generation: "brew-start".into(),
            intent,
            draft: document.clone(),
            equipment_draft: Default::default(),
        },
    )
    .await?;
    let preview = match preview {
        PreviewOutcome::Preview(preview) => preview,
        PreviewOutcome::UnresolvedProfile => {
            return Ok(StartBrewOutcome::UnresolvedEquipmentProfile);
        }
        PreviewOutcome::UnresolvedStyle => {
            return Ok(StartBrewOutcome::UnresolvedStyle);
        }
        PreviewOutcome::Forbidden => return Ok(StartBrewOutcome::Forbidden),
        PreviewOutcome::ContextChanged => return Ok(StartBrewOutcome::ContextChanged),
        PreviewOutcome::Invalid => return Ok(StartBrewOutcome::InvalidRequest),
        PreviewOutcome::Unavailable => return Ok(StartBrewOutcome::Unavailable),
    };
    if !preview.validation.startable() {
        return Ok(StartBrewOutcome::NotReady(preview.validation));
    }
    let Some(context) = preview.context.clone() else {
        return Ok(StartBrewOutcome::NotReady(preview.validation));
    };
    let Some(context_id) = preview.context_digest.clone() else {
        return Ok(StartBrewOutcome::NotReady(preview.validation));
    };
    if context_id != request.expected_calculation_context {
        return Ok(StartBrewOutcome::ContextConflict { refreshed: preview });
    }

    let plan = BrewPlanV1 {
        schema_version: 1,
        style: document.style.clone(),
        recipe: document,
        recipe_revision: selected,
        equipment: context.effective_equipment.clone(),
        equipment_profile_id: context
            .equipment_profile
            .as_ref()
            .map(|profile| profile.profile_id.clone()),
        equipment_profile_revision: context
            .equipment_profile
            .as_ref()
            .map(|profile| profile.revision),
        equipment_profile_metadata_revision: context
            .equipment_profile
            .as_ref()
            .map(|profile| profile.metadata_revision),
        original_targets: preview.targets.clone(),
        assumptions: context.assumptions.clone(),
        applied_defaults: context.applied_defaults.clone(),
        diagnostics: preview.validation.diagnostics.clone(),
        style_reference: preview.style.clone(),
        style_comparison: preview.style_comparison.clone(),
        engine_version: context.engine_version.clone(),
        engine_build: context.engine_build.clone(),
        engine_source_hash: context.engine_source_hash.clone(),
        policy_version: context.policy_version.clone(),
        policy_source_hash: context.policy_source_hash.clone(),
        native_schema_version: context.native_schema_version,
        result_schema_version: context.result_schema_version,
        context_id: context_id.clone(),
        calculation_context: context,
    };
    let (plan_json, plan_digest) = brew_store::encode_plan(&plan).map_err(|error| {
        tracing::error!(error = %error, "brew plan cannot be encoded");
        Error::Database(krausen_core::Error::Internal(
            "brew plan cannot be encoded".into(),
        ))
    })?;
    let brew_id = Uuid::new_v4().to_string();
    let started_at = chrono::Utc::now().timestamp();
    brew_store::insert(
        &mut tx,
        &workspace_id,
        &brew_id,
        &recipe_id,
        selected,
        BrewStatusV1::Started,
        1,
        started_at,
        &actor.actor_id,
        BREW_PLAN_SCHEMA_VERSION,
        &plan_json,
        &plan_digest,
    )
    .await
    .map_err(boundary_error)?;

    let outcome = StartBrewOutcome::Started {
        brew_id: brew_id.clone(),
        status: BrewStatusV1::Started,
        revision: 1,
        recipe_id: recipe_id.clone(),
        selected_recipe_revision: selected,
        started_at,
        started_by: actor.actor_id.clone(),
        plan,
    };
    let result = serde_json::to_value(&outcome)?;
    record_request(&mut tx, &key, &result, Some(&brew_id)).await?;
    record_change(&mut tx, actor, "brew.start.v1", &brew_id, 1).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(outcome)
}

async fn lock_workspace(tx: &mut DbTx<'_>, workspace_id: &str) -> Result<(), Error> {
    let sql = if matches!(&tx, DbTx::Postgres(_)) {
        "SELECT revision FROM workspaces WHERE workspace_id = $1 AND status = 'active' FOR UPDATE"
    } else {
        "SELECT revision FROM workspaces WHERE workspace_id = $1 AND status = 'active'"
    };
    let found = scalar!(tx, i64, sql, workspace_id).map_err(db_error)?;
    if found.is_none() {
        return Err(Error::Forbidden);
    }
    Ok(())
}

async fn recipe_head(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    recipe_id: &str,
    lock: bool,
) -> Result<Option<i64>, Error> {
    let sql = if lock && matches!(&tx, DbTx::Postgres(_)) {
        "SELECT native_head_revision FROM recipes WHERE workspace_id = $1 AND recipe_id = $2 FOR SHARE"
    } else {
        "SELECT native_head_revision FROM recipes WHERE workspace_id = $1 AND recipe_id = $2"
    };
    scalar!(tx, i64, sql, workspace_id, recipe_id).map_err(db_error)
}

async fn recipe_archived(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    recipe_id: &str,
) -> Result<bool, Error> {
    let count = scalar!(
        tx,
        i64,
        "SELECT COUNT(*) FROM recipes WHERE workspace_id = $1 AND recipe_id = $2 AND deleted_at IS NOT NULL",
        workspace_id,
        recipe_id
    )
    .map_err(db_error)?;
    Ok(count.unwrap_or(0) > 0)
}

async fn head_conflict(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    workspace_id: &str,
    recipe_id: &str,
    expected_head: i64,
    actual_head: i64,
) -> Result<StartBrewOutcome, Error> {
    let refreshed = if actual_head > 0 {
        match native_store::load_document(tx, workspace_id, recipe_id, actual_head).await {
            Ok(Some(document)) => {
                let intent = OperationIntent {
                    account_id: actor.actor_id.clone(),
                    workspace_id: workspace_id.to_owned(),
                };
                match crate::formulation_kernel::resolve_in_tx(
                    tx,
                    actor,
                    PreviewRequest {
                        draft_generation: "brew-start-head-conflict".into(),
                        intent,
                        draft: document,
                        equipment_draft: Default::default(),
                    },
                )
                .await?
                {
                    PreviewOutcome::Preview(preview) => Some(preview),
                    _ => None,
                }
            }
            _ => None,
        }
    } else {
        None
    };
    Ok(StartBrewOutcome::HeadConflict {
        expected_head,
        actual_head,
        refreshed,
    })
}
