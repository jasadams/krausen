// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recipe equipment context: profile selection and recipe-local overrides.
//!
//! Selecting a profile and adjusting values for this recipe edit only the recipe
//! draft. Reusable profiles are edited by the independent Equipment manager.
//! [`EquipmentSummary`] is the stable interface other recipe presentation
//! (for example the recipe header) consumes; it carries no editing behavior.
use crate::{
    components::{
        BreweryDraftField, BreweryDraftSelect, BreweryNotice, BrewerySection, Button, ButtonLink,
        ButtonSize, ButtonVariant,
    },
    pages::{
        equipment_profiles::{ProfileLibrary, use_profile_library},
        recipe_draft::RecipeDraft,
    },
};
use krausen_types::{
    brewery::{CoolingMethodV1, EquipmentProfile, EquipmentSettingsV1, RecipeDocumentV1},
    request_context::OperationIntent,
};
use leptos::prelude::*;

/// Which equipment the recipe is using, independent of any editing UI.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum EquipmentSource {
    /// Values come only from this recipe's own settings.
    #[default]
    RecipeOnly,
    Profile {
        id: String,
        name: String,
    },
    /// A profile is selected but its record is not loaded yet. This is not the
    /// same claim as "unavailable"; the header must not say recipe-only.
    Loading {
        id: String,
    },
    /// A profile is selected but is archived or otherwise unusable.
    Unavailable {
        id: String,
    },
}

/// Selected identity and how many recipe-local overrides are in effect.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EquipmentSummary {
    pub source: EquipmentSource,
    pub override_count: usize,
}

impl EquipmentSummary {
    pub fn identity(&self) -> String {
        match &self.source {
            EquipmentSource::RecipeOnly => "Recipe-only equipment".into(),
            EquipmentSource::Profile { name, .. } => format!("Profile: {name}"),
            EquipmentSource::Loading { .. } => "Selected equipment profile".into(),
            EquipmentSource::Unavailable { .. } => "Equipment profile unavailable".into(),
        }
    }
    pub fn overrides(&self) -> String {
        match self.override_count {
            0 => "No recipe overrides".into(),
            1 => "1 recipe override".into(),
            n => format!("{n} recipe overrides"),
        }
    }
    pub fn from_document(document: &RecipeDocumentV1, profile: Option<&EquipmentProfile>) -> Self {
        Self::from_document_with_library(document, profile, true)
    }

    /// `library_loaded` separates "selected profile, details still loading" from a
    /// genuinely missing or archived profile, so the header never claims either.
    pub fn from_document_with_library(
        document: &RecipeDocumentV1,
        profile: Option<&EquipmentProfile>,
        library_loaded: bool,
    ) -> Self {
        let source = match (&document.equipment_profile_id, profile) {
            (None, _) => EquipmentSource::RecipeOnly,
            (Some(id), Some(p)) if p.profile_id == *id && !p.archived => EquipmentSource::Profile {
                id: id.clone(),
                name: p.name.clone(),
            },
            (Some(id), _) if !library_loaded => EquipmentSource::Loading { id: id.clone() },
            (Some(id), _) => EquipmentSource::Unavailable { id: id.clone() },
        };
        Self {
            source,
            override_count: override_count(&document.equipment_overrides),
        }
    }
}

impl EquipmentSummary {
    /// First paint, before the profile library has loaded: a selected profile is
    /// reported as selected without claiming a name, and never as recipe-only.
    pub fn from_saved_document(document: &RecipeDocumentV1) -> Self {
        let source = match &document.equipment_profile_id {
            None => EquipmentSource::RecipeOnly,
            Some(id) => EquipmentSource::Loading { id: id.clone() },
        };
        Self { source, override_count: override_count(&document.equipment_overrides) }
    }
}

/// Whether the loaded profile pages can decide the selection. A failed fetch is
/// decisive (unavailable plus Retry), and a selected profile that is not in the
/// loaded pages is undecided only while more pages remain.
pub fn library_decidable(loaded: bool, load_failed: bool, profile_known: bool, has_more: bool) -> bool {
    load_failed || (loaded && (profile_known || !has_more))
}

/// Number of settings this recipe overrides; the cooling method counts with the numeric ones.
pub fn override_count(overrides: &EquipmentSettingsV1) -> usize {
    SPECS
        .iter()
        .filter(|spec| (spec.read)(overrides).is_some())
        .count()
        + usize::from(overrides.cooling_method.is_some())
}

