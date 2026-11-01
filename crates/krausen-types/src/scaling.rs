// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared linear-ingredients-v1 scaling and duplication contracts.
//!
//! Preview and apply are separate operations. Apply is guarded by the observed
//! recipe head and by the complete calculation context accepted from the
//! server-issued preview, never by a client-calculated target.

use crate::{
    brewery::RecipeDocumentV1,
    formulation::{CalculationContext, Targets},
    request_context::OperationIntent,
    validation::Validation,
};
use serde::{Deserialize, Serialize};

pub const SCALE_POLICY_LINEAR_INGREDIENTS_V1: &str = "linear-ingredients-v1";

/// Read-only scaling preview input. The source is always the current saved,
/// non-archived head; unsaved drafts are scaled through the editor draft only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ScalePreviewRequest {
    pub intent: OperationIntent,
    pub recipe_id: String,
    pub target_batch_size_l: f64,
}

/// The server-issued context a caller accepted in a scaling preview.
///
/// `digest` binds the source revision, target, policy and calculation-context
/// digest. It is a comparison value, not an authorization token; apply still
/// resolves every component under its own transaction and lock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcceptedScaleContext {
    pub source_revision: i64,
    pub target_batch_size_l: f64,
    pub policy_version: String,
    pub calculation_context_digest: String,
    pub context: CalculationContext,
    pub targets: Targets,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScaleApplyRequest {
    pub intent: OperationIntent,
    pub recipe_id: String,
    pub expected_revision: i64,
    pub target_batch_size_l: f64,
    pub accepted: AcceptedScaleContext,
    pub request_id: String,
}

/// Field-level multiplier evidence. Only fields that actually change are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ScaleSection {
    Fermentable,
    Hop,
    MashWater,
    KettleTopUp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ScaleChange {
    pub section: ScaleSection,
    pub row_id: Option<String>,
    pub field: String,
    pub before: f64,
    pub after: f64,
}

/// Explicit effects of scaling that are not themselves multiplied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ScaleWarning {
    KettleCapacityExceeded {
        capacity_l: f64,
        post_boil_l: f64,
        pre_boil_l: Option<f64>,
    },
    CapacityUnknown {
        post_boil_l: f64,
    },
    FixedLossesRetained {
        transfer_loss_l: Option<f64>,
        boil_off_l_per_hour: Option<f64>,
        pre_boil_deadspace_l: Option<f64>,
        grain_absorption_l_per_kg: Option<f64>,
    },
    UnscaledIngredients {
        yeast_rows: usize,
        other_ingredient_rows: usize,
    },
    DerivedMashWater {
        thickness_l_per_kg: f64,
    },
}

/// Reasons scaling cannot produce a preview. These are input/identity problems,
/// never silently replaced with a factor of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ScaleProblem {
    MissingSourceVolume,
    InvalidSourceVolume,
    InvalidTargetVolume,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScalePreview {
    pub recipe_id: String,
    pub source_revision: i64,
    pub source_batch_size_l: f64,
    pub target_batch_size_l: f64,
    pub factor: f64,
    pub source_document: RecipeDocumentV1,
    pub scaled_document: RecipeDocumentV1,
    pub changes: Vec<ScaleChange>,
    pub source_targets: Targets,
    pub targets: Targets,
    pub validation: Validation,
    pub warnings: Vec<ScaleWarning>,
    pub accepted: AcceptedScaleContext,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScaleOutcome {
    Preview(ScalePreview),
    Unscalable(ScaleProblem),
    Archived,
    NotFound,
    Forbidden,
    ContextChanged,
    UnresolvedProfile,
    UnresolvedStyle,
    Invalid(Validation),
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScaleApplyOutcome {
    Applied {
        recipe_id: String,
        revision: i64,
        source_revision: i64,
    },
    Unchanged {
        recipe_id: String,
        revision: i64,
    },
    Conflict {
        expected: i64,
        actual: i64,
    },
    ContextConflict {
        fresh: Box<ScalePreview>,
    },
    Unscalable(ScaleProblem),
    Archived,
    NotFound,
    Forbidden,
    ContextChanged,
    UnresolvedProfile,
    UnresolvedStyle,
    Invalid(Validation),
    InvalidRequest,
    RequestIdentityConflict,
    ResultExpired {
        resource_id: Option<String>,
    },
    Retryable,
    Unavailable,
}

/// Duplicate the current saved, non-archived head under a new independent identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DuplicateRecipe {
    pub intent: OperationIntent,
    pub source_recipe_id: String,
    pub expected_revision: i64,
    pub title: Option<String>,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DuplicateOutcome {
    Duplicated {
        recipe_id: String,
        revision: i64,
        source_revision: i64,
    },
    Conflict {
        expected: i64,
        actual: i64,
    },
    Archived,
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
