// SPDX-License-Identifier: AGPL-3.0-or-later
//! Human presentation of structured validation diagnostics (KRA-34, UX-006).
//!
//! The brewery validator owns codes, semantic row paths, severity and affected
//! targets; nothing here decides whether a draft is valid. This module only turns
//! those facts into field- and row-specific wording and resolves each path to the
//! stable DOM id of the control that repairs it, by stable row id and never by
//! position. The Start review and the print sheet use the same wording through
//! [`plain_message`].
use crate::pages::{
    ingredient_addition::{
        AdditionCategory, category_for_head, diagnostic_field, diagnostic_row_field, field_label,
    },
    recipe_editor::BATCH_FIELD,
    recipe_equipment::{CHILLING_FIELD, chilling_control_hidden, diagnostic_spec},
    recipe_process::{diagnostic_step, fields as process_fields, show_kettle_top_up, show_manual_mash_water, step_field_label},
};
use crate::pages::recipe_draft::RecipeDraft;
use krausen_types::{
    brewery::RecipeDocumentV1,
    validation::{Diagnostic, Readiness, Validation},
};
use crate::components::diagnostic_list::{DiagnosticList, Presented, Route};
use leptos::prelude::*;

/// Wording for one validator code: the short text beside the control (whose own
/// label already names the field) and the tail of the full sentence. One table, so
/// the two cannot drift.
struct CodeText {
    code: &'static str,
    inline: &'static str,
    tail: &'static str,
}

const CODES: &[CodeText] = &[
    CodeText { code: "missing", inline: "Needed to start a brew", tail: "is missing" },
    CodeText { code: "unfinished_quantity", inline: "Enter an amount above zero to start a brew", tail: "needs an amount above zero" },
    CodeText { code: "negative", inline: "Enter a non-negative value", tail: "cannot be negative" },
    CodeText { code: "out_of_range", inline: "Value is out of range", tail: "is outside its allowed range" },
    CodeText { code: "non_finite", inline: "Enter a finite number", tail: "is not a valid number" },
    CodeText { code: "exposure_exceeds_boil", inline: "Longer than the boil time", tail: "is longer than the boil time" },
    CodeText { code: "negative_sparge", inline: "More than the water available", tail: "is more than the water available" },
    CodeText { code: "opening_exceeds_diameter", inline: "Wider than the kettle diameter", tail: "is wider than the kettle diameter" },
    CodeText { code: "plateau_not_below_boil", inline: "Must be below the boil temperature", tail: "must be below the boil temperature" },
    CodeText { code: "unsupported_fermentable_stage", inline: "Choose a supported stage to start a brew", tail: "needs a supported stage (mash or boil)" },
    CodeText { code: "unsupported_hop_stage", inline: "Choose a supported stage to start a brew", tail: "needs a supported use" },
    CodeText { code: "unsupported_fermentable_kind_or_stage", inline: "Choose a type that matches the stage to start a brew", tail: "needs a type that matches its stage" },
    CodeText { code: "unsupported_cooled_hop_addition", inline: "Clear this to start a brew", tail: "is not supported on a whirlpool addition; clear it" },
    CodeText { code: "missing_or_unsupported_mash_water_method", inline: "Choose a mash water method to start a brew", tail: "needs a supported method" },
];

fn code_text(code: &str) -> Option<&'static CodeText> {
    CODES.iter().find(|entry| entry.code == code)
}

/// Short text shown beside the control itself.
pub fn inline_text(code: &str, severity: Readiness) -> &'static str {
    match (code_text(code), severity) {
        (Some(entry), _) => entry.inline,
        (None, Readiness::Invalid) => "Check this value",
        (None, _) => "Needed to start a brew",
    }
}

/// What is wrong with the named field, as a sentence tail.
fn tail(code: &str) -> &'static str {
    code_text(code).map_or("needs attention", |entry| entry.tail)
}