struct Spec {
    id: &'static str,
    label: &'static str,
    unit: &'static str,
    scale: f64,
    path: &'static str,
    read: fn(&EquipmentSettingsV1) -> Option<f64>,
    write: fn(&mut EquipmentSettingsV1, Option<f64>),
}
/// Select value of the deliberate "Recipe-only equipment" choice (profile ids are UUIDs).
const RECIPE_ONLY_CHOICE: &str = "recipe-only";

static SPECS: [Spec; 16] = [
    Spec { id: "recipe-efficiency-override", label: "Brewhouse efficiency override", unit: "%", scale: 100.0, path: "equipment_overrides.efficiency_override_fraction", read: |s| s.efficiency_override_fraction, write: |s, v| s.efficiency_override_fraction = v },
    Spec { id: "recipe-mash-volume", label: "Mash tun volume", unit: "L", scale: 1.0, path: "equipment_overrides.mash_tun_volume_l", read: |s| s.mash_tun_volume_l, write: |s, v| s.mash_tun_volume_l = v },
    Spec { id: "recipe-efficiency", label: "Mash efficiency", unit: "%", scale: 100.0, path: "equipment_overrides.mash_efficiency_fraction", read: |s| s.mash_efficiency_fraction, write: |s, v| s.mash_efficiency_fraction = v },
    Spec { id: "recipe-deadspace", label: "Mash tun deadspace", unit: "L", scale: 1.0, path: "equipment_overrides.mash_tun_deadspace_l", read: |s| s.mash_tun_deadspace_l, write: |s, v| s.mash_tun_deadspace_l = v },
    Spec { id: "recipe-thickness", label: "Mash thickness", unit: "L/kg", scale: 1.0, path: "equipment_overrides.mash_thickness_l_per_kg", read: |s| s.mash_thickness_l_per_kg, write: |s, v| s.mash_thickness_l_per_kg = v },
    Spec { id: "recipe-mash-loss", label: "Mash tun loss", unit: "L", scale: 1.0, path: "equipment_overrides.mash_tun_loss_l", read: |s| s.mash_tun_loss_l, write: |s, v| s.mash_tun_loss_l = v },
    Spec { id: "recipe-absorption", label: "Mash grain absorption", unit: "L/kg", scale: 1.0, path: "equipment_overrides.mash_grain_absorption_l_per_kg", read: |s| s.mash_grain_absorption_l_per_kg, write: |s, v| s.mash_grain_absorption_l_per_kg = v },
    Spec { id: "recipe-sparge-volume", label: "Sparge vessel volume", unit: "L", scale: 1.0, path: "equipment_overrides.sparge_vessel_volume_l", read: |s| s.sparge_vessel_volume_l, write: |s, v| s.sparge_vessel_volume_l = v },
    Spec { id: "recipe-sparge-absorption", label: "Sparge grain absorption", unit: "L/kg", scale: 1.0, path: "equipment_overrides.sparge_grain_absorption_l_per_kg", read: |s| s.sparge_grain_absorption_l_per_kg, write: |s, v| s.sparge_grain_absorption_l_per_kg = v },
    Spec { id: "recipe-sparge-deadspace", label: "Sparge deadspace volume", unit: "L", scale: 1.0, path: "equipment_overrides.sparge_deadspace_l", read: |s| s.sparge_deadspace_l, write: |s, v| s.sparge_deadspace_l = v },
    Spec { id: "recipe-capacity", label: "Boil volume", unit: "L", scale: 1.0, path: "equipment_overrides.boil_volume_l", read: |s| s.boil_volume_l, write: |s, v| s.boil_volume_l = v },
    Spec { id: "recipe-shrinkage", label: "Wort shrinkage", unit: "%", scale: 100.0, path: "equipment_overrides.wort_shrinkage_fraction", read: |s| s.wort_shrinkage_fraction, write: |s, v| s.wort_shrinkage_fraction = v },
    Spec { id: "recipe-boil-off", label: "Boil loss", unit: "L/hr", scale: 1.0, path: "equipment_overrides.boil_loss_l_per_hour", read: |s| s.boil_loss_l_per_hour, write: |s, v| s.boil_loss_l_per_hour = v },
    Spec { id: "recipe-loss", label: "Trub and chiller loss", unit: "L", scale: 1.0, path: "equipment_overrides.trub_and_chiller_loss_l", read: |s| s.trub_and_chiller_loss_l, write: |s, v| s.trub_and_chiller_loss_l = v },
    Spec { id: "recipe-diameter", label: "Kettle diameter", unit: "cm", scale: 1.0, path: "equipment_overrides.kettle_diameter_cm", read: |s| s.kettle_diameter_cm, write: |s, v| s.kettle_diameter_cm = v },
    Spec { id: "recipe-cooling-flow", label: "Flow rate", unit: "L/min", scale: 1.0, path: "equipment_overrides.cooling_flow_rate_l_per_minute", read: |s| s.cooling_flow_rate_l_per_minute, write: |s, v| s.cooling_flow_rate_l_per_minute = v },
];

