// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared structured recipe process summary for printing and export.
//!
//! This is a read model built by `krausen-app` from one selected saved revision
//! or one unsaved native draft. It is not a second editable recipe and does not
//! persist anything. Historical frozen brew plans belong to their brew owner and
//! reuse the printable component rather than this request.
use crate::{
    brewery::{
        ChillingV1, EquipmentSettingsV1, IngredientSource, RecipeDocumentV1, StyleReference,
    },
    formulation::{StyleComparison, StyleRanges, Targets},
    request_context::OperationIntent,
    validation::{Diagnostic, Readiness, Validation},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProcessSummaryRequest {
    pub intent: OperationIntent,
    pub selection: ProcessSummarySelection,
}

/// Exactly one source is identified so a printed sheet cannot silently mix a
/// saved revision with a newer local draft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProcessSummarySelection {
    Saved {
        recipe_id: String,
        revision: i64,
    },
    Draft {
        recipe_id: Option<String>,
        base_revision: Option<i64>,
        document: RecipeDocumentV1,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProcessSummaryOutcome {
    Summary(ProcessSummary),
    NotFound,
    Forbidden,
    ContextChanged,
    UnresolvedEquipmentProfile,
    UnresolvedStyle,
    Invalid(Validation),
    InvalidRequest,
    /// A stored revision uses a native schema this reader does not support.
    UnsupportedRevision {
        schema_version: i32,
    },
    /// Storage, database or genuinely unknown infrastructure failure.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SummarySource {
    Saved {
        recipe_id: String,
        revision: i64,
    },
    UnsavedDraft {
        recipe_id: Option<String>,
        base_revision: Option<i64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngredientSection {
    Fermentable,
    Hop,
    Yeast,
    Other,
}

/// One projected ingredient row. Amounts keep native units and full precision;
/// absent values stay absent rather than becoming an invented zero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryIngredient {
    pub row_id: String,
    pub section: IngredientSection,
    pub name: Option<String>,
    pub producer: Option<String>,
    pub amount: Option<f64>,
    pub unit: Option<String>,
    pub stage: Option<String>,
    pub time_minutes: Option<f64>,
    pub temperature_c: Option<f64>,
    pub fermentable_type: Option<String>,
    pub yield_fraction: Option<f64>,
    pub colour_ebc: Option<f64>,
    pub alpha_fraction: Option<f64>,
    pub attenuation_fraction: Option<f64>,
    pub source: Option<IngredientSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryStep {
    pub row_id: String,
    pub name: Option<String>,
    pub time_minutes: Option<f64>,
    pub temperature_c: Option<f64>,
    pub volume_l: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryProcess {
    pub apparent_attenuation_fraction: Option<f64>,
    pub mash_steps: Vec<SummaryStep>,
    pub fermentation_steps: Vec<SummaryStep>,
    pub whirlpool_minutes: Option<f64>,
    pub whirlpool_temperature_c: Option<f64>,
    pub mash_water_l: Option<f64>,
    pub mash_water_method: Option<String>,
    pub kettle_top_up_l: Option<f64>,
    pub chilling: Option<ChillingV1>,
    pub unsupported: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryEquipment {
    pub profile_id: Option<String>,
    pub profile_name: Option<String>,
    pub profile_revision: Option<i64>,
    /// `saved_profile` or `caller_supplied_equipment_draft`, copied from the
    /// shared calculation context rather than inferred by an adapter.
    pub provenance: String,
    pub settings: EquipmentSettingsV1,
}

/// Version identities needed to interpret the included estimates. The canonical
/// digest is the same value produced by `krausen-app::formulation`; it is a
/// comparison identity, not authorization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryContext {
    pub digest: Option<String>,
    pub engine_version: String,
    pub engine_build: String,
    pub policy_version: String,
    pub native_schema_version: u32,
    pub result_schema_version: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessSummary {
    pub source: SummarySource,
    pub title: String,
    pub description: Option<String>,
    pub style: Option<StyleReference>,
    pub batch_size_l: Option<f64>,
    pub boil_minutes: Option<f64>,
    pub notes: Option<String>,
    pub readiness: Readiness,
    pub diagnostics: Vec<Diagnostic>,
    pub targets: Targets,
    pub style_ranges: Option<StyleRanges>,
    pub style_comparison: Option<StyleComparison>,
    pub assumptions: Vec<String>,
    pub equipment: SummaryEquipment,
    pub context: SummaryContext,
    pub fermentables: Vec<SummaryIngredient>,
    pub hops: Vec<SummaryIngredient>,
    pub yeast: Vec<SummaryIngredient>,
    pub other_ingredients: Vec<SummaryIngredient>,
    pub process: SummaryProcess,
}
