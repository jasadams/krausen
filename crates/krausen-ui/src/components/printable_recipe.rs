// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared printable recipe/process summary. KRA-22 reuses this component for
//! frozen historical plans; this module never loads or mutates a brew.
use krausen_types::{
    brewery::ChillingV1,
    formulation::RangeStatus,
    process_summary::{
        IngredientSection, ProcessSummary, SummaryIngredient, SummarySource, SummaryStep,
    },
    validation::{Diagnostic, Readiness, Target},
};
use leptos::prelude::*;

fn fixed(value: Option<f64>, digits: usize) -> String {
    value
        .map(|value| format!("{value:.digits$}"))
        .unwrap_or_else(|| "Unavailable".into())
}

fn trimmed(value: f64, digits: usize) -> String {
    let formatted = format!("{value:.digits$}");
    let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn amount_text(amount: Option<f64>, unit: Option<&str>) -> String {
    match (amount, unit) {
        (Some(amount), Some(unit)) => format!("{} {unit}", trimmed(amount, 3)),
        (Some(amount), None) => trimmed(amount, 3),
        (None, Some(unit)) => format!("Unavailable {unit}"),
        (None, None) => "Unavailable".into(),
    }
}

fn percent(value: Option<f64>) -> String {
    value
        .map(|value| format!("{}%", trimmed(value * 100.0, 2)))
        .unwrap_or_else(|| "Unavailable".into())
}

fn source_label(source: &SummarySource) -> String {
    match source {
        SummarySource::Saved { revision, .. } => format!("Saved revision {revision}"),
        SummarySource::HistoricalBrew {
            brew_id,
            selected_recipe_revision,
            ..
        } => {
            format!("Historical brew {brew_id} · frozen recipe revision {selected_recipe_revision}")
        }
        SummarySource::UnsavedDraft {
            base_revision: Some(revision),
            ..
        } => format!("Unsaved draft · based on revision {revision}"),
        SummarySource::UnsavedDraft {
            base_revision: None,
            ..
        } => "Unsaved draft · not saved".into(),
    }
}

fn readiness_label(readiness: Readiness) -> &'static str {
    match readiness {
        Readiness::Ready => "Ready",
        Readiness::Incomplete => "Incomplete",
        Readiness::Invalid => "Invalid",
    }
}

fn target_value(
    value: Option<f64>,
    digits: usize,
    label: &'static str,
    unit: &'static str,
    diagnostics: &[Diagnostic],
    target: Target,
) -> AnyView {
    let affected = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.affected_targets.contains(&target));
    if value.is_none() || affected.is_some() {
        let reason = affected
            .map(diagnostic_message)
            .unwrap_or_else(|| "Not calculated from the current inputs.".into());
        view! {
            <div class="print-sheet__metric">
                <span class="print-sheet__metric-label">{label}</span>
                <strong class="print-sheet__unavailable">"Unavailable"</strong>
                <span class="print-sheet__metric-hint">{reason}</span>
            </div>
        }
        .into_any()
    } else {
        view! {
            <div class="print-sheet__metric">
                <span class="print-sheet__metric-label">{label}</span>
                <strong>{fixed(value, digits)}<span class="print-sheet__unit">{unit}</span></strong>
            </div>
        }
        .into_any()
    }
}

fn range_status(status: Option<RangeStatus>) -> &'static str {
    match status {
        Some(RangeStatus::Below) => "below",
        Some(RangeStatus::Within) => "within",
        Some(RangeStatus::Above) => "above",
        None => "not comparable",
    }
}

