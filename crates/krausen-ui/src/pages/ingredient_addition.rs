// SPDX-License-Identifier: AGPL-3.0-or-later
//! One Add grammar and row grammar for fermentables, hops, yeast and other
//! additions (UX-002/003, ACC-001/004).
//!
//! Browsing or typing in an Add task never changes the recipe: Cancel creates no
//! row and the explicit Add to recipe command creates exactly one stable local
//! row. Fermentables keep the existing searchable catalogue/library picker; the
//! other groups use truthful manual specification forms because no supported
//! catalogue exists for them. Brewing arithmetic, defaults and totals stay in
//! the shared services; this module only moves entered values.
use crate::{
    components::{
        BreweryDialog, BreweryDraftField, BreweryDraftSelect, BreweryPanel, Button, ButtonVariant,
        brewery_kit::{BreweryCompactIngredientRow, BreweryActionGroup, BreweryFieldRow},
    },
    pages::{
        catalogue_picker::CataloguePicker,
        recipe_draft::{RecipeDraft, RemovedInputs},
    },
};
use krausen_types::{
    brewery::{
        FermentableAddition, HopAddition, HopSpecification, IngredientAddition,
        IngredientSpecification, RecipeDocumentV1,
    },
    request_context::OperationIntent,
};
use leptos::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdditionCategory {
    Fermentable,
    Hop,
    Yeast,
    Other,
}

impl AdditionCategory {
    pub(crate) const ALL: [Self; 4] = [Self::Fermentable, Self::Hop, Self::Yeast, Self::Other];
    /// DOM-id and raw-input path prefix; unchanged from the previous editor.
    pub(crate) fn prefix(self) -> &'static str {
        match self {
            Self::Fermentable => "fermentables",
            Self::Hop => "hops",
            Self::Yeast => "yeast",
            Self::Other => "other_ingredients",
        }
    }
    pub(crate) fn noun(self) -> &'static str {
        match self {
            Self::Fermentable => "fermentable",
            Self::Hop => "hop",
            Self::Yeast => "yeast",
            Self::Other => "other ingredient",
        }
    }
    fn opener(self) -> &'static str {
        match self {
            Self::Fermentable => "Add fermentable",
            Self::Hop => "Add hop",
            Self::Yeast => "Add yeast",
            Self::Other => "Add other ingredient",
        }
    }
    fn amount_unit(self) -> &'static str {
        match self {
            Self::Fermentable => "kg",
            Self::Hop => "g",
            Self::Yeast | Self::Other => "",
        }
    }
    pub(crate) fn add_button_id(self) -> String {
        format!("{}-add", self.prefix())
    }
}

/// Every editable control of an addition row, across all four groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Field {
    Name,
    Producer,
    Kind,
    Stage,
    Unit,
    Amount,
    Yield,
    Colour,
    BoilMinutes,
    ContactMinutes,
    Temperature,
    Alpha,
    Attenuation,
}

impl Field {
    fn is_percent(self) -> bool {
        matches!(self, Self::Yield | Self::Alpha | Self::Attenuation)
    }
    /// DOM-id suffix. Stable across releases: diagnostics and tests use them.
    fn dom_suffix(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Producer => "producer",
            Self::Kind => "type",
            Self::Stage => "stage",
            Self::Unit => "unit",
            Self::Amount => "amount",
            Self::Yield | Self::Alpha => "yield-alpha",
            Self::Colour | Self::ContactMinutes => "colour-time",
            Self::BoilMinutes | Self::Temperature => "stage-detail",
            Self::Attenuation => "attenuation",
        }
    }
    /// Raw-input path suffix, matching the diagnostic registry's row paths.
    fn path_suffix(self, category: AdditionCategory) -> &'static str {
        match (category, self) {
            (AdditionCategory::Fermentable, Self::Amount) => "amount_kg",
            (AdditionCategory::Fermentable, Self::Yield) => "specification.yield_fraction",
            (AdditionCategory::Fermentable, Self::Colour) => "specification.colour_ebc",
            (AdditionCategory::Fermentable, Self::BoilMinutes) => "addition.time_minutes",
            (AdditionCategory::Fermentable, Self::Stage) => "addition.stage",
            (AdditionCategory::Fermentable, Self::Kind) => "specification.type",
            (AdditionCategory::Hop, Self::Stage) => "stage",
            (AdditionCategory::Hop, Self::Amount) => "amount_g",
            (AdditionCategory::Hop, Self::Alpha) => "specification.alpha_fraction",
            (AdditionCategory::Hop, Self::ContactMinutes) => "contact_minutes",
            (AdditionCategory::Hop, Self::Temperature) => "temperature_c",
            (_, Self::Amount) => "amount",
            (_, Self::Attenuation) => "attenuation",
            _ => "",
        }
    }
}

pub(crate) fn dom_id(category: AdditionCategory, row_id: &str, field: Field) -> String {
    format!("{}-{row_id}-{}", category.prefix(), field.dom_suffix())
}
fn remove_button_id(category: AdditionCategory, row_id: &str) -> String {
    format!("{}-{row_id}-remove", category.prefix())
}
pub(crate) fn raw_path(category: AdditionCategory, row_id: &str, field: Field) -> String {
    format!("{}[{row_id}].{}", category.prefix(), field.path_suffix(category))
}
/// Structured-diagnostic path for fields whose service path differs from the raw path.
fn service_path(category: AdditionCategory, row_id: &str, field: Field) -> Option<String> {
    match (category, field) {
        (AdditionCategory::Yeast | AdditionCategory::Other, Field::Amount) => {
            Some(format!("ingredients[{row_id}].amount"))
        }
        (AdditionCategory::Yeast, Field::Attenuation) => {
            Some(format!("yeast[{row_id}].specification.attenuation_fraction"))
        }
        _ => None,
    }
}

/// Stable opening map for the diagnostic registry (KRA-34): the DOM id of the
/// control a structured row path addresses and whether it sits in the row's
/// collapsed details. Paths that address no editable row control return `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldOpening {
    pub dom_id: String,
    pub in_details: bool,
    /// Visible control label, e.g. "Alpha acid".
    pub label: &'static str,
    /// Identity of the row the control belongs to, including its position.
    pub row_label: String,
}

pub fn diagnostic_field(doc: &RecipeDocumentV1, path: &str) -> Option<FieldOpening> {
    let (head, rest) = path.split_once('[')?;
    let (row_id, tail) = rest.split_once("].")?;
    let category = match head {
        "ingredients" => [AdditionCategory::Yeast, AdditionCategory::Other]
            .into_iter()
            .find(|category| row_ids(doc, *category).iter().any(|id| id == row_id))?,
        _ => category_for_head(head)?,
    };
    if !row_ids(doc, category).iter().any(|id| id == row_id) {
        return None;
    }
    let field = diagnostic_row_field(category, tail)?;
    Some(FieldOpening {
        dom_id: dom_id(category, row_id, field),
        in_details: field_in_details(category, field),
        label: field_label(category, field).0,
        row_label: row_label(doc, category, row_id),
    })
}

