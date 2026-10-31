// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared recipe library and retained revision operation contracts.
use crate::{brewery::RecipeDocumentV1, request_context::OperationIntent};
use serde::{Deserialize, Serialize};
fn default_limit() -> i64 {
    50
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LibraryQuery {
    pub intent: OperationIntent,
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HistoryQuery {
    pub intent: OperationIntent,
    pub recipe_id: String,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RevisionQuery {
    pub intent: OperationIntent,
    pub recipe_id: String,
    pub revision: i64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ArchiveRecipe {
    pub intent: OperationIntent,
    pub recipe_id: String,
    pub expected_metadata_revision: i64,
    pub archived: bool,
    pub request_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RecoveryQuery {
    pub intent: OperationIntent,
    pub recipe_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeSummary {
    pub recipe_id: String,
    pub title: String,
    pub style: Option<String>,
    pub batch_size_l: Option<f64>,
    pub revision: i64,
    pub metadata_revision: i64,
    pub archived: bool,
    pub needs_recovery: bool,
    pub created_at: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RevisionSummary {
    pub revision: i64,
    pub saved_by: Option<String>,
    pub saved_at: Option<String>,
    pub restored_from_revision: Option<i64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryPage {
    pub items: Vec<RecipeSummary>,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryPage {
    pub items: Vec<RevisionSummary>,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LibraryOutcome {
    Recipes(LibraryPage),
    History(HistoryPage),
    Revision(RecipeDocumentV1),
    LegacySource {
        source: String,
        diagnostic: String,
    },
    Archive {
        recipe_id: String,
        metadata_revision: i64,
        archived: bool,
    },
    NotFound,
    Conflict {
        expected: i64,
        actual: i64,
    },
    RequestIdentityConflict,
    ResultExpired,
    ContextChanged,
    Forbidden,
    Invalid,
    Unavailable,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum LibraryCommand {
    List(LibraryQuery),
    History(HistoryQuery),
    Revision(RevisionQuery),
    Archive(ArchiveRecipe),
    Recover(RecoveryQuery),
}