/// Control id and label for an equipment diagnostic. `equipment.*` names the
/// resolved effective value, which the brewer corrects by overriding it here.
pub(crate) fn diagnostic_spec(path: &str) -> Option<(&'static str, &'static str)> {
    let field = path.strip_prefix("equipment.").or_else(|| path.strip_prefix("equipment_overrides."))?;
    let wanted = format!("equipment_overrides.{field}");
    SPECS.iter().find(|spec| spec.path == wanted).map(|spec| (spec.id, spec.label))
}

/// `(DOM id, visible label)` of the cooling method select, declared once for the
/// control and the diagnostic registry.
pub(crate) const COOLING_FIELD: (&str, &str) = ("recipe-cooling-method", "Cooling method");

/// Appends a start requirement to a field's existing explanatory hint.
fn with_need(need: Option<String>, hint: String) -> String {
    match need {
        Some(need) => format!("{need}. {hint}"),
        None => hint,
    }
}

/// Effective equipment values for a review, each marked with where it comes from:
/// a recipe override, the saved profile, or the recipe's own settings when no
/// profile is selected. `overrides` are the recipe's own `equipment_overrides`.
pub fn effective_lines(
    effective: &EquipmentSettingsV1,
    overrides: &EquipmentSettingsV1,
    has_profile: bool,
) -> Vec<(String, bool)> {
    let origin = |overridden: bool| {
        if overridden {
            "recipe override"
        } else if has_profile {
            "from profile"
        } else {
            "recipe setting"
        }
    };
    let mut lines: Vec<(String, bool)> = SPECS
        .iter()
        .filter_map(|spec| {
            let value = (spec.read)(effective)?;
            let overridden = (spec.read)(overrides).is_some();
            Some((
                format!("{}: {} {} · {}", spec.label, shown(value, spec.scale), spec.unit, origin(overridden)),
                overridden,
            ))
        })
        .collect();
    if let Some(method) = effective.cooling_method {
        lines.push((
            format!("Cooling method: {} · {}", method.label(), origin(overrides.cooling_method.is_some())),
            overrides.cooling_method.is_some(),
        ));
    }
    lines
}

fn shown(value: f64, scale: f64) -> String {
    (((value * scale) * 1e6).round() / 1e6).to_string()
}

/// Plain-language inherited/effective/overridden state for one value.
pub fn override_hint(
    spec_unit: &str,
    scale: f64,
    overridden: Option<f64>,
    inherited: Option<f64>,
    profile: Option<&str>,
    profile_known: bool,
) -> String {
    let inherited_text = match (inherited, profile, profile_known) {
        (Some(v), Some(name), _) => format!("{} {spec_unit} from {name}", shown(v, scale)),
        (None, Some(_), false) => "profile values not available".to_owned(),
        (None, Some(name), true) => format!("{name} has no value"),
        (_, None, _) => "no profile selected".to_owned(),
    };
    match overridden {
        Some(v) => format!(
            "Overridden for this recipe: effective {} {spec_unit}. Inherited: {inherited_text}.",
            shown(v, scale)
        ),
        None => match inherited {
            Some(v) => format!("Inherited: effective {} {spec_unit} from {}.", shown(v, scale), profile.unwrap_or("profile")),
            None => format!("Not overridden. Inherited: {inherited_text}."),
        },
    }
}

