// SPDX-License-Identifier: AGPL-3.0-or-later

//! Live showcase: these are the components used by the recipe editor.
use crate::components::{
    BreweryAddition, BreweryDialog, BreweryDisclosure, BreweryField, BreweryIngredientRow,
    BreweryMetric, BreweryNotice, BreweryPanel, BrewerySaveStatus, BrewerySelect, BreweryTabs,
    BreweryTextArea, Button, ButtonSize, ButtonVariant, CatalogueItem, PrintableRecipe, SaveState,
    StyleRange, StyleState, SwatchRole, TokenSwatch,
};
use leptos::prelude::*;

fn printable_example() -> krausen_types::process_summary::ProcessSummary {
    use krausen_types::{
        brewery::{ChillingV1, EquipmentSettingsV1, StyleReference},
        formulation::Targets,
        process_summary::{
            IngredientSection, ProcessSummary, SummaryContext, SummaryEquipment, SummaryIngredient,
            SummaryProcess, SummarySource, SummaryStep,
        },
        validation::Readiness,
    };
    let ingredient = |row_id: &str,
                      section: IngredientSection,
                      name: &str,
                      producer: Option<&str>,
                      amount: Option<f64>,
                      unit: &str| SummaryIngredient {
        row_id: row_id.into(),
        section,
        name: Some(name.into()),
        producer: producer.map(str::to_owned),
        amount,
        unit: Some(unit.into()),
        stage: None,
        time_minutes: None,
        temperature_c: None,
        fermentable_type: None,
        yield_fraction: None,
        colour_ebc: None,
        alpha_fraction: None,
        attenuation_fraction: None,
        source: None,
    };
    ProcessSummary {
        source: SummarySource::Saved {
            recipe_id: "showcase-recipe".into(),
            revision: 7,
        },
        title: "Munich Helles".into(),
        description: Some("A clean, malt-forward lager for the summer.".into()),
        style: Some(StyleReference {
            name: "Munich Helles".into(),
            edition: Some("BJCP 2021".into()),
            style_id: Some("4A".into()),
        }),
        batch_size_l: Some(20.0),
        boil_minutes: Some(60.0),
        notes: Some("Pitch at 10 °C and rise to 12 °C after two days.".into()),
        readiness: Readiness::Ready,
        diagnostics: vec![],
        targets: Targets {
            total_water_l: Some(28.36),
            pre_boil_l: Some(25.0),
            post_boil_l: Some(22.0),
            grain_absorption_l: Some(3.36),
            mash_water_l: Some(15.0),
            sparge_water_l: Some(13.36),
            kettle_water_l: Some(28.36),
            pre_boil_sg: Some(1.0378),
            original_sg: Some(1.042954545455),
            final_sg: Some(1.008590909091),
            abv_percent: Some(4.5),
            colour_ebc: Some(7.8),
            ibu: Some(21.0),
        },
        style_ranges: None,
        style_comparison: None,
        assumptions: vec![
            "volume_reference_20c".into(),
            "profile:brewhouse_loss_calibrated".into(),
        ],
        equipment: SummaryEquipment {
            profile_id: Some("showcase-equipment".into()),
            profile_name: Some("20 L kettle".into()),
            profile_revision: Some(3),
            provenance: "saved_profile".into(),
            settings: EquipmentSettingsV1 {
                mash_efficiency_fraction: Some(0.75),
                boil_off_l_per_hour: Some(3.0),
                transfer_loss_l: Some(2.0),
                grain_absorption_l_per_kg: Some(0.8),
                chilling: Some(ChillingV1::Natural),
                ..Default::default()
            },
        },
        context: SummaryContext {
            digest: Some("7d3c0f4ee4example".into()),
            engine_version: "krausen-calc-v1".into(),
            engine_build: "showcase".into(),
            policy_version: "krausen-formulation-v1".into(),
            native_schema_version: 1,
            result_schema_version: 1,
        },
        fermentables: vec![
            SummaryIngredient {
                fermentable_type: Some("grain".into()),
                yield_fraction: Some(0.8),
                colour_ebc: Some(3.0),
                stage: Some("mash".into()),
                ..ingredient(
                    "grain-1",
                    IngredientSection::Fermentable,
                    "Pilsner malt",
                    Some("Weyermann"),
                    Some(3.6),
                    "kg",
                )
            },
            SummaryIngredient {
                fermentable_type: Some("grain".into()),
                yield_fraction: Some(0.78),
                colour_ebc: Some(7.0),
                stage: Some("mash".into()),
                ..ingredient(
                    "grain-2",
                    IngredientSection::Fermentable,
                    "Vienna malt",
                    Some("Weyermann"),
                    Some(0.6),
                    "kg",
                )
            },
        ],
        hops: vec![SummaryIngredient {
            stage: Some("boil".into()),
            time_minutes: Some(60.0),
            alpha_fraction: Some(0.045),
            ..ingredient(
                "hop-1",
                IngredientSection::Hop,
                "Hallertauer Mittelfrüh",
                None,
                Some(28.0),
                "g",
            )
        }],
        yeast: vec![SummaryIngredient {
            attenuation_fraction: Some(0.78),
            ..ingredient(
                "yeast-1",
                IngredientSection::Yeast,
                "German Lager",
                Some("Wyeast 2308"),
                Some(1.0),
                "pack",
            )
        }],
        other_ingredients: vec![],
        process: SummaryProcess {
            apparent_attenuation_fraction: Some(0.78),
            mash_steps: vec![
                SummaryStep {
                    row_id: "mash-1".into(),
                    name: Some("Mash in".into()),
                    time_minutes: Some(60.0),
                    temperature_c: Some(63.0),
                    volume_l: Some(15.0),
                },
                SummaryStep {
                    row_id: "mash-2".into(),
                    name: Some("Sparge".into()),
                    time_minutes: Some(30.0),
                    temperature_c: Some(76.0),
                    volume_l: Some(13.36),
                },
            ],
            fermentation_steps: vec![SummaryStep {
                row_id: "ferment-1".into(),
                name: Some("Primary".into()),
                time_minutes: Some(14.0 * 24.0 * 60.0),
                temperature_c: Some(10.0),
                volume_l: None,
            }],
            whirlpool_minutes: Some(15.0),
            whirlpool_temperature_c: Some(95.0),
            mash_water_l: Some(15.0),
            mash_water_method: Some("manual".into()),
            kettle_top_up_l: Some(0.0),
            chilling: Some(ChillingV1::Natural),
            unsupported: Default::default(),
        },
    }
}

