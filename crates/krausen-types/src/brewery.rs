// SPDX-License-Identifier: AGPL-3.0-or-later

//! Versioned native brewing documents. Values are data, not validation policy.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Stored revision envelope. Legacy backfills may lack an observed actor/time;
/// migration time is recorded separately rather than invented as authorship.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeRevisionV1 {
    pub workspace_id: String,
    pub recipe_id: String,
    pub revision: u64,
    pub document: RecipeDocumentV1,
    pub content_digest: String,
    pub saved_by: Option<String>,
    pub saved_at: Option<String>,
    pub migrated_at: Option<String>,
    pub summary: Option<String>,
}

/// A recipe's editable content; identity and authorship live in its revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeDocumentV1 {
    pub schema_version: u32,
    pub title: String,
    pub description: Option<String>,
    pub style: Option<StyleReference>,
    pub batch_size_l: Option<f64>,
    pub boil_minutes: Option<f64>,
    pub equipment_profile_id: Option<String>,
    #[serde(default)]
    pub equipment_overrides: EquipmentSettingsV1,
    pub fermentables: Vec<FermentableAddition>,
    pub hops: Vec<HopAddition>,
    pub yeast: Vec<IngredientAddition>,
    pub other_ingredients: Vec<IngredientAddition>,
    pub process: ProcessV1,
    pub notes: Option<String>,
    pub import_provenance_id: Option<String>,
    pub extensions: BTreeMap<String, Value>,
}

impl Default for RecipeDocumentV1 {
    fn default() -> Self {
        Self {
            schema_version: 1,
            title: String::new(),
            description: None,
            style: None,
            batch_size_l: None,
            boil_minutes: None,
            equipment_profile_id: None,
            equipment_overrides: EquipmentSettingsV1::default(),
            fermentables: vec![],
            hops: vec![],
            yeast: vec![],
            other_ingredients: vec![],
            process: ProcessV1::default(),
            notes: None,
            import_provenance_id: None,
            extensions: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleReference {
    pub name: String,
    pub edition: Option<String>,
    pub style_id: Option<String>,
}

/// Sparse settings represent absence, including an explicit recipe override.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EquipmentSettingsV1 {
    pub schema_version: u32,
    pub efficiency_override_fraction: Option<f64>,
    pub mash_efficiency_fraction: Option<f64>,
    pub boil_off_l_per_hour: Option<f64>,
    pub transfer_loss_l: Option<f64>,
    pub pre_boil_deadspace_l: Option<f64>,
    pub grain_absorption_l_per_kg: Option<f64>,
    pub mash_thickness_l_per_kg: Option<f64>,
    pub kettle_capacity_l: Option<f64>,
    pub kettle_diameter_cm: Option<f64>,
    pub kettle_opening_cm: Option<f64>,
    pub boil_temperature_c: Option<f64>,
    pub chilling: Option<ChillingV1>,
    pub extensions: BTreeMap<String, Value>,
}

impl Default for EquipmentSettingsV1 {
    fn default() -> Self {
        Self {
            schema_version: 1,
            efficiency_override_fraction: None,
            mash_efficiency_fraction: None,
            boil_off_l_per_hour: None,
            transfer_loss_l: None,
            pre_boil_deadspace_l: None,
            grain_absorption_l_per_kg: None,
            mash_thickness_l_per_kg: None,
            kettle_capacity_l: None,
            kettle_diameter_cm: None,
            kettle_opening_cm: None,
            boil_temperature_c: None,
            chilling: None,
            extensions: BTreeMap::new(),
        }
    }
}

/// Absent settings are distinct from an explicit choice of natural cooling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum ChillingV1 {
    Natural,
    Forced {
        rate_per_minute: Option<f64>,
        plateau_c: Option<f64>,
    },
    Counterflow {
        litres_per_minute: Option<f64>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngredientSource {
    pub scope: String,
    pub catalogue_id: Option<String>,
    pub entry_id: String,
    pub version: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FermentableSpecification {
    pub name: String,
    pub producer: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub yield_fraction: Option<f64>,
    pub colour_ebc: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdditionStage {
    pub stage: String,
    pub time_minutes: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FermentableAddition {
    pub row_id: String,
    pub amount_kg: Option<f64>,
    pub addition: AdditionStage,
    pub specification: FermentableSpecification,
    pub source: Option<IngredientSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HopSpecification {
    pub name: String,
    pub producer: Option<String>,
    pub alpha_fraction: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HopAddition {
    pub row_id: String,
    pub amount_g: Option<f64>,
    pub stage: String,
    pub contact_minutes: Option<f64>,
    pub temperature_c: Option<f64>,
    pub specification: HopSpecification,
    pub source: Option<IngredientSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngredientSpecification {
    pub name: String,
    pub producer: Option<String>,
    pub attenuation_fraction: Option<f64>,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngredientAddition {
    pub row_id: String,
    pub amount: Option<f64>,
    pub unit: Option<String>,
    pub specification: IngredientSpecification,
    pub source: Option<IngredientSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessStep {
    pub row_id: String,
    pub name: Option<String>,
    pub time_minutes: Option<f64>,
    pub temperature_c: Option<f64>,
    pub volume_l: Option<f64>,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProcessV1 {
    pub mash_steps: Vec<ProcessStep>,
    pub fermentation_steps: Vec<ProcessStep>,
    pub whirlpool_minutes: Option<f64>,
    pub whirlpool_temperature_c: Option<f64>,
    pub mash_water_l: Option<f64>,
    pub mash_water_method: Option<String>,
    pub kettle_top_up_l: Option<f64>,
    pub unsupported: BTreeMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_document_round_trips_without_inventing_values() {
        let mut document = RecipeDocumentV1::default();
        document.title = "Draft".into();
        document
            .process
            .unsupported
            .insert("schedule".into(), serde_json::json!({"stage": "custom"}));
        document
            .extensions
            .insert("vendor".into(), serde_json::json!({"unknown": [1, 2]}));
        let encoded = serde_json::to_string(&document).unwrap();
        let restored: RecipeDocumentV1 = serde_json::from_str(&encoded).unwrap();
        assert_eq!(restored, document);
        assert_eq!(restored.batch_size_l, None);
        assert_eq!(restored.process.unsupported["schedule"]["stage"], "custom");
    }
}
