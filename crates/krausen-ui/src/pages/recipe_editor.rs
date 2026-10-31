// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recipe editor over the shared, authorized recipe operations.
use crate::{
    components::{
        BreweryDisclosure, BreweryDraftArea, BreweryDraftField, BreweryDraftSelect, BreweryNotice,
        BreweryPanel, Button,
    },
    pages::recipe_draft::RecipeDraft,
    server_fns::recipe_editor::{read_recipe, save_recipe},
};
use krausen_types::{
    brewery::RecipeDocumentV1,
    recipe_editor::{RecipeReadOutcome, RecipeSaveOutcome, RecipeSnapshot},
};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

#[component]
pub fn RecipeEditorPage() -> impl IntoView {
    let params = use_params_map();
    let load = LocalResource::new(move || {
        let id = params.read().get("id").unwrap_or_default();
        async move {
            if id == "new" {
                Ok(None)
            } else {
                read_recipe(id).await.map(Some)
            }
        }
    });
    view! { <Suspense fallback=|| view! { <p>"Loading recipe…"</p> }>
        {move || Suspend::new(async move { match load.await {
            Ok(None) => view! { <RecipeForm initial=None/> }.into_any(),
            Ok(Some(RecipeReadOutcome::Found(snapshot))) => view! { <RecipeForm initial=Some(snapshot)/> }.into_any(),
            Ok(Some(RecipeReadOutcome::NotFound)) => view! { <BreweryNotice title="Recipe not found" body="It may have been archived or belongs to another workspace."/> }.into_any(),
            Ok(Some(RecipeReadOutcome::Forbidden)) => view! { <BreweryNotice title="Access denied" body="You no longer have access to this workspace." warning=true/> }.into_any(),
            _ => view! { <BreweryNotice title="Unable to load recipe" body="Try again when the connection is available." warning=true/> }.into_any(),
        } })}
    </Suspense> }
}

#[derive(Clone)]
struct PendingRecipeSave {
    recipe_id: Option<String>,
    expected: Option<i64>,
    document: RecipeDocumentV1,
    request_id: String,
    generation: u64,
}

