// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared start-brew command and transport-neutral outcomes.
use crate::{
    brewery::{BrewPlanV1, BrewStatusV1},
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