#[component]
fn OverrideField(
    spec: &'static Spec,
    draft: RwSignal<RecipeDraft>,
    profile: Signal<Option<EquipmentProfile>>,
    profile_name: Signal<Option<String>>,
    profile_known: Signal<bool>,
) -> impl IntoView {
    let read = spec.read;
    let write = spec.write;
    let path = spec.path;
    let scale = spec.scale;
    let value = Signal::derive(move || {
        draft.with(|d| d.number_text(path, read(&d.document().equipment_overrides).map(|v| v * scale)))
    });
    // The validator reports a missing or invalid effective value as `equipment.*`
    // and an invalid recipe override as `equipment_overrides.*`; both land here.
    let effective_path = path.replacen("equipment_overrides.", "equipment.", 1);
    let error = Signal::derive({
        let effective = effective_path.clone();
        move || draft.with(|d| d.number_error(path).or_else(|| d.validation_error(&effective)).map(str::to_owned))
    });
    let overridden = Signal::derive(move || {
        draft.with(|d| read(&d.document().equipment_overrides).is_some() || d.raw_numbers().contains_key(path))
    });
    let hint = Signal::derive(move || {
        override_hint(
            spec.unit,
            scale,
            draft.with(|d| read(&d.document().equipment_overrides)),
            profile.with(|p| p.as_ref().and_then(|p| read(&p.settings))),
            profile_name.get().as_deref(),
            profile_known.get(),
        )
    });
    let needs = Signal::derive(move || draft.with(|d| d.validation_hint(path).or_else(|| d.validation_hint(&effective_path)).map(str::to_owned)));
    let hint = Signal::derive(move || with_need(needs.get(), hint.get()));
    let change = Callback::new(move |text| {
        draft.update(|d| {
            d.edit_number(path, text, |doc, n| write(&mut doc.equipment_overrides, n.map(|v| v / scale)))
        })
    });
    let blur = Callback::new(move |()| draft.update(|d| d.settle_number(path)));
    let reset = move |_| {
        draft.update(|d| d.edit_clearing(path, |doc| write(&mut doc.equipment_overrides, None)));
    };
    view! {
        <div>
            <BreweryDraftField id=spec.id label=spec.label unit=spec.unit value=value error=error hint=hint on_change=change on_blur=blur numeric=true/>
            <Show when=move || overridden.get()>
                <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm aria_label=format!("Reset {} to inherited", spec.label) on:click=reset>"Reset"</Button>
            </Show>
        </div>
    }
}

