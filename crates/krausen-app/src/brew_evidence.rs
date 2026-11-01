// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared brew evidence, selection, lifecycle and archive operations.
//!
//! Every public operation owns one application transaction, rechecks current
//! authority under the workspace lock, locks the brew aggregate for writes and
//! records request/change evidence before committing. UI and MCP adapters call
//! these operations; neither transport implements its own rules.

use crate::{
    ActorContext, Error, RequestIdentity, authorize_workspace, boundary_error, check_intent,
    record_change, record_request, recover_request,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use krausen_brewery::{
    brew_evidence_store as store,
    brew_store::{self, StoredBrew},
};
use krausen_core::{DbPool, DbTx, canonical_json};
use krausen_types::{
    brew::{
        ActualFormulationCommand, ActualFormulationHistoryRequest, ActualFormulationOutcome,
        ActualFormulationPage, ActualFormulationViewRequest, AmendActualFormulationRequest,
        ArchiveBrewRequest, BrewArchiveOutcome, BrewEvidenceCommand, BrewEvidenceOutcome,
        BrewEvidenceView, BrewEvidenceViewOutcome, BrewEvidenceViewRequest, BrewLifecycleOutcome,
        BrewTransitionV1, CorrectMeasurementRequest, CorrectNoteRequest,
        EffectiveActualFormulation, MeasurementCommand, MeasurementListRequest, MeasurementOutcome,
        MeasurementPage, MeasurementSlot, NoteCommand, NoteHistoryPage, NoteListRequest,
        NoteOutcome, NotePage, NoteView, RecordMeasurementRequest, RecordNoteRequest,
        RetractMeasurementRequest, RetractNoteRequest, SelectMeasurementRequest,
        TransitionBrewRequest,
    },
    brewery::{
        ActualFormulationRevision, ActualFormulationV1, ActualSelection, ActualSelectionEvent,
        BrewArchiveEvent, BrewLifecycleEvent, BrewPlanV1, BrewStatusV1, MeasurementEntry,
        MeasurementMetadataV1, ObservationNote, ObservationNoteV1, ObservationRevision, ProcessV1,
        QUANTITIES, QuantityDefinition, quantity_definition,
    },
    request_context::OperationIntent,
};
use serde::Serialize;
use std::collections::HashSet;
use uuid::Uuid;

/// Current notes shown on the overview; the paginated list operation exposes
/// the rest.
const VIEW_NOTE_LIMIT: i64 = 50;
const VIEW_LIFECYCLE_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 100;
const MAX_SOURCE_LEN: usize = 200;
const MAX_NOTE_LEN: usize = 20_000;
const MAX_REASON_LEN: usize = 2_000;
const MAX_METADATA_JSON_LEN: usize = 16_384;
const MAX_ACTUAL_JSON_LEN: usize = 262_144;

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn valid_request_id(value: &str) -> bool {
    Uuid::parse_str(value).is_ok()
}

fn validate_identifier(value: &str, field: &'static str) -> Result<(), Error> {
    if value.is_empty() || value.len() > 64 {
        return Err(Error::Invalid(field));
    }
    Ok(())
}

fn validate_source(value: &str) -> Result<(), Error> {
    let trimmed = value.trim();
    if trimmed.is_empty() || value.len() > MAX_SOURCE_LEN || value.chars().any(char::is_control) {
        return Err(Error::Invalid("measurement source"));
    }
    Ok(())
}

fn validate_text(value: &str, field: &'static str) -> Result<(), Error> {
    if value.trim().is_empty() || value.len() > MAX_NOTE_LEN {
        return Err(Error::Invalid(field));
    }
    Ok(())
}

fn validate_reason(value: Option<&str>) -> Result<(), Error> {
    if let Some(value) = value {
        if value.len() > MAX_REASON_LEN {
            return Err(Error::Invalid("reason"));
        }
    }
    Ok(())
}

fn required_reason(value: Option<&str>) -> Result<String, Error> {
    let Some(value) = value else {
        return Err(Error::Invalid("reason"));
    };
    let trimmed = value.trim();
    if trimmed.is_empty() || value.len() > MAX_REASON_LEN {
        return Err(Error::Invalid("reason"));
    }
    Ok(trimmed.to_owned())
}

fn finite(value: f64, field: &'static str) -> Result<(), Error> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(Error::Invalid(field))
    }
}

fn finite_non_negative(value: f64, field: &'static str) -> Result<(), Error> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(Error::Invalid(field))
    }
}

fn optional_finite(value: Option<f64>, field: &'static str) -> Result<(), Error> {
    match value {
        Some(value) => finite(value, field),
        None => Ok(()),
    }
}

fn optional_non_negative(value: Option<f64>, field: &'static str) -> Result<(), Error> {
    match value {
        Some(value) => finite_non_negative(value, field),
        None => Ok(()),
    }
}

fn validate_quantity(quantity_id: &str) -> Result<&'static QuantityDefinition, Error> {
    quantity_definition(quantity_id).ok_or(Error::Invalid("quantity"))
}

fn validate_measurement_value(quantity: &QuantityDefinition, value: f64) -> Result<(), Error> {
    finite(value, "measurement value")?;
    match quantity.basis {
        "volume" if value < 0.0 => Err(Error::Invalid("measurement value")),
        "specific_gravity" if value <= 0.0 => Err(Error::Invalid("measurement value")),
        _ => Ok(()),
    }
}

fn validate_metadata(metadata: &MeasurementMetadataV1) -> Result<(), Error> {
    optional_finite(metadata.sample_temperature_c, "sample temperature")?;
    optional_finite(metadata.reference_temperature_c, "reference temperature")?;
    for value in [&metadata.instrument, &metadata.method]
        .into_iter()
        .flatten()
    {
        if value.len() > MAX_SOURCE_LEN || value.chars().any(char::is_control) {
            return Err(Error::Invalid("measurement metadata"));
        }
    }
    let encoded =
        serde_json::to_string(&metadata.extensions).map_err(|_| Error::Invalid("metadata"))?;
    if encoded.len() > MAX_METADATA_JSON_LEN {
        return Err(Error::Invalid("metadata"));
    }
    Ok(())
}

fn validate_observed_at(value: Option<i64>) -> Result<(), Error> {
    if matches!(value, Some(value) if value <= 0) {
        return Err(Error::Invalid("observed_at"));
    }
    Ok(())
}

fn validate_limit(limit: i64) -> Result<i64, Error> {
    if (1..=MAX_LIMIT).contains(&limit) {
        Ok(limit)
    } else {
        Err(Error::Invalid("limit"))
    }
}

fn digest_of<T: Serialize>(value: &T) -> Result<String, Error> {
    let value =
        canonical_json::from_serializable(value).map_err(|_| Error::Invalid("request payload"))?;
    canonical_json::sha256(&value).map_err(|_| Error::Invalid("request payload"))
}

fn canonical_document<T: Serialize>(value: &T) -> Result<(String, String), Error> {
    let value = canonical_json::from_serializable(value).map_err(|_| Error::Invalid("document"))?;
    let digest = canonical_json::sha256(&value).map_err(|_| Error::Invalid("document"))?;
    let json =
        String::from_utf8(canonical_json::bytes(&value).map_err(|_| Error::Invalid("document"))?)
            .map_err(|_| Error::Invalid("document"))?;
    Ok((json, digest))
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceCursor {
    version: u8,
    workspace_id: String,
    brew_id: String,
    kind: String,
    filter: String,
    order: String,
    position: i64,
    id: String,
}

fn decode_cursor(
    value: &str,
    actor: &ActorContext,
    brew_id: &str,
    kind: &str,
    filter: &str,
    order: &str,
) -> Result<(i64, String), Error> {
    if value.len() > 2048 {
        return Err(Error::Invalid("cursor"));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| Error::Invalid("cursor"))?;
    let cursor: EvidenceCursor =
        serde_json::from_slice(&bytes).map_err(|_| Error::Invalid("cursor"))?;
    if cursor.version != 1
        || cursor.workspace_id != actor.workspace_id
        || cursor.brew_id != brew_id
        || cursor.kind != kind
        || cursor.filter != filter
        || cursor.order != order
        || cursor.position < 0
        || cursor.id.len() > 128
    {
        return Err(Error::Invalid("cursor"));
    }
    Ok((cursor.position, cursor.id))
}

fn encode_cursor(
    actor: &ActorContext,
    brew_id: &str,
    kind: &str,
    filter: &str,
    order: &str,
    position: i64,
    id: &str,
) -> Result<String, Error> {
    Ok(URL_SAFE_NO_PAD.encode(serde_json::to_vec(&EvidenceCursor {
        version: 1,
        workspace_id: actor.workspace_id.clone(),
        brew_id: brew_id.to_owned(),
        kind: kind.to_owned(),
        filter: filter.to_owned(),
        order: order.to_owned(),
        position,
        id: id.to_owned(),
    })?))
}

/// Acquire the shared write boundary and recheck authority after the workspace
/// lock, matching the brew-start transaction order.
async fn begin<'a>(
    pool: &'a DbPool,
    actor: &ActorContext,
    intent: &OperationIntent,
) -> Result<DbTx<'a>, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, intent)?;
    crate::brew_start::lock_workspace(&mut tx, &actor.workspace_id).await?;
    authorize_workspace(&mut tx, actor).await?;
    Ok(tx)
}