fn chilling_label(chilling: Option<&ChillingV1>) -> String {
    match chilling {
        None => "Not specified".into(),
        Some(ChillingV1::Natural) => "Natural cooling".into(),
        Some(ChillingV1::Forced {
            rate_per_minute,
            plateau_c,
        }) => {
            let mut parts = vec!["Forced chilling".to_owned()];
            if let Some(rate) = rate_per_minute {
                parts.push(format!("{} / min", trimmed(*rate, 3)));
            }
            if let Some(plateau) = plateau_c {
                parts.push(format!("plateau {} °C", trimmed(*plateau, 2)));
            }
            parts.join(" · ")
        }
        Some(ChillingV1::Counterflow { litres_per_minute }) => match litres_per_minute {
            Some(rate) => format!("Counterflow · {} L/min", trimmed(*rate, 3)),
            None => "Counterflow".into(),
        },
    }
}

fn assumption_text(key: &str) -> String {
    let known = match key {
        "volume_reference_20c" => Some("Volumes are referenced to 20 °C."),
        "solid_addition_volume_negligible" => Some("Solid additions have negligible volume."),
        "kettle_top_up_default_zero" => Some("No kettle top-up is assumed."),
        "whirlpool_duration_default_zero" => Some("No whirlpool duration is assumed."),
        _ => None,
    };
    if let Some(text) = known {
        return text.to_owned();
    }
    if let Some(profile) = key.strip_prefix("profile:") {
        return format!(
            "Equipment profile assumption: {}.",
            profile.replace('_', " ")
        );
    }
    format!("{}.", key.replace('_', " "))
}

pub fn diagnostic_message(diagnostic: &Diagnostic) -> String {
    let subject = if diagnostic.path.starts_with("fermentables[") {
        "Fermentable"
    } else if diagnostic.path.starts_with("hops[") {
        "Hop addition"
    } else if diagnostic.path.starts_with("yeast[") {
        "Yeast"
    } else if diagnostic.path.starts_with("ingredients[") {
        "Ingredient"
    } else if diagnostic.path.starts_with("process.") {
        "Process"
    } else if diagnostic.path.starts_with("equipment") {
        "Equipment"
    } else {
        "Recipe"
    };
    let message = match diagnostic.code.as_str() {
        "missing" => "needs more information",
        "unfinished_quantity" => "needs a quantity",
        "negative" => "contains a negative value",
        "out_of_range" => "contains a value outside its allowed range",
        "non_finite" => "contains an invalid number",
        "invalid_row_id" => "has an invalid addition",
        _ => "needs attention",
    };
    format!("{subject} {message}.")
}

fn timing_text(row: &SummaryIngredient) -> String {
    let mut parts = Vec::new();
    if let Some(stage) = &row.stage {
        parts.push(stage.replace('_', " "));
    }
    if let Some(minutes) = row.time_minutes {
        parts.push(format!("{} min", trimmed(minutes, 2)));
    }
    if let Some(temperature) = row.temperature_c {
        parts.push(format!("{} °C", trimmed(temperature, 2)));
    }
    if parts.is_empty() {
        "Not specified".into()
    } else {
        parts.join(" · ")
    }
}

fn detail_text(row: &SummaryIngredient) -> String {
    let mut parts = Vec::new();
    match row.section {
        IngredientSection::Fermentable => {
            if let Some(kind) = &row.fermentable_type {
                parts.push(kind.clone());
            }
            if let Some(value) = row.yield_fraction {
                parts.push(format!("{} yield", trimmed(value * 100.0, 2)));
            }
            if let Some(value) = row.colour_ebc {
                parts.push(format!("{} EBC", trimmed(value, 2)));
            }
        }
        IngredientSection::Hop => {
            if let Some(value) = row.alpha_fraction {
                parts.push(format!("{} alpha", trimmed(value * 100.0, 2)));
            }
        }
        IngredientSection::Yeast => {
            if let Some(value) = row.attenuation_fraction {
                parts.push(format!("{} attenuation", trimmed(value * 100.0, 2)));
            }
        }
        IngredientSection::Other => {}
    }
    if parts.is_empty() {
        "—".into()
    } else {
        parts.join(" · ")
    }
}

