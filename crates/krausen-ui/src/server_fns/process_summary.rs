// SPDX-License-Identifier: AGPL-3.0-or-later
//! Browser transport for the shared structured recipe process summary.
use krausen_types::process_summary::{ProcessSummaryOutcome, ProcessSummaryRequest};
use leptos::prelude::*;

/// Serialize nested native data in a JSON envelope without changing its typed
/// contract or letting non-finite numbers become null.
pub async fn request_process_summary_shared(
    request: ProcessSummaryRequest,
) -> Result<ProcessSummaryOutcome, ServerFnError> {
    let request_json = match krausen_types::request_context::checked_json(&request) {
        Ok(json) => json,
        Err(()) => return Ok(ProcessSummaryOutcome::InvalidRequest),
    };
    request_process_summary(request_json).await
}

#[server(prefix = "/leptos-api")]
pub async fn request_process_summary(
    request_json: String,
) -> Result<ProcessSummaryOutcome, ServerFnError> {
    let ac = super::AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let request: ProcessSummaryRequest = match serde_json::from_str(&request_json) {
        Ok(request) => request,
        Err(_) => return Ok(ProcessSummaryOutcome::InvalidRequest),
    };
    Ok(krausen_app::process_summary::read(ac.db(), &actor, request).await)
}

/// Shared browser-adapter execution used by parity tests and other server code.
#[cfg(feature = "ssr")]
pub async fn execute_process_summary(
    db: &krausen_core::DbPool,
    actor: &krausen_app::ActorContext,
    request: ProcessSummaryRequest,
) -> ProcessSummaryOutcome {
    krausen_app::process_summary::read(db, actor, request).await
}
