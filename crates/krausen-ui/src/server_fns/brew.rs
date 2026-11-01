// SPDX-License-Identifier: AGPL-3.0-or-later
//! Browser transport for shared atomic brew start.
#[cfg(feature = "ssr")]
use super::AuthenticatedContext;
use krausen_types::brew::{StartBrewOutcome, StartBrewRequest};
use leptos::prelude::*;

/// Serialize nested native data through the same checked envelope used by the
/// other native commands.
pub async fn start_brew_shared(
    request: StartBrewRequest,
) -> Result<StartBrewOutcome, ServerFnError> {
    let request_json = match krausen_types::request_context::checked_json(&request) {
        Ok(json) => json,
        Err(()) => return Ok(StartBrewOutcome::InvalidRequest),
    };
    start_brew(request_json).await
}

#[server(prefix = "/leptos-api")]
pub async fn start_brew(request_json: String) -> Result<StartBrewOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let request: StartBrewRequest = match serde_json::from_str(&request_json) {
        Ok(request) => request,
        Err(_) => return Ok(StartBrewOutcome::InvalidRequest),
    };
    Ok(krausen_app::brew_start::start(ac.db(), &actor, request).await)
}

/// Shared browser-adapter execution used by parity tests and other server code.
#[cfg(feature = "ssr")]
pub async fn execute_start_brew(
    db: &krausen_core::DbPool,
    actor: &krausen_app::ActorContext,
    request: StartBrewRequest,
) -> StartBrewOutcome {
    krausen_app::brew_start::start(db, actor, request).await
}