/// Which addition group a structured path head names. `ingredients` is shared by
/// yeast and other additions, so it is resolved against the document instead.
pub(crate) fn category_for_head(head: &str) -> Option<AdditionCategory> {
    Some(match head {
        "fermentables" => AdditionCategory::Fermentable,
        "hops" => AdditionCategory::Hop,
        "yeast" => AdditionCategory::Yeast,
        "other_ingredients" => AdditionCategory::Other,
        _ => return None,
    })
}

/// The row control a structured field path (the part after `[row_id].`) addresses.
pub(crate) fn diagnostic_row_field(category: AdditionCategory, tail: &str) -> Option<Field> {
    Some(match (category, tail) {
        (AdditionCategory::Fermentable, "amount_kg") => Field::Amount,
        (AdditionCategory::Fermentable, "specification.name") => Field::Name,
        (AdditionCategory::Fermentable, "specification.producer") => Field::Producer,
        (AdditionCategory::Fermentable, "specification.type") => Field::Kind,
        (AdditionCategory::Fermentable, "specification.yield_fraction") => Field::Yield,
        (AdditionCategory::Fermentable, "specification.colour_ebc") => Field::Colour,
        (AdditionCategory::Fermentable, "addition.stage") => Field::Stage,
        (AdditionCategory::Fermentable, "addition.time_minutes") => Field::BoilMinutes,
        (AdditionCategory::Hop, "amount_g") => Field::Amount,
        (AdditionCategory::Hop, "specification.name") => Field::Name,
        (AdditionCategory::Hop, "specification.producer") => Field::Producer,
        (AdditionCategory::Hop, "specification.alpha_fraction") => Field::Alpha,
        (AdditionCategory::Hop, "stage") => Field::Stage,
        (AdditionCategory::Hop, "contact_minutes") => Field::ContactMinutes,
        (AdditionCategory::Hop, "temperature_c") => Field::Temperature,
        (AdditionCategory::Yeast | AdditionCategory::Other, "amount") => Field::Amount,
        (AdditionCategory::Yeast | AdditionCategory::Other, "unit") => Field::Unit,
        (AdditionCategory::Yeast | AdditionCategory::Other, "specification.name") => Field::Name,
        (AdditionCategory::Yeast | AdditionCategory::Other, "specification.producer") => {
            Field::Producer
        }
        (AdditionCategory::Yeast, "specification.attenuation_fraction") => Field::Attenuation,
        _ => return None,
    })
}

/// Routine identity, amount and schedule controls are always visible.
fn field_in_details(category: AdditionCategory, field: Field) -> bool {
    !matches!(
        (category, field),
        (_, Field::Name | Field::Amount)
            | (AdditionCategory::Fermentable, Field::Stage)
            | (AdditionCategory::Hop, Field::Stage | Field::ContactMinutes)
            | (AdditionCategory::Yeast | AdditionCategory::Other, Field::Unit)
    )
}

// ---- exact percent boundary -------------------------------------------------

/// Moves a plain decimal string's point without binary arithmetic, so a stored
/// fraction displays as the percent a brewer typed (0.07 → "7", never
/// 7.000000000000001) and a typed percent becomes the nearest fraction.
fn shift_decimal(text: &str, places: i32) -> Option<String> {
    if text.contains(['e', 'E']) {
        return None;
    }
    let (negative, unsigned) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (int, frac) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if (int.is_empty() && frac.is_empty()) || !int.chars().chain(frac.chars()).all(|c| c.is_ascii_digit()) {
        return None;
    }
    let digits = format!("{int}{frac}");
    let point = i32::try_from(int.len()).ok()?.checked_add(places)?;
    let (whole, fraction) = if point <= 0 {
        (
            "0".to_owned(),
            format!("{}{digits}", "0".repeat(usize::try_from(-point).ok()?)),
        )
    } else if usize::try_from(point).ok()? >= digits.len() {
        (
            format!("{digits}{}", "0".repeat(usize::try_from(point).ok()? - digits.len())),
            String::new(),
        )
    } else {
        let split = usize::try_from(point).ok()?;
        (digits[..split].to_owned(), digits[split..].to_owned())
    };
    let whole = whole.trim_start_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    let fraction = fraction.trim_end_matches('0');
    let magnitude = if fraction.is_empty() { whole.to_owned() } else { format!("{whole}.{fraction}") };
    Some(if negative && magnitude != "0" { format!("-{magnitude}") } else { magnitude })
}

pub(crate) fn fraction_to_percent(value: f64) -> f64 {
    shift_decimal(&value.to_string(), 2)
        .and_then(|text| text.parse().ok())
        .unwrap_or(value * 100.0)
}
pub(crate) fn percent_to_fraction(value: f64) -> f64 {
    shift_decimal(&value.to_string(), -2)
        .and_then(|text| text.parse().ok())
        .unwrap_or(value / 100.0)
}

// ---- document access --------------------------------------------------------

pub(crate) fn row_ids(doc: &RecipeDocumentV1, category: AdditionCategory) -> Vec<String> {
    match category {
        AdditionCategory::Fermentable => doc.fermentables.iter().map(|r| r.row_id.clone()).collect(),
        AdditionCategory::Hop => doc.hops.iter().map(|r| r.row_id.clone()).collect(),
        AdditionCategory::Yeast => doc.yeast.iter().map(|r| r.row_id.clone()).collect(),
        AdditionCategory::Other => doc.other_ingredients.iter().map(|r| r.row_id.clone()).collect(),
    }
}

fn ingredient_rows(doc: &RecipeDocumentV1, category: AdditionCategory) -> &Vec<IngredientAddition> {
    match category {
        AdditionCategory::Yeast => &doc.yeast,
        _ => &doc.other_ingredients,
    }
}
fn ingredient_rows_mut(
    doc: &mut RecipeDocumentV1,
    category: AdditionCategory,
) -> &mut Vec<IngredientAddition> {
    match category {
        AdditionCategory::Yeast => &mut doc.yeast,
        _ => &mut doc.other_ingredients,
    }
}

fn row_text(doc: &RecipeDocumentV1, category: AdditionCategory, id: &str, field: Field) -> String {
    let value = match category {
        AdditionCategory::Fermentable => doc.fermentables.iter().find(|r| r.row_id == id).and_then(|r| match field {
            Field::Name => r.specification.name.clone(),
            Field::Producer => r.specification.producer.clone(),
            Field::Kind => r.specification.kind.clone(),
            Field::Stage => Some(r.addition.stage.clone()),
            _ => None,
        }),
        AdditionCategory::Hop => doc.hops.iter().find(|r| r.row_id == id).and_then(|r| match field {
            Field::Name => r.specification.name.clone(),
            Field::Producer => r.specification.producer.clone(),
            Field::Stage => Some(r.stage.clone()),
            _ => None,
        }),
        AdditionCategory::Yeast | AdditionCategory::Other => ingredient_rows(doc, category).iter().find(|r| r.row_id == id).and_then(|r| match field {
            Field::Name => r.specification.name.clone(),
            Field::Producer => r.specification.producer.clone(),
            Field::Unit => r.unit.clone(),
            _ => None,
        }),
    };
    value.unwrap_or_default()
}

