// SPDX-License-Identifier: AGPL-3.0-or-later

//! Live showcase: these are the components used by the recipe editor.
use crate::components::{
    BreweryDialog, BreweryField, BreweryIngredientRow, BreweryMetric, BreweryNotice, BreweryPanel,
    BrewerySaveStatus, BrewerySelect, BreweryTabs, BreweryTextArea, Button, ButtonSize,
    ButtonVariant, CatalogueItem, SaveState, StyleRange, StyleState, SwatchRole, TokenSwatch,
};
use leptos::prelude::*;

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
                    <Show when=move || show_grain.get() fallback=move || view! { <Button variant=ButtonVariant::Secondary on:click=move |_| show_grain.set(true)>"Restore example"</Button> }>
                        <BreweryIngredientRow name="Pilsner malt" description="Weyermann · 3 EBC" unit="kg" amount=grain on_remove=Callback::new(move |()| show_grain.set(false))/>
                    </Show>
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
            <BreweryDialog id="showcase-dialog" title="Keep your changes?" description="A saved recipe revision is separate from your draft." open=dialog><p>"You can return to this recipe before deciding."</p></BreweryDialog>
        </div>
    }
}
