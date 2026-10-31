// SPDX-License-Identifier: AGPL-3.0-or-later

//! Initial, conservative conversion of the legacy BeerXML record to native data.
//! The preserved record is a sidecar, not the editable source of truth.

use krausen_types::brewery::{
    AdditionStage, FermentableAddition, FermentableSpecification, HopAddition, HopSpecification,
    IngredientAddition, IngredientSpecification, ProcessStep, RecipeDocumentV1, StyleReference,
};

/// Result of a migration projection. Original file bytes must be stored separately
/// when available; legacy database records cannot reconstruct those bytes.
pub struct NativeMigration {
    pub document: RecipeDocumentV1,
    /// Original parsed legacy record for immutable provenance storage.
    pub legacy_record: serde_json::Value,
    /// Original bytes and their SHA-256 when the original file is available.
    pub original_source: Option<OriginalSource>,
    pub warnings: Vec<MappingDiagnostic>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OriginalSource {
    pub bytes: Vec<u8>,
    pub sha256: String,
}

impl OriginalSource {
    pub fn has_valid_hash(&self) -> bool {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(&self.bytes)) == self.sha256
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NativeImportError {
    #[error(transparent)]
    Parse(#[from] crate::ParseError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Parse an original source, retaining exact UTF-8 bytes separately from edits.
pub fn from_xml(source: &str) -> Result<Vec<NativeMigration>, NativeImportError> {
    use sha2::{Digest, Sha256};
    let parsed = crate::parse(source)?;
    let original = OriginalSource {
        bytes: source.as_bytes().to_vec(),
        sha256: format!("{:x}", Sha256::digest(source.as_bytes())),
    };
    parsed
        .recipes
        .iter()
        .map(|recipe| {
            let mut result = from_legacy(recipe)?;
            result.original_source = Some(original.clone());
            result
                .warnings
                .retain(|warning| warning.code != "original_bytes_unavailable");
            Ok(result)
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MappingDiagnostic {
    pub code: String,
    pub path: String,
    pub message: String,
}

fn warning(code: &str, path: &str, message: &str) -> MappingDiagnostic {
    MappingDiagnostic {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

/// Project known unambiguous fields while retaining the complete legacy record.
pub fn from_legacy(recipe: &crate::Recipe) -> Result<NativeMigration, serde_json::Error> {
    let mut document = RecipeDocumentV1 {
        title: recipe.name().to_owned(),
        batch_size_l: recipe.header.batch_size().copied(),
        boil_minutes: recipe.header.boil_time().copied(),
        notes: recipe.header.notes().cloned(),
        style: recipe.style.as_ref().and_then(|style| {
            style.name().map(|name| StyleReference {
                name: name.clone(),
                edition: style.style_guide().cloned(),
                style_id: None,
            })
        }),
        ..Default::default()
    };
    let legacy_record = serde_json::to_value(recipe)?;
    let mut warnings = vec![warning(
        "original_bytes_unavailable",
        "source",
        "Original file bytes are unavailable in legacy records",
    )];
    if recipe.header.efficiency().is_some() {
        warnings.push(warning(
            "efficiency_choice_required",
            "equipment_overrides.efficiency_override_fraction",
            "Source efficiency retained without selecting an override",
        ));
    }
    for (index, item) in recipe.fermentables.iter().enumerate() {
        document.fermentables.push(FermentableAddition {
            row_id: format!("fermentable-{}", index + 1),
            amount_kg: item.amount().copied(),
            addition: AdditionStage {
                stage: if item.add_after_boil().copied() == Some(true) {
                    "unsupported_after_boil"
                } else {
                    "unspecified"
                }
                .into(),
                time_minutes: None,
            },
            specification: FermentableSpecification {
                name: item.name().cloned(),
                producer: item.supplier().cloned(),
                kind: item.type_().cloned(),
                yield_fraction: item.yield_().map(|v| v / 100.0),
                colour_ebc: None,
            },
            source: None,
        });
        if item.color().is_some() {
            warnings.push(warning(
                "colour_interpretation_required",
                &format!("fermentables[{index}].specification.colour_ebc"),
                "Source colour requires an explicit interpretation",
            ));
        }
    }
    for (index, item) in recipe.hops.iter().enumerate() {
        let stage = item.use_().cloned().unwrap_or_else(|| "unspecified".into());
        // BeerXML dry-hop time is in days, unlike boil contact time in minutes.
        let contact_minutes = if stage.eq_ignore_ascii_case("dry hop") {
            item.time().map(|days| days * 24.0 * 60.0)
        } else {
            item.time().copied()
        };
        document.hops.push(HopAddition {
            row_id: format!("hop-{}", index + 1),
            amount_g: item.amount().map(|v| v * 1000.0),
            stage,
            contact_minutes,
            temperature_c: None,
            specification: HopSpecification {
                name: item.name().cloned(),
                producer: None,
                alpha_fraction: item.alpha().map(|v| v / 100.0),
            },
            source: None,
        });
    }
    for (index, item) in recipe.yeasts.iter().enumerate() {
        document.yeast.push(IngredientAddition {
            row_id: format!("yeast-{}", index + 1),
            amount: item.amount().copied(),
            unit: item
                .amount_is_weight()
                .map(|weight| if *weight { "kg" } else { "l" }.to_owned()),
            specification: IngredientSpecification {
                name: item.name().cloned(),
                producer: item.laboratory().cloned(),
                attenuation_fraction: item.attenuation().map(|v| v / 100.0),
                extensions: Default::default(),
            },
            source: None,
        });
    }
    for (index, item) in recipe.miscs.iter().enumerate() {
        let mut extensions = std::collections::BTreeMap::new();
        extensions.insert("beerxml".into(), serde_json::to_value(item)?);
        document.other_ingredients.push(IngredientAddition {
            row_id: format!("misc-{}", index + 1),
            amount: item.amount().copied(),
            unit: item
                .amount_is_weight()
                .map(|weight| if *weight { "kg" } else { "l" }.to_owned()),
            specification: IngredientSpecification {
                name: item.name().cloned(),
                producer: None,
                attenuation_fraction: None,
                extensions,
            },
            source: None,
        });
    }
    if let Some(mash) = &recipe.mash {
        for (index, step) in mash.steps.iter().enumerate() {
            let mut extensions = std::collections::BTreeMap::new();
            extensions.insert("beerxml".into(), serde_json::to_value(step)?);
            document.process.mash_steps.push(ProcessStep {
                row_id: format!("mash-{}", index + 1),
                name: step.name().cloned(),
                time_minutes: step.step_time().copied(),
                temperature_c: step.step_temp().copied(),
                volume_l: step.infuse_amount().copied(),
                extensions,
            });
        }
    }
    if recipe.mash.is_some()
        || !recipe.yeasts.is_empty()
        || !recipe.miscs.is_empty()
        || !recipe.waters.is_empty()
        || recipe.equipment.is_some()
    {
        document
            .process
            .unsupported
            .insert("legacy_unmapped_process".into(), serde_json::json!(true));
        warnings.push(warning(
            "legacy_fields_unmapped",
            "process",
            "Some legacy fields are preserved but not projected",
        ));
    }
    Ok(NativeMigration {
        document,
        legacy_record,
        original_source: None,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_retains_legacy_record_and_does_not_adopt_efficiency() {
        let source = r#"<RECIPES><RECIPE><NAME>Imported</NAME><BATCH_SIZE>20</BATCH_SIZE><EFFICIENCY>75</EFFICIENCY><FERMENTABLES><FERMENTABLE><NAME>Malt</NAME><AMOUNT>4</AMOUNT><YIELD>80</YIELD><COLOR>3</COLOR></FERMENTABLE></FERMENTABLES></RECIPE></RECIPES>"#;
        let parsed = crate::parse(source).unwrap();
        let migrated = from_legacy(&parsed.recipes[0]).unwrap();
        assert_eq!(migrated.document.title, "Imported");
        assert_eq!(
            migrated.document.fermentables[0]
                .specification
                .yield_fraction,
            Some(0.8)
        );
        assert_eq!(
            migrated.document.fermentables[0].specification.colour_ebc,
            None
        );
        assert_eq!(
            migrated
                .document
                .equipment_overrides
                .efficiency_override_fraction,
            None
        );
        assert_eq!(
            migrated.legacy_record,
            serde_json::to_value(&parsed.recipes[0]).unwrap()
        );
        assert!(
            migrated
                .warnings
                .iter()
                .any(|w| w.code == "efficiency_choice_required")
        );
        let round_trip: RecipeDocumentV1 =
            serde_json::from_str(&serde_json::to_string(&migrated.document).unwrap()).unwrap();
        assert_eq!(round_trip, migrated.document);
        let imported = from_xml(source).unwrap().remove(0);
        assert_eq!(
            imported.original_source.as_ref().unwrap().bytes,
            source.as_bytes()
        );
        assert_eq!(imported.original_source.as_ref().unwrap().sha256.len(), 64);
        assert!(
            !imported
                .warnings
                .iter()
                .any(|w| w.code == "original_bytes_unavailable")
        );
    }

    #[test]
    fn maps_stable_rows_and_preserves_unsupported_source() {
        let source = include_str!("../tests/fixtures/synthetic.xml");
        let migrated = from_xml(source).unwrap().remove(0);
        assert_eq!(migrated.document.fermentables[0].row_id, "fermentable-1");
        assert_eq!(migrated.document.hops[0].amount_g, Some(30.0));
        assert_eq!(
            migrated.document.process.mash_steps[0].temperature_c,
            Some(65.0)
        );
        assert_eq!(
            migrated.document.other_ingredients[0]
                .specification
                .name
                .as_deref(),
            Some("Example spice")
        );
        let legacy: crate::Recipe = serde_json::from_value(migrated.legacy_record).unwrap();
        assert_eq!(legacy, crate::parse(source).unwrap().recipes.remove(0));
        assert!(
            migrated
                .document
                .process
                .unsupported
                .contains_key("legacy_unmapped_process")
        );
    }
}