fn set_row_text(doc: &mut RecipeDocumentV1, category: AdditionCategory, id: &str, field: Field, value: String) {
    let optional = (!value.is_empty()).then(|| value.clone());
    match category {
        AdditionCategory::Fermentable => if let Some(r) = doc.fermentables.iter_mut().find(|r| r.row_id == id) {
            match field {
                Field::Name => r.specification.name = optional,
                Field::Producer => r.specification.producer = optional,
                Field::Kind => r.specification.kind = optional,
                Field::Stage => r.addition.stage = value,
                _ => {}
            }
        },
        AdditionCategory::Hop => if let Some(r) = doc.hops.iter_mut().find(|r| r.row_id == id) {
            match field {
                Field::Name => r.specification.name = optional,
                Field::Producer => r.specification.producer = optional,
                Field::Stage => r.stage = value,
                _ => {}
            }
        },
        AdditionCategory::Yeast | AdditionCategory::Other => if let Some(r) = ingredient_rows_mut(doc, category).iter_mut().find(|r| r.row_id == id) {
            match field {
                Field::Name => r.specification.name = optional,
                Field::Producer => r.specification.producer = optional,
                Field::Unit => r.unit = optional,
                _ => {}
            }
        },
    }
}

/// Native-unit value (fractions stay fractions here).
fn row_number(doc: &RecipeDocumentV1, category: AdditionCategory, id: &str, field: Field) -> Option<f64> {
    match category {
        AdditionCategory::Fermentable => doc.fermentables.iter().find(|r| r.row_id == id).and_then(|r| match field {
            Field::Amount => r.amount_kg,
            Field::Yield => r.specification.yield_fraction,
            Field::Colour => r.specification.colour_ebc,
            Field::BoilMinutes => r.addition.time_minutes,
            _ => None,
        }),
        AdditionCategory::Hop => doc.hops.iter().find(|r| r.row_id == id).and_then(|r| match field {
            Field::Amount => r.amount_g,
            Field::Alpha => r.specification.alpha_fraction,
            Field::ContactMinutes => r.contact_minutes,
            Field::Temperature => r.temperature_c,
            _ => None,
        }),
        AdditionCategory::Yeast | AdditionCategory::Other => ingredient_rows(doc, category).iter().find(|r| r.row_id == id).and_then(|r| match field {
            Field::Amount => r.amount,
            Field::Attenuation => r.specification.attenuation_fraction,
            _ => None,
        }),
    }
}

fn set_row_number(doc: &mut RecipeDocumentV1, category: AdditionCategory, id: &str, field: Field, value: Option<f64>) {
    match category {
        AdditionCategory::Fermentable => if let Some(r) = doc.fermentables.iter_mut().find(|r| r.row_id == id) {
            match field {
                Field::Amount => r.amount_kg = value,
                Field::Yield => r.specification.yield_fraction = value,
                Field::Colour => r.specification.colour_ebc = value,
                Field::BoilMinutes => r.addition.time_minutes = value,
                _ => {}
            }
        },
        AdditionCategory::Hop => if let Some(r) = doc.hops.iter_mut().find(|r| r.row_id == id) {
            match field {
                Field::Amount => r.amount_g = value,
                Field::Alpha => r.specification.alpha_fraction = value,
                Field::ContactMinutes => r.contact_minutes = value,
                Field::Temperature => r.temperature_c = value,
                _ => {}
            }
        },
        AdditionCategory::Yeast | AdditionCategory::Other => if let Some(r) = ingredient_rows_mut(doc, category).iter_mut().find(|r| r.row_id == id) {
            match field {
                Field::Amount => r.amount = value,
                Field::Attenuation => r.specification.attenuation_fraction = value,
                _ => {}
            }
        },
    }
}

fn stage_label(category: AdditionCategory, stage: &str) -> String {
    match (category, stage) {
        (_, "") => String::new(),
        (AdditionCategory::Fermentable, "mash") => "Mash".into(),
        (_, "boil") => "Boil".into(),
        (AdditionCategory::Hop, "whirlpool") => "Whirlpool".into(),
        (AdditionCategory::Hop, "dry_hop") => "Dry hop".into(),
        (_, other) => other.to_owned(),
    }
}

fn display_name(doc: &RecipeDocumentV1, category: AdditionCategory, id: &str) -> String {
    let name = row_text(doc, category, id, Field::Name);
    if name.trim().is_empty() { format!("unnamed {}", category.noun()) } else { name }
}

/// Distinguishes duplicate names by use/stage, time and position (ACC-001).
/// The name leads so "Remove {label}" still begins with the ingredient's name.
pub(crate) fn row_label(doc: &RecipeDocumentV1, category: AdditionCategory, id: &str) -> String {
    let ids = row_ids(doc, category);
    let Some(position) = ids.iter().position(|row| row == id) else { return String::new() };
    let name = row_text(doc, category, id, Field::Name);
    let mut parts = if name.trim().is_empty() {
        vec![format!("Unnamed {}", category.noun())]
    } else {
        vec![name, category.noun().to_owned()]
    };
    let stage = stage_label(category, &row_text(doc, category, id, Field::Stage));
    if !stage.is_empty() {
        parts.push(stage);
    }
    if category == AdditionCategory::Hop {
        if let Some(minutes) = row_number(doc, category, id, Field::ContactMinutes) {
            parts.push(format!("{minutes} min"));
        }
    }
    parts.push(format!("{} of {}", position + 1, ids.len()));
    parts.join(", ")
}

fn row_description(doc: &RecipeDocumentV1, category: AdditionCategory, id: &str, draft: &RecipeDraft) -> String {
    let mut parts = Vec::new();
    let producer = row_text(doc, category, id, Field::Producer);
    if !producer.is_empty() {
        parts.push(producer);
    }
    let valid = |field: Field| draft.number_error(&raw_path(category, id, field)).is_none();
    match category {
        AdditionCategory::Fermentable => {
            let kind = row_text(doc, category, id, Field::Kind);
            if !kind.is_empty() {
                parts.push(kind);
            }
            if valid(Field::Colour) {
                if let Some(value) = row_number(doc, category, id, Field::Colour) {
                    let formatted = crate::components::brew_history_presentation::format_measurement_value(value, "ebc");
                    parts.push(if value != 0.0 && formatted == "0 EBC" { format!("{value} EBC") } else { format!("≈ {formatted}") });
                }
            }
        }
        AdditionCategory::Hop => {
            if valid(Field::Alpha) {
                if let Some(value) = row_number(doc, category, id, Field::Alpha) {
                    parts.push(format!("Alpha {}%", fraction_to_percent(value)));
                }
            }
        }
        AdditionCategory::Yeast => {
            if valid(Field::Attenuation) {
                if let Some(value) = row_number(doc, category, id, Field::Attenuation) {
                    parts.push(format!("Yeast attenuation {}% (informational)", fraction_to_percent(value)));
                }
            }
        }
        AdditionCategory::Other => {}
    }
    parts.join(" · ")
}