async fn lock_brew(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    brew_id: &str,
) -> Result<Option<store::BrewEvidenceState>, Error> {
    store::lock(tx, &actor.workspace_id, brew_id)
        .await
        .map_err(boundary_error)
}

fn target_for(plan: &BrewPlanV1, quantity_id: &str) -> Option<f64> {
    let targets = &plan.original_targets;
    match quantity_id {
        "gravity.original.v1" => targets.original_sg,
        "gravity.final.v1" => targets.final_sg,
        "volume.pre_boil.v1" => targets.pre_boil_l,
        "volume.post_boil.v1" => targets.post_boil_l,
        "volume.into_fermenter.v1" => plan.recipe.batch_size_l,
        _ => None,
    }
}

fn validate_fraction(
    value: Option<f64>,
    field: &'static str,
    allow_zero: bool,
) -> Result<(), Error> {
    match value {
        Some(value) => {
            finite(value, field)?;
            let valid = if allow_zero {
                (0.0..=1.0).contains(&value)
            } else {
                value > 0.0 && value <= 1.0
            };
            if valid {
                Ok(())
            } else {
                Err(Error::Invalid(field))
            }
        }
        None => Ok(()),
    }
}

fn unique_actual_row_ids(document: &ActualFormulationV1) -> Result<(), Error> {
    let mut seen = HashSet::new();
    let rows = document
        .fermentables
        .iter()
        .map(|row| ("fermentables", row.row_id.as_str()))
        .chain(
            document
                .hops
                .iter()
                .map(|row| ("hops", row.row_id.as_str())),
        )
        .chain(
            document
                .yeast
                .iter()
                .map(|row| ("yeast", row.row_id.as_str())),
        )
        .chain(
            document
                .other_ingredients
                .iter()
                .map(|row| ("other_ingredients", row.row_id.as_str())),
        )
        .chain(
            document
                .process
                .mash_steps
                .iter()
                .map(|row| ("process.mash_steps", row.row_id.as_str())),
        )
        .chain(
            document
                .process
                .fermentation_steps
                .iter()
                .map(|row| ("process.fermentation_steps", row.row_id.as_str())),
        );
    for (group, id) in rows {
        if id.is_empty() || !seen.insert((group, id)) {
            return Err(Error::Invalid("actual row identity"));
        }
    }
    Ok(())
}

fn validate_actual(document: &ActualFormulationV1) -> Result<(), Error> {
    if document.schema_version != 1 {
        return Err(Error::Invalid("actual formulation schema"));
    }
    optional_non_negative(document.boil_minutes, "boil duration")?;
    for row in &document.fermentables {
        validate_identifier(&row.row_id, "fermentable row")?;
        optional_non_negative(row.amount_kg, "fermentable amount")?;
        optional_non_negative(row.addition.time_minutes, "fermentable time")?;
        validate_fraction(row.specification.yield_fraction, "fermentable yield", false)?;
        optional_non_negative(row.specification.colour_ebc, "fermentable colour")?;
    }
    for row in &document.hops {
        validate_identifier(&row.row_id, "hop row")?;
        optional_non_negative(row.amount_g, "hop amount")?;
        optional_non_negative(row.contact_minutes, "hop contact time")?;
        optional_finite(row.temperature_c, "hop temperature")?;
        validate_fraction(row.specification.alpha_fraction, "hop alpha", true)?;
    }
    for row in document.yeast.iter().chain(&document.other_ingredients) {
        validate_identifier(&row.row_id, "ingredient row")?;
        optional_non_negative(row.amount, "ingredient amount")?;
        validate_fraction(
            row.specification.attenuation_fraction,
            "ingredient attenuation",
            true,
        )?;
    }
    validate_process(&document.process)?;
    unique_actual_row_ids(document)?;
    if let Some(notes) = &document.notes {
        if notes.len() > MAX_NOTE_LEN {
            return Err(Error::Invalid("actual notes"));
        }
    }
    let encoded = serde_json::to_string(&document.extensions)
        .map_err(|_| Error::Invalid("actual extensions"))?;
    if encoded.len() > MAX_METADATA_JSON_LEN {
        return Err(Error::Invalid("actual extensions"));
    }
    let (json, _) = canonical_document(document)?;
    if json.len() > MAX_ACTUAL_JSON_LEN {
        return Err(Error::Invalid("actual formulation"));
    }
    Ok(())
}

fn validate_process(process: &ProcessV1) -> Result<(), Error> {
    validate_fraction(
        process.apparent_attenuation_fraction,
        "process attenuation",
        true,
    )?;
    optional_non_negative(process.whirlpool_minutes, "whirlpool duration")?;
    optional_finite(process.whirlpool_temperature_c, "whirlpool temperature")?;
    optional_non_negative(process.mash_water_l, "mash water")?;
    optional_non_negative(process.kettle_top_up_l, "kettle top-up")?;
    for step in process.mash_steps.iter().chain(&process.fermentation_steps) {
        validate_identifier(&step.row_id, "process step")?;
        optional_non_negative(step.time_minutes, "process step time")?;
        optional_finite(step.temperature_c, "process step temperature")?;
        optional_non_negative(step.volume_l, "process step volume")?;
    }
    Ok(())
}

fn map_measurement_error(error: Error) -> MeasurementOutcome {
    match error {
        Error::Forbidden => MeasurementOutcome::Forbidden,
        Error::ContextChanged => MeasurementOutcome::ContextChanged,
        Error::BrewArchived => MeasurementOutcome::Archived,
        Error::MeasurementEntryConflict { expected, actual } => {
            MeasurementOutcome::EntryConflict { expected, actual }
        }
        Error::Conflict { expected, actual } => MeasurementOutcome::Conflict { expected, actual },
        Error::RequestIdentityConflict => MeasurementOutcome::RequestIdentityConflict,
        Error::ResultExpired { resource_id } => MeasurementOutcome::ResultExpired { resource_id },
        Error::Retryable => MeasurementOutcome::Retryable,
        Error::Invalid(_) => MeasurementOutcome::Invalid,
        other => {
            tracing::warn!(error = %other, "brew measurement operation unavailable");
            MeasurementOutcome::Unavailable
        }
    }
}

fn map_note_error(error: Error) -> NoteOutcome {
    match error {
        Error::Forbidden => NoteOutcome::Forbidden,
        Error::ContextChanged => NoteOutcome::ContextChanged,
        Error::BrewArchived => NoteOutcome::Archived,
        Error::Conflict { expected, actual } => NoteOutcome::Conflict { expected, actual },
        Error::RequestIdentityConflict => NoteOutcome::RequestIdentityConflict,
        Error::ResultExpired { resource_id } => NoteOutcome::ResultExpired { resource_id },
        Error::Retryable => NoteOutcome::Retryable,
        Error::Invalid(_) => NoteOutcome::Invalid,
        other => {
            tracing::warn!(error = %other, "brew note operation unavailable");
            NoteOutcome::Unavailable
        }
    }
}

fn map_actual_error(error: Error) -> ActualFormulationOutcome {
    match error {
        Error::Forbidden => ActualFormulationOutcome::Forbidden,
        Error::ContextChanged => ActualFormulationOutcome::ContextChanged,
        Error::BrewArchived => ActualFormulationOutcome::Archived,
        Error::Conflict { expected, actual } => ActualFormulationOutcome::Conflict {
            expected: Some(expected),
            actual: Some(actual),
        },
        Error::RequestIdentityConflict => ActualFormulationOutcome::RequestIdentityConflict,
        Error::ResultExpired { resource_id } => {
            ActualFormulationOutcome::ResultExpired { resource_id }
        }
        Error::Retryable => ActualFormulationOutcome::Retryable,
        Error::Invalid(_) => ActualFormulationOutcome::Invalid,
        other => {
            tracing::warn!(error = %other, "brew actual operation unavailable");
            ActualFormulationOutcome::Unavailable
        }
    }
}

fn map_lifecycle_error(error: Error) -> BrewLifecycleOutcome {
    match error {
        Error::Forbidden => BrewLifecycleOutcome::Forbidden,
        Error::ContextChanged => BrewLifecycleOutcome::ContextChanged,
        Error::BrewArchived => BrewLifecycleOutcome::Archived,
        Error::Conflict { expected, actual } => BrewLifecycleOutcome::Conflict { expected, actual },
        Error::RequestIdentityConflict => BrewLifecycleOutcome::RequestIdentityConflict,
        Error::ResultExpired { resource_id } => BrewLifecycleOutcome::ResultExpired { resource_id },
        Error::Retryable => BrewLifecycleOutcome::Retryable,
        Error::Invalid(_) => BrewLifecycleOutcome::Invalid,
        other => {
            tracing::warn!(error = %other, "brew lifecycle operation unavailable");
            BrewLifecycleOutcome::Unavailable
        }
    }
}

