// SPDX-License-Identifier: AGPL-3.0-or-later

//! Recipes — list, import, inspect, delete.
//!
//! Enough of a page to exercise the whole path end to end: store a recipe,
//! read it back, watch the engine derive it. The bench sheet, which is the
//! thing a brewer actually holds, comes later.

use leptos::prelude::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::components::{Button, ButtonLink, ButtonVariant, Card, EmptyState, Spinner, INPUT_CLASS};
use crate::server_fns::recipes::{
    delete_recipe, get_recipe, import_beerxml, list_recipes, recover_legacy_recipe, RecipeDetail, RecipeListItem,
};

#[component]
pub fn RecipesPage() -> impl IntoView {
    // Bumped after every write so the list and the open recipe both refetch.
    let (revision, set_revision) = signal(0_u32);
    let (selected, set_selected) = signal(Option::<String>::None);
    let (message, set_message) = signal(Option::<String>::None);

    let recipes = Resource::new(
        move || revision.get(),
        |_| async move { list_recipes().await },
    );

    let detail = Resource::new(
        move || (selected.get(), revision.get()),
        |(id, _)| async move {
            match id {
                Some(id) => get_recipe(id).await,
                None => Ok(None),
            }
        },
    );

    let recovery = Resource::new(
        move || (selected.get(), revision.get()),
        |(id, _)| async move {
            match id { Some(id) => recover_legacy_recipe(id).await, None => Ok(None) }
        },
    );

    let import = Action::new(|xml: &String| {
        let xml = xml.clone();
        async move { import_beerxml(xml).await }
    });

    Effect::new(move |_| {
        if let Some(result) = import.value().get() {
            match result {
                Ok(imported) => {
                    set_message.set(Some(format!(
                        "Imported {} recipe{}.",
                        imported.len(),
                        if imported.len() == 1 { "" } else { "s" }
                    )));
                    set_revision.update(|r| *r += 1);
                }
                Err(e) => set_message.set(Some(format!("Import failed: {e}"))),
            }
        }
    });

    let remove = Action::new(|id: &String| {
        let id = id.clone();
        async move { delete_recipe(id).await }
    });

    Effect::new(move |_| {
        if let Some(result) = remove.value().get() {
            match result {
                Ok(true) => {
                    set_message.set(Some("Recipe deleted.".to_string()));
                    set_selected.set(None);
                    set_revision.update(|r| *r += 1);
                }
                Ok(false) => set_message.set(Some("That recipe was already gone.".to_string())),
                Err(e) => set_message.set(Some(format!("Delete failed: {e}"))),
            }
        }
    });

    let paste = RwSignal::new(String::new());

    view! {
        <div class="p-6 max-w-6xl mx-auto space-y-6">
            <div>
                <h1 class="text-2xl font-semibold">"Recipes"</h1>
                <p class="text-muted-foreground mt-1">"Browse stored recipes and open the editor."</p>
                <ButtonLink href="/recipe-editor/new">"New recipe"</ButtonLink>
            </div>

            {move || message.get().map(|m| view! {
                <div class="rounded-md border border-border bg-muted px-4 py-2 text-sm">{m}</div>
            })}

            <Card class="p-4 space-y-3">
                <label class="text-sm font-medium" for="beerxml">"Import BeerXML"</label>
                <textarea
                    id="beerxml"
                    class=format!("{INPUT_CLASS} h-32 font-mono text-xs")
                    placeholder="<RECIPES><RECIPE>…</RECIPE></RECIPES>"
                    prop:value=move || paste.get()
                    on:input=move |ev| paste.set(event_target_value(&ev))
                />
                <div class="flex items-center gap-3">
                    <Button
                        on:click=move |_| {
                            let xml = paste.get_untracked();
                            if xml.trim().is_empty() {
                                set_message.set(Some("Nothing to import.".to_string()));
                            } else {
                                set_message.set(None);
                                import.dispatch(xml);
                            }
                        }
                    >
                        "Import"
                    </Button>
                    <Show when=move || import.pending().get()>
                        <Spinner/>
                    </Show>
                </div>
            </Card>

            <div class="grid gap-6 md:grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)]">
                // ── The list ────────────────────────────────────────────
                <Card class="p-4">
                    <h2 class="font-medium mb-3">"Stored"</h2>
                    <Suspense fallback=|| view! { <Spinner/> }>
                        {move || Suspend::new(async move {
                            match recipes.await {
                                Ok(items) if items.is_empty() => view! {
                                    <EmptyState
                                        title="No recipes yet"
                                        description="Import some BeerXML above to get started."
                                    />
                                }.into_any(),
                                Ok(items) => view! {
                                    <ul class="divide-y divide-border">
                                        {items.into_iter().map(|r| recipe_row(r, set_selected)).collect_view()}
                                    </ul>
                                }.into_any(),
                                Err(e) => view! {
                                    <p class="text-sm text-destructive">{format!("Could not load recipes: {e}")}</p>
                                }.into_any(),
                            }
                        })}
                    </Suspense>
                </Card>

                // ── The selected recipe ─────────────────────────────────
                <Card class="p-4">
                    <Suspense fallback=|| view! { <Spinner/> }>
                        {move || Suspend::new(async move {
                            match detail.await {
                                Ok(Some(d)) => recipe_detail(d, remove).into_any(),
                                Ok(None) => view! {
                                    <p class="text-sm text-muted-foreground">
                                        "Select a recipe to see what the engine makes of it."
                                    </p>
                                }.into_any(),
                                Err(e) => view! {
                                    <p class="text-sm text-destructive">{format!("Could not load that recipe: {e}")}</p>
                                }.into_any(),
                            }
                        })}
                    </Suspense>
                    <Suspense fallback=|| ()>
                        {move || Suspend::new(async move {
                            match recovery.await {
                                Ok(Some((source, diagnostic))) => {
                                    let href = format!("data:application/json;base64,{}", STANDARD.encode(source.as_bytes()));
                                    view! { <div class="mt-4 space-y-2"><p class="text-sm text-destructive">{diagnostic}</p><a class="underline" href=href download="legacy-recipe.json">"Download stored legacy record"</a><pre class="overflow-x-auto text-xs">{source}</pre></div> }.into_any()
                                },
                                _ => ().into_any(),
                            }
                        })}
                    </Suspense>
                </Card>
            </div>
        </div>
    }
}