#[component]
pub fn DesignSystemPage() -> impl IntoView {
    let name = RwSignal::new("Munich Helles".to_owned());
    let volume = RwSignal::new("20".to_owned());
    let invalid = RwSignal::new("-1".to_owned());
    let dialog = RwSignal::new(false);
    let grain = RwSignal::new("4.20".to_owned());
    let show_grain = RwSignal::new(true);
    let style = RwSignal::new("example-a".to_owned());
    let notes = RwSignal::new("A clean, malt-forward lager.".to_owned());
    let active_tab = RwSignal::new("recipe".to_owned());
    let selected_item = RwSignal::new(false);
    view! {
        <div class="kit-showcase">
            <header class="kit-showcase__heading"><p class="kit-showcase__eyebrow">"Krausen design system"</p><h1>"A more considered brew day."</h1><p>"The shared controls, typography and calculation states used across your workspace."</p></header>
            <div class="kit-showcase__grid">
                <BreweryPanel title="A grounded palette" description="Semantic tokens keep every surface consistent.">
                    <div class="kit-showcase__swatches"><TokenSwatch role=SwatchRole::Background/><TokenSwatch role=SwatchRole::Surface/><TokenSwatch role=SwatchRole::Ink/><TokenSwatch role=SwatchRole::Copper/><TokenSwatch role=SwatchRole::Sage/><TokenSwatch role=SwatchRole::Border/></div>
                </BreweryPanel>
                <BreweryPanel title="Type with a point of view" description="Instrument Serif, DM Sans and Geist Mono.">
                    <div class="kit-showcase__stack"><p class="kit-type-specimen">"Made to be brewed."</p><p>"Clear, comfortable reading for the details that matter."</p><p class="kit-number-specimen">"1.045    20.5 L    78%"</p></div>
                </BreweryPanel>
                <BreweryPanel title="Actions" description="Clear actions and quiet alternatives.">
                    <div class="kit-showcase__row"><Button>"Save recipe"</Button><Button variant=ButtonVariant::Secondary>"Preview"</Button><Button variant=ButtonVariant::Ghost>"Cancel"</Button><Button variant=ButtonVariant::Destructive size=ButtonSize::Sm>"Delete"</Button><Button disabled=true>"Saving…"</Button></div>
                </BreweryPanel>
                <BreweryPanel title="Fields" description="Labels, units and validation stay visible.">
                    <div class="kit-showcase__stack"><BreweryField id="show-name" label="Recipe name" value=name/><BreweryField id="show-volume" label="Batch volume" value=volume unit="L" hint="Volume into the fermenter"/><BreweryField id="show-invalid" label="Amount" value=invalid unit="kg" error="Enter a non-negative amount"/><BrewerySelect id="show-style" label="Style" value=style options=vec![("example-a".into(), "Example style A".into()), ("example-b".into(), "Example style B".into())]/><BreweryTextArea id="show-notes" label="Notes" value=notes/></div>
                </BreweryPanel>
                <BreweryPanel title="Measurements" description="Style ranges are guidance, not a save restriction.">
                    <div class="kit-showcase__metrics"><BreweryMetric label="Original gravity" value="1.049" unit="SG"><StyleRange label="Original gravity" min=1.044 max=1.048 value=Some(1.049) state=StyleState::Above unit="SG" digits=3/></BreweryMetric><BreweryMetric label="Bitterness" value="—" unit="IBU"><StyleRange label="Bitterness" min=16.0 max=22.0 value=None state=StyleState::Awaiting unit="IBU"/></BreweryMetric></div>
                </BreweryPanel>
                <BreweryPanel title="Ingredients" description="Editable quantities stay with their source specification.">
                    <BreweryAddition>
                    <Show when=move || show_grain.get() fallback=move || view! { <Button variant=ButtonVariant::Secondary on:click=move |_| show_grain.set(true)>"Restore example"</Button> }>
                        <BreweryIngredientRow name="Pilsner malt" description="Weyermann · 3 EBC" unit="kg" amount=grain on_remove=Callback::new(move |()| show_grain.set(false))/>
                    </Show>
                    </BreweryAddition>
                </BreweryPanel>
                <BreweryPanel title="Save states" description="A draft is separate from a saved revision.">
                    <div class="kit-showcase__row"><BrewerySaveStatus state=SaveState::Saved/><BrewerySaveStatus state=SaveState::Unsaved/><BrewerySaveStatus state=SaveState::Saving/><BrewerySaveStatus state=SaveState::Conflict/><BrewerySaveStatus state=SaveState::Error/></div>
                </BreweryPanel>
                <BreweryPanel title="Ingredient catalogue" description="A copied specification stays with the recipe.">
                    <CatalogueItem name="Vienna malt" producer="Weyermann" details="7 EBC · 78% yield" source="Starter catalogue" selected=selected_item/>
                </BreweryPanel>
                <BreweryPanel title="Tabs" description="Use the arrow keys to move between sections.">
                    <BreweryTabs id="show-tabs" label="Recipe sections" active=active_tab items=vec![("recipe".into(), "Recipe".into()), ("history".into(), "History".into()), ("brews".into(), "Brews".into())]/>
                    <div role="tabpanel" id="show-tabs-panel-recipe" aria-labelledby="show-tabs-tab-recipe" hidden=move || active_tab.get() != "recipe">"Recipe section"</div>
                    <div role="tabpanel" id="show-tabs-panel-history" aria-labelledby="show-tabs-tab-history" hidden=move || active_tab.get() != "history">"History section"</div>
                    <div role="tabpanel" id="show-tabs-panel-brews" aria-labelledby="show-tabs-tab-brews" hidden=move || active_tab.get() != "brews">"Brews section"</div>
                </BreweryPanel>
                <BreweryPanel title="Feedback" description="Status is expressed in words as well as colour.">
                    <div class="kit-showcase__stack"><BreweryNotice title="Draft incomplete" body="Add a batch volume to see the affected estimates." warning=true/><BreweryNotice title="Recipe saved" body="Your revision is available to everyone in this workspace."/><Button variant=ButtonVariant::Secondary on:click=move |_| dialog.set(true)>"Open dialog"</Button></div>
                </BreweryPanel>
            </div>
            <BreweryDisclosure title="Printable recipe and process summary">
                <p class="note-caption">"The same shared summary is used by the editor print preview and the MCP operation. A saved revision and an unsaved draft are identified separately."</p>
                <div class="print-preview__sheet"><PrintableRecipe summary=printable_example()/></div>
            </BreweryDisclosure>
            <BreweryDialog id="showcase-dialog" title="Keep your changes?" description="A saved recipe revision is separate from your draft." open=dialog><p>"You can return to this recipe before deciding."</p></BreweryDialog>
        </div>
    }
}
