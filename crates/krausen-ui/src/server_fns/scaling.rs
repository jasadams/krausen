// SPDX-License-Identifier: AGPL-3.0-or-later
//! Browser transport for shared scaling and duplication operations.
use krausen_types::scaling::{
    DuplicateOutcome, DuplicateRecipe, ScaleApplyOutcome, ScaleApplyRequest, ScaleOutcome,
    ScalePreviewRequest, ScaleProblem,
};
use leptos::prelude::*;

pub async fn scale_preview_shared(
    request: ScalePreviewRequest,
) -> Result<ScaleOutcome, ServerFnError> {
    let Ok(json) = krausen_types::request_context::checked_json(&request) else {
        return Ok(ScaleOutcome::Unscalable(ScaleProblem::InvalidTargetVolume));
    };
    execute_scale_preview(json).await
}

#[server(prefix = "/leptos-api")]
pub async fn execute_scale_preview(request_json: String) -> Result<ScaleOutcome, ServerFnError> {
    let ac = super::AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let Ok(request) = serde_json::from_str(&request_json) else {
        return Ok(ScaleOutcome::Unscalable(ScaleProblem::InvalidTargetVolume));
    };
    Ok(execute_preview(ac.db(), &actor, request).await)
}

pub async fn scale_apply_shared(
    request: ScaleApplyRequest,
) -> Result<ScaleApplyOutcome, ServerFnError> {
    let Ok(json) = krausen_types::request_context::checked_json(&request) else {
        return Ok(ScaleApplyOutcome::InvalidRequest);
    };
    execute_scale_apply(json).await
}

#[server(prefix = "/leptos-api")]
pub async fn execute_scale_apply(request_json: String) -> Result<ScaleApplyOutcome, ServerFnError> {
    let ac = super::AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let Ok(request) = serde_json::from_str(&request_json) else {
        return Ok(ScaleApplyOutcome::InvalidRequest);
    };
    Ok(execute_apply(ac.db(), &actor, request).await)
}

pub async fn duplicate_recipe_shared(
    request: DuplicateRecipe,
) -> Result<DuplicateOutcome, ServerFnError> {
    let Ok(json) = krausen_types::request_context::checked_json(&request) else {
        return Ok(DuplicateOutcome::InvalidRequest);
    };
    execute_duplicate_recipe(json).await
}

#[server(prefix = "/leptos-api")]
pub async fn execute_duplicate_recipe(
    request_json: String,
) -> Result<DuplicateOutcome, ServerFnError> {
    let ac = super::AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let Ok(request) = serde_json::from_str(&request_json) else {
        return Ok(DuplicateOutcome::InvalidRequest);
    };
    Ok(execute_duplicate(ac.db(), &actor, request).await)
}

#[cfg(feature = "ssr")]
pub async fn execute_preview(
    db: &krausen_core::DbPool,
    actor: &krausen_app::ActorContext,
    request: ScalePreviewRequest,
) -> ScaleOutcome {
    krausen_app::scaling::preview(db, actor, request).await
}

#[cfg(feature = "ssr")]
pub async fn execute_apply(
    db: &krausen_core::DbPool,
    actor: &krausen_app::ActorContext,
    request: ScaleApplyRequest,
) -> ScaleApplyOutcome {
    krausen_app::scaling::apply(db, actor, request).await
}

#[cfg(feature = "ssr")]
pub async fn execute_duplicate(
    db: &krausen_core::DbPool,
    actor: &krausen_app::ActorContext,
    request: DuplicateRecipe,
) -> DuplicateOutcome {
    krausen_app::scaling::duplicate(db, actor, request).await
}
