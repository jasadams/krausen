// SPDX-License-Identifier: AGPL-3.0-or-later
//! Browser transport for shared formulation operations.
use krausen_types::formulation::{PreviewOutcome, PreviewRequest, StyleListOutcome};
use leptos::prelude::*;

/// Serialize nested native data in a JSON envelope without changing its typed contract.
pub async fn preview_formulation_shared(
    request: PreviewRequest,
) -> Result<PreviewOutcome, ServerFnError> {
    let request_json = match krausen_types::formulation::checked_request_json(&request) {
        Ok(json) => json,
        Err(()) => return Ok(PreviewOutcome::Invalid),
    };
    preview_formulation(request_json).await
}
#[server(prefix = "/leptos-api")]
pub async fn preview_formulation(request_json: String) -> Result<PreviewOutcome, ServerFnError> {
    let ac = super::AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let request: PreviewRequest = match serde_json::from_str(&request_json) {
        Ok(request) => request,
        Err(_) => return Ok(PreviewOutcome::Invalid),
    };
    Ok(execute_preview(ac.db(), &actor, request).await)
}
#[server(prefix = "/leptos-api")]
pub async fn list_formulation_styles(
    account_id: String,
    workspace_id: String,
) -> Result<StyleListOutcome, ServerFnError> {
    let ac = super::AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let intent = krausen_types::request_context::OperationIntent {
        account_id,
        workspace_id,
    };
    Ok(execute_styles(ac.db(), &actor, &intent).await)
}

#[cfg(feature = "ssr")]
pub async fn execute_preview(
    db: &krausen_core::DbPool,
    actor: &krausen_app::ActorContext,
    request: PreviewRequest,
) -> PreviewOutcome {
    krausen_app::formulation::project_preview(
        krausen_app::formulation::preview(db, actor, request).await,
    )
}
#[cfg(feature = "ssr")]
pub async fn execute_styles(
    db: &krausen_core::DbPool,
    actor: &krausen_app::ActorContext,
    intent: &krausen_types::request_context::OperationIntent,
) -> StyleListOutcome {
    krausen_app::formulation::project_styles(
        krausen_app::formulation::list_styles(db, actor, intent).await,
    )
}
