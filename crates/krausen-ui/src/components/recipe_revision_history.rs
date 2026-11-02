// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared, read-only revision history for the library and editor.
use crate::{
    components::{BreweryNotice, BreweryPanel, Button, ButtonVariant},
    pages::recipe_editor::current_context_matches,
    server_fns::recipe_library::library_command_shared,
};
use krausen_types::{
    brewery::RecipeDocumentV1,
    recipe_library::{HistoryPage, HistoryQuery, LibraryCommand, LibraryOutcome, RevisionQuery},
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

/// Read-only presentation of retained source data; no targets are recalculated.
#[component]
fn HistoricalRecipe(revision: i64, document: RecipeDocumentV1) -> impl IntoView {
    let title = format!("Historical revision {revision} · {}", document.title);
    let style = document.style.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| "Not specified".into());
    let batch = document.batch_size_l.map(|v| format!("{v} L")).unwrap_or_else(|| "Not specified".into());
    let boil = document.boil_minutes.map(|v| format!("{v} min")).unwrap_or_else(|| "Not specified".into());
    let fermentables = document.fermentables.iter().map(|r| format!("{} · {} · {}", r.specification.name.as_deref().unwrap_or("Unnamed fermentable"), r.amount_kg.map(|v| format!("{v} kg")).unwrap_or_else(|| "quantity not specified".into()), r.addition.stage)).collect::<Vec<_>>();
    let hops = document.hops.iter().map(|r| format!("{} · {} · {}", r.specification.name.as_deref().unwrap_or("Unnamed hop"), r.amount_g.map(|v| format!("{v} g")).unwrap_or_else(|| "quantity not specified".into()), r.stage)).collect::<Vec<_>>();
    let yeast = document.yeast.iter().map(|r| format!("{} · {}", r.specification.name.as_deref().unwrap_or("Unnamed yeast"), ingredient_amount(r.amount, r.unit.as_deref()))).collect::<Vec<_>>();
    let other = document.other_ingredients.iter().map(|r| format!("{} · {}", r.specification.name.as_deref().unwrap_or("Unnamed ingredient"), ingredient_amount(r.amount, r.unit.as_deref()))).collect::<Vec<_>>();
    let mash = document.process.mash_steps.iter().map(step_text).collect::<Vec<_>>();
    let fermentation = document.process.fermentation_steps.iter().map(step_text).collect::<Vec<_>>();
    let saved_data = match serde_json::to_string_pretty(&document) {
        Ok(value) => value,
        Err(error) => { tracing::warn!(error = %error, "Unable to display saved revision data"); "Saved data could not be displayed.".into() }
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
        Some(value) => match unit { Some(unit) if !unit.is_empty() => format!("{value} {unit}"), _ => value.to_string() },
        None => "quantity not specified".into(),
    }
}
fn step_text(step: &krausen_types::brewery::ProcessStep) -> String {
    let mut parts = vec![step.name.as_deref().unwrap_or("Unnamed step").to_owned()];
    if let Some(value) = step.temperature_c { parts.push(format!("{value} °C")); }
    if let Some(value) = step.time_minutes { parts.push(format!("{value} min")); }
    if let Some(value) = step.volume_l { parts.push(format!("{value} L")); }
    parts.join(" · ")
}
fn saved_rows(items: Vec<String>) -> impl IntoView {
    if items.is_empty() { view! { <p>"None saved"</p> }.into_any() }
    else { view! { <ul>{items.into_iter().map(|item| view! { <li>{item}</li> }).collect_view()}</ul> }.into_any() }
}

