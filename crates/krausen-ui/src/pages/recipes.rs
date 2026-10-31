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
        RecipeSummary, RecoveryQuery,
    },
    request_context::OperationIntent,
};
use leptos::prelude::*;

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
    let history_generation = RwSignal::new(0_u64);
    let pending = RwSignal::new(None::<ArchiveRecipe>);
    let blocked = RwSignal::new(false);
    let saving = RwSignal::new(false);
    let context_valid = RwSignal::new(true);
    #[cfg(target_arch = "wasm32")]
    let context_generation = RwSignal::new(0_u64);
    #[cfg(target_arch = "wasm32")]
    {
        let recheck = Callback::new(move |_| {
            context_valid.set(false);
            generation.update(|n| *n += 1);
            history_generation.update(|n| *n += 1);
            context_generation.update(|n| *n += 1);
            let sequence = context_generation.get_untracked();
            results.set(Vec::new());
            history.set(None);
            recovery.set(None);
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
                    results.set(Vec::new());
                    next.set(None);
                    status.set("Account or workspace changed. Reopen the library in the original workspace.".into());
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
        history.set(None);
        load.run(None);
    });
    let show_history = Callback::new(
        move |(id, name, cursor): (String, String, Option<String>)| {
            let append = cursor.is_some();
            if !append {
                history_generation.update(|n| *n += 1);
                history.set(None);
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
                        history.set(None);
                        results.set(Vec::new());
                        status.set("Account or workspace changed. Reopen the library.".into());
                    }
                    _ => status.set("Unable to load revision history.".into()),
                }
            });
        },
    );
    let recover = Callback::new(move |recipe_id: String| {
        recovery.set(None);
        let query = RecoveryQuery {
            intent: intent.get_value(),
            recipe_id,
        };
        leptos::task::spawn_local(async move {
            match library_command_shared(LibraryCommand::Recover(query)).await {
                Ok(LibraryOutcome::LegacySource { source, diagnostic }) => {
                    recovery.set(Some((source, diagnostic)))
                }
                Ok(LibraryOutcome::Forbidden | LibraryOutcome::ContextChanged) => {
                    results.set(Vec::new());
                    context_valid.set(false);
                }
                _ => status.set("Unable to recover the stored recipe source.".into()),
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
            saving.set(false);
            match result {
                Ok(LibraryOutcome::Archive { .. }) => {
                    pending.set(None);
                    history.set(None);
                    load.run(None);
                }
                Ok(LibraryOutcome::Unavailable) | Err(_) => {
                    status.set("The result is uncertain. Retry the same request.".into())
                }
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
                    results.set(Vec::new());
                    status.set("Account or workspace changed. Reopen the library.".into());
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
                    {recovery_item.then(|| view! { <Button variant=ButtonVariant::Secondary on:click=move |_| recover.run(recovery_id.clone())>"Recover stored source"</Button> })}
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
        {move || recovery.get().map(|(source, diagnostic)| { let href = format!("data:application/json;base64,{}", STANDARD.encode(source.as_bytes())); view! { <BreweryPanel title="Stored recipe source"><p>{diagnostic}</p><a href=href download="legacy-recipe.json">"Download stored source"</a><pre>{source}</pre></BreweryPanel> } })}
        {move || history.get().map(|(id, page)| { let title = format!("History · {}", history_name.get()); let more_id = id.clone(); let more_name = history_name.get(); let more_cursor = page.next_cursor.clone(); view! { <BreweryPanel title=title><ul>{page.items.into_iter().map(|entry| view! { <li>{format!("Revision {}", entry.revision)}{entry.saved_at.map(|date| format!(" · {date}")).unwrap_or_default()}{entry.restored_from_revision.map(|r| format!(" · restored from {r}")).unwrap_or_default()}</li> }).collect_view()}</ul>{more_cursor.map(|cursor| view! { <Button variant=ButtonVariant::Secondary on:click=move |_| show_history.run((more_id.clone(), more_name.clone(), Some(cursor.clone())))>"Load more revisions"</Button> })}</BreweryPanel> } })}
    </div> }
}