fn map_archive_error(error: Error) -> BrewArchiveOutcome {
    match error {
        Error::Forbidden => BrewArchiveOutcome::Forbidden,
        Error::ContextChanged => BrewArchiveOutcome::ContextChanged,
        Error::Conflict { expected, actual } => BrewArchiveOutcome::Conflict { expected, actual },
        Error::RequestIdentityConflict => BrewArchiveOutcome::RequestIdentityConflict,
        Error::ResultExpired { resource_id } => BrewArchiveOutcome::ResultExpired { resource_id },
        Error::Retryable => BrewArchiveOutcome::Retryable,
        Error::Invalid(_) => BrewArchiveOutcome::Invalid,
        other => {
            tracing::warn!(error = %other, "brew archive operation unavailable");
            BrewArchiveOutcome::Unavailable
        }
    }
}

fn map_view_error(error: Error) -> BrewEvidenceViewOutcome {
    match error {
        Error::Forbidden => BrewEvidenceViewOutcome::Forbidden,
        Error::ContextChanged => BrewEvidenceViewOutcome::ContextChanged,
        Error::Retryable => BrewEvidenceViewOutcome::Retryable,
        Error::Invalid(_) => BrewEvidenceViewOutcome::Invalid,
        other => {
            tracing::warn!(error = %other, "brew evidence view unavailable");
            BrewEvidenceViewOutcome::Unavailable
        }
    }
}

/// Dispatch one shared evidence command for transport adapters.
pub async fn execute(
    pool: &DbPool,
    actor: &ActorContext,
    command: BrewEvidenceCommand,
) -> BrewEvidenceOutcome {
    match command {
        BrewEvidenceCommand::View(request) => {
            BrewEvidenceOutcome::View(view(pool, actor, request).await)
        }
        BrewEvidenceCommand::Measurement(command) => {
            BrewEvidenceOutcome::Measurement(measurements(pool, actor, command).await)
        }
        BrewEvidenceCommand::Note(command) => {
            BrewEvidenceOutcome::Note(notes(pool, actor, command).await)
        }
        BrewEvidenceCommand::Actual(command) => {
            BrewEvidenceOutcome::Actual(actual(pool, actor, command).await)
        }
        BrewEvidenceCommand::Lifecycle(request) => {
            BrewEvidenceOutcome::Lifecycle(transition(pool, actor, request).await)
        }
        BrewEvidenceCommand::Archive(request) => {
            BrewEvidenceOutcome::Archive(archive(pool, actor, request).await)
        }
    }
}

pub async fn view(
    pool: &DbPool,
    actor: &ActorContext,
    request: BrewEvidenceViewRequest,
) -> BrewEvidenceViewOutcome {
    match view_inner(pool, actor, request).await {
        Ok(outcome) => outcome,
        Err(error) => map_view_error(error),
    }
}

async fn view_inner(
    pool: &DbPool,
    actor: &ActorContext,
    request: BrewEvidenceViewRequest,
) -> Result<BrewEvidenceViewOutcome, Error> {
    validate_identifier(&request.brew_id, "brew")?;
    let mut tx = begin(pool, actor, &request.intent).await?;
    let workspace_id = actor.workspace_id.clone();
    let Some(stored) = brew_store::load(&mut tx, &workspace_id, &request.brew_id)
        .await
        .map_err(boundary_error)?
    else {
        return Ok(BrewEvidenceViewOutcome::NotFound);
    };
    let Some(state) = store::load_state(&mut tx, &workspace_id, &request.brew_id)
        .await
        .map_err(boundary_error)?
    else {
        return Ok(BrewEvidenceViewOutcome::NotFound);
    };

    let mut measurements = Vec::with_capacity(QUANTITIES.len());
    for quantity in QUANTITIES {
        let selection =
            store::load_selection(&mut tx, &workspace_id, &request.brew_id, quantity.id)
                .await
                .map_err(boundary_error)?;
        let selected_root = selection
            .as_ref()
            .and_then(|selection| selection.selected_root_observation_id.clone());
        let current = match &selected_root {
            Some(root) => store::load_current_entry(&mut tx, &workspace_id, &request.brew_id, root)
                .await
                .map_err(boundary_error)?,
            None => None,
        };
        measurements.push(MeasurementSlot {
            quantity_id: quantity.id.to_owned(),
            label: quantity.label.to_owned(),
            unit: quantity.unit.to_owned(),
            stage: quantity.stage.to_owned(),
            basis: quantity.basis.to_owned(),
            target: target_for(&stored.plan, quantity.id),
            selection_revision: selection
                .as_ref()
                .map(|selection| selection.revision)
                .unwrap_or(0),
            selected_root_observation_id: selected_root,
            current,
        });
    }

    let note_rows = store::list_current_notes(
        &mut tx,
        &workspace_id,
        &request.brew_id,
        None,
        VIEW_NOTE_LIMIT + 1,
    )
    .await
    .map_err(boundary_error)?;
    let notes_truncated = note_rows.len() as i64 > VIEW_NOTE_LIMIT;
    let notes = note_rows
        .into_iter()
        .take(VIEW_NOTE_LIMIT as usize)
        .map(|(note, current)| NoteView { note, current })
        .collect();

    let lifecycle_after = match request.lifecycle_cursor.as_deref() {
        Some(cursor) => Some(
            decode_cursor(
                cursor,
                actor,
                &request.brew_id,
                "lifecycle",
                "all",
                "brew_revision",
            )?
            .0,
        ),
        None => None,
    };
    let mut lifecycle = store::list_lifecycle_events(
        &mut tx,
        &workspace_id,
        &request.brew_id,
        lifecycle_after,
        VIEW_LIFECYCLE_LIMIT + 1,
    )
    .await
    .map_err(boundary_error)?;
    let lifecycle_next_cursor = if lifecycle.len() as i64 > VIEW_LIFECYCLE_LIMIT {
        lifecycle.truncate(VIEW_LIFECYCLE_LIMIT as usize);
        lifecycle
            .last()
            .map(|event| {
                encode_cursor(
                    actor,
                    &request.brew_id,
                    "lifecycle",
                    "all",
                    "brew_revision",
                    event.brew_revision,
                    "",
                )
            })
            .transpose()?
    } else {
        None
    };
    let actual = store::load_current_actual(&mut tx, &workspace_id, &request.brew_id)
        .await
        .map_err(boundary_error)?;
    tx.commit().await.map_err(boundary_error)?;

    Ok(BrewEvidenceViewOutcome::View(BrewEvidenceView {
        brew_id: stored.brew_id,
        recipe_id: stored.recipe_id,
        selected_recipe_revision: stored.selected_recipe_revision,
        status: stored.status,
        revision: stored.revision,
        started_at: stored.started_at,
        started_by: stored.started_by,
        archived: state.archived,
        metadata_revision: state.metadata_revision,
        plan: stored.plan,
        actual,
        measurements,
        notes,
        notes_truncated,
        lifecycle,
        lifecycle_next_cursor,
    }))
}

pub async fn measurements(
    pool: &DbPool,
    actor: &ActorContext,
    command: MeasurementCommand,
) -> MeasurementOutcome {
    let result = match command {
        MeasurementCommand::Record(request) => record_measurement(pool, actor, request).await,
        MeasurementCommand::Correct(request) => correct_measurement(pool, actor, request).await,
        MeasurementCommand::Retract(request) => retract_measurement(pool, actor, request).await,
        MeasurementCommand::Select(request) => select_measurement(pool, actor, request).await,
        MeasurementCommand::List(request) => list_measurements(pool, actor, request).await,
    };
    match result {
        Ok(outcome) => outcome,
        Err(error) => map_measurement_error(error),
    }
}

async fn record_measurement(
    pool: &DbPool,
    actor: &ActorContext,
    request: RecordMeasurementRequest,
) -> Result<MeasurementOutcome, Error> {
    validate_identifier(&request.brew_id, "brew")?;
    let quantity = validate_quantity(&request.quantity_id)?;
    validate_measurement_value(quantity, request.value)?;
    validate_observed_at(request.observed_at)?;
    validate_source(&request.source)?;
    validate_metadata(&request.metadata)?;
    if !valid_request_id(&request.request_id) {
        return Err(Error::Invalid("request_id"));
    }
    let digest = digest_of(&(
        &request.intent,
        &request.brew_id,
        &request.quantity_id,
        request.value,
        request.observed_at,
        &request.source,
        &request.metadata,
    ))?;

    let mut tx = begin(pool, actor, &request.intent).await?;
    let key = RequestIdentity {
        actor,
        operation: "brew.measurement.record.v1",
        request_id: &request.request_id,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        return Ok(serde_json::from_value(result)?);
    }
    let Some(state) = lock_brew(&mut tx, actor, &request.brew_id).await? else {
        return Ok(MeasurementOutcome::NotFound);
    };
    if state.archived {
        return Ok(MeasurementOutcome::Archived);
    }

    let observed_at = request.observed_at.unwrap_or_else(now);
    let time_source = if request.observed_at.is_some() {
        "supplied"
    } else {
        "recorded_default"
    };
    let recorded_at = now();
    let entry_id = Uuid::new_v4().to_string();
    let entry = MeasurementEntry {
        workspace_id: actor.workspace_id.clone(),
        brew_id: request.brew_id.clone(),
        entry_id: entry_id.clone(),
        quantity_id: quantity.id.to_owned(),
        unit: quantity.unit.to_owned(),
        basis: quantity.basis.to_owned(),
        stage: quantity.stage.to_owned(),
        value: Some(request.value),
        observed_at,
        time_source: time_source.to_owned(),
        recorded_at,
        actor_id: actor.actor_id.clone(),
        source: request.source,
        root_observation_id: entry_id,
        supersedes_entry_id: None,
        schema_version: 1,
        metadata: request.metadata,
    };
    store::insert_measurement(&mut tx, &entry)
        .await
        .map_err(boundary_error)?;

    // Entering the first actual for a quantity selects its root atomically.
    // Later readings never replace the primary implicitly.
    if store::load_selection(&mut tx, &actor.workspace_id, &request.brew_id, quantity.id)
        .await
        .map_err(boundary_error)?
        .is_none()
    {
        let selection = ActualSelection {
            workspace_id: actor.workspace_id.clone(),
            brew_id: request.brew_id.clone(),
            quantity_id: quantity.id.to_owned(),
            revision: 1,
            selected_root_observation_id: Some(entry.entry_id.clone()),
            updated_at: recorded_at,
            updated_by: actor.actor_id.clone(),
        };
        store::insert_selection(&mut tx, &selection)
            .await
            .map_err(boundary_error)?;
        store::insert_selection_event(
            &mut tx,
            &ActualSelectionEvent {
                workspace_id: actor.workspace_id.clone(),
                brew_id: request.brew_id.clone(),
                quantity_id: quantity.id.to_owned(),
                revision: 1,
                selected_root_observation_id: Some(entry.entry_id.clone()),
                actor_id: actor.actor_id.clone(),
                recorded_at,
            },
        )
        .await
        .map_err(boundary_error)?;
    }

    let outcome = MeasurementOutcome::Recorded(entry);
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&outcome)?,
        Some(&request.brew_id),
    )
    .await?;
    record_change(
        &mut tx,
        actor,
        "brew.measurement.record.v1",
        &request.brew_id,
        state.revision,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(outcome)
}

