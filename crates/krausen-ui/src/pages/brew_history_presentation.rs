// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared human presentation for historical brew evidence and comparisons.
//!
//! These helpers format stored plan/actual/measurement values for display; they
//! never recalculate, normalize or persist brewing data.
use krausen_types::{
    brew_history::{
        BrewChangeKind, BrewComparisonValue, BrewFormulationChange, BrewHistorySection,
    },
    brewery::{BrewStatusV1, MeasurementEntry},
    validation::Readiness,
};

pub(crate) fn status_label(status: BrewStatusV1) -> &'static str {
    match status {
        BrewStatusV1::Started => "Started",
        BrewStatusV1::Completed => "Completed",
        BrewStatusV1::Cancelled => "Cancelled",
    }
}

pub(crate) fn format_timestamp(seconds: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, 0)
        .map(|value| value.format("%d %b %Y, %H:%M UTC").to_string())
        .unwrap_or_else(|| "Unknown time".to_owned())
}

pub(crate) fn actor_text(actor_id: &str, current_actor: Option<&str>) -> &'static str {
    if current_actor == Some(actor_id) {
        "you"
    } else {
        "another workspace member"
    }
}

pub(crate) fn unit_label(unit: &str) -> String {
    match unit {
        "sg" => "SG".to_owned(),
        "l" => "L".to_owned(),
        "c" => "°C".to_owned(),
        "kg" => "kg".to_owned(),
        "g" => "g".to_owned(),
        "min" => "min".to_owned(),
        "ebc" => "EBC".to_owned(),
        "" => String::new(),
        other => other.to_owned(),
    }
}

fn trim_fixed(value: f64, digits: usize) -> String {
    let text = format!("{value:.digits$}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.is_empty() || text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

fn measurement_digits(unit: &str) -> usize {
    match unit {
        "sg" => 3,
        "l" => 2,
        "c" => 1,
        _ => 2,
    }
}

pub(crate) fn format_measurement_value(value: f64, unit: &str) -> String {
    let digits = measurement_digits(unit);
    let number = if unit == "sg" {
        format!("{value:.digits$}")
    } else {
        trim_fixed(value, digits)
    };
    let unit = unit_label(unit);
    if unit.is_empty() {
        number
    } else {
        format!("{number} {unit}")
    }
}

pub(crate) fn measurement_value_or(value: Option<f64>, unit: &str, missing: &str) -> String {
    value
        .map(|value| format_measurement_value(value, unit))
        .unwrap_or_else(|| missing.to_owned())
}

pub(crate) fn format_delta(delta: f64, unit: &str) -> String {
    let digits = measurement_digits(unit);
    let threshold = 0.5 * 10_f64.powi(-(digits as i32));
    let delta = if delta.abs() < threshold { 0.0 } else { delta };
    let number = if unit == "sg" {
        format!("{delta:+.digits$}")
    } else {
        let text = format!("{delta:+.digits$}");
        let text = text.trim_end_matches('0').trim_end_matches('.');
        text.to_owned()
    };
    let unit = unit_label(unit);
    if unit.is_empty() {
        number
    } else {
        format!("{number} {unit}")
    }
}

pub(crate) fn measurement_warning(code: &str) -> String {
    match code {
        "no_planned_target_for_basis" => {
            "No planned target uses this measurement's quantity and basis.".to_owned()
        }
        "planned_reference_temperature_unknown" => {
            "The frozen plan does not state a reference temperature, so this reading is not compared."
                .to_owned()
        }
        "measurement_temperature_unknown" => {
            "No sample temperature was recorded, so the reading is not compared with the 20 °C plan volumes."
                .to_owned()
        }
        "unlike_sample_temperature" => {
            "The sample temperature differs from the plan's 20 °C reference; the raw reading is retained without conversion."
                .to_owned()
        }
        "measurement_reference_temperature_unknown" => {
            "No instrument reference temperature was recorded for this gravity reading.".to_owned()
        }
        "unlike_reference_temperature" => {
            "The instrument reference temperature differs from the plan's 20 °C reference.".to_owned()
        }
        other => format!("{}.", humanize_words(other)),
    }
}

pub(crate) fn humanize_words(value: &str) -> String {
    let mut words = value
        .split(['_', '-', '.'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>();
    if words.is_empty() {
        "Value".to_owned()
    } else {
        let first = words.remove(0);
        if words.is_empty() {
            first
        } else {
            format!("{first} {}", words.join(" "))
        }
    }
}

pub(crate) fn section_label(section: BrewHistorySection) -> &'static str {
    match section {
        BrewHistorySection::Fermentable => "Fermentable",
        BrewHistorySection::Hop => "Hop",
        BrewHistorySection::Yeast => "Yeast",
        BrewHistorySection::OtherIngredient => "Other ingredient",
        BrewHistorySection::Process => "Process",
        BrewHistorySection::MashStep => "Mash step",
        BrewHistorySection::FermentationStep => "Fermentation step",
        BrewHistorySection::Notes => "Notes",
        BrewHistorySection::Extensions => "Document",
    }
}

pub(crate) fn change_kind_label(kind: BrewChangeKind) -> &'static str {
    match kind {
        BrewChangeKind::Changed => "Changed",
        BrewChangeKind::Added => "Added",
        BrewChangeKind::Removed => "Removed",
    }
}

fn process_field_label(field: &str) -> &'static str {
    match field {
        "boil_minutes" => "Boil duration",
        "apparent_attenuation_fraction" => "Apparent attenuation",
        "whirlpool_minutes" => "Whirlpool duration",
        "whirlpool_temperature_c" => "Whirlpool temperature",
        "mash_water_l" => "Mash water",
        "mash_water_method" => "Mash water method",
        "kettle_top_up_l" => "Kettle top-up",
        "unsupported" => "Unsupported process data",
        _ => "Process value",
    }
}

fn process_field_unit(field: &str) -> &'static str {
    match field {
        "boil_minutes" | "whirlpool_minutes" => "min",
        "whirlpool_temperature_c" => "c",
        "mash_water_l" | "kettle_top_up_l" => "l",
        _ => "",
    }
}