#[component]
fn RecipeForm(initial: Option<RecipeSnapshot>) -> impl IntoView {
    let recipe_id = RwSignal::new(initial.as_ref().map(|s| s.recipe_id.clone()));
    let revision = RwSignal::new(initial.as_ref().map(|s| s.revision));
    let draft = RwSignal::new(RecipeDraft::new(
        initial.map(|s| s.document).unwrap_or_default(),
    ));
    let status = RwSignal::new(String::new());
    let pending = RwSignal::new(false);
    let retry = RwSignal::new(None::<PendingRecipeSave>);
    let title = Signal::derive(move || draft.with(|d| d.document().title.clone()));
    let title_change =
        Callback::new(move |value: String| draft.update(|d| d.edit(|doc| doc.title = value)));
    let notes =
        Signal::derive(move || draft.with(|d| d.document().notes.clone().unwrap_or_default()));
    let description = Signal::derive(move || {
        draft.with(|d| d.document().description.clone().unwrap_or_default())
    });
    let description_change = Callback::new(move |value: String| {
        draft.update(|d| d.edit(|doc| doc.description = (!value.is_empty()).then_some(value)))
    });
    let notes_change = Callback::new(move |value: String| {
        draft.update(|d| d.edit(|doc| doc.notes = (!value.is_empty()).then_some(value)))
    });
    let mash_method = Signal::derive(move || {
        draft.with(|d| {
            d.document()
                .process
                .mash_water_method
                .clone()
                .unwrap_or_default()
        })
    });
    let mash_method_change = Callback::new(move |value: String| {
        draft.update(|d| {
            d.edit(|doc| doc.process.mash_water_method = (!value.is_empty()).then_some(value))
        })
    });

    let chilling = Signal::derive(move || {
        draft.with(|d| {
            match &d.document().equipment_overrides.chilling {
                None => "",
                Some(krausen_types::brewery::ChillingV1::Natural) => "natural",
                Some(krausen_types::brewery::ChillingV1::Forced { .. }) => "forced",
                Some(krausen_types::brewery::ChillingV1::Counterflow { .. }) => "counterflow",
            }
            .to_owned()
        })
    });
    let chilling_change = Callback::new(move |value: String| {
        draft.update(|d| {
            d.remove_number_fields("equipment_overrides.chilling.");
            d.edit(|doc| {
                doc.equipment_overrides.chilling = match value.as_str() {
                    "natural" => Some(krausen_types::brewery::ChillingV1::Natural),
                    "forced" => Some(krausen_types::brewery::ChillingV1::Forced {
                        rate_per_minute: None,
                        plateau_c: None,
                    }),
                    "counterflow" => Some(krausen_types::brewery::ChillingV1::Counterflow {
                        litres_per_minute: None,
                    }),
                    _ => None,
                };
            })
        })
    });
    let save = move |_| {
        if pending.get_untracked() {
            return;
        }
        let attempt = if let Some(attempt) = retry.get_untracked() {
            attempt
        } else {
            let snapshot = draft.with_untracked(|d| (d.snapshot(), d.generation));
            let Ok(document) = snapshot.0 else {
                status.set("Complete invalid number fields before saving.".into());
                return;
            };
            let Some(request_id) = crate::utils::request_id::new_request_id() else {
                status.set("Unable to create a secure save request.".into());
                return;
            };
            let attempt = PendingRecipeSave {
                recipe_id: recipe_id.get_untracked(),
                expected: revision.get_untracked(),
                document,
                request_id,
                generation: snapshot.1,
            };
            retry.set(Some(attempt.clone()));
            attempt
        };
        pending.set(true);
        status.set("Saving…".into());
        leptos::task::spawn_local(async move {
            #[cfg(target_arch = "wasm32")]
            let creating = attempt.recipe_id.is_none();
            let result = save_recipe(
                attempt.recipe_id,
                attempt.expected,
                attempt.document,
                attempt.request_id,
            )
            .await;
            pending.set(false);
            match result {
                Ok(RecipeSaveOutcome::Saved {
                    recipe_id: id,
                    revision: saved,
                }) => {
                    retry.set(None);
                    #[cfg(target_arch = "wasm32")]
                    if creating {
                        if let Some(window) = web_sys::window() {
                            let result = window.history().and_then(|history| {
                                history.replace_state_with_url(
                                    &wasm_bindgen::JsValue::NULL,
                                    "",
                                    Some(&format!("/recipe-editor/{id}")),
                                )
                            });
                            if result.is_err() {
                                tracing::warn!("Could not update the saved recipe address");
                            }
                        }
                    }
                    recipe_id.set(Some(id));
                    revision.set(Some(saved));
                    if draft.with_untracked(|d| d.generation) == attempt.generation {
                        draft.update(|d| d.dirty = false);
                        status.set(format!("Saved revision {saved}."));
                    } else {
                        status.set(format!(
                            "Saved revision {saved}; newer changes remain unsaved."
                        ));
                    }
                }
                Ok(RecipeSaveOutcome::Invalid(validation)) => {
                    retry.set(None);
                    if draft.with_untracked(|d| d.generation) == attempt.generation {
                        draft.update(|d| d.set_validation(validation));
                        status.set("Correct the invalid fields before saving.".into());
                    } else {
                        status.set(
                            "An earlier save was invalid; your newer changes remain unsaved."
                                .into(),
                        );
                    }
                }
                Ok(RecipeSaveOutcome::Conflict { actual, .. }) => {
                    retry.set(None);
                    status.set(format!(
                        "Changed elsewhere (current revision {actual}). Your draft is unchanged."
                    ));
                }
                Ok(RecipeSaveOutcome::ResultExpired { resource_id }) => {
                    status.set(match resource_id {
                        Some(id) => format!(
                            "The saved response expired. Reopen recipe {id} before saving again."
                        ),
                        None => {
                            "The saved response expired. Reopen the recipe before saving again."
                                .into()
                        }
                    });
                }
                Ok(RecipeSaveOutcome::RequestIdentityConflict) => {
                    status.set("The prior save request could not be verified. Reopen the recipe before saving again.".into());
                }
                Ok(RecipeSaveOutcome::NotFound) => {
                    retry.set(None);
                    status.set("This recipe is no longer available in this workspace. Your draft is still open.".into());
                }
                Ok(RecipeSaveOutcome::InvalidRequest) => {
                    retry.set(None);
                    status.set("The save request was invalid. Your draft is still open.".into());
                }
                Ok(RecipeSaveOutcome::Forbidden) => {
                    status.set("You no longer have access to this workspace.".into())
                }
                Ok(RecipeSaveOutcome::UnresolvedEquipmentProfile) => {
                    retry.set(None);
                    status.set("The selected equipment profile is unavailable.".into())
                }
                Ok(
                    RecipeSaveOutcome::UnresolvedIngredientSource
                    | RecipeSaveOutcome::InvalidProvenance,
                ) => {
                    retry.set(None);
                    status.set("An ingredient or import reference could not be verified.".into());
                }
                _ => status.set(
                    "Save failed. Retry to confirm the previous save; your draft is still open."
                        .into(),
                ),
            }
        });
    };
    view! { <div class="recipe-editor">
        <header class="recipe-editor__header"><div><p class="kit-showcase__eyebrow">"Your recipe"</p><h1>"Recipe editor"</h1><p>"Changes are saved only when you choose Save."</p></div><div class="recipe-editor__actions"><span role="status">{move || {
            let message = status.get();
            if draft.with(|d| d.dirty) && (message.is_empty() || message.starts_with("Saved revision")) { "Unsaved changes".to_owned() } else { message }
        }}</span><Button disabled=pending on:click=save>"Save recipe"</Button></div></header>
        <div class="recipe-editor__summary">
            <BreweryDraftField id="recipe-title" label="Recipe name" value=title on_change=title_change/>
            <NumberField id="recipe-batch" label="Batch volume" unit="L" path="batch_size_l" draft=draft read=|d: &RecipeDocumentV1| d.batch_size_l write=|d: &mut RecipeDocumentV1,n| d.batch_size_l=n/>
            <NumberField id="recipe-boil" label="Boil time" unit="min" path="boil_minutes" draft=draft read=|d: &RecipeDocumentV1| d.boil_minutes write=|d: &mut RecipeDocumentV1,n| d.boil_minutes=n/>
        </div>
        <div class="recipe-editor__layout"><div class="recipe-editor__sections">
            <IngredientEditor draft=draft/>
            <AdditionalIngredients draft=draft/>
            <ProcessSteps draft=draft/>
            <BreweryDisclosure title="Recipe process and notes"><div class="recipe-editor__fields">
                <NumberField id="recipe-attenuation" label="Whole-wort apparent attenuation" unit="fraction" path="process.apparent_attenuation_fraction" draft=draft read=|d: &RecipeDocumentV1| d.process.apparent_attenuation_fraction write=|d: &mut RecipeDocumentV1,n| d.process.apparent_attenuation_fraction=n/>
                <BreweryDraftArea id="recipe-description" label="Description" value=description on_change=description_change/>
                <BreweryDraftArea id="recipe-notes" label="Notes" value=notes on_change=notes_change/>
                <BreweryDraftSelect id="recipe-mash-method" label="Mash water method" value=mash_method on_change=mash_method_change options=vec![("".into(), "Not specified".into()), ("manual".into(), "Manual volume".into()), ("profile_thickness".into(), "From mash thickness".into())]/>
                <NumberField id="recipe-mash-water" label="Mash water" unit="L" path="process.mash_water_l" draft=draft read=|d: &RecipeDocumentV1| d.process.mash_water_l write=|d: &mut RecipeDocumentV1,n| d.process.mash_water_l=n/>
                <NumberField id="recipe-top-up" label="Kettle top-up" unit="L" path="process.kettle_top_up_l" draft=draft read=|d: &RecipeDocumentV1| d.process.kettle_top_up_l write=|d: &mut RecipeDocumentV1,n| d.process.kettle_top_up_l=n/>
                <NumberField id="recipe-whirlpool" label="Whirlpool time" unit="min" path="process.whirlpool_minutes" draft=draft read=|d: &RecipeDocumentV1| d.process.whirlpool_minutes write=|d: &mut RecipeDocumentV1,n| d.process.whirlpool_minutes=n/>
                <NumberField id="recipe-whirlpool-temp" label="Whirlpool temperature" unit="°C" path="process.whirlpool_temperature_c" draft=draft read=|d: &RecipeDocumentV1| d.process.whirlpool_temperature_c write=|d: &mut RecipeDocumentV1,n| d.process.whirlpool_temperature_c=n/>
            </div></BreweryDisclosure>
            <BreweryDisclosure title="Equipment settings">
                <Show when=move || draft.with(|d| d.document().equipment_profile_id.is_some())>
                    <BreweryNotice title="Equipment profile unavailable" body="The stored profile cannot be resolved yet. You may explicitly use the draft equipment settings instead." warning=true/>
                    <Button variant=crate::components::ButtonVariant::Secondary on:click=move |_| draft.update(|d| d.edit(|doc| doc.equipment_profile_id=None))>"Use draft equipment settings"</Button>
                </Show>
                <div class="recipe-editor__fields">
                <NumberField id="recipe-efficiency-override" label="Brewhouse efficiency override" unit="fraction" path="equipment_overrides.efficiency_override_fraction" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.efficiency_override_fraction write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.efficiency_override_fraction=n/>
                <NumberField id="recipe-efficiency" label="Mash efficiency" unit="fraction" path="equipment_overrides.mash_efficiency_fraction" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.mash_efficiency_fraction write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.mash_efficiency_fraction=n/>
                <NumberField id="recipe-boil-off" label="Boil-off" unit="L/h" path="equipment_overrides.boil_off_l_per_hour" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.boil_off_l_per_hour write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.boil_off_l_per_hour=n/>
                <NumberField id="recipe-loss" label="Transfer loss" unit="L" path="equipment_overrides.transfer_loss_l" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.transfer_loss_l write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.transfer_loss_l=n/>
                <NumberField id="recipe-deadspace" label="Pre-boil deadspace" unit="L" path="equipment_overrides.pre_boil_deadspace_l" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.pre_boil_deadspace_l write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.pre_boil_deadspace_l=n/>
                <NumberField id="recipe-absorption" label="Grain absorption" unit="L/kg" path="equipment_overrides.grain_absorption_l_per_kg" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.grain_absorption_l_per_kg write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.grain_absorption_l_per_kg=n/>
                <NumberField id="recipe-thickness" label="Mash thickness" unit="L/kg" path="equipment_overrides.mash_thickness_l_per_kg" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.mash_thickness_l_per_kg write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.mash_thickness_l_per_kg=n/>
                <NumberField id="recipe-capacity" label="Kettle capacity" unit="L" path="equipment_overrides.kettle_capacity_l" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.kettle_capacity_l write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.kettle_capacity_l=n/>
                <NumberField id="recipe-diameter" label="Kettle diameter" unit="cm" path="equipment_overrides.kettle_diameter_cm" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.kettle_diameter_cm write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.kettle_diameter_cm=n/>
                <NumberField id="recipe-opening" label="Kettle opening" unit="cm" path="equipment_overrides.kettle_opening_cm" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.kettle_opening_cm write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.kettle_opening_cm=n/>
                <NumberField id="recipe-boil-temp" label="Boil temperature" unit="°C" path="equipment_overrides.boil_temperature_c" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.boil_temperature_c write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.boil_temperature_c=n/>
                <BreweryDraftSelect id="recipe-chilling" label="Chilling" value=chilling on_change=chilling_change options=vec![("".into(), "Not specified".into()), ("natural".into(), "Natural".into()), ("forced".into(), "Forced".into()), ("counterflow".into(), "Counterflow".into())]/>
                <Show when=move || chilling.get() == "forced"><NumberField id="recipe-chilling-rate" label="Cooling rate" unit="per min" path="equipment_overrides.chilling.rate_per_minute" draft=draft read=|d: &RecipeDocumentV1| match &d.equipment_overrides.chilling { Some(krausen_types::brewery::ChillingV1::Forced{rate_per_minute,..}) => *rate_per_minute, _ => None } write=|d: &mut RecipeDocumentV1,n| if let Some(krausen_types::brewery::ChillingV1::Forced{rate_per_minute,..})=&mut d.equipment_overrides.chilling { *rate_per_minute=n }/><NumberField id="recipe-chilling-plateau" label="Cooling plateau" unit="°C" path="equipment_overrides.chilling.plateau_c" draft=draft read=|d: &RecipeDocumentV1| match &d.equipment_overrides.chilling { Some(krausen_types::brewery::ChillingV1::Forced{plateau_c,..}) => *plateau_c, _ => None } write=|d: &mut RecipeDocumentV1,n| if let Some(krausen_types::brewery::ChillingV1::Forced{plateau_c,..})=&mut d.equipment_overrides.chilling { *plateau_c=n }/></Show>
                <Show when=move || chilling.get() == "counterflow"><NumberField id="recipe-chilling-flow" label="Counterflow rate" unit="L/min" path="equipment_overrides.chilling.litres_per_minute" draft=draft read=|d: &RecipeDocumentV1| match &d.equipment_overrides.chilling { Some(krausen_types::brewery::ChillingV1::Counterflow{litres_per_minute}) => *litres_per_minute, _ => None } write=|d: &mut RecipeDocumentV1,n| if let Some(krausen_types::brewery::ChillingV1::Counterflow{litres_per_minute})=&mut d.equipment_overrides.chilling { *litres_per_minute=n }/></Show>
            </div></BreweryDisclosure>
        </div><aside class="recipe-editor__preview"><BreweryPanel title="Your formulation" description="Estimates use the current draft.">
            <p>"Awaiting preview."</p>
            <div role="status">{move || draft.with(|d| d.diagnostics().iter().map(|diagnostic| view! { <p>{diagnostic_message(diagnostic)}</p> }).collect_view())}</div>
        </BreweryPanel></aside></div>
    </div> }
}

