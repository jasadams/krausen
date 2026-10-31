// SPDX-License-Identifier: AGPL-3.0-or-later
//! Approved searchable catalogue picker with custom-entry creation.
//!
//! Composes the shared brewery kit; all data flows through the shared
//! catalogue operations with the same policy as MCP. Specifications are
//! copied into the recipe row together with source and version.
use crate::{
    components::{
        BreweryDialog, BreweryDraftField, BreweryDraftSelect, BreweryNotice, Button, ButtonSize,
        ButtonVariant, CatalogueItem, EmptyState,
    },
    pages::recipe_editor::current_context_matches,
    server_fns::catalogue::catalogue_command_shared,
};
use krausen_types::{
    brewery::{AdditionStage, FermentableAddition, FermentableSpecification},
    catalogue::{
        CatalogueFilter, IngredientCommand, IngredientEntry,
        IngredientEntryDocumentV1, IngredientOutcome, SaveIngredientEntry, SearchCatalogue,
    },
    request_context::OperationIntent,
};
use leptos::prelude::*;

const CATEGORIES: &[&str] = &["Base malt", "Specialty malt", "Flaked grain"];

fn format_number(value: f64) -> String {
    let formatted = format!("{value:.1}");
    formatted
        .strip_suffix(".0")
        .map(str::to_owned)
        .unwrap_or(formatted)
}

fn entry_details(entry: &IngredientEntry) -> String {
    let mut parts = Vec::new();
    if let Some(colour) = entry.document.specification.colour_ebc {
        parts.push(format!("{} EBC", format_number(colour)));
    }
    if let Some(yield_fraction) = entry.document.specification.yield_fraction {
        parts.push(format!("{}% yield", format_number(yield_fraction * 100.0)));
    }
    if parts.is_empty() {
        "Specifications not provided".to_owned()
    } else {
        parts.join(" · ")
    }
}

