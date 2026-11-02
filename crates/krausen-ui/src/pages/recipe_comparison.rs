// SPDX-License-Identifier: AGPL-3.0-or-later
//! Read-only named changes against the editor's original saved baseline.
use krausen_types::brewery::RecipeDocumentV1;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, PartialEq)]
pub(super) struct RecipeChange {
    pub label: String,
    pub before: String,
    pub after: String,
}

pub(super) fn changes(
    baseline: &RecipeDocumentV1,
    candidate: &RecipeDocumentV1,
    raw: &BTreeMap<String, String>,
) -> Result<Vec<RecipeChange>, serde_json::Error> {
    let before = serde_json::to_value(baseline)?;
    let after = serde_json::to_value(candidate)?;
    // Retained yeast UI text predates the native specification path. Resolve
    // that one established alias without rewriting the draft/recovery record.
    let raw = raw.iter().map(|(path, text)| {
        let canonical = if path.starts_with("yeast[") {
            path.strip_suffix(".attenuation").map(|prefix| format!("{prefix}.specification.attenuation_fraction"))
                .unwrap_or_else(|| path.clone())
        } else { path.clone() };
        (canonical, text.clone())
    }).collect::<BTreeMap<_, _>>();
    let mut result = Vec::new();
    let mut visited_raw = BTreeSet::new();
    compare(&before, &after, "", "", &raw, &mut visited_raw, &mut result);
    for (path, text) in &raw {
        if !visited_raw.contains(path) {
            let (value, label) = retained_field(&before, &after, path);
            result.push(RecipeChange { label, before: native_text(value, path), after: entered_text(text, path) });
        }
    }
    Ok(result)
}

fn native_text(value: &Value, path: &str) -> String {
    if path.ends_with("_fraction") && value.is_number() { format!("{} (fraction)", display(value)) }
    else { display(value) }
}

fn entered_text(text: &str, path: &str) -> String {
    if text.is_empty() { "Empty (entered text)".into() }
    else if path.ends_with("_fraction") { format!("{text} % (entered text)") }
    else { text.into() }
}

fn native_rows(path: &str) -> bool {
    matches!(path, "fermentables" | "hops" | "yeast" | "other_ingredients" | "process.mash_steps" | "process.fermentation_steps")
}

fn native_row(path: &str) -> bool {
    path.split_once('[').is_some_and(|(parent, tail)| native_rows(parent)
        && tail.strip_suffix(']').is_some_and(|id| !id.contains(']')))
}

fn retained_field<'a>(before: &'a Value, after: &'a Value, path: &str) -> (&'a Value, String) {
    let mut old = before;
    let mut new = after;
    let mut labels = Vec::new();
    for segment in path.split('.') {
        if let Some((key, selector)) = segment.split_once('[') {
            let id = selector.trim_end_matches(']');
            labels.push(field_label(key));
            let find = |value: &'a Value| value.get(key).and_then(Value::as_array)
                .and_then(|rows| rows.iter().enumerate().find(|(_, row)| row.get("row_id").and_then(Value::as_str) == Some(id)));
            let previous = find(old);
            let current = find(new);
            if let Some((i, row)) = current.or(previous) { labels.push(row_name(row, i)); }
            old = previous.map(|(_, row)| row).unwrap_or(&Value::Null);
            new = current.map(|(_, row)| row).unwrap_or(&Value::Null);
        } else {
            labels.push(field_label(segment));
            old = old.get(segment).unwrap_or(&Value::Null);
            new = new.get(segment).unwrap_or(&Value::Null);
        }
    }
    (old, labels.join(" / "))
}

fn display(value: &Value) -> String {
    match value {
        Value::Null => "Not supplied".into(),
        Value::String(text) if text.is_empty() => "Empty".into(),
        Value::String(text) => text.clone(),
        value => value.to_string(),
    }
}