// ---- manual Add task: pure building ------------------------------------------

/// Text exactly as entered in a manual Add form.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ManualValues {
    pub name: String,
    pub producer: String,
    pub unit: String,
    pub amount: String,
    pub stage: String,
    pub contact_minutes: String,
    pub alpha_percent: String,
    pub temperature: String,
    pub attenuation_percent: String,
}

pub(crate) enum NewRow {
    Hop(HopAddition),
    Ingredient(IngredientAddition),
}

/// Native numbers are stored as numbers; the text the brewer typed is kept next
/// to them so exact entry survives Add (INT-001).
pub(crate) fn raw_entries(category: AdditionCategory, row_id: &str, values: &ManualValues) -> Vec<(String, String)> {
    let fields: &[(Field, &str)] = match category {
        AdditionCategory::Hop => &[
            (Field::Amount, &values.amount),
            (Field::ContactMinutes, &values.contact_minutes),
            (Field::Alpha, &values.alpha_percent),
            (Field::Temperature, &values.temperature),
        ],
        AdditionCategory::Yeast => &[(Field::Amount, &values.amount), (Field::Attenuation, &values.attenuation_percent)],
        AdditionCategory::Other => &[(Field::Amount, &values.amount)],
        AdditionCategory::Fermentable => &[],
    };
    fields
        .iter()
        .filter(|(_, text)| !text.trim().is_empty())
        .map(|(field, text)| (raw_path(category, row_id, *field), (*text).to_owned()))
        .collect()
}

/// `Ok(None)` is absence; a malformed entry is an error, never zero.
pub(crate) fn parse_optional(text: &str) -> Result<Option<f64>, ()> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    text.parse::<f64>().ok().filter(|n| n.is_finite()).map(Some).ok_or(())
}

fn optional_text(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_owned())
}

/// Validates every number before any row exists; the error lists the DOM
/// suffixes of the malformed fields so the first one can take focus.
pub(crate) fn build_row(
    category: AdditionCategory,
    row_id: String,
    values: &ManualValues,
) -> Result<(NewRow, Vec<(String, String)>), Vec<Field>> {
    let raw = raw_entries(category, &row_id, values);
    let mut bad = Vec::new();
    let mut number = |field: Field, text: &str| match parse_optional(text) {
        Ok(value) => value,
        Err(()) => {
            bad.push(field);
            None
        }
    };
    let amount = number(Field::Amount, &values.amount);
    let contact = number(Field::ContactMinutes, &values.contact_minutes);
    let alpha = number(Field::Alpha, &values.alpha_percent);
    let temperature = number(Field::Temperature, &values.temperature);
    let attenuation = number(Field::Attenuation, &values.attenuation_percent);
    if !bad.is_empty() {
        return Err(bad);
    }
    let row = match category {
        AdditionCategory::Hop => NewRow::Hop(HopAddition {
            row_id,
            amount_g: amount,
            stage: values.stage.clone(),
            contact_minutes: contact,
            temperature_c: temperature,
            specification: HopSpecification {
                name: optional_text(&values.name),
                producer: optional_text(&values.producer),
                alpha_fraction: alpha.map(percent_to_fraction),
            },
            source: None,
        }),
        AdditionCategory::Yeast | AdditionCategory::Other => NewRow::Ingredient(IngredientAddition {
            row_id,
            amount,
            unit: optional_text(&values.unit),
            specification: IngredientSpecification {
                name: optional_text(&values.name),
                producer: optional_text(&values.producer),
                attenuation_fraction: (category == AdditionCategory::Yeast)
                    .then(|| attenuation.map(percent_to_fraction))
                    .flatten(),
                extensions: Default::default(),
            },
            source: None,
        }),
        AdditionCategory::Fermentable => return Err(Vec::new()),
    };
    Ok((row, raw))
}

fn push_new_row(doc: &mut RecipeDocumentV1, category: AdditionCategory, row: NewRow) {
    match row {
        NewRow::Hop(row) => doc.hops.push(row),
        NewRow::Ingredient(row) => ingredient_rows_mut(doc, category).push(row),
    }
}

// ---- scoped removal and Undo -------------------------------------------------

/// Removals kept per group for Undo.
pub(crate) const UNDO_DEPTH: usize = 20;

#[derive(Clone, Debug, PartialEq)]
enum RowPayload {
    Fermentable(FermentableAddition),
    Hop(HopAddition),
    Ingredient(IngredientAddition),
}

/// Everything needed to put one removed row back exactly where it was.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UndoRemoval {
    category: AdditionCategory,
    id: String,
    index: usize,
    name: String,
    epoch: u64,
    payload: RowPayload,
    inputs: RemovedInputs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Restore {
    Restored,
    AlreadyPresent,
    /// The document was replaced (reload/remote revision) after the removal.
    Stale,
}

pub(crate) fn remove_row(draft: &mut RecipeDraft, category: AdditionCategory, id: &str) -> Option<UndoRemoval> {
    let doc = draft.document();
    let index = row_ids(doc, category).iter().position(|row| row == id)?;
    let name = display_name(doc, category, id);
    let epoch = draft.epoch();
    let payload = match category {
        AdditionCategory::Fermentable => RowPayload::Fermentable(doc.fermentables[index].clone()),
        AdditionCategory::Hop => RowPayload::Hop(doc.hops[index].clone()),
        AdditionCategory::Yeast | AdditionCategory::Other => {
            RowPayload::Ingredient(ingredient_rows(doc, category)[index].clone())
        }
    };
    let prefix = format!("{}[{id}].", category.prefix());
    let owned = id.to_owned();
    let inputs = draft.edit_removing(&prefix, |doc| match category {
        AdditionCategory::Fermentable => doc.fermentables.retain(|r| r.row_id != owned),
        AdditionCategory::Hop => doc.hops.retain(|r| r.row_id != owned),
        AdditionCategory::Yeast | AdditionCategory::Other => {
            ingredient_rows_mut(doc, category).retain(|r| r.row_id != owned)
        }
    });
    Some(UndoRemoval { category, id: owned, index, name, epoch, payload, inputs })
}