fn ingredient_names(row: &SummaryIngredient) -> (String, Option<String>) {
    (
        row.name
            .clone()
            .unwrap_or_else(|| "Unnamed ingredient".into()),
        row.producer.clone(),
    )
}

fn ingredient_section(
    title: &'static str,
    empty: &'static str,
    rows: &[SummaryIngredient],
) -> AnyView {
    if rows.is_empty() {
        return view! {
            <section class="print-sheet__section">
                <h2>{title}</h2>
                <p class="print-sheet__muted">{empty}</p>
            </section>
        }
        .into_any();
    }
    view! {
        <section class="print-sheet__section">
            <h2>{title}</h2>
            <table class="print-sheet__table">
                <thead>
                    <tr><th scope="col">"Ingredient"</th><th scope="col">"Amount"</th><th scope="col">"Stage / time"</th><th scope="col">"Specification"</th></tr>
                </thead>
                <tbody>
                    {rows.iter().map(|row| {
                        let (name, producer) = ingredient_names(row);
                        view! {
                            <tr>
                                <td><span class="print-sheet__name">{name}</span>{producer.map(|producer| view! { <span class="print-sheet__sub">{producer}</span> })}</td>
                                <td class="print-sheet__numeric">{amount_text(row.amount, row.unit.as_deref())}</td>
                                <td>{timing_text(row)}</td>
                                <td>{detail_text(row)}</td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </section>
    }
    .into_any()
}

fn duration_text(value: f64, fermentation: bool) -> String {
    if fermentation && value >= 1440.0 {
        return format!("{} d", trimmed(value / 1440.0, 2));
    }
    if fermentation && value >= 60.0 {
        return format!("{} h", trimmed(value / 60.0, 2));
    }
    format!("{} min", trimmed(value, 2))
}

fn step_table(
    title: &'static str,
    empty: &'static str,
    steps: &[SummaryStep],
    fermentation: bool,
) -> AnyView {
    if steps.is_empty() {
        return view! {
            <section class="print-sheet__section">
                <h2>{title}</h2>
                <p class="print-sheet__muted">{empty}</p>
            </section>
        }
        .into_any();
    }
    view! {
        <section class="print-sheet__section">
            <h2>{title}</h2>
            <table class="print-sheet__table">
                <thead>
                    <tr><th scope="col">"Step"</th><th scope="col">"Time"</th><th scope="col">"Temperature"</th><th scope="col">"Volume"</th></tr>
                </thead>
                <tbody>
                    {steps.iter().map(|step| view! {
                        <tr>
                            <td>{step.name.clone().unwrap_or_else(|| "Unnamed step".into())}</td>
                            <td class="print-sheet__numeric">{step.time_minutes.map(|value| duration_text(value, fermentation)).unwrap_or_else(|| "—".into())}</td>
                            <td class="print-sheet__numeric">{step.temperature_c.map(|value| format!("{} °C", trimmed(value, 2))).unwrap_or_else(|| "—".into())}</td>
                            <td class="print-sheet__numeric">{step.volume_l.map(|value| format!("{} L", trimmed(value, 2))).unwrap_or_else(|| "—".into())}</td>
                        </tr>
                    }).collect_view()}
                </tbody>
            </table>
        </section>
    }
    .into_any()
}

#[component]
pub fn PrintableRecipe(summary: ProcessSummary) -> impl IntoView {
    let targets = &summary.targets;
    let diagnostics = summary.diagnostics.clone();
    let style_title = summary.style.as_ref().map(|style| match &style.style_id {
        Some(id) => format!("{id} · {}", style.name),
        None => style.name.clone(),
    });
    let equipment = match (
        &summary.equipment.profile_name,
        summary.equipment.profile_revision,
    ) {
        (Some(name), Some(revision)) => format!("{name} · revision {revision}"),
        (Some(name), None) => name.clone(),
        (None, _) => "Caller-supplied equipment settings".into(),
    };
    let batch = summary
        .batch_size_l
        .map(|value| format!("{} L batch", trimmed(value, 2)))
        .unwrap_or_else(|| "Batch volume unavailable".into());
    let boil = summary
        .boil_minutes
        .map(|value| format!("{} min boil", trimmed(value, 2)))
        .unwrap_or_else(|| "Boil time unavailable".into());
    let lede = std::iter::once(batch)
        .chain(std::iter::once(boil))
        .chain(style_title.clone())
        .collect::<Vec<_>>()
        .join(" · ");
    let digest = summary
        .context
        .digest
        .clone()
        .unwrap_or_else(|| "Context digest unavailable".into());
    let context_versions = format!(
        "Engine {} ({}) · policy {} · native schema v{} · result schema v{}",
        summary.context.engine_version,
        summary.context.engine_build,
        summary.context.policy_version,
        summary.context.native_schema_version,
        summary.context.result_schema_version,
    );
    let equipment_settings = summary.equipment.settings.clone();
    let process = summary.process.clone();
    let fermentables = summary.fermentables.clone();
    let hops = summary.hops.clone();
    let yeast = summary.yeast.clone();
    let other = summary.other_ingredients.clone();
    let assumptions = summary.assumptions.clone();
    let description = summary.description.clone();
    let notes = summary.notes.clone();
    let style_ranges = summary.style_ranges.clone();
    let style_comparison = summary.style_comparison.clone();
    let source = source_label(&summary.source);
    let title = if summary.title.is_empty() {
        "Untitled recipe".to_owned()
    } else {
        summary.title.clone()
    };
    view! {
        <article class="print-sheet" aria-label="Printable recipe and process summary">
            <header class="print-sheet__header">
                <p class="print-sheet__eyebrow">"Brew sheet"</p>
                <h1>{title}</h1>
                <p class="print-sheet__lede">{lede}</p>
                <p class="print-sheet__meta">{format!("{source} · {}", readiness_label(summary.readiness))}</p>
                <p class="print-sheet__meta">{format!("Equipment: {equipment}")}</p>
            </header>
            <section class="print-sheet__section" aria-label="Calculated targets">
                <h2>"Calculated targets"</h2>
                <div class="print-sheet__metrics">
                    {target_value(targets.original_sg, 3, "Original gravity", "SG", &diagnostics, Target::OriginalGravity)}
                    {target_value(targets.final_sg, 3, "Final gravity", "SG", &diagnostics, Target::FinalGravity)}
                    {target_value(targets.abv_percent, 1, "Alcohol", "% ABV", &diagnostics, Target::Alcohol)}
                    {target_value(targets.ibu, 1, "Bitterness", "IBU", &diagnostics, Target::Bitterness)}
                    {target_value(targets.colour_ebc, 1, "Colour", "EBC", &diagnostics, Target::Colour)}
                    {target_value(targets.total_water_l, 1, "Total water", "L", &diagnostics, Target::Water)}
                </div>
                {style_ranges.clone().map(|style| {
                    let comparison = style_comparison.clone();
                    let line = |label: &'static str, range: &krausen_types::formulation::Range, status: Option<RangeStatus>, unit: &'static str, digits: usize| {
                        format!("{label} {}–{} {unit} ({})", trimmed(range.min, digits), trimmed(range.max, digits), range_status(status))
                    };
                    view! {
                        <p class="print-sheet__ranges">
                            {format!("{} · {} · ", style.name, style.edition)}
                            {line("OG", &style.original_sg, comparison.as_ref().and_then(|c| c.original_sg), "SG", 3)}
                            {" · "}
                            {line("FG", &style.final_sg, comparison.as_ref().and_then(|c| c.final_sg), "SG", 3)}
                            {" · "}
                            {line("ABV", &style.abv_percent, comparison.as_ref().and_then(|c| c.abv_percent), "%", 1)}
                            {" · "}
                            {line("IBU", &style.ibu, comparison.as_ref().and_then(|c| c.ibu), "IBU", 1)}
                            {" · "}
                            {line("EBC", &style.colour_ebc, comparison.as_ref().and_then(|c| c.colour_ebc), "EBC", 1)}
                        </p>
                    }
                })}
                <p class="print-sheet__muted">"Estimates use the full-precision shared formulation context; they are not measured results."</p>
            </section>
            <section class="print-sheet__section">
                <h2>"Equipment and water"</h2>
                <dl class="print-sheet__facts">
                    <div><dt>"Equipment source"</dt><dd>{equipment}</dd></div>
                    <div><dt>"Provenance"</dt><dd>{summary.equipment.provenance.replace('_', " ")}</dd></div>
                    <div><dt>"Brewhouse efficiency"</dt><dd>{percent(equipment_settings.efficiency_override_fraction)}</dd></div>
                    <div><dt>"Mash efficiency"</dt><dd>{percent(equipment_settings.mash_efficiency_fraction)}</dd></div>
                    <div><dt>"Boil-off"</dt><dd>{equipment_settings.boil_off_l_per_hour.map(|value| format!("{} L/h", trimmed(value, 2))).unwrap_or_else(|| "Not specified".into())}</dd></div>
                    <div><dt>"Transfer loss"</dt><dd>{equipment_settings.transfer_loss_l.map(|value| format!("{} L", trimmed(value, 2))).unwrap_or_else(|| "Not specified".into())}</dd></div>
                    <div><dt>"Pre-boil deadspace"</dt><dd>{equipment_settings.pre_boil_deadspace_l.map(|value| format!("{} L", trimmed(value, 2))).unwrap_or_else(|| "Not specified".into())}</dd></div>
                    <div><dt>"Grain absorption"</dt><dd>{equipment_settings.grain_absorption_l_per_kg.map(|value| format!("{} L/kg", trimmed(value, 2))).unwrap_or_else(|| "Not specified".into())}</dd></div>
                    <div><dt>"Mash water"</dt><dd>{process.mash_water_l.map(|value| format!("{} L", trimmed(value, 2))).unwrap_or_else(|| "Not specified".into())}</dd></div>
                    <div><dt>"Mash water method"</dt><dd>{process.mash_water_method.clone().unwrap_or_else(|| "Not specified".into())}</dd></div>
                    <div><dt>"Sparge water"</dt><dd>{targets.sparge_water_l.map(|value| format!("{} L", trimmed(value, 2))).unwrap_or_else(|| "Unavailable".into())}</dd></div>
                    <div><dt>"Kettle water"</dt><dd>{targets.kettle_water_l.map(|value| format!("{} L", trimmed(value, 2))).unwrap_or_else(|| "Unavailable".into())}</dd></div>
                    <div><dt>"Kettle top-up"</dt><dd>{process.kettle_top_up_l.map(|value| format!("{} L", trimmed(value, 2))).unwrap_or_else(|| "Not specified".into())}</dd></div>
                    <div><dt>"Whirlpool"</dt><dd>{process.whirlpool_minutes.map(|value| format!("{} min", trimmed(value, 2))).unwrap_or_else(|| "Not specified".into())}</dd></div>
                    <div><dt>"Whirlpool temperature"</dt><dd>{process.whirlpool_temperature_c.map(|value| format!("{} °C", trimmed(value, 2))).unwrap_or_else(|| "Not specified".into())}</dd></div>
                    <div><dt>"Chilling"</dt><dd>{chilling_label(process.chilling.as_ref())}</dd></div>
                    <div><dt>"Apparent attenuation"</dt><dd>{percent(process.apparent_attenuation_fraction)}</dd></div>
                </dl>
                {(!process.unsupported.is_empty()).then(|| {
                    let keys = process.unsupported.keys().cloned().collect::<Vec<_>>().join(", ");
                    view! { <p class="print-sheet__muted">{format!("Retained unsupported process data, not calculated here: {keys}.")}</p> }
                })}
            </section>
            {ingredient_section("Fermentables", "No fermentables recorded.", &fermentables)}
            {ingredient_section("Hops", "No hops recorded.", &hops)}
            {ingredient_section("Yeast", "No yeast recorded.", &yeast)}
            {ingredient_section("Other ingredients", "No other ingredients recorded.", &other)}
            {step_table("Mash steps", "No mash steps recorded.", &process.mash_steps, false)}
            {step_table("Fermentation steps", "No fermentation steps recorded.", &process.fermentation_steps, true)}
            <section class="print-sheet__section">
                <h2>"Assumptions"</h2>
                {if assumptions.is_empty() {
                    view! { <p class="print-sheet__muted">"No additional assumptions recorded."</p> }.into_any()
                } else {
                    view! { <ul class="print-sheet__list">{assumptions.iter().map(|key| view! { <li>{assumption_text(key)}</li> }).collect_view()}</ul> }.into_any()
                }}
            </section>
            <section class="print-sheet__section">
                <h2>"Validation and unavailable data"</h2>
                <p>{format!("Readiness: {}.", readiness_label(summary.readiness))}</p>
                {if diagnostics.is_empty() {
                    view! { <p class="print-sheet__muted">"No validation diagnostics."</p> }.into_any()
                } else {
                    view! { <ul class="print-sheet__list">{diagnostics.iter().map(|diagnostic| view! {
                        <li><strong>{readiness_label(diagnostic.severity)}</strong> " · " {diagnostic_message(diagnostic)} <span class="print-sheet__path">{diagnostic.path.clone()}</span></li>
                    }).collect_view()}</ul> }.into_any()
                }}
            </section>
            {description.map(|description| view! {
                <section class="print-sheet__section"><h2>"Description"</h2><p class="print-sheet__prose">{description}</p></section>
            })}
            {notes.map(|notes| view! {
                <section class="print-sheet__section"><h2>"Brewer notes"</h2><p class="print-sheet__prose">{notes}</p></section>
            })}
            <footer class="print-sheet__footer">
                <p class="print-sheet__muted">{context_versions}</p>
                <p class="print-sheet__digest">{format!("Calculation context: {digest}")}</p>
                <p class="print-sheet__muted">"This sheet identifies its source revision or unsaved draft. Later recipe or equipment changes are not reflected here."</p>
            </footer>
        </article>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_labels_distinguish_saved_revision_and_unsaved_draft() {
        assert_eq!(
            source_label(&SummarySource::Saved {
                recipe_id: "r".into(),
                revision: 7
            }),
            "Saved revision 7"
        );
        assert_eq!(
            source_label(&SummarySource::UnsavedDraft {
                recipe_id: Some("r".into()),
                base_revision: Some(7)
            }),
            "Unsaved draft · based on revision 7"
        );
        assert_eq!(
            source_label(&SummarySource::UnsavedDraft {
                recipe_id: None,
                base_revision: None
            }),
            "Unsaved draft · not saved"
        );
    }

    #[test]
    fn unavailable_is_not_rendered_as_zero() {
        assert_eq!(fixed(None, 3), "Unavailable");
        assert_eq!(fixed(Some(0.0), 3), "0.000");
        assert_eq!(trimmed(0.0, 3), "0");
        assert_eq!(amount_text(Some(4.2), Some("kg")), "4.2 kg");
        assert_eq!(amount_text(None, Some("kg")), "Unavailable kg");
    }

    #[test]
    fn diagnostic_text_identifies_the_affected_row_family() {
        let diagnostic = Diagnostic {
            code: "negative".into(),
            path: "hops[abc].contact_minutes".into(),
            severity: Readiness::Invalid,
            affected_targets: vec![Target::Bitterness],
        };
        assert_eq!(
            diagnostic_message(&diagnostic),
            "Hop addition contains a negative value."
        );
    }
}