fn explanation(path: &str, code: &str) -> String {
    match code {
        "unsupported_schema" => "This recipe uses a schema version this app cannot edit. Its values are preserved unchanged and cannot be corrected here.".to_owned(),
        "unsupported_process" => "This recipe carries process data this app cannot edit. It is preserved unchanged; correct it in the tool that created it.".to_owned(),
        "invalid_row_id" => "A row has a missing or duplicated internal id. The recipe is preserved unchanged and the row cannot be repaired here.".to_owned(),
        _ => format!("{} needs attention but has no control in this editor.", capitalized(&path.replace(['_', '.'], " "))),
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

fn lowered(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|first| first.to_lowercase().chain(chars).collect()).unwrap_or_default()
}

fn sentence(row: Option<&str>, label: &str, code: &str) -> String {
    let body = format!("{} {}.", capitalized(label), tail(code));
    match row {
        Some(row) => format!("{row}: {body}"),
        None => body,
    }
}

/// Where a path resolves to, before wording.
struct Located {
    row: Option<String>,
    label: String,
    route: Route,
    /// Why the link goes to a governing control instead of the named field.
    note: Option<String>,
}

fn fixed_field(path: &str) -> Option<(&'static str, &'static str)> {
    use process_fields as f;
    let (id, label) = match path {
        "batch_size_l" => BATCH_FIELD,
        "boil_minutes" => f::BOIL,
        "process.whirlpool_minutes" => f::WHIRLPOOL,
        "process.whirlpool_temperature_c" => f::WHIRLPOOL_TEMP,
        "process.kettle_top_up_l" => f::TOP_UP,
        "process.mash_water_l" => f::MASH_WATER,
        "process.mash_water_method" => f::MASH_METHOD,
        "process.apparent_attenuation_fraction" => f::ATTENUATION,
        "equipment.chilling" | "equipment_overrides.chilling" => CHILLING_FIELD,
        _ => return None,
    };
    Some((label, id))
}

/// A control that is only rendered in some states links to the control that
/// governs it instead, never to an input that is not on the page.
fn governed(draft: &RecipeDraft, mut located: Located) -> Located {
    let Route::Field { dom_id } = &located.route else { return located };
    let doc = draft.document();
    let (governing, note) = match dom_id.as_str() {
        id if id == process_fields::MASH_WATER.0 && !show_manual_mash_water(draft) => (process_fields::MASH_METHOD.0, "Change Mash water method to adjust it."),
        id if id == process_fields::TOP_UP.0 && !show_kettle_top_up(draft) => (process_fields::MASH_METHOD.0, "Choose a Mash water method to enter it."),
        id if chilling_control_hidden(doc, id) => (CHILLING_FIELD.0, "Set Chilling for this recipe to enter it."),
        _ => return located,
    };
    located.route = Route::Field { dom_id: governing.into() };
    located.note = Some(note.into());
    located
}

fn explain() -> Located {
    Located { row: None, label: String::new(), route: Route::Explain, note: None }
}

/// `Err(())` means the path addresses a row that no longer exists: stale.
fn locate(draft: &RecipeDraft, path: &str, code: &str) -> Result<Located, ()> {
    let doc = draft.document();
    let field = |row: Option<String>, label: &str, dom_id: &str| Located { row, label: label.into(), route: Route::Field { dom_id: dom_id.into() }, note: None };
    if code == "empty_fermentables" {
        return Ok(field(None, "Fermentables", &AdditionCategory::Fermentable.add_button_id()));
    }
    if let Some((label, dom_id)) = fixed_field(path) {
        return Ok(governed(draft, field(None, label, dom_id)));
    }
    if path.starts_with("process.mash_steps[") || path.starts_with("process.fermentation_steps[") {
        return match diagnostic_step(doc, path) {
            Some(step) => Ok(field(Some(step.row_label), step.label, &step.dom_id)),
            None if path.ends_with(".row_id") => Ok(explain()),
            None => Err(()),
        };
    }
    if let Some((dom_id, label)) = diagnostic_spec(path) {
        return Ok(governed(draft, field(None, label, dom_id)));
    }
    if let Some((head, rest)) = path.split_once('[') {
        if rest.ends_with("].row_id") {
            return Ok(explain());
        }
        return match diagnostic_field(doc, path) {
            Some(opening) => Ok(field(Some(opening.row_label), opening.label, &opening.dom_id)),
            None if matches!(head, "fermentables" | "hops" | "yeast" | "other_ingredients" | "ingredients") => Err(()),
            None => Ok(explain()),
        };
    }
    Ok(explain())
}

/// The path the validator reports for a raw-input path. The two vocabularies
/// differ for yeast/other amounts, yeast attenuation and equipment overrides.
pub fn validator_paths(raw: &str) -> Vec<String> {
    let mut paths = vec![raw.to_owned()];
    if let Some(service) = crate::pages::ingredient_addition::service_path_for_raw(raw) {
        paths.push(service);
    } else if let Some(effective) = effective_equipment_path(raw) {
        paths.push(effective);
    }
    paths
}

/// The validator's effective-value path for an equipment override path: it reports
/// one setting under both `equipment_overrides.X` and `equipment.X`.
fn effective_equipment_path(path: &str) -> Option<String> {
    path.strip_prefix("equipment_overrides.").map(|field| format!("equipment.{field}"))
}

fn key_for(path: &str, code: &str) -> String {
    let clean: String = format!("{path}-{code}").chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    format!("diagnostic-{clean}")
}

fn present_one(draft: &RecipeDraft, diagnostic: &Diagnostic) -> Option<Presented> {
    let located = locate(draft, &diagnostic.path, &diagnostic.code).ok()?;
    let message = match &located.route {
        Route::Explain => explanation(&diagnostic.path, &diagnostic.code),
        Route::Field { .. } if diagnostic.code == "empty_fermentables" => "Add at least one fermentable.".to_owned(),
        Route::Field { .. } => {
            let base = sentence(located.row.as_deref(), &located.label, &diagnostic.code);
            match &located.note {
                Some(note) => format!("{base} {note}"),
                None => base,
            }
        }
    };
    Some(Presented {
        key: key_for(&diagnostic.path, &diagnostic.code),
        path: diagnostic.path.clone(),
        code: diagnostic.code.clone(),
        severity: diagnostic.severity,
        message,
        route: located.route,
    })
}

/// Document order of the control a path addresses, so the summary reads top to
/// bottom the way the editor does.
fn order(doc: &RecipeDocumentV1, path: &str) -> (u8, usize) {
    fn position<'a>(mut ids: impl Iterator<Item = &'a String>, id: &str) -> usize {
        ids.position(|row| row == id).unwrap_or(usize::MAX)
    }
    let row_id = path.split_once('[').and_then(|(_, rest)| rest.split_once(']')).map(|(id, _)| id).unwrap_or("");
    match path.split_once('[').map_or(path, |(head, _)| head) {
        "batch_size_l" => (0, 0),
        "boil_minutes" => (1, 0),
        "fermentables" => (2, position(doc.fermentables.iter().map(|r| &r.row_id), row_id)),
        "hops" => (3, position(doc.hops.iter().map(|r| &r.row_id), row_id)),
        "yeast" | "other_ingredients" | "ingredients" => (4, 0),
        "process.mash_steps" => (5, position(doc.process.mash_steps.iter().map(|r| &r.row_id), row_id)),
        "process.fermentation_steps" => (6, position(doc.process.fermentation_steps.iter().map(|r| &r.row_id), row_id)),
        other if other.starts_with("process.") => (5, usize::MAX),
        other if other.starts_with("equipment") => (7, 0),
        _ => (8, 0),
    }
}