fn is_machine_identifier(value: &str) -> bool {
    let uuid_like = value.len() == 36 && value.chars().filter(|c| *c == '-').count() == 4;
    let hex_like = value.len() >= 16
        && value
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == '-' || c == '_');
    uuid_like || hex_like
}

pub(crate) fn item_label(change: &BrewFormulationChange) -> String {
    if let Some(field) = &change.field {
        return process_field_label(field).to_owned();
    }
    if let Some(name) = change
        .actual
        .as_ref()
        .and_then(|value| value.name.clone())
        .or_else(|| change.planned.as_ref().and_then(|value| value.name.clone()))
    {
        return name;
    }
    match change.section {
        BrewHistorySection::Notes => "Brew notes".to_owned(),
        BrewHistorySection::Extensions => "Document extensions".to_owned(),
        BrewHistorySection::Process => "Process value".to_owned(),
        _ => change
            .row_id
            .as_deref()
            .filter(|id| !is_machine_identifier(id))
            .map(humanize_words)
            .unwrap_or_else(|| "Ingredient row".to_owned()),
    }
}

fn comparison_value_text(
    change: &BrewFormulationChange,
    value: Option<&BrewComparisonValue>,
) -> String {
    let Some(value) = value else {
        return "Not recorded".to_owned();
    };
    let mut parts = Vec::new();
    if let Some(amount) = value.amount {
        let unit = value
            .unit
            .as_deref()
            .filter(|unit| !unit.is_empty())
            .unwrap_or_else(|| process_field_unit(change.field.as_deref().unwrap_or_default()));
        let digits = measurement_digits(unit);
        let number = if unit == "sg" {
            format!("{amount:.digits$}")
        } else {
            trim_fixed(amount, digits)
        };
        let unit = unit_label(unit);
        if unit.is_empty() {
            parts.push(number);
        } else {
            parts.push(format!("{number} {unit}"));
        }
    }
    if let Some(name) = value.name.as_deref() {
        parts.push(name.to_owned());
    }
    if let Some(stage) = value.stage.as_deref() {
        parts.push(humanize_words(stage));
    }
    if let Some(minutes) = value.time_minutes {
        parts.push(format!("{} min", trim_fixed(minutes, 2)));
    }
    if let Some(temperature) = value.temperature_c {
        parts.push(format!("{} °C", trim_fixed(temperature, 1)));
    }
    if let Some(volume) = value.volume_l {
        parts.push(format!("{} L", trim_fixed(volume, 2)));
    }
    if let Some(text) = value.text.as_deref() {
        parts.push(text.to_owned());
    }
    if parts.is_empty() {
        "Changed".to_owned()
    } else {
        parts.join(" · ")
    }
}

