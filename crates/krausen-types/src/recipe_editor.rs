// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared recipe editor outcomes for browser and MCP adapters.
use crate::{brewery::RecipeDocumentV1, validation::Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeSnapshot {
    pub recipe_id: String,
    pub revision: i64,
    pub document: RecipeDocumentV1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RecipeReadOutcome {
    Found(RecipeSnapshot),
    NotFound,
    Forbidden,
    ContextChanged,
    Unavailable,
}

/// The workspace default profile a create took, and its revision when it was applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppliedEquipment {
    pub profile_id: String,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RecipeSaveOutcome {
    Saved {
        recipe_id: String,
        revision: i64,
        /// The profile the server applied because the recipe named none: the caller adopts it.
        #[serde(default)]
        default_equipment: Option<AppliedEquipment>,
        /// The user's default batch size the server applied because a new recipe carried none:
        /// the caller adopts it.
        #[serde(default)]
        default_batch_size_l: Option<f64>,
    },
    Invalid(Validation),
    UnresolvedEquipmentProfile,
    UnresolvedIngredientSource,
    InvalidProvenance,
    InvalidRequest,
    Conflict { expected: i64, actual: i64 },
    NotFound,
    Forbidden,
    ContextChanged,
    RequestIdentityConflict,
    ResultExpired { resource_id: Option<String> },
    Retryable,
    Unavailable,
}
