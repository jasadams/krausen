// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recipe library over the same authorized operations as MCP.
use crate::{
    components::{
        BreweryDraftField, BreweryDraftSelect, BreweryNotice, BreweryPanel, Button, ButtonLink,
        ButtonVariant,
    },
    server_fns::recipe_library::library_command_shared,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use krausen_types::{
    recipe_library::{
        ArchiveRecipe, HistoryPage, HistoryQuery, LibraryCommand, LibraryOutcome, LibraryQuery,
        RecipeSummary, RecoveryQuery, RevisionQuery,
    },
    request_context::OperationIntent,
};
use leptos::prelude::*;

fn authority_failed(result: &Result<LibraryOutcome, ServerFnError>) -> bool {
    match result {
        Ok(LibraryOutcome::Forbidden | LibraryOutcome::ContextChanged) => true,
        Err(error) => crate::utils::auth_refresh::is_auth_error(&error.to_string()),
        _ => false,
    }
}

#[component]
pub fn RecipesPage() -> impl IntoView {
    let context =
        LocalResource::new(|| async { crate::server_fns::context::get_user_context().await });
    view! { <Suspense fallback=|| view! { <BreweryNotice title="Loading recipes" body="Please wait."/> }>
        {move || Suspend::new(async move { match context.await {
            Ok(ctx) => match ctx.workspace_id {
                Some(workspace_id) => view! { <RecipeLibrary intent=OperationIntent { account_id: ctx.user_id, workspace_id }/> }.into_any(),
                None => view! { <BreweryNotice title="Workspace required" body="Choose a workspace to see its recipes." warning=true/> }.into_any(),
            },
            Err(_) => view! { <BreweryNotice title="Unable to load recipes" body="Sign in and try again." warning=true/> }.into_any(),
        } })}
    </Suspense> }
}
#[component]
fn RecipeLibrary(intent: OperationIntent) -> impl IntoView {
    let intent = StoredValue::new(intent);
    let search = RwSignal::new(String::new());
    let archived = RwSignal::new(false);
    let results = RwSignal::new(Vec::<RecipeSummary>::new());
    let next = RwSignal::new(None::<String>);
    let generation = RwSignal::new(0_u64);
    let loading = RwSignal::new(false);
    let status = RwSignal::new(String::new());
    let history = RwSignal::new(None::<(String, HistoryPage)>);
    let history_name = RwSignal::new(String::new());
    let recovery = RwSignal::new(None::<(String, String)>);
    let selected_revision = RwSignal::new(None::<(i64, krausen_types::brewery::RecipeDocumentV1)>);
    let detail_generation = RwSignal::new(0_u64);
    let detail_loading = RwSignal::new(false);
    let history_generation = RwSignal::new(0_u64);
    let pending = RwSignal::new(None::<ArchiveRecipe>);
    let blocked = RwSignal::new(false);
    let saving = RwSignal::new(false);
    let context_valid = RwSignal::new(true);
    let context_generation = RwSignal::new(0_u64);
    // Every authority failure and context recheck removes retained data and
    // invalidates all in-flight responses before any new context is accepted.
    let invalidate_context = Callback::new(move |_| {
        context_valid.set(false);
        context_generation.update(|n| *n += 1);
        generation.update(|n| *n += 1);
        history_generation.update(|n| *n += 1);
        detail_generation.update(|n| *n += 1);
        results.set(Vec::new());
        next.set(None);
        history.set(None);
        recovery.set(None);
        selected_revision.set(None);
        loading.set(false);
        detail_loading.set(false);
        status.set(
            "Account or workspace changed. Reopen the library in the original workspace.".into(),
        );
    });
    #[cfg(target_arch = "wasm32")]
    {
        let recheck = Callback::new(move |_| {
            invalidate_context.run(());
            let sequence = context_generation.get_untracked();
            let expected = intent.get_value();
            leptos::task::spawn_local(async move {
                let valid = crate::server_fns::context::get_user_context()
                    .await
                    .is_ok_and(|current| {
                        current.user_id == expected.account_id
                            && current.workspace_id.as_deref()
                                == Some(expected.workspace_id.as_str())
                    });
                if context_generation.try_get_untracked() == Some(sequence) {
                    context_valid.set(valid);
                }
            });
        });
        let focus = window_event_listener(leptos::ev::focus, move |_| recheck.run(()));
        let storage = window_event_listener(leptos::ev::storage, move |event| {
            if event.key().as_deref() == Some(crate::components::layout::CONTEXT_CHANGED_KEY) {
                recheck.run(());
            }
        });
        on_cleanup(move || {
            focus.remove();
            storage.remove();
        });
    }

    let load = Callback::new(move |cursor: Option<String>| {
        if cursor.is_none() {
            generation.update(|v| *v += 1);
        }
        let sequence = generation.get_untracked();
        let append = cursor.is_some();
        let query = LibraryQuery {
            intent: intent.get_value(),
            search: search.get_untracked(),
            archived: archived.get_untracked(),
            cursor,
            limit: 50,
        };
        loading.set(true);
        leptos::task::spawn_local(async move {
            let response = library_command_shared(LibraryCommand::List(query)).await;
            if context_valid.try_get_untracked().is_none() {
                return;
            }
            if authority_failed(&response) {
                invalidate_context.run(());
                return;
            }
            if generation.try_get_untracked() != Some(sequence) {
                return;
            }
            loading.set(false);
            match response {
                Ok(LibraryOutcome::Recipes(page)) => {
                    if append {
                        results.update(|items| items.extend(page.items));
                    } else {
                        results.set(page.items);
                    }
                    next.set(page.next_cursor);
                    status.set(String::new());
                }
                Ok(LibraryOutcome::ContextChanged | LibraryOutcome::Forbidden) => {
                    invalidate_context.run(())
                }
                Err(error) if crate::utils::auth_refresh::is_auth_error(&error.to_string()) => {
                    invalidate_context.run(())
                }
                _ => status.set("Unable to load recipes. Try again.".into()),
            }
        });
    });
    Effect::new(move |_| {
        let _ = search.get();
        let _ = archived.get();
        if !context_valid.get() {
            return;
        }
        history_generation.update(|n| *n += 1);
        detail_generation.update(|n| *n += 1);
        history.set(None);
        selected_revision.set(None);
        recovery.set(None);
        detail_loading.set(false);
        load.run(None);
    });
    let show_history = Callback::new(
        move |(id, name, cursor): (String, String, Option<String>)| {
            let append = cursor.is_some();
            if !append {
                history_generation.update(|n| *n += 1);
                detail_generation.update(|n| *n += 1);
                history.set(None);
                selected_revision.set(None);
                recovery.set(None);
                detail_loading.set(false);
            }
            let sequence = history_generation.get_untracked();
            let query = HistoryQuery {
                intent: intent.get_value(),
                recipe_id: id.clone(),
                cursor,
                limit: 50,
            };
            history_name.set(name);
            leptos::task::spawn_local(async move {
                let response = library_command_shared(LibraryCommand::History(query)).await;
                if context_valid.try_get_untracked().is_none() {
                    return;
                }
                if authority_failed(&response) {
                    invalidate_context.run(());
                    return;
                }
                if history_generation.try_get_untracked() != Some(sequence) {
                    return;
                }
                match response {
                    Ok(LibraryOutcome::History(page)) => {
                        if append {
                            history.update(|current| {
                                if let Some((current_id, old)) = current {
                                    if *current_id == id {
                                        old.items.extend(page.items);
                                        old.next_cursor = page.next_cursor;
                                    }
                                }
                            });
                        } else {
                            history.set(Some((id, page)));
                        }
                    }
                    Ok(LibraryOutcome::Forbidden | LibraryOutcome::ContextChanged) => {
                        invalidate_context.run(())
                    }
                    Err(error) if crate::utils::auth_refresh::is_auth_error(&error.to_string()) => {
                        invalidate_context.run(())
                    }
                    _ => status.set("Unable to load revision history.".into()),
                }
            });
        },
    );
    let recover = Callback::new(move |recipe_id: String| {
        detail_generation.update(|n| *n += 1);
        let sequence = detail_generation.get_untracked();
        history_generation.update(|n| *n += 1);
        history.set(None);
        selected_revision.set(None);
        recovery.set(None);
        detail_loading.set(true);
        let query = RecoveryQuery {
            intent: intent.get_value(),
            recipe_id,
        };
        leptos::task::spawn_local(async move {
            let response = library_command_shared(LibraryCommand::Recover(query)).await;
            if context_valid.try_get_untracked().is_none() {
                return;
            }
            if authority_failed(&response) {
                invalidate_context.run(());
                return;
            }
            if detail_generation.try_get_untracked() != Some(sequence)
                || context_valid.try_get_untracked() != Some(true)
            {
                return;
            }
            detail_loading.set(false);
            match response {
                Ok(LibraryOutcome::LegacySource { source, diagnostic }) => {
                    recovery.set(Some((source, diagnostic)))
                }
                Ok(LibraryOutcome::Forbidden | LibraryOutcome::ContextChanged) => {
                    invalidate_context.run(())
                }
                Err(error) if crate::utils::auth_refresh::is_auth_error(&error.to_string()) => {
                    invalidate_context.run(())
                }
                _ => status.set("Unable to recover the stored recipe source.".into()),
            }
        });
    });
    let inspect_revision = Callback::new(move |(recipe_id, revision): (String, i64)| {
        detail_generation.update(|n| *n += 1);
        let sequence = detail_generation.get_untracked();
        selected_revision.set(None);
        recovery.set(None);
        detail_loading.set(true);
        let query = RevisionQuery {
            intent: intent.get_value(),
            recipe_id: recipe_id.clone(),
            revision,
        };
        leptos::task::spawn_local(async move {
            let response = library_command_shared(LibraryCommand::Revision(query)).await;
            if context_valid.try_get_untracked().is_none() {
                return;
            }
            if authority_failed(&response) {
                invalidate_context.run(());
                return;
            }
            if detail_generation.try_get_untracked() != Some(sequence)
                || context_valid.try_get_untracked() != Some(true)
                || history
                    .try_with_untracked(|h| h.as_ref().is_some_and(|(id, _)| id == &recipe_id))
                    != Some(true)
            {
                return;
            }
            detail_loading.set(false);
            match response {
                Ok(LibraryOutcome::Revision(document)) => {
                    selected_revision.set(Some((revision, document)))
                }
                Ok(LibraryOutcome::Forbidden | LibraryOutcome::ContextChanged) => {
                    invalidate_context.run(())
                }
                Err(error) if crate::utils::auth_refresh::is_auth_error(&error.to_string()) => {
                    invalidate_context.run(())
                }
                _ => status.set("Unable to load that saved revision.".into()),
            }
        });
    });
    let execute_archive = Callback::new(move |command: ArchiveRecipe| {
        if saving.get_untracked() || blocked.get_untracked() {
            return;
        }
        saving.set(true);
        pending.set(Some(command.clone()));
        leptos::task::spawn_local(async move {
            let result = library_command_shared(LibraryCommand::Archive(command)).await;
            if context_valid.try_get_untracked().is_none() {
                return;
            }
            saving.set(false);
            match result {
                Ok(LibraryOutcome::Archive { .. }) => {
                    pending.set(None);
                    history_generation.update(|n| *n += 1);
                    detail_generation.update(|n| *n += 1);
                    history.set(None);
                    selected_revision.set(None);
                    recovery.set(None);
                    detail_loading.set(false);
                    load.run(None);
                }
                Ok(LibraryOutcome::Unavailable) => {
                    status.set("The result is uncertain. Retry the same request.".into())
                }
                Err(error) if crate::utils::auth_refresh::is_auth_error(&error.to_string()) => {
                    invalidate_context.run(())
                }
                Err(_) => status.set("The result is uncertain. Retry the same request.".into()),
                Ok(LibraryOutcome::ResultExpired | LibraryOutcome::RequestIdentityConflict) => {
                    blocked.set(true);
                    status.set("The previous request cannot be confirmed. Review the library before continuing.".into());
                }
                Ok(LibraryOutcome::Conflict { .. }) => {
                    pending.set(None);
                    status.set("Recipe changed elsewhere. Refresh before trying again.".into());
                    load.run(None);
                }
                Ok(LibraryOutcome::Forbidden | LibraryOutcome::ContextChanged) => {
                    invalidate_context.run(())
                }
                _ => {
                    pending.set(None);
                    status.set("Unable to update the recipe.".into());
                }
            }
        });
    });
    let search_change = Callback::new(move |value| search.set(value));
    let archive_filter = Signal::derive(move || {
        if archived.get() {
            "archived".to_owned()
        } else {
            "active".to_owned()
        }
    });
    let filter_change = Callback::new(move |value: String| archived.set(value == "archived"));
    view! { <Show when=move || !context_valid.get()><BreweryNotice title="Workspace changed" body="Return to the original workspace to use this library." warning=true/></Show><div class="recipe-editor" hidden=move || !context_valid.get()>
        <header class="recipe-editor__header"><div><p class="kit-showcase__eyebrow">"Your workspace"</p><h1>"Recipes"</h1><p>"Find a recipe and continue brewing."</p></div><ButtonLink href="/recipe-editor/new">"New recipe"</ButtonLink></header>
        <BreweryPanel title="Recipe library" description="Search saved recipes or show archived records.">
            <div class="recipe-editor__fields"><BreweryDraftField id="library-search" label="Search recipes" value=Signal::derive(move || search.get()) on_change=search_change/><BreweryDraftSelect id="library-filter" label="Show" value=archive_filter on_change=filter_change options=vec![("active".into(), "Active recipes".into()), ("archived".into(), "Archived recipes".into())]/></div>
            <Show when=move || loading.get()><BreweryNotice title="Loading recipes" body="Please wait."/></Show>
            {move || (!status.get().is_empty()).then(|| view! { <p role="status">{status.get()}</p> })}
            <Show when=move || !loading.get() && results.with(|items| items.is_empty()) && status.with(|s| s.is_empty())><BreweryNotice title="No recipes found" body="Change the search or create a recipe."/></Show>
            <ul>{move || results.get().into_iter().map(|item| {
                let id = item.recipe_id.clone(); let title = item.title.clone(); let archived_item = item.archived; let recovery_item = item.needs_recovery; let revision = item.metadata_revision;
                let history_id = id.clone(); let history_title = title.clone(); let archive_id = id.clone(); let recovery_id = id.clone();
                view! { <li><BreweryPanel title=title.clone()>
                    <p>{item.style.unwrap_or_else(|| "No style".into())}{item.batch_size_l.map(|v| format!(" · {v} L")).unwrap_or_default()}{format!(" · revision {}", item.revision)}</p>
                    {(!archived_item && !recovery_item).then(|| view! { <ButtonLink href=format!("/recipe-editor/{id}") variant=ButtonVariant::Secondary aria_label=format!("Open {title}")>"Open recipe"</ButtonLink> })}
                    {recovery_item.then(|| view! { <Button variant=ButtonVariant::Secondary aria_label=format!("Recover stored source for {title}") on:click=move |_| recover.run(recovery_id.clone())>"Recover stored source"</Button> })}
                    {(!recovery_item).then(|| view! { <Button variant=ButtonVariant::Ghost aria_label=format!("History for {title}") on:click=move |_| show_history.run((history_id.clone(), history_title.clone(), None))>"History"</Button> })}
                    {(!recovery_item).then(|| view! { <Button variant=ButtonVariant::Secondary aria_label=format!("{} {title}", if archived_item { "Restore" } else { "Archive" }) disabled=Signal::derive(move || saving.get() || pending.get().is_some() || blocked.get()) on:click=move |_| {
                        let Some(request_id) = crate::utils::request_id::new_request_id() else { status.set("Unable to create request identity.".into()); return; };
                        execute_archive.run(ArchiveRecipe { intent: intent.get_value(), recipe_id: archive_id.clone(), expected_metadata_revision: revision, archived: !archived_item, request_id });
                    }>{if archived_item { "Restore" } else { "Archive" }}</Button> })}
                </BreweryPanel></li> }
            }).collect_view() }</ul>
            <Show when=move || next.get().is_some()><Button variant=ButtonVariant::Secondary disabled=loading on:click=move |_| load.run(next.get_untracked())>"Load more recipes"</Button></Show>
            <Show when=move || pending.get().is_some() && !blocked.get()><Button variant=ButtonVariant::Secondary disabled=saving on:click=move |_| { if let Some(command) = pending.get_untracked() { execute_archive.run(command); } }>"Retry previous request"</Button></Show>
        </BreweryPanel>
        {move || recovery.get().map(|(source, diagnostic)| { let href = format!("data:application/json;base64,{}", STANDARD.encode(source.as_bytes())); view! { <BreweryPanel title="Stored recipe source"><p>{diagnostic}</p><a href=href download="legacy-recipe.json">"Download stored source"</a><pre class="overflow-x-auto">{source}</pre></BreweryPanel> } })}
        {move || history.get().map(|(id, page)| { let title = format!("History · {}", history_name.get()); let more_id = id.clone(); let more_name = history_name.get(); let more_cursor = page.next_cursor.clone(); view! { <BreweryPanel title=title><ul>{page.items.into_iter().map(|entry| { let recipe_id = id.clone(); let revision = entry.revision; view! { <li>{format!("Revision {}", revision)}{entry.saved_at.map(|date| format!(" · {date}")).unwrap_or_default()}{entry.restored_from_revision.map(|r| format!(" · restored from {r}")).unwrap_or_default()}<Button variant=ButtonVariant::Ghost aria_label=format!("Inspect revision {revision}") on:click=move |_| inspect_revision.run((recipe_id.clone(), revision))>"Inspect"</Button></li> } }).collect_view()}</ul>{more_cursor.map(|cursor| view! { <Button variant=ButtonVariant::Secondary on:click=move |_| show_history.run((more_id.clone(), more_name.clone(), Some(cursor.clone())))>"Load more revisions"</Button> })}</BreweryPanel> } })}
        <Show when=move || detail_loading.get()><BreweryNotice title="Loading saved content" body="Please wait."/></Show>
        {move || selected_revision.get().map(|(revision, document)| view! { <HistoricalRecipe revision=revision document=document/> })}
    </div> }
}

/// Read-only presentation of retained source data; no targets are recalculated.
#[component]
fn HistoricalRecipe(
    revision: i64,
    document: krausen_types::brewery::RecipeDocumentV1,
) -> impl IntoView {
    let title = format!("Historical revision {revision} · {}", document.title);
    let style = document
        .style
        .as_ref()
        .map(|style| style.name.clone())
        .unwrap_or_else(|| "Not specified".into());
    let batch = document
        .batch_size_l
        .map(|value| format!("{value} L"))
        .unwrap_or_else(|| "Not specified".into());
    let boil = document
        .boil_minutes
        .map(|value| format!("{value} min"))
        .unwrap_or_else(|| "Not specified".into());
    let fermentables = document
        .fermentables
        .iter()
        .map(|row| {
            format!(
                "{} · {} · {}",
                row.specification
                    .name
                    .as_deref()
                    .unwrap_or("Unnamed fermentable"),
                row.amount_kg
                    .map(|value| format!("{value} kg"))
                    .unwrap_or_else(|| "quantity not specified".into()),
                row.addition.stage
            )
        })
        .collect::<Vec<_>>();
    let hops = document
        .hops
        .iter()
        .map(|row| {
            format!(
                "{} · {} · {}",
                row.specification.name.as_deref().unwrap_or("Unnamed hop"),
                row.amount_g
                    .map(|value| format!("{value} g"))
                    .unwrap_or_else(|| "quantity not specified".into()),
                row.stage
            )
        })
        .collect::<Vec<_>>();
    let yeast = document
        .yeast
        .iter()
        .map(|row| {
            format!(
                "{} · {}",
                row.specification.name.as_deref().unwrap_or("Unnamed yeast"),
                ingredient_amount(row.amount, row.unit.as_deref())
            )
        })
        .collect::<Vec<_>>();
    let other = document
        .other_ingredients
        .iter()
        .map(|row| {
            format!(
                "{} · {}",
                row.specification
                    .name
                    .as_deref()
                    .unwrap_or("Unnamed ingredient"),
                ingredient_amount(row.amount, row.unit.as_deref())
            )
        })
        .collect::<Vec<_>>();
    let mash = document
        .process
        .mash_steps
        .iter()
        .map(step_text)
        .collect::<Vec<_>>();
    let fermentation = document
        .process
        .fermentation_steps
        .iter()
        .map(step_text)
        .collect::<Vec<_>>();
    let saved_data = match serde_json::to_string_pretty(&document) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(error = %error, "Unable to display saved revision data");
            "Saved data could not be displayed.".into()
        }
    };
    view! { <BreweryPanel title=title description="Read-only saved content. The current recipe has not been changed.">
        <p>{format!("Style: {style} · Batch: {batch} · Boil: {boil}")}</p>
        {document.description.map(|text| view! { <p>{text}</p> })}
        <h3>"Fermentables"</h3>{saved_rows(fermentables)}
        <h3>"Hops"</h3>{saved_rows(hops)}
        <h3>"Yeast"</h3>{saved_rows(yeast)}
        <h3>"Other ingredients"</h3>{saved_rows(other)}
        <h3>"Mash steps"</h3>{saved_rows(mash)}
        <h3>"Fermentation steps"</h3>{saved_rows(fermentation)}
        {document.notes.map(|text| view! { <div><h3>"Notes"</h3><p>{text}</p></div> })}
        <details><summary>"Complete saved data"</summary><pre class="overflow-x-auto">{saved_data}</pre></details>
    </BreweryPanel> }
}
fn ingredient_amount(amount: Option<f64>, unit: Option<&str>) -> String {
    match amount {
        Some(value) => match unit {
            Some(unit) if !unit.is_empty() => format!("{value} {unit}"),
            _ => value.to_string(),
        },
        None => "quantity not specified".into(),
    }
}
fn step_text(step: &krausen_types::brewery::ProcessStep) -> String {
    let mut parts = vec![step.name.as_deref().unwrap_or("Unnamed step").to_owned()];
    if let Some(value) = step.temperature_c {
        parts.push(format!("{value} °C"));
    }
    if let Some(value) = step.time_minutes {
        parts.push(format!("{value} min"));
    }
    if let Some(value) = step.volume_l {
        parts.push(format!("{value} L"));
    }
    parts.join(" · ")
}
fn saved_rows(items: Vec<String>) -> impl IntoView {
    if items.is_empty() {
        view! { <p>"None saved"</p> }.into_any()
    } else {
        view! { <ul>{items.into_iter().map(|item| view! { <li>{item}</li> }).collect_view()}</ul> }
            .into_any()
    }
}
