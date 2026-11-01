// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared start-brew command and transport-neutral outcomes.
use crate::{
    brewery::{
        ActualFormulationRevision, ActualFormulationV1, ActualSelection, BrewLifecycleEvent,
        BrewPlanV1, BrewStatusV1, MeasurementEntry, MeasurementMetadataV1, ObservationNote,
        ObservationRevision,
    },
    formulation::Preview,
    request_context::OperationIntent,
    validation::Validation,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct StartBrewRequest {
    pub intent: OperationIntent,
    pub recipe_id: String,
    /// Immutable revision selected by the caller. It may be historical.
    pub selected_recipe_revision: i64,
    /// Present only for a start-current workflow. Must equal both the selected
    /// revision and the current head; historical starts omit it.
    pub expected_head_revision: Option<i64>,
    /// Accepted context digest from the shared preview.
    pub expected_calculation_context: String,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StartBrewOutcome {
    Started {
        brew_id: String,
        status: BrewStatusV1,
        revision: i64,
        recipe_id: String,
        selected_recipe_revision: i64,
        started_at: i64,
        started_by: String,
        plan: BrewPlanV1,
    },
    /// Start-current head guard failed. `refreshed` is the current-head preview
    /// when it could be produced; it is never silently accepted.
    HeadConflict {
        expected_head: i64,
        actual_head: i64,
        refreshed: Option<Preview>,
    },
    /// The accepted context no longer matches the complete in-transaction context.
    ContextConflict {
        refreshed: Preview,
    },
    /// The selected recipe is structurally valid but its calculation is not Ready.
    NotReady(Validation),
    UnresolvedEquipmentProfile,
    UnresolvedStyle,
    RecipeArchived,
    EquipmentProfileArchived,
    NotFound,
    Forbidden,
    ContextChanged,
    InvalidRequest,
    RequestIdentityConflict,
    ResultExpired {
        resource_id: Option<String>,
    },
    Retryable,
    Unavailable,
}

fn default_measurement_source() -> String {
    "manual".into()
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BrewEvidenceViewRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
    /// Continuation for the bounded lifecycle overview.
    #[serde(default)]
    pub lifecycle_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasurementSlot {
    pub quantity_id: String,
    pub label: String,
    pub unit: String,
    pub stage: String,
    pub basis: String,
    pub target: Option<f64>,
    /// Current selection revision; `0` when no selection row exists.
    pub selection_revision: i64,
    pub selected_root_observation_id: Option<String>,
    pub current: Option<MeasurementEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteView {
    pub note: ObservationNote,
    pub current: ObservationRevision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrewEvidenceView {
    pub brew_id: String,
    pub recipe_id: String,
    pub selected_recipe_revision: i64,
    pub status: BrewStatusV1,
    pub revision: i64,
    pub started_at: i64,
    pub started_by: String,
    pub archived: bool,
    pub metadata_revision: i64,
    pub plan: BrewPlanV1,
    pub actual: Option<ActualFormulationRevision>,
    pub measurements: Vec<MeasurementSlot>,
    pub notes: Vec<NoteView>,
    /// True when more current notes exist than the bounded view returned.
    pub notes_truncated: bool,
    pub lifecycle: Vec<BrewLifecycleEvent>,
    /// Continuation for older lifecycle events when the bounded page is full.
    pub lifecycle_next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BrewEvidenceViewOutcome {
    View(BrewEvidenceView),
    NotFound,
    Forbidden,
    ContextChanged,
    Invalid,
    Retryable,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RecordMeasurementRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
    pub quantity_id: String,
    pub value: f64,
    #[serde(default)]
    pub observed_at: Option<i64>,
    #[serde(default = "default_measurement_source")]
    pub source: String,
    #[serde(default)]
    pub metadata: MeasurementMetadataV1,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CorrectMeasurementRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
    /// Current entry being corrected; must belong to this brew and quantity.
    pub entry_id: String,
    /// Required when the corrected entry is the selected root's current entry.
    #[serde(default)]
    pub expected_selection_revision: Option<i64>,
    pub value: f64,
    #[serde(default = "default_measurement_source")]
    pub source: String,
    #[serde(default)]
    pub metadata: Option<MeasurementMetadataV1>,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RetractMeasurementRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
    pub entry_id: String,
    #[serde(default)]
    pub expected_selection_revision: Option<i64>,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SelectMeasurementRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
    pub quantity_id: String,
    /// `None` clears the primary selection without deleting evidence.
    #[serde(default)]
    pub root_observation_id: Option<String>,
    pub expected_selection_revision: i64,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MeasurementListRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
    /// When present, return this root's immutable lineage; otherwise current roots.
    #[serde(default)]
    pub root_observation_id: Option<String>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasurementPage {
    pub items: Vec<MeasurementEntry>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MeasurementOutcome {
    Recorded(MeasurementEntry),
    Corrected(MeasurementEntry),
    Retracted(MeasurementEntry),
    Selected(ActualSelection),
    Page(MeasurementPage),
    NotFound,
    Archived,
    Conflict { expected: i64, actual: i64 },
    EntryConflict { expected: String, actual: String },
    RequestIdentityConflict,
    ResultExpired { resource_id: Option<String> },
    ContextChanged,
    Forbidden,
    Invalid,
    Retryable,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum MeasurementCommand {
    Record(RecordMeasurementRequest),
    Correct(CorrectMeasurementRequest),
    Retract(RetractMeasurementRequest),
    Select(SelectMeasurementRequest),
    List(MeasurementListRequest),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RecordNoteRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
    pub text: String,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CorrectNoteRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
    pub note_id: String,
    pub expected_revision: i64,
    pub text: String,
    #[serde(default)]
    pub reason: Option<String>,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RetractNoteRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
    pub note_id: String,
    pub expected_revision: i64,
    #[serde(default)]
    pub reason: Option<String>,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NoteListRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
    /// When present, return this note's immutable revision history.
    #[serde(default)]
    pub note_id: Option<String>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotePage {
    pub items: Vec<NoteView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteHistoryPage {
    pub items: Vec<ObservationRevision>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NoteOutcome {
    Recorded(ObservationRevision),
    Corrected(ObservationRevision),
    Retracted(ObservationRevision),
    Page(NotePage),
    History(NoteHistoryPage),
    NotFound,
    Archived,
    Conflict { expected: i64, actual: i64 },
    RequestIdentityConflict,
    ResultExpired { resource_id: Option<String> },
    ContextChanged,
    Forbidden,
    Invalid,
    Retryable,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum NoteCommand {
    Record(RecordNoteRequest),
    Correct(CorrectNoteRequest),
    Retract(RetractNoteRequest),
    List(NoteListRequest),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AmendActualFormulationRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
    /// `None` when the plan is still the effective actual; otherwise the current actual revision.
    #[serde(default)]
    pub expected_actual_revision: Option<i64>,
    pub document: ActualFormulationV1,
    #[serde(default)]
    pub reason: Option<String>,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ActualFormulationViewRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ActualFormulationHistoryRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EffectiveActualFormulation {
    /// `plan` until the first explicit amendment, then `actual`.
    pub source: String,
    pub revision: Option<i64>,
    pub document: ActualFormulationV1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActualFormulationPage {
    pub items: Vec<ActualFormulationRevision>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ActualFormulationOutcome {
    Amended(ActualFormulationRevision),
    View(EffectiveActualFormulation),
    History(ActualFormulationPage),
    NotFound,
    Archived,
    Conflict {
        expected: Option<i64>,
        actual: Option<i64>,
    },
    RequestIdentityConflict,
    ResultExpired {
        resource_id: Option<String>,
    },
    ContextChanged,
    Forbidden,
    Invalid,
    Retryable,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ActualFormulationCommand {
    Amend(AmendActualFormulationRequest),
    Get(ActualFormulationViewRequest),
    History(ActualFormulationHistoryRequest),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum BrewTransitionV1 {
    Complete,
    Cancel,
    Reopen,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TransitionBrewRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
    pub expected_revision: i64,
    pub transition: BrewTransitionV1,
    #[serde(default)]
    pub reason: Option<String>,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BrewLifecycleOutcome {
    Transitioned {
        brew_id: String,
        status: BrewStatusV1,
        revision: i64,
    },
    InvalidTransition {
        from: BrewStatusV1,
        to: BrewStatusV1,
    },
    Conflict {
        expected: i64,
        actual: i64,
    },
    NotFound,
    Archived,
    RequestIdentityConflict,
    ResultExpired {
        resource_id: Option<String>,
    },
    ContextChanged,
    Forbidden,
    Invalid,
    Retryable,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ArchiveBrewRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
    pub archived: bool,
    pub expected_metadata_revision: i64,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BrewArchiveOutcome {
    Archived {
        brew_id: String,
        archived: bool,
        metadata_revision: i64,
    },
    Conflict {
        expected: i64,
        actual: i64,
    },
    NotFound,
    RequestIdentityConflict,
    ResultExpired {
        resource_id: Option<String>,
    },
    ContextChanged,
    Forbidden,
    Invalid,
    Retryable,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BrewEvidenceOutcome {
    View(BrewEvidenceViewOutcome),
    Measurement(MeasurementOutcome),
    Note(NoteOutcome),
    Actual(ActualFormulationOutcome),
    Lifecycle(BrewLifecycleOutcome),
    Archive(BrewArchiveOutcome),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "family", rename_all = "snake_case")]
pub enum BrewEvidenceCommand {
    View(BrewEvidenceViewRequest),
    Measurement(MeasurementCommand),
    Note(NoteCommand),
    Actual(ActualFormulationCommand),
    Lifecycle(TransitionBrewRequest),
    Archive(ArchiveBrewRequest),
}
