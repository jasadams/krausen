// SPDX-License-Identifier: AGPL-3.0-or-later

//! Live showcase: these are the components used by the recipe editor.
use crate::components::brewery_kit::BreweryBadge;
use crate::components::{
    BreweryDialog, BreweryDisclosure, BreweryField,
    BreweryCompactIngredientRow, BreweryMetric, BreweryNotice, BreweryPanel, BrewerySaveStatus, BrewerySelect, BreweryTabs,
    BreweryTextArea, Button, ButtonSize, ButtonVariant, CatalogueItem, PrintableRecipe, SaveState,
    StyleRange, StyleState, SwatchRole, TokenSwatch,
    BreweryActionGroup, BreweryDraftField, BreweryFieldRow, BreweryPopover, BrewerySection,
    MetricDensity, NoticeAnnouncement, NoticeTone, PanelPresentation,
};
use leptos::prelude::*;
use phosphor_leptos::Icon;

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
            policy_version: "krausen-formulation-v2".into(),
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
    let grain_name = RwSignal::new("Pilsner malt".to_owned());
    let show_grain = RwSignal::new(true);
    let style = RwSignal::new("example-a".to_owned());
    let notes = RwSignal::new("A clean, malt-forward lager.".to_owned());
    let active_tab = RwSignal::new("recipe".to_owned());
    let selected_item = RwSignal::new(false);
    let search_query = RwSignal::new(String::new());
    let popup = RwSignal::new(false);
    let nested_popup = RwSignal::new(false);
    let field_error = RwSignal::new(None::<String>);
    let settled = RwSignal::new(String::new());
    let hop_name = RwSignal::new("Hallertauer Mittelfrüh — long shared schedule example".to_owned());
    let hop_amount = RwSignal::new("28.0".to_owned());
    let hop_minutes = RwSignal::new("60".to_owned());
    let hop_stage = RwSignal::new("boil".to_owned());
    let changing_popup = RwSignal::new(false);
    let more_memberships = RwSignal::new(false);
    let temporary_popup = RwSignal::new(false);
    let show_temporary = RwSignal::new(true);
    view! {
        <div class="kit-showcase">
            <header class="kit-showcase__heading"><p class="kit-showcase__eyebrow">"Krausen / Design system / 01"</p><h1>"Good design, "<em>"on repeat."</em></h1><p>"One considered system. Every ingredient, every interaction, every screen."</p><div class="kit-showcase__badges"><BreweryBadge tone="accent">"Living component library"</BreweryBadge><BreweryBadge>"Approved direction"</BreweryBadge></div></header>
            <div class="kit-showcase__principles"><div><span>"01 / Warm & precise"</span><p>"A little craft. A lot of clarity."</p></div><div><span>"02 / Component first"</span><p>"Same building blocks. Everywhere."</p></div><div><span>"03 / Quietly useful"</span><p>"Nothing between you and the recipe."</p></div></div>
            <div class="kit-showcase__grid">
                <BreweryPanel title="A grounded palette" eyebrow="Foundations / Colour" description="Semantic tokens, rather than page-specific colours.">
                    <div class="kit-showcase__swatches"><TokenSwatch role=SwatchRole::Background/><TokenSwatch role=SwatchRole::Surface/><TokenSwatch role=SwatchRole::Ink/><TokenSwatch role=SwatchRole::Copper/><TokenSwatch role=SwatchRole::Sage/><TokenSwatch role=SwatchRole::Border/></div>
                    <div class="kit-showcase__badges"><BreweryBadge tone="success">"Saved"</BreweryBadge><BreweryBadge tone="warning">"Unsaved"</BreweryBadge><BreweryBadge tone="accent">"All grain"</BreweryBadge></div>
                </BreweryPanel>
                <BreweryPanel title="Type with a point of view" eyebrow="Foundations / Typography">
                    <div class="kit-showcase__stack"><p class="kit-type-specimen">"Made to be brewed."</p><p>"DM Sans keeps the everyday clear and effortless."</p><p class="kit-number-specimen">"1.045　 20.5 L　 78%"</p><p class="kit-showcase__token">"INSTRUMENT SERIF / DM SANS / GEIST MONO"</p></div>
                </BreweryPanel>
                <BreweryPanel title="Actions with hierarchy" eyebrow="Component / Button" description="A deliberate primary action. Supporting controls stay quiet."
                    actions=std::sync::Arc::new(move || view! { <Button variant=ButtonVariant::Ghost>"Back"</Button> }.into_any())
                    footer=std::sync::Arc::new(move || view! { <BreweryActionGroup end=true label="Example task actions"><Button variant=ButtonVariant::Ghost>"Cancel"</Button><Button>"Save recipe"</Button></BreweryActionGroup> }.into_any())>
                    <div class="kit-showcase__stack"><div class="kit-showcase__row"><Button><Icon icon=phosphor_leptos::CHECK size="16px"/>"Save recipe"</Button><Button variant=ButtonVariant::Secondary><Icon icon=phosphor_leptos::PLUS size="16px"/>"Add ingredient"</Button><Button variant=ButtonVariant::Ghost>"View history"</Button></div><div class="kit-showcase__row"><Button disabled=true>"Saving…"</Button><Button variant=ButtonVariant::Secondary size=ButtonSize::Icon aria_label="Equipment settings"><Icon icon=phosphor_leptos::GEAR size="16px"/></Button><Button variant=ButtonVariant::Destructive size=ButtonSize::Sm>"Delete"</Button></div><p class="kit-showcase__token">"Button · variant / size / icon / disabled"</p></div>
                </BreweryPanel>
                <BreweryPanel title="Ordinary anchored actions" description="Tab leaves naturally; Escape returns to the opener. No menu roles or arrow-key promise.">
                    <BreweryActionGroup label="Popover examples">
                        <BreweryPopover id="showcase-actions" label="Recipe actions" open=popup return_focus_id="showcase-popover-successor">
                            <a href="#showcase-popover-successor">"View source context"</a>
                            <Button variant=ButtonVariant::Ghost on:click=move |_| { popup.set(false); settled.set("Example action selected".into()); }>"Use selected source"</Button>
                            <Button variant=ButtonVariant::Secondary disabled=true>"Export unavailable source"</Button>
                            <p class="note-caption">"Save a source before exporting. This example performs no business command."</p>
                        </BreweryPopover>
                        <Button variant=ButtonVariant::Secondary on:click=move |_| dialog.set(true)>"Open Navigation example"</Button>
                        <BreweryPopover id="showcase-changing-actions" label="Changing actions example" open=changing_popup>
                            <Button variant=ButtonVariant::Secondary on:click=move |_| more_memberships.set(true)>"Load example memberships"</Button>
                            <Show when=move || more_memberships.get()>{(1..=20).map(|number| view! { <Button variant=ButtonVariant::Ghost>{format!("Loaded membership {number}")}</Button> }).collect_view()}</Show>
                        </BreweryPopover>
                        <Show when=move || show_temporary.get()>
                            <BreweryPopover id="showcase-temporary-actions" label="Temporary actions example" open=temporary_popup return_focus_id="showcase-popover-successor">
                                <Button variant=ButtonVariant::Secondary on:click=move |_| show_temporary.set(false)>"Finish temporary example"</Button>
                            </BreweryPopover>
                        </Show>
                    </BreweryActionGroup>
                    <h3 id="showcase-popover-successor" tabindex="-1">"After actions"</h3>
                    <p role="status">{move || settled.get()}</p>
                </BreweryPanel>
                <BreweryPanel title="Forms without the friction" eyebrow="Component / Field & Select" description="Labels, units and validation stay visible.">
                    <BrewerySection title="Source fields"><BreweryFieldRow><BreweryField id="show-volume" label="Batch volume" value=volume unit="L" numeric=true hint="Volume into the fermenter"/><BrewerySelect id="show-style" label="Style" value=style options=vec![("example-a".into(), "Example style A".into()), ("example-b".into(), "Example style B".into())]/><BreweryField id="show-invalid" label="Amount" value=invalid unit="kg" numeric=true error="Enter a non-negative amount"/><BreweryField id="show-name" label="Recipe name" value=name/><BreweryTextArea id="show-notes" label="Notes" value=notes/></BreweryFieldRow></BrewerySection>
                    <BrewerySection title="Raw field states"><BreweryFieldRow>
                        <BreweryDraftField id="show-raw" label="Raw quantity" unit="kg" numeric=true value=Signal::from(grain) on_change=Callback::new(move |next: String| { grain.set(next); field_error.set(None); }) on_blur=Callback::new(move |()| field_error.set(Some("Demonstration of a settled field message; raw text is retained.".into()))) error={Signal::<Option<String>>::from(field_error)} hint="Type a transient value, then blur; this example does not parse or normalize."/>
                        <BreweryField id="show-readonly" label="Read-only source amount" unit="kg" numeric=true value=grain read_only=true hint="Readable and focusable, but not editable"/>
                        <BreweryField id="show-disabled" label="Unavailable amount" unit="kg" numeric=true value=grain disabled=true hint="Unavailable while this example is pending"/>
                    </BreweryFieldRow></BrewerySection>
                </BreweryPanel>
                <BreweryPanel title="Measurements" presentation=PanelPresentation::Summary mobile_compact=true description="Style ranges are guidance, not a save restriction.">
                    <crate::components::BreweryBeerPortrait/>
                    <div class="kit-showcase__metrics"><BreweryMetric label="Original gravity" value="1.049" unit="SG"><StyleRange label="Original gravity" min=1.044 max=1.048 value=Some(1.049) state=StyleState::Above unit="SG" digits=3/></BreweryMetric><BreweryMetric label="Bitterness" value="—" unit="IBU"><StyleRange label="Bitterness" min=16.0 max=22.0 value=None state=StyleState::Awaiting unit="IBU"/></BreweryMetric></div>
                </BreweryPanel>
                <BreweryPanel title="Brew measurements" description="Frozen planned targets and selected readings remain distinct.">
                    <div class="kit-showcase__table-scroll" tabindex="0" role="region" aria-label="Brew measurements table"><table class="print-sheet__table"><thead><tr><th scope="col">"Measurement"</th><th scope="col">"Planned target"</th><th scope="col">"Selected actual"</th><th scope="col">"Comparison"</th></tr></thead><tbody>
                        <crate::components::BrewMeasurementRow label="Original gravity".to_owned() basis="Specific gravity".to_owned() planned=crate::components::brew_history_presentation::measurement_value_or(Some(1.050), "sg", "No planned target") actual=crate::components::brew_history_presentation::measurement_value_or(Some(1.056), "sg", "No reading recorded") comparison=crate::components::brew_history_presentation::measurement_status_text(true, Some(0.006), "sg", None)/>
                        <crate::components::BrewMeasurementRow label="Batch volume".to_owned() planned=crate::components::brew_history_presentation::measurement_value_or(Some(20.5), "l", "No planned target") actual=crate::components::brew_history_presentation::measurement_value_or(None, "l", "No reading recorded") comparison="No reading recorded.".to_owned()/>
                    </tbody></table></div>
                    <dl class="print-sheet__facts"><crate::components::BrewProvenanceFact label="Recipe revision".to_owned() value="Revision 3".to_owned()/></dl>
                </BreweryPanel>
                <BreweryPanel title="Ingredients" description="Editable quantities stay with their source specification.">
                    <BreweryCompactIngredientRow label=Signal::derive(move || hop_name.get()) description=Signal::derive(|| "Hop schedule example — ordinary decisions remain outside secondary details".to_owned())
                        name_field=view! { <BreweryField id="show-hop-name" label="Hop name" value=hop_name/> }.into_any()
                        amount_field=view! { <BreweryField id="show-hop-amount" label="Hop amount" unit="g" numeric=true value=hop_amount/> }.into_any()
                        schedule=view! { <BreweryFieldRow><BrewerySelect id="show-hop-stage" label="Hop stage" value=hop_stage options=vec![("boil".into(), "Boil".into()), ("whirlpool".into(), "Whirlpool".into())]/><BreweryField id="show-hop-minutes" label="Contact time" unit="min" numeric=true value=hop_minutes/></BreweryFieldRow> }.into_any()
                        details=view! { <BreweryField id="show-hop-alpha" label="Alpha acid example" unit="%" value=RwSignal::new("4.5".to_owned()) numeric=true/> }.into_any()
                        on_remove=Callback::new(move |()| settled.set("Example removal requested; demonstration row retained.".into()))/>
                    <Show when=move || show_grain.get() fallback=move || view! { <Button variant=ButtonVariant::Secondary on:click=move |_| show_grain.set(true)>"Restore example"</Button> }>
                        <BreweryCompactIngredientRow label=Signal::derive(move || { let name = grain_name.get(); if name.trim().is_empty() { "Unnamed fermentable".to_owned() } else { name } }) description=Signal::derive(|| "Weyermann · 3 EBC".to_owned()) name_field=view! { <BreweryField id="show-grain-name" label="Name" value=grain_name/> }.into_any() amount_field=view! { <BreweryField id="show-grain-amount" label="Amount" unit="kg" value=grain numeric=true/> }.into_any() details=view! { <p>"Source specification and stage"</p> }.into_any() on_remove=Callback::new(move |()| show_grain.set(false))/>
                    </Show>
                </BreweryPanel>
                <BreweryPanel title="Save states" description="A draft is separate from a saved revision.">
                    <BreweryActionGroup label="Pending task actions"><Button pending=true>"Save changes"</Button><Button variant=ButtonVariant::Ghost disabled=true>"Cancel"</Button></BreweryActionGroup>
                    <BreweryActionGroup><BrewerySaveStatus state=SaveState::Saved quiet=true/><BrewerySaveStatus state=SaveState::Unsaved quiet=true/><BrewerySaveStatus state=SaveState::Saving quiet=true/><BrewerySaveStatus state=SaveState::Conflict quiet=true/><BrewerySaveStatus state=SaveState::Error quiet=true/></BreweryActionGroup>
                    <BreweryMetric label="Supporting water" value="28.36" unit="L" density=MetricDensity::Quiet>{()}</BreweryMetric>
                    <BreweryMetric label="Compact estimate" value="1.049" unit="SG" density=MetricDensity::Compact>{()}</BreweryMetric>
                </BreweryPanel>
                <BreweryPanel title="Ingredient catalogue" description="A copied specification stays with the recipe.">
                    <div class="k-catalogue-list">
                        <CatalogueItem name="Vienna malt" producer="Weyermann" details="7.0 EBC · 77.9% yield" source="Starter catalogue" selected=selected_item on_select=Callback::new(move |()| selected_item.update(|selected| *selected = !*selected))/>
                        <CatalogueItem name="My local pale malt" producer="Your maltster" details="4.2 EBC · 81.0% yield" source="Your library" selected=Signal::derive(move || false)/>
                    </div>
                    <div class="k-catalogue-toolbar">
                        <BreweryField id="show-catalogue-search" label="Search fermentables" value=search_query input_type="search"/>
                        <Button variant=ButtonVariant::Secondary>"Create fermentable"</Button>
                    </div>
                    <div class="k-catalogue-filters">
                        <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm aria_pressed="true">"All fermentables"</Button>
                        <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm aria_pressed="false">"Your library"</Button>
                        <span class="note-caption" role="status">"2 shown"</span>
                    </div>
                </BreweryPanel>
                <BreweryPanel title="Tabs" description="Use the arrow keys to move between sections.">
                    <BreweryTabs id="show-tabs" label="Recipe sections" active=active_tab items=vec![("recipe".into(), "Recipe".into()), ("history".into(), "History".into()), ("brews".into(), "Brews".into())]/>
                    <div role="tabpanel" id="show-tabs-panel-recipe" aria-labelledby="show-tabs-tab-recipe" hidden=move || active_tab.get() != "recipe">"Recipe section"</div>
                    <div role="tabpanel" id="show-tabs-panel-history" aria-labelledby="show-tabs-tab-history" hidden=move || active_tab.get() != "history">"History section"</div>
                    <div role="tabpanel" id="show-tabs-panel-brews" aria-labelledby="show-tabs-tab-brews" hidden=move || active_tab.get() != "brews">"Brews section"</div>
                </BreweryPanel>
                <BreweryPanel title="Feedback" description="Status is expressed in words as well as colour.">
                    <BreweryNotice title="Example save failed" body="Keep this page open and retry when the connection is available. Your local values remain editable." tone=NoticeTone::Error/>
                    <div class="kit-showcase__stack"><BreweryNotice title="Draft incomplete" body="Add a batch volume to see the affected estimates." warning=true/><BreweryNotice title="Recipe saved" body="Your revision is available to everyone in this workspace." announcement=NoticeAnnouncement::Settled/><Button variant=ButtonVariant::Secondary on:click=move |_| dialog.set(true)>"Open dialog"</Button></div>
                </BreweryPanel>
            </div>
            <BreweryDisclosure title="Printable recipe and process summary">
                <p class="note-caption">"The same shared summary is used by the editor print preview and the MCP operation. A saved revision and an unsaved draft are identified separately."</p>
                <div class="print-preview__sheet"><PrintableRecipe summary=printable_example()/></div>
            </BreweryDisclosure>
            <BreweryDialog id="showcase-dialog" title="Keep your changes?" description="A saved recipe revision is separate from your draft." open=dialog><p>"You can return to this recipe before deciding."</p><BreweryPopover id="showcase-nested-account" label="Account actions example" open=nested_popup><a href="#showcase-popover-successor">"Profile example"</a>{(1..=30).map(|number| view! { <Button variant=ButtonVariant::Ghost>{format!("Workspace example {number}{}", if number == 1 { " (current)" } else { "" })}</Button> }).collect_view()}</BreweryPopover></BreweryDialog>
        </div>
    }
}