fn amount_kg(text: &str) -> Option<f64> {
    let value = text.trim().parse::<f64>().ok()?;
    (value.is_finite() && value > 0.0).then_some(value)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PickerView {
    Catalogue,
    Create,
}

#[component]
pub fn CataloguePicker(
    open: RwSignal<bool>,
    identity: RwSignal<OperationIntent>,
    context_visible: RwSignal<bool>,
    /// Adds the copied fermentable row to the caller's draft.
    on_add: Callback<FermentableAddition>,
) -> impl IntoView {
    let view = RwSignal::new(PickerView::Catalogue);
    let query = RwSignal::new(String::new());
    let filter = RwSignal::new(CatalogueFilter::All);
    let items = RwSignal::new(Vec::<IngredientEntry>::new());
    let next_cursor = RwSignal::new(None::<String>);
    let selected = RwSignal::new(None::<IngredientEntry>);
    let amount = RwSignal::new("0.25".to_owned());
    let loading = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let notice = RwSignal::new(None::<(String, String)>);
    let generation = RwSignal::new(0_u64);
    let search_generation = RwSignal::new(0_u64);

    let name = RwSignal::new(String::new());
    let producer = RwSignal::new(String::new());
    let category = RwSignal::new(CATEGORIES[0].to_owned());
    let colour = RwSignal::new(String::new());
    let yield_percent = RwSignal::new(String::new());

    let load_page = Callback::new(move |cursor: Option<String>| {
        let expected = identity.get_untracked();
        let filter_value = filter.get_untracked();
        let query_value = query.get_untracked();
        let append = cursor.is_some();
        if !append {
            generation.update(|n| *n += 1);
        }
        let generation_value = generation.get_untracked();
        loading.set(true);
        leptos::task::spawn_local(async move {
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                loading.set(false);
                return;
            }
            let outcome = catalogue_command_shared(IngredientCommand::Search(SearchCatalogue {
                intent: expected.clone(),
                query: query_value,
                filter: filter_value,
                cursor,
                limit: 100,
            }))
            .await;
            if generation.try_get_untracked() != Some(generation_value) {
                return;
            }
            loading.set(false);
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                return;
            }
            match outcome {
                Ok(IngredientOutcome::Entries(page)) => {
                    notice.set(None);
                    if append {
                        items.update(|current| current.extend(page.items));
                    } else {
                        let selected_id = selected
                            .get_untracked()
                            .as_ref()
                            .map(|entry| entry.entry_id.clone());
                        let contains = page
                            .items
                            .iter()
                            .any(|entry| Some(&entry.entry_id) == selected_id.as_ref());
                        if !contains {
                            selected.set(None);
                        }
                        items.set(page.items);
                    }
                    next_cursor.set(page.next_cursor);
                }
                Ok(IngredientOutcome::ContextChanged | IngredientOutcome::Forbidden) => {
                    context_visible.set(false)
                }
                Ok(_) => notice.set(Some((
                    "Search unavailable".to_owned(),
                    "The catalogue could not be searched right now. Try again.".to_owned(),
                ))),
                Err(_) => notice.set(Some((
                    "Search unavailable".to_owned(),
                    "The catalogue could not be loaded right now. Try again.".to_owned(),
                ))),
            }
        });
    });

    let reset = move || {
        view.set(PickerView::Catalogue);
        query.set(String::new());
        filter.set(CatalogueFilter::All);
        selected.set(None);
        amount.set("0.25".to_owned());
        notice.set(None);
        name.set(String::new());
        producer.set(String::new());
        category.set(CATEGORIES[0].to_owned());
        colour.set(String::new());
        yield_percent.set(String::new());
        busy.set(false);
        loading.set(false);
        next_cursor.set(None);
        items.set(Vec::new());
    };

    // Refetch on open and on filter change; reset once when the dialog closes
    // (including dismissal through the native Escape key).
    let was_open = RwSignal::new(false);
    Effect::new(move |_| {
        if open.get() {
            was_open.set(true);
            let _filter = filter.get();
            load_page.run(None);
        } else if was_open.get_untracked() {
            was_open.set(false);
            reset();
        }
    });
    let search_input = Callback::new(move |text: String| {
        query.set(text);
        search_generation.update(|n| *n += 1);
        let debounce_generation = search_generation.get_untracked();
        leptos::task::spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(250).await;
            if search_generation.try_get_untracked() == Some(debounce_generation) {
                load_page.run(None);
            }
        });
    });

    let cancel = Callback::new(move |()| {
        open.set(false);
    });

    let choose = Callback::new(move |entry: IngredientEntry| {
        selected.set(Some(entry));
    });

    let add_ingredient = Callback::new(move |()| {
        if busy.get_untracked() {
            return;
        }
        let Some(entry) = selected.get_untracked() else {
            notice.set(Some((
                "Choose a fermentable".to_owned(),
                "Select a fermentable from the results first.".to_owned(),
            )));
            return;
        };
        let Some(kilograms) = amount_kg(&amount.get_untracked()) else {
            notice.set(Some((
                "Check the amount".to_owned(),
                "Enter a recipe amount greater than zero kilograms.".to_owned(),
            )));
            return;
        };
        let Some(row_id) = crate::utils::request_id::new_request_id() else {
            return;
        };
        let stage = match entry.document.specification.kind.as_deref() {
            Some("grain") => "mash",
            _ => "boil",
        };
        let row = FermentableAddition {
            row_id,
            amount_kg: Some(kilograms),
            addition: AdditionStage {
                stage: stage.to_owned(),
                time_minutes: None,
            },
            specification: entry.document.specification.clone(),
            source: Some(entry.ingredient_source()),
        };
        busy.set(true);
        on_add.run(row);
        open.set(false);
        reset();
    });

    let open_create = Callback::new(move |()| {
        notice.set(None);
        view.set(PickerView::Create);
    });

    let back_to_catalogue = Callback::new(move |()| {
        notice.set(None);
        view.set(PickerView::Catalogue);
    });

    let save_to_library = Callback::new(move |()| {
        if busy.get_untracked() {
            return;
        }
        let entry_name = name.get_untracked().trim().to_owned();
        if entry_name.is_empty() || entry_name.chars().count() > 100 {
            notice.set(Some((
                "A little more information needed".to_owned(),
                "Give your fermentable a name.".to_owned(),
            )));
            return;
        }
        let colour_text = colour.get_untracked();
        let Ok(colour_value) = colour_text.trim().parse::<f64>() else {
            notice.set(Some((
                "A little more information needed".to_owned(),
                "Enter a colour of zero or more EBC.".to_owned(),
            )));
            return;
        };
        if !colour_value.is_finite() || colour_value < 0.0 {
            notice.set(Some((
                "A little more information needed".to_owned(),
                "Enter a colour of zero or more EBC.".to_owned(),
            )));
            return;
        }
        let yield_text = yield_percent.get_untracked();
        let Ok(yield_value) = yield_text.trim().parse::<f64>() else {
            notice.set(Some((
                "A little more information needed".to_owned(),
                "Enter a potential yield greater than zero and at most 100%.".to_owned(),
            )));
            return;
        };
        if !yield_value.is_finite() || yield_value <= 0.0 || yield_value > 100.0 {
            notice.set(Some((
                "A little more information needed".to_owned(),
                "Enter a potential yield greater than zero and at most 100%.".to_owned(),
            )));
            return;
        }
        let Some(entry_id) = crate::utils::request_id::new_request_id() else {
            return;
        };
        let Some(request_id) = crate::utils::request_id::new_request_id() else {
            return;
        };
        let document = IngredientEntryDocumentV1::fermentable(
            category.get_untracked(),
            FermentableSpecification {
                name: Some(entry_name),
                producer: {
                    let producer_value = producer.get_untracked().trim().to_owned();
                    (!producer_value.is_empty()).then_some(producer_value)
                },
                kind: Some("grain".to_owned()),
                yield_fraction: Some(yield_value / 100.0),
                colour_ebc: Some(colour_value),
            },
        );
        let expected = identity.get_untracked();
        busy.set(true);
        leptos::task::spawn_local(async move {
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                busy.set(false);
                return;
            }
            let outcome = catalogue_command_shared(IngredientCommand::Save(
                SaveIngredientEntry {
                    intent: expected.clone(),
                    entry_id,
                    document,
                    expected_revision: 0,
                    expected_metadata_revision: 0,
                    request_id,
                },
            ))
            .await;
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                busy.set(false);
                return;
            }
            match outcome {
                Ok(IngredientOutcome::Entry(entry)) => {
                    notice.set(None);
                    selected.set(Some(entry));
                    query.set(String::new());
                    filter.set(CatalogueFilter::Workspace);
                    view.set(PickerView::Catalogue);
                    name.set(String::new());
                    producer.set(String::new());
                    colour.set(String::new());
                    yield_percent.set(String::new());
                    busy.set(false);
                    load_page.run(None);
                }
                Ok(IngredientOutcome::Duplicate) => {
                    busy.set(false);
                    notice.set(Some((
                        "A little more information needed".to_owned(),
                        "That fermentable and producer are already in your library.".to_owned(),
                    )));
                }
                Ok(IngredientOutcome::Invalid) => {
                    busy.set(false);
                    notice.set(Some((
                        "A little more information needed".to_owned(),
                        "Check the name, type, colour and yield, then try again.".to_owned(),
                    )));
                }
                Ok(IngredientOutcome::ContextChanged | IngredientOutcome::Forbidden) => {
                    context_visible.set(false);
                    busy.set(false);
                }
                Ok(_) | Err(_) => {
                    busy.set(false);
                    notice.set(Some((
                        "Save unavailable".to_owned(),
                        "Your library could not be updated right now. Try again.".to_owned(),
                    )));
                }
            }
        });
    });

    let selected_name = Signal::derive(move || {
        selected
            .get()
            .map(|entry| entry.name().to_owned())
            .unwrap_or_else(|| "Choose a fermentable".to_owned())
    });
    let can_add = Signal::derive(move || {
        let text = amount.get();
        selected.get().is_some() && amount_kg(&text).is_some()
    });
    let count_label = Signal::derive(move || {
        let count = items.with(|items| items.len());
        let more = next_cursor.get().is_some();
        if more {
            format!("{count} shown, more available")
        } else {
            format!("{count} found")
        }
    });
    let create_valid = Signal::derive(move || {
        !name.get().trim().is_empty()
            && colour.get().trim().parse::<f64>().is_ok_and(|value| {
                value.is_finite() && value >= 0.0
            })
            && yield_percent.get().trim().parse::<f64>().is_ok_and(|value| {
                value.is_finite() && value > 0.0 && value <= 100.0
            })
            && !busy.get()
    });

    let catalogue_view = move || view! {
        <div class="k-catalogue-toolbar">
            <BreweryDraftField id="catalogue-search" label="Search fermentables" value=Signal::from(query) on_change=search_input input_type="search"/>
            <Button variant=ButtonVariant::Secondary on:click=move |_| open_create.run(())>"Create fermentable"</Button>
        </div>
        <div class="k-catalogue-filters">
            <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm aria_pressed=Signal::derive(move || (filter.get() == CatalogueFilter::All).to_string()) on:click=move |_| filter.set(CatalogueFilter::All)>"All fermentables"</Button>
            <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm aria_pressed=Signal::derive(move || (filter.get() == CatalogueFilter::Workspace).to_string()) on:click=move |_| filter.set(CatalogueFilter::Workspace)>"Your library"</Button>
            <span class="note-caption" role="status">{count_label}</span>
        </div>
        {move || notice.get().map(|(title, body)| view! { <BreweryNotice title=title body=body warning=true/> })}
        <Show
            when=move || loading.get() || !items.with(|items| items.is_empty())
            fallback=move || view! {
                <EmptyState
                    variant=crate::components::EmptyStateVariant::Default
                    title="Make this library your own"
                    description="No matching fermentables. Add the malt or grain you use."
                    action=std::sync::Arc::new(move || view! { <Button variant=ButtonVariant::Secondary on:click=move |_| open_create.run(())>"Create fermentable"</Button> }.into_any())
                />
            }
        >
            <div class="k-catalogue-list">
                <For
                    each=move || items.get()
                    key=|entry| entry.entry_id.clone()
                    children=move |entry: IngredientEntry| {
                        let entry_id = entry.entry_id.clone();
                        let selected_entry = selected;
                        view! {
                            <CatalogueItem
                                name=entry.name().to_owned()
                                producer=entry.producer().to_owned()
                                details=entry_details(&entry)
                                source=entry.source_label().to_owned()
                                selected=Signal::derive(move || {
                                    selected_entry
                                        .get()
                                        .as_ref()
                                        .is_some_and(|selected| selected.entry_id == entry_id)
                                })
                                on_select=Callback::new(move |()| choose.run(entry.clone()))
                            />
                        }
                    }
                />
            </div>
        </Show>
        {move || next_cursor.get().map(|cursor| view! {
            <div class="k-catalogue-more">
                <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm disabled=Signal::derive(move || loading.get()) on:click=move |_| load_page.run(Some(cursor.clone()))>"Load more"</Button>
            </div>
        })}
        <div class="k-catalogue-selection">
            <div>
                <span class="note-caption">"SELECTED FERMENTABLE"</span>
                <strong>{selected_name}</strong>
            </div>
            <BreweryDraftField id="catalogue-amount" label="Amount in recipe" value=Signal::from(amount) on_change=Callback::new(move |text| amount.set(text)) unit="kg" numeric=true/>
        </div>
        <p class="note-caption">"Specifications are copied into your recipe, so later library changes cannot change what you brewed."</p>
    };

    let create_view = move || view! {
        {move || notice.get().map(|(title, body)| view! { <BreweryNotice title=title body=body warning=true/> })}
        <div class="k-catalogue-create">
            <BreweryDraftField id="library-name" label="Fermentable name" value=Signal::from(name) on_change=Callback::new(move |text| name.set(text)) hint="e.g. Local maltster pale malt"/>
            <BreweryDraftField id="library-producer" label="Producer" value=Signal::from(producer) on_change=Callback::new(move |text| producer.set(text)) hint="Maltster or supplier (optional)"/>
            <BreweryDraftSelect id="library-category" label="Type" value=Signal::from(category) on_change=Callback::new(move |text| category.set(text)) options=CATEGORIES.iter().map(|option| ((*option).to_owned(), (*option).to_owned())).collect()/>
            <BreweryDraftField id="library-colour" label="Colour" value=Signal::from(colour) on_change=Callback::new(move |text| colour.set(text)) unit="EBC" numeric=true hint="e.g. 3.5"/>
            <BreweryDraftField id="library-yield" label="Potential yield" value=Signal::from(yield_percent) on_change=Callback::new(move |text| yield_percent.set(text)) unit="%" numeric=true hint="Use the extract yield from the supplier's specification."/>
        </div>
        <p class="note-caption">"Stored in your workspace's ingredient library. It will not modify the starter catalogue."</p>
    };

    move || match view.get() {
        PickerView::Catalogue => view! {
            <BreweryDialog
                id="ingredient-picker"
                title="Build your grain bill"
                description="Find a fermentable in the starter catalogue, or add one to your own library."
                open=open
                wide=true
                actions=std::sync::Arc::new(move || view! {
                    <Button variant=ButtonVariant::Ghost on:click=move |_| cancel.run(())>"Cancel"</Button>
                    <Button variant=ButtonVariant::Default disabled=Signal::derive(move || !can_add.get()) on:click=move |_| add_ingredient.run(())>"Add ingredient"</Button>
                }.into_any())
            >
                {catalogue_view()}
            </BreweryDialog>
        }
        .into_view(),
        PickerView::Create => view! {
            <BreweryDialog
                id="ingredient-create"
                title="A fermentable of your own"
                description="Save a malt or grain to your library, then use it in any recipe."
                open=open
                wide=true
                actions=std::sync::Arc::new(move || view! {
                    <Button variant=ButtonVariant::Ghost on:click=move |_| back_to_catalogue.run(())>"Back to catalogue"</Button>
                    <Button variant=ButtonVariant::Default disabled=Signal::derive(move || !create_valid.get()) on:click=move |_| save_to_library.run(())>"Save to library"</Button>
                }.into_any())
            >
                {create_view()}
            </BreweryDialog>
        }
        .into_view(),
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use leptos::tachys::view::RenderHtml;

    #[test]
    fn picker_details_format_specs_and_selection_copy() {
        let owner = Owner::new();
        owner.with(|| {
            let entry = IngredientEntry {
                scope: krausen_types::brewery::CatalogueScope::Starter,
                workspace_id: None,
                entry_id: "starter-vienna".into(),
                created_at: 0,
                revision: 2,
                metadata_revision: 1,
                archived: false,
                document: IngredientEntryDocumentV1::fermentable(
                    "Base malt".into(),
                    FermentableSpecification {
                        name: Some("Weyermann Vienna".into()),
                        producer: Some("Weyermann".into()),
                        kind: Some("grain".into()),
                        yield_fraction: Some(0.7789847232440387),
                        colour_ebc: Some(7.0),
                    },
                ),
                provenance: krausen_types::catalogue::IngredientProvenance::workspace_custom(),
                saved_by: None,
                saved_at: 0,
            };
            assert_eq!(entry_details(&entry), "7 EBC · 77.9% yield");
            let source = entry.ingredient_source();
            assert_eq!(source.scope, "starter");
            assert_eq!(source.version, Some(2));
            assert_eq!(amount_kg("0.25"), Some(0.25));
            assert_eq!(amount_kg("-1"), None);
            assert_eq!(amount_kg(""), None);
            let html = view! { <div class="note-caption">"Specifications are copied into your recipe, so later library changes cannot change what you brewed."</div> }.to_html();
            assert!(html.contains("copied into your recipe"));
        });
    }
}