/// Profile selection plus "Adjust for this recipe". Writes the selected identity
/// and override count to `summary` for other recipe presentation to consume.
#[component]
pub fn RecipeEquipment(
    draft: RwSignal<RecipeDraft>,
    identity: RwSignal<OperationIntent>,
    context_visible: RwSignal<bool>,
    recipe_id: RwSignal<Option<String>>,
    summary: RwSignal<EquipmentSummary>,
) -> impl IntoView {
    let library: ProfileLibrary = use_profile_library(identity, context_visible);
    let selection = Signal::derive(move || {
        draft.with(|d| d.document().equipment_profile_id.clone().unwrap_or_default())
    });
    let selected_profile = Signal::derive(move || {
        let id = selection.get();
        (!id.is_empty())
            .then(|| library.profiles.with(|l| l.iter().find(|p| p.profile_id == id).cloned()))
            .flatten()
    });
    // Archived profiles remain visible by name but cannot be inherited from.
    let usable_profile = Signal::derive(move || selected_profile.get().filter(|p| !p.archived));
    let profile_name = Signal::derive(move || selected_profile.get().map(|p| p.name));
    let profile_known = Signal::derive(move || usable_profile.get().is_some());
    Effect::new(move |_| {
        // The library may still be paging; a selected profile that is not in the
        // loaded pages is undecided, not unavailable.
        let decidable = library_decidable(
            library.loaded.get(),
            !library.error.get().is_empty(),
            selected_profile.get().is_some(),
            library.next_cursor.get().is_some(),
        );
        let next = draft.with(|d| EquipmentSummary::from_document_with_library(d.document(), usable_profile.get().as_ref(), decidable));
        if summary.get_untracked() != next {
            summary.set(next);
        }
    });
    // Three states: a profile, a deliberate "Recipe-only equipment" choice, or nothing chosen yet.
    // Only "nothing chosen" lets a new recipe take the workspace default.
    let choice = Signal::derive(move || {
        if draft.with(|d| d.document().recipe_only_equipment) { RECIPE_ONLY_CHOICE.to_owned() } else { selection.get() }
    });
    let choose = Callback::new(move |value: String| {
        draft.update(|d| d.edit(|doc| {
            doc.recipe_only_equipment = value == RECIPE_ONLY_CHOICE;
            doc.equipment_profile_id = (!value.is_empty() && value != RECIPE_ONLY_CHOICE).then_some(value);
        }));
    });
    let manage_href = Signal::derive(move || {
        // An unsaved recipe has no address to return to; it stays protected by its own guard.
        let back = recipe_id.get().map(|id| format!("return=/recipe-editor/{id}"));
        let id = selection.get();
        let profile = (!id.is_empty()).then(|| format!("profile={id}"));
        let query = [profile, back].into_iter().flatten().collect::<Vec<_>>().join("&");
        if query.is_empty() { "/equipment".to_owned() } else { format!("/equipment?{query}") }
    });
    let cooling = Signal::derive(move || draft.with(|d| d.document().equipment_overrides.cooling_method.map_or("", CoolingMethodV1::key).to_owned()));
    let cooling_change = Callback::new(move |value: String| {
        draft.update(|d| {
            d.edit(|doc| {
                doc.equipment_overrides.cooling_method = CoolingMethodV1::from_key(&value);
            })
        })
    });
    let cooling_error = Signal::derive(move || draft.with(|d| d.validation_error("equipment.cooling_method").map(str::to_owned)));
    let cooling_need = Signal::derive(move || draft.with(|d| d.validation_hint("equipment.cooling_method").map(str::to_owned)));
    let cooling_hint = Signal::derive(move || {
        let inherited = usable_profile.get().map(|p| (p.name.clone(), p.settings.cooling_method.map_or("", CoolingMethodV1::key)));
        match (cooling.get().as_str(), inherited) {
            ("", Some((name, ""))) => format!("Not overridden. {name} has no cooling method."),
            ("", Some((name, method))) => format!("Inherited: effective {method} cooling from {name}."),
            ("", None) => "Not overridden. No profile values to inherit.".to_owned(),
            (method, _) => format!("Overridden for this recipe: effective {method} cooling."),
        }
    });
    let cooling_hint = Signal::derive(move || with_need(cooling_need.get(), cooling_hint.get()));
    let cooling_options: Vec<(String, String)> = std::iter::once((String::new(), "Inherit / not specified".to_owned()))
        .chain(CoolingMethodV1::ALL.iter().map(|method| (method.key().to_owned(), method.label().to_owned())))
        .collect();
    view! {
        <BrewerySection title="Equipment for this recipe">
            <div class="recipe-editor__fields">
                {move || {
                    // Track only the list: rebuilding on each choice would drop keyboard focus.
                    let selected = selection.get_untracked();
                    let mut options = vec![(String::new(), "No equipment chosen".to_owned()), (RECIPE_ONLY_CHOICE.to_owned(), "Recipe-only equipment".to_owned())];
                    options.extend(library.profiles.get().into_iter().filter(|p| !p.archived || p.profile_id == selected).map(|p| {
                        let label = if p.archived { format!("{} (archived, unavailable)", p.name) } else { p.name };
                        (p.profile_id, label)
                    }));
                    view! { <BreweryDraftSelect id="equipment-profile-select" label="Equipment profile" value=choice on_change=choose options=options/> }
                }}
            </div>
            <p class="k-field__hint">{move || match summary.get().source {
                EquipmentSource::Profile { name, .. } => format!("Using profile “{name}” for this recipe. Recipe overrides below apply only to this recipe."),
                EquipmentSource::Loading { .. } => "The selected profile’s details are still loading; the settings below apply to this recipe until they arrive.".to_owned(),
                EquipmentSource::Unavailable { .. } => "The selected profile is not available here.".to_owned(),
                EquipmentSource::RecipeOnly if draft.with(|d| d.document().recipe_only_equipment) => "Using recipe-only equipment: the settings below belong to this recipe.".to_owned(),
                EquipmentSource::RecipeOnly => "No equipment chosen. The settings below belong to this recipe; a new recipe takes the workspace default equipment when it is first saved unless you choose Recipe-only equipment.".to_owned(),
            }}</p>
            <Show when=move || library.next_cursor.get().is_some()>
                <Button variant=ButtonVariant::Secondary disabled=library.loading on:click=move |_| library.load_more()>"Load more profiles"</Button>
            </Show>
            <Show when=move || !library.error.get().is_empty()>
                <BreweryNotice title="Profiles unavailable" body=library.error.get() warning=true/>
                <Button variant=ButtonVariant::Secondary on:click=move |_| library.reload()>"Retry loading profiles"</Button>
            </Show>
            {move || {
                let id = selection.get();
                let missing = !id.is_empty() && library.loaded.get() && !library.profiles.with(|l| l.iter().any(|p| p.profile_id == id && !p.archived));
                missing.then(|| view! { <BreweryNotice title=if library.next_cursor.get().is_some() { "Equipment profile not loaded" } else { "Equipment profile unavailable" } body="Load more profiles, choose an active profile or use recipe-only equipment." warning=true/> })
            }}
            {move || view! { <ButtonLink href=manage_href.get() variant=ButtonVariant::Secondary>"Manage equipment profiles"</ButtonLink> }}
        </BrewerySection>
        <BrewerySection title="Adjust for this recipe">
            <p class="k-field__hint">{move || summary.get().overrides()}". A blank value inherits from the selected profile. Saving the recipe saves these overrides only; shared profiles are managed in Equipment."</p>
            <div class="recipe-editor__fields">
                {SPECS.iter().map(|spec| view! { <OverrideField spec=spec draft=draft profile=usable_profile profile_name=profile_name profile_known=profile_known/> }).collect_view()}
                <div>
                    <BreweryDraftSelect id=COOLING_FIELD.0 label=COOLING_FIELD.1 value=cooling on_change=cooling_change error=cooling_error hint=cooling_hint options=cooling_options.clone()/>
                    <Show when=move || cooling.get() != "">
                        <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm aria_label="Reset Cooling method to inherited" on:click=move |_| cooling_change.run(String::new())>"Reset"</Button>
                    </Show>
                </div>
            </div>
        </BrewerySection>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(settings: EquipmentSettingsV1, archived: bool) -> EquipmentProfile {
        EquipmentProfile {
            workspace_id: "w".into(),
            profile_id: "p".into(),
            created_at: 0,
            name: "Garage".into(),
            revision: 1,
            metadata_revision: 1,
            archived,
            is_default: false,
            settings,
            about: Default::default(),
            provenance: krausen_types::brewery::EquipmentProvenance { source: "manual".into(), assumptions: vec![] },
            saved_by: "u".into(),
            saved_at: 0,
        }
    }

    #[test]
    fn every_recipe_override_field_is_counted_including_the_cooling_method() {
        let mut overrides = EquipmentSettingsV1::default();
        assert_eq!(override_count(&overrides), 0);
        overrides.efficiency_override_fraction = Some(0.7);
        overrides.mash_efficiency_fraction = Some(0.75);
        overrides.kettle_diameter_cm = Some(30.0);
        assert_eq!(override_count(&overrides), 3);
        overrides.cooling_method = Some(CoolingMethodV1::CounterflowChiller);
        assert_eq!(override_count(&overrides), 4);
    }

    #[test]
    fn the_specs_cover_every_native_numeric_setting_with_distinct_ids_and_paths() {
        let mut all = EquipmentSettingsV1::default();
        for spec in &SPECS {
            (spec.write)(&mut all, Some(1.0));
            assert_eq!((spec.read)(&all), Some(1.0), "{}", spec.id);
        }
        assert_eq!(override_count(&all), 16);
        let mut ids: Vec<_> = SPECS.iter().map(|s| s.id).collect();
        let mut paths: Vec<_> = SPECS.iter().map(|s| s.path).collect();
        let total = ids.len();
        ids.sort();
        ids.dedup();
        paths.sort();
        paths.dedup();
        assert_eq!((ids.len(), paths.len()), (total, total));
    }

    #[test]
    fn the_overrides_offer_every_spec_calculation_field() {
        let labels: Vec<_> = SPECS.iter().map(|s| s.label).collect();
        for wanted in [
            "Mash tun volume", "Mash efficiency", "Mash tun deadspace", "Mash thickness", "Mash tun loss",
            "Mash grain absorption", "Sparge vessel volume", "Sparge grain absorption", "Sparge deadspace volume",
            "Boil volume", "Wort shrinkage", "Boil loss", "Trub and chiller loss", "Kettle diameter", "Flow rate",
            "Brewhouse efficiency override",
        ] {
            assert!(labels.contains(&wanted), "{wanted}");
        }
        assert!(diagnostic_spec("equipment.mash_tun_loss_l").is_some());
        assert!(diagnostic_spec("equipment.kettle_opening_cm").is_none(), "removed from the profile and the overrides");
    }

    #[test]
    fn library_states_decide_or_stay_loading_truthfully() {
        assert!(!library_decidable(false, false, false, false));
        assert!(!library_decidable(false, false, false, true));
        assert!(library_decidable(true, false, false, false));
        assert!(!library_decidable(true, false, false, true));
        assert!(library_decidable(true, false, true, true));
        assert!(library_decidable(false, true, false, true), "a failed fetch is decided");
    }

    #[test]
    fn summary_keeps_selected_profile_loading_until_the_library_can_decide() {
        let mut document = RecipeDocumentV1::default();
        document.equipment_profile_id = Some("profile-1".into());
        assert!(matches!(EquipmentSummary::from_saved_document(&document).source, EquipmentSource::Loading { .. }));
        assert!(matches!(EquipmentSummary::from_document_with_library(&document, None, false).source, EquipmentSource::Loading { .. }));
        assert!(matches!(EquipmentSummary::from_document_with_library(&document, None, true).source, EquipmentSource::Unavailable { .. }));
    }

    #[test]
    fn summary_names_the_profile_or_recipe_only_equipment_and_flags_unavailable_selection() {
        let mut document = RecipeDocumentV1::default();
        assert_eq!(EquipmentSummary::from_document(&document, None).identity(), "Recipe-only equipment");
        document.equipment_profile_id = Some("p".into());
        let active = profile(Default::default(), false);
        assert_eq!(EquipmentSummary::from_document(&document, Some(&active)).identity(), "Profile: Garage");
        let archived = profile(Default::default(), true);
        assert!(matches!(EquipmentSummary::from_document(&document, Some(&archived)).source, EquipmentSource::Unavailable { .. }));
        assert!(matches!(EquipmentSummary::from_document(&document, None).source, EquipmentSource::Unavailable { .. }));
        document.equipment_overrides.boil_loss_l_per_hour = Some(3.0);
        assert_eq!(EquipmentSummary::from_document(&document, None).overrides(), "1 recipe override");
    }

    #[test]
    fn hints_distinguish_inherited_effective_and_overridden_values() {
        assert_eq!(override_hint("L/h", 1.0, None, Some(3.0), Some("Garage"), true), "Inherited: effective 3 L/h from Garage.");
        assert_eq!(
            override_hint("L/h", 1.0, Some(4.0), Some(3.0), Some("Garage"), true),
            "Overridden for this recipe: effective 4 L/h. Inherited: 3 L/h from Garage."
        );
        assert_eq!(override_hint("%", 100.0, Some(0.7), None, None, false), "Overridden for this recipe: effective 70 %. Inherited: no profile selected.");
        assert!(override_hint("L", 1.0, None, None, Some("Garage"), false).contains("not available"));
    }

    fn lines(method: CoolingMethodV1, overridden: bool) -> Vec<String> {
        let effective = EquipmentSettingsV1 { cooling_method: Some(method), cooling_flow_rate_l_per_minute: Some(4.0), ..Default::default() };
        let overrides = EquipmentSettingsV1 { cooling_method: overridden.then_some(method), ..Default::default() };
        effective_lines(&effective, &overrides, true).into_iter().map(|(line, _)| line).collect()
    }

    #[test]
    fn the_cooling_method_and_flow_rate_are_listed_with_their_origin() {
        let found = lines(CoolingMethodV1::CounterflowChiller, true);
        assert_eq!(
            found,
            vec![
                "Flow rate: 4 L/min · from profile",
                "Cooling method: Counterflow chiller · recipe override",
            ]
        );
        assert_eq!(
            lines(CoolingMethodV1::Natural, false),
            vec!["Flow rate: 4 L/min · from profile", "Cooling method: Natural · from profile"]
        );
    }
}