async fn correct_measurement(
    pool: &DbPool,
    actor: &ActorContext,
    request: CorrectMeasurementRequest,
) -> Result<MeasurementOutcome, Error> {
    validate_identifier(&request.brew_id, "brew")?;
    validate_identifier(&request.entry_id, "measurement entry")?;
    finite(request.value, "measurement value")?;
    validate_source(&request.source)?;
    if let Some(metadata) = &request.metadata {
        validate_metadata(metadata)?;
    }
    if matches!(request.expected_selection_revision, Some(value) if value <= 0) {
        return Err(Error::Invalid("selection revision"));
    }
    if !valid_request_id(&request.request_id) {
        return Err(Error::Invalid("request_id"));
    }
    let digest = digest_of(&(
        &request.intent,
        &request.brew_id,
        &request.entry_id,
        request.expected_selection_revision,
        request.value,
        &request.source,
        &request.metadata,
    ))?;

    let mut tx = begin(pool, actor, &request.intent).await?;
    let key = RequestIdentity {
        actor,
        operation: "brew.measurement.correct.v1",
        request_id: &request.request_id,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        return Ok(serde_json::from_value(result)?);
    }
    let Some(state) = lock_brew(&mut tx, actor, &request.brew_id).await? else {
        return Ok(MeasurementOutcome::NotFound);
    };
    if state.archived {
        return Ok(MeasurementOutcome::Archived);
    }
    let Some(target) = store::load_measurement(
        &mut tx,
        &actor.workspace_id,
        &request.brew_id,
        &request.entry_id,
    )
    .await
    .map_err(boundary_error)?
    else {
        return Ok(MeasurementOutcome::NotFound);
    };
    let Some(current) = store::load_current_entry(
        &mut tx,
        &actor.workspace_id,
        &request.brew_id,
        &target.root_observation_id,
    )
    .await
    .map_err(boundary_error)?
    else {
        return Ok(MeasurementOutcome::NotFound);
    };
    if current.entry_id != target.entry_id {
        return Ok(MeasurementOutcome::EntryConflict {
            expected: target.entry_id,
            actual: current.entry_id,
        });
    }
    let Some(quantity) = quantity_definition(&target.quantity_id) else {
        return Err(Error::Invalid("quantity"));
    };
    validate_measurement_value(quantity, request.value)?;
    // A retraction is terminal for its root; re-adding a reading is a new root.
    if target.value.is_none() {
        return Ok(MeasurementOutcome::Invalid);
    }

    let selection = store::load_selection(
        &mut tx,
        &actor.workspace_id,
        &request.brew_id,
        &target.quantity_id,
    )
    .await
    .map_err(boundary_error)?;
    let selected = selection
        .as_ref()
        .and_then(|selection| selection.selected_root_observation_id.as_deref())
        == Some(target.root_observation_id.as_str());
    if selected {
        let Some(expected) = request.expected_selection_revision else {
            return Ok(MeasurementOutcome::Invalid);
        };
        let actual = selection
            .as_ref()
            .map(|selection| selection.revision)
            .unwrap_or(0);
        if expected != actual {
            return Ok(MeasurementOutcome::Conflict { expected, actual });
        }
    } else if let Some(expected) = request.expected_selection_revision {
        let actual = selection
            .as_ref()
            .map(|selection| selection.revision)
            .unwrap_or(0);
        if expected != actual {
            return Ok(MeasurementOutcome::Conflict { expected, actual });
        }
    }

    let recorded_at = now();
    let entry = MeasurementEntry {
        workspace_id: actor.workspace_id.clone(),
        brew_id: request.brew_id.clone(),
        entry_id: Uuid::new_v4().to_string(),
        quantity_id: target.quantity_id.clone(),
        unit: target.unit.clone(),
        basis: target.basis.clone(),
        stage: target.stage.clone(),
        value: Some(request.value),
        observed_at: target.observed_at,
        time_source: target.time_source.clone(),
        recorded_at,
        actor_id: actor.actor_id.clone(),
        source: request.source,
        root_observation_id: target.root_observation_id.clone(),
        supersedes_entry_id: Some(target.entry_id.clone()),
        schema_version: 1,
        metadata: request.metadata.unwrap_or_else(|| target.metadata.clone()),
    };
    store::insert_measurement(&mut tx, &entry)
        .await
        .map_err(boundary_error)?;

    if selected {
        let selection = selection.expect("selected implies a selection row");
        let next = ActualSelection {
            revision: selection.revision + 1,
            selected_root_observation_id: Some(target.root_observation_id.clone()),
            updated_at: recorded_at,
            updated_by: actor.actor_id.clone(),
            ..selection.clone()
        };
        if !store::update_selection(
            &mut tx,
            &actor.workspace_id,
            &request.brew_id,
            &target.quantity_id,
            selection.revision,
            &next,
        )
        .await
        .map_err(boundary_error)?
        {
            return Err(Error::Retryable);
        }
        store::insert_selection_event(
            &mut tx,
            &ActualSelectionEvent {
                workspace_id: actor.workspace_id.clone(),
                brew_id: request.brew_id.clone(),
                quantity_id: target.quantity_id.clone(),
                revision: next.revision,
                selected_root_observation_id: Some(target.root_observation_id.clone()),
                actor_id: actor.actor_id.clone(),
                recorded_at,
            },
        )
        .await
        .map_err(boundary_error)?;
    }

    let outcome = MeasurementOutcome::Corrected(entry);
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&outcome)?,
        Some(&request.brew_id),
    )
    .await?;
    record_change(
        &mut tx,
        actor,
        "brew.measurement.correct.v1",
        &request.brew_id,
        state.revision,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(outcome)
}