/// What makes two diagnostics the same problem: the path and code, where the
/// validator's override path (`equipment_overrides.*`) and effective path
/// (`equipment.*`) for one setting count as one. Message text never decides it.
pub(crate) fn problem_key(d: &Diagnostic) -> (String, String) {
    (effective_equipment_path(&d.path).unwrap_or_else(|| d.path.clone()), d.code.clone())
}

/// Every distinct diagnostic that still applies to `doc`, in editor order.
/// Distinctness is the (path, code) pair, never the message text, so two rows
/// with the same problem stay two entries. A diagnostic for a row that has since
/// been removed is dropped rather than linked to nothing.
pub fn present_all(draft: &RecipeDraft, diagnostics: &[Diagnostic]) -> Vec<Presented> {
    let doc = draft.document();
    let mut seen = std::collections::BTreeSet::new();
    let mut presented: Vec<(usize, Presented)> = diagnostics
        .iter()
        .enumerate()
        .filter(|(_, d)| seen.insert(problem_key(d)))
        .filter_map(|(index, d)| present_one(draft, d).map(|p| (index, p)))
        .collect();
    presented.sort_by_key(|(index, p)| (order(doc, &p.path), *index));
    presented.into_iter().map(|(_, p)| p).collect()
}

/// Document-free wording for surfaces that hold no editable draft (the print
/// sheet and the Start review). Rows are named by group, not by identity.
pub fn plain_message(diagnostic: &Diagnostic) -> String {
    let path = diagnostic.path.as_str();
    if diagnostic.code == "empty_fermentables" {
        return "Add at least one fermentable.".into();
    }
    if let Some((label, _)) = fixed_field(path) {
        return sentence(None, label, &diagnostic.code);
    }
    if let Some((_, label)) = diagnostic_spec(path) {
        return sentence(None, &format!("equipment {}", lowered(label)), &diagnostic.code);
    }
    for (prefix, noun) in [("process.mash_steps[", "Mash step"), ("process.fermentation_steps[", "Fermentation step")] {
        if let Some(rest) = path.strip_prefix(prefix) {
            let Some(label) = rest.split_once("].").and_then(|(_, t)| step_field_label(t)) else {
                return explanation(path, &diagnostic.code);
            };
            return sentence(None, &format!("{noun} {}", lowered(label)), &diagnostic.code);
        }
    }
    if let Some((head, rest)) = path.split_once('[') {
        let tail_path = rest.split_once("].").map(|(_, t)| t).unwrap_or("");
        let category = match head {
            "ingredients" => Some(AdditionCategory::Other),
            other => category_for_head(other),
        };
        if let Some(category) = category {
            if let Some(field) = diagnostic_row_field(category, tail_path) {
                let noun = if head == "ingredients" { "Ingredient".to_owned() } else { capitalized(category.noun()) };
                return sentence(None, &format!("{noun} {}", lowered(field_label(category, field).0)), &diagnostic.code);
            }
        }
    }
    explanation(path, &diagnostic.code)
}

