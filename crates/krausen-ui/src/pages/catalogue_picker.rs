// SPDX-License-Identifier: AGPL-3.0-or-later
//! Approved searchable catalogue picker with custom-entry creation, editing
//! and guarded archive/restore.
//!
//! Composes the shared brewery kit; all data flows through the shared
//! catalogue operations with the same policy as MCP. Specifications are
//! copied into the recipe row together with source and version. Save request
//! identities are allocated once per form/action session and retained across
//! uncertain outcomes so a retry can never mint a second command.
use crate::{
    components::{
        BreweryDialog, BreweryDraftField, BreweryDraftSelect, BreweryNotice, Button, ButtonSize,
        ButtonVariant, CatalogueItem, EmptyState,
    },
    pages::recipe_editor::current_context_matches,
    server_fns::catalogue::catalogue_command_shared,
};
use krausen_types::{
    brewery::{AdditionStage, CatalogueScope, FermentableAddition, FermentableSpecification},
    catalogue::{
        ArchiveIngredientEntry, CatalogueFilter, IngredientCommand, IngredientEntry,
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

/// Result badge: archived state is the more important label in library views.
fn entry_source_label(entry: &IngredientEntry) -> &'static str {
    if entry.archived {
        "Archived"
    } else {
        entry.source_label()
    }
}

fn amount_kg(text: &str) -> Option<f64> {
    let value = text.trim().parse::<f64>().ok()?;
    (value.is_finite() && value > 0.0).then_some(value)
}

/// Distinct terminal outcomes so expired/conflict/identity failures are never
/// collapsed into a generic retry message.
fn terminal_notice(outcome: &IngredientOutcome) -> Option<(String, String, bool)> {
    match outcome {
        IngredientOutcome::Duplicate => Some((
            "Already in your library".to_owned(),
            "That fermentable and producer are already in your library.".to_owned(),
            true,
        )),
        IngredientOutcome::Invalid => Some((
            "A little more information needed".to_owned(),
            "Check the name, type, colour and yield, then try again.".to_owned(),
            true,
        )),
        IngredientOutcome::Conflict { actual, .. } => Some((
            "Changed elsewhere".to_owned(),
            format!(
                "The library entry changed to revision {actual}. Reopen it to work from the current version."
            ),
            true,
        )),
        IngredientOutcome::Archived => Some((
            "Entry archived".to_owned(),
            "This library entry is archived. Restore it before editing.".to_owned(),
            true,
        )),
        IngredientOutcome::ResultExpired => Some((
            "Earlier confirmation expired".to_owned(),
            "Your earlier save completed, but its confirmation expired. The entry is available in the library.".to_owned(),
            false,
        )),
        IngredientOutcome::RequestIdentityConflict => Some((
            "Request already recorded differently".to_owned(),
            "This request was already recorded with different content, so nothing changed. Reopen the entry and try again.".to_owned(),
            true,
        )),
        IngredientOutcome::Forbidden | IngredientOutcome::ContextChanged => None,
        IngredientOutcome::Entry(_)
        | IngredientOutcome::Entries(_)
        | IngredientOutcome::NotFound => Some((
            "Save unavailable".to_owned(),
            "The library could not be updated right now. Try again.".to_owned(),
            true,
        )),
        IngredientOutcome::Unavailable => Some((
            "Save unavailable".to_owned(),
            "The library could not be reached. Try again.".to_owned(),
            true,
        )),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PickerView {
    Catalogue,
    Create,
    Edit,
}

/// Selected-fermentable summary and the guarded library management actions.
///
/// This is the real picker block, shared with the component tests so the
/// Edit/Archive/Restore gating cannot be reimplemented only in test code.
#[component]
fn CataloguePickerSelection(
    selected: RwSignal<Option<IngredientEntry>>,
    amount: RwSignal<String>,
    archive_retrying: RwSignal<bool>,
    #[prop(into)] busy: Signal<bool>,
    on_edit: Callback<()>,
    on_archive: Callback<bool>,
) -> impl IntoView {
    let selected_name = Signal::derive(move || {
        selected
            .get()
            .map(|entry| entry.name().to_owned())
            .unwrap_or_else(|| "Choose a fermentable".to_owned())
    });
    let selected_workspace = Signal::derive(move || {
        selected
            .get()
            .as_ref()
            .is_some_and(|entry| entry.scope == CatalogueScope::Workspace)
    });
    let selected_archived = Signal::derive(move || {
        selected
            .get()
            .as_ref()
            .is_some_and(|entry| entry.archived)
    });
    view! {
        <div class="k-catalogue-selection">
            <div>
                <span class="note-caption">"SELECTED FERMENTABLE"</span>
                <strong>{move || selected_name.get()}</strong>
            </div>
            <BreweryDraftField id="catalogue-amount" label="Amount in recipe" value=Signal::from(amount) on_change=Callback::new(move |text| amount.set(text)) unit="kg" numeric=true/>
        </div>
        {move || selected_workspace.get().then(|| view! {
            <div class="k-catalogue-manage">
                <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm disabled=Signal::derive(move || selected_archived.get() || busy.get() || archive_retrying.get()) on:click=move |_| on_edit.run(())>"Edit library entry"</Button>
                <Show when=move || selected_archived.get() fallback=move || view! {
                    <Button variant=ButtonVariant::GhostDestructive size=ButtonSize::Sm disabled=Signal::derive(move || busy.get()) on:click=move |_| on_archive.run(true)>{move || if archive_retrying.get() { "Retry previous request" } else { "Archive" }}</Button>
                }>
                    <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm disabled=Signal::derive(move || busy.get()) on:click=move |_| on_archive.run(false)>{move || if archive_retrying.get() { "Retry previous request" } else { "Restore" }}</Button>
                </Show>
            </div>
        })}
        <p class="note-caption">"Specifications are copied into your recipe, so later library changes cannot change what you brewed."</p>
    }
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
    let notice = RwSignal::new(None::<(String, String, bool)>);
    let generation = RwSignal::new(0_u64);
    let search_generation = RwSignal::new(0_u64);

    // Form state. entry_id/request_id are allocated once per form session and
    // retained across uncertain outcomes; they rotate only after a committed
    // result so a retry always replays the identical command.
    let name = RwSignal::new(String::new());
    let producer = RwSignal::new(String::new());
    let category = RwSignal::new(CATEGORIES[0].to_owned());
    let colour = RwSignal::new(String::new());
    let yield_percent = RwSignal::new(String::new());
    let edit_target = RwSignal::new(None::<IngredientEntry>);
    let entry_id = RwSignal::new(None::<String>);
    let request_id = RwSignal::new(None::<String>);
    let retrying = RwSignal::new(false);
    // The full command is retained across an uncertain outcome so a retry is
    // sent verbatim even if the editable form values later change.
    let pending_save = RwSignal::new(None::<SaveIngredientEntry>);

    // Archive/restore identity, stable for one selected-entry action session.
    let pending_archive = RwSignal::new(None::<ArchiveIngredientEntry>);
    let archive_retrying = RwSignal::new(false);

    let load_page = Callback::new(move |cursor: Option<String>| {
        let expected = identity.get_untracked();
        let filter_value = filter.get_untracked();
        let query_value = query.get_untracked();
        let include_archived = filter_value == CatalogueFilter::Workspace;
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
                include_archived,
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
                        if !contains && !archive_retrying.get_untracked() {
                            selected.set(None);
                            pending_archive.set(None);
                            archive_retrying.set(false);
                        }
                        items.set(page.items);
                    }
                    next_cursor.set(page.next_cursor);
                }
                Ok(IngredientOutcome::ContextChanged | IngredientOutcome::Forbidden) => {
                    context_visible.set(false)
                }
                Ok(other) => {
                    if let Some((title, body, warning)) = terminal_notice(&other) {
                        notice.set(Some((title, body, warning)));
                    }
                }
                Err(_) => notice.set(Some((
                    "Search unavailable".to_owned(),
                    "The catalogue could not be loaded right now. Try again.".to_owned(),
                    true,
                ))),
            }
        });
    });

    let clear_form = move || {
        name.set(String::new());
        producer.set(String::new());
        category.set(CATEGORIES[0].to_owned());
        colour.set(String::new());
        yield_percent.set(String::new());
        edit_target.set(None);
        entry_id.set(None);
        request_id.set(None);
        pending_save.set(None);
        retrying.set(false);
    };

    let reset = move || {
        view.set(PickerView::Catalogue);
        query.set(String::new());
        filter.set(CatalogueFilter::All);
        selected.set(None);
        amount.set("0.25".to_owned());
        notice.set(None);
        busy.set(false);
        loading.set(false);
        next_cursor.set(None);
        items.set(Vec::new());
        pending_archive.set(None);
        archive_retrying.set(false);
        clear_form();
    };

    // Refetch on open and on filter change; reset once when the dialog closes
    // (including dismissal through the native Escape key).
    let was_open = RwSignal::new(false);
    Effect::new(move |_| {
        if open.get() {
            was_open.set(true);
            let _filter = filter.get();
            // User-initiated open/filter changes clear stale feedback here.
            // Background search completion must not erase a form notice.
            notice.set(None);
            load_page.run(None);
        } else if was_open.get_untracked() {
            was_open.set(false);
            reset();
        }
    });
    let search_input = Callback::new(move |text: String| {
        query.set(text);
        notice.set(None);
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
        // Lock the target while an archive is uncertain or a request is in
        // flight; the retry must address the original entry.
        if busy.get_untracked() || archive_retrying.get_untracked() {
            return;
        }
        selected.set(Some(entry));
        pending_archive.set(None);
        archive_retrying.set(false);
        notice.set(None);
    });

    let begin_create = Callback::new(move |()| {
        if busy.get_untracked() || archive_retrying.get_untracked() {
            return;
        }
        notice.set(None);
        clear_form();
        pending_archive.set(None);
        let Some(new_entry_id) = crate::utils::request_id::new_request_id() else {
            return;
        };
        let Some(new_save_request_id) = crate::utils::request_id::new_request_id() else {
            return;
        };
        entry_id.set(Some(new_entry_id));
        request_id.set(Some(new_save_request_id));
        view.set(PickerView::Create);
    });

    let begin_edit = Callback::new(move |()| {
        if busy.get_untracked() || archive_retrying.get_untracked() {
            return;
        }
        let Some(entry) = selected.get_untracked() else {
            return;
        };
        if entry.scope != CatalogueScope::Workspace {
            return;
        }
        notice.set(None);
        pending_save.set(None);
        pending_archive.set(None);
        name.set(entry.name().to_owned());
        producer.set(entry.producer().to_owned());
        category.set(entry.document.category.clone());
        colour.set(
            entry
                .document
                .specification
                .colour_ebc
                .map(|value| value.to_string())
                .unwrap_or_default(),
        );
        yield_percent.set(
            entry
                .document
                .specification
                .yield_fraction
                .map(|value| format!("{}", value * 100.0))
                .unwrap_or_default(),
        );
        let Some(edit_request_id) = crate::utils::request_id::new_request_id() else {
            return;
        };
        entry_id.set(Some(entry.entry_id.clone()));
        request_id.set(Some(edit_request_id));
        retrying.set(false);
        edit_target.set(Some(entry));
        view.set(PickerView::Edit);
    });

    let back_to_catalogue = Callback::new(move |()| {
        notice.set(None);
        clear_form();
        pending_archive.set(None);
        archive_retrying.set(false);
        view.set(PickerView::Catalogue);
    });

    let add_ingredient = Callback::new(move |()| {
        if busy.get_untracked() {
            return;
        }
        let Some(entry) = selected.get_untracked() else {
            notice.set(Some((
                "Choose a fermentable".to_owned(),
                "Select a fermentable from the results first.".to_owned(),
                true,
            )));
            return;
        };
        if entry.archived {
            notice.set(Some((
                "Entry archived".to_owned(),
                "Restore this library entry before adding it to a recipe.".to_owned(),
                true,
            )));
            return;
        }
        let Some(kilograms) = amount_kg(&amount.get_untracked()) else {
            notice.set(Some((
                "Check the amount".to_owned(),
                "Enter a recipe amount greater than zero kilograms.".to_owned(),
                true,
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

    let save_entry = Callback::new(move |()| {
        if busy.get_untracked() {
            return;
        }
        let save = if retrying.get_untracked() {
            let Some(pending) = pending_save.get_untracked() else {
                notice.set(Some((
                    "Retry unavailable".to_owned(),
                    "Reopen the form to start a new save.".to_owned(),
                    true,
                )));
                return;
            };
            pending
        } else {
            let editing = edit_target.get_untracked();
            let session_entry_id = entry_id.get_untracked();
            let session_request_id = request_id.get_untracked();
            let (Some(target_entry_id), Some(target_request_id)) =
                (session_entry_id, session_request_id)
            else {
                return;
            };
            let entry_name = name.get_untracked().trim().to_owned();
            if entry_name.is_empty() || entry_name.chars().count() > 100 {
                notice.set(Some((
                    "A little more information needed".to_owned(),
                    "Give your fermentable a name.".to_owned(),
                    true,
                )));
                return;
            }
            let colour_text = colour.get_untracked();
            let Ok(colour_value) = colour_text.trim().parse::<f64>() else {
                notice.set(Some((
                    "A little more information needed".to_owned(),
                    "Enter a colour of zero or more EBC.".to_owned(),
                    true,
                )));
                return;
            };
            if !colour_value.is_finite() || colour_value < 0.0 {
                notice.set(Some((
                    "A little more information needed".to_owned(),
                    "Enter a colour of zero or more EBC.".to_owned(),
                    true,
                )));
                return;
            }
            let yield_text = yield_percent.get_untracked();
            let Ok(yield_value) = yield_text.trim().parse::<f64>() else {
                notice.set(Some((
                    "A little more information needed".to_owned(),
                    "Enter a potential yield greater than zero and at most 100%.".to_owned(),
                    true,
                )));
                return;
            };
            if !yield_value.is_finite() || yield_value <= 0.0 || yield_value > 100.0 {
                notice.set(Some((
                    "A little more information needed".to_owned(),
                    "Enter a potential yield greater than zero and at most 100%.".to_owned(),
                    true,
                )));
                return;
            }
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
            let save = SaveIngredientEntry {
                intent: identity.get_untracked(),
                entry_id: target_entry_id,
                document,
                expected_revision: editing.as_ref().map(|entry| entry.revision).unwrap_or(0),
                expected_metadata_revision: editing
                    .as_ref()
                    .map(|entry| entry.metadata_revision)
                    .unwrap_or(0),
                request_id: target_request_id,
            };
            pending_save.set(Some(save.clone()));
            save
        };
        let is_edit = save.expected_revision > 0;
        let command = IngredientCommand::Save(save);
        let expected = identity.get_untracked();
        busy.set(true);
        leptos::task::spawn_local(async move {
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                busy.set(false);
                return;
            }
            let outcome = catalogue_command_shared(command).await;
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                busy.set(false);
                return;
            }
            busy.set(false);
            match &outcome {
                Ok(IngredientOutcome::Entry(entry)) => {
                    notice.set(None);
                    retrying.set(false);
                    pending_save.set(None);
                    if is_edit {
                        // Committed: rotate the request identity, keep the
                        // edited entry current for a further guarded change.
                        request_id.set(crate::utils::request_id::new_request_id());
                        selected.set(Some(entry.clone()));
                        edit_target.set(Some(entry.clone()));
                        pending_archive.set(None);
                        view.set(PickerView::Catalogue);
                        load_page.run(None);
                    } else {
                        request_id.set(None);
                        entry_id.set(None);
                        selected.set(Some(entry.clone()));
                        query.set(String::new());
                        filter.set(CatalogueFilter::Workspace);
                        view.set(PickerView::Catalogue);
                        load_page.run(None);
                    }
                }
                Ok(IngredientOutcome::ResultExpired) => {
                    // The command committed earlier; its confirmation is gone.
                    // Discard the identity and show the fresh library state.
                    retrying.set(false);
                    pending_save.set(None);
                    notice.set(terminal_notice(
                        match &outcome { Ok(value) => value, Err(_) => unreachable!() },
                    ));
                    clear_form();
                    view.set(PickerView::Catalogue);
                    load_page.run(None);
                }
                Ok(IngredientOutcome::RequestIdentityConflict) => {
                    retrying.set(false);
                    pending_save.set(None);
                    notice.set(terminal_notice(
                        match &outcome { Ok(value) => value, Err(_) => unreachable!() },
                    ));
                    clear_form();
                    view.set(PickerView::Catalogue);
                    load_page.run(None);
                }
                Ok(IngredientOutcome::ContextChanged | IngredientOutcome::Forbidden) => {
                    pending_save.set(None);
                    context_visible.set(false)
                }
                Ok(other) => {
                    retrying.set(false);
                    pending_save.set(None);
                    notice.set(terminal_notice(other));
                }
                Err(_) => {
                    // Uncertain outcome: retain the full command so the next
                    // click replays it verbatim, independent of form edits.
                    retrying.set(true);
                    notice.set(Some((
                        "Save may not have completed".to_owned(),
                        "The response was lost before the result was confirmed. Retry the previous request; the same request cannot duplicate data.".to_owned(),
                        true,
                    )));
                }
            }
        });
    });

    let archive_entry = Callback::new(move |target_archived: bool| {
        if busy.get_untracked() {
            return;
        }
        let archive = if archive_retrying.get_untracked() {
            let Some(pending) = pending_archive.get_untracked() else {
                notice.set(Some((
                    "Retry unavailable".to_owned(),
                    "Reopen the library entry to try again.".to_owned(),
                    true,
                )));
                return;
            };
            pending
        } else {
            let Some(entry) = selected.get_untracked() else {
                return;
            };
            if entry.scope != CatalogueScope::Workspace {
                return;
            }
            let Some(request_id) = crate::utils::request_id::new_request_id() else {
                return;
            };
            let archive = ArchiveIngredientEntry {
                intent: identity.get_untracked(),
                entry_id: entry.entry_id.clone(),
                archived: target_archived,
                expected_metadata_revision: entry.metadata_revision,
                request_id,
            };
            pending_archive.set(Some(archive.clone()));
            archive
        };
        let command = IngredientCommand::Archive(archive);
        let expected = identity.get_untracked();
        busy.set(true);
        leptos::task::spawn_local(async move {
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                busy.set(false);
                return;
            }
            let outcome = catalogue_command_shared(command).await;
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                busy.set(false);
                return;
            }
            busy.set(false);
            match &outcome {
                Ok(IngredientOutcome::Entry(entry)) => {
                    notice.set(None);
                    archive_retrying.set(false);
                    pending_archive.set(None);
                    selected.set(Some(entry.clone()));
                    load_page.run(None);
                }
                Ok(IngredientOutcome::ResultExpired)
                | Ok(IngredientOutcome::RequestIdentityConflict) => {
                    archive_retrying.set(false);
                    pending_archive.set(None);
                    notice.set(terminal_notice(
                        match &outcome { Ok(value) => value, Err(_) => unreachable!() },
                    ));
                    load_page.run(None);
                }
                Ok(IngredientOutcome::ContextChanged | IngredientOutcome::Forbidden) => {
                    pending_archive.set(None);
                    context_visible.set(false)
                }
                Ok(other) => {
                    archive_retrying.set(false);
                    pending_archive.set(None);
                    notice.set(terminal_notice(other));
                }
                Err(_) => {
                    archive_retrying.set(true);
                    notice.set(Some((
                        "Change may not have completed".to_owned(),
                        "The response was lost before the result was confirmed. Retry the previous request; the same request cannot apply twice.".to_owned(),
                        true,
                    )));
                }
            }
        });
    });

    let can_add = Signal::derive(move || {
        let text = amount.get();
        selected
            .get()
            .as_ref()
            .is_some_and(|entry| !entry.archived)
            && amount_kg(&text).is_some()
            && !busy.get()
            && !archive_retrying.get()
    });
    let count_label = Signal::derive(move || {
        let count = items.with(|items| items.len());
        let more = next_cursor.get().is_some();
        let archived_note = if filter.get() == CatalogueFilter::Workspace {
            ", including archived"
        } else {
            ""
        };
        if more {
            format!("{count} shown{archived_note}, more available")
        } else {
            format!("{count} found{archived_note}")
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
    let notice_view = move || {
        notice
            .get()
            .map(|(title, body, warning)| view! { <BreweryNotice title=title body=body warning=warning/> })
    };

    let catalogue_view = move || view! {
        <div class="k-catalogue-toolbar">
            <BreweryDraftField id="catalogue-search" label="Search fermentables" value=Signal::from(query) on_change=search_input input_type="search"/>
            <Button variant=ButtonVariant::Secondary disabled=Signal::derive(move || busy.get() || archive_retrying.get()) on:click=move |_| begin_create.run(())>"Create fermentable"</Button>
        </div>
        <div class="k-catalogue-filters">
            <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm aria_pressed=Signal::derive(move || (filter.get() == CatalogueFilter::All).to_string()) on:click=move |_| filter.set(CatalogueFilter::All)>"All fermentables"</Button>
            <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm aria_pressed=Signal::derive(move || (filter.get() == CatalogueFilter::Workspace).to_string()) on:click=move |_| filter.set(CatalogueFilter::Workspace)>"Your library"</Button>
            <span class="note-caption" role="status">{move || count_label.get()}</span>
        </div>
        {move || notice_view()}
        <Show
            when=move || loading.get() || !items.with(|items| items.is_empty())
            fallback=move || view! {
                <EmptyState
                    variant=crate::components::EmptyStateVariant::Default
                    title="Make this library your own"
                    description="No matching fermentables. Add the malt or grain you use."
                    action=std::sync::Arc::new(move || view! { <Button variant=ButtonVariant::Secondary disabled=Signal::derive(move || busy.get() || archive_retrying.get()) on:click=move |_| begin_create.run(())>"Create fermentable"</Button> }.into_any())
                />
            }
        >
            <div class="k-catalogue-list">
                <For
                    each=move || items.get()
                    // Recreate the row when its revision, metadata or archive
                    // state changes; otherwise the captured entry and copied
                    // details would remain stale after an edit/archive.
                    key=|entry| {
                        (
                            entry.entry_id.clone(),
                            entry.revision,
                            entry.metadata_revision,
                            entry.archived,
                        )
                    }
                    children=move |entry: IngredientEntry| {
                        let entry_id = entry.entry_id.clone();
                        let selected_entry = selected;
                        let source_label = entry_source_label(&entry);
                        view! {
                            <CatalogueItem
                                name=entry.name().to_owned()
                                producer=entry.producer().to_owned()
                                details=entry_details(&entry)
                                source=source_label.to_owned()
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
        <CataloguePickerSelection
            selected=selected
            amount=amount
            archive_retrying=archive_retrying
            busy=busy
            on_edit=Callback::new(move |()| begin_edit.run(()))
            on_archive=Callback::new(move |archived| archive_entry.run(archived))
        />
    };

    let form_view = move || view! {
        {move || notice_view()}
        {move || {
            let disabled = busy.get() || retrying.get();
            view! {
                <div class="k-catalogue-create">
                    <BreweryDraftField id="library-name" label="Fermentable name" value=Signal::from(name) on_change=Callback::new(move |text| name.set(text)) hint="e.g. Local maltster pale malt" disabled=disabled/>
                    <BreweryDraftField id="library-producer" label="Producer" value=Signal::from(producer) on_change=Callback::new(move |text| producer.set(text)) hint="Maltster or supplier (optional)" disabled=disabled/>
                    <BreweryDraftSelect id="library-category" label="Type" value=Signal::from(category) on_change=Callback::new(move |text| category.set(text)) options=CATEGORIES.iter().map(|option| ((*option).to_owned(), (*option).to_owned())).collect() disabled=disabled/>
                    <BreweryDraftField id="library-colour" label="Colour" value=Signal::from(colour) on_change=Callback::new(move |text| colour.set(text)) unit="EBC" numeric=true hint="e.g. 3.5" disabled=disabled/>
                    <BreweryDraftField id="library-yield" label="Potential yield" value=Signal::from(yield_percent) on_change=Callback::new(move |text| yield_percent.set(text)) unit="%" numeric=true hint="Use the extract yield from the supplier's specification." disabled=disabled/>
                </div>
            }
        }}
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
                    <Button variant=ButtonVariant::Default disabled=Signal::derive(move || !create_valid.get()) on:click=move |_| save_entry.run(())>{move || if retrying.get() { "Retry previous request" } else { "Save to library" }}</Button>
                }.into_any())
            >
                {form_view()}
            </BreweryDialog>
        }
        .into_view(),
        PickerView::Edit => view! {
            <BreweryDialog
                id="ingredient-edit"
                title="Edit your library entry"
                description="Update the specification kept in your workspace library. Recipes keep their own copied specifications."
                open=open
                wide=true
                actions=std::sync::Arc::new(move || view! {
                    <Button variant=ButtonVariant::Ghost on:click=move |_| back_to_catalogue.run(())>"Back to catalogue"</Button>
                    <Button variant=ButtonVariant::Default disabled=Signal::derive(move || !create_valid.get()) on:click=move |_| save_entry.run(())>{move || if retrying.get() { "Retry previous request" } else { "Save changes" }}</Button>
                }.into_any())
            >
                {form_view()}
            </BreweryDialog>
        }
        .into_view(),
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use krausen_types::brewery::CatalogueScope;
    use leptos::tachys::view::RenderHtml;

    fn starter_entry() -> IngredientEntry {
        IngredientEntry {
            scope: CatalogueScope::Starter,
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
        }
    }

    fn workspace_entry() -> IngredientEntry {
        let mut entry = starter_entry();
        entry.scope = CatalogueScope::Workspace;
        entry.workspace_id = Some("ws".into());
        entry
    }

    fn render_selection(entry: Option<IngredientEntry>, retrying: bool, busy: bool) -> String {
        let owner = Owner::new();
        owner.with(|| {
            let selected = RwSignal::new(entry);
            let amount = RwSignal::new("4.5".to_owned());
            let archive_retrying = RwSignal::new(retrying);
            let busy = RwSignal::new(busy);
            view! {
                <BreweryDialog
                    id="ingredient-picker-test"
                    title="Build your grain bill"
                    description="Find a fermentable in the starter catalogue, or add one to your own library."
                    open=RwSignal::new(false)
                    wide=true
                >
                    <CataloguePickerSelection
                        selected=selected
                        amount=amount
                        archive_retrying=archive_retrying
                        busy=busy
                        on_edit=Callback::new(move |()| {})
                        on_archive=Callback::new(move |_| {})
                    />
                </BreweryDialog>
            }
            .to_html()
        })
    }

    fn button_open_tag<'a>(html: &'a str, label: &str) -> &'a str {
        let label_at = html.find(label).expect("rendered button label");
        let button_at = html[..label_at]
            .rfind("<button")
            .expect("rendered button element");
        &html[button_at..label_at]
    }

    fn button_is_disabled(html: &str, label: &str) -> bool {
        // The shared Button class list contains Tailwind `disabled:` variants,
        // so look for the boolean attribute token, not the substring.
        button_open_tag(html, label).split_whitespace().any(|part| {
            part == "disabled" || part == "disabled=\"\"" || part == "disabled=\"true\""
        })
    }

    #[test]
    fn picker_details_format_specs_and_selection_copy() {
        let owner = Owner::new();
        owner.with(|| {
            let entry = starter_entry();
            assert_eq!(entry_details(&entry), "7 EBC · 77.9% yield");
            let source = entry.ingredient_source();
            assert_eq!(source.scope, "starter");
            assert_eq!(source.version, Some(2));
            assert_eq!(amount_kg("0.25"), Some(0.25));
            assert_eq!(amount_kg("-1"), None);
            assert_eq!(amount_kg(""), None);
            assert_eq!(entry_source_label(&entry), "Starter catalogue");
        });
    }

    #[test]
    fn archived_badge_and_terminal_notices_are_distinct() {
        let mut archived = starter_entry();
        archived.scope = CatalogueScope::Workspace;
        archived.workspace_id = Some("ws".into());
        archived.archived = true;
        assert_eq!(entry_source_label(&archived), "Archived");

        let expired = terminal_notice(&IngredientOutcome::ResultExpired)
            .expect("expired must be reported");
        assert!(expired.1.contains("confirmation expired"));
        let identity = terminal_notice(&IngredientOutcome::RequestIdentityConflict)
            .expect("identity conflict must be reported");
        assert!(identity.1.contains("different content"));
        let conflict = terminal_notice(&IngredientOutcome::Conflict {
            expected: 1,
            actual: 3,
        })
        .expect("conflict must be reported");
        assert!(conflict.1.contains("revision 3"));
        let duplicate = terminal_notice(&IngredientOutcome::Duplicate).expect("duplicate");
        assert!(duplicate.1.contains("already in your library"));
        assert_ne!(expired.1, identity.1);
        assert_ne!(conflict.1, expired.1);
        // Transport/unavailable keeps a retryable message distinct from the
        // terminal identity classes above.
        let unavailable = terminal_notice(&IngredientOutcome::Unavailable).expect("unavailable");
        assert_ne!(unavailable.1, expired.1);
    }

    #[test]
    fn composed_picker_renders_real_management_actions_by_scope_and_archive_state() {
        let workspace = render_selection(Some(workspace_entry()), false, false);
        assert!(workspace.contains("Build your grain bill"));
        assert!(workspace.contains("Edit library entry"));
        assert!(workspace.contains("Archive"));
        assert!(!workspace.contains("Restore"));
        assert!(!button_is_disabled(&workspace, "Edit library entry"));

        // The public starter catalogue is read-only: no library management.
        let starter = render_selection(Some(starter_entry()), false, false);
        assert!(!starter.contains("Edit library entry"));
        assert!(!starter.contains("Archive"));
        assert!(!starter.contains("Restore"));

        // Archived workspace entries can be restored, but not edited or
        // archived again until the restore commits.
        let mut archived = workspace_entry();
        archived.archived = true;
        let archived_html = render_selection(Some(archived), false, false);
        assert!(archived_html.contains("Edit library entry"));
        assert!(button_is_disabled(&archived_html, "Edit library entry"));
        assert!(archived_html.contains("Restore"));
        assert!(!archived_html.contains("Archive"));
        assert!(!button_is_disabled(&archived_html, "Restore"));

        // An uncertain archive outcome retains an enabled retry action; only
        // an in-flight request disables it.
        let retrying = render_selection(Some(workspace_entry()), true, false);
        assert!(retrying.contains("Retry previous request"));
        assert!(!button_is_disabled(&retrying, "Retry previous request"));

        let busy = render_selection(Some(workspace_entry()), false, true);
        assert!(busy.contains("Archive"));
        assert!(button_is_disabled(&busy, "Archive"));
    }
}