fn field_label(key: &str) -> String {
    match key {
        "batch_size_l" => "Batch into fermenter (L)",
        "boil_minutes" => "Boil duration (min)",
        "amount_kg" => "Amount (kg)",
        "amount_g" => "Amount (g)",
        "time_minutes" | "contact_minutes" => "Duration (min)",
        "temperature_c" | "whirlpool_temperature_c" => "Temperature (°C)",
        "volume_l" => "Volume (L)",
        "equipment_overrides" => "Recipe equipment settings",
        "equipment_profile_id" => "Selected equipment profile",
        "other_ingredients" => "Other additions",
        "mash_steps" => "Mash steps",
        "fermentation_steps" => "Fermentation steps",
        "extensions" => "Preserved source data",
        "alpha_fraction" => "Alpha",
        "yield_fraction" => "Yield",
        "attenuation_fraction" => "Informational yeast attenuation",
        "apparent_attenuation_fraction" => "Wort attenuation",
        "efficiency_override_fraction" => "Recipe efficiency override",
        "mash_efficiency_fraction" => "Mash efficiency",
        "colour_ebc" => "Colour (EBC)",
        key => return key.replace('_', " "),
    }.into()
}

fn row_name(row: &Value, position: usize) -> String {
    let name = row.get("specification").and_then(|s| s.get("name"))
        .or_else(|| row.get("name")).and_then(Value::as_str)
        .filter(|name| !name.is_empty()).unwrap_or("Unnamed row");
    format!("{name} (row {})", position + 1)
}

