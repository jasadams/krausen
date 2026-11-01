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

// ── Shared evidence operations (KRA-21) ────────────────────────────────────

use krausen_types::brew::{
    ActualFormulationCommand, ActualFormulationOutcome, ArchiveBrewRequest, BrewArchiveOutcome,
    BrewEvidenceViewOutcome, BrewEvidenceViewRequest, BrewLifecycleOutcome, MeasurementCommand,
    MeasurementOutcome, NoteCommand, NoteOutcome, TransitionBrewRequest,
};

pub async fn brew_evidence_view_shared(
    request: BrewEvidenceViewRequest,
) -> Result<BrewEvidenceViewOutcome, ServerFnError> {
    let request_json = match krausen_types::request_context::checked_json(&request) {
        Ok(json) => json,
        Err(()) => return Ok(BrewEvidenceViewOutcome::Invalid),
    };
    brew_evidence_view(request_json).await
}

#[server(prefix = "/leptos-api")]
pub async fn brew_evidence_view(
    request_json: String,
) -> Result<BrewEvidenceViewOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let request: BrewEvidenceViewRequest = match serde_json::from_str(&request_json) {
        Ok(request) => request,
        Err(_) => return Ok(BrewEvidenceViewOutcome::Invalid),
    };
    Ok(krausen_app::brew_evidence::view(ac.db(), &actor, request).await)
}

pub async fn brew_measurement_shared(
    command: MeasurementCommand,
) -> Result<MeasurementOutcome, ServerFnError> {
    let request_json = match krausen_types::request_context::checked_json(&command) {
        Ok(json) => json,
        Err(()) => return Ok(MeasurementOutcome::Invalid),
    };
    brew_measurement(request_json).await
}

#[server(prefix = "/leptos-api")]
pub async fn brew_measurement(request_json: String) -> Result<MeasurementOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let command: MeasurementCommand = match serde_json::from_str(&request_json) {
        Ok(command) => command,
        Err(_) => return Ok(MeasurementOutcome::Invalid),
    };
    Ok(krausen_app::brew_evidence::measurements(ac.db(), &actor, command).await)
}

pub async fn brew_note_shared(command: NoteCommand) -> Result<NoteOutcome, ServerFnError> {
    let request_json = match krausen_types::request_context::checked_json(&command) {
        Ok(json) => json,
        Err(()) => return Ok(NoteOutcome::Invalid),
    };
    brew_note(request_json).await
}

#[server(prefix = "/leptos-api")]
pub async fn brew_note(request_json: String) -> Result<NoteOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let command: NoteCommand = match serde_json::from_str(&request_json) {
        Ok(command) => command,
        Err(_) => return Ok(NoteOutcome::Invalid),
    };
    Ok(krausen_app::brew_evidence::notes(ac.db(), &actor, command).await)
}

pub async fn brew_actual_shared(
    command: ActualFormulationCommand,
) -> Result<ActualFormulationOutcome, ServerFnError> {
    let request_json = match krausen_types::request_context::checked_json(&command) {
        Ok(json) => json,
        Err(()) => return Ok(ActualFormulationOutcome::Invalid),
    };
    brew_actual(request_json).await
}

#[server(prefix = "/leptos-api")]
pub async fn brew_actual(request_json: String) -> Result<ActualFormulationOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let command: ActualFormulationCommand = match serde_json::from_str(&request_json) {
        Ok(command) => command,
        Err(_) => return Ok(ActualFormulationOutcome::Invalid),
    };
    Ok(krausen_app::brew_evidence::actual(ac.db(), &actor, command).await)
}

pub async fn brew_lifecycle_shared(
    request: TransitionBrewRequest,
) -> Result<BrewLifecycleOutcome, ServerFnError> {
    let request_json = match krausen_types::request_context::checked_json(&request) {
        Ok(json) => json,
        Err(()) => return Ok(BrewLifecycleOutcome::Invalid),
    };
    brew_lifecycle(request_json).await
}

#[server(prefix = "/leptos-api")]
pub async fn brew_lifecycle(request_json: String) -> Result<BrewLifecycleOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let request: TransitionBrewRequest = match serde_json::from_str(&request_json) {
        Ok(request) => request,
        Err(_) => return Ok(BrewLifecycleOutcome::Invalid),
    };
    Ok(krausen_app::brew_evidence::transition(ac.db(), &actor, request).await)
}

pub async fn brew_archive_shared(
    request: ArchiveBrewRequest,
) -> Result<BrewArchiveOutcome, ServerFnError> {
    let request_json = match krausen_types::request_context::checked_json(&request) {
        Ok(json) => json,
        Err(()) => return Ok(BrewArchiveOutcome::Invalid),
    };
    brew_archive(request_json).await
}

#[server(prefix = "/leptos-api")]
pub async fn brew_archive(request_json: String) -> Result<BrewArchiveOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let request: ArchiveBrewRequest = match serde_json::from_str(&request_json) {
        Ok(request) => request,
        Err(_) => return Ok(BrewArchiveOutcome::Invalid),
    };
    Ok(krausen_app::brew_evidence::archive(ac.db(), &actor, request).await)
}
