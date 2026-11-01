// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared read/comparison and feedback contracts for recipe brew history.
//!
//! These operations project KRA-20 immutable brew plans and KRA-21 evidence.
//! They never become a second recipe, plan or evidence store.

use crate::{
    brew::MeasurementSlot,
    brewery::{ActualFormulationRevision, BrewPlanV1, BrewStatusV1, MeasurementEntry},
    formulation::{AppliedDefault, Preview},
    process_summary::ProcessSummary,
    request_context::OperationIntent,
    validation::Validation,
};
use serde::{Deserialize, Serialize};

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RecipeBrewHistoryQuery {
    pub intent: OperationIntent,
    pub recipe_id: String,
    /// Explicit history includes archived brews by default; archive only hides
    /// ordinary lists, never retained evidence.
    #[serde(default = "default_true")]
    pub include_archived: bool,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeBrewSummary {
    pub brew_id: String,
    pub recipe_id: String,
    pub selected_recipe_revision: i64,
    pub status: BrewStatusV1,
    pub revision: i64,
    pub started_at: i64,
    pub started_by: String,
    pub archived: bool,
    pub metadata_revision: i64,
    pub actual_revision: Option<i64>,
    pub plan_content_digest: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeBrewHistoryPage {
    pub items: Vec<RecipeBrewSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RecipeBrewHistoryOutcome {
    History(RecipeBrewHistoryPage),
    NotFound,
    Forbidden,
    ContextChanged,
    InvalidRequest,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BrewComparisonRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
}

/// Target from the immutable plan beside the currently selected reading.
/// Missing and unlike-basis values are explicit, never coerced to zero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrewMeasurementComparison {
    pub quantity_id: String,
    pub label: String,
    pub unit: String,
    pub stage: String,
    pub basis: String,
    pub target: Option<f64>,
    pub selected: Option<MeasurementEntry>,
    /// False when the planned plan has no target for this quantity/basis.
    pub comparable: bool,
    /// Only present when both sides are present and the planned target exists.
    pub delta: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrewHistorySection {
    Fermentable,
    Hop,
    Yeast,
    OtherIngredient,
    Process,
    MashStep,
    FermentationStep,
    Notes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrewChangeKind {
    Changed,
    Added,
    Removed,
}

/// Complete value for one changed row/field. Absent values remain absent.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BrewComparisonValue {
    pub name: Option<String>,
    pub amount: Option<f64>,
    pub unit: Option<String>,
    pub stage: Option<String>,
    pub time_minutes: Option<f64>,
    pub temperature_c: Option<f64>,
    pub volume_l: Option<f64>,
    pub text: Option<String>,
}

/// One stable, selectable actual-formulation feedback change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrewFormulationChange {
    /// Stable key used by feedback preview/apply selection.
    pub id: String,
    pub section: BrewHistorySection,
    pub row_id: Option<String>,
    /// Process scalar name when `section == Process`.
    pub field: Option<String>,
    pub kind: BrewChangeKind,
    pub planned: Option<BrewComparisonValue>,
    pub actual: Option<BrewComparisonValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrewComparison {
    pub brew_id: String,
    pub recipe_id: String,
    pub selected_recipe_revision: i64,
    pub status: BrewStatusV1,
    pub revision: i64,
    pub metadata_revision: i64,
    pub archived: bool,
    pub plan: BrewPlanV1,
    pub actual: Option<ActualFormulationRevision>,
    pub measurements: Vec<BrewMeasurementComparison>,
    pub changes: Vec<BrewFormulationChange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BrewComparisonOutcome {
    Comparison(BrewComparison),
    NotFound,
    Forbidden,
    ContextChanged,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BrewFeedbackPreviewRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
    /// The current actual revision the caller selected; stale values conflict.
    #[serde(default)]
    pub expected_actual_revision: Option<i64>,
    /// Stable change IDs from `BrewComparison.changes`.
    pub change_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrewFeedbackPreview {
    pub brew_id: String,
    pub recipe_id: String,
    pub source_actual_revision: i64,
    /// Current recipe head the proposal is based on.
    pub base_recipe_head: i64,
    pub proposed_recipe: crate::brewery::RecipeDocumentV1,
    pub applied_change_ids: Vec<String>,
    pub preview: Preview,
    /// Mapping limitations/warnings, never silent field drops.
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BrewFeedbackPreviewOutcome {
    Preview(BrewFeedbackPreview),
    NoActual,
    NoChanges,
    NotFound,
    Forbidden,
    ContextChanged,
    Conflict {
        expected: Option<i64>,
        actual: Option<i64>,
    },
    InvalidRequest,
    UnresolvedProfile,
    UnresolvedStyle,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BrewFeedbackApplyRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
    #[serde(default)]
    pub expected_actual_revision: Option<i64>,
    pub change_ids: Vec<String>,
    pub expected_recipe_head: i64,
    /// Accepted context digest from feedback preview.
    pub expected_calculation_context: String,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BrewFeedbackApplyOutcome {
    Applied {
        brew_id: String,
        recipe_id: String,
        source_actual_revision: i64,
        recipe_revision: i64,
    },
    Unchanged {
        brew_id: String,
        recipe_id: String,
        recipe_revision: i64,
    },
    HeadConflict {
        expected: i64,
        actual: i64,
    },
    ContextConflict {
        refreshed: Preview,
    },
    ActualConflict {
        expected: Option<i64>,
        actual: Option<i64>,
    },
    NoActual,
    NoChanges,
    NotFound,
    Forbidden,
    ContextChanged,
    InvalidRequest,
    UnresolvedProfile,
    UnresolvedStyle,
    Invalid(Validation),
    UnresolvedIngredientSource,
    RequestIdentityConflict,
    ResultExpired {
        resource_id: Option<String>,
    },
    Retryable,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BrewPrintRequest {
    pub intent: OperationIntent,
    pub brew_id: String,
}

/// Frozen historical print/export. `summary` is built only from the stored plan;
/// `actual` and `measurements` remain separate and never replace original targets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrewPrint {
    pub brew_id: String,
    pub recipe_id: String,
    pub selected_recipe_revision: i64,
    pub summary: ProcessSummary,
    pub applied_defaults: Vec<AppliedDefault>,
    pub actual: Option<ActualFormulationRevision>,
    pub measurements: Vec<MeasurementSlot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BrewPrintOutcome {
    Print(BrewPrint),
    NotFound,
    Forbidden,
    ContextChanged,
    InvalidRequest,
    Unavailable,
}