#[component]
fn NumberField(
    id: &'static str,
    label: &'static str,
    unit: &'static str,
    path: &'static str,
    draft: RwSignal<RecipeDraft>,
    read: fn(&RecipeDocumentV1) -> Option<f64>,
    write: fn(&mut RecipeDocumentV1, Option<f64>),
) -> impl IntoView {
    let value = Signal::derive(move || draft.with(|d| d.number_text(path, read(d.document()))));
    let error = Signal::derive(move || draft.with(|d| d.number_error(path).map(str::to_owned)));
    let change = Callback::new(move |text| draft.update(|d| d.edit_number(path, text, write)));
    view! { <BreweryDraftField id=id label=label unit=unit value=value error=error on_change=change/> }
}

fn diagnostic_message(diagnostic: &krausen_types::validation::Diagnostic) -> String {
    let subject = if diagnostic.path.starts_with("fermentables[") {
        "Fermentable"
    } else if diagnostic.path.starts_with("hops[") {
        "Hop addition"
    } else if diagnostic.path.starts_with("yeast[") {
        "Yeast"
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

#[derive(Clone, Copy)]
enum IngredientKind {
    Fermentable,
    Hop,
}
impl IngredientKind {
    fn prefix(self) -> &'static str {
        match self {
            Self::Fermentable => "fermentables",
            Self::Hop => "hops",
        }
    }
}
#[component]
fn IngredientEditor(draft: RwSignal<RecipeDraft>) -> impl IntoView {
    let add_grain = move |_| {
        let Some(id) = crate::utils::request_id::new_request_id() else {
            return;
        };
        draft.update(|d| {
            d.edit(|doc| {
                doc.fermentables
                    .push(krausen_types::brewery::FermentableAddition {
                        row_id: id,
                        amount_kg: None,
                        addition: krausen_types::brewery::AdditionStage {
                            stage: String::new(),
                            time_minutes: None,
                        },
                        specification: krausen_types::brewery::FermentableSpecification {
                            name: None,
                            producer: None,
                            kind: None,
                            yield_fraction: None,
                            colour_ebc: None,
                        },
                        source: None,
                    })
            })
        });
    };
    let add_hop = move |_| {
        let Some(id) = crate::utils::request_id::new_request_id() else {
            return;
        };
        draft.update(|d| {
            d.edit(|doc| {
                doc.hops.push(krausen_types::brewery::HopAddition {
                    row_id: id,
                    amount_g: None,
                    stage: String::new(),
                    contact_minutes: None,
                    temperature_c: None,
                    specification: krausen_types::brewery::HopSpecification {
                        name: None,
                        producer: None,
                        alpha_fraction: None,
                    },
                    source: None,
                })
            })
        });
    };
    view! {
        <BreweryPanel title="Fermentables" description="Specify the grain, extract or sugar copied into this recipe.">
            <For each=move || draft.with(|d| d.document().fermentables.iter().map(|r| r.row_id.clone()).collect::<Vec<_>>()) key=|id| id.clone() children=move |id| view! { <IngredientFields draft=draft id=id kind=IngredientKind::Fermentable/> } />
            <Button variant=crate::components::ButtonVariant::Secondary on:click=add_grain>"Add fermentable"</Button>
        </BreweryPanel>
        <BreweryPanel title="Hops" description="Record each addition and its contact time.">
            <For each=move || draft.with(|d| d.document().hops.iter().map(|r| r.row_id.clone()).collect::<Vec<_>>()) key=|id| id.clone() children=move |id| view! { <IngredientFields draft=draft id=id kind=IngredientKind::Hop/> } />
            <Button variant=crate::components::ButtonVariant::Secondary on:click=add_hop>"Add hop"</Button>
        </BreweryPanel>
    }
}

#[derive(Clone, Copy)]
enum IngredientText {
    Name,
    Kind,
    Stage,
}
#[derive(Clone, Copy)]
enum IngredientNumber {
    Amount,
    YieldOrAlpha,
    ColourOrTime,
    StageDetail,
}

#[component]
fn IngredientFields(
    draft: RwSignal<RecipeDraft>,
    id: String,
    kind: IngredientKind,
) -> impl IntoView {
    let remove_id = id.clone();
    let remove = move |_| {
        let prefix = format!("{}[{remove_id}].", kind.prefix());
        draft.update(|d| {
            d.remove_number_fields(&prefix);
            d.edit(|doc| match kind {
                IngredientKind::Fermentable => doc.fermentables.retain(|r| r.row_id != remove_id),
                IngredientKind::Hop => doc.hops.retain(|r| r.row_id != remove_id),
            });
        });
    };
    let name_id = id.clone();
    let type_id = id.clone();
    let stage_id = id.clone();
    let quantity_id = id.clone();
    let measure_id = id.clone();
    let extra_id = id.clone();
    let stage_detail_id = id.clone();
    view! { <div class="recipe-editor__addition">
        <div class="recipe-editor__fields"><IngredientTextField draft=draft id=name_id kind=kind field=IngredientText::Name label="Name"/>
        <IngredientNumberField draft=draft id=quantity_id kind=kind field=IngredientNumber::Amount label="Amount" unit=if matches!(kind, IngredientKind::Hop) {"g"} else {"kg"}/></div>
        <BreweryDisclosure title="Addition details"><div class="recipe-editor__fields">
            {matches!(kind, IngredientKind::Fermentable).then(|| view! { <IngredientTextField draft=draft id=type_id kind=kind field=IngredientText::Kind label="Type"/> })}
            <IngredientTextField draft=draft id=stage_id kind=kind field=IngredientText::Stage label="Stage"/>
            <IngredientNumberField draft=draft id=measure_id kind=kind field=IngredientNumber::YieldOrAlpha label=if matches!(kind, IngredientKind::Hop) {"Alpha acid"} else {"Yield"} unit="fraction"/>
            <IngredientNumberField draft=draft id=extra_id kind=kind field=IngredientNumber::ColourOrTime label=if matches!(kind, IngredientKind::Hop) {"Contact time"} else {"Colour"} unit=if matches!(kind, IngredientKind::Hop) {"min"} else {"EBC"}/>
            <IngredientNumberField draft=draft id=stage_detail_id kind=kind field=IngredientNumber::StageDetail label=if matches!(kind, IngredientKind::Hop) {"Whirlpool temperature"} else {"Boil addition time"} unit=if matches!(kind, IngredientKind::Hop) {"°C"} else {"min"}/>
        </div></BreweryDisclosure>
        <Button variant=crate::components::ButtonVariant::GhostDestructive on:click=remove>"Remove addition"</Button>
    </div> }
}

fn ingredient_text(
    doc: &RecipeDocumentV1,
    id: &str,
    kind: IngredientKind,
    field: IngredientText,
) -> String {
    match kind {
        IngredientKind::Fermentable => {
            doc.fermentables
                .iter()
                .find(|r| r.row_id == id)
                .map(|r| match field {
                    IngredientText::Name => r.specification.name.clone().unwrap_or_default(),
                    IngredientText::Kind => r.specification.kind.clone().unwrap_or_default(),
                    IngredientText::Stage => r.addition.stage.clone(),
                })
        }
        IngredientKind::Hop => doc
            .hops
            .iter()
            .find(|r| r.row_id == id)
            .map(|r| match field {
                IngredientText::Name => r.specification.name.clone().unwrap_or_default(),
                IngredientText::Stage => r.stage.clone(),
                IngredientText::Kind => String::new(),
            }),
    }
    .unwrap_or_default()
}
fn set_ingredient_text(
    doc: &mut RecipeDocumentV1,
    id: &str,
    kind: IngredientKind,
    field: IngredientText,
    value: String,
) {
    let optional = || {
        if value.is_empty() {
            None
        } else {
            Some(value.clone())
        }
    };
    match kind {
        IngredientKind::Fermentable => {
            if let Some(r) = doc.fermentables.iter_mut().find(|r| r.row_id == id) {
                match field {
                    IngredientText::Name => r.specification.name = optional(),
                    IngredientText::Kind => r.specification.kind = optional(),
                    IngredientText::Stage => r.addition.stage = value,
                }
            }
        }
        IngredientKind::Hop => {
            if let Some(r) = doc.hops.iter_mut().find(|r| r.row_id == id) {
                match field {
                    IngredientText::Name => r.specification.name = optional(),
                    IngredientText::Stage => r.stage = value,
                    IngredientText::Kind => (),
                }
            }
        }
    }
}
#[component]
fn IngredientTextField(
    draft: RwSignal<RecipeDraft>,
    id: String,
    kind: IngredientKind,
    field: IngredientText,
    label: &'static str,
) -> impl IntoView {
    let input_id = format!(
        "{}-{id}-{}",
        kind.prefix(),
        match field {
            IngredientText::Name => "name",
            IngredientText::Kind => "type",
            IngredientText::Stage => "stage",
        }
    );
    let read_id = id.clone();
    let value = Signal::derive(move || {
        draft.with(|d| ingredient_text(d.document(), &read_id, kind, field))
    });
    let change = Callback::new(move |text| {
        let id = id.clone();
        draft.update(|d| d.edit(|doc| set_ingredient_text(doc, &id, kind, field, text)));
    });
    match field {
        IngredientText::Name => view! { <BreweryDraftField id=input_id label=label value=value on_change=change/> }.into_any(),
        IngredientText::Kind => view! { <BreweryDraftSelect id=input_id label="Type" value=value on_change=change options=vec![("".into(), "Select a type".into()), ("grain".into(), "Grain".into()), ("extract".into(), "Extract".into()), ("sugar".into(), "Sugar".into())]/> }.into_any(),
        IngredientText::Stage => {
            let options = match kind { IngredientKind::Fermentable => vec![("".into(), "Select a stage".into()), ("mash".into(), "Mash".into()), ("boil".into(), "Boil".into())], IngredientKind::Hop => vec![("".into(), "Select a stage".into()), ("boil".into(), "Boil".into()), ("whirlpool".into(), "Whirlpool".into()), ("dry_hop".into(), "Dry hop".into())] };
            view! { <BreweryDraftSelect id=input_id label="Stage" value=value on_change=change options=options/> }.into_any()
        }
    }
}
fn ingredient_number(
    doc: &RecipeDocumentV1,
    id: &str,
    kind: IngredientKind,
    field: IngredientNumber,
) -> Option<f64> {
    match kind {
        IngredientKind::Fermentable => {
            doc.fermentables
                .iter()
                .find(|r| r.row_id == id)
                .and_then(|r| match field {
                    IngredientNumber::Amount => r.amount_kg,
                    IngredientNumber::YieldOrAlpha => r.specification.yield_fraction,
                    IngredientNumber::ColourOrTime => r.specification.colour_ebc,
                    IngredientNumber::StageDetail => r.addition.time_minutes,
                })
        }
        IngredientKind::Hop => doc
            .hops
            .iter()
            .find(|r| r.row_id == id)
            .and_then(|r| match field {
                IngredientNumber::Amount => r.amount_g,
                IngredientNumber::YieldOrAlpha => r.specification.alpha_fraction,
                IngredientNumber::ColourOrTime => r.contact_minutes,
                IngredientNumber::StageDetail => r.temperature_c,
            }),
    }
}
fn set_ingredient_number(
    doc: &mut RecipeDocumentV1,
    id: &str,
    kind: IngredientKind,
    field: IngredientNumber,
    value: Option<f64>,
) {
    match kind {
        IngredientKind::Fermentable => {
            if let Some(r) = doc.fermentables.iter_mut().find(|r| r.row_id == id) {
                match field {
                    IngredientNumber::Amount => r.amount_kg = value,
                    IngredientNumber::YieldOrAlpha => r.specification.yield_fraction = value,
                    IngredientNumber::ColourOrTime => r.specification.colour_ebc = value,
                    IngredientNumber::StageDetail => r.addition.time_minutes = value,
                }
            }
        }
        IngredientKind::Hop => {
            if let Some(r) = doc.hops.iter_mut().find(|r| r.row_id == id) {
                match field {
                    IngredientNumber::Amount => r.amount_g = value,
                    IngredientNumber::YieldOrAlpha => r.specification.alpha_fraction = value,
                    IngredientNumber::ColourOrTime => r.contact_minutes = value,
                    IngredientNumber::StageDetail => r.temperature_c = value,
                }
            }
        }
    }
}
#[component]
fn IngredientNumberField(
    draft: RwSignal<RecipeDraft>,
    id: String,
    kind: IngredientKind,
    field: IngredientNumber,
    label: &'static str,
    unit: &'static str,
) -> impl IntoView {
    let path = format!(
        "{}[{id}].{}",
        kind.prefix(),
        match (kind, field) {
            (IngredientKind::Fermentable, IngredientNumber::Amount) => "amount_kg",
            (IngredientKind::Fermentable, IngredientNumber::YieldOrAlpha) =>
                "specification.yield_fraction",
            (IngredientKind::Fermentable, IngredientNumber::ColourOrTime) =>
                "specification.colour_ebc",
            (IngredientKind::Fermentable, IngredientNumber::StageDetail) => "addition.time_minutes",
            (IngredientKind::Hop, IngredientNumber::Amount) => "amount_g",
            (IngredientKind::Hop, IngredientNumber::YieldOrAlpha) => "specification.alpha_fraction",
            (IngredientKind::Hop, IngredientNumber::ColourOrTime) => "contact_minutes",
            (IngredientKind::Hop, IngredientNumber::StageDetail) => "temperature_c",
        }
    );
    let input_id = format!(
        "{}-{id}-{}",
        kind.prefix(),
        match field {
            IngredientNumber::Amount => "amount",
            IngredientNumber::YieldOrAlpha => "yield-alpha",
            IngredientNumber::ColourOrTime => "colour-time",
            IngredientNumber::StageDetail => "stage-detail",
        }
    );
    let read_path = path.clone();
    let read_id = id.clone();
    let error_path = path.clone();
    let value = Signal::derive(move || {
        draft.with(|d| {
            d.number_text(
                &read_path,
                ingredient_number(d.document(), &read_id, kind, field),
            )
        })
    });
    let error =
        Signal::derive(move || draft.with(|d| d.number_error(&error_path).map(str::to_owned)));
    let change = Callback::new(move |text| {
        let id = id.clone();
        let path = path.clone();
        draft.update(|d| {
            d.edit_number(&path, text, |doc, n| {
                set_ingredient_number(doc, &id, kind, field, n)
            })
        });
    });
    view! { <BreweryDraftField id=input_id label=label unit=unit value=value error=error on_change=change/> }
}

#[derive(Clone, Copy)]
enum AdditionalKind {
    Yeast,
    Other,
}
impl AdditionalKind {
    fn prefix(self) -> &'static str {
        match self {
            Self::Yeast => "yeast",
            Self::Other => "other_ingredients",
        }
    }
    fn rows(self, doc: &RecipeDocumentV1) -> &Vec<krausen_types::brewery::IngredientAddition> {
        match self {
            Self::Yeast => &doc.yeast,
            Self::Other => &doc.other_ingredients,
        }
    }
    fn rows_mut(
        self,
        doc: &mut RecipeDocumentV1,
    ) -> &mut Vec<krausen_types::brewery::IngredientAddition> {
        match self {
            Self::Yeast => &mut doc.yeast,
            Self::Other => &mut doc.other_ingredients,
        }
    }
}
#[component]
fn AdditionalIngredients(draft: RwSignal<RecipeDraft>) -> impl IntoView {
    view! { <AdditionalGroup draft=draft kind=AdditionalKind::Yeast/><AdditionalGroup draft=draft kind=AdditionalKind::Other/> }
}
#[component]
fn AdditionalGroup(draft: RwSignal<RecipeDraft>, kind: AdditionalKind) -> impl IntoView {
    let add = move |_| {
        let Some(id) = crate::utils::request_id::new_request_id() else {
            return;
        };
        draft.update(|d| {
            d.edit(|doc| {
                kind.rows_mut(doc)
                    .push(krausen_types::brewery::IngredientAddition {
                        row_id: id,
                        amount: None,
                        unit: None,
                        source: None,
                        specification: krausen_types::brewery::IngredientSpecification {
                            name: None,
                            producer: None,
                            attenuation_fraction: None,
                            extensions: Default::default(),
                        },
                    })
            })
        });
    };
    view! { <BreweryPanel title=if matches!(kind, AdditionalKind::Yeast) { "Yeast" } else { "Other ingredients" }>
        <For each=move || draft.with(|d| kind.rows(d.document()).iter().map(|r| r.row_id.clone()).collect::<Vec<_>>()) key=|id| id.clone() children=move |id| view! { <AdditionalFields draft=draft id=id kind=kind/> } />
        <Button variant=crate::components::ButtonVariant::Secondary on:click=add>"Add ingredient"</Button>
    </BreweryPanel> }
}
#[derive(Clone, Copy)]
enum AdditionalText {
    Name,
    Producer,
    Unit,
}
#[component]
fn AdditionalFields(
    draft: RwSignal<RecipeDraft>,
    id: String,
    kind: AdditionalKind,
) -> impl IntoView {
    let remove_id = id.clone();
    let remove = move |_| {
        let prefix = format!("{}[{remove_id}].", kind.prefix());
        draft.update(|d| {
            d.remove_number_fields(&prefix);
            d.edit(|doc| kind.rows_mut(doc).retain(|r| r.row_id != remove_id));
        });
    };
    let name_id = id.clone();
    let producer_id = id.clone();
    let unit_id = id.clone();
    let amount_id = id.clone();
    let attenuation_id = id.clone();
    view! { <div class="kit-showcase__stack">
        <AdditionalTextField draft=draft id=name_id kind=kind field=AdditionalText::Name label="Name"/>
        <AdditionalTextField draft=draft id=producer_id kind=kind field=AdditionalText::Producer label="Producer"/>
        <AdditionalTextField draft=draft id=unit_id kind=kind field=AdditionalText::Unit label="Unit"/>
        <AdditionalNumberField draft=draft id=amount_id kind=kind attenuation=false/>
        {matches!(kind, AdditionalKind::Yeast).then(|| view! { <AdditionalNumberField draft=draft id=attenuation_id kind=kind attenuation=true/> })}
        <Button variant=crate::components::ButtonVariant::GhostDestructive on:click=remove>"Remove ingredient"</Button>
    </div> }
}
#[component]
fn AdditionalTextField(
    draft: RwSignal<RecipeDraft>,
    id: String,
    kind: AdditionalKind,
    field: AdditionalText,
    label: &'static str,
) -> impl IntoView {
    let suffix = match field {
        AdditionalText::Name => "name",
        AdditionalText::Producer => "producer",
        AdditionalText::Unit => "unit",
    };
    let input_id = format!("{}-{id}-{suffix}", kind.prefix());
    let read_id = id.clone();
    let value = Signal::derive(move || {
        draft.with(|d| {
            kind.rows(d.document())
                .iter()
                .find(|r| r.row_id == read_id)
                .and_then(|r| match field {
                    AdditionalText::Name => r.specification.name.clone(),
                    AdditionalText::Producer => r.specification.producer.clone(),
                    AdditionalText::Unit => r.unit.clone(),
                })
                .unwrap_or_default()
        })
    });
    let change = Callback::new(move |text: String| {
        let id = id.clone();
        draft.update(|d| {
            d.edit(|doc| {
                if let Some(r) = kind.rows_mut(doc).iter_mut().find(|r| r.row_id == id) {
                    let value = (!text.is_empty()).then_some(text);
                    match field {
                        AdditionalText::Name => r.specification.name = value,
                        AdditionalText::Producer => r.specification.producer = value,
                        AdditionalText::Unit => r.unit = value,
                    }
                }
            })
        });
    });
    view! { <BreweryDraftField id=input_id label=label value=value on_change=change/> }
}
#[component]
fn AdditionalNumberField(
    draft: RwSignal<RecipeDraft>,
    id: String,
    kind: AdditionalKind,
    attenuation: bool,
) -> impl IntoView {
    let suffix = if attenuation { "attenuation" } else { "amount" };
    let path = format!("{}[{id}].{suffix}", kind.prefix());
    let input_id = format!("{}-{id}-{suffix}", kind.prefix());
    let read_id = id.clone();
    let read_path = path.clone();
    let error_path = path.clone();
    let value = Signal::derive(move || {
        draft.with(|d| {
            let n = kind
                .rows(d.document())
                .iter()
                .find(|r| r.row_id == read_id)
                .and_then(|r| {
                    if attenuation {
                        r.specification.attenuation_fraction
                    } else {
                        r.amount
                    }
                });
            d.number_text(&read_path, n)
        })
    });
    let error =
        Signal::derive(move || draft.with(|d| d.number_error(&error_path).map(str::to_owned)));
    let change = Callback::new(move |text| {
        let id = id.clone();
        let path = path.clone();
        draft.update(|d| {
            d.edit_number(&path, text, |doc, n| {
                if let Some(r) = kind.rows_mut(doc).iter_mut().find(|r| r.row_id == id) {
                    if attenuation {
                        r.specification.attenuation_fraction = n
                    } else {
                        r.amount = n
                    }
                }
            })
        });
    });
    view! { <BreweryDraftField id=input_id label=if attenuation {"Yeast attenuation (informational)"} else {"Amount"} value=value error=error on_change=change/> }
}

