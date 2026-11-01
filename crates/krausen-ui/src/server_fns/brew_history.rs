// SPDX-License-Identifier: AGPL-3.0-or-later
//! Browser transport for shared brew history, comparison and feedback.
use krausen_types::brew_history::{
    BrewComparisonOutcome, BrewComparisonRequest, BrewFeedbackApplyOutcome,
    BrewFeedbackApplyRequest, BrewFeedbackPreviewOutcome, BrewFeedbackPreviewRequest,
    BrewPrintOutcome, BrewPrintRequest, RecipeBrewHistoryOutcome, RecipeBrewHistoryQuery,
};
use leptos::prelude::*;

pub async fn brew_history_shared(
    request: RecipeBrewHistoryQuery,
) -> Result<RecipeBrewHistoryOutcome, ServerFnError> {
    let Ok(json) = krausen_types::request_context::checked_json(&request) else {
        return Ok(RecipeBrewHistoryOutcome::InvalidRequest);
    };
    execute_brew_history(json).await
}
#[server(prefix = "/leptos-api")]
pub async fn execute_brew_history(
    request_json: String,
) -> Result<RecipeBrewHistoryOutcome, ServerFnError> {
    let ac = super::AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let Ok(request) = serde_json::from_str(&request_json) else {
        return Ok(RecipeBrewHistoryOutcome::InvalidRequest);
    };
    Ok(krausen_app::brew_history::history(ac.db(), &actor, request).await)
}

pub async fn brew_comparison_shared(
    request: BrewComparisonRequest,
) -> Result<BrewComparisonOutcome, ServerFnError> {
    let Ok(json) = krausen_types::request_context::checked_json(&request) else {
        return Ok(BrewComparisonOutcome::Unavailable);
    };
    execute_brew_comparison(json).await
}
#[server(prefix = "/leptos-api")]
pub async fn execute_brew_comparison(
    request_json: String,
) -> Result<BrewComparisonOutcome, ServerFnError> {
    let ac = super::AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let Ok(request) = serde_json::from_str(&request_json) else {
        return Ok(BrewComparisonOutcome::Unavailable);
    };
    Ok(krausen_app::brew_history::comparison(ac.db(), &actor, request).await)
}

pub async fn brew_feedback_preview_shared(
    request: BrewFeedbackPreviewRequest,
) -> Result<BrewFeedbackPreviewOutcome, ServerFnError> {
    let Ok(json) = krausen_types::request_context::checked_json(&request) else {
        return Ok(BrewFeedbackPreviewOutcome::InvalidRequest);
    };
    execute_brew_feedback_preview(json).await
}
#[server(prefix = "/leptos-api")]
pub async fn execute_brew_feedback_preview(
    request_json: String,
) -> Result<BrewFeedbackPreviewOutcome, ServerFnError> {
    let ac = super::AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let Ok(request) = serde_json::from_str(&request_json) else {
        return Ok(BrewFeedbackPreviewOutcome::InvalidRequest);
    };
    Ok(krausen_app::brew_history::feedback_preview(ac.db(), &actor, request).await)
}

pub async fn brew_feedback_apply_shared(
    request: BrewFeedbackApplyRequest,
) -> Result<BrewFeedbackApplyOutcome, ServerFnError> {
    let Ok(json) = krausen_types::request_context::checked_json(&request) else {
        return Ok(BrewFeedbackApplyOutcome::InvalidRequest);
    };
    execute_brew_feedback_apply(json).await
}
#[server(prefix = "/leptos-api")]
pub async fn execute_brew_feedback_apply(
    request_json: String,
) -> Result<BrewFeedbackApplyOutcome, ServerFnError> {
    let ac = super::AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let Ok(request) = serde_json::from_str(&request_json) else {
        return Ok(BrewFeedbackApplyOutcome::InvalidRequest);
    };
    Ok(krausen_app::brew_history::feedback_apply(ac.db(), &actor, request).await)
}

pub async fn brew_print_shared(
    request: BrewPrintRequest,
) -> Result<BrewPrintOutcome, ServerFnError> {
    let Ok(json) = krausen_types::request_context::checked_json(&request) else {
        return Ok(BrewPrintOutcome::InvalidRequest);
    };
    execute_brew_print(json).await
}
#[server(prefix = "/leptos-api")]
pub async fn execute_brew_print(request_json: String) -> Result<BrewPrintOutcome, ServerFnError> {
    let ac = super::AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let Ok(request) = serde_json::from_str(&request_json) else {
        return Ok(BrewPrintOutcome::InvalidRequest);
    };
    Ok(krausen_app::brew_history::historical_print(ac.db(), &actor, request).await)
}