fn compare(before: &Value, after: &Value, path: &str, label: &str, raw: &BTreeMap<String, String>, visited_raw: &mut BTreeSet<String>, out: &mut Vec<RecipeChange>) {
    if (before.is_object() && (after.is_object() || after.is_null()))
        || (after.is_object() && before.is_null()) {
        let keys = before.as_object().into_iter().chain(after.as_object()).flat_map(|m| m.keys()).collect::<BTreeSet<_>>();
        for key in keys {
            if (key == "schema_version" && matches!(path, "" | "equipment_overrides"))
                || (key == "row_id" && native_row(path)) { continue; }
            let next_path = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
            let next_label = if label.is_empty() { field_label(key) } else { format!("{label} / {}", field_label(key)) };
            compare(before.get(key).unwrap_or(&Value::Null), after.get(key).unwrap_or(&Value::Null), &next_path, &next_label, raw, visited_raw, out);
        }
        return;
    }
    if (before.is_array() && (after.is_array() || after.is_null()))
        || (after.is_array() && before.is_null()) {
        let old = before.as_array().map(Vec::as_slice).unwrap_or_default();
        let new = after.as_array().map(Vec::as_slice).unwrap_or_default();
        if native_rows(path) && old.iter().chain(new).all(|row| row.get("row_id").and_then(Value::as_str).is_some()) {
            let ids = |rows: &[Value]| rows.iter().filter_map(|row| row.get("row_id").and_then(Value::as_str).map(str::to_owned)).collect::<Vec<_>>();
            if ids(old) != ids(new) {
                let names = |rows: &[Value]| rows.iter().enumerate().map(|(i, row)| row_name(row, i)).collect::<Vec<_>>().join(" → ");
                out.push(RecipeChange { label: format!("{label} / Rows and order"), before: names(old), after: names(new) });
            }
            let mut seen = BTreeSet::new();
            for row in old.iter().chain(new) {
                let id = row["row_id"].as_str().expect("row identity checked above");
                if !seen.insert(id) { continue; }
                let previous = old.iter().find(|row| row["row_id"] == id);
                let current = new.iter().find(|row| row["row_id"] == id);
                let (position, named) = new.iter().enumerate().find(|(_, row)| row["row_id"] == id)
                    .or_else(|| old.iter().enumerate().find(|(_, row)| row["row_id"] == id)).expect("row belongs to either side");
                compare(previous.unwrap_or(&Value::Null), current.unwrap_or(&Value::Null), &format!("{path}[{id}]"), &format!("{label} / {}", row_name(named, position)), raw, visited_raw, out);
            }
            return;
        }
    }
    let before_text = native_text(before, path);
    let after_text = raw.get(path).map(|text| {
        visited_raw.insert(path.to_owned());
        entered_text(text, path)
    }).unwrap_or_else(|| native_text(after, path));
    if before != after || (raw.contains_key(path) && after_text != before_text) {
        out.push(RecipeChange { label: label.into(), before: before_text, after: after_text });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::recipe_draft::RecipeDraft;

    #[test]
    fn review_percentage_basis_and_yeast_alias_retain_exact_attempted_text() {
        use krausen_types::brewery::{IngredientAddition, IngredientSpecification};
        let mut base = RecipeDocumentV1::default();
        base.process.apparent_attenuation_fraction = Some(0.75);
        base.equipment_overrides.efficiency_override_fraction = Some(0.75);
        base.equipment_overrides.mash_efficiency_fraction = Some(0.75);
        base.yeast.push(IngredientAddition { row_id: "yeast-stable".into(), amount: None, unit: None,
            specification: IngredientSpecification { name: Some("House yeast".into()), producer: None,
                attenuation_fraction: Some(0.75), extensions: BTreeMap::new() }, source: None });
        for path in ["process.apparent_attenuation_fraction", "equipment_overrides.efficiency_override_fraction",
            "equipment_overrides.mash_efficiency_fraction", "yeast[yeast-stable].attenuation"] {
            let raw = BTreeMap::from([(path.into(), "75.0000000000000".into())]);
            let retained = base.clone();
            let difference = changes(&base, &base, &raw).unwrap();
            assert_eq!(difference.len(), 1, "raw field omitted: {path}");
            assert_eq!(difference[0].after, "75.0000000000000 % (entered text)", "wrong display basis: {path}");
            assert_eq!(difference[0].before, "0.75 (fraction)");
            assert_eq!(base, retained, "comparison cannot mutate canonical values");
        }
    }

    #[test]
    fn review_raw_only_omitted_optional_value_is_not_lost() {
        let base = RecipeDocumentV1::default();
        for text in ["-", "1e-"] {
            let raw = BTreeMap::from([("process.apparent_attenuation_fraction".into(), text.into())]);
            let difference = changes(&base, &base, &raw).unwrap();
            assert_eq!(difference.len(), 1, "absent native value still has attempted raw text");
            assert!(difference[0].after.contains(text));
        }
    }

    #[test]
    fn review_unsupported_source_metadata_names_are_content_not_native_envelope() {
        let base = RecipeDocumentV1::default();
        let mut changed = base.clone();
        changed.process.unsupported.insert("source".into(), serde_json::json!({"schema_version":77,"row_id":"source-row","nested":[{"schema_version":3,"row_id":"nested-source"}]}));
        let difference = changes(&base, &changed, &BTreeMap::new()).unwrap();
        assert!(difference.iter().any(|d| d.label.contains("schema version") && d.after == "77"));
        assert!(difference.iter().any(|d| d.label.contains("row id") && d.after == "source-row"));
        assert!(difference.iter().any(|d| d.after.contains("nested-source")));
    }

    #[test]
    fn raw_precision_is_compared_without_normalization_or_mutation() {
        let base = RecipeDocumentV1 { batch_size_l: Some(20.0), ..Default::default() };
        let mut draft = RecipeDraft::new(base.clone());
        draft.edit_number("batch_size_l", "20.000000000000000".into(), |d, n| d.batch_size_l = n);
        let saved = draft.recovery();
        let difference = changes(&base, draft.document(), draft.raw_numbers()).unwrap();
        assert_eq!(difference.len(), 1);
        assert_eq!(difference[0].label, "Batch into fermenter (L)");
        assert_eq!(difference[0].after, "20.000000000000000");
        assert_eq!(draft.recovery(), saved);
    }

    #[test]
    fn ingredient_and_process_changes_use_names_and_keep_preserved_source_values() {
        use krausen_types::brewery::{ProcessStep, HopAddition, HopSpecification};
        let mut base = RecipeDocumentV1::default();
        base.hops.push(HopAddition { row_id: "hop-stable".into(), amount_g: Some(10.0), stage: "unsupported_stage".into(), contact_minutes: None, temperature_c: None, specification: HopSpecification { name: Some("Hallertau".into()), producer: None, alpha_fraction: None }, source: None });
        base.process.mash_steps.push(ProcessStep { row_id: "step-stable".into(), name: Some("Saccharification".into()), time_minutes: Some(60.0), temperature_c: None, volume_l: None, extensions: BTreeMap::new() });
        let mut next = base.clone();
        next.hops[0].amount_g = Some(12.0);
        next.process.mash_steps[0].time_minutes = Some(75.0);
        next.extensions.insert("unsupported".into(), serde_json::json!({"original":"retain exactly"}));
        let difference = changes(&base, &next, &BTreeMap::new()).unwrap();
        assert!(difference.iter().any(|d| d.label.contains("Hallertau") && d.label.contains("Amount (g)")));
        assert!(difference.iter().any(|d| d.label.contains("Saccharification") && d.after == "75.0"));
        assert!(difference.iter().any(|d| d.label.contains("Preserved source data") && d.after == "retain exactly"));
        assert_eq!(next.hops[0].stage, "unsupported_stage");
    }
}