async fn retract_measurement(
    pool: &DbPool,
    actor: &ActorContext,
    request: RetractMeasurementRequest,
) -> Result<MeasurementOutcome, Error> {
    validate_identifier(&request.brew_id, "brew")?;
    validate_identifier(&request.entry_id, "measurement entry")?;
    if matches!(request.expected_selection_revision, Some(value) if value <= 0) {
        return Err(Error::Invalid("selection revision"));
    }
    if !valid_request_id(&request.request_id) {
        return Err(Error::Invalid("request_id"));
    }
    let digest = digest_of(&(
        &request.intent,
        &request.brew_id,
        &request.entry_id,
        request.expected_selection_revision,
    ))?;

    let mut tx = begin(pool, actor, &request.intent).await?;
    let key = RequestIdentity {
        actor,
        operation: "brew.measurement.retract.v1",
        request_id: &request.request_id,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        return Ok(serde_json::from_value(result)?);
    }
    let Some(state) = lock_brew(&mut tx, actor, &request.brew_id).await? else {
        return Ok(MeasurementOutcome::NotFound);
    };
    if state.archived {
        return Ok(MeasurementOutcome::Archived);
    }
    let Some(target) = store::load_measurement(
        &mut tx,
        &actor.workspace_id,
        &request.brew_id,
        &request.entry_id,
    )
    .await
    .map_err(boundary_error)?
    else {
        return Ok(MeasurementOutcome::NotFound);
    };
    let Some(current) = store::load_current_entry(
        &mut tx,
        &actor.workspace_id,
        &request.brew_id,
        &target.root_observation_id,
    )
    .await
    .map_err(boundary_error)?
    else {
        return Ok(MeasurementOutcome::NotFound);
    };
    if current.entry_id != target.entry_id {
        return Ok(MeasurementOutcome::EntryConflict {
            expected: target.entry_id,
            actual: current.entry_id,
        });
    }
    if target.value.is_none() {
        return Ok(MeasurementOutcome::Invalid);
    }

    let selection = store::load_selection(
        &mut tx,
        &actor.workspace_id,
        &request.brew_id,
        &target.quantity_id,
    )
    .await
    .map_err(boundary_error)?;
    let selected = selection
        .as_ref()
        .and_then(|selection| selection.selected_root_observation_id.as_deref())
        == Some(target.root_observation_id.as_str());
    if selected {
        let Some(expected) = request.expected_selection_revision else {
            return Ok(MeasurementOutcome::Invalid);
        };
        let actual = selection
            .as_ref()
            .map(|selection| selection.revision)
            .unwrap_or(0);
        if expected != actual {
            return Ok(MeasurementOutcome::Conflict { expected, actual });
        }
    } else if let Some(expected) = request.expected_selection_revision {
        let actual = selection
            .as_ref()
            .map(|selection| selection.revision)
            .unwrap_or(0);
        if expected != actual {
            return Ok(MeasurementOutcome::Conflict { expected, actual });
        }
    }

    let recorded_at = now();
    let entry = MeasurementEntry {
        workspace_id: actor.workspace_id.clone(),
        brew_id: request.brew_id.clone(),
        entry_id: Uuid::new_v4().to_string(),
        quantity_id: target.quantity_id.clone(),
        unit: target.unit.clone(),
        basis: target.basis.clone(),
        stage: target.stage.clone(),
        value: None,
        observed_at: target.observed_at,
        time_source: target.time_source.clone(),
        recorded_at,
        actor_id: actor.actor_id.clone(),
        source: target.source.clone(),
        root_observation_id: target.root_observation_id.clone(),
        supersedes_entry_id: Some(target.entry_id.clone()),
        schema_version: 1,
        metadata: target.metadata.clone(),
    };
    store::insert_measurement(&mut tx, &entry)
        .await
        .map_err(boundary_error)?;

    if selected {
        let selection = selection.expect("selected implies a selection row");
        let next = ActualSelection {
            revision: selection.revision + 1,
            selected_root_observation_id: None,
            updated_at: recorded_at,
            updated_by: actor.actor_id.clone(),
            ..selection.clone()
        };
        if !store::update_selection(
            &mut tx,
            &actor.workspace_id,
            &request.brew_id,
            &target.quantity_id,
            selection.revision,
            &next,
        )
        .await
        .map_err(boundary_error)?
        {
            return Err(Error::Retryable);
        }
        store::insert_selection_event(
            &mut tx,
            &ActualSelectionEvent {
                workspace_id: actor.workspace_id.clone(),
                brew_id: request.brew_id.clone(),
                quantity_id: target.quantity_id.clone(),
                revision: next.revision,
                selected_root_observation_id: None,
                actor_id: actor.actor_id.clone(),
                recorded_at,
            },
        )
        .await
        .map_err(boundary_error)?;
    }

    let outcome = MeasurementOutcome::Retracted(entry);
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&outcome)?,
        Some(&request.brew_id),
    )
    .await?;
    record_change(
        &mut tx,
        actor,
        "brew.measurement.retract.v1",
        &request.brew_id,
        state.revision,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(outcome)
}

async fn select_measurement(
    pool: &DbPool,
    actor: &ActorContext,
    request: SelectMeasurementRequest,
) -> Result<MeasurementOutcome, Error> {
    validate_identifier(&request.brew_id, "brew")?;
    let quantity = validate_quantity(&request.quantity_id)?;
    if request.expected_selection_revision < 0 {
        return Err(Error::Invalid("selection revision"));
    }
    if let Some(root) = &request.root_observation_id {
        validate_identifier(root, "observation root")?;
    }
    if !valid_request_id(&request.request_id) {
        return Err(Error::Invalid("request_id"));
    }
    let digest = digest_of(&(
        &request.intent,
        &request.brew_id,
        &request.quantity_id,
        &request.root_observation_id,
        request.expected_selection_revision,
    ))?;

    let mut tx = begin(pool, actor, &request.intent).await?;
    let key = RequestIdentity {
        actor,
        operation: "brew.measurement.select.v1",
        request_id: &request.request_id,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        return Ok(serde_json::from_value(result)?);
    }
    let Some(state) = lock_brew(&mut tx, actor, &request.brew_id).await? else {
        return Ok(MeasurementOutcome::NotFound);
    };
    if state.archived {
        return Ok(MeasurementOutcome::Archived);
    }

    if let Some(root) = &request.root_observation_id {
        let Some(entry) =
            store::load_measurement(&mut tx, &actor.workspace_id, &request.brew_id, root)
                .await
                .map_err(boundary_error)?
        else {
            return Ok(MeasurementOutcome::NotFound);
        };
        if entry.root_observation_id != entry.entry_id || entry.quantity_id != request.quantity_id {
            return Ok(MeasurementOutcome::Invalid);
        }
        let Some(current) =
            store::load_current_entry(&mut tx, &actor.workspace_id, &request.brew_id, root)
                .await
                .map_err(boundary_error)?
        else {
            return Ok(MeasurementOutcome::NotFound);
        };
        if current.value.is_none() {
            return Ok(MeasurementOutcome::Invalid);
        }
    }

    let existing =
        store::load_selection(&mut tx, &actor.workspace_id, &request.brew_id, quantity.id)
            .await
            .map_err(boundary_error)?;
    let recorded_at = now();
    let outcome = match existing {
        Some(selection) => {
            if selection.revision != request.expected_selection_revision {
                return Ok(MeasurementOutcome::Conflict {
                    expected: request.expected_selection_revision,
                    actual: selection.revision,
                });
            }
            if selection.selected_root_observation_id == request.root_observation_id {
                MeasurementOutcome::Selected(selection)
            } else {
                let next = ActualSelection {
                    revision: selection.revision + 1,
                    selected_root_observation_id: request.root_observation_id.clone(),
                    updated_at: recorded_at,
                    updated_by: actor.actor_id.clone(),
                    ..selection.clone()
                };
                if !store::update_selection(
                    &mut tx,
                    &actor.workspace_id,
                    &request.brew_id,
                    quantity.id,
                    selection.revision,
                    &next,
                )
                .await
                .map_err(boundary_error)?
                {
                    return Err(Error::Retryable);
                }
                store::insert_selection_event(
                    &mut tx,
                    &ActualSelectionEvent {
                        workspace_id: actor.workspace_id.clone(),
                        brew_id: request.brew_id.clone(),
                        quantity_id: quantity.id.to_owned(),
                        revision: next.revision,
                        selected_root_observation_id: request.root_observation_id.clone(),
                        actor_id: actor.actor_id.clone(),
                        recorded_at,
                    },
                )
                .await
                .map_err(boundary_error)?;
                MeasurementOutcome::Selected(next)
            }
        }
        None => {
            if request.expected_selection_revision != 0 {
                return Ok(MeasurementOutcome::Conflict {
                    expected: request.expected_selection_revision,
                    actual: 0,
                });
            }
            let selection = ActualSelection {
                workspace_id: actor.workspace_id.clone(),
                brew_id: request.brew_id.clone(),
                quantity_id: quantity.id.to_owned(),
                revision: 1,
                selected_root_observation_id: request.root_observation_id.clone(),
                updated_at: recorded_at,
                updated_by: actor.actor_id.clone(),
            };
            store::insert_selection(&mut tx, &selection)
                .await
                .map_err(boundary_error)?;
            store::insert_selection_event(
                &mut tx,
                &ActualSelectionEvent {
                    workspace_id: actor.workspace_id.clone(),
                    brew_id: request.brew_id.clone(),
                    quantity_id: quantity.id.to_owned(),
                    revision: 1,
                    selected_root_observation_id: request.root_observation_id.clone(),
                    actor_id: actor.actor_id.clone(),
                    recorded_at,
                },
            )
            .await
            .map_err(boundary_error)?;
            MeasurementOutcome::Selected(selection)
        }
    };

    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&outcome)?,
        Some(&request.brew_id),
    )
    .await?;
    record_change(
        &mut tx,
        actor,
        "brew.measurement.select.v1",
        &request.brew_id,
        state.revision,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(outcome)
}