#[derive(Clone, Copy)]
enum StepKind {
    Mash,
    Fermentation,
}
impl StepKind {
    fn prefix(self) -> &'static str {
        match self {
            Self::Mash => "process.mash_steps",
            Self::Fermentation => "process.fermentation_steps",
        }
    }
    fn rows(self, doc: &RecipeDocumentV1) -> &Vec<krausen_types::brewery::ProcessStep> {
        match self {
            Self::Mash => &doc.process.mash_steps,
            Self::Fermentation => &doc.process.fermentation_steps,
        }
    }
    fn rows_mut(self, doc: &mut RecipeDocumentV1) -> &mut Vec<krausen_types::brewery::ProcessStep> {
        match self {
            Self::Mash => &mut doc.process.mash_steps,
            Self::Fermentation => &mut doc.process.fermentation_steps,
        }
    }
}
#[component]
fn ProcessSteps(draft: RwSignal<RecipeDraft>) -> impl IntoView {
    view! { <StepGroup draft=draft kind=StepKind::Mash/><StepGroup draft=draft kind=StepKind::Fermentation/> }
}
#[component]
fn StepGroup(draft: RwSignal<RecipeDraft>, kind: StepKind) -> impl IntoView {
    let add = move |_| {
        let Some(id) = crate::utils::request_id::new_request_id() else {
            return;
        };
        draft.update(|d| {
            d.edit(|doc| {
                kind.rows_mut(doc)
                    .push(krausen_types::brewery::ProcessStep {
                        row_id: id,
                        name: None,
                        time_minutes: None,
                        temperature_c: None,
                        volume_l: None,
                        extensions: Default::default(),
                    })
            })
        });
    };
    view! { <BreweryPanel title=if matches!(kind, StepKind::Mash) { "Mash steps" } else { "Fermentation steps" }>
        <For each=move || draft.with(|d| kind.rows(d.document()).iter().map(|r| r.row_id.clone()).collect::<Vec<_>>()) key=|id| id.clone() children=move |id| view! { <StepFields draft=draft id=id kind=kind/> } />
        <Button variant=crate::components::ButtonVariant::Secondary on:click=add>"Add step"</Button>
    </BreweryPanel> }
}
#[derive(Clone, Copy)]
enum StepNumber {
    Time,
    Temperature,
    Volume,
}
#[component]
fn StepFields(draft: RwSignal<RecipeDraft>, id: String, kind: StepKind) -> impl IntoView {
    let remove_id = id.clone();
    let remove = move |_| {
        let prefix = format!("{}[{remove_id}].", kind.prefix());
        draft.update(|d| {
            d.remove_number_fields(&prefix);
            d.edit(|doc| kind.rows_mut(doc).retain(|r| r.row_id != remove_id));
        });
    };
    let read_id = id.clone();
    let change_id = id.clone();
    let name_id = format!("{}-{id}-name", kind.prefix());
    let name = Signal::derive(move || {
        draft.with(|d| {
            kind.rows(d.document())
                .iter()
                .find(|r| r.row_id == read_id)
                .and_then(|r| r.name.clone())
                .unwrap_or_default()
        })
    });
    let change = Callback::new(move |text: String| {
        let id = change_id.clone();
        draft.update(|d| {
            d.edit(|doc| {
                if let Some(r) = kind.rows_mut(doc).iter_mut().find(|r| r.row_id == id) {
                    r.name = (!text.is_empty()).then_some(text)
                }
            })
        });
    });
    let time_id = id.clone();
    let temp_id = id.clone();
    let volume_id = id.clone();
    view! { <div class="kit-showcase__stack"><BreweryDraftField id=name_id label="Step name" value=name on_change=change/>
        <StepNumberField draft=draft id=time_id kind=kind field=StepNumber::Time label="Time" unit="min"/>
        <StepNumberField draft=draft id=temp_id kind=kind field=StepNumber::Temperature label="Temperature" unit="°C"/>
        <StepNumberField draft=draft id=volume_id kind=kind field=StepNumber::Volume label="Volume" unit="L"/>
        <Button variant=crate::components::ButtonVariant::GhostDestructive on:click=remove>"Remove step"</Button>
    </div> }
}
#[component]
fn StepNumberField(
    draft: RwSignal<RecipeDraft>,
    id: String,
    kind: StepKind,
    field: StepNumber,
    label: &'static str,
    unit: &'static str,
) -> impl IntoView {
    let suffix = match field {
        StepNumber::Time => "time_minutes",
        StepNumber::Temperature => "temperature_c",
        StepNumber::Volume => "volume_l",
    };
    let path = format!("{}[{id}].{suffix}", kind.prefix());
    let input_id = format!("{}-{id}-{suffix}", kind.prefix());
    let read_id = id.clone();
    let read_path = path.clone();
    let error_path = path.clone();
    let value = Signal::derive(move || {
        draft.with(|d| {
            let n = kind
                .rows(d.document())
                .iter()
                .find(|r| r.row_id == read_id)
                .and_then(|r| match field {
                    StepNumber::Time => r.time_minutes,
                    StepNumber::Temperature => r.temperature_c,
                    StepNumber::Volume => r.volume_l,
                });
            d.number_text(&read_path, n)
        })
    });
    let error =
        Signal::derive(move || draft.with(|d| d.number_error(&error_path).map(str::to_owned)));
    let change = Callback::new(move |text| {
        let id = id.clone();
        let path = path.clone();
        draft.update(|d| {
            d.edit_number(&path, text, |doc, n| {
                if let Some(r) = kind.rows_mut(doc).iter_mut().find(|r| r.row_id == id) {
                    match field {
                        StepNumber::Time => r.time_minutes = n,
                        StepNumber::Temperature => r.temperature_c = n,
                        StepNumber::Volume => r.volume_l = n,
                    }
                }
            })
        });
    });
    view! { <BreweryDraftField id=input_id label=label unit=unit value=value error=error on_change=change/> }
}
