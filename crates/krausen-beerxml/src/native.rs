// SPDX-License-Identifier: AGPL-3.0-or-later

//! Initial, conservative conversion of the legacy BeerXML record to native data.
//! The preserved record is a sidecar, not the editable source of truth.

use krausen_types::brewery::{
    AdditionStage, FermentableAddition, FermentableSpecification, HopAddition, HopSpecification,
    IngredientAddition, IngredientSpecification, ProcessStep, RecipeDocumentV1, StyleReference,
};

/// Mapping used for newly committed imports and stable-row source matching.
pub const MAPPING_VERSION: &str = "beerxml-native-v2";

/// Result of a migration projection. Original file bytes must be stored separately
/// when available; legacy database records cannot reconstruct those bytes.
pub struct NativeMigration {
    pub document: RecipeDocumentV1,
    /// Original parsed legacy record for immutable provenance storage.
    pub legacy_record: serde_json::Value,
    /// Original bytes and their SHA-256 when the original file is available.
    pub original_source: Option<OriginalSource>,
    /// Source element locations and raw opening tags, including attributes.
    /// The offsets address the immutable original bytes, not an editable XML model.
    pub source_elements: Vec<SourceElement>,
    pub choices: Option<ImportChoices>,
    pub source_recipe_index: Option<usize>,
    pub source_estimates: krausen_types::interchange::SourceEstimates,
    pub warnings: Vec<MappingDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceElement {
    pub path: String,
    pub row_id: Option<String>,
    pub start: u64,
    pub open_end: u64,
}

/// Index each source element, including envelope and unknown elements. Offsets
/// preserve the original opening tag and attributes without normalising them.
fn source_elements(source: &str) -> Result<Vec<SourceElement>, crate::ParseError> {
    use quick_xml::{Reader, events::Event};
    use std::collections::BTreeMap;
    let mut reader = Reader::from_str(source);
    let mut stack: Vec<(String, BTreeMap<String, usize>, Option<String>)> = Vec::new();
    let mut root_counts = BTreeMap::new();
    let mut out = Vec::new();
    loop {
        let start = reader.buffer_position();
        let event = reader.read_event().map_err(|e| crate::ParseError::Xml {
            position: reader.buffer_position(),
            message: e.to_string(),
        })?;
        match event {
            Event::Start(element) | Event::Empty(element) => {
                let empty =
                    source.as_bytes().get(reader.buffer_position() as usize - 2) == Some(&b'/');
                let name = String::from_utf8_lossy(element.name().as_ref()).into_owned();
                let counts = stack
                    .last_mut()
                    .map(|p| &mut p.1)
                    .unwrap_or(&mut root_counts);
                let count = counts.entry(name.clone()).or_insert(0);
                *count += 1;
                let index = *count;
                let parent = stack.last().map_or("", |p| p.0.as_str());
                let path = format!("{parent}/{name}[{index}]");
                let own_row = match name.as_str() {
                    "FERMENTABLE" => Some(format!("fermentable-{index}")),
                    "HOP" => Some(format!("hop-{index}")),
                    "YEAST" => Some(format!("yeast-{index}")),
                    "MISC" => Some(format!("misc-{index}")),
                    "MASH_STEP" => Some(format!("mash-{index}")),
                    _ => stack.last().and_then(|p| p.2.clone()),
                };
                out.push(SourceElement {
                    path: path.clone(),
                    row_id: own_row.clone(),
                    start,
                    open_end: reader.buffer_position(),
                });
                if out.len() > 25_000 {
                    return Err(crate::ParseError::Xml {
                        position: reader.buffer_position(),
                        message: "source contains too many elements".into(),
                    });
                }
                if !empty {
                    stack.push((path, BTreeMap::new(), own_row));
                }
            }
            Event::End(_) => {
                stack.pop();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
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
    #[error("source contains characters forbidden by XML 1.0")]
    InvalidCharacters,
    #[error("source must declare UTF-8 encoding")]
    UnsupportedEncoding,
}

/// XML 1.0 character repertoire; callers must reject rather than strip data.
pub fn valid_xml_characters(text: &str) -> bool {
    text.chars().all(|c| matches!(c as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF))
}

pub use krausen_types::interchange::{
    ColourUnit, EfficiencyChoice, ImportChoices, MappingDiagnostic,
};

/// Parse an original source, retaining exact UTF-8 bytes separately from edits.
pub fn from_xml(source: &str) -> Result<Vec<NativeMigration>, NativeImportError> {
    from_xml_with_choices(source, &ImportChoices::default())
}

/// Project source values with explicit choices, without changing the source record.
pub fn from_xml_with_choices(
    source: &str,
    choices: &ImportChoices,
) -> Result<Vec<NativeMigration>, NativeImportError> {
    project_xml(source, choices, None)
}

/// Project one selected recipe without copying a multi-recipe attachment once
/// per unselected record.
pub fn from_xml_at(
    source: &str,
    choices: &ImportChoices,
    recipe_index: usize,
) -> Result<Option<NativeMigration>, NativeImportError> {
    Ok(project_xml(source, choices, Some(recipe_index))?
        .into_iter()
        .next())
}

fn project_xml(
    source: &str,
    choices: &ImportChoices,
    selected: Option<usize>,
) -> Result<Vec<NativeMigration>, NativeImportError> {
    use sha2::{Digest, Sha256};
    if !valid_xml_characters(source) {
        return Err(NativeImportError::InvalidCharacters);
    }
    let mut reader = quick_xml::Reader::from_str(source);
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Decl(declaration)) => {
                if let Some(encoding) = declaration.encoding() {
                    let encoding = encoding.map_err(|_| NativeImportError::UnsupportedEncoding)?;
                    if !encoding.eq_ignore_ascii_case(b"UTF-8")
                        && !encoding.eq_ignore_ascii_case(b"UTF8")
                    {
                        return Err(NativeImportError::UnsupportedEncoding);
                    }
                }
            }
            Ok(
                quick_xml::events::Event::Start(_)
                | quick_xml::events::Event::Empty(_)
                | quick_xml::events::Event::Eof,
            ) => break,
            Err(error) => {
                return Err(crate::ParseError::Xml {
                    position: reader.buffer_position(),
                    message: error.to_string(),
                }
                .into());
            }
            _ => {}
        }
    }
    let parsed = crate::parse(source)?;
    let elements = source_elements(source)?;
    let original = OriginalSource {
        bytes: source.as_bytes().to_vec(),
        sha256: format!("{:x}", Sha256::digest(source.as_bytes())),
    };
    parsed
        .recipes
        .iter()
        .enumerate()
        .filter(|(index, _)| selected.is_none_or(|selected| selected == *index))
        .map(|(recipe_index, recipe)| {
            let mut result = from_legacy(recipe)?;
            // Legacy backfill embedded parsed records in native extension fields.
            // New imports retain those records only in immutable provenance, so
            // later native edits cannot leave a second editable copy behind.
            for row in &mut result.document.other_ingredients {
                row.specification.extensions.remove("beerxml");
            }
            for step in &mut result.document.process.mash_steps {
                step.extensions.remove("beerxml");
            }
            let recipe_path = format!("/RECIPE[{}]", recipe_index + 1);
            result.source_elements = elements
                .iter()
                .filter(|element| {
                    !element.path.contains("/RECIPE[") || element.path.contains(&recipe_path)
                })
                .cloned()
                .collect();
            if let Some(root) = result.source_elements.first() {
                if root.path != "/RECIPES[1]" && root.path != "/RECIPE[1]" {
                    result.warnings.push(warning("source_envelope_unrecognised", &root.path, "The source uses a nonstandard envelope that is preserved with the original source"));
                }
            }
            result.choices = Some(choices.clone());
            result.source_recipe_index = Some(recipe_index);
            preserved_extensions(&recipe.header, "recipe", &mut result.warnings);
            preserved_fields(&recipe.header, "recipe", &["NAME", "VERSION", "BATCH_SIZE", "BOIL_TIME", "NOTES", "EFFICIENCY"], &mut result.warnings);
            for (index, row) in recipe.fermentables.iter().enumerate() {
                preserved_extensions(
                    row,
                    &format!("fermentables.fermentable-{}", index + 1),
                    &mut result.warnings,
                );
                preserved_fields(row, &format!("fermentables.fermentable-{}", index + 1), &["NAME", "VERSION", "TYPE", "AMOUNT", "YIELD", "COLOR", "SUPPLIER"], &mut result.warnings);
            }
            for (index, row) in recipe.hops.iter().enumerate() {
                preserved_extensions(
                    row,
                    &format!("hops.hop-{}", index + 1),
                    &mut result.warnings,
                );
                preserved_fields(row, &format!("hops.hop-{}", index + 1), &["NAME", "VERSION", "ALPHA", "AMOUNT", "USE", "TIME"], &mut result.warnings);
            }
            for (index, row) in recipe.yeasts.iter().enumerate() {
                preserved_extensions(
                    row,
                    &format!("yeast.yeast-{}", index + 1),
                    &mut result.warnings,
                );
                preserved_fields(row, &format!("yeast.yeast-{}", index + 1), &["NAME", "VERSION", "AMOUNT", "AMOUNT_IS_WEIGHT", "LABORATORY", "ATTENUATION"], &mut result.warnings);
            }
            for (index, row) in recipe.miscs.iter().enumerate() {
                preserved_extensions(
                    row,
                    &format!("other_ingredients.misc-{}", index + 1),
                    &mut result.warnings,
                );
                preserved_fields(row, &format!("other_ingredients.misc-{}", index + 1), &["NAME", "VERSION", "AMOUNT", "AMOUNT_IS_WEIGHT"], &mut result.warnings);
            }
            for (index, row) in recipe.waters.iter().enumerate() {
                preserved_extensions(
                    row,
                    &format!("source.water-{}", index + 1),
                    &mut result.warnings,
                );
                preserved_fields(row, &format!("source.water-{}", index + 1), &[], &mut result.warnings);
            }
            if let Some(style) = &recipe.style {
                preserved_extensions(style, "source.style", &mut result.warnings);
                preserved_fields(style, "source.style", &["NAME", "VERSION", "CATEGORY_NUMBER", "STYLE_LETTER", "STYLE_GUIDE"], &mut result.warnings);
            }
            if let Some(equipment) = &recipe.equipment {
                preserved_extensions(equipment, "source.equipment", &mut result.warnings);
                preserved_fields(equipment, "source.equipment", &[], &mut result.warnings);
            }
            if let Some(mash) = &recipe.mash {
                preserved_extensions(&mash.header, "source.mash", &mut result.warnings);
                preserved_fields(&mash.header, "source.mash", &[], &mut result.warnings);
                for (index, step) in mash.steps.iter().enumerate() {
                    preserved_extensions(
                        step,
                        &format!("process.mash-{}", index + 1),
                        &mut result.warnings,
                    );
                    preserved_fields(step, &format!("process.mash-{}", index + 1), &["NAME", "VERSION", "STEP_TIME", "STEP_TEMP", "INFUSE_AMOUNT"], &mut result.warnings);
                }
            }
            for element in &parsed.extensions {
                result.warnings.push(warning(
                    "source_envelope_preserved",
                    &format!("source/{}", element.name),
                    "Unmapped envelope data is preserved with the original source",
                ));
            }
            for element in &result.source_elements {
                let tag = &source[element.start as usize..element.open_end as usize];
                if tag
                    .trim_end_matches(['>', '/'])
                    .trim()
                    .split_whitespace()
                    .nth(1)
                    .is_some()
                {
                    result.warnings.push(warning(
                        "source_attributes_preserved",
                        &element.path,
                        "Source attributes are preserved with the original source",
                    ));
                }
                let segment = element.path.rsplit('/').next().unwrap_or_default();
                if let Some((name, index)) = segment.rsplit_once('[') {
                    let index = index.trim_end_matches(']').parse::<usize>().unwrap_or(1);
                    if index > 1 && !matches!(name, "RECIPE" | "HOP" | "FERMENTABLE" | "YEAST" | "MISC" | "WATER" | "MASH_STEP") {
                        result.warnings.push(warning("source_duplicate_element", &element.path, "Repeated source elements are preserved with the original source; native mapping may use only one"));
                    }
                }
            }
            if let Some(choice) = choices.efficiency {
                if choice == EfficiencyChoice::RecipeOverride {
                    result
                        .document
                        .equipment_overrides
                        .efficiency_override_fraction =
                        exact_number(recipe.header.efficiency.as_ref()).map(|value| value / 100.0);
                }
                if choice == EfficiencyChoice::SelectedEquipment && recipe.header.efficiency.is_some() {
                    result.warnings.push(warning("source_field_preserved", "recipe/EFFICIENCY", "Source efficiency is retained in the original while selected equipment supplies the calculation"));
                }
                result
                    .warnings
                    .retain(|w| w.code != "efficiency_choice_required");
            }
            if let Some(unit) = choices.colour {
                for (index, (row, original)) in result
                    .document
                    .fermentables
                    .iter_mut()
                    .zip(&recipe.fermentables)
                    .enumerate()
                {
                    let converted = exact_number(original.color.as_ref()).map(|value| match unit {
                        ColourUnit::Srm => value * 1.97,
                        ColourUnit::Lovibond if value == 0.0 => 0.0,
                        ColourUnit::Lovibond => (value * 1.3546 - 0.76) * 1.97,
                    });
                    if converted.is_some_and(|v| !v.is_finite()) {
                        result.warnings.push(warning(
                            "source_number_unmapped",
                            &format!("fermentables[{index}].specification.colour_ebc"),
                            "Source colour cannot be represented after conversion",
                        ));
                    }
                    row.specification.colour_ebc = converted.filter(|v| v.is_finite());
                }
                result
                    .warnings
                    .retain(|w| w.code != "colour_interpretation_required");
            }
            result.original_source = Some(original.clone());
            result
                .warnings
                .retain(|warning| warning.code != "original_bytes_unavailable");
            Ok(result)
        })
        .collect()
}

fn warning(code: &str, path: &str, message: &str) -> MappingDiagnostic {
    MappingDiagnostic {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

// Legacy scalar parsing accepts a leading number for display-oriented fields.
// Native inputs require the complete source value so `1.2.3` cannot become 1.2.
fn exact_number(value: Option<&crate::record::Scalar<f64>>) -> Option<f64> {
    value?
        .raw()
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
}

fn note_unmapped_number(
    value: Option<&crate::record::Scalar<f64>>,
    path: &str,
    warnings: &mut Vec<MappingDiagnostic>,
) {
    if value.is_some() && exact_number(value).is_none() {
        warnings.push(warning("source_number_unmapped", path, "Source value is not an unambiguous finite number and is preserved with the original source"));
    }
}

fn scaled_number(
    value: Option<&crate::record::Scalar<f64>>,
    scale: f64,
    path: &str,
    warnings: &mut Vec<MappingDiagnostic>,
) -> Option<f64> {
    let result = exact_number(value)? * scale;
    if result.is_finite() {
        Some(result)
    } else {
        warnings.push(warning(
            "source_number_unmapped",
            path,
            "Source value cannot be represented after unit conversion",
        ));
        None
    }
}

fn preserved_extensions<R: crate::record::Record>(
    record: &R,
    path: &str,
    warnings: &mut Vec<MappingDiagnostic>,
) {
    for item in record.extensions() {
        warnings.push(warning(
            "source_extension_preserved",
            &format!("{path}/{}", item.name),
            "Unmapped source data is preserved with the original source",
        ));
    }
}

fn preserved_fields<R: crate::record::Record>(
    record: &R,
    path: &str,
    mapped: &[&str],
    warnings: &mut Vec<MappingDiagnostic>,
) {
    for (tag, value) in record.scalars() {
        if tag == "VERSION" && value.trim() != "1" && value.trim() != "1.0" {
            warnings.push(warning(
                "source_version_preserved",
                &format!("{path}/VERSION"),
                "A source record version differs from the exported BeerXML version",
            ));
        }
        if !mapped.contains(&tag) {
            warnings.push(warning(
                "source_field_preserved",
                &format!("{path}/{tag}"),
                "A source field is retained in the original but is not a native input",
            ));
        }
    }
}

/// An edited export is generated from native data; diagnostics identify data
/// this projection did not emit. Original-file retrieval is a separate action.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NativeExport {
    pub xml: String,
    pub diagnostics: Vec<MappingDiagnostic>,
}

/// Build a BeerXML projection without copying stale fields from an import.
pub fn to_xml(document: &RecipeDocumentV1) -> NativeExport {
    use crate::record::Scalar;
    let mut recipe = crate::Recipe::default();
    recipe.header.version = Some(Scalar::from_value(1));
    recipe.header.name = Some(Scalar::from_value(document.title.clone()));
    recipe.header.batch_size = document.batch_size_l.map(Scalar::from_value);
    recipe.header.boil_time = document.boil_minutes.map(Scalar::from_value);
    recipe.header.notes = document.notes.clone().map(Scalar::from_value);
    recipe.header.efficiency = document
        .equipment_overrides
        .efficiency_override_fraction
        .map(|v| Scalar::from_value(v * 100.0));
    if let Some(style) = &document.style {
        let mut record = crate::Style::default();
        record.version = Some(Scalar::from_value(1));
        record.name = Some(Scalar::from_value(style.name.clone()));
        record.style_guide = style.edition.clone().map(Scalar::from_value);
        if let Some(id) = &style.style_id {
            let prefix = id.chars().take_while(|c| c.is_ascii_digit()).count();
            if prefix > 0
                && id[prefix..].len() == 1
                && id[prefix..].chars().all(|c| c.is_ascii_alphabetic())
            {
                record.category_number = Some(Scalar::from_value(id[..prefix].to_owned()));
                record.style_letter = Some(Scalar::from_value(id[prefix..].to_owned()));
            }
        }
        recipe.style = Some(record);
    }
    for row in &document.fermentables {
        let mut record = crate::Fermentable::default();
        record.version = Some(Scalar::from_value(1));
        record.name = row.specification.name.clone().map(Scalar::from_value);
        record.supplier = row.specification.producer.clone().map(Scalar::from_value);
        record.type_ = row.specification.kind.clone().map(Scalar::from_value);
        record.amount = row.amount_kg.map(Scalar::from_value);
        record.yield_ = row
            .specification
            .yield_fraction
            .map(|v| Scalar::from_value(v * 100.0));
        record.color = row
            .specification
            .colour_ebc
            .map(|v| Scalar::from_value(v / 1.97));
        record.display_color = row
            .specification
            .colour_ebc
            .map(|v| Scalar::from_value(format!("{} SRM", v / 1.97)));
        recipe.fermentables.push(record);
    }
    for row in &document.hops {
        let mut record = crate::Hop::default();
        record.version = Some(Scalar::from_value(1));
        record.name = row.specification.name.clone().map(Scalar::from_value);
        record.alpha = row
            .specification
            .alpha_fraction
            .map(|v| Scalar::from_value(v * 100.0));
        record.amount = row.amount_g.map(|v| Scalar::from_value(v / 1000.0));
        let use_name = match row.stage.as_str() {
            "boil" => Some("Boil"),
            "dry_hop" => Some("Dry Hop"),
            _ => None,
        };
        record.use_ = use_name.map(|value| Scalar::from_value(value.to_owned()));
        if use_name.is_some() {
            record.time = row.contact_minutes.map(|v| {
                Scalar::from_value(if row.stage == "dry_hop" {
                    v / 1440.0
                } else {
                    v
                })
            });
        }
        recipe.hops.push(record);
    }
    for row in &document.yeast {
        let mut record = crate::Yeast::default();
        record.version = Some(Scalar::from_value(1));
        record.name = row.specification.name.clone().map(Scalar::from_value);
        record.laboratory = row.specification.producer.clone().map(Scalar::from_value);
        record.attenuation = row
            .specification
            .attenuation_fraction
            .map(|v| Scalar::from_value(v * 100.0));
        if matches!(row.unit.as_deref(), Some("kg" | "l")) {
            record.amount = row.amount.map(Scalar::from_value);
            record.amount_is_weight = Some(Scalar::from_value(row.unit.as_deref() == Some("kg")));
        }
        recipe.yeasts.push(record);
    }
    for row in &document.other_ingredients {
        let mut record = crate::Misc::default();
        record.version = Some(Scalar::from_value(1));
        record.name = row.specification.name.clone().map(Scalar::from_value);
        if matches!(row.unit.as_deref(), Some("kg" | "l")) {
            record.amount = row.amount.map(Scalar::from_value);
            record.amount_is_weight = Some(Scalar::from_value(row.unit.as_deref() == Some("kg")));
        }
        recipe.miscs.push(record);
    }
    if !document.process.mash_steps.is_empty() {
        let mut mash = crate::Mash::default();
        mash.header.version = Some(Scalar::from_value(1));
        for step in &document.process.mash_steps {
            let mut record = crate::MashStep::default();
            record.version = Some(Scalar::from_value(1));
            record.name = step.name.clone().map(Scalar::from_value);
            record.step_time = step.time_minutes.map(Scalar::from_value);
            record.step_temp = step.temperature_c.map(Scalar::from_value);
            record.infuse_amount = step.volume_l.map(Scalar::from_value);
            mash.steps.push(record);
        }
        recipe.mash = Some(mash);
    }
    let mut diagnostics = Vec::new();
    // Unsupported fields are called out rather than reconstructed from old XML.
    if document.import_provenance_id.is_some() {
        diagnostics.push(warning(
            "source_fragments_not_emitted",
            "source",
            "Some original source information may be omitted; the unedited original remains available",
        ));
    }
    for row in &document.fermentables {
        if row.addition.stage != "unspecified"
            || row.addition.time_minutes.is_some()
            || row.source.is_some()
        {
            diagnostics.push(warning(
                "native_fields_not_emitted",
                &format!("fermentables.{}", row.row_id),
                "Native addition stage, timing or catalogue source is not represented",
            ));
        }
    }
    for row in &document.hops {
        if !matches!(row.stage.as_str(), "boil" | "dry_hop") {
            diagnostics.push(warning(
                "unsupported_hop_stage",
                &format!("hops.{}.stage", row.row_id),
                "Hop stage and timing cannot be represented without changing their meaning",
            ));
        }
        if row.temperature_c.is_some()
            || row.source.is_some()
            || row.specification.producer.is_some()
        {
            diagnostics.push(warning(
                "native_fields_not_emitted",
                &format!("hops.{}", row.row_id),
                "Native temperature, producer or catalogue source is not represented",
            ));
        }
    }
    if document.style.as_ref().is_some_and(|s| {
        s.style_id.as_ref().is_some_and(|id| {
            let prefix = id.chars().take_while(|c| c.is_ascii_digit()).count();
            prefix == 0
                || id[prefix..].len() != 1
                || !id[prefix..].chars().all(|c| c.is_ascii_alphabetic())
        })
    }) {
        diagnostics.push(warning(
            "native_fields_not_emitted",
            "style.style_id",
            "The native style identifier is not represented",
        ));
    }
    if document.description.is_some() {
        diagnostics.push(warning(
            "native_fields_not_emitted",
            "description",
            "Native description is not represented",
        ));
    }
    for row in &document.yeast {
        if row.source.is_some()
            || !row.specification.extensions.is_empty()
            || (row.amount.is_some() && !matches!(row.unit.as_deref(), Some("kg" | "l")))
        {
            diagnostics.push(warning(
                "native_fields_not_emitted",
                &format!("yeast.{}", row.row_id),
                "Yeast source, extensions or unsupported amount unit is not represented",
            ));
        }
    }
    for row in &document.other_ingredients {
        if row.source.is_some()
            || row.specification.producer.is_some()
            || row.specification.attenuation_fraction.is_some()
            || !row.specification.extensions.is_empty()
            || (row.amount.is_some() && !matches!(row.unit.as_deref(), Some("kg" | "l")))
        {
            diagnostics.push(warning(
                "native_fields_not_emitted",
                &format!("other_ingredients.{}", row.row_id),
                "Other ingredient details or unsupported amount unit are not represented",
            ));
        }
    }
    for row in &document.process.mash_steps {
        if !row.extensions.is_empty() {
            diagnostics.push(warning(
                "native_fields_not_emitted",
                &format!("process.mash_steps.{}", row.row_id),
                "Mash step extensions are not represented",
            ));
        }
    }
    for row in &document.process.fermentation_steps {
        diagnostics.push(warning(
            "native_fields_not_emitted",
            &format!("process.fermentation_steps.{}", row.row_id),
            "Fermentation step is not represented",
        ));
    }
    let process = &document.process;
    if process.apparent_attenuation_fraction.is_some()
        || !process.fermentation_steps.is_empty()
        || process.whirlpool_minutes.is_some()
        || process.whirlpool_temperature_c.is_some()
        || process.mash_water_l.is_some()
        || process.mash_water_method.is_some()
        || process.kettle_top_up_l.is_some()
        || !process.unsupported.is_empty()
    {
        diagnostics.push(warning(
            "native_fields_not_emitted",
            "process",
            "Native process settings are not represented",
        ));
    }
    if !document.extensions.is_empty() {
        diagnostics.push(warning(
            "native_fields_not_emitted",
            "extensions",
            "Native extensions are not represented",
        ));
    }
    let mut unmapped_equipment = document.equipment_overrides.clone();
    unmapped_equipment.efficiency_override_fraction = None;
    if document.equipment_profile_id.is_some() || unmapped_equipment != Default::default() {
        diagnostics.push(warning(
            "native_fields_not_emitted",
            "equipment",
            "Some equipment identity or settings are not represented",
        ));
    }
    NativeExport {
        xml: crate::write_recipe_document(&recipe),
        diagnostics,
    }
}

/// Carry an unknown row fragment forward only while its imported native row is
/// unchanged and its original XML had no attributes that the writer would lose.
/// Other source data remains available through the original attachment.
pub fn to_xml_with_source(
    document: &RecipeDocumentV1,
    source: &str,
    recipe_index: usize,
    choices: &ImportChoices,
) -> NativeExport {
    use crate::record::Record;
    fn safe_fragment(fragment: &crate::RawElement) -> bool {
        let mut characters = fragment.name.chars();
        characters
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && characters.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
            // The structural parser trims text and cannot prove whitespace
            // inside an unknown container is insignificant.
            && fragment.children.is_empty()
    }
    fn tag_has_attributes(tag: &str) -> bool {
        let mut reader = quick_xml::Reader::from_str(tag);
        match reader.read_event() {
            Ok(quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e)) => {
                let has_attributes = e.attributes().next().is_some();
                has_attributes
            }
            _ => true,
        }
    }
    fn child_name<'a>(element: &'a SourceElement, parent: &str) -> Option<&'a str> {
        let (path, segment) = element.path.rsplit_once('/')?;
        if path != parent {
            return None;
        }
        Some(segment.rsplit_once('[')?.0)
    }
    fn row_order_safe<R: Record>(
        elements: &[SourceElement],
        recipe_path: &str,
        row_id: &str,
    ) -> bool {
        let mut roots = elements.iter().filter(|element| {
            element.row_id.as_deref() == Some(row_id)
                && element.path.contains(recipe_path)
                && element
                    .path
                    .rsplit('/')
                    .next()
                    .is_some_and(|segment| segment.starts_with(&format!("{}[", R::TAG)))
        });
        let Some(root) = roots.next() else {
            return false;
        };
        if roots.next().is_some() {
            return false;
        }
        let mut extension_seen = false;
        for name in elements
            .iter()
            .filter_map(|element| child_name(element, &root.path))
        {
            if R::default().set_scalar(name, String::new()) {
                if extension_seen {
                    return false;
                }
            } else {
                extension_seen = true;
            }
        }
        true
    }
    let mut output = to_xml(document);
    let Ok(source_document) = crate::parse(source) else {
        return output;
    };
    let Some(source_recipe) = source_document.recipes.get(recipe_index) else {
        return output;
    };
    let Ok(Some(baseline)) = from_xml_at(source, choices, recipe_index) else {
        return output;
    };
    for (name, value) in [
        (
            "original_gravity",
            &baseline.source_estimates.original_gravity,
        ),
        ("final_gravity", &baseline.source_estimates.final_gravity),
        ("colour", &baseline.source_estimates.colour),
        ("bitterness", &baseline.source_estimates.bitterness),
        ("alcohol", &baseline.source_estimates.alcohol),
    ] {
        if value.is_some() {
            output.diagnostics.push(warning(
                "source_estimate_omitted",
                &format!("source.estimates.{name}"),
                "A source application's estimate is not emitted as a current native value",
            ));
        }
    }
    let Ok(mut projected) = crate::parse(&output.xml) else {
        return output;
    };
    let Some(recipe) = projected.recipes.first_mut() else {
        return output;
    };
    let recipe_path = format!("/RECIPE[{}]", recipe_index + 1);
    let recipe_root = baseline
        .source_elements
        .iter()
        .find(|element| element.path == recipe_path || element.path.ends_with(&recipe_path))
        .map(|element| element.path.as_str());
    let header_order_safe = recipe_root.is_some_and(|root| {
        let mut extension_seen = false;
        let mut structured_seen = false;
        for name in baseline
            .source_elements
            .iter()
            .filter_map(|element| child_name(element, root))
        {
            if matches!(
                name,
                "STYLE"
                    | "EQUIPMENT"
                    | "MASH"
                    | "HOPS"
                    | "FERMENTABLES"
                    | "YEASTS"
                    | "MISCS"
                    | "WATERS"
            ) {
                structured_seen = true;
            } else if crate::RecipeHeader::default().set_scalar(name, String::new()) {
                if extension_seen || structured_seen {
                    return false;
                }
            } else {
                if structured_seen {
                    return false;
                }
                extension_seen = true;
            }
        }
        true
    });
    let attributed_ancestor = baseline.source_elements.iter().any(|element| {
        element.row_id.is_none()
            && tag_has_attributes(&source[element.start as usize..element.open_end as usize])
    });
    let has_attributes = |row: &str| {
        source.contains("<!DOCTYPE")
            || attributed_ancestor
            || baseline
                .source_elements
                .iter()
                .filter(|element| {
                    element.path.contains(&recipe_path) && element.row_id.as_deref() == Some(row)
                })
                .any(|element| {
                    let tag = &source[element.start as usize..element.open_end as usize];
                    tag_has_attributes(tag)
                })
    };
    let leaf_text_preserved = |row_id: Option<&str>, fragment: &crate::RawElement| {
        let mut elements = baseline.source_elements.iter().filter(|element| {
            element.row_id.as_deref() == row_id
                && element
                    .path
                    .rsplit('/')
                    .next()
                    .and_then(|segment| segment.rsplit_once('['))
                    .is_some_and(|(name, _)| name == fragment.name)
        });
        let Some(element) = elements.next() else {
            return false;
        };
        if elements.next().is_some() {
            return false;
        }
        let opening = &source[element.start as usize..element.open_end as usize];
        if opening.ends_with("/>") {
            return true;
        }
        let remainder = &source[element.open_end as usize..];
        let Some(end) = remainder.find('<') else {
            return false;
        };
        let text = &remainder[..end];
        text == text.trim() && remainder[end..].starts_with("</")
    };
    let mut baseline_document = baseline.document.clone();
    baseline_document.import_provenance_id = document.import_provenance_id.clone();
    let unchanged_source = baseline_document == *document
        && source_document.recipes.len() == 1
        && baseline
            .source_elements
            .first()
            .is_some_and(|root| root.path == "/RECIPES[1]" || root.path == "/RECIPE[1]")
        && !source.contains("<!DOCTYPE")
        && baseline.source_elements.iter().all(|element| {
            !tag_has_attributes(&source[element.start as usize..element.open_end as usize])
        });
    let mut envelope_fragments = Vec::new();
    for fragment in &source_document.extensions {
        // The writer emits envelope extensions after the recipe. Preserve an
        // unknown fragment only if this keeps its relative source position.
        let recipe_start = baseline
            .source_elements
            .iter()
            .find(|e| e.path == recipe_path || e.path.ends_with(&recipe_path))
            .map(|e| e.start);
        let root_path = baseline
            .source_elements
            .first()
            .map(|e| e.path.as_str())
            .unwrap_or_default();
        let fragment_path = format!("{root_path}/{}[1]", fragment.name);
        let fragment_after_recipe = recipe_start.is_some_and(|start| {
            baseline
                .source_elements
                .iter()
                .any(|e| e.path == fragment_path && e.start > start)
        });
        if unchanged_source
            && safe_fragment(fragment)
            && leaf_text_preserved(None, fragment)
            && fragment_after_recipe
        {
            envelope_fragments.push(fragment.clone());
        } else {
            output.diagnostics.push(warning(
                "source_fragment_omitted",
                &format!("source/{}", fragment.name),
                "An unmapped envelope fragment is not included in the edited export",
            ));
        }
    }
    for fragment in source_recipe.header.extensions() {
        let reserved = matches!(
            fragment.name.as_str(),
            "STYLE"
                | "EQUIPMENT"
                | "MASH"
                | "HOPS"
                | "FERMENTABLES"
                | "YEASTS"
                | "MISCS"
                | "WATERS"
        );
        if unchanged_source
            && header_order_safe
            && safe_fragment(fragment)
            && leaf_text_preserved(None, fragment)
            && !reserved
            && !crate::RecipeHeader::default().set_scalar(&fragment.name, String::new())
        {
            recipe.header.extensions.push(fragment.clone());
        } else {
            output.diagnostics.push(warning(
                "source_fragment_omitted",
                &format!("source/{}", fragment.name),
                "An unmapped recipe fragment is not included in the edited export",
            ));
        }
    }
    for diagnostic in &baseline.warnings {
        if diagnostic.code == "source_version_preserved" {
            output.diagnostics.push(warning(
                "source_version_omitted",
                &diagnostic.path,
                "The source record version differs from the exported version",
            ));
        }
        if diagnostic.code == "source_field_preserved" {
            output.diagnostics.push(warning("source_field_omitted", &diagnostic.path, "A source field without a current native mapping is not included in the edited export"));
        }
        if diagnostic.code == "source_extension_preserved"
            && !diagnostic.path.starts_with("recipe/")
            && !diagnostic.path.starts_with("hops.")
            && !diagnostic.path.starts_with("fermentables.")
        {
            output.diagnostics.push(warning(
                "source_fragment_omitted",
                &diagnostic.path,
                "An unmapped source fragment is not included in the edited export",
            ));
        }
    }
    for element in &baseline.source_elements {
        let tag = &source[element.start as usize..element.open_end as usize];
        let attributes = tag_has_attributes(tag);
        if attributes {
            output.diagnostics.push(warning(
                "source_attributes_omitted",
                &element.path,
                "Source XML attributes are not included in the edited export",
            ));
        }
    }
    for (index, row) in document.fermentables.iter().enumerate() {
        if let Some((old_index, old)) = baseline
            .document
            .fermentables
            .iter()
            .enumerate()
            .find(|(_, old)| old.row_id == row.row_id)
        {
            let original = &source_recipe.fermentables[old_index];
            if original.extensions().is_empty() {
                continue;
            }
            if row == old
                && !has_attributes(&row.row_id)
                && row_order_safe::<crate::Fermentable>(
                    &baseline.source_elements,
                    &recipe_path,
                    &row.row_id,
                )
            {
                for fragment in original.extensions() {
                    if safe_fragment(fragment)
                        && leaf_text_preserved(Some(&row.row_id), fragment)
                        && !crate::Fermentable::default().set_scalar(&fragment.name, String::new())
                    {
                        recipe.fermentables[index].extensions.push(fragment.clone());
                    } else {
                        output.diagnostics.push(warning(
                            "source_fragment_omitted",
                            &format!("fermentables.{}/{}", row.row_id, fragment.name),
                            "A source fragment conflicts with a current native field",
                        ));
                    }
                }
            } else {
                for fragment in original.extensions() {
                    output.diagnostics.push(warning(
                        "source_fragment_omitted",
                        &format!("fermentables.{}/{}", row.row_id, fragment.name),
                        "The source fragment no longer has an unchanged safe mapping",
                    ));
                }
            }
        }
    }
    for (index, row) in document.hops.iter().enumerate() {
        if let Some((old_index, old)) = baseline
            .document
            .hops
            .iter()
            .enumerate()
            .find(|(_, old)| old.row_id == row.row_id)
        {
            let original = &source_recipe.hops[old_index];
            if original.extensions().is_empty() {
                continue;
            }
            if row == old
                && !has_attributes(&row.row_id)
                && row_order_safe::<crate::Hop>(
                    &baseline.source_elements,
                    &recipe_path,
                    &row.row_id,
                )
            {
                for fragment in original.extensions() {
                    if safe_fragment(fragment)
                        && leaf_text_preserved(Some(&row.row_id), fragment)
                        && !crate::Hop::default().set_scalar(&fragment.name, String::new())
                    {
                        recipe.hops[index].extensions.push(fragment.clone());
                    } else {
                        output.diagnostics.push(warning(
                            "source_fragment_omitted",
                            &format!("hops.{}/{}", row.row_id, fragment.name),
                            "A source fragment conflicts with a current native field",
                        ));
                    }
                }
            } else {
                for fragment in original.extensions() {
                    output.diagnostics.push(warning(
                        "source_fragment_omitted",
                        &format!("hops.{}/{}", row.row_id, fragment.name),
                        "The source fragment no longer has an unchanged safe mapping",
                    ));
                }
            }
        }
    }
    for (index, row) in baseline.document.fermentables.iter().enumerate() {
        if !document
            .fermentables
            .iter()
            .any(|current| current.row_id == row.row_id)
        {
            for fragment in source_recipe.fermentables[index].extensions() {
                output.diagnostics.push(warning(
                    "source_fragment_omitted",
                    &format!("fermentables.{}/{}", row.row_id, fragment.name),
                    "The imported row was removed",
                ));
            }
        }
    }
    for (index, row) in baseline.document.hops.iter().enumerate() {
        if !document
            .hops
            .iter()
            .any(|current| current.row_id == row.row_id)
        {
            for fragment in source_recipe.hops[index].extensions() {
                output.diagnostics.push(warning(
                    "source_fragment_omitted",
                    &format!("hops.{}/{}", row.row_id, fragment.name),
                    "The imported row was removed",
                ));
            }
        }
    }
    projected.extensions.extend(envelope_fragments);
    output.xml = crate::write(&projected);
    output
}