/// Wording for a printed or reviewed summary: the same sentence as
/// [`plain_message`], preceded by the row it is about (name, group, stage and
/// position) so two rows with the same problem read differently.
pub fn summary_message(summary: &krausen_types::process_summary::ProcessSummary, diagnostic: &Diagnostic) -> String {
    use krausen_types::process_summary::{SummaryIngredient, SummaryStep};
    let plain = plain_message(diagnostic);
    let Some((head, rest)) = diagnostic.path.split_once('[') else { return plain };
    let Some((id, _)) = rest.split_once(']') else { return plain };
    let ingredient = |rows: &[SummaryIngredient], noun: &str| -> Option<String> {
        let position = rows.iter().position(|row| row.row_id == id)?;
        let row = &rows[position];
        let mut parts = vec![match row.name.as_deref().filter(|name| !name.trim().is_empty()) {
            Some(name) => format!("{name}, {noun}"),
            None => format!("Unnamed {noun}"),
        }];
        if let Some(stage) = row.stage.as_deref().filter(|stage| !stage.is_empty()) {
            parts.push(capitalized(&stage.replace('_', " ")));
        }
        parts.push(format!("{} of {}", position + 1, rows.len()));
        Some(parts.join(", "))
    };
    let step = |rows: &[SummaryStep], noun: &str| -> Option<String> {
        let position = rows.iter().position(|row| row.row_id == id)?;
        let base = format!("{noun} {} of {}", position + 1, rows.len());
        Some(match rows[position].name.as_deref().filter(|name| !name.is_empty()) {
            Some(name) => format!("{base}, {name}"),
            None => base,
        })
    };
    let row = match head {
        "fermentables" => ingredient(&summary.fermentables, "fermentable"),
        "hops" => ingredient(&summary.hops, "hop"),
        "yeast" => ingredient(&summary.yeast, "yeast"),
        "other_ingredients" => ingredient(&summary.other_ingredients, "other ingredient"),
        "ingredients" => ingredient(&summary.yeast, "yeast").or_else(|| ingredient(&summary.other_ingredients, "other ingredient")),
        "process.mash_steps" => step(&summary.process.mash_steps, "Mash step"),
        "process.fermentation_steps" => step(&summary.process.fermentation_steps, "Fermentation step"),
        _ => None,
    };
    match row {
        Some(row) => format!("{row}: {plain}"),
        None => plain,
    }
}

/// One concise settled readiness announcement: the count, never the list.
pub fn readiness_summary(validation: &Validation) -> String {
    let count = |severity: Readiness| {
        let mut seen = std::collections::BTreeSet::new();
        validation.diagnostics.iter().filter(|d| d.severity == severity && seen.insert(problem_key(d))).count()
    };
    let plural = |n: usize, one: &str, many: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {many}") };
    match validation.state {
        Readiness::Ready => "Ready".into(),
        Readiness::Incomplete => match count(Readiness::Incomplete) {
            0 => "Incomplete".into(),
            n => format!("Incomplete: {} needed to start a brew", plural(n, "input", "inputs")),
        },
        Readiness::Invalid => match count(Readiness::Invalid) {
            0 => "Invalid".into(),
            n => format!("Invalid: {} to correct", plural(n, "value", "values")),
        },
    }
}

/// The batch volume to present beside the estimates. A preview's context repeats
/// the submitted value even when the validator rejected it; that rejected input is
/// not an estimate, so only the validator's verdict decides whether to show it
/// (UX-007). Component availability is still the caller's state.
pub(crate) fn estimated_batch_volume(validation: &Validation, batch_size_l: Option<f64>) -> Option<f64> {
    let rejected = validation.diagnostics.iter().any(|diagnostic| {
        diagnostic.path == "batch_size_l" && diagnostic.severity == Readiness::Invalid
    });
    if rejected { None } else { batch_size_l }
}

/// First control a brewer must correct before the recipe can be saved: a
/// malformed number whose format feedback is already showing, else the first
/// retained Invalid diagnostic whose field has not been edited since (an edit
/// drops its entry). A first character such as "-" is transitional and never
/// counts. Retained entries stay targetable while the list is out of date, so
/// typing in another field never makes the command blink away and move the page.
pub fn first_invalid(draft: &RecipeDraft) -> Option<Presented> {
    let doc = draft.document();
    let malformed = draft.settled_malformed().into_iter().filter_map(|path| {
        let validator = validator_paths(&path).pop().unwrap_or_else(|| path.clone());
        let located = locate(draft, &validator, "").ok()?;
        let Route::Field { dom_id } = located.route else { return None };
        Some((order(doc, &validator), Presented {
            key: key_for(&path, "malformed"),
            path,
            code: "malformed".into(),
            severity: Readiness::Invalid,
            message: "Enter a number".into(),
            route: Route::Field { dom_id },
        }))
    });
    malformed.min_by_key(|(order, _)| *order).map(|(_, p)| p).or_else(|| {
        present_all(draft, draft.diagnostics()).into_iter().find(|p| p.severity == Readiness::Invalid)
    })
}