/// Restores ID, position and every raw string. A row that has since reappeared
/// under the same ID is never duplicated.
pub(crate) fn restore_row(draft: &mut RecipeDraft, undo: UndoRemoval) -> Restore {
    if undo.epoch != draft.epoch() {
        return Restore::Stale;
    }
    if row_ids(draft.document(), undo.category).contains(&undo.id) {
        return Restore::AlreadyPresent;
    }
    let UndoRemoval { category, index, payload, inputs, .. } = undo;
    draft.edit_restoring(inputs, |doc| match payload {
        RowPayload::Fermentable(row) => {
            let at = index.min(doc.fermentables.len());
            doc.fermentables.insert(at, row);
        }
        RowPayload::Hop(row) => {
            let at = index.min(doc.hops.len());
            doc.hops.insert(at, row);
        }
        RowPayload::Ingredient(row) => {
            let rows = ingredient_rows_mut(doc, category);
            let at = index.min(rows.len());
            rows.insert(at, row);
        }
    });
    Restore::Restored
}

// ---- components --------------------------------------------------------------

/// The four ingredient groups in recipe order. The only seam the recipe editor uses.
#[component]
pub fn IngredientAdditions(
    draft: RwSignal<RecipeDraft>,
    identity: RwSignal<OperationIntent>,
    context_visible: RwSignal<bool>,
) -> impl IntoView {
    view! {
        {AdditionCategory::ALL.into_iter().map(|category| view! {
            <AdditionPanel category=category draft=draft identity=identity context_visible=context_visible/>
        }).collect_view()}
    }
}

#[component]
fn AdditionPanel(
    category: AdditionCategory,
    draft: RwSignal<RecipeDraft>,
    identity: RwSignal<OperationIntent>,
    context_visible: RwSignal<bool>,
) -> impl IntoView {
    let open = RwSignal::new(false);
    let status = RwSignal::new(String::new());
    // Most recent removal last; each entry keeps its own row, position and raw text.
    let undo = RwSignal::new(Vec::<UndoRemoval>::new());
    // One concise settled outcome. The label carries position, so an identical
    // repeat (same name, same use) is still a different announcement.
    let committed = Callback::new(move |row_id: String| {
        let label = draft.with_untracked(|d| row_label(d.document(), category, &row_id));
        status.set(format!("Added {label}."));
        crate::utils::focus::focus_first_available(vec![dom_id(category, &row_id, Field::Amount)]);
    });
    let add_manual = Callback::new(move |(row_id, row, raw): (String, NewRow, Vec<(String, String)>)| {
        draft.update(|d| {
            d.edit(|doc| push_new_row(doc, category, row));
            d.retain_raw_numbers(raw);
        });
        committed.run(row_id);
    });
    let add_fermentable = Callback::new(move |row: FermentableAddition| {
        let row_id = row.row_id.clone();
        draft.update(|d| d.edit(|doc| doc.fermentables.push(row)));
        committed.run(row_id);
    });
    let remove = Callback::new(move |id: String| {
        let ids = draft.with_untracked(|d| row_ids(d.document(), category));
        let position = ids.iter().position(|row| *row == id);
        let label = draft.with_untracked(|d| row_label(d.document(), category, &id));
        let Some(removed) = draft.try_update(|d| remove_row(d, category, &id)).flatten() else { return };
        let mut successors = Vec::new();
        if let Some(position) = position {
            if let Some(next) = ids.get(position + 1) {
                successors.push(remove_button_id(category, next));
            }
            if position > 0 {
                successors.push(remove_button_id(category, &ids[position - 1]));
            }
        }
        successors.push(category.add_button_id());
        status.set(format!("Removed {label}."));
        undo.update(|stack| {
            stack.push(removed);
            if stack.len() > UNDO_DEPTH {
                stack.remove(0);
            }
        });
        crate::utils::focus::focus_first_available(successors);
    });
    let restore = move |_| {
        let mut token = None;
        undo.update(|stack| token = stack.pop());
        let Some(token) = token else { return };
        let (id, name) = (token.id.clone(), token.name.clone());
        match draft.try_update(|d| restore_row(d, token)) {
            Some(Restore::Restored) => {
                let label = draft.with_untracked(|d| row_label(d.document(), category, &id));
                status.set(format!("Restored {label}."));
                crate::utils::focus::focus_first_available(vec![dom_id(category, &id, Field::Name)]);
            }
            Some(Restore::AlreadyPresent) => status.set(format!("{name} is already in the recipe.")),
            Some(Restore::Stale) | None => {
                // Every older token belongs to a replaced document too.
                undo.set(Vec::new());
                status.set(format!("{name} can no longer be restored because the recipe was saved or reloaded."));
            }
        }
    };
    let (title, description) = match category {
        AdditionCategory::Fermentable => ("Fermentables", "Specify the grain, extract or sugar copied into this recipe."),
        AdditionCategory::Hop => ("Hops", "Record each addition and its contact time."),
        AdditionCategory::Yeast => ("Yeast", "Record the yeast and its informational attenuation."),
        AdditionCategory::Other => ("Other ingredients", "Record any other addition with its amount and unit."),
    };
    view! {
        <BreweryPanel title=title description=description>
            <For each=move || draft.with(|d| row_ids(d.document(), category)) key=|id| id.clone() children=move |id| view! { <AdditionRow category=category draft=draft id=id on_remove=remove/> } />
            <p class="note-caption" role="status">{move || status.get()}</p>
            <BreweryActionGroup>
                <Button id=category.add_button_id() variant=ButtonVariant::Secondary on:click=move |_| open.set(true)>{category.opener()}</Button>
                {move || undo.with(|stack| stack.last().map(|token| format!("Undo removal of {}", token.name))).map(|label| view! {
                    <Button variant=ButtonVariant::Ghost aria_label=label on:click=restore>"Undo"</Button>
                })}
            </BreweryActionGroup>
            {match category {
                AdditionCategory::Fermentable => view! { <CataloguePicker open=open identity=identity context_visible=context_visible on_add=add_fermentable/> }.into_any(),
                _ => view! { <ManualAdditionDialog category=category open=open on_add=add_manual/> }.into_any(),
            }}
        </BreweryPanel>
    }
}