pub(crate) fn change_planned_text(change: &BrewFormulationChange) -> String {
    if change.planned.is_none() {
        "Not planned".to_owned()
    } else {
        comparison_value_text(change, change.planned.as_ref())
    }
}

pub(crate) fn change_actual_text(change: &BrewFormulationChange) -> String {
    if change.actual.is_none() {
        "Not recorded".to_owned()
    } else {
        comparison_value_text(change, change.actual.as_ref())
    }
}

pub(crate) fn change_section_text(change: &BrewFormulationChange) -> String {
    if change.section == BrewHistorySection::Process {
        "Process".to_owned()
    } else {
        format!(
            "{} · {}",
            section_label(change.section),
            change_kind_label(change.kind)
        )
    }
}

pub(crate) fn measurement_status_text(
    comparable: bool,
    delta: Option<f64>,
    unit: &str,
    warning: Option<&str>,
) -> String {
    if !comparable {
        return warning
            .map(measurement_warning)
            .unwrap_or_else(|| "Not comparable with the planned target.".to_owned());
    }
    match delta {
        Some(delta) => format!("Difference {}", format_delta(delta, unit)),
        None => "No reading recorded".to_owned(),
    }
}

pub(crate) fn selected_reading_context(entry: &MeasurementEntry, basis: &str) -> String {
    let mut parts = Vec::new();
    if let Some(temperature) = entry.metadata.sample_temperature_c {
        parts.push(format!("sample {} °C", trim_fixed(temperature, 1)));
    }
    if basis == "specific_gravity" {
        if let Some(reference) = entry.metadata.reference_temperature_c {
            parts.push(format!("reference {} °C", trim_fixed(reference, 1)));
        }
    }
    if entry.is_retraction() {
        parts.push("retracted".to_owned());
    }
    parts.join(" · ")
}

pub(crate) fn revision_text(value: Option<i64>) -> String {
    value
        .map(|revision| format!("revision {revision}"))
        .unwrap_or_else(|| "no recorded revision".to_owned())
}

pub(crate) fn validation_label(state: Readiness) -> &'static str {
    match state {
        Readiness::Ready => "Ready",
        Readiness::Incomplete => "Incomplete",
        Readiness::Invalid => "Invalid",
    }
}

pub(crate) fn diagnostic_text(text: &str) -> String {
    for prefix in [
        "fermentables:",
        "hops:",
        "yeast:",
        "other_ingredients:",
        "mash_steps:",
        "fermentation_steps:",
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            if let Some((row, message)) = rest.split_once(':') {
                let row = if is_machine_identifier(row) {
                    "Row".to_owned()
                } else {
                    humanize_words(row)
                };
                return format!("{row}:{}", message);
            }
        }
    }
    text.to_owned()
}

#[cfg(test)]
mod presentation_tests {
    use super::*;

