// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared ingredient catalogue operation contracts.
//!
//! Starter entries are the explicit public read-only exception: they carry
//! catalogue/edition identity and documented provenance instead of workspace
//! ownership. Workspace custom entries are strictly tenant-scoped. The
//! versioned native records themselves live in [`crate::brewery`], like the
//! equipment records, and are re-exported here for adapter convenience.
use crate::request_context::OperationIntent;
use serde::{Deserialize, Serialize};

pub use crate::brewery::{
    CatalogueScope, INGREDIENT_ENTRY_KINDS, IngredientEntry, IngredientEntryDocumentV1,
    IngredientProvenance,
};

/// Picker filter. Mirrors the approved "All fermentables"/"Your library" pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum CatalogueFilter {
    #[default]
    All,
    Workspace,
}

impl CatalogueFilter {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Workspace => "workspace",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngredientPage {
    pub items: Vec<IngredientEntry>,
    pub next_cursor: Option<String>,
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SaveIngredientEntry {
    pub intent: OperationIntent,
    /// Server-generated UUID for creation; the existing ID for an edit.
    pub entry_id: String,
    pub document: IngredientEntryDocumentV1,
    /// Create requires 0; an edit requires the observed content revision.
    pub expected_revision: i64,
    /// Create requires 0; an edit requires the observed metadata revision.
    pub expected_metadata_revision: i64,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ArchiveIngredientEntry {
    pub intent: OperationIntent,
    pub entry_id: String,
    pub archived: bool,
    pub expected_metadata_revision: i64,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct IngredientQuery {
    pub intent: OperationIntent,
    pub entry_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SearchCatalogue {
    pub intent: OperationIntent,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub filter: CatalogueFilter,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IngredientOutcome {
    Entry(IngredientEntry),
    Entries(IngredientPage),
    NotFound,
    Archived,
    Duplicate,
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
pub enum IngredientCommand {
    Save(SaveIngredientEntry),
    Archive(ArchiveIngredientEntry),
    Get(IngredientQuery),
    Search(SearchCatalogue),
}