async fn list_measurements(
    pool: &DbPool,
    actor: &ActorContext,
    request: MeasurementListRequest,
) -> Result<MeasurementOutcome, Error> {
    validate_identifier(&request.brew_id, "brew")?;
    let limit = validate_limit(request.limit)?;
    if let Some(root) = &request.root_observation_id {
        validate_identifier(root, "observation root")?;
    }
    let (kind, filter, order) = match &request.root_observation_id {
        Some(root) => ("measurement.lineage", root.as_str(), "recorded_at,entry_id"),
        None => ("measurement.current", "current", "recorded_at,entry_id"),
    };
    let after = match request.cursor.as_deref() {
        Some(cursor) => Some(decode_cursor(
            cursor,
            actor,
            &request.brew_id,
            kind,
            filter,
            order,
        )?),
        None => None,
    };

    let mut tx = begin(pool, actor, &request.intent).await?;
    let workspace_id = actor.workspace_id.clone();
    if store::load_state(&mut tx, &workspace_id, &request.brew_id)
        .await
        .map_err(boundary_error)?
        .is_none()
    {
        return Ok(MeasurementOutcome::NotFound);
    }
    let after_ref = after
        .as_ref()
        .map(|(timestamp, id)| (timestamp, id.as_str()));
    let rows = match &request.root_observation_id {
        Some(root) => {
            let Some(entry) =
                store::load_measurement(&mut tx, &workspace_id, &request.brew_id, root)
                    .await
                    .map_err(boundary_error)?
            else {
                return Ok(MeasurementOutcome::NotFound);
            };
            if entry.root_observation_id != entry.entry_id {
                return Ok(MeasurementOutcome::NotFound);
            }
            store::list_root_lineage(
                &mut tx,
                &workspace_id,
                &request.brew_id,
                root,
                after_ref,
                limit + 1,
            )
            .await
            .map_err(boundary_error)?
        }
        None => store::list_current_entries(
            &mut tx,
            &workspace_id,
            &request.brew_id,
            after_ref,
            limit + 1,
        )
        .await
        .map_err(boundary_error)?,
    };
    tx.commit().await.map_err(boundary_error)?;

    let mut items = rows;
    let next_cursor = if items.len() as i64 > limit {
        items.truncate(limit as usize);
        items
            .last()
            .map(|entry| {
                encode_cursor(
                    actor,
                    &request.brew_id,
                    kind,
                    filter,
                    order,
                    entry.recorded_at,
                    &entry.entry_id,
                )
            })
            .transpose()?
    } else {
        None
    };
    Ok(MeasurementOutcome::Page(MeasurementPage {
        items,
        next_cursor,
    }))
}

pub async fn notes(pool: &DbPool, actor: &ActorContext, command: NoteCommand) -> NoteOutcome {
    let result = match command {
        NoteCommand::Record(request) => record_note(pool, actor, request).await,
        NoteCommand::Correct(request) => correct_note(pool, actor, request).await,
        NoteCommand::Retract(request) => retract_note(pool, actor, request).await,
        NoteCommand::List(request) => list_notes(pool, actor, request).await,
    };
    match result {
        Ok(outcome) => outcome,
        Err(error) => map_note_error(error),
    }
}

fn note_document(text: Option<String>) -> ObservationNoteV1 {
    ObservationNoteV1 {
        schema_version: 1,
        text,
        extensions: Default::default(),
    }
}

async fn record_note(
    pool: &DbPool,
    actor: &ActorContext,
    request: RecordNoteRequest,
) -> Result<NoteOutcome, Error> {
    validate_identifier(&request.brew_id, "brew")?;
    validate_text(&request.text, "note text")?;
    if !valid_request_id(&request.request_id) {
        return Err(Error::Invalid("request_id"));
    }
    let digest = digest_of(&(&request.intent, &request.brew_id, &request.text))?;

    let mut tx = begin(pool, actor, &request.intent).await?;
    let key = RequestIdentity {
        actor,
        operation: "brew.note.record.v1",
        request_id: &request.request_id,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        return Ok(serde_json::from_value(result)?);
    }
    let Some(state) = lock_brew(&mut tx, actor, &request.brew_id).await? else {
        return Ok(NoteOutcome::NotFound);
    };
    if state.archived {
        return Ok(NoteOutcome::Archived);
    }

    let recorded_at = now();
    let note_id = Uuid::new_v4().to_string();
    let document = note_document(Some(request.text));
    let (_, content_digest) = canonical_document(&document)?;
    store::insert_note(
        &mut tx,
        &ObservationNote {
            workspace_id: actor.workspace_id.clone(),
            brew_id: request.brew_id.clone(),
            note_id: note_id.clone(),
            current_revision: 1,
            created_at: recorded_at,
            created_by: actor.actor_id.clone(),
        },
    )
    .await
    .map_err(boundary_error)?;
    let revision = ObservationRevision {
        workspace_id: actor.workspace_id.clone(),
        brew_id: request.brew_id.clone(),
        note_id,
        revision: 1,
        document,
        content_digest,
        supersedes_revision: None,
        actor_id: actor.actor_id.clone(),
        recorded_at,
        reason: None,
    };
    store::insert_note_revision(&mut tx, &revision)
        .await
        .map_err(boundary_error)?;

    let outcome = NoteOutcome::Recorded(revision);
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&outcome)?,
        Some(&request.brew_id),
    )
    .await?;
    record_change(
        &mut tx,
        actor,
        "brew.note.record.v1",
        &request.brew_id,
        state.revision,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(outcome)
}

async fn correct_note(
    pool: &DbPool,
    actor: &ActorContext,
    request: CorrectNoteRequest,
) -> Result<NoteOutcome, Error> {
    validate_identifier(&request.brew_id, "brew")?;
    validate_identifier(&request.note_id, "note")?;
    validate_text(&request.text, "note text")?;
    validate_reason(request.reason.as_deref())?;
    if request.expected_revision <= 0 {
        return Err(Error::Invalid("note revision"));
    }
    if !valid_request_id(&request.request_id) {
        return Err(Error::Invalid("request_id"));
    }
    let digest = digest_of(&(
        &request.intent,
        &request.brew_id,
        &request.note_id,
        request.expected_revision,
        &request.text,
        &request.reason,
    ))?;

    let mut tx = begin(pool, actor, &request.intent).await?;
    let key = RequestIdentity {
        actor,
        operation: "brew.note.correct.v1",
        request_id: &request.request_id,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        return Ok(serde_json::from_value(result)?);
    }
    let Some(state) = lock_brew(&mut tx, actor, &request.brew_id).await? else {
        return Ok(NoteOutcome::NotFound);
    };
    if state.archived {
        return Ok(NoteOutcome::Archived);
    }
    let Some(note) = store::load_note(
        &mut tx,
        &actor.workspace_id,
        &request.brew_id,
        &request.note_id,
    )
    .await
    .map_err(boundary_error)?
    else {
        return Ok(NoteOutcome::NotFound);
    };
    if note.current_revision != request.expected_revision {
        return Ok(NoteOutcome::Conflict {
            expected: request.expected_revision,
            actual: note.current_revision,
        });
    }

    let recorded_at = now();
    let document = note_document(Some(request.text));
    let (_, content_digest) = canonical_document(&document)?;
    let revision = ObservationRevision {
        workspace_id: actor.workspace_id.clone(),
        brew_id: request.brew_id.clone(),
        note_id: request.note_id.clone(),
        revision: note.current_revision + 1,
        document,
        content_digest,
        supersedes_revision: Some(note.current_revision),
        actor_id: actor.actor_id.clone(),
        recorded_at,
        reason: request.reason,
    };
    store::insert_note_revision(&mut tx, &revision)
        .await
        .map_err(boundary_error)?;
    if !store::update_note_revision(
        &mut tx,
        &actor.workspace_id,
        &request.brew_id,
        &request.note_id,
        note.current_revision,
        revision.revision,
    )
    .await
    .map_err(boundary_error)?
    {
        return Err(Error::Retryable);
    }

    let outcome = NoteOutcome::Corrected(revision);
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&outcome)?,
        Some(&request.brew_id),
    )
    .await?;
    record_change(
        &mut tx,
        actor,
        "brew.note.correct.v1",
        &request.brew_id,
        state.revision,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(outcome)
}

async fn retract_note(
    pool: &DbPool,
    actor: &ActorContext,
    request: RetractNoteRequest,
) -> Result<NoteOutcome, Error> {
    validate_identifier(&request.brew_id, "brew")?;
    validate_identifier(&request.note_id, "note")?;
    validate_reason(request.reason.as_deref())?;
    if request.expected_revision <= 0 {
        return Err(Error::Invalid("note revision"));
    }
    if !valid_request_id(&request.request_id) {
        return Err(Error::Invalid("request_id"));
    }
    let digest = digest_of(&(
        &request.intent,
        &request.brew_id,
        &request.note_id,
        request.expected_revision,
        &request.reason,
    ))?;

    let mut tx = begin(pool, actor, &request.intent).await?;
    let key = RequestIdentity {
        actor,
        operation: "brew.note.retract.v1",
        request_id: &request.request_id,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        return Ok(serde_json::from_value(result)?);
    }
    let Some(state) = lock_brew(&mut tx, actor, &request.brew_id).await? else {
        return Ok(NoteOutcome::NotFound);
    };
    if state.archived {
        return Ok(NoteOutcome::Archived);
    }
    let Some(note) = store::load_note(
        &mut tx,
        &actor.workspace_id,
        &request.brew_id,
        &request.note_id,
    )
    .await
    .map_err(boundary_error)?
    else {
        return Ok(NoteOutcome::NotFound);
    };
    if note.current_revision != request.expected_revision {
        return Ok(NoteOutcome::Conflict {
            expected: request.expected_revision,
            actual: note.current_revision,
        });
    }

    let recorded_at = now();
    let document = note_document(None);
    let (_, content_digest) = canonical_document(&document)?;
    let revision = ObservationRevision {
        workspace_id: actor.workspace_id.clone(),
        brew_id: request.brew_id.clone(),
        note_id: request.note_id.clone(),
        revision: note.current_revision + 1,
        document,
        content_digest,
        supersedes_revision: Some(note.current_revision),
        actor_id: actor.actor_id.clone(),
        recorded_at,
        reason: request.reason,
    };
    store::insert_note_revision(&mut tx, &revision)
        .await
        .map_err(boundary_error)?;
    if !store::update_note_revision(
        &mut tx,
        &actor.workspace_id,
        &request.brew_id,
        &request.note_id,
        note.current_revision,
        revision.revision,
    )
    .await
    .map_err(boundary_error)?
    {
        return Err(Error::Retryable);
    }

    let outcome = NoteOutcome::Retracted(revision);
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&outcome)?,
        Some(&request.brew_id),
    )
    .await?;
    record_change(
        &mut tx,
        actor,
        "brew.note.retract.v1",
        &request.brew_id,
        state.revision,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(outcome)
}

