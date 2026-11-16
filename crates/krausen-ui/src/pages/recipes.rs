// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recipe library over the same authorized operations as MCP.
use crate::{
    components::{
        BreweryDraftField, BreweryDraftSelect, BreweryNotice, BreweryPanel, Button, ButtonLink,
        ButtonVariant, RecipeRevisionHistory,
    },
    server_fns::recipe_library::library_command_shared,
    utils::{
        list_position::{self, ListPosition},
        nav_state::{current_location, library_filter, library_href, parse_revision},
    },
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use krausen_types::{
    recipe_library::{
        ArchiveRecipe, LibraryCommand, LibraryOutcome, LibraryQuery, RecipeSummary, RecoveryQuery,
    },
    request_context::OperationIntent,
};
use leptos::prelude::*;
use leptos_router::hooks::{use_location, use_navigate, use_query_map};

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
    // Search, archive filter and the inline saved-version selection live in the
    // URL (see utils::nav_state), so a return, reload or Back restores them.
    let query = use_query_map();
    let navigate = use_navigate();
    let location = use_location();
    let search = RwSignal::new(query.with_untracked(|q| q.get("q")).unwrap_or_default());
    let archived = RwSignal::new(query.with_untracked(|q| q.get("show")).as_deref() == Some("archived"));
    let selected_revision = RwSignal::new(query.with_untracked(|q| parse_revision(q.get("revision").as_deref())));
    // Pages loaded so far and a saved position still to restore on return.
    let pages_loaded = RwSignal::new(0_u32);
    let restore = RwSignal::new(None::<ListPosition>);
    let position_key = StoredValue::new(list_position::storage_key(&intent.get_value().account_id, &intent.get_value().workspace_id));
    #[cfg(target_arch = "wasm32")]
    if let Some(saved) = list_position::take(&position_key.get_value()) {
        let here = current_location(&location.pathname.get_untracked(), &location.search.get_untracked());
        let plain = search.get_untracked().is_empty() && !archived.get_untracked() && query.with_untracked(|q| q.get("versions").is_none());
        if saved.location == here {
            restore.set(Some(saved));
        } else if plain {
            // A plain return to Recipes resumes the search/filter being left.
            if let Some((saved_search, saved_archived)) = library_filter(&saved.location) {
                search.set(saved_search);
                archived.set(saved_archived);
                restore.set(Some(saved));
            }
        }
    }
    let results = RwSignal::new(Vec::<RecipeSummary>::new());
    let next = RwSignal::new(None::<String>);
    let generation = RwSignal::new(0_u64);
    let loading = RwSignal::new(false);
    let status = RwSignal::new(String::new());
    // Selection owns the shared history component's lifetime. Clearing it
    // disposes list/detail requests as well as any retained historical content.
    let history = RwSignal::new(
        query
            .with_untracked(|q| q.get("versions"))
            .filter(|id| !id.is_empty())
            .map(|id| (id, "Selected recipe".to_owned())),
    );
    let recovery = RwSignal::new(None::<(String, String)>);
    let detail_generation = RwSignal::new(0_u64);
    let detail_loading = RwSignal::new(false);
    let pending = RwSignal::new(None::<ArchiveRecipe>);
    let blocked = RwSignal::new(false);
    let saving = RwSignal::new(false);
    let context_valid = RwSignal::new(true);
    let context_generation = RwSignal::new(0_u64);
    // Every authority failure and context recheck removes retained data and
    // invalidates all in-flight responses before any new context is accepted.
    let invalidate_context = Callback::new(move |_| {
        // A retired history selection can still report an authority failure
        // while this library is live, but must not access a disposed parent.
        if context_valid.try_get_untracked().is_none() {
            return;
        }
        context_valid.set(false);
        context_generation.update(|n| *n += 1);
        generation.update(|n| *n += 1);
        detail_generation.update(|n| *n += 1);
        results.set(Vec::new());
        next.set(None);
        history.set(None);
        recovery.set(None);
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
                        pages_loaded.update(|n| *n += 1);
                    } else {
                        results.set(page.items);
                        pages_loaded.set(1);
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
    Effect::new(move |first: Option<()>| {
        let _ = search.get();
        let _ = archived.get();
        if !context_valid.get() {
            return;
        }
        detail_generation.update(|n| *n += 1);
        if first.is_some() {
            history.set(None);
            selected_revision.set(None);
        }
        recovery.set(None);
        detail_loading.set(false);
        load.run(None);
    });
    // The heading of a URL-opened selection takes the authorized list title.
    Effect::new(move |_| {
        let Some((id, title)) = history.get() else { return };
        if let Some(found) = results.with(|items| items.iter().find(|item| item.recipe_id == id).map(|item| item.title.clone())) {
            if found != title {
                history.set(Some((id, found)));
            }
        }
    });
    // URL -> state (Back/Forward, bookmarks); state -> URL (replace, never push).
    Effect::new(move |_| {
        let (q, show, versions, revision) = query.with(|q| {
            (
                q.get("q").unwrap_or_default(),
                q.get("show").as_deref() == Some("archived"),
                q.get("versions").filter(|id| !id.is_empty()),
                parse_revision(q.get("revision").as_deref()),
            )
        });
        if search.get_untracked() != q { search.set(q); }
        if archived.get_untracked() != show { archived.set(show); }
        let revision = versions.as_ref().and(revision);
        if selected_revision.get_untracked() != revision { selected_revision.set(revision); }
        let current_history = history.get_untracked().map(|(id, _)| id);
        if current_history != versions {
            history.set(versions.map(|id| (id, "Selected recipe".to_owned())));
        }
    });
    Effect::new(move |_| {
        let target = library_href(
            &search.get(),
            archived.get(),
            history.with(|h| h.as_ref().map(|(id, _)| id.clone())).as_deref().map(|id| (id, selected_revision.get())),
        );
        if location.pathname.get_untracked() != "/recipes" {
            return;
        }
        let here = current_location(&location.pathname.get_untracked(), &location.search.get_untracked());
        if here != target {
            navigate(&target, leptos_router::NavigateOptions { replace: true, ..Default::default() });
        }
    });
    // Replays the pages and scroll left behind by the last visit, then stops.
    Effect::new(move |_| {
        let Some(position) = restore.get() else { return };
        if loading.get() || !context_valid.get() {
            return;
        }
        let loaded = pages_loaded.get();
        if loaded == 0 {
            if !status.with(|s| s.is_empty()) {
                restore.set(None);
            }
            return;
        }
        if loaded < position.pages && next.with(|cursor| cursor.is_some()) {
            load.run(next.get_untracked());
            return;
        }
        restore.set(None);
        #[cfg(target_arch = "wasm32")]
        request_animation_frame(move || {
            use wasm_bindgen::JsCast;
            if let Some(main) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.query_selector(".app-shell__main").ok().flatten())
                .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
            {
                main.set_scroll_top(position.scroll_top);
            }
        });
    });
    // Following a recipe link leaves a position to restore on the way back.
    let remember_position = move |event: leptos::ev::MouseEvent| {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            let on_link = event
                .target()
                .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
                .is_some_and(|el| el.closest("a[href]").ok().flatten().is_some());
            if on_link {
                let scroll_top = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.query_selector(".app-shell__main").ok().flatten())
                    .map(|main| main.scroll_top())
                    .unwrap_or_default();
                list_position::save(
                    &position_key.get_value(),
                    &ListPosition {
                        pages: pages_loaded.get_untracked().clamp(1, list_position::MAX_RESTORE_PAGES),
                        scroll_top: scroll_top.max(0),
                        location: library_href(&search.get_untracked(), archived.get_untracked(), None),
                    },
                );
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = event;
    };
    let show_history = Callback::new(move |(id, name): (String, String)| {
        selected_revision.set(None);
        detail_generation.update(|n| *n += 1);
        recovery.set(None);
        detail_loading.set(false);
        history.set(Some((id, name)));
    });
    let recover = Callback::new(move |recipe_id: String| {
        detail_generation.update(|n| *n += 1);
        let sequence = detail_generation.get_untracked();
        history.set(None);
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
                    detail_generation.update(|n| *n += 1);
                    history.set(None);
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
    view! { <Show when=move || !context_valid.get()><BreweryNotice title="Workspace changed" body="Return to the original workspace to use this library." warning=true/></Show><div class="recipe-library" hidden=move || !context_valid.get()>
        <header class="recipe-library__topbar"><span>"Recipes"</span><div class="recipe-library__actions"><ButtonLink href="/beerxml" variant=ButtonVariant::Secondary>"Import BeerXML"</ButtonLink><ButtonLink href="/recipe-editor/new">"New recipe"</ButtonLink></div></header>
        <crate::components::wayfinding::PageTitle text=Signal::derive(|| "Recipes".to_owned())/><div class="recipe-library__intro"><p class="kit-showcase__eyebrow">"Your brewing notebook"</p><h1 id="recipes-heading" tabindex="-1">"Good things "<em>"start here."</em></h1><p>"Recipes to refine, return to, and make your own."</p><p>"Brew history and equipment are available from each recipe."</p></div>
        <div class="recipe-library__content">
        <BreweryPanel title="Recipe library" description="Search saved recipes or show archived records.">
            <div class="recipe-library__toolbar"><BreweryDraftField id="library-search" label="Search recipes" value=Signal::derive(move || search.get()) on_change=search_change/><BreweryDraftSelect id="library-filter" label="Show" value=archive_filter on_change=filter_change options=vec![("active".into(), "Active recipes".into()), ("archived".into(), "Archived recipes".into())]/></div>
            <Show when=move || loading.get()><BreweryNotice title="Loading recipes" body="Please wait."/></Show>
            {move || (!status.get().is_empty()).then(|| view! { <p role="status">{status.get()}</p> })}
            <Show when=move || !loading.get() && results.with(|items| items.is_empty()) && status.with(|s| s.is_empty())><BreweryNotice title="No recipes found" body="Change the search or create a recipe."/></Show>
            <ul class="recipe-library__list" on:click=remember_position>{move || results.get().into_iter().map(|item| {
                let id = item.recipe_id.clone(); let title = item.title.clone(); let archived_item = item.archived; let recovery_item = item.needs_recovery; let revision = item.metadata_revision;
                let history_id = id.clone(); let history_title = title.clone(); let archive_id = id.clone(); let recovery_id = id.clone();
                view! { <li class="recipe-library__row"><div class="recipe-library__identity"><h3>{title.clone()}</h3>
                    <p class="recipe-library__meta">{item.style.unwrap_or_else(|| "No style".into())}{item.batch_size_l.map(|v| format!(" · {v} L")).unwrap_or_default()}{format!(" · revision {}", item.revision)}{if archived_item { " · Archived" } else { "" }}</p></div><div class="recipe-library__actions">
                    {(!archived_item && !recovery_item).then(|| view! { <ButtonLink href=format!("/recipe-editor/{id}") variant=ButtonVariant::Secondary aria_label=format!("Open {title}")>"Open recipe"</ButtonLink> })}
                    {(!recovery_item).then(|| view! { <ButtonLink href=format!("/recipes/{}/brews", item.recipe_id) variant=ButtonVariant::GhostMuted aria_label=format!("Brews for {title}")>"Brews"</ButtonLink> })}
                    {recovery_item.then(|| view! { <Button variant=ButtonVariant::Secondary aria_label=format!("Recover stored source for {title}") on:click=move |_| recover.run(recovery_id.clone())>"Recover stored source"</Button> })}
                    {(!recovery_item).then(|| view! { <Button variant=ButtonVariant::Ghost aria_label=format!("Saved versions for {title}") on:click=move |_| show_history.run((history_id.clone(), history_title.clone()))>"Saved versions"</Button> })}
                    {(!recovery_item).then(|| view! { <Button variant=ButtonVariant::Secondary aria_label=format!("{} {title}", if archived_item { "Restore" } else { "Archive" }) disabled=Signal::derive(move || saving.get() || pending.get().is_some() || blocked.get()) on:click=move |_| {
                        let Some(request_id) = crate::utils::request_id::new_request_id() else { status.set("Unable to create request identity.".into()); return; };
                        execute_archive.run(ArchiveRecipe { intent: intent.get_value(), recipe_id: archive_id.clone(), expected_metadata_revision: revision, archived: !archived_item, request_id });
                    }>{if archived_item { "Restore" } else { "Archive" }}</Button> })}
                </div></li> }
            }).collect_view() }</ul>
            <Show when=move || next.get().is_some()><Button variant=ButtonVariant::Secondary disabled=loading on:click=move |_| load.run(next.get_untracked())>"Load more recipes"</Button></Show>
            <Show when=move || pending.get().is_some() && !blocked.get()><Button variant=ButtonVariant::Secondary disabled=saving on:click=move |_| { if let Some(command) = pending.get_untracked() { execute_archive.run(command); } }>"Retry previous request"</Button></Show>
        </BreweryPanel>
        {move || recovery.get().map(|(source, diagnostic)| { let href = format!("data:application/json;base64,{}", STANDARD.encode(source.as_bytes())); view! { <BreweryPanel title="Stored recipe source"><p>{diagnostic}</p><a href=href download="legacy-recipe.json">"Download stored source"</a><pre class="overflow-x-auto">{source}</pre></BreweryPanel> } })}
        {move || history.get().map(|(id, title)| view! { <RecipeRevisionHistory intent=intent.get_value() recipe_id=id recipe_title=title refresh_key=Signal::derive(|| 0_i64) on_authority_failure=invalidate_context selected_revision=selected_revision on_select=Callback::new(move |next| selected_revision.set(next))/> })}
        <Show when=move || detail_loading.get()><BreweryNotice title="Loading saved content" body="Please wait."/></Show>
        </div>
    </div> }
}