#[component]
fn AdditionRow(
    category: AdditionCategory,
    draft: RwSignal<RecipeDraft>,
    id: String,
    on_remove: Callback<String>,
) -> impl IntoView {
    let label_id = id.clone();
    let label = Signal::derive(move || draft.with(|d| row_label(d.document(), category, &label_id)));
    let description_id = id.clone();
    let description = Signal::derive(move || draft.with(|d| row_description(d.document(), category, &description_id, d)));
    let schedule = match category {
        AdditionCategory::Fermentable => text_field(category, draft, &id, Field::Stage),
        AdditionCategory::Hop => {
            let (stage, contact) = (text_field(category, draft, &id, Field::Stage), number_field(category, draft, &id, Field::ContactMinutes));
            view! { <BreweryFieldRow>{stage}{contact}</BreweryFieldRow> }.into_any()
        }
        AdditionCategory::Yeast | AdditionCategory::Other => text_field(category, draft, &id, Field::Unit),
    };
    let secondary = match category {
        AdditionCategory::Fermentable => view! {
            {text_field(category, draft, &id, Field::Kind)}
            {number_field(category, draft, &id, Field::Yield)}
            {number_field(category, draft, &id, Field::Colour)}
            {number_field(category, draft, &id, Field::BoilMinutes)}
        }.into_any(),
        AdditionCategory::Hop => view! {
            {number_field(category, draft, &id, Field::Alpha)}
            {number_field(category, draft, &id, Field::Temperature)}
        }.into_any(),
        AdditionCategory::Yeast => number_field(category, draft, &id, Field::Attenuation),
        AdditionCategory::Other => ().into_any(),
    };
    let details = view! { <div class="recipe-editor__fields">{text_field(category, draft, &id, Field::Producer)}{secondary}</div> }.into_any();
    let remove_id = id.clone();
    let remove = Callback::new({
        let id = id.clone();
        move |()| on_remove.run(id.clone())
    });
    let name_field = text_field(category, draft, &id, Field::Name);
    let amount_field = number_field(category, draft, &id, Field::Amount);
    view! { <BreweryCompactIngredientRow label=label description=description name_field=name_field amount_field=amount_field schedule=schedule details=details on_remove=remove remove_id=remove_button_id(category, &remove_id)/> }
}

fn text_field(category: AdditionCategory, draft: RwSignal<RecipeDraft>, id: &str, field: Field) -> AnyView {
    let id = id.to_owned();
    view! { <RowTextField category=category draft=draft id=id field=field/> }.into_any()
}
fn number_field(category: AdditionCategory, draft: RwSignal<RecipeDraft>, id: &str, field: Field) -> AnyView {
    let id = id.to_owned();
    view! { <RowNumberField category=category draft=draft id=id field=field/> }.into_any()
}

pub(crate) fn field_label(category: AdditionCategory, field: Field) -> (&'static str, &'static str) {
    match (category, field) {
        (_, Field::Name) => ("Name", ""),
        (_, Field::Producer) => ("Producer", ""),
        (_, Field::Kind) => ("Type", ""),
        (AdditionCategory::Hop, Field::Stage) => ("Use", ""),
        (_, Field::Stage) => ("Stage", ""),
        (_, Field::Unit) => ("Unit", ""),
        (_, Field::Amount) => ("Amount", category.amount_unit()),
        (_, Field::Yield) => ("Yield", "%"),
        (_, Field::Colour) => ("Colour", "EBC"),
        (_, Field::BoilMinutes) => ("Boil addition time", "min"),
        (_, Field::ContactMinutes) => ("Contact time", "min"),
        (_, Field::Temperature) => ("Whirlpool temperature", "°C"),
        (_, Field::Alpha) => ("Alpha acid", "%"),
        (_, Field::Attenuation) => ("Yeast attenuation (informational)", "%"),
    }
}

fn stage_options(category: AdditionCategory) -> Vec<(String, String)> {
    let mut options = vec![(String::new(), if category == AdditionCategory::Hop { "Select a use" } else { "Select a stage" }.to_owned())];
    let choices: &[(&str, &str)] = match category {
        AdditionCategory::Hop => &[("boil", "Boil"), ("whirlpool", "Whirlpool"), ("dry_hop", "Dry hop")],
        _ => &[("mash", "Mash"), ("boil", "Boil")],
    };
    options.extend(choices.iter().map(|(v, l)| ((*v).to_owned(), (*l).to_owned())));
    options
}

#[component]
fn RowTextField(
    category: AdditionCategory,
    draft: RwSignal<RecipeDraft>,
    id: String,
    field: Field,
) -> impl IntoView {
    let input_id = dom_id(category, &id, field);
    let (label, _) = field_label(category, field);
    let read_id = id.clone();
    let value = Signal::derive(move || draft.with(|d| row_text(d.document(), category, &read_id, field)));
    let select_path = raw_path(category, &id, field);
    let error = Signal::derive({
        let path = select_path.clone();
        move || draft.with(|d| d.validation_error(&path).map(str::to_owned))
    });
    let hint = Signal::derive(move || draft.with(|d| d.validation_hint(&select_path).map(str::to_owned)));
    let change = Callback::new(move |text: String| {
        let id = id.clone();
        draft.update(|d| d.edit(|doc| set_row_text(doc, category, &id, field, text)));
    });
    match field {
        Field::Kind => view! { <BreweryDraftSelect id=input_id label=label value=value on_change=change error=error hint=hint options=vec![("".into(), "Select a type".into()), ("grain".into(), "Grain".into()), ("extract".into(), "Extract".into()), ("sugar".into(), "Sugar".into())]/> }.into_any(),
        Field::Stage => view! { <BreweryDraftSelect id=input_id label=label value=value on_change=change error=error hint=hint options=stage_options(category)/> }.into_any(),
        _ => view! { <BreweryDraftField id=input_id label=label value=value on_change=change/> }.into_any(),
    }
}

#[component]
fn RowNumberField(
    category: AdditionCategory,
    draft: RwSignal<RecipeDraft>,
    id: String,
    field: Field,
) -> impl IntoView {
    let path = raw_path(category, &id, field);
    let service = service_path(category, &id, field);
    let input_id = dom_id(category, &id, field);
    let (label, unit) = field_label(category, field);
    let percent = field.is_percent();
    let read_id = id.clone();
    let read_path = path.clone();
    let error_path = path.clone();
    let blur_path = path.clone();
    let blur = Callback::new(move |()| draft.update(|d| d.settle_number(&blur_path)));
    let value = Signal::derive(move || {
        draft.with(|d| {
            d.number_text(
                &read_path,
                row_number(d.document(), category, &read_id, field)
                    .map(|n| if percent { fraction_to_percent(n) } else { n }),
            )
        })
    });
    let error = Signal::derive(move || {
        draft.with(|d| {
            d.number_error(&error_path)
                .or_else(|| service.as_deref().and_then(|path| d.validation_error(path)))
                .map(str::to_owned)
        })
    });
    let hint_path = path.clone();
    let hint_service = service_path(category, &id, field);
    let hint = Signal::derive(move || {
        draft.with(|d| {
            d.validation_hint(&hint_path)
                .or_else(|| hint_service.as_deref().and_then(|path| d.validation_hint(path)))
                .map(str::to_owned)
        })
    });
    let change = Callback::new(move |text: String| {
        let id = id.clone();
        let path = path.clone();
        draft.update(|d| {
            d.edit_number(&path, text, |doc, n| {
                set_row_number(doc, category, &id, field, n.map(|v| if percent { percent_to_fraction(v) } else { v }))
            })
        });
    });
    view! { <BreweryDraftField id=input_id label=label unit=unit value=value error=error hint=hint on_change=change on_blur=blur numeric=true/> }
}