async fn list_notes(
    pool: &DbPool,
    actor: &ActorContext,
    request: NoteListRequest,
) -> Result<NoteOutcome, Error> {
    validate_identifier(&request.brew_id, "brew")?;
    let limit = validate_limit(request.limit)?;
    if let Some(note_id) = &request.note_id {
        validate_identifier(note_id, "note")?;
    }
    let (kind, filter, order) = match &request.note_id {
        Some(note_id) => ("note.history", note_id.as_str(), "revision"),
        None => ("note.current", "current", "created_at,note_id"),
    };
    let decoded = match request.cursor.as_deref() {
        Some(cursor) => Some(decode_cursor(
            cursor,
            actor,
            &request.brew_id,
            kind,
            filter,
            order,
        )?),
        None => None,
    };

    let mut tx = begin(pool, actor, &request.intent).await?;
    let workspace_id = actor.workspace_id.clone();
    if store::load_state(&mut tx, &workspace_id, &request.brew_id)
        .await
        .map_err(boundary_error)?
        .is_none()
    {
        return Ok(NoteOutcome::NotFound);
    }
    let outcome = match &request.note_id {
        Some(note_id) => {
            if store::load_note(&mut tx, &workspace_id, &request.brew_id, note_id)
                .await
                .map_err(boundary_error)?
                .is_none()
            {
                return Ok(NoteOutcome::NotFound);
            }
            let after = match &decoded {
                Some((position, id)) => {
                    if *position <= 0 || !id.is_empty() {
                        return Err(Error::Invalid("cursor"));
                    }
                    Some(*position)
                }
                None => None,
            };
            let mut items = store::list_note_revisions(
                &mut tx,
                &workspace_id,
                &request.brew_id,
                note_id,
                after,
                limit + 1,
            )
            .await
            .map_err(boundary_error)?;
            let next_cursor = if items.len() as i64 > limit {
                items.truncate(limit as usize);
                items
                    .last()
                    .map(|revision| {
                        encode_cursor(
                            actor,
                            &request.brew_id,
                            kind,
                            filter,
                            order,
                            revision.revision,
                            "",
                        )
                    })
                    .transpose()?
            } else {
                None
            };
            NoteOutcome::History(NoteHistoryPage { items, next_cursor })
        }
        None => {
            let after = match &decoded {
                Some((position, id)) => {
                    if id.is_empty() || id.len() > 128 {
                        return Err(Error::Invalid("cursor"));
                    }
                    Some((*position, id.clone()))
                }
                None => None,
            };
            let after_ref = after
                .as_ref()
                .map(|(timestamp, id)| (timestamp, id.as_str()));
            let mut rows = store::list_current_notes(
                &mut tx,
                &workspace_id,
                &request.brew_id,
                after_ref,
                limit + 1,
            )
            .await
            .map_err(boundary_error)?;
            let next_cursor = if rows.len() as i64 > limit {
                rows.truncate(limit as usize);
                rows.last()
                    .map(|(note, _)| {
                        encode_cursor(
                            actor,
                            &request.brew_id,
                            kind,
                            filter,
                            order,
                            note.created_at,
                            &note.note_id,
                        )
                    })
                    .transpose()?
            } else {
                None
            };
            let items = rows
                .into_iter()
                .map(|(note, current)| NoteView { note, current })
                .collect();
            NoteOutcome::Page(NotePage { items, next_cursor })
        }
    };
    tx.commit().await.map_err(boundary_error)?;
    Ok(outcome)
}

pub async fn actual(
    pool: &DbPool,
    actor: &ActorContext,
    command: ActualFormulationCommand,
) -> ActualFormulationOutcome {
    let result = match command {
        ActualFormulationCommand::Amend(request) => amend_actual(pool, actor, request).await,
        ActualFormulationCommand::Get(request) => get_actual(pool, actor, request).await,
        ActualFormulationCommand::History(request) => history_actual(pool, actor, request).await,
    };
    match result {
        Ok(outcome) => outcome,
        Err(error) => map_actual_error(error),
    }
}

async fn amend_actual(
    pool: &DbPool,
    actor: &ActorContext,
    request: AmendActualFormulationRequest,
) -> Result<ActualFormulationOutcome, Error> {
    validate_identifier(&request.brew_id, "brew")?;
    validate_actual(&request.document)?;
    validate_reason(request.reason.as_deref())?;
    if matches!(request.expected_actual_revision, Some(value) if value <= 0) {
        return Err(Error::Invalid("actual revision"));
    }
    if !valid_request_id(&request.request_id) {
        return Err(Error::Invalid("request_id"));
    }
    let digest = digest_of(&(
        &request.intent,
        &request.brew_id,
        request.expected_actual_revision,
        &request.document,
        &request.reason,
    ))?;

    let mut tx = begin(pool, actor, &request.intent).await?;
    let key = RequestIdentity {
        actor,
        operation: "brew.actual.amend.v1",
        request_id: &request.request_id,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        return Ok(serde_json::from_value(result)?);
    }
    let Some(state) = lock_brew(&mut tx, actor, &request.brew_id).await? else {
        return Ok(ActualFormulationOutcome::NotFound);
    };
    if state.archived {
        return Ok(ActualFormulationOutcome::Archived);
    }
    let current = store::load_current_actual(&mut tx, &actor.workspace_id, &request.brew_id)
        .await
        .map_err(boundary_error)?;
    let actual_revision = current.as_ref().map(|revision| revision.revision);
    if request.expected_actual_revision != actual_revision {
        return Ok(ActualFormulationOutcome::Conflict {
            expected: request.expected_actual_revision,
            actual: actual_revision,
        });
    }

    let (_, content_digest) = canonical_document(&request.document)?;
    let revision = ActualFormulationRevision {
        workspace_id: actor.workspace_id.clone(),
        brew_id: request.brew_id.clone(),
        revision: actual_revision.unwrap_or(0) + 1,
        predecessor_revision: actual_revision,
        document: request.document,
        content_digest,
        actor_id: actor.actor_id.clone(),
        recorded_at: now(),
        reason: request.reason,
    };
    store::insert_actual_formulation(&mut tx, &revision)
        .await
        .map_err(boundary_error)?;
    store::set_actual_revision(
        &mut tx,
        &actor.workspace_id,
        &request.brew_id,
        revision.revision,
    )
    .await
    .map_err(boundary_error)?;

    let outcome = ActualFormulationOutcome::Amended(revision);
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&outcome)?,
        Some(&request.brew_id),
    )
    .await?;
    record_change(
        &mut tx,
        actor,
        "brew.actual.amend.v1",
        &request.brew_id,
        state.revision,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(outcome)
}

async fn get_actual(
    pool: &DbPool,
    actor: &ActorContext,
    request: ActualFormulationViewRequest,
) -> Result<ActualFormulationOutcome, Error> {
    validate_identifier(&request.brew_id, "brew")?;
    let mut tx = begin(pool, actor, &request.intent).await?;
    let workspace_id = actor.workspace_id.clone();
    let Some(stored) = brew_store::load(&mut tx, &workspace_id, &request.brew_id)
        .await
        .map_err(boundary_error)?
    else {
        return Ok(ActualFormulationOutcome::NotFound);
    };
    let current = store::load_current_actual(&mut tx, &workspace_id, &request.brew_id)
        .await
        .map_err(boundary_error)?;
    tx.commit().await.map_err(boundary_error)?;
    let view = match current {
        Some(revision) => EffectiveActualFormulation {
            source: "actual".into(),
            revision: Some(revision.revision),
            document: revision.document,
        },
        None => EffectiveActualFormulation {
            source: "plan".into(),
            revision: None,
            document: krausen_brewery::brew_evidence_store::actual_from_plan(&stored.plan),
        },
    };
    Ok(ActualFormulationOutcome::View(view))
}

async fn history_actual(
    pool: &DbPool,
    actor: &ActorContext,
    request: ActualFormulationHistoryRequest,
) -> Result<ActualFormulationOutcome, Error> {
    validate_identifier(&request.brew_id, "brew")?;
    let limit = validate_limit(request.limit)?;
    let after = match request.cursor.as_deref() {
        Some(cursor) => {
            let (position, id) = decode_cursor(
                cursor,
                actor,
                &request.brew_id,
                "actual.history",
                "all",
                "revision",
            )?;
            if position <= 0 || !id.is_empty() {
                return Err(Error::Invalid("cursor"));
            }
            Some(position)
        }
        None => None,
    };
    let mut tx = begin(pool, actor, &request.intent).await?;
    let workspace_id = actor.workspace_id.clone();
    if store::load_state(&mut tx, &workspace_id, &request.brew_id)
        .await
        .map_err(boundary_error)?
        .is_none()
    {
        return Ok(ActualFormulationOutcome::NotFound);
    }
    let mut items =
        store::list_actual_revisions(&mut tx, &workspace_id, &request.brew_id, after, limit + 1)
            .await
            .map_err(boundary_error)?;
    let next_cursor = if items.len() as i64 > limit {
        items.truncate(limit as usize);
        items
            .last()
            .map(|revision| {
                encode_cursor(
                    actor,
                    &request.brew_id,
                    "actual.history",
                    "all",
                    "revision",
                    revision.revision,
                    "",
                )
            })
            .transpose()?
    } else {
        None
    };
    tx.commit().await.map_err(boundary_error)?;
    Ok(ActualFormulationOutcome::History(ActualFormulationPage {
        items,
        next_cursor,
    }))
}

