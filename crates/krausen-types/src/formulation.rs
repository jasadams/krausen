// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared transport-neutral formulation inputs and preview outcomes.
use crate::{
    brewery::{EquipmentSettingsV1, RecipeDocumentV1},
    validation::Validation,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PreviewRequest {
    pub draft_generation: String,
    pub intent: crate::request_context::OperationIntent,
    pub draft: RecipeDocumentV1,
    /// Explicit unsaved settings, never a claimed saved profile.
    pub equipment_draft: EquipmentSettingsV1,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Targets {
    pub total_water_l: Option<f64>,
    pub pre_boil_l: Option<f64>,
    pub post_boil_l: Option<f64>,
    pub grain_absorption_l: Option<f64>,
    pub mash_water_l: Option<f64>,
    pub sparge_water_l: Option<f64>,
    pub kettle_water_l: Option<f64>,
    pub pre_boil_sg: Option<f64>,
    pub original_sg: Option<f64>,
    pub final_sg: Option<f64>,
    pub abv_percent: Option<f64>,
    pub colour_ebc: Option<f64>,
    pub ibu: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preview {
    pub draft_generation: String,
    pub validation: Validation,
    pub targets: Targets,
    pub context_digest: Option<String>,
    pub context: Option<CalculationContext>,
    pub style: Option<StyleRanges>,
    pub style_comparison: Option<StyleComparison>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PreviewOutcome {
    Preview(Preview),
    UnresolvedProfile,
    UnresolvedStyle,
    Forbidden,
    ContextChanged,
    Invalid,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StyleListOutcome {
    Styles(Vec<crate::brewery::StyleReference>),
    Forbidden,
    ContextChanged,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Range {
    pub min: f64,
    pub max: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleRanges {
    pub style_id: String,
    pub name: String,
    pub edition: String,
    pub reference_version: String,
    pub original_sg: Range,
    pub final_sg: Range,
    pub ibu: Range,
    pub abv_percent: Range,
    pub colour_ebc: Range,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RangeStatus {
    Below,
    Within,
    Above,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleComparison {
    pub original_sg: Option<RangeStatus>,
    pub final_sg: Option<RangeStatus>,
    pub ibu: Option<RangeStatus>,
    pub abv_percent: Option<RangeStatus>,
    pub colour_ebc: Option<RangeStatus>,
}

/// One engine fallback value actually used during calculation. It is frozen
/// with the plan so a historical read does not need a later engine build to
/// recover an applied default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppliedDefault {
    pub path: String,
    pub value: f64,
    pub unit: String,
}

/// Complete unrounded resolved inputs and version identities used for context hashing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalculationContext {
    pub workspace_id: String,
    pub draft: RecipeDocumentV1,
    pub equipment_draft: EquipmentSettingsV1,
    pub effective_equipment: EquipmentSettingsV1,
    pub equipment_provenance: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equipment_profile: Option<crate::equipment::EquipmentProfile>,
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub applied_defaults: Vec<AppliedDefault>,
    pub engine_version: String,
    pub engine_build: String,
    pub engine_source_hash: String,
    pub policy_source_hash: String,
    pub policy_version: String,
    pub native_schema_version: u32,
    pub result_schema_version: u32,
    pub style: Option<StyleRanges>,
}

/// Encode the transport envelope without allowing JSON to turn NaN/Infinity
/// into null, which would otherwise change Invalid into Incomplete.
pub fn checked_request_json(request: &PreviewRequest) -> Result<String, ()> {
    crate::request_context::checked_json(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_encoding_rejects_nonfinite_instead_of_replacing_it_with_null() {
        let mut request = PreviewRequest {
            draft_generation: "1".into(),
            intent: crate::request_context::OperationIntent {
                account_id: "a".into(),
                workspace_id: "w".into(),
            },
            draft: RecipeDocumentV1::default(),
            equipment_draft: EquipmentSettingsV1::default(),
        };
        request.draft.batch_size_l = Some(f64::NAN);
        assert!(checked_request_json(&request).is_err());
        request.draft.batch_size_l = None;
        assert!(checked_request_json(&request).is_ok());
    }
}