/// Truthful manual specification task for groups without a supported catalogue.
#[component]
fn ManualAdditionDialog(
    category: AdditionCategory,
    open: RwSignal<bool>,
    on_add: Callback<(String, NewRow, Vec<(String, String)>)>,
) -> impl IntoView {
    let values = RwSignal::new(ManualValues::default());
    let settled = RwSignal::new(false);
    let failure = RwSignal::new(None::<String>);
    let was_open = RwSignal::new(false);
    Effect::new(move |_| {
        if open.get() {
            was_open.set(true);
            crate::utils::focus::focus_first_available(vec![format!("manual-{}-name", category.prefix())]);
        } else if was_open.get_untracked() {
            was_open.set(false);
            values.set(ManualValues::default());
            settled.set(false);
            failure.set(None);
        }
    });
    let text = move |select: fn(&ManualValues) -> String| Signal::derive(move || values.with(select));
    let set = move |apply: fn(&mut ManualValues, String)| Callback::new(move |value: String| values.update(|v| apply(v, value)));
    let number_error = move |select: fn(&ManualValues) -> String| {
        Signal::derive(move || {
            (settled.get() && parse_optional(&values.with(select)).is_err()).then(|| "Enter a number".to_owned())
        })
    };
    let blur = Callback::new(move |()| settled.set(true));
    let field_id = move |suffix: &str| format!("manual-{}-{suffix}", category.prefix());
    let commit = move || {
        settled.set(true);
        let Some(row_id) = crate::utils::request_id::new_request_id() else {
            failure.set(Some("A stable row identity could not be created, so nothing was added.".to_owned()));
            return;
        };
        match build_row(category, row_id.clone(), &values.get_untracked()) {
            Ok((row, raw)) => {
                failure.set(None);
                on_add.run((row_id, row, raw));
                open.set(false);
            }
            Err(bad) => {
                let first = bad.first().copied().unwrap_or(Field::Amount);
                crate::utils::focus::focus_first_available(vec![format!("manual-{}-{}", category.prefix(), manual_suffix(first))]);
            }
        }
    };
    let (unit_label, _) = field_label(category, Field::Amount);
    let amount_unit = category.amount_unit();
    view! {
        <BreweryDialog
            id=format!("ingredient-add-{}", category.prefix())
            title=format!("Add {}", category.noun())
            description="Enter the specification you will use. Nothing is added to the recipe until you choose Add to recipe."
            open=open
            wide=true
            actions=std::sync::Arc::new(move || view! {
                <Button variant=ButtonVariant::Ghost on:click=move |_| open.set(false)>"Cancel"</Button>
                <Button variant=ButtonVariant::Default on:click=move |_| commit()>"Add to recipe"</Button>
            }.into_any())
        >
            {move || open.get().then(|| view! {
            {move || failure.get().map(|message| view! { <p class="k-field__error" role="alert">{message}</p> })}
            <div class="recipe-editor__fields">
                <BreweryDraftField id=field_id("name") label="Name" value=text(|v| v.name.clone()) on_change=set(|v, s| v.name = s)/>
                <BreweryDraftField id=field_id("producer") label="Producer" value=text(|v| v.producer.clone()) on_change=set(|v, s| v.producer = s)/>
                {matches!(category, AdditionCategory::Yeast | AdditionCategory::Other).then(|| view! {
                    <BreweryDraftField id=field_id("unit") label="Unit" value=text(|v| v.unit.clone()) on_change=set(|v, s| v.unit = s)/>
                })}
                <BreweryDraftField id=field_id("amount") label=unit_label unit=amount_unit value=text(|v| v.amount.clone()) on_change=set(|v, s| v.amount = s) on_blur=blur numeric=true error=number_error(|v| v.amount.clone())/>
                {matches!(category, AdditionCategory::Hop).then(|| view! {
                    <BreweryDraftSelect id=field_id("stage") label="Use" value=text(|v| v.stage.clone()) on_change=set(|v, s| v.stage = s) options=stage_options(AdditionCategory::Hop)/>
                    <BreweryDraftField id=field_id("contact") label="Contact time" unit="min" value=text(|v| v.contact_minutes.clone()) on_change=set(|v, s| v.contact_minutes = s) on_blur=blur numeric=true error=number_error(|v| v.contact_minutes.clone())/>
                    <BreweryDraftField id=field_id("alpha") label="Alpha acid" unit="%" value=text(|v| v.alpha_percent.clone()) on_change=set(|v, s| v.alpha_percent = s) on_blur=blur numeric=true error=number_error(|v| v.alpha_percent.clone())/>
                    <BreweryDraftField id=field_id("temperature") label="Whirlpool temperature" unit="°C" value=text(|v| v.temperature.clone()) on_change=set(|v, s| v.temperature = s) on_blur=blur numeric=true error=number_error(|v| v.temperature.clone())/>
                })}
                {matches!(category, AdditionCategory::Yeast).then(|| view! {
                    <BreweryDraftField id=field_id("attenuation") label="Yeast attenuation (informational)" unit="%" value=text(|v| v.attenuation_percent.clone()) on_change=set(|v, s| v.attenuation_percent = s) on_blur=blur numeric=true error=number_error(|v| v.attenuation_percent.clone())/>
                })}
            </div>
            })}
        </BreweryDialog>
    }
}