pub async fn transition(
    pool: &DbPool,
    actor: &ActorContext,
    request: TransitionBrewRequest,
) -> BrewLifecycleOutcome {
    match transition_inner(pool, actor, request).await {
        Ok(outcome) => outcome,
        Err(error) => map_lifecycle_error(error),
    }
}

async fn transition_inner(
    pool: &DbPool,
    actor: &ActorContext,
    request: TransitionBrewRequest,
) -> Result<BrewLifecycleOutcome, Error> {
    validate_identifier(&request.brew_id, "brew")?;
    validate_reason(request.reason.as_deref())?;
    if request.expected_revision <= 0 {
        return Err(Error::Invalid("brew revision"));
    }
    if !valid_request_id(&request.request_id) {
        return Err(Error::Invalid("request_id"));
    }
    let digest = digest_of(&(
        &request.intent,
        &request.brew_id,
        request.expected_revision,
        request.transition,
        &request.reason,
    ))?;

    let mut tx = begin(pool, actor, &request.intent).await?;
    let key = RequestIdentity {
        actor,
        operation: "brew.lifecycle.transition.v1",
        request_id: &request.request_id,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        return Ok(serde_json::from_value(result)?);
    }
    let Some(state) = lock_brew(&mut tx, actor, &request.brew_id).await? else {
        return Ok(BrewLifecycleOutcome::NotFound);
    };
    if state.archived {
        return Ok(BrewLifecycleOutcome::Archived);
    }
    if state.revision != request.expected_revision {
        return Ok(BrewLifecycleOutcome::Conflict {
            expected: request.expected_revision,
            actual: state.revision,
        });
    }

    let (next_status, reason_required) = match (state.status, request.transition) {
        (BrewStatusV1::Started, BrewTransitionV1::Complete) => (BrewStatusV1::Completed, false),
        (BrewStatusV1::Started, BrewTransitionV1::Cancel) => (BrewStatusV1::Cancelled, false),
        (BrewStatusV1::Completed, BrewTransitionV1::Cancel) => (BrewStatusV1::Cancelled, true),
        (BrewStatusV1::Cancelled, BrewTransitionV1::Complete) => (BrewStatusV1::Completed, true),
        (BrewStatusV1::Completed | BrewStatusV1::Cancelled, BrewTransitionV1::Reopen) => {
            (BrewStatusV1::Started, true)
        }
        (status, transition) => {
            return Ok(BrewLifecycleOutcome::InvalidTransition {
                from: status,
                to: match transition {
                    BrewTransitionV1::Complete => BrewStatusV1::Completed,
                    BrewTransitionV1::Cancel => BrewStatusV1::Cancelled,
                    BrewTransitionV1::Reopen => BrewStatusV1::Started,
                },
            });
        }
    };
    let reason = if reason_required {
        Some(required_reason(request.reason.as_deref())?)
    } else {
        request
            .reason
            .map(|reason| reason.trim().to_owned())
            .filter(|reason| !reason.is_empty())
    };

    let next_revision = state.revision + 1;
    if !store::update_status(
        &mut tx,
        &actor.workspace_id,
        &request.brew_id,
        state.revision,
        state.status,
        next_revision,
        next_status,
    )
    .await
    .map_err(boundary_error)?
    {
        let actual = store::load_state(&mut tx, &actor.workspace_id, &request.brew_id)
            .await
            .map_err(boundary_error)?
            .map(|state| state.revision)
            .unwrap_or(0);
        return Ok(BrewLifecycleOutcome::Conflict {
            expected: state.revision,
            actual,
        });
    }
    store::insert_lifecycle_event(
        &mut tx,
        &BrewLifecycleEvent {
            workspace_id: actor.workspace_id.clone(),
            brew_id: request.brew_id.clone(),
            brew_revision: next_revision,
            from_status: state.status,
            to_status: next_status,
            reason,
            actor_id: actor.actor_id.clone(),
            recorded_at: now(),
        },
    )
    .await
    .map_err(boundary_error)?;

    let outcome = BrewLifecycleOutcome::Transitioned {
        brew_id: request.brew_id.clone(),
        status: next_status,
        revision: next_revision,
    };
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&outcome)?,
        Some(&request.brew_id),
    )
    .await?;
    record_change(
        &mut tx,
        actor,
        "brew.lifecycle.transition.v1",
        &request.brew_id,
        next_revision,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(outcome)
}

pub async fn archive(
    pool: &DbPool,
    actor: &ActorContext,
    request: ArchiveBrewRequest,
) -> BrewArchiveOutcome {
    match archive_inner(pool, actor, request).await {
        Ok(outcome) => outcome,
        Err(error) => map_archive_error(error),
    }
}

async fn archive_inner(
    pool: &DbPool,
    actor: &ActorContext,
    request: ArchiveBrewRequest,
) -> Result<BrewArchiveOutcome, Error> {
    validate_identifier(&request.brew_id, "brew")?;
    if request.expected_metadata_revision <= 0 {
        return Err(Error::Invalid("metadata revision"));
    }
    if !valid_request_id(&request.request_id) {
        return Err(Error::Invalid("request_id"));
    }
    let digest = digest_of(&(
        &request.intent,
        &request.brew_id,
        request.archived,
        request.expected_metadata_revision,
    ))?;

    let mut tx = begin(pool, actor, &request.intent).await?;
    let key = RequestIdentity {
        actor,
        operation: "brew.archive.v1",
        request_id: &request.request_id,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        return Ok(serde_json::from_value(result)?);
    }
    let Some(state) = lock_brew(&mut tx, actor, &request.brew_id).await? else {
        return Ok(BrewArchiveOutcome::NotFound);
    };
    if state.metadata_revision != request.expected_metadata_revision {
        return Ok(BrewArchiveOutcome::Conflict {
            expected: request.expected_metadata_revision,
            actual: state.metadata_revision,
        });
    }
    let outcome = if state.archived == request.archived {
        BrewArchiveOutcome::Archived {
            brew_id: request.brew_id.clone(),
            archived: state.archived,
            metadata_revision: state.metadata_revision,
        }
    } else {
        let next_revision = state.metadata_revision + 1;
        if !store::update_archive(
            &mut tx,
            &actor.workspace_id,
            &request.brew_id,
            state.metadata_revision,
            next_revision,
            request.archived,
        )
        .await
        .map_err(boundary_error)?
        {
            let actual = store::load_state(&mut tx, &actor.workspace_id, &request.brew_id)
                .await
                .map_err(boundary_error)?
                .map(|state| state.metadata_revision)
                .unwrap_or(0);
            return Ok(BrewArchiveOutcome::Conflict {
                expected: state.metadata_revision,
                actual,
            });
        }
        store::insert_archive_event(
            &mut tx,
            &BrewArchiveEvent {
                workspace_id: actor.workspace_id.clone(),
                brew_id: request.brew_id.clone(),
                metadata_revision: next_revision,
                archived: request.archived,
                actor_id: actor.actor_id.clone(),
                recorded_at: now(),
            },
        )
        .await
        .map_err(boundary_error)?;
        record_change(
            &mut tx,
            actor,
            "brew.archive.v1",
            &request.brew_id,
            next_revision,
        )
        .await?;
        BrewArchiveOutcome::Archived {
            brew_id: request.brew_id.clone(),
            archived: request.archived,
            metadata_revision: next_revision,
        }
    };
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&outcome)?,
        Some(&request.brew_id),
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(outcome)
}

/// Re-exported for adapters that only need the stored brew envelope.
pub async fn load_brew(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    brew_id: &str,
) -> Result<Option<StoredBrew>, Error> {
    brew_store::load(tx, workspace_id, brew_id)
        .await
        .map_err(boundary_error)
}

#[cfg(test)]
mod retryable_mapper_tests {
    use super::*;

    #[test]
    fn retryable_is_preserved_across_new_families() {
        assert_eq!(
            map_measurement_error(Error::Retryable),
            MeasurementOutcome::Retryable
        );
        assert_eq!(map_note_error(Error::Retryable), NoteOutcome::Retryable);
        assert_eq!(
            map_actual_error(Error::Retryable),
            ActualFormulationOutcome::Retryable
        );
        assert_eq!(
            map_lifecycle_error(Error::Retryable),
            BrewLifecycleOutcome::Retryable
        );
        assert_eq!(
            map_archive_error(Error::Retryable),
            BrewArchiveOutcome::Retryable
        );
        assert_eq!(
            map_view_error(Error::Retryable),
            BrewEvidenceViewOutcome::Retryable
        );
    }
}