#[component]
pub fn RecipeRevisionHistory(
    intent: OperationIntent,
    recipe_id: String,
    recipe_title: String,
    refresh_key: Signal<i64>,
    on_authority_failure: Callback<()>,
) -> impl IntoView {
    let intent = StoredValue::new(intent);
    let recipe_id = StoredValue::new(recipe_id);
    let page = RwSignal::new(None::<HistoryPage>);
    let selected = RwSignal::new(None::<(i64, RecipeDocumentV1)>);
    let generation = RwSignal::new(0_u64);
    let detail_generation = RwSignal::new(0_u64);
    let loading = RwSignal::new(false);
    let detail_loading = RwSignal::new(false);
    let error = RwSignal::new(String::new());
    let load = Callback::new(move |cursor: Option<String>| {
        if loading.get_untracked() { return; }
        let append = cursor.is_some();
        let sequence = generation.get_untracked();
        let expected_refresh = refresh_key.get_untracked();
        let captured = intent.get_value();
        let query = HistoryQuery { intent: captured.clone(), recipe_id: recipe_id.get_value(), cursor, limit: 50 };
        loading.set(true);
        error.set(String::new());
        leptos::task::spawn_local(async move {
            let result = library_command_shared(LibraryCommand::History(query)).await;
            if authority_failed(&result) { let _ = on_authority_failure.try_run(()); return; }
            if generation.try_get_untracked().is_none() { return; }
            if !current_context_matches(&captured).await {
                if generation.try_get_untracked().is_some() { let _ = on_authority_failure.try_run(()); }
                return;
            }
            if generation.try_get_untracked() != Some(sequence) || refresh_key.try_get_untracked() != Some(expected_refresh) { return; }
            loading.set(false);
            match result {
                Ok(LibraryOutcome::History(next)) => {
                    if append {
                        page.update(|current| if let Some(current) = current { current.items.extend(next.items); current.next_cursor = next.next_cursor; });
                    } else { page.set(Some(next)); }
                }
                _ => error.set("Unable to load revision history. Try again.".into()),
            }
        });
    });
    Effect::new(move |_| {
        let _ = refresh_key.get();
        generation.update(|v| *v += 1);
        detail_generation.update(|v| *v += 1);
        page.set(None);
        selected.set(None);
        loading.set(false);
        detail_loading.set(false);
        load.run(None);
    });
    let inspect = Callback::new(move |revision: i64| {
        detail_generation.update(|v| *v += 1);
        let sequence = detail_generation.get_untracked();
        let expected_refresh = refresh_key.get_untracked();
        let captured = intent.get_value();
        let query = RevisionQuery { intent: captured.clone(), recipe_id: recipe_id.get_value(), revision };
        selected.set(None);
        detail_loading.set(true);
        error.set(String::new());
        leptos::task::spawn_local(async move {
            let result = library_command_shared(LibraryCommand::Revision(query)).await;
            if authority_failed(&result) { let _ = on_authority_failure.try_run(()); return; }
            if detail_generation.try_get_untracked().is_none() { return; }
            if !current_context_matches(&captured).await {
                if detail_generation.try_get_untracked().is_some() { let _ = on_authority_failure.try_run(()); }
                return;
            }
            if detail_generation.try_get_untracked() != Some(sequence) || refresh_key.try_get_untracked() != Some(expected_refresh) { return; }
            detail_loading.set(false);
            match result {
                Ok(LibraryOutcome::Revision(document)) => selected.set(Some((revision, document))),
                _ => error.set("Unable to load that saved revision. Try again.".into()),
            }
        });
    });
    let title = format!("History · {recipe_title}");
    view! {
        <BreweryPanel title=title>
            <Show when=move || loading.get()><BreweryNotice title="Loading history" body="Please wait."/></Show>
            {move || (!error.get().is_empty()).then(|| view! { <p role="status">{error.get()}</p> })}
            {move || page.get().map(|history| {
                let empty = history.items.is_empty();
                let cursor = history.next_cursor;
                view! {
                    {empty.then(|| view! { <p>"No saved revisions."</p> })}
                    <ul>{history.items.into_iter().map(|entry| {
                        let revision = entry.revision;
                        view! { <li>{format!("Revision {revision}")}{entry.saved_at.map(|date| format!(" · {date}")).unwrap_or_default()}{entry.restored_from_revision.map(|r| format!(" · restored from {r}")).unwrap_or_default()}<Button variant=ButtonVariant::Ghost aria_label=format!("Inspect revision {revision}") on:click=move |_| inspect.run(revision)>"Inspect"</Button></li> }
                    }).collect_view()}</ul>
                    {cursor.map(|cursor| view! { <Button variant=ButtonVariant::Secondary disabled=loading on:click=move |_| load.run(Some(cursor.clone()))>"Load more revisions"</Button> })}
                }
            })}
            <Show when=move || !loading.get() && page.get().is_none() && !error.get().is_empty()><Button variant=ButtonVariant::Secondary on:click=move |_| load.run(None)>"Retry history"</Button></Show>
        </BreweryPanel>
        <Show when=move || detail_loading.get()><BreweryNotice title="Loading saved content" body="Please wait."/></Show>
        {move || selected.get().map(|(revision, document)| view! { <HistoricalRecipe revision=revision document=document/> })}
    }
}