/// The summary for the recipe editor: what the draft's last settled check found.
#[component]
pub fn DiagnosticSummary(draft: RwSignal<RecipeDraft>) -> impl IntoView {
    let items = Memo::new(move |_| draft.with(|d| present_all(d, d.diagnostics())));
    let stale = Signal::derive(move || draft.with(|d| !d.diagnostics_current()));
    view! { <DiagnosticList items=Signal::from(items) stale=stale/> }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krausen_types::brewery::{HopAddition, HopSpecification};

    fn hop(id: &str, name: &str, stage: &str, contact: f64) -> HopAddition {
        HopAddition {
            row_id: id.into(),
            amount_g: Some(10.0),
            specification: HopSpecification { name: Some(name.into()), producer: None, alpha_fraction: None },
            stage: stage.into(),
            contact_minutes: Some(contact),
            temperature_c: None,
            source: None,
        }
    }

    fn diag(path: &str, code: &str, severity: Readiness) -> Diagnostic {
        Diagnostic { code: code.into(), path: path.into(), severity, affected_targets: vec![] }
    }

    fn doc() -> RecipeDocumentV1 {
        let mut doc = RecipeDocumentV1::default();
        doc.hops = vec![hop("h1", "Cascade", "boil", 60.0), hop("h2", "Cascade", "boil", 15.0)];
        doc
    }

    #[test]
    fn two_hops_with_the_same_problem_stay_distinct_and_name_their_rows() {
        let doc = doc();
        let list = present_all(&RecipeDraft::new(doc.clone()), &[
            diag("hops[h1].specification.alpha_fraction", "missing", Readiness::Incomplete),
            diag("hops[h2].specification.alpha_fraction", "missing", Readiness::Incomplete),
        ]);
        assert_eq!(list.len(), 2);
        assert_ne!(list[0].message, list[1].message);
        assert!(list[0].message.contains("1 of 2") && list[1].message.contains("2 of 2"), "{list:?}");
        assert!(list[0].message.contains("Alpha acid is missing"));
        assert_eq!(list[0].route, Route::Field { dom_id: "hops-h1-yield-alpha".into() });
        assert_eq!(list[1].route, Route::Field { dom_id: "hops-h2-yield-alpha".into() });
    }

    #[test]
    fn identical_path_and_code_are_one_entry_but_other_codes_on_it_are_kept() {
        let doc = doc();
        let alpha = "hops[h1].specification.alpha_fraction";
        let list = present_all(&RecipeDraft::new(doc.clone()), &[
            diag(alpha, "missing", Readiness::Incomplete),
            diag(alpha, "missing", Readiness::Incomplete),
            diag(alpha, "out_of_range", Readiness::Invalid),
        ]);
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn a_removed_row_is_dropped_not_linked_to_nothing() {
        let mut doc = doc();
        doc.hops.remove(0);
        let list = present_all(&RecipeDraft::new(doc.clone()), &[
            diag("hops[h1].amount_g", "missing", Readiness::Incomplete),
            diag("hops[h2].amount_g", "missing", Readiness::Incomplete),
        ]);
        assert_eq!(list.len(), 1);
        assert!(list[0].message.contains("1 of 1"), "positions follow the current document: {list:?}");
    }

    #[test]
    fn rows_are_resolved_by_id_after_an_insert_changes_every_position() {
        let mut doc = doc();
        doc.hops.insert(0, hop("h0", "Saaz", "boil", 30.0));
        let list = present_all(&RecipeDraft::new(doc.clone()), &[diag("hops[h2].amount_g", "missing", Readiness::Incomplete)]);
        assert_eq!(list[0].route, Route::Field { dom_id: "hops-h2-amount".into() });
        assert!(list[0].message.contains("3 of 3"), "{list:?}");
    }

    #[test]
    fn preserved_data_gets_an_explanation_and_no_link() {
        let doc = doc();
        for (path, code) in [("schema_version", "unsupported_schema"), ("process.unsupported", "unsupported_process"), ("hops[h1].row_id", "invalid_row_id")] {
            let list = present_all(&RecipeDraft::new(doc.clone()), &[diag(path, code, Readiness::Invalid)]);
            assert_eq!(list.len(), 1, "{path}");
            assert_eq!(list[0].route, Route::Explain, "{path}");
            assert!(list[0].message.contains("cannot"), "{}", list[0].message);
        }
    }

    #[test]
    fn selects_and_process_fields_resolve_to_their_stable_ids() {
        let mut doc = doc();
        doc.process.mash_steps.push(krausen_types::brewery::ProcessStep { row_id: "m1".into(), name: Some("Rest".into()), time_minutes: None, temperature_c: None, volume_l: None, extensions: Default::default() });
        let list = present_all(&RecipeDraft::new(doc.clone()), &[
            diag("process.mash_water_method", "missing_or_unsupported_mash_water_method", Readiness::Incomplete),
            diag("process.mash_steps[m1].time_minutes", "negative", Readiness::Invalid),
            diag("hops[h1].stage", "unsupported_hop_stage", Readiness::Incomplete),
            diag("equipment.chilling", "missing", Readiness::Incomplete),
            diag("equipment.kettle_opening_cm", "opening_exceeds_diameter", Readiness::Invalid),
            diag("fermentables", "empty_fermentables", Readiness::Incomplete),
        ]);
        let ids: Vec<_> = list.iter().map(|p| match &p.route { Route::Field { dom_id } => dom_id.as_str(), Route::Explain => "-" }).collect();
        assert!(ids.contains(&"recipe-mash-method") && ids.contains(&"process.mash_steps-m1-time_minutes"));
        assert!(ids.contains(&"hops-h1-stage"), "{ids:?}");
        assert!(ids.contains(&"recipe-chilling") && ids.contains(&"fermentables-add"), "{ids:?}");
        assert!(ids.contains(&"recipe-opening"), "{ids:?}");
    }

    fn field_id(p: &Presented) -> &str {
        match &p.route { Route::Field { dom_id } => dom_id, Route::Explain => "-" }
    }

    #[test]
    fn an_empty_draft_links_add_at_least_one_fermentable_to_the_add_control() {
        let list = present_all(&RecipeDraft::new(RecipeDocumentV1::default()), &[diag("fermentables", "empty_fermentables", Readiness::Incomplete)]);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].message, "Add at least one fermentable.");
        assert_eq!(field_id(&list[0]), "fermentables-add");
    }

    #[test]
    fn several_invalid_entries_of_one_category_stay_separate_and_in_order() {
        let doc = doc();
        let list = present_all(&RecipeDraft::new(doc.clone()), &[
            diag("hops[h2].amount_g", "negative", Readiness::Invalid),
            diag("hops[h1].amount_g", "negative", Readiness::Invalid),
            diag("hops[h1].contact_minutes", "exposure_exceeds_boil", Readiness::Invalid),
        ]);
        let ids: Vec<_> = list.iter().map(field_id).collect();
        assert_eq!(ids, ["hops-h1-amount", "hops-h1-colour-time", "hops-h2-amount"].map(|s| s));
        assert_eq!(list.iter().map(|p| p.message.as_str()).collect::<std::collections::BTreeSet<_>>().len(), 3);
    }

    #[test]
    fn a_conditional_control_that_is_not_rendered_links_to_the_control_that_governs_it() {
        let mut doc = doc();
        doc.process.mash_water_method = Some("profile_thickness".into());
        let sparge = [diag("process.mash_water_l", "negative_sparge", Readiness::Invalid)];
        let list = present_all(&RecipeDraft::new(doc.clone()), &sparge);
        assert_eq!(field_id(&list[0]), "recipe-mash-method", "mash water is only on the page for the manual method");
        doc.process.mash_water_method = Some("manual".into());
        assert_eq!(field_id(&present_all(&RecipeDraft::new(doc.clone()), &sparge)[0]), "recipe-mash-water");

        use krausen_types::brewery::ChillingV1;
        let rate = [diag("equipment.chilling.rate_per_minute", "missing", Readiness::Incomplete)];
        let inherited = present_all(&RecipeDraft::new(doc.clone()), &rate);
        assert_eq!(field_id(&inherited[0]), "recipe-chilling", "no chilling override: the rate field is not rendered");
        assert!(inherited[0].message.contains("Set Chilling for this recipe"), "{}", inherited[0].message);
        doc.equipment_overrides.chilling = Some(ChillingV1::Forced { rate_per_minute: None, plateau_c: None });
        assert_eq!(field_id(&present_all(&RecipeDraft::new(doc), &rate)[0]), "recipe-chilling-rate");
    }

    #[test]
    fn raw_and_validator_paths_are_mapped_both_ways() {
        assert_eq!(validator_paths("yeast[y1].amount"), ["yeast[y1].amount", "ingredients[y1].amount"]);
        assert_eq!(validator_paths("other_ingredients[o1].amount"), ["other_ingredients[o1].amount", "ingredients[o1].amount"]);
        assert_eq!(validator_paths("yeast[y1].attenuation"), ["yeast[y1].attenuation", "yeast[y1].specification.attenuation_fraction"]);
        assert_eq!(validator_paths("equipment_overrides.kettle_opening_cm"), ["equipment_overrides.kettle_opening_cm", "equipment.kettle_opening_cm"]);
        assert_eq!(validator_paths("batch_size_l"), ["batch_size_l"]);
    }

    #[test]
    fn review_ignores_a_transitional_first_character_and_finds_a_settled_malformed_yeast_attenuation() {
        use krausen_types::brewery::{IngredientAddition, IngredientSpecification};
        let mut doc = RecipeDocumentV1::default();
        doc.yeast.push(IngredientAddition { row_id: "y1".into(), amount: None, unit: None, specification: IngredientSpecification { name: Some("US-05".into()), producer: None, attenuation_fraction: None, extensions: Default::default() }, source: None });
        let mut draft = RecipeDraft::new(doc);
        draft.edit_number("yeast[y1].attenuation", "-".into(), |_, _| {});
        assert!(first_invalid(&draft).is_none(), "typing the first character is not an error");
        draft.edit_number("yeast[y1].attenuation", "7x".into(), |_, _| {});
        assert!(first_invalid(&draft).is_none(), "not settled until blur or an explicit review");
        draft.settle_number("yeast[y1].attenuation");
        let first = first_invalid(&draft).expect("settled malformed text is a highlighted field");
        assert_eq!(field_id(&first), "yeast-y1-attenuation");
    }

    #[test]
    fn diagnostics_from_an_earlier_draft_are_not_presented_as_current() {
        use krausen_types::validation::Validation;
        let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
        let generation = draft.generation;
        draft.apply_validation(generation, Validation { state: Readiness::Invalid, diagnostics: vec![diag("batch_size_l", "negative", Readiness::Invalid)] });
        assert!(draft.diagnostics_current());
        assert_eq!(first_invalid(&draft).map(|p| p.path), Some("batch_size_l".into()));
        // The check is no longer trusted at the same generation (context loss, retry):
        // the list is out of date as long as the estimate is cleared, and stays targetable.
        draft.clear_readiness();
        assert!(!draft.diagnostics_current());
        assert_eq!(first_invalid(&draft).map(|p| p.path), Some("batch_size_l".into()));
        assert!(draft.apply_validation(generation, Validation { state: Readiness::Invalid, diagnostics: vec![diag("batch_size_l", "negative", Readiness::Invalid)] }));
        assert!(draft.diagnostics_current(), "a settled check at this generation makes it current again");
        draft.edit(|doc| doc.title = "moved on".into());
        assert!(!draft.diagnostics_current());
        assert_eq!(draft.diagnostics().len(), 1, "still shown, marked out of date");
        assert_eq!(first_invalid(&draft).map(|p| p.path), Some("batch_size_l".into()), "Review keeps targeting the retained entry, so the command never blinks while another field is edited");
        draft.edit_number("boil_minutes", "61".into(), |doc, v| doc.boil_minutes = v);
        assert_eq!(first_invalid(&draft).map(|p| p.path), Some("batch_size_l".into()), "still there after typing in a different field");
        draft.edit_number("batch_size_l", "20".into(), |doc, v| doc.batch_size_l = v);
        assert!(first_invalid(&draft).is_none(), "editing the field itself retires its entry");
        let mut restored = RecipeDraft::new(RecipeDocumentV1::default());
        let generation = restored.generation;
        restored.apply_validation(generation, Validation { state: Readiness::Invalid, diagnostics: vec![diag("batch_size_l", "negative", Readiness::Invalid)] });
        let contents = restored.recovery();
        restored.restore(contents);
        assert!(!restored.diagnostics_current(), "a recovery restore replaces the document");
    }

    #[test]
    fn every_validator_code_has_specific_wording() {
        // The codes crates/krausen-brewery/src/validation.rs can emit.
        for code in [
            "missing", "unfinished_quantity", "negative", "out_of_range", "non_finite", "exposure_exceeds_boil",
            "negative_sparge", "opening_exceeds_diameter", "plateau_not_below_boil", "unsupported_fermentable_stage",
            "unsupported_hop_stage", "unsupported_fermentable_kind_or_stage", "unsupported_cooled_hop_addition",
            "missing_or_unsupported_mash_water_method",
        ] {
            assert!(code_text(code).is_some(), "{code} would read as generic");
        }
        let plateau = plain_message(&diag("equipment.chilling.plateau_c", "plateau_not_below_boil", Readiness::Invalid));
        assert_eq!(plateau, "Equipment cooling plateau must be below the boil temperature.");
    }

    #[test]
    fn one_problem_reported_under_override_and_effective_paths_is_listed_once() {
        let doc = doc();
        let list = present_all(&RecipeDraft::new(doc), &[
            diag("equipment_overrides.boil_off_l_per_hour", "negative", Readiness::Invalid),
            diag("equipment.boil_off_l_per_hour", "negative", Readiness::Invalid),
            diag("hops[h1].amount_g", "negative", Readiness::Invalid),
            diag("hops[h2].amount_g", "negative", Readiness::Invalid),
        ]);
        assert_eq!(list.len(), 3, "{list:?}");
        assert_eq!(list.iter().filter(|p| p.message.starts_with("Boil-off")).count(), 1);
    }

    #[test]
    fn two_different_problems_that_share_a_governing_control_stay_two_and_the_count_matches_the_list() {
        use krausen_types::validation::Validation;
        let draft = RecipeDraft::new(doc());
        let diagnostics = vec![
            diag("equipment.chilling.rate_per_minute", "missing", Readiness::Incomplete),
            diag("equipment.chilling.plateau_c", "missing", Readiness::Incomplete),
            diag("equipment_overrides.boil_off_l_per_hour", "negative", Readiness::Invalid),
            diag("equipment.boil_off_l_per_hour", "negative", Readiness::Invalid),
        ];
        let list = present_all(&draft, &diagnostics);
        assert_eq!(list.iter().filter(|p| p.severity == Readiness::Incomplete).count(), 2, "rate and plateau both govern via Chilling but are two problems");
        let validation = Validation { state: Readiness::Invalid, diagnostics };
        assert_eq!(readiness_summary(&validation), "Invalid: 1 value to correct");
        let incomplete = Validation { state: Readiness::Incomplete, diagnostics: validation.diagnostics.into_iter().filter(|d| d.severity == Readiness::Incomplete).collect() };
        assert_eq!(readiness_summary(&incomplete), "Incomplete: 2 inputs needed to start a brew");
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn summary_reads_in_editor_order() {
        let mut doc = doc();
        doc.process.mash_steps.clear();
        let list = present_all(&RecipeDraft::new(doc.clone()), &[
            diag("process.mash_water_l", "missing", Readiness::Incomplete),
            diag("hops[h2].amount_g", "missing", Readiness::Incomplete),
            diag("batch_size_l", "missing", Readiness::Incomplete),
            diag("hops[h1].amount_g", "missing", Readiness::Incomplete),
        ]);
        let paths: Vec<_> = list.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, ["batch_size_l", "hops[h1].amount_g", "hops[h2].amount_g", "process.mash_water_l"]);
    }

    #[test]
    fn plain_wording_names_the_field_and_not_just_a_category() {
        let hop_alpha = plain_message(&diag("hops[h1].specification.alpha_fraction", "missing", Readiness::Incomplete));
        assert_eq!(hop_alpha, "Hop alpha acid is missing.");
        assert_eq!(plain_message(&diag("batch_size_l", "missing", Readiness::Incomplete)), "Batch volume is missing.");
        assert_eq!(plain_message(&diag("hops[h1].amount_g", "negative", Readiness::Invalid)), "Hop amount cannot be negative.");
    }

    #[test]
    fn readiness_announcement_is_a_count_not_the_list() {
        let validation = Validation {
            state: Readiness::Incomplete,
            diagnostics: vec![diag("batch_size_l", "missing", Readiness::Incomplete), diag("boil_minutes", "missing", Readiness::Incomplete)],
        };
        assert_eq!(readiness_summary(&validation), "Incomplete: 2 inputs needed to start a brew");
        let invalid = Validation { state: Readiness::Invalid, diagnostics: vec![diag("batch_size_l", "negative", Readiness::Invalid)] };
        assert_eq!(readiness_summary(&invalid), "Invalid: 1 value to correct");
        assert_eq!(readiness_summary(&Validation { state: Readiness::Ready, diagnostics: vec![] }), "Ready");
    }

    #[test]
    fn a_validator_rejected_batch_volume_is_not_an_estimate() {
        // The preview context repeats the submitted draft, including a value the
        // validator rejected. Only that verdict decides whether it is shown.
        for (code, value) in [("negative", -5.0), ("out_of_range", 0.0)] {
            let validation = Validation { state: Readiness::Invalid, diagnostics: vec![diag("batch_size_l", code, Readiness::Invalid)] };
            assert_eq!(estimated_batch_volume(&validation, Some(value)), None, "{code} is not an estimate");
        }
    }

    #[test]
    fn an_accepted_batch_volume_stays_and_other_invalid_fields_do_not_hide_it() {
        let ready = Validation { state: Readiness::Ready, diagnostics: vec![] };
        assert_eq!(estimated_batch_volume(&ready, Some(20.0)), Some(20.0));
        let other_field = Validation { state: Readiness::Invalid, diagnostics: vec![diag("hops[h1].amount_g", "negative", Readiness::Invalid)] };
        assert_eq!(estimated_batch_volume(&other_field, Some(20.0)), Some(20.0), "a different invalid value does not make the batch an estimate");
        let missing = Validation { state: Readiness::Incomplete, diagnostics: vec![diag("batch_size_l", "missing", Readiness::Incomplete)] };
        assert_eq!(estimated_batch_volume(&missing, None), None);
    }
}