fn manual_suffix(field: Field) -> &'static str {
    match field {
        Field::ContactMinutes => "contact",
        Field::Alpha => "alpha",
        Field::Temperature => "temperature",
        Field::Attenuation => "attenuation",
        _ => "amount",
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    fn hop(id: &str, name: &str, stage: &str, minutes: Option<f64>) -> HopAddition {
        HopAddition {
            row_id: id.into(),
            amount_g: Some(10.0),
            stage: stage.into(),
            contact_minutes: minutes,
            temperature_c: None,
            specification: HopSpecification { name: Some(name.into()), producer: None, alpha_fraction: Some(0.07) },
            source: None,
        }
    }

    fn draft_with_hops() -> RecipeDraft {
        let mut doc = RecipeDocumentV1::default();
        doc.hops = vec![
            hop("h1", "Cascade", "boil", Some(60.0)),
            hop("h2", "Cascade", "whirlpool", Some(15.0)),
            hop("h3", "Saaz", "dry_hop", None),
        ];
        RecipeDraft::new(doc)
    }

    #[test]
    fn percent_boundary_is_exact_decimal_not_binary_arithmetic() {
        assert_eq!(fraction_to_percent(0.07), 7.0);
        assert_ne!(0.07_f64 * 100.0, 7.0, "the old display path exposed binary noise");
        assert_eq!(fraction_to_percent(0.055), 5.5);
        assert_eq!(percent_to_fraction(5.5), 0.055);
        assert_eq!(percent_to_fraction(7.1), 0.071);
        assert_eq!(percent_to_fraction(100.0), 1.0);
        assert_eq!(percent_to_fraction(0.0), 0.0);
        assert_eq!(fraction_to_percent(1.0), 100.0);
        assert_eq!(shift_decimal("0.0000001", 2).as_deref(), Some("0.00001"));
        assert_eq!(shift_decimal("123", -2).as_deref(), Some("1.23"));
        assert_eq!(shift_decimal("-0.5", 2).as_deref(), Some("-50"));
    }

    #[test]
    fn duplicate_hop_labels_include_use_time_and_position() {
        let draft = draft_with_hops();
        let doc = draft.document();
        assert_eq!(row_label(doc, AdditionCategory::Hop, "h1"), "Cascade, hop, Boil, 60 min, 1 of 3");
        assert_eq!(row_label(doc, AdditionCategory::Hop, "h2"), "Cascade, hop, Whirlpool, 15 min, 2 of 3");
        assert_eq!(row_label(doc, AdditionCategory::Hop, "h3"), "Saaz, hop, Dry hop, 3 of 3");
        // A remove action still starts with the ingredient's name.
        assert!(format!("Remove {}", row_label(doc, AdditionCategory::Hop, "h1")).starts_with("Remove Cascade"));
    }

    #[test]
    fn hop_alpha_description_is_human_percent_never_a_raw_fraction() {
        let draft = draft_with_hops();
        let text = row_description(draft.document(), AdditionCategory::Hop, "h1", &draft);
        assert_eq!(text, "Alpha 7%");
        assert!(!text.contains("fraction"));
    }

    #[test]
    fn remove_and_undo_restore_id_order_and_raw_strings() {
        let mut draft = draft_with_hops();
        draft.edit_number("hops[h2].contact_minutes", "15.123456789012345678".into(), |doc, n| doc.hops[1].contact_minutes = n);
        draft.edit_number("hops[h2].temperature_c", "9x".into(), |_, _| {});
        let before = draft.document().clone();
        let removed = remove_row(&mut draft, AdditionCategory::Hop, "h2").expect("row existed");
        assert_eq!(row_ids(draft.document(), AdditionCategory::Hop), ["h1", "h3"]);
        assert!(draft.raw_numbers().get("hops[h2].contact_minutes").is_none());
        assert!(draft.unfinished_inputs().is_empty());
        assert_eq!(restore_row(&mut draft, removed.clone()), Restore::Restored);
        assert_eq!(draft.document(), &before);
        assert_eq!(draft.raw_numbers().get("hops[h2].contact_minutes").map(String::as_str), Some("15.123456789012345678"));
        assert_eq!(draft.unfinished_inputs(), [("hops[h2].temperature_c".to_owned(), "9x".to_owned())]);
        assert_eq!(restore_row(&mut draft, removed), Restore::AlreadyPresent, "a present row is never duplicated");
        assert_eq!(row_ids(draft.document(), AdditionCategory::Hop), ["h1", "h2", "h3"]);
    }

    #[test]
    fn restore_clamps_to_the_list_when_earlier_rows_changed() {
        let mut draft = draft_with_hops();
        let removed = remove_row(&mut draft, AdditionCategory::Hop, "h3").unwrap();
        remove_row(&mut draft, AdditionCategory::Hop, "h1").unwrap();
        assert_eq!(restore_row(&mut draft, removed), Restore::Restored);
        assert_eq!(row_ids(draft.document(), AdditionCategory::Hop), ["h2", "h3"]);
    }

    #[test]
    fn undo_is_refused_after_the_whole_document_was_replaced() {
        let mut draft = draft_with_hops();
        let removed = remove_row(&mut draft, AdditionCategory::Hop, "h2").unwrap();
        let replacement = draft.document().clone();
        draft.replace_saved(replacement);
        assert_eq!(restore_row(&mut draft, removed), Restore::Stale);
        assert_eq!(row_ids(draft.document(), AdditionCategory::Hop), ["h1", "h3"]);
    }

    #[test]
    fn manual_rows_are_built_only_from_valid_numbers_and_keep_absence() {
        let values = ManualValues { name: "Cascade".into(), amount: "20".into(), alpha_percent: "5.5".into(), ..Default::default() };
        let Ok((NewRow::Hop(row), raw)) = build_row(AdditionCategory::Hop, "r".into(), &values) else { panic!("valid hop") };
        assert_eq!(
            raw,
            [("hops[r].amount_g".to_owned(), "20".to_owned()), ("hops[r].specification.alpha_fraction".to_owned(), "5.5".to_owned())],
            "typed text is kept beside the numeric value"
        );
        assert_eq!(row.amount_g, Some(20.0));
        assert_eq!(row.specification.alpha_fraction, Some(0.055));
        assert_eq!(row.contact_minutes, None, "absence is not zero");
        assert_eq!(row.stage, "", "no invented use");
        let bad = ManualValues { amount: "2x".into(), contact_minutes: "1,5".into(), ..values };
        assert!(matches!(build_row(AdditionCategory::Hop, "r".into(), &bad), Err(fields) if fields == [Field::Amount, Field::ContactMinutes]));
    }

    #[test]
    fn only_yeast_keeps_an_informational_attenuation() {
        let values = ManualValues { attenuation_percent: "75".into(), ..Default::default() };
        let Ok((NewRow::Ingredient(yeast), _)) = build_row(AdditionCategory::Yeast, "y".into(), &values) else { panic!() };
        assert_eq!(yeast.specification.attenuation_fraction, Some(0.75));
        let Ok((NewRow::Ingredient(other), _)) = build_row(AdditionCategory::Other, "o".into(), &values) else { panic!() };
        assert_eq!(other.specification.attenuation_fraction, None);
    }

    #[test]
    fn diagnostic_rows_map_to_stable_dom_ids_and_report_details_openings() {
        let mut draft = draft_with_hops();
        let mut doc = draft.document().clone();
        doc.yeast.push(IngredientAddition { row_id: "y1".into(), amount: None, unit: None, specification: IngredientSpecification { name: None, producer: None, attenuation_fraction: None, extensions: Default::default() }, source: None });
        draft.replace_saved(doc);
        let doc = draft.document();
        let contact = diagnostic_field(doc, "hops[h2].contact_minutes").unwrap();
        assert_eq!((contact.dom_id.as_str(), contact.in_details, contact.label), ("hops-h2-colour-time", false, "Contact time"));
        assert!(contact.row_label.ends_with("2 of 3"), "{}", contact.row_label);
        let alpha = diagnostic_field(doc, "hops[h1].specification.alpha_fraction").unwrap();
        assert_eq!((alpha.dom_id.as_str(), alpha.in_details, alpha.label), ("hops-h1-yield-alpha", true, "Alpha acid"));
        let amount = diagnostic_field(doc, "ingredients[y1].amount").unwrap();
        assert_eq!(amount.dom_id, "yeast-y1-amount");
        assert!(!amount.in_details);
        assert_eq!(diagnostic_field(doc, "yeast[y1].specification.attenuation_fraction").unwrap().dom_id, "yeast-y1-attenuation");
        assert_eq!(diagnostic_field(doc, "hops[missing].amount_g"), None, "no link to a nonexistent row");
        assert_eq!(diagnostic_field(doc, "hops[h1].unknown"), None);
    }
}
