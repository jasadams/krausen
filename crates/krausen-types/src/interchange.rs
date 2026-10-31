// SPDX-License-Identifier: AGPL-3.0-or-later
//! Transport-neutral BeerXML choices and outcomes.
use serde::{Deserialize, Serialize};

/// Maximum UTF-8 source size accepted by the shared import operation.
pub const MAX_BEERXML_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ColourUnit {
    Lovibond,
    Srm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum EfficiencyChoice {
    RecipeOverride,
    SelectedEquipment,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ImportChoices {
    pub colour: Option<ColourUnit>,
    pub efficiency: Option<EfficiencyChoice>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MappingDiagnostic {
    pub code: String,
    pub path: String,
    pub message: String,
}

/// Source application's estimates, shown for comparison and never used as inputs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEstimates {
    pub original_gravity: Option<String>,
    pub final_gravity: Option<String>,
    pub colour: Option<String>,
    pub bitterness: Option<String>,
    pub alcohol: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ImportRequest {
    pub intent: crate::request_context::OperationIntent,
    pub xml: String,
    pub recipe_index: usize,
    pub choices: ImportChoices,
    pub equipment_profile_id: Option<String>,
    /// Revision observed in the shared preview; required when importing with a profile.
    pub expected_profile_revision: Option<i64>,
    pub expected_profile_metadata_revision: Option<i64>,
    pub equipment_draft: crate::brewery::EquipmentSettingsV1,
    pub draft_generation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportPreview {
    pub draft_generation: String,
    pub document: crate::brewery::RecipeDocumentV1,
    pub source_sha256: String,
    pub selected_profile_revision: Option<i64>,
    pub selected_profile_metadata_revision: Option<i64>,
    pub source_estimates: SourceEstimates,
    pub diagnostics: Vec<MappingDiagnostic>,
    pub calculation: crate::formulation::PreviewOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ImportPreviewOutcome {
    Preview(ImportPreview),
    InvalidSource,
    Forbidden,
    ContextChanged,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ImportCommitOutcome {
    Imported {
        recipe_id: String,
        revision: i64,
        source_sha256: String,
        diagnostics: Vec<MappingDiagnostic>,
    },
    ChoicesRequired(Vec<MappingDiagnostic>),
    InvalidSource,
    InvalidRequest,
    Invalid(crate::validation::Validation),
    UnresolvedProfile,
    ProfileChanged,
    Forbidden,
    ContextChanged,
    RequestIdentityConflict,
    ResultExpired {
        resource_id: Option<String>,
    },
    Retryable,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ExportSelection {
    Saved {
        recipe_id: String,
        revision: i64,
    },
    Draft {
        recipe_id: Option<String>,
        document: crate::brewery::RecipeDocumentV1,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ExportRequest {
    pub intent: crate::request_context::OperationIntent,
    pub selection: ExportSelection,
    pub acknowledge_loss: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExportOutcome {
    Exported {
        xml: String,
        diagnostics: Vec<MappingDiagnostic>,
        unsaved_draft: bool,
    },
    LossAcknowledgementRequired(Vec<MappingDiagnostic>),
    NotFound,
    Invalid,
    Forbidden,
    ContextChanged,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OriginalOutcome {
    Original { bytes: Vec<u8>, sha256: String },
    NotFound,
    Forbidden,
    ContextChanged,
    Unavailable,
}