fn recipe_row(r: RecipeListItem, set_selected: WriteSignal<Option<String>>) -> impl IntoView {
    let id = r.recipe_id.clone();
    let detail_line = {
        let mut parts = Vec::new();
        if let Some(style) = &r.style_name {
            parts.push(style.clone());
        }
        if let Some(size) = r.batch_size_l {
            parts.push(format!("{size:.1} L"));
        }
        if let Some(boil) = r.boil_minutes {
            parts.push(format!("{boil} min boil"));
        }
        parts.join(" · ")
    };

    view! {
        <li>
            <ButtonLink href=format!("/recipe-editor/{}", r.recipe_id) variant=ButtonVariant::Ghost>"Edit recipe"</ButtonLink>
            <button
                type="button"
                class="w-full text-left py-2 px-2 rounded-md hover:bg-secondary transition-colors duration-200 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                on:click=move |_| set_selected.set(Some(id.clone()))
            >
                <div class="flex items-baseline justify-between gap-3">
                    <span class="font-medium">{r.name}</span>
                    <span class="text-xs text-muted-foreground shrink-0">{r.source}</span>
                </div>
                <Show when={let d = detail_line.clone(); move || !d.is_empty()}>
                    <div class="text-xs text-muted-foreground">{detail_line.clone()}</div>
                </Show>
            </button>
        </li>
    }
}

fn recipe_detail(
    d: RecipeDetail,
    remove: Action<String, Result<bool, ServerFnError>>,
) -> impl IntoView {
    let id = d.recipe_id.clone();
    let warnings = d.warnings.clone();

    view! {
        <div class="space-y-4">
            <div class="flex items-start justify-between gap-3">
                <h2 class="font-medium">{d.name}</h2>
                <Button
                    variant=ButtonVariant::Destructive
                    on:click=move |_| { remove.dispatch(id.clone()); }
                >
                    "Delete"
                </Button>
            </div>

            {d.derived.map(|v| view! {
                <div class="grid grid-cols-2 gap-x-6 gap-y-1 text-sm">
                    {figure("Pre-boil", format!("{:.4}", v.pre_boil_gravity))}
                    {figure("OG", format!("{:.4}", v.original_gravity))}
                    {figure("FG", format!("{:.4}", v.final_gravity))}
                    {figure("ABV", format!("{:.1}%", v.abv_percent))}
                    {figure("Colour", format!("{:.1} EBC", v.colour_ebc))}
                    {figure("IBU", format!("{:.1}", v.ibu_total))}
                    {figure("IBU (boil only)", format!("{:.1}", v.ibu_boil_only))}
                    {figure("Pre-boil volume", format!("{:.2} L", v.pre_boil_l))}
                    {figure("Mash water", format!("{:.2} L", v.mash_water_l))}
                    {figure("Sparge water", format!("{:.2} L", v.sparge_water_l))}
                </div>
            })}

            <Show when={let w = warnings.clone(); move || !w.is_empty()}>
                <div class="rounded-md border border-border bg-muted p-3">
                    <div class="text-xs font-medium mb-1">"What the import could not do"</div>
                    <ul class="text-xs text-muted-foreground list-disc pl-4 space-y-0.5">
                        {warnings.clone().into_iter()
                            .map(|w| view! { <li>{w}</li> })
                            .collect_view()}
                    </ul>
                </div>
            </Show>

            <details>
                <summary class="text-xs text-muted-foreground cursor-pointer">
                    "BeerXML (everything stored, including what Krausen does not use)"
                </summary>
                <pre class="mt-2 text-[10px] font-mono overflow-x-auto max-h-80 bg-muted p-2 rounded">
                    {d.beerxml}
                </pre>
            </details>
        </div>
    }
}

fn figure(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="flex justify-between gap-2 border-b border-border/50 py-0.5">
            <span class="text-muted-foreground">{label}</span>
            <span class="font-medium tabular-nums">{value}</span>
        </div>
    }
}