    fn change(
        section: BrewHistorySection,
        id: &str,
        field: Option<&str>,
        planned: Option<BrewComparisonValue>,
        actual: Option<BrewComparisonValue>,
    ) -> BrewFormulationChange {
        BrewFormulationChange {
            id: id.into(),
            section,
            row_id: None,
            field: field.map(str::to_owned),
            kind: BrewChangeKind::Changed,
            planned,
            actual,
        }
    }

    #[test]
    fn process_change_uses_human_label_and_units() {
        let row = change(
            BrewHistorySection::Process,
            "process:boil_minutes",
            Some("boil_minutes"),
            Some(BrewComparisonValue {
                amount: Some(60.0),
                ..Default::default()
            }),
            Some(BrewComparisonValue {
                amount: Some(70.0),
                ..Default::default()
            }),
        );
        assert_eq!(item_label(&row), "Boil duration");
        assert_eq!(change_planned_text(&row), "60 min");
        assert_eq!(change_actual_text(&row), "70 min");
        let rendered = format!(
            "{} · {} · {} → {}",
            change_section_text(&row),
            item_label(&row),
            change_planned_text(&row),
            change_actual_text(&row)
        );
        assert!(!rendered.contains("BrewComparisonValue"));
        assert!(!rendered.contains("process:boil_minutes"));
        assert!(!rendered.contains("Some("));
    }

    #[test]
    fn ingredient_change_uses_resolved_name_and_unit() {
        let row = change(
            BrewHistorySection::Fermentable,
            "fermentables:grain-1",
            None,
            Some(BrewComparisonValue {
                name: Some("Pilsner malt".into()),
                amount: Some(4.2),
                unit: Some("kg".into()),
                stage: Some("mash".into()),
                ..Default::default()
            }),
            Some(BrewComparisonValue {
                name: Some("Pilsner malt".into()),
                amount: Some(4.5),
                unit: Some("kg".into()),
                stage: Some("mash".into()),
                ..Default::default()
            }),
        );
        assert_eq!(item_label(&row), "Pilsner malt");
        assert_eq!(change_planned_text(&row), "4.2 kg · Pilsner malt · Mash");
        assert_eq!(change_actual_text(&row), "4.5 kg · Pilsner malt · Mash");
    }

    #[test]
    fn missing_and_unlike_states_are_explicit_not_zero() {
        assert_eq!(
            measurement_value_or(None, "l", "No reading recorded"),
            "No reading recorded"
        );
        assert_eq!(format_measurement_value(1.05, "sg"), "1.050 SG");
        assert_eq!(format_measurement_value(22.0, "l"), "22 L");
        assert_eq!(format_measurement_value(95.0, "c"), "95 °C");
        let hot = measurement_status_text(false, None, "l", Some("unlike_sample_temperature"));
        assert!(hot.contains("20 °C reference"));
        assert!(!hot.contains("unlike_"));
        let missing =
            measurement_status_text(false, None, "l", Some("no_planned_target_for_basis"));
        assert!(missing.contains("No planned target"));
        assert_eq!(
            measurement_status_text(true, Some(0.006), "sg", None),
            "Difference +0.006 SG"
        );
    }

    #[test]
    fn timestamps_and_actors_are_human() {
        let text = format_timestamp(1_760_000_000);
        assert!(text.contains("UTC"));
        assert!(!text.contains("1760000000"));
        assert_eq!(
            actor_text("user-machine-id", Some("user-machine-id")),
            "you"
        );
        assert_eq!(
            actor_text("user-other", Some("user-machine-id")),
            "another workspace member"
        );
    }

    #[test]
    fn diagnostics_drop_machine_row_prefix_but_keep_message() {
        assert_eq!(
            diagnostic_text(
                "fermentables:grain-1: recipe specification preserved; actual specification not copied"
            ),
            "Grain 1: recipe specification preserved; actual specification not copied"
        );
        assert_eq!(
            diagnostic_text(
                "document extensions are retained as evidence and are not applied to the recipe"
            ),
            "document extensions are retained as evidence and are not applied to the recipe"
        );
    }
}