/// Project known unambiguous fields while retaining the complete legacy record.
pub fn from_legacy(recipe: &crate::Recipe) -> Result<NativeMigration, serde_json::Error> {
    let mut document = RecipeDocumentV1 {
        title: recipe.name().to_owned(),
        batch_size_l: exact_number(recipe.header.batch_size.as_ref()),
        boil_minutes: exact_number(recipe.header.boil_time.as_ref()),
        notes: recipe.header.notes().cloned(),
        style: recipe.style.as_ref().and_then(|style| {
            style.name().map(|name| StyleReference {
                name: name.clone(),
                edition: style.style_guide().cloned(),
                style_id: style
                    .category_number()
                    .zip(style.style_letter())
                    .map(|(number, letter)| format!("{number}{letter}")),
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
    note_unmapped_number(
        recipe.header.batch_size.as_ref(),
        "batch_size_l",
        &mut warnings,
    );
    note_unmapped_number(
        recipe.header.boil_time.as_ref(),
        "boil_minutes",
        &mut warnings,
    );
    note_unmapped_number(
        recipe.header.efficiency.as_ref(),
        "equipment_overrides.efficiency_override_fraction",
        &mut warnings,
    );
    if exact_number(recipe.header.efficiency.as_ref()).is_some() {
        warnings.push(warning(
            "efficiency_choice_required",
            "equipment_overrides.efficiency_override_fraction",
            "Source efficiency retained without selecting an override",
        ));
    }
    for (index, item) in recipe.fermentables.iter().enumerate() {
        note_unmapped_number(
            item.amount.as_ref(),
            &format!("fermentables[{index}].amount_kg"),
            &mut warnings,
        );
        note_unmapped_number(
            item.yield_.as_ref(),
            &format!("fermentables[{index}].specification.yield_fraction"),
            &mut warnings,
        );
        note_unmapped_number(
            item.color.as_ref(),
            &format!("fermentables[{index}].specification.colour_ebc"),
            &mut warnings,
        );
        if item
            .add_after_boil
            .as_ref()
            .is_some_and(|v| v.value().is_none())
        {
            warnings.push(warning(
                "source_stage_unmapped",
                &format!("fermentables[{index}].addition.stage"),
                "Source addition timing is unrecognised and is preserved with the original source",
            ));
        }
        document.fermentables.push(FermentableAddition {
            row_id: format!("fermentable-{}", index + 1),
            amount_kg: exact_number(item.amount.as_ref()),
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
                kind: item
                    .type_()
                    .map(|kind| match kind.to_ascii_lowercase().as_str() {
                        "grain" => "grain".into(),
                        "extract" | "dry extract" => "extract".into(),
                        "sugar" => "sugar".into(),
                        _ => kind.clone(),
                    }),
                yield_fraction: exact_number(item.yield_.as_ref()).map(|v| v / 100.0),
                colour_ebc: None,
            },
            source: None,
        });
        if exact_number(item.color.as_ref()).is_some() {
            warnings.push(warning(
                "colour_interpretation_required",
                &format!("fermentables[{index}].specification.colour_ebc"),
                "Source colour requires an explicit interpretation",
            ));
        }
    }
    for (index, item) in recipe.hops.iter().enumerate() {
        note_unmapped_number(
            item.amount.as_ref(),
            &format!("hops[{index}].amount_g"),
            &mut warnings,
        );
        note_unmapped_number(
            item.alpha.as_ref(),
            &format!("hops[{index}].specification.alpha_fraction"),
            &mut warnings,
        );
        note_unmapped_number(
            item.time.as_ref(),
            &format!("hops[{index}].contact_minutes"),
            &mut warnings,
        );
        let source_stage = item.use_().cloned().unwrap_or_else(|| "unspecified".into());
        // BeerXML dry-hop time is in days, unlike boil contact time in minutes.
        let contact_minutes = if source_stage.eq_ignore_ascii_case("dry hop") {
            scaled_number(
                item.time.as_ref(),
                1440.0,
                &format!("hops[{index}].contact_minutes"),
                &mut warnings,
            )
        } else if source_stage.eq_ignore_ascii_case("boil") {
            exact_number(item.time.as_ref())
        } else {
            if item.time.is_some() {
                warnings.push(warning(
                    "source_hop_timing_unmapped",
                    &format!("hops[{index}].contact_minutes"),
                    "Source timing is retained without interpreting an unsupported hop stage",
                ));
            }
            None
        };
        document.hops.push(HopAddition {
            row_id: format!("hop-{}", index + 1),
            amount_g: scaled_number(
                item.amount.as_ref(),
                1000.0,
                &format!("hops[{index}].amount_g"),
                &mut warnings,
            ),
            stage: match source_stage.to_ascii_lowercase().as_str() {
                "boil" => "boil".into(),
                "dry hop" => "dry_hop".into(),
                _ => source_stage,
            },
            contact_minutes,
            temperature_c: None,
            specification: HopSpecification {
                name: item.name().cloned(),
                producer: None,
                alpha_fraction: exact_number(item.alpha.as_ref()).map(|v| v / 100.0),
            },
            source: None,
        });
    }
    for (index, item) in recipe.yeasts.iter().enumerate() {
        note_unmapped_number(
            item.amount.as_ref(),
            &format!("yeast[{index}].amount"),
            &mut warnings,
        );
        note_unmapped_number(
            item.attenuation.as_ref(),
            &format!("yeast[{index}].specification.attenuation_fraction"),
            &mut warnings,
        );
        if item
            .amount_is_weight
            .as_ref()
            .is_some_and(|v| v.value().is_none())
        {
            warnings.push(warning(
                "source_unit_unmapped",
                &format!("yeast[{index}].unit"),
                "Source amount unit is unrecognised and is preserved with the original source",
            ));
        }
        document.yeast.push(IngredientAddition {
            row_id: format!("yeast-{}", index + 1),
            amount: exact_number(item.amount.as_ref()),
            unit: item
                .amount_is_weight()
                .map(|weight| if *weight { "kg" } else { "l" }.to_owned()),
            specification: IngredientSpecification {
                name: item.name().cloned(),
                producer: item.laboratory().cloned(),
                attenuation_fraction: exact_number(item.attenuation.as_ref()).map(|v| v / 100.0),
                extensions: Default::default(),
            },
            source: None,
        });
    }
    for (index, item) in recipe.miscs.iter().enumerate() {
        note_unmapped_number(
            item.amount.as_ref(),
            &format!("other_ingredients[{index}].amount"),
            &mut warnings,
        );
        if item
            .amount_is_weight
            .as_ref()
            .is_some_and(|v| v.value().is_none())
        {
            warnings.push(warning(
                "source_unit_unmapped",
                &format!("other_ingredients[{index}].unit"),
                "Source amount unit is unrecognised and is preserved with the original source",
            ));
        }
        let mut extensions = std::collections::BTreeMap::new();
        extensions.insert("beerxml".into(), serde_json::to_value(item)?);
        document.other_ingredients.push(IngredientAddition {
            row_id: format!("misc-{}", index + 1),
            amount: exact_number(item.amount.as_ref()),
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
            note_unmapped_number(
                step.step_time.as_ref(),
                &format!("process.mash_steps[{index}].time_minutes"),
                &mut warnings,
            );
            note_unmapped_number(
                step.step_temp.as_ref(),
                &format!("process.mash_steps[{index}].temperature_c"),
                &mut warnings,
            );
            note_unmapped_number(
                step.infuse_amount.as_ref(),
                &format!("process.mash_steps[{index}].volume_l"),
                &mut warnings,
            );
            let mut extensions = std::collections::BTreeMap::new();
            extensions.insert("beerxml".into(), serde_json::to_value(step)?);
            document.process.mash_steps.push(ProcessStep {
                row_id: format!("mash-{}", index + 1),
                name: step.name().cloned(),
                time_minutes: exact_number(step.step_time.as_ref()),
                temperature_c: exact_number(step.step_temp.as_ref()),
                volume_l: exact_number(step.infuse_amount.as_ref()),
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
        source_elements: Vec::new(),
        choices: None,
        source_recipe_index: None,
        source_estimates: krausen_types::interchange::SourceEstimates {
            original_gravity: recipe.header.est_og.as_ref().map(|v| v.raw().to_owned()),
            final_gravity: recipe.header.est_fg.as_ref().map(|v| v.raw().to_owned()),
            colour: recipe.header.est_color.as_ref().map(|v| v.raw().to_owned()),
            bitterness: recipe.header.ibu.as_ref().map(|v| v.raw().to_owned()),
            alcohol: recipe.header.est_abv.as_ref().map(|v| v.raw().to_owned()),
        },
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_choices_project_colour_and_efficiency_without_changing_source() {
        let xml = "<RECIPES><RECIPE><NAME>Choice</NAME><EFFICIENCY>75</EFFICIENCY><FERMENTABLES><FERMENTABLE><NAME>Malt</NAME><COLOR>3</COLOR></FERMENTABLE><FERMENTABLE><NAME>Missing</NAME></FERMENTABLE></FERMENTABLES></RECIPE></RECIPES>";
        let unresolved = from_xml(xml).unwrap().remove(0);
        assert!(
            unresolved
                .warnings
                .iter()
                .any(|w| w.code == "colour_interpretation_required")
        );
        assert!(
            unresolved
                .warnings
                .iter()
                .any(|w| w.code == "efficiency_choice_required")
        );
        let srm = from_xml_with_choices(
            xml,
            &ImportChoices {
                colour: Some(ColourUnit::Srm),
                efficiency: Some(EfficiencyChoice::RecipeOverride),
            },
        )
        .unwrap()
        .remove(0);
        assert_eq!(
            srm.document.fermentables[0].specification.colour_ebc,
            Some(3.0 * 1.97)
        );
        assert_eq!(srm.document.fermentables[1].specification.colour_ebc, None);
        assert_eq!(
            srm.document
                .equipment_overrides
                .efficiency_override_fraction,
            Some(0.75)
        );
        assert!(
            !srm.warnings
                .iter()
                .any(|w| w.code == "colour_interpretation_required"
                    || w.code == "efficiency_choice_required")
        );
        assert_eq!(srm.original_source.unwrap().bytes, xml.as_bytes());
        let lovibond = from_xml_with_choices(
            xml,
            &ImportChoices {
                colour: Some(ColourUnit::Lovibond),
                efficiency: Some(EfficiencyChoice::SelectedEquipment),
            },
        )
        .unwrap()
        .remove(0);
        assert_eq!(
            lovibond.document.fermentables[0].specification.colour_ebc,
            Some((3.0 * 1.3546 - 0.76) * 1.97)
        );
        assert_eq!(
            lovibond
                .document
                .equipment_overrides
                .efficiency_override_fraction,
            None
        );
        assert_eq!(lovibond.legacy_record, unresolved.legacy_record);
    }

    #[test]
    fn indexes_source_attributes_envelopes_and_stable_row_paths() {
        let xml = r#"<RECIPES vendor="outer"><META x="1"/><RECIPE><HOPS><HOP id="h"><NAME>Saaz</NAME><VENDOR flag="y"/></HOP></HOPS></RECIPE></RECIPES>"#;
        let imported = from_xml(xml).unwrap().remove(0);
        let root = &imported.source_elements[0];
        assert_eq!(
            &xml[root.start as usize..root.open_end as usize],
            r#"<RECIPES vendor="outer">"#
        );
        let vendor = imported
            .source_elements
            .iter()
            .find(|e| e.path.ends_with("/VENDOR[1]"))
            .unwrap();
        assert_eq!(vendor.row_id.as_deref(), Some("hop-1"));
        assert_eq!(
            &xml[vendor.start as usize..vendor.open_end as usize],
            r#"<VENDOR flag="y"/>"#
        );
        assert!(
            imported
                .source_elements
                .iter()
                .any(|e| e.path == "/RECIPES[1]/META[1]")
        );
    }

    #[test]
    fn repeated_source_scalars_are_disclosed_and_original_is_retained() {
        let source = "<RECIPES><RECIPE><NAME>First</NAME><NAME>Second</NAME></RECIPE></RECIPES>";
        let imported = from_xml(source).unwrap().remove(0);
        assert!(
            imported
                .warnings
                .iter()
                .any(|d| d.code == "source_duplicate_element" && d.path.ends_with("/NAME[2]"))
        );
        assert_eq!(imported.original_source.unwrap().bytes, source.as_bytes());
    }

    #[test]
    fn selected_multi_recipe_import_preserves_full_original_and_selected_path() {
        let source = "<RECIPES><RECIPE><NAME>First</NAME></RECIPE><RECIPE><NAME>Second</NAME><HOPS><HOP><NAME>Hop</NAME></HOP></HOPS></RECIPE></RECIPES>";
        let selected = from_xml_at(source, &ImportChoices::default(), 1)
            .unwrap()
            .unwrap();
        assert_eq!(selected.document.title, "Second");
        assert_eq!(selected.source_recipe_index, Some(1));
        assert_eq!(selected.original_source.unwrap().bytes, source.as_bytes());
        assert!(
            selected
                .source_elements
                .iter()
                .any(|e| e.path.contains("/RECIPE[2]"))
        );
        assert!(
            !selected
                .source_elements
                .iter()
                .any(|e| e.path.contains("/RECIPE[1]"))
        );
        assert!(
            from_xml_at(source, &ImportChoices::default(), 2)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn source_estimates_are_evidence_not_native_inputs() {
        let source = "<RECIPES><RECIPE><NAME>Test</NAME><EST_OG>1.055 SG</EST_OG><IBU>32.4 IBU</IBU></RECIPE></RECIPES>";
        let imported = from_xml(source).unwrap().remove(0);
        assert_eq!(
            imported.source_estimates.original_gravity.as_deref(),
            Some("1.055 SG")
        );
        assert_eq!(
            imported.source_estimates.bitterness.as_deref(),
            Some("32.4 IBU")
        );
        assert!(
            imported
                .warnings
                .iter()
                .any(|w| w.code == "source_field_preserved" && w.path.ends_with("/IBU"))
        );
        assert_eq!(
            imported.document.process.apparent_attenuation_fraction,
            None
        );
    }

    #[test]
    fn explicit_style_identity_survives_native_projection_and_export() {
        let source = "<RECIPES><RECIPE><NAME>Test</NAME><STYLE><NAME>Munich Helles</NAME><STYLE_GUIDE>BJCP 2021</STYLE_GUIDE><CATEGORY_NUMBER>4</CATEGORY_NUMBER><STYLE_LETTER>A</STYLE_LETTER></STYLE></RECIPE></RECIPES>";
        let document = from_xml(source).unwrap().remove(0).document;
        let style = document.style.as_ref().unwrap();
        assert_eq!(style.edition.as_deref(), Some("BJCP 2021"));
        assert_eq!(style.style_id.as_deref(), Some("4A"));
        let exported = to_xml(&document);
        let parsed = crate::parse(&exported.xml).unwrap();
        assert_eq!(
            parsed.recipes[0]
                .style
                .as_ref()
                .unwrap()
                .category_number()
                .map(String::as_str),
            Some("4")
        );
        assert_eq!(
            parsed.recipes[0]
                .style
                .as_ref()
                .unwrap()
                .style_letter()
                .map(String::as_str),
            Some("A")
        );
    }

    #[test]
    fn numeric_inputs_are_never_truncated_or_moved_between_stages() {
        let source = "<RECIPES><RECIPE><NAME>Test</NAME><BATCH_SIZE>1e1</BATCH_SIZE><HOPS><HOP><NAME>Hop</NAME><USE>Aroma</USE><TIME>2</TIME><AMOUNT>1.2.3</AMOUNT></HOP></HOPS></RECIPE></RECIPES>";
        let imported = from_xml(source).unwrap().remove(0);
        assert_eq!(imported.document.batch_size_l, Some(10.0));
        assert_eq!(imported.document.hops[0].amount_g, None);
        assert_eq!(imported.document.hops[0].contact_minutes, None);
        assert_eq!(imported.document.hops[0].stage, "Aroma");
        assert!(
            imported
                .warnings
                .iter()
                .any(|w| w.code == "source_number_unmapped")
        );
        assert!(
            imported
                .warnings
                .iter()
                .any(|w| w.code == "source_hop_timing_unmapped")
        );
        let overflow = from_xml("<RECIPES><RECIPE><HOPS><HOP><USE>Dry Hop</USE><TIME>1e308</TIME><AMOUNT>1e308</AMOUNT></HOP></HOPS></RECIPE></RECIPES>").unwrap().remove(0);
        assert_eq!(overflow.document.hops[0].amount_g, None);
        assert_eq!(overflow.document.hops[0].contact_minutes, None);
        assert_eq!(
            overflow
                .warnings
                .iter()
                .filter(|w| w.code == "source_number_unmapped")
                .count(),
            2
        );
    }

    #[test]
    fn invalid_xml_characters_are_rejected_instead_of_stripped() {
        assert!(matches!(
            from_xml("<RECIPE><NAME>bad\u{0001}</NAME></RECIPE>"),
            Err(NativeImportError::InvalidCharacters)
        ));
        assert!(!valid_xml_characters("native\u{0001}title"));
        assert!(valid_xml_characters("notes\nwith\ttabs and 😀"));
        assert!(matches!(
            from_xml("<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><RECIPE/>"),
            Err(NativeImportError::UnsupportedEncoding)
        ));
        assert!(from_xml("<?xml version=\"1.0\" encoding=\"UTF-8\"?><RECIPE/>").is_ok());
    }

    #[test]
    fn dry_hop_days_round_trip_without_turning_into_boil_minutes() {
        let source = "<RECIPES><RECIPE><HOPS><HOP><NAME>Hop</NAME><USE>Dry Hop</USE><TIME>2.5</TIME></HOP></HOPS></RECIPE></RECIPES>";
        let document = from_xml(source).unwrap().remove(0).document;
        assert_eq!(document.hops[0].stage, "dry_hop");
        assert_eq!(document.hops[0].contact_minutes, Some(3600.0));
        let output = crate::parse(&to_xml(&document).xml).unwrap();
        assert_eq!(
            output.recipes[0].hops[0].use_().map(String::as_str),
            Some("Dry Hop")
        );
        assert_eq!(output.recipes[0].hops[0].time().copied(), Some(2.5));
    }

    #[test]
    fn edited_export_uses_current_native_values_and_reports_source_omission() {
        let source = r#"<RECIPES><RECIPE><NAME>Old</NAME><HOPS><HOP><NAME>Old hop</NAME><AMOUNT>0.01</AMOUNT><USE>Boil</USE><TIME>60</TIME><VENDOR>source</VENDOR></HOP></HOPS></RECIPE></RECIPES>"#;
        let mut document = from_xml(source).unwrap().remove(0).document;
        document.import_provenance_id = Some("retained-source".into());
        document.title = "New".into();
        document.hops[0].specification.name = Some("New hop".into());
        document.hops[0].amount_g = Some(35.0);
        let export = to_xml(&document);
        let parsed = crate::parse(&export.xml).unwrap();
        assert_eq!(parsed.recipes[0].name(), "New");
        assert_eq!(
            parsed.recipes[0].hops[0].name().map(String::as_str),
            Some("New hop")
        );
        assert_eq!(parsed.recipes[0].hops[0].amount().copied(), Some(0.035));
        assert!(!export.xml.contains("<VENDOR>"));
        assert!(
            export
                .diagnostics
                .iter()
                .any(|d| d.code == "source_fragments_not_emitted")
        );
    }

    #[test]
    fn source_extensions_follow_only_unchanged_stable_rows() {
        let source = "<RECIPES><RECIPE><NAME>Test</NAME><HOPS><HOP><NAME>Saaz</NAME><USE>Boil</USE><TIME>60</TIME><VENDOR>kept</VENDOR></HOP></HOPS></RECIPE></RECIPES>";
        let choices = ImportChoices::default();
        let mut draft = from_xml_with_choices(source, &choices)
            .unwrap()
            .remove(0)
            .document;
        draft.import_provenance_id = Some("source".into());
        let same = to_xml_with_source(&draft, source, 0, &choices);
        assert!(same.xml.contains("<VENDOR>kept</VENDOR>"));
        draft.hops[0].specification.name = Some("Changed".into());
        let changed = to_xml_with_source(&draft, source, 0, &choices);
        assert!(!changed.xml.contains("<VENDOR>"));
        assert!(
            changed
                .diagnostics
                .iter()
                .any(|d| d.code == "source_fragment_omitted")
        );
    }

    #[test]
    fn extension_follows_row_identity_after_reordering() {
        let source = "<RECIPES><RECIPE><HOPS><HOP><NAME>One</NAME><USE>Boil</USE><VENDOR>one</VENDOR></HOP><HOP><NAME>Two</NAME><USE>Boil</USE><VENDOR>two</VENDOR></HOP></HOPS></RECIPE></RECIPES>";
        let choices = ImportChoices::default();
        let mut draft = from_xml_with_choices(source, &choices)
            .unwrap()
            .remove(0)
            .document;
        draft.import_provenance_id = Some("source".into());
        draft.hops.swap(0, 1);
        let output = to_xml_with_source(&draft, source, 0, &choices);
        let parsed = crate::parse(&output.xml).unwrap();
        assert_eq!(
            parsed.recipes[0].hops[0].name().map(String::as_str),
            Some("Two")
        );
        assert_eq!(
            parsed.recipes[0].hops[0].extensions[0].text.as_deref(),
            Some("two")
        );
        assert_eq!(
            parsed.recipes[0].hops[1].extensions[0].text.as_deref(),
            Some("one")
        );
    }

    #[test]
    fn unknown_fragment_with_unreproducible_order_is_omitted() {
        let source = "<RECIPES><RECIPE><HOPS><HOP><VENDOR>before</VENDOR><NAME>Hop</NAME><USE>Boil</USE></HOP></HOPS></RECIPE></RECIPES>";
        let choices = ImportChoices::default();
        let mut document = from_xml_with_choices(source, &choices)
            .unwrap()
            .remove(0)
            .document;
        document.import_provenance_id = Some("source".into());
        let output = to_xml_with_source(&document, source, 0, &choices);
        assert!(!output.xml.contains("<VENDOR>"));
        assert!(
            output
                .diagnostics
                .iter()
                .any(|d| d.code == "source_fragment_omitted")
        );
    }

    #[test]
    fn unknown_fragment_whitespace_is_not_silently_trimmed() {
        let source = "<RECIPES><RECIPE><HOPS><HOP><NAME>Hop</NAME><USE>Boil</USE><VENDOR> keep spaces </VENDOR></HOP></HOPS></RECIPE></RECIPES>";
        let choices = ImportChoices::default();
        let mut document = from_xml_with_choices(source, &choices)
            .unwrap()
            .remove(0)
            .document;
        document.import_provenance_id = Some("source".into());
        let output = to_xml_with_source(&document, source, 0, &choices);
        assert!(!output.xml.contains("<VENDOR>"));
        assert!(
            output
                .diagnostics
                .iter()
                .any(|d| d.code == "source_fragment_omitted")
        );
    }

    #[test]
    fn repeated_wrappers_do_not_reassociate_unknown_fragments() {
        let source = "<RECIPES><RECIPE><HOPS><HOP><NAME>First</NAME><USE>Boil</USE></HOP></HOPS><HOPS><HOP><NAME>Second</NAME><USE>Boil</USE><VENDOR>do not move</VENDOR></HOP></HOPS></RECIPE></RECIPES>";
        let choices = ImportChoices::default();
        let mut document = from_xml_with_choices(source, &choices)
            .unwrap()
            .remove(0)
            .document;
        document.import_provenance_id = Some("source".into());
        let output = to_xml_with_source(&document, source, 0, &choices);
        assert!(!output.xml.contains("<VENDOR>"));
        assert!(
            output
                .diagnostics
                .iter()
                .any(|d| d.code == "source_fragment_omitted")
        );
    }

    #[test]
    fn envelope_fragments_are_only_reused_for_unchanged_single_recipe() {
        let source = "<RECIPES><RECIPE><NAME>Test</NAME><VENDOR_RECIPE>recipe</VENDOR_RECIPE></RECIPE><VENDOR_ENVELOPE>file</VENDOR_ENVELOPE></RECIPES>";
        let choices = ImportChoices::default();
        let mut document = from_xml_with_choices(source, &choices)
            .unwrap()
            .remove(0)
            .document;
        document.import_provenance_id = Some("source".into());
        let unchanged = to_xml_with_source(&document, source, 0, &choices);
        assert!(
            unchanged
                .xml
                .contains("<VENDOR_ENVELOPE>file</VENDOR_ENVELOPE>")
        );
        assert!(
            unchanged
                .xml
                .contains("<VENDOR_RECIPE>recipe</VENDOR_RECIPE>")
        );
        document.title = "Edited".into();
        let changed = to_xml_with_source(&document, source, 0, &choices);
        assert!(!changed.xml.contains("<VENDOR_ENVELOPE>"));
        assert!(!changed.xml.contains("<VENDOR_RECIPE>"));
        assert!(
            changed
                .diagnostics
                .iter()
                .any(|d| d.code == "source_fragment_omitted")
        );
    }

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
        assert!(
            !migrated.document.other_ingredients[0]
                .specification
                .extensions
                .contains_key("beerxml")
        );
        assert!(
            !migrated.document.process.mash_steps[0]
                .extensions
                .contains_key("beerxml")
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
