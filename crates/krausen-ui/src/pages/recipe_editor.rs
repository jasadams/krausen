// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recipe editor over the shared, authorized recipe operations.
use crate::{
    components::{
        BreweryAddition, BreweryDisclosure, BreweryDraftArea, BreweryDraftField,
        BreweryDraftSelect, BreweryNotice, BreweryPanel, Button,
    },
    pages::recipe_draft::RecipeDraft,
    server_fns::recipe_editor::{read_recipe, save_recipe},
};
use krausen_types::{
    brewery::RecipeDocumentV1,
    recipe_editor::{RecipeReadOutcome, RecipeSaveOutcome, RecipeSnapshot},
};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

type EditorIdentity = krausen_types::request_context::OperationIntent;

pub(super) async fn current_context_matches(expected: &EditorIdentity) -> bool {
    let result = crate::server_fns::context::get_user_context().await;
    #[cfg(target_arch = "wasm32")]
    let result = if result
        .as_ref()
        .err()
        .is_some_and(|e| crate::utils::auth_refresh::is_auth_error(&e.to_string()))
        && crate::utils::auth_refresh::try_refresh().await
    {
        crate::server_fns::context::get_user_context().await
    } else {
        result
    };
    result.is_ok_and(|ctx| {
        ctx.user_id == expected.account_id
            && ctx.workspace_id.as_deref() == Some(expected.workspace_id.as_str())
    })
}

fn comparison_text(document: &RecipeDocumentV1) -> String {
    fn visit(value: &serde_json::Value, label: &str, lines: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(fields) => {
                for (name, value) in fields {
                    if name == "schema_version" || name == "row_id" {
                        continue;
                    }
                    let name = name.replace('_', " ");
                    let path = if label.is_empty() {
                        name
                    } else {
                        format!("{label} / {name}")
                    };
                    visit(value, &path, lines);
                }
            }
            serde_json::Value::Array(items) => {
                for (index, value) in items.iter().enumerate() {
                    visit(value, &format!("{label} {}", index + 1), lines);
                }
            }
            serde_json::Value::Null => {}
            serde_json::Value::String(text) if text.is_empty() => {}
            serde_json::Value::String(text) => lines.push(format!("{label}: {text}")),
            value => lines.push(format!("{label}: {value}")),
        }
    }
    let mut lines = Vec::new();
    if let Ok(value) = serde_json::to_value(document) {
        visit(&value, "", &mut lines);
    }
    if lines.is_empty() {
        "No recipe details".into()
    } else {
        lines.join("\n")
    }
}

#[component]
pub fn RecipeEditorPage() -> impl IntoView {
    let params = use_params_map();
    let load = LocalResource::new(move || {
        let id = params.read().get("id").unwrap_or_default();
        async move {
            let context = crate::server_fns::context::get_user_context().await;
            #[cfg(target_arch = "wasm32")]
            let context = if context
                .as_ref()
                .err()
                .is_some_and(|e| crate::utils::auth_refresh::is_auth_error(&e.to_string()))
                && crate::utils::auth_refresh::try_refresh().await
            {
                crate::server_fns::context::get_user_context().await
            } else {
                context
            };
            let context = context?;
            let identity = EditorIdentity {
                account_id: context.user_id,
                workspace_id: context
                    .workspace_id
                    .ok_or_else(|| ServerFnError::new("Workspace required"))?,
            };
            let result = if id == "new" {
                None
            } else {
                let result = read_recipe(
                    identity.account_id.clone(),
                    identity.workspace_id.clone(),
                    id.clone(),
                )
                .await;
                #[cfg(target_arch = "wasm32")]
                let result = if result
                    .as_ref()
                    .err()
                    .is_some_and(|e| crate::utils::auth_refresh::is_auth_error(&e.to_string()))
                    && crate::utils::auth_refresh::try_refresh().await
                {
                    read_recipe(
                        identity.account_id.clone(),
                        identity.workspace_id.clone(),
                        id,
                    )
                    .await
                } else {
                    result
                };
                Some(result?)
            };
            Ok::<_, ServerFnError>((identity, result))
        }
    });
    view! { <Suspense fallback=|| view! { <p>"Loading recipe…"</p> }>
        {move || Suspend::new(async move { match load.await {
            Ok((identity, None)) => view! { <RecipeForm identity=identity initial=None/> }.into_any(),
            Ok((identity, Some(RecipeReadOutcome::Found(snapshot)))) => view! { <RecipeForm identity=identity initial=Some(snapshot)/> }.into_any(),
            Ok((_, Some(RecipeReadOutcome::NotFound))) => view! { <BreweryNotice title="Recipe not found" body="It may have been archived or belongs to another workspace."/> }.into_any(),
            Ok((_, Some(RecipeReadOutcome::Forbidden))) => view! { <BreweryNotice title="Access denied" body="You no longer have access to this workspace." warning=true/> }.into_any(),
            Ok((_, Some(RecipeReadOutcome::ContextChanged))) => view! { <BreweryNotice title="Account or workspace changed" body="Return to the original context and reopen this recipe." warning=true/> }.into_any(),
            _ => view! { <BreweryNotice title="Unable to load recipe" body="Try again when the connection is available." warning=true/> }.into_any(),
        } })}
    </Suspense> }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct PendingRecipeSave {
    recipe_id: Option<String>,
    expected: Option<i64>,
    document: RecipeDocumentV1,
    request_id: String,
    generation: u64,
}

#[component]
fn RecipeForm(identity: EditorIdentity, initial: Option<RecipeSnapshot>) -> impl IntoView {
    let identity = RwSignal::new(identity);
    let context_visible = RwSignal::new(true);
    let profile_generation = RwSignal::new(0_u64);
    let preview_gate = StoredValue::new(crate::pages::recipe_preview_gate::PreviewGate::default());
    let recipe_id = RwSignal::new(initial.as_ref().map(|s| s.recipe_id.clone()));
    let revision = RwSignal::new(initial.as_ref().map(|s| s.revision));
    let baseline = RwSignal::new(
        initial
            .as_ref()
            .map(|s| s.document.clone())
            .unwrap_or_default(),
    );
    let draft = RwSignal::new(RecipeDraft::new(
        initial.map(|s| s.document).unwrap_or_default(),
    ));
    #[cfg(target_arch = "wasm32")]
    {
        let check_generation = RwSignal::new(0_u64);
        let recheck = Callback::new(move |()| {
            context_visible.set(false);
            preview_gate.update_value(|gate| {
                gate.invalidate();
            });
            let generation = check_generation
                .get_untracked()
                .checked_add(1)
                .expect("context check generation exhausted");
            check_generation.set(generation);
            let expected = identity.get_untracked();
            leptos::task::spawn_local(async move {
                let matches = current_context_matches(&expected).await;
                if check_generation.try_get_untracked() == Some(generation) {
                    context_visible.set(matches);
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
    let status = RwSignal::new(String::new());
    let pending = RwSignal::new(false);
    let retry = RwSignal::new(None::<PendingRecipeSave>);
    let conflict = RwSignal::new(None::<RecipeSnapshot>);
    let editor_guard = expect_context::<crate::components::layout::EditorGuard>();
    #[cfg(target_arch = "wasm32")]
    {
        // The router may mount this form before it writes its new URL to
        // browser history, so derive the current route from router state.
        let location = leptos_router::hooks::use_location();
        let origin = web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_else(|| {
                tracing::warn!("Unable to read the editor origin");
                String::new()
            });
        editor_guard.current_url.set(format!(
            "{}{}{}{}",
            origin,
            location.pathname.get_untracked(),
            location.search.get_untracked(),
            location.hash.get_untracked()
        ));
    };
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{JsCast, closure::Closure};
        let window = web_sys::window().expect("browser window");
        let unload = Closure::<dyn FnMut(web_sys::BeforeUnloadEvent)>::new(
            move |event: web_sys::BeforeUnloadEvent| {
                if editor_guard.dirty.get_untracked() {
                    event.prevent_default();
                    event.set_return_value("");
                }
            },
        );
        if window
            .add_event_listener_with_callback("beforeunload", unload.as_ref().unchecked_ref())
            .is_err()
        {
            tracing::warn!("Unable to install the unsaved-changes warning");
            editor_guard.navigation_error.set(
                "The browser's unsaved-changes warning is unavailable; keep this page open.".into(),
            );
        }
        let listener = send_wrapper::SendWrapper::new((window, unload));
        on_cleanup(move || {
            let (window, unload) = &*listener;
            if window
                .remove_event_listener_with_callback(
                    "beforeunload",
                    unload.as_ref().unchecked_ref(),
                )
                .is_err()
            {
                tracing::warn!("Unable to remove the unsaved-changes warning");
            }
        });
    }
    Effect::new(move |_| editor_guard.message.set(status.get()));
    Effect::new(move |_| editor_guard.pending.set(pending.get()));
    Effect::new(move |_| {
        editor_guard
            .dirty
            .set(draft.with(|d| d.dirty) || pending.get() || retry.get().is_some())
    });
    on_cleanup(move || {
        editor_guard.dirty.set(false);
        editor_guard.pending.set(false);
    });
    let recovery_ready = RwSignal::new(false);
    let retained = RwSignal::new(false);
    let recovery_choice =
        RwSignal::new(Vec::<(String, crate::pages::recipe_recovery::RecoveryRecord)>::new());
    let recovery_storage_key = RwSignal::new(None::<String>);
    #[cfg(target_arch = "wasm32")]
    {
        use crate::pages::recipe_recovery::{RecoveryRecord, RecoveryScope};
        let installation = web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_else(|| {
                tracing::warn!("Unable to scope local draft recovery to an installation");
                String::new()
            });
        let expected = identity.get_untracked();
        let initial_scope = RecoveryScope {
            installation: installation.clone(),
            account_id: expected.account_id.clone(),
            workspace_id: expected.workspace_id.clone(),
            recipe_id: recipe_id.get_untracked(),
            base_revision: revision.get_untracked(),
        };
        let tab = crate::utils::request_id::new_request_id();
        let checking_recovery = RwSignal::new(false);
        Effect::new(move |_| {
            if recovery_ready.get() || !context_visible.get() || checking_recovery.get() {
                return;
            }
            if initial_scope.installation.is_empty() {
                status.set("Local draft recovery is unavailable in this browser.".into());
                recovery_ready.set(true);
                return;
            }
            checking_recovery.set(true);
            let expected = expected.clone();
            let initial_scope = initial_scope.clone();
            leptos::task::spawn_local(async move {
                if !current_context_matches(&expected).await {
                    context_visible.set(false);
                    checking_recovery.set(false);
                    return;
                }
                let authorized = crate::server_fns::formulation::list_formulation_styles(
                    expected.account_id.clone(),
                    expected.workspace_id.clone(),
                )
                .await;
                if !matches!(
                    authorized,
                    Ok(krausen_types::formulation::StyleListOutcome::Styles(_))
                ) || !current_context_matches(&expected).await
                {
                    context_visible.set(false);
                    checking_recovery.set(false);
                    return;
                }
                match crate::pages::recipe_recovery::find(&initial_scope) {
                    Ok(candidate) => recovery_choice.set(candidate),
                    Err(_) => {
                        tracing::warn!("Unable to read retained recipe drafts");
                        status.set("Local draft recovery is unavailable in this browser.".into())
                    }
                }
                recovery_ready.set(true);
                checking_recovery.set(false);
            });
        });
        Effect::new(move |_| {
            let ready = recovery_ready.get();
            let choosing = !recovery_choice.get().is_empty();
            let visible = context_visible.get();
            let current = draft.get();
            let in_flight = retry.get();
            let current_id = recipe_id.get();
            let current_revision = revision.get();
            if !ready || choosing || !visible {
                return;
            }
            if installation.is_empty() {
                return;
            }
            let Some(tab) = tab.as_ref() else {
                if current.dirty {
                    status.set("Local draft recovery is unavailable in this browser.".into());
                }
                return;
            };
            if current.dirty || in_flight.is_some() {
                let pinned = identity.get_untracked();
                let scope = RecoveryScope {
                    installation: installation.clone(),
                    account_id: pinned.account_id,
                    workspace_id: pinned.workspace_id,
                    recipe_id: current_id,
                    base_revision: current_revision,
                };
                let key = scope.key(tab);
                let record = RecoveryRecord {
                    version: 1,
                    scope,
                    draft: current.recovery(),
                    generation: current.generation,
                    pending: in_flight,
                    updated_at: js_sys::Date::now(),
                };
                if crate::pages::recipe_recovery::write(&key, &record).is_err() {
                    tracing::warn!("Unable to retain a recipe draft locally");
                    retained.set(false);
                    status.set("Unable to retain this draft locally; keep this page open.".into());
                } else {
                    retained.set(true);
                    if let Some(old) = recovery_storage_key
                        .get_untracked()
                        .filter(|old| old != &key)
                    {
                        if crate::pages::recipe_recovery::remove(&old).is_err() {
                            tracing::warn!("Unable to remove an older retained recipe draft");
                            status.set(
                                "An older retained draft could not be removed from this browser."
                                    .into(),
                            );
                        }
                    }
                    recovery_storage_key.set(Some(key));
                }
            } else if let Some(old) = recovery_storage_key.get_untracked() {
                if crate::pages::recipe_recovery::remove(&old).is_ok() {
                    recovery_storage_key.set(None);
                    retained.set(false);
                } else {
                    tracing::warn!("Unable to remove a retained recipe draft");
                    status.set("The retained draft could not be removed from this browser.".into());
                }
            }
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    recovery_ready.set(true);
    #[cfg(target_arch = "wasm32")]
    {
        let focus = window_event_listener(leptos::ev::focus, move |_| {
            if !recovery_ready.get_untracked()
                || !recovery_choice.get_untracked().is_empty()
                || pending.get_untracked()
            {
                return;
            }
            let (Some(id), Some(observed)) = (recipe_id.get_untracked(), revision.get_untracked())
            else {
                return;
            };
            let expected = identity.get_untracked();
            leptos::task::spawn_local(async move {
                if !current_context_matches(&expected).await {
                    context_visible.set(false);
                    return;
                }
                let result = read_recipe(
                    expected.account_id.clone(),
                    expected.workspace_id.clone(),
                    id.clone(),
                )
                .await;
                if !current_context_matches(&expected).await {
                    context_visible.set(false);
                    return;
                }
                if recipe_id.try_get_untracked() != Some(Some(id))
                    || revision.try_get_untracked() != Some(Some(observed))
                    || pending.try_get_untracked() != Some(false)
                {
                    return;
                }
                if let Ok(RecipeReadOutcome::Found(snapshot)) = result {
                    if snapshot.revision != observed {
                        if draft.with_untracked(|d| d.dirty) || retry.get_untracked().is_some() {
                            status.set(format!(
                                "Changed elsewhere (current revision {}). Your draft is unchanged.",
                                snapshot.revision
                            ));
                            conflict.set(Some(snapshot));
                        } else {
                            baseline.set(snapshot.document.clone());
                            revision.set(Some(snapshot.revision));
                            draft.update(|d| d.replace_saved(snapshot.document));
                            status.set("Updated to the current saved revision.".into());
                        }
                    }
                }
            });
        });
        on_cleanup(move || focus.remove());
    }
    let styles = RwSignal::new(Vec::<krausen_types::brewery::StyleReference>::new());
    let style_sequence = RwSignal::new(0_u64);
    Effect::new(move |_| {
        let visible = context_visible.get();
        let sequence = style_sequence
            .get_untracked()
            .checked_add(1)
            .expect("style request generation exhausted");
        style_sequence.set(sequence);
        if !visible {
            styles.set(Vec::new());
            return;
        }
        let expected = identity.get_untracked();
        leptos::task::spawn_local(async move {
            if !current_context_matches(&expected).await {
                if style_sequence.try_get_untracked() == Some(sequence) {
                    context_visible.set(false);
                }
                return;
            }
            let result = crate::server_fns::formulation::list_formulation_styles(
                expected.account_id.clone(),
                expected.workspace_id.clone(),
            )
            .await;
            #[cfg(target_arch = "wasm32")]
            let result = if result
                .as_ref()
                .err()
                .is_some_and(|e| crate::utils::auth_refresh::is_auth_error(&e.to_string()))
                && crate::utils::auth_refresh::try_refresh().await
            {
                if !current_context_matches(&expected).await {
                    if style_sequence.try_get_untracked() == Some(sequence) {
                        context_visible.set(false);
                    }
                    return;
                }
                crate::server_fns::formulation::list_formulation_styles(
                    expected.account_id.clone(),
                    expected.workspace_id.clone(),
                )
                .await
            } else {
                result
            };
            if style_sequence.try_get_untracked() != Some(sequence)
                || context_visible.try_get_untracked() != Some(true)
            {
                return;
            }
            match result {
                Ok(krausen_types::formulation::StyleListOutcome::Styles(list)) => styles.set(list),
                Ok(
                    krausen_types::formulation::StyleListOutcome::ContextChanged
                    | krausen_types::formulation::StyleListOutcome::Forbidden,
                ) => context_visible.set(false),
                Err(e) if crate::utils::auth_refresh::is_auth_error(&e.to_string()) => {
                    context_visible.set(false)
                }
                _ => styles.set(Vec::new()),
            }
        });
    });
    let selected_style = Signal::derive(move || {
        draft.with(|d| {
            d.document()
                .style
                .as_ref()
                .map(|style| {
                    if styles.with(|list| list.contains(style)) {
                        style.name.clone()
                    } else {
                        format!("Unavailable: {}", style.name)
                    }
                })
                .unwrap_or_default()
        })
    });
    let change_style = Callback::new(move |value: String| {
        if value.is_empty() {
            draft.update(|d| d.edit(|doc| doc.style = None));
        } else if let Some(style) =
            styles.with_untracked(|list| list.iter().find(|style| style.name == value).cloned())
        {
            draft.update(|d| d.edit(|doc| doc.style = Some(style)));
        }
    });
    let title = Signal::derive(move || draft.with(|d| d.document().title.clone()));
    let title_change =
        Callback::new(move |value: String| draft.update(|d| d.edit(|doc| doc.title = value)));
    let notes =
        Signal::derive(move || draft.with(|d| d.document().notes.clone().unwrap_or_default()));
    let description = Signal::derive(move || {
        draft.with(|d| d.document().description.clone().unwrap_or_default())
    });
    let description_change = Callback::new(move |value: String| {
        draft.update(|d| d.edit(|doc| doc.description = (!value.is_empty()).then_some(value)))
    });
    let notes_change = Callback::new(move |value: String| {
        draft.update(|d| d.edit(|doc| doc.notes = (!value.is_empty()).then_some(value)))
    });
    let mash_method = Signal::derive(move || {
        draft.with(|d| {
            d.document()
                .process
                .mash_water_method
                .clone()
                .unwrap_or_default()
        })
    });
    let mash_method_change = Callback::new(move |value: String| {
        draft.update(|d| {
            d.edit(|doc| doc.process.mash_water_method = (!value.is_empty()).then_some(value))
        })
    });

    let chilling = Signal::derive(move || {
        draft.with(|d| {
            match &d.document().equipment_overrides.chilling {
                None => "",
                Some(krausen_types::brewery::ChillingV1::Natural) => "natural",
                Some(krausen_types::brewery::ChillingV1::Forced { .. }) => "forced",
                Some(krausen_types::brewery::ChillingV1::Counterflow { .. }) => "counterflow",
            }
            .to_owned()
        })
    });
    let chilling_change = Callback::new(move |value: String| {
        draft.update(|d| {
            d.edit_clearing("equipment_overrides.chilling.", |doc| {
                doc.equipment_overrides.chilling = match value.as_str() {
                    "natural" => Some(krausen_types::brewery::ChillingV1::Natural),
                    "forced" => Some(krausen_types::brewery::ChillingV1::Forced {
                        rate_per_minute: None,
                        plateau_c: None,
                    }),
                    "counterflow" => Some(krausen_types::brewery::ChillingV1::Counterflow {
                        litres_per_minute: None,
                    }),
                    _ => None,
                };
            })
        })
    });
    let save = move || {
        let identity_signal = identity;
        if pending.get_untracked()
            || !context_visible.get_untracked()
            || !recovery_ready.get_untracked()
            || !recovery_choice.get_untracked().is_empty()
        {
            editor_guard.saving_to_leave.set(false);
            return;
        }
        let attempt = if let Some(attempt) = retry.get_untracked() {
            attempt
        } else {
            let snapshot = draft.with_untracked(|d| (d.snapshot(), d.generation));
            let Ok(document) = snapshot.0 else {
                status.set("Complete invalid number fields before saving.".into());
                editor_guard.saving_to_leave.set(false);
                return;
            };
            let Some(request_id) = crate::utils::request_id::new_request_id() else {
                status.set("Unable to create a secure save request.".into());
                editor_guard.saving_to_leave.set(false);
                return;
            };
            let attempt = PendingRecipeSave {
                recipe_id: recipe_id.get_untracked(),
                expected: revision.get_untracked(),
                document,
                request_id,
                generation: snapshot.1,
            };
            retry.set(Some(attempt.clone()));
            attempt
        };
        pending.set(true);
        editor_guard.pending.set(true);
        status.set("Saving…".into());
        leptos::task::spawn_local(async move {
            #[cfg(target_arch = "wasm32")]
            let creating = attempt.recipe_id.is_none();
            let identity = identity.get_untracked();
            let same_context = current_context_matches(&identity).await;
            if pending.try_get_untracked().is_none() {
                return;
            }
            if !same_context {
                context_visible.set(false);
                pending.set(false);
                editor_guard.pending.set(false);
                editor_guard.saving_to_leave.set(false);
                return;
            }
            let result = save_recipe(
                identity.account_id.clone(),
                identity.workspace_id.clone(),
                attempt.recipe_id.clone(),
                attempt.expected,
                attempt.document.clone(),
                attempt.request_id.clone(),
            )
            .await;
            #[cfg(target_arch = "wasm32")]
            let result = if result
                .as_ref()
                .err()
                .is_some_and(|e| crate::utils::auth_refresh::is_auth_error(&e.to_string()))
                && crate::utils::auth_refresh::try_refresh().await
            {
                if !current_context_matches(&identity).await {
                    context_visible.set(false);
                    pending.set(false);
                    editor_guard.pending.set(false);
                    editor_guard.saving_to_leave.set(false);
                    return;
                }
                save_recipe(
                    identity.account_id,
                    identity.workspace_id,
                    attempt.recipe_id,
                    attempt.expected,
                    attempt.document.clone(),
                    attempt.request_id,
                )
                .await
            } else {
                result
            };
            if pending.try_get_untracked().is_none() {
                return;
            }
            pending.set(false);
            editor_guard.pending.set(false);
            if !matches!(&result, Ok(RecipeSaveOutcome::Saved { .. })) {
                editor_guard.saving_to_leave.set(false);
            }
            match result {
                Ok(RecipeSaveOutcome::Saved {
                    recipe_id: id,
                    revision: saved,
                }) => {
                    retry.set(None);
                    baseline.set(attempt.document.clone());
                    #[cfg(target_arch = "wasm32")]
                    if creating {
                        if let Some(window) = web_sys::window() {
                            let result = window.history().and_then(|history| {
                                history.replace_state_with_url(
                                    &wasm_bindgen::JsValue::NULL,
                                    "",
                                    Some(&format!("/recipe-editor/{id}")),
                                )
                            });
                            if result.is_err() {
                                tracing::warn!("Could not update the saved recipe address");
                            } else if let Ok(href) = window.location().href() {
                                editor_guard.current_url.set(href);
                            }
                        }
                    }
                    recipe_id.set(Some(id));
                    revision.set(Some(saved));
                    if draft.with_untracked(|d| d.generation) == attempt.generation {
                        draft.update(|d| d.mark_saved());
                        status.set(format!("Saved revision {saved}."));
                        #[cfg(target_arch = "wasm32")]
                        let local_cleared = recovery_storage_key
                            .get_untracked()
                            .is_none_or(|key| crate::pages::recipe_recovery::remove(&key).is_ok());
                        #[cfg(not(target_arch = "wasm32"))]
                        let local_cleared = true;
                        if local_cleared {
                            recovery_storage_key.set(None);
                            if editor_guard.saving_to_leave.get_untracked() {
                                editor_guard.finish();
                            }
                        } else {
                            tracing::warn!("Unable to clear the retained draft after saving");
                            editor_guard.saving_to_leave.set(false);
                            status.set(format!("Saved revision {saved}, but the retained local draft could not be cleared."));
                        }
                    } else {
                        editor_guard.saving_to_leave.set(false);
                        draft.update(|d| d.mark_earlier_document_saved(attempt.document.clone()));
                        status.set(format!(
                            "Saved revision {saved}; newer changes remain unsaved."
                        ));
                    }
                }
                Ok(RecipeSaveOutcome::Invalid(validation)) => {
                    retry.set(None);
                    if draft.with_untracked(|d| d.generation) == attempt.generation {
                        draft.update(|d| d.set_validation(validation));
                        status.set("Correct the invalid fields before saving.".into());
                    } else {
                        status.set(
                            "An earlier save was invalid; your newer changes remain unsaved."
                                .into(),
                        );
                    }
                }
                Ok(RecipeSaveOutcome::Conflict { actual, .. }) => {
                    retry.set(None);
                    status.set(format!(
                        "Changed elsewhere (current revision {actual}). Your draft is unchanged."
                    ));
                    if let Some(id) = recipe_id.get_untracked() {
                        let expected = identity_signal.get_untracked();
                        if current_context_matches(&expected).await {
                            let response = read_recipe(
                                expected.account_id.clone(),
                                expected.workspace_id.clone(),
                                id,
                            )
                            .await;
                            if !current_context_matches(&expected).await
                                || !context_visible.get_untracked()
                            {
                                context_visible.set(false);
                                return;
                            }
                            match response {
                                Ok(RecipeReadOutcome::Found(snapshot)) => conflict.set(Some(snapshot)),
                                Ok(RecipeReadOutcome::Forbidden | RecipeReadOutcome::ContextChanged) => context_visible.set(false),
                                _ => status.set("The current saved revision could not be loaded. Your draft is unchanged.".into()),
                            }
                        } else {
                            context_visible.set(false);
                        }
                    }
                }
                Ok(RecipeSaveOutcome::ResultExpired { resource_id }) => {
                    status.set(match resource_id {
                        Some(id) => format!(
                            "The saved response expired. Reopen recipe {id} before saving again."
                        ),
                        None => {
                            "The saved response expired. Reopen the recipe before saving again."
                                .into()
                        }
                    });
                }
                Ok(RecipeSaveOutcome::RequestIdentityConflict) => {
                    status.set("The prior save request could not be verified. Reopen the recipe before saving again.".into());
                }
                Ok(RecipeSaveOutcome::NotFound) => {
                    retry.set(None);
                    status.set("This recipe is no longer available in this workspace. Your draft is still open.".into());
                }
                Ok(RecipeSaveOutcome::InvalidRequest) => {
                    retry.set(None);
                    status.set("The save request was invalid. Your draft is still open.".into());
                }
                Ok(RecipeSaveOutcome::ContextChanged) => {
                    context_visible.set(false);
                    status.set("Your account or workspace changed. The draft is held in its original context.".into());
                }
                Ok(RecipeSaveOutcome::Forbidden) => {
                    context_visible.set(false);
                    status.set("You no longer have access to this workspace.".into())
                }
                Ok(RecipeSaveOutcome::UnresolvedEquipmentProfile) => {
                    retry.set(None);
                    status.set("The selected equipment profile is unavailable.".into())
                }
                Ok(
                    RecipeSaveOutcome::UnresolvedIngredientSource
                    | RecipeSaveOutcome::InvalidProvenance,
                ) => {
                    retry.set(None);
                    status.set("An ingredient or import reference could not be verified.".into());
                }
                Err(e) if crate::utils::auth_refresh::is_auth_error(&e.to_string()) => {
                    context_visible.set(false);
                    status.set(
                        "Your session expired. Sign in again; your draft is still open.".into(),
                    );
                }
                _ => status.set(
                    "Save failed. Retry to confirm the previous save; your draft is still open."
                        .into(),
                ),
            }
            if editor_guard.saving_to_leave.get_untracked() && draft.with_untracked(|d| d.dirty) {
                editor_guard.saving_to_leave.set(false);
            }
        });
    };
    editor_guard
        .save
        .set_value(Some(Callback::new(move |()| save())));
    editor_guard
        .discard
        .set_value(Some(Callback::new(move |()| {
            #[cfg(target_arch = "wasm32")]
            if let Some(key) = recovery_storage_key.get_untracked() {
                if crate::pages::recipe_recovery::remove(&key).is_err() {
                    tracing::warn!("Unable to discard a retained recipe draft");
                    status.set("Could not discard the retained local draft.".into());
                    return false;
                }
                recovery_storage_key.set(None);
            }
            draft.update(|d| d.replace_saved(baseline.get_untracked()));
            retry.set(None);
            true
        })));
    on_cleanup(move || {
        editor_guard.save.set_value(None);
        editor_guard.discard.set_value(None);
    });
    let restore_recovery = Callback::new(move |chosen: String| {
        if let Some((key, record)) = recovery_choice
            .get_untracked()
            .into_iter()
            .find(|(key, _)| *key == chosen)
        {
            let valid_pending = record.pending.as_ref().is_none_or(|p| {
                p.recipe_id == record.scope.recipe_id && p.expected == record.scope.base_revision
            });
            if !valid_pending {
                status.set(
                    "The retained save request is invalid; the draft was not restored.".into(),
                );
                return;
            }
            revision.set(record.scope.base_revision);
            draft.update(|d| d.restore(record.draft));
            let mut attempt = record.pending;
            if let Some(attempt) = &mut attempt {
                let current = draft.with_untracked(|d| d.generation);
                attempt.generation = if attempt.generation == record.generation {
                    current
                } else {
                    current.saturating_sub(1)
                };
            }
            retry.set(attempt);
            recovery_storage_key.set(Some(key));
            recovery_choice.set(Vec::new());
            status.set("Recovered local draft; it has not been saved.".into());
        }
    });
    let discard_recovery = Callback::new(move |key: String| {
        #[cfg(target_arch = "wasm32")]
        if crate::pages::recipe_recovery::remove(&key).is_err() {
            tracing::warn!("Unable to discard a retained recipe draft");
            status.set("Could not remove the retained draft.".into());
            return;
        }
        recovery_choice.update(|records| records.retain(|(candidate, _)| candidate != &key));
    });
    view! { <Show when=move || !context_visible.get()><BreweryNotice title="Recipe held in its original workspace" body="Return to the account and workspace where you opened this draft to continue." warning=true/></Show>
        <Show when=move || context_visible.get() && !recovery_choice.get().is_empty()>
            <section class="recipe-editor"><BreweryNotice title="A retained draft is available" body="Local drafts are not saved recipe revisions. Restore one or discard them before continuing."/>
                {move || recovery_choice.get().into_iter().enumerate().map(|(index, (key, record))| {
                    let restore_key = key.clone(); let discard_key = key;
                    let base = record.scope.base_revision.map(|n| format!("revision {n}")).unwrap_or_else(|| "new recipe".into());
                    let title = if record.draft.title().is_empty() { "Untitled" } else { record.draft.title() };
                    view! { <div><p>{format!("Retained draft {} · {title} · based on {base}", index + 1)}</p><Button on:click=move |_| restore_recovery.run(restore_key.clone())>"Restore draft"</Button><Button variant=crate::components::ButtonVariant::Secondary on:click=move |_| discard_recovery.run(discard_key.clone())>"Discard retained draft"</Button></div> }
                }).collect_view()}
                <Button variant=crate::components::ButtonVariant::Secondary on:click=move |_| recovery_choice.set(Vec::new())>"Start a separate draft"</Button>
                <p role="status">{move || status.get()}</p></section>
        </Show>
        <div class="recipe-editor" hidden=move || !context_visible.get() || !recovery_ready.get() || !recovery_choice.get().is_empty()>
        {move || conflict.get().map(|current| {
            let revision_number = current.revision;
            let remote_for_reload = current.clone();
            let remote_for_reapply = current.document.clone();
            let remote_display = comparison_text(&current.document);
            view! { <section aria-label="Save conflict" class="k-panel">
                <h2>"A newer saved revision is available"</h2>
                <p>{format!("Your draft is based on revision {}; the current saved revision is {}.", revision.get().map(|v| v.to_string()).unwrap_or_else(|| "new".into()), revision_number)}</p>
                <details><summary>"Compare saved revision and your draft"</summary><h3>"Current saved revision"</h3><pre class="overflow-auto">{remote_display}</pre><h3>"Your retained draft"</h3><pre class="overflow-auto">{move || draft.with(|d| comparison_text(d.document()))}</pre><p>{move || draft.with(|d| d.unfinished_inputs().into_iter().map(|(path, text)| format!("{}: {text}", path.replace('_', " "))).collect::<Vec<_>>().join("; "))}</p></details>
                <Button variant=crate::components::ButtonVariant::Secondary on:click=move |_| {
                    #[cfg(target_arch = "wasm32")]
                    if let Some(key) = recovery_storage_key.get_untracked() {
                        if crate::pages::recipe_recovery::remove(&key).is_err() { tracing::warn!("Unable to discard a retained recipe draft"); status.set("Could not discard the retained local draft.".into()); return; }
                        recovery_storage_key.set(None);
                    }
                    baseline.set(remote_for_reload.document.clone()); draft.update(|d| d.replace_saved(remote_for_reload.document.clone())); revision.set(Some(remote_for_reload.revision)); retry.set(None); conflict.set(None); status.set("Loaded the current saved revision; local changes were discarded.".into());
                }>"Discard draft and reload"</Button>
                <Button on:click=move |_| { baseline.set(remote_for_reapply.clone()); draft.update(|d| d.mark_earlier_document_saved(remote_for_reapply.clone())); revision.set(Some(revision_number)); retry.set(None); conflict.set(None); status.set("Draft retained against the current revision. Review it, then choose Save.".into()); }>"Reapply my draft"</Button>
            </section> }
        })}
        <header class="recipe-editor__header"><div><p class="kit-showcase__eyebrow">"Your recipe"</p><h1>{move || draft.with(|d| if d.document().title.is_empty() { "Recipe editor".to_owned() } else { d.document().title.clone() })}</h1><p>"Changes are saved only when you choose Save."</p></div><div class="recipe-editor__actions"><span role="status">{move || {
            let message = status.get();
            if draft.with(|d| d.dirty) && (message.is_empty() || message.starts_with("Saved revision")) { if retained.get() { "Unsaved changes · retained locally".to_owned() } else { "Unsaved changes".to_owned() } } else { message }
        }}</span><Button variant=crate::components::ButtonVariant::Secondary disabled=Signal::derive(move || pending.get() || !draft.with(|d| d.can_undo())) on:click=move |_| draft.update(|d| d.undo())>"Undo"</Button><Button disabled=pending on:click=move |_| save()>"Save recipe"</Button></div></header>
        <div class="recipe-editor__summary">
            <BreweryDraftField id="recipe-title" label="Recipe name" value=title on_change=title_change/>
            <NumberField id="recipe-batch" label="Batch volume" unit="L" path="batch_size_l" draft=draft read=|d: &RecipeDocumentV1| d.batch_size_l write=|d: &mut RecipeDocumentV1,n| d.batch_size_l=n/>
            <NumberField id="recipe-boil" label="Boil time" unit="min" path="boil_minutes" draft=draft read=|d: &RecipeDocumentV1| d.boil_minutes write=|d: &mut RecipeDocumentV1,n| d.boil_minutes=n/>
            {move || { let mut options = vec![(String::new(), "No style".to_owned())]; options.extend(styles.get().into_iter().map(|style| (style.name.clone(), format!("{} · {}", style.style_id.unwrap_or_default(), style.name)))); view! { <BreweryDraftSelect id="recipe-style" label="Beer style" value=selected_style on_change=change_style options=options/> } }}
        </div>
        <div class="recipe-editor__layout"><div class="recipe-editor__sections">
            <crate::pages::beerxml::BeerXmlExport identity=identity context_visible=context_visible recipe_id=recipe_id revision=revision draft=draft/>
            <IngredientEditor draft=draft/>
            <AdditionalIngredients draft=draft/>
            <ProcessSteps draft=draft/>
            <BreweryDisclosure title="Recipe process and notes"><div class="recipe-editor__fields">
                <NumberField id="recipe-attenuation" label="Whole-wort apparent attenuation" unit="%" scale=100.0 path="process.apparent_attenuation_fraction" draft=draft read=|d: &RecipeDocumentV1| d.process.apparent_attenuation_fraction write=|d: &mut RecipeDocumentV1,n| d.process.apparent_attenuation_fraction=n/>
                <BreweryDraftArea id="recipe-description" label="Description" value=description on_change=description_change/>
                <BreweryDraftArea id="recipe-notes" label="Notes" value=notes on_change=notes_change/>
                <BreweryDraftSelect id="recipe-mash-method" label="Mash water method" value=mash_method on_change=mash_method_change options=vec![("".into(), "Not specified".into()), ("manual".into(), "Manual volume".into()), ("profile_thickness".into(), "From mash thickness".into())]/>
                <NumberField id="recipe-mash-water" label="Mash water" unit="L" path="process.mash_water_l" draft=draft read=|d: &RecipeDocumentV1| d.process.mash_water_l write=|d: &mut RecipeDocumentV1,n| d.process.mash_water_l=n/>
                <NumberField id="recipe-top-up" label="Kettle top-up" unit="L" path="process.kettle_top_up_l" draft=draft read=|d: &RecipeDocumentV1| d.process.kettle_top_up_l write=|d: &mut RecipeDocumentV1,n| d.process.kettle_top_up_l=n/>
                <NumberField id="recipe-whirlpool" label="Whirlpool time" unit="min" path="process.whirlpool_minutes" draft=draft read=|d: &RecipeDocumentV1| d.process.whirlpool_minutes write=|d: &mut RecipeDocumentV1,n| d.process.whirlpool_minutes=n/>
                <NumberField id="recipe-whirlpool-temp" label="Whirlpool temperature" unit="°C" path="process.whirlpool_temperature_c" draft=draft read=|d: &RecipeDocumentV1| d.process.whirlpool_temperature_c write=|d: &mut RecipeDocumentV1,n| d.process.whirlpool_temperature_c=n/>
            </div></BreweryDisclosure>
            <BreweryDisclosure title="Equipment settings">
                <crate::pages::equipment_profiles::EquipmentProfiles draft=draft identity=identity context_visible=context_visible refresh_preview=profile_generation/>
                <div class="recipe-editor__fields">
                <NumberField id="recipe-efficiency-override" label="Brewhouse efficiency override" unit="%" scale=100.0 path="equipment_overrides.efficiency_override_fraction" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.efficiency_override_fraction write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.efficiency_override_fraction=n/>
                <NumberField id="recipe-efficiency" label="Mash efficiency" unit="%" scale=100.0 path="equipment_overrides.mash_efficiency_fraction" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.mash_efficiency_fraction write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.mash_efficiency_fraction=n/>
                <NumberField id="recipe-boil-off" label="Boil-off" unit="L/h" path="equipment_overrides.boil_off_l_per_hour" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.boil_off_l_per_hour write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.boil_off_l_per_hour=n/>
                <NumberField id="recipe-loss" label="Transfer loss" unit="L" path="equipment_overrides.transfer_loss_l" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.transfer_loss_l write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.transfer_loss_l=n/>
                <NumberField id="recipe-deadspace" label="Pre-boil deadspace" unit="L" path="equipment_overrides.pre_boil_deadspace_l" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.pre_boil_deadspace_l write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.pre_boil_deadspace_l=n/>
                <NumberField id="recipe-absorption" label="Grain absorption" unit="L/kg" path="equipment_overrides.grain_absorption_l_per_kg" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.grain_absorption_l_per_kg write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.grain_absorption_l_per_kg=n/>
                <NumberField id="recipe-thickness" label="Mash thickness" unit="L/kg" path="equipment_overrides.mash_thickness_l_per_kg" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.mash_thickness_l_per_kg write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.mash_thickness_l_per_kg=n/>
                <NumberField id="recipe-capacity" label="Kettle capacity" unit="L" path="equipment_overrides.kettle_capacity_l" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.kettle_capacity_l write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.kettle_capacity_l=n/>
                <NumberField id="recipe-diameter" label="Kettle diameter" unit="cm" path="equipment_overrides.kettle_diameter_cm" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.kettle_diameter_cm write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.kettle_diameter_cm=n/>
                <NumberField id="recipe-opening" label="Kettle opening" unit="cm" path="equipment_overrides.kettle_opening_cm" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.kettle_opening_cm write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.kettle_opening_cm=n/>
                <NumberField id="recipe-boil-temp" label="Boil temperature" unit="°C" path="equipment_overrides.boil_temperature_c" draft=draft read=|d: &RecipeDocumentV1| d.equipment_overrides.boil_temperature_c write=|d: &mut RecipeDocumentV1,n| d.equipment_overrides.boil_temperature_c=n/>
                <BreweryDraftSelect id="recipe-chilling" label="Chilling" value=chilling on_change=chilling_change options=vec![("".into(), "Not specified".into()), ("natural".into(), "Natural".into()), ("forced".into(), "Forced".into()), ("counterflow".into(), "Counterflow".into())]/>
                <Show when=move || chilling.get() == "forced"><NumberField id="recipe-chilling-rate" label="Cooling rate" unit="per min" path="equipment_overrides.chilling.rate_per_minute" draft=draft read=|d: &RecipeDocumentV1| match &d.equipment_overrides.chilling { Some(krausen_types::brewery::ChillingV1::Forced{rate_per_minute,..}) => *rate_per_minute, _ => None } write=|d: &mut RecipeDocumentV1,n| if let Some(krausen_types::brewery::ChillingV1::Forced{rate_per_minute,..})=&mut d.equipment_overrides.chilling { *rate_per_minute=n }/><NumberField id="recipe-chilling-plateau" label="Cooling plateau" unit="°C" path="equipment_overrides.chilling.plateau_c" draft=draft read=|d: &RecipeDocumentV1| match &d.equipment_overrides.chilling { Some(krausen_types::brewery::ChillingV1::Forced{plateau_c,..}) => *plateau_c, _ => None } write=|d: &mut RecipeDocumentV1,n| if let Some(krausen_types::brewery::ChillingV1::Forced{plateau_c,..})=&mut d.equipment_overrides.chilling { *plateau_c=n }/></Show>
                <Show when=move || chilling.get() == "counterflow"><NumberField id="recipe-chilling-flow" label="Counterflow rate" unit="L/min" path="equipment_overrides.chilling.litres_per_minute" draft=draft read=|d: &RecipeDocumentV1| match &d.equipment_overrides.chilling { Some(krausen_types::brewery::ChillingV1::Counterflow{litres_per_minute}) => *litres_per_minute, _ => None } write=|d: &mut RecipeDocumentV1,n| if let Some(krausen_types::brewery::ChillingV1::Counterflow{litres_per_minute})=&mut d.equipment_overrides.chilling { *litres_per_minute=n }/></Show>
            </div></BreweryDisclosure>
        </div><aside class="recipe-editor__preview"><FormulationPreview draft=draft identity=identity context_visible=context_visible gate=preview_gate profile_generation=profile_generation/></aside></div>
    </div> }
}

#[component]
fn NumberField(
    id: &'static str,
    label: &'static str,
    unit: &'static str,
    path: &'static str,
    draft: RwSignal<RecipeDraft>,
    #[prop(default = 1.0)] scale: f64,
    read: fn(&RecipeDocumentV1) -> Option<f64>,
    write: fn(&mut RecipeDocumentV1, Option<f64>),
) -> impl IntoView {
    let value = Signal::derive(move || {
        draft.with(|d| d.number_text(path, read(d.document()).map(|v| v * scale)))
    });
    let error = Signal::derive(move || draft.with(|d| d.number_error(path).map(str::to_owned)));
    let change = Callback::new(move |text| {
        draft.update(|d| d.edit_number(path, text, |doc, n| write(doc, n.map(|v| v / scale))))
    });
    view! { <BreweryDraftField id=id label=label unit=unit value=value error=error on_change=change numeric=true/> }
}

fn diagnostic_message(diagnostic: &krausen_types::validation::Diagnostic) -> String {
    let subject = if diagnostic.path.starts_with("fermentables[") {
        "Fermentable"
    } else if diagnostic.path.starts_with("hops[") {
        "Hop addition"
    } else if diagnostic.path.starts_with("yeast[") {
        "Yeast"
    } else if diagnostic.path.starts_with("ingredients[") {
        "Ingredient"
    } else if diagnostic.path.starts_with("process.") {
        "Process"
    } else if diagnostic.path.starts_with("equipment") {
        "Equipment"
    } else {
        "Recipe"
    };
    let message = match diagnostic.code.as_str() {
        "missing" => "needs more information",
        "unfinished_quantity" => "needs a quantity",
        "negative" => "contains a negative value",
        "out_of_range" => "contains a value outside its allowed range",
        "non_finite" => "contains an invalid number",
        "invalid_row_id" => "has an invalid addition",
        _ => "needs attention",
    };
    format!("{subject} {message}.")
}

#[derive(Clone, Copy)]
enum IngredientKind {
    Fermentable,
    Hop,
}
impl IngredientKind {
    fn prefix(self) -> &'static str {
        match self {
            Self::Fermentable => "fermentables",
            Self::Hop => "hops",
        }
    }
}
#[component]
fn IngredientEditor(draft: RwSignal<RecipeDraft>) -> impl IntoView {
    let add_grain = move |_| {
        let Some(id) = crate::utils::request_id::new_request_id() else {
            return;
        };
        draft.update(|d| {
            d.edit(|doc| {
                doc.fermentables
                    .push(krausen_types::brewery::FermentableAddition {
                        row_id: id,
                        amount_kg: None,
                        addition: krausen_types::brewery::AdditionStage {
                            stage: String::new(),
                            time_minutes: None,
                        },
                        specification: krausen_types::brewery::FermentableSpecification {
                            name: None,
                            producer: None,
                            kind: None,
                            yield_fraction: None,
                            colour_ebc: None,
                        },
                        source: None,
                    })
            })
        });
    };
    let add_hop = move |_| {
        let Some(id) = crate::utils::request_id::new_request_id() else {
            return;
        };
        draft.update(|d| {
            d.edit(|doc| {
                doc.hops.push(krausen_types::brewery::HopAddition {
                    row_id: id,
                    amount_g: None,
                    stage: String::new(),
                    contact_minutes: None,
                    temperature_c: None,
                    specification: krausen_types::brewery::HopSpecification {
                        name: None,
                        producer: None,
                        alpha_fraction: None,
                    },
                    source: None,
                })
            })
        });
    };
    view! {
        <BreweryPanel title="Fermentables" description="Specify the grain, extract or sugar copied into this recipe.">
            <For each=move || draft.with(|d| d.document().fermentables.iter().map(|r| r.row_id.clone()).collect::<Vec<_>>()) key=|id| id.clone() children=move |id| view! { <IngredientFields draft=draft id=id kind=IngredientKind::Fermentable/> } />
            <Button variant=crate::components::ButtonVariant::Secondary on:click=add_grain>"Add fermentable"</Button>
        </BreweryPanel>
        <BreweryPanel title="Hops" description="Record each addition and its contact time.">
            <For each=move || draft.with(|d| d.document().hops.iter().map(|r| r.row_id.clone()).collect::<Vec<_>>()) key=|id| id.clone() children=move |id| view! { <IngredientFields draft=draft id=id kind=IngredientKind::Hop/> } />
            <Button variant=crate::components::ButtonVariant::Secondary on:click=add_hop>"Add hop"</Button>
        </BreweryPanel>
    }
}

#[derive(Clone, Copy)]
enum IngredientText {
    Name,
    Kind,
    Stage,
}
#[derive(Clone, Copy)]
enum IngredientNumber {
    Amount,
    YieldOrAlpha,
    ColourOrTime,
    StageDetail,
}

#[component]
fn IngredientFields(
    draft: RwSignal<RecipeDraft>,
    id: String,
    kind: IngredientKind,
) -> impl IntoView {
    let remove_id = id.clone();
    let remove = move |_| {
        let prefix = format!("{}[{remove_id}].", kind.prefix());
        draft.update(|d| {
            d.edit_clearing(&prefix, |doc| match kind {
                IngredientKind::Fermentable => doc.fermentables.retain(|r| r.row_id != remove_id),
                IngredientKind::Hop => doc.hops.retain(|r| r.row_id != remove_id),
            });
        });
    };
    let name_id = id.clone();
    let type_id = id.clone();
    let stage_id = id.clone();
    let quantity_id = id.clone();
    let measure_id = id.clone();
    let extra_id = id.clone();
    let stage_detail_id = id.clone();
    view! { <BreweryAddition>
        <div class="recipe-editor__fields"><IngredientTextField draft=draft id=name_id kind=kind field=IngredientText::Name label="Name"/>
        <IngredientNumberField draft=draft id=quantity_id kind=kind field=IngredientNumber::Amount label="Amount" unit=if matches!(kind, IngredientKind::Hop) {"g"} else {"kg"}/></div>
        <BreweryDisclosure title="Addition details"><div class="recipe-editor__fields">
            {matches!(kind, IngredientKind::Fermentable).then(|| view! { <IngredientTextField draft=draft id=type_id kind=kind field=IngredientText::Kind label="Type"/> })}
            <IngredientTextField draft=draft id=stage_id kind=kind field=IngredientText::Stage label="Stage"/>
            <IngredientNumberField draft=draft id=measure_id kind=kind field=IngredientNumber::YieldOrAlpha label=if matches!(kind, IngredientKind::Hop) {"Alpha acid"} else {"Yield"} unit="%"/>
            <IngredientNumberField draft=draft id=extra_id kind=kind field=IngredientNumber::ColourOrTime label=if matches!(kind, IngredientKind::Hop) {"Contact time"} else {"Colour"} unit=if matches!(kind, IngredientKind::Hop) {"min"} else {"EBC"}/>
            <IngredientNumberField draft=draft id=stage_detail_id kind=kind field=IngredientNumber::StageDetail label=if matches!(kind, IngredientKind::Hop) {"Whirlpool temperature"} else {"Boil addition time"} unit=if matches!(kind, IngredientKind::Hop) {"°C"} else {"min"}/>
        </div></BreweryDisclosure>
        <Button variant=crate::components::ButtonVariant::GhostDestructive on:click=remove>"Remove addition"</Button>
    </BreweryAddition> }
}

fn ingredient_text(
    doc: &RecipeDocumentV1,
    id: &str,
    kind: IngredientKind,
    field: IngredientText,
) -> String {
    match kind {
        IngredientKind::Fermentable => {
            doc.fermentables
                .iter()
                .find(|r| r.row_id == id)
                .map(|r| match field {
                    IngredientText::Name => r.specification.name.clone().unwrap_or_default(),
                    IngredientText::Kind => r.specification.kind.clone().unwrap_or_default(),
                    IngredientText::Stage => r.addition.stage.clone(),
                })
        }
        IngredientKind::Hop => doc
            .hops
            .iter()
            .find(|r| r.row_id == id)
            .map(|r| match field {
                IngredientText::Name => r.specification.name.clone().unwrap_or_default(),
                IngredientText::Stage => r.stage.clone(),
                IngredientText::Kind => String::new(),
            }),
    }
    .unwrap_or_default()
}
fn set_ingredient_text(
    doc: &mut RecipeDocumentV1,
    id: &str,
    kind: IngredientKind,
    field: IngredientText,
    value: String,
) {
    let optional = || {
        if value.is_empty() {
            None
        } else {
            Some(value.clone())
        }
    };
    match kind {
        IngredientKind::Fermentable => {
            if let Some(r) = doc.fermentables.iter_mut().find(|r| r.row_id == id) {
                match field {
                    IngredientText::Name => r.specification.name = optional(),
                    IngredientText::Kind => r.specification.kind = optional(),
                    IngredientText::Stage => r.addition.stage = value,
                }
            }
        }
        IngredientKind::Hop => {
            if let Some(r) = doc.hops.iter_mut().find(|r| r.row_id == id) {
                match field {
                    IngredientText::Name => r.specification.name = optional(),
                    IngredientText::Stage => r.stage = value,
                    IngredientText::Kind => (),
                }
            }
        }
    }
}
#[component]
fn IngredientTextField(
    draft: RwSignal<RecipeDraft>,
    id: String,
    kind: IngredientKind,
    field: IngredientText,
    label: &'static str,
) -> impl IntoView {
    let input_id = format!(
        "{}-{id}-{}",
        kind.prefix(),
        match field {
            IngredientText::Name => "name",
            IngredientText::Kind => "type",
            IngredientText::Stage => "stage",
        }
    );
    let read_id = id.clone();
    let value = Signal::derive(move || {
        draft.with(|d| ingredient_text(d.document(), &read_id, kind, field))
    });
    let change = Callback::new(move |text| {
        let id = id.clone();
        draft.update(|d| d.edit(|doc| set_ingredient_text(doc, &id, kind, field, text)));
    });
    match field {
        IngredientText::Name => view! { <BreweryDraftField id=input_id label=label value=value on_change=change/> }.into_any(),
        IngredientText::Kind => view! { <BreweryDraftSelect id=input_id label="Type" value=value on_change=change options=vec![("".into(), "Select a type".into()), ("grain".into(), "Grain".into()), ("extract".into(), "Extract".into()), ("sugar".into(), "Sugar".into())]/> }.into_any(),
        IngredientText::Stage => {
            let options = match kind { IngredientKind::Fermentable => vec![("".into(), "Select a stage".into()), ("mash".into(), "Mash".into()), ("boil".into(), "Boil".into())], IngredientKind::Hop => vec![("".into(), "Select a stage".into()), ("boil".into(), "Boil".into()), ("whirlpool".into(), "Whirlpool".into()), ("dry_hop".into(), "Dry hop".into())] };
            view! { <BreweryDraftSelect id=input_id label="Stage" value=value on_change=change options=options/> }.into_any()
        }
    }
}
fn ingredient_number(
    doc: &RecipeDocumentV1,
    id: &str,
    kind: IngredientKind,
    field: IngredientNumber,
) -> Option<f64> {
    match kind {
        IngredientKind::Fermentable => {
            doc.fermentables
                .iter()
                .find(|r| r.row_id == id)
                .and_then(|r| match field {
                    IngredientNumber::Amount => r.amount_kg,
                    IngredientNumber::YieldOrAlpha => r.specification.yield_fraction,
                    IngredientNumber::ColourOrTime => r.specification.colour_ebc,
                    IngredientNumber::StageDetail => r.addition.time_minutes,
                })
        }
        IngredientKind::Hop => doc
            .hops
            .iter()
            .find(|r| r.row_id == id)
            .and_then(|r| match field {
                IngredientNumber::Amount => r.amount_g,
                IngredientNumber::YieldOrAlpha => r.specification.alpha_fraction,
                IngredientNumber::ColourOrTime => r.contact_minutes,
                IngredientNumber::StageDetail => r.temperature_c,
            }),
    }
}
fn set_ingredient_number(
    doc: &mut RecipeDocumentV1,
    id: &str,
    kind: IngredientKind,
    field: IngredientNumber,
    value: Option<f64>,
) {
    match kind {
        IngredientKind::Fermentable => {
            if let Some(r) = doc.fermentables.iter_mut().find(|r| r.row_id == id) {
                match field {
                    IngredientNumber::Amount => r.amount_kg = value,
                    IngredientNumber::YieldOrAlpha => r.specification.yield_fraction = value,
                    IngredientNumber::ColourOrTime => r.specification.colour_ebc = value,
                    IngredientNumber::StageDetail => r.addition.time_minutes = value,
                }
            }
        }
        IngredientKind::Hop => {
            if let Some(r) = doc.hops.iter_mut().find(|r| r.row_id == id) {
                match field {
                    IngredientNumber::Amount => r.amount_g = value,
                    IngredientNumber::YieldOrAlpha => r.specification.alpha_fraction = value,
                    IngredientNumber::ColourOrTime => r.contact_minutes = value,
                    IngredientNumber::StageDetail => r.temperature_c = value,
                }
            }
        }
    }
}
#[component]
fn IngredientNumberField(
    draft: RwSignal<RecipeDraft>,
    id: String,
    kind: IngredientKind,
    field: IngredientNumber,
    label: &'static str,
    unit: &'static str,
) -> impl IntoView {
    let path = format!(
        "{}[{id}].{}",
        kind.prefix(),
        match (kind, field) {
            (IngredientKind::Fermentable, IngredientNumber::Amount) => "amount_kg",
            (IngredientKind::Fermentable, IngredientNumber::YieldOrAlpha) =>
                "specification.yield_fraction",
            (IngredientKind::Fermentable, IngredientNumber::ColourOrTime) =>
                "specification.colour_ebc",
            (IngredientKind::Fermentable, IngredientNumber::StageDetail) => "addition.time_minutes",
            (IngredientKind::Hop, IngredientNumber::Amount) => "amount_g",
            (IngredientKind::Hop, IngredientNumber::YieldOrAlpha) => "specification.alpha_fraction",
            (IngredientKind::Hop, IngredientNumber::ColourOrTime) => "contact_minutes",
            (IngredientKind::Hop, IngredientNumber::StageDetail) => "temperature_c",
        }
    );
    let input_id = format!(
        "{}-{id}-{}",
        kind.prefix(),
        match field {
            IngredientNumber::Amount => "amount",
            IngredientNumber::YieldOrAlpha => "yield-alpha",
            IngredientNumber::ColourOrTime => "colour-time",
            IngredientNumber::StageDetail => "stage-detail",
        }
    );
    let read_path = path.clone();
    let read_id = id.clone();
    let error_path = path.clone();
    let value = Signal::derive(move || {
        draft.with(|d| {
            d.number_text(
                &read_path,
                ingredient_number(d.document(), &read_id, kind, field).map(|n| {
                    if matches!(field, IngredientNumber::YieldOrAlpha) {
                        n * 100.0
                    } else {
                        n
                    }
                }),
            )
        })
    });
    let error =
        Signal::derive(move || draft.with(|d| d.number_error(&error_path).map(str::to_owned)));
    let change = Callback::new(move |text| {
        let id = id.clone();
        let path = path.clone();
        draft.update(|d| {
            d.edit_number(&path, text, |doc, n| {
                set_ingredient_number(
                    doc,
                    &id,
                    kind,
                    field,
                    n.map(|v| {
                        if matches!(field, IngredientNumber::YieldOrAlpha) {
                            v / 100.0
                        } else {
                            v
                        }
                    }),
                )
            })
        });
    });
    view! { <BreweryDraftField id=input_id label=label unit=unit value=value error=error on_change=change numeric=true/> }
}

#[derive(Clone, Copy)]
enum AdditionalKind {
    Yeast,
    Other,
}
impl AdditionalKind {
    fn prefix(self) -> &'static str {
        match self {
            Self::Yeast => "yeast",
            Self::Other => "other_ingredients",
        }
    }
    fn rows(self, doc: &RecipeDocumentV1) -> &Vec<krausen_types::brewery::IngredientAddition> {
        match self {
            Self::Yeast => &doc.yeast,
            Self::Other => &doc.other_ingredients,
        }
    }
    fn rows_mut(
        self,
        doc: &mut RecipeDocumentV1,
    ) -> &mut Vec<krausen_types::brewery::IngredientAddition> {
        match self {
            Self::Yeast => &mut doc.yeast,
            Self::Other => &mut doc.other_ingredients,
        }
    }
}
#[component]
fn AdditionalIngredients(draft: RwSignal<RecipeDraft>) -> impl IntoView {
    view! { <AdditionalGroup draft=draft kind=AdditionalKind::Yeast/><AdditionalGroup draft=draft kind=AdditionalKind::Other/> }
}
#[component]
fn AdditionalGroup(draft: RwSignal<RecipeDraft>, kind: AdditionalKind) -> impl IntoView {
    let add = move |_| {
        let Some(id) = crate::utils::request_id::new_request_id() else {
            return;
        };
        draft.update(|d| {
            d.edit(|doc| {
                kind.rows_mut(doc)
                    .push(krausen_types::brewery::IngredientAddition {
                        row_id: id,
                        amount: None,
                        unit: None,
                        source: None,
                        specification: krausen_types::brewery::IngredientSpecification {
                            name: None,
                            producer: None,
                            attenuation_fraction: None,
                            extensions: Default::default(),
                        },
                    })
            })
        });
    };
    view! { <BreweryPanel title=if matches!(kind, AdditionalKind::Yeast) { "Yeast" } else { "Other ingredients" }>
        <For each=move || draft.with(|d| kind.rows(d.document()).iter().map(|r| r.row_id.clone()).collect::<Vec<_>>()) key=|id| id.clone() children=move |id| view! { <AdditionalFields draft=draft id=id kind=kind/> } />
        <Button variant=crate::components::ButtonVariant::Secondary on:click=add>"Add ingredient"</Button>
    </BreweryPanel> }
}
#[derive(Clone, Copy)]
enum AdditionalText {
    Name,
    Producer,
    Unit,
}
#[component]
fn AdditionalFields(
    draft: RwSignal<RecipeDraft>,
    id: String,
    kind: AdditionalKind,
) -> impl IntoView {
    let remove_id = id.clone();
    let remove = move |_| {
        let prefix = format!("{}[{remove_id}].", kind.prefix());
        draft.update(|d| {
            d.edit_clearing(&prefix, |doc| {
                kind.rows_mut(doc).retain(|r| r.row_id != remove_id)
            });
        });
    };
    let name_id = id.clone();
    let producer_id = id.clone();
    let unit_id = id.clone();
    let amount_id = id.clone();
    let attenuation_id = id.clone();
    view! { <BreweryAddition>
        <AdditionalTextField draft=draft id=name_id kind=kind field=AdditionalText::Name label="Name"/>
        <AdditionalTextField draft=draft id=producer_id kind=kind field=AdditionalText::Producer label="Producer"/>
        <AdditionalTextField draft=draft id=unit_id kind=kind field=AdditionalText::Unit label="Unit"/>
        <AdditionalNumberField draft=draft id=amount_id kind=kind attenuation=false/>
        {matches!(kind, AdditionalKind::Yeast).then(|| view! { <AdditionalNumberField draft=draft id=attenuation_id kind=kind attenuation=true/> })}
        <Button variant=crate::components::ButtonVariant::GhostDestructive on:click=remove>"Remove ingredient"</Button>
    </BreweryAddition> }
}
#[component]
fn AdditionalTextField(
    draft: RwSignal<RecipeDraft>,
    id: String,
    kind: AdditionalKind,
    field: AdditionalText,
    label: &'static str,
) -> impl IntoView {
    let suffix = match field {
        AdditionalText::Name => "name",
        AdditionalText::Producer => "producer",
        AdditionalText::Unit => "unit",
    };
    let input_id = format!("{}-{id}-{suffix}", kind.prefix());
    let read_id = id.clone();
    let value = Signal::derive(move || {
        draft.with(|d| {
            kind.rows(d.document())
                .iter()
                .find(|r| r.row_id == read_id)
                .and_then(|r| match field {
                    AdditionalText::Name => r.specification.name.clone(),
                    AdditionalText::Producer => r.specification.producer.clone(),
                    AdditionalText::Unit => r.unit.clone(),
                })
                .unwrap_or_default()
        })
    });
    let change = Callback::new(move |text: String| {
        let id = id.clone();
        draft.update(|d| {
            d.edit(|doc| {
                if let Some(r) = kind.rows_mut(doc).iter_mut().find(|r| r.row_id == id) {
                    let value = (!text.is_empty()).then_some(text);
                    match field {
                        AdditionalText::Name => r.specification.name = value,
                        AdditionalText::Producer => r.specification.producer = value,
                        AdditionalText::Unit => r.unit = value,
                    }
                }
            })
        });
    });
    view! { <BreweryDraftField id=input_id label=label value=value on_change=change/> }
}
#[component]
fn AdditionalNumberField(
    draft: RwSignal<RecipeDraft>,
    id: String,
    kind: AdditionalKind,
    attenuation: bool,
) -> impl IntoView {
    let suffix = if attenuation { "attenuation" } else { "amount" };
    let path = format!("{}[{id}].{suffix}", kind.prefix());
    let input_id = format!("{}-{id}-{suffix}", kind.prefix());
    let read_id = id.clone();
    let read_path = path.clone();
    let error_path = path.clone();
    let diagnostic_path = if attenuation {
        format!("yeast[{id}].specification.attenuation_fraction")
    } else {
        format!("ingredients[{id}].amount")
    };
    let value = Signal::derive(move || {
        draft.with(|d| {
            let n = kind
                .rows(d.document())
                .iter()
                .find(|r| r.row_id == read_id)
                .and_then(|r| {
                    if attenuation {
                        r.specification.attenuation_fraction
                    } else {
                        r.amount
                    }
                });
            d.number_text(
                &read_path,
                n.map(|v| if attenuation { v * 100.0 } else { v }),
            )
        })
    });
    let error = Signal::derive(move || {
        draft.with(|d| {
            d.number_error(&error_path)
                .or_else(|| d.validation_error(&diagnostic_path))
                .map(str::to_owned)
        })
    });
    let change = Callback::new(move |text| {
        let id = id.clone();
        let path = path.clone();
        draft.update(|d| {
            d.edit_number(&path, text, |doc, n| {
                if let Some(r) = kind.rows_mut(doc).iter_mut().find(|r| r.row_id == id) {
                    if attenuation {
                        r.specification.attenuation_fraction = n.map(|v| v / 100.0)
                    } else {
                        r.amount = n
                    }
                }
            })
        });
    });
    if attenuation {
        view! { <BreweryDraftField id=input_id label="Yeast attenuation (informational)" unit="%" value=value error=error on_change=change numeric=true/> }.into_any()
    } else {
        view! { <BreweryDraftField id=input_id label="Amount" value=value error=error on_change=change numeric=true/> }.into_any()
    }
}

#[derive(Clone, Copy)]
enum StepKind {
    Mash,
    Fermentation,
}
impl StepKind {
    fn prefix(self) -> &'static str {
        match self {
            Self::Mash => "process.mash_steps",
            Self::Fermentation => "process.fermentation_steps",
        }
    }
    fn rows(self, doc: &RecipeDocumentV1) -> &Vec<krausen_types::brewery::ProcessStep> {
        match self {
            Self::Mash => &doc.process.mash_steps,
            Self::Fermentation => &doc.process.fermentation_steps,
        }
    }
    fn rows_mut(self, doc: &mut RecipeDocumentV1) -> &mut Vec<krausen_types::brewery::ProcessStep> {
        match self {
            Self::Mash => &mut doc.process.mash_steps,
            Self::Fermentation => &mut doc.process.fermentation_steps,
        }
    }
}
#[component]
fn ProcessSteps(draft: RwSignal<RecipeDraft>) -> impl IntoView {
    view! { <StepGroup draft=draft kind=StepKind::Mash/><StepGroup draft=draft kind=StepKind::Fermentation/> }
}
#[component]
fn StepGroup(draft: RwSignal<RecipeDraft>, kind: StepKind) -> impl IntoView {
    let add = move |_| {
        let Some(id) = crate::utils::request_id::new_request_id() else {
            return;
        };
        draft.update(|d| {
            d.edit(|doc| {
                kind.rows_mut(doc)
                    .push(krausen_types::brewery::ProcessStep {
                        row_id: id,
                        name: None,
                        time_minutes: None,
                        temperature_c: None,
                        volume_l: None,
                        extensions: Default::default(),
                    })
            })
        });
    };
    view! { <BreweryPanel title=if matches!(kind, StepKind::Mash) { "Mash steps" } else { "Fermentation steps" }>
        <For each=move || draft.with(|d| kind.rows(d.document()).iter().map(|r| r.row_id.clone()).collect::<Vec<_>>()) key=|id| id.clone() children=move |id| view! { <StepFields draft=draft id=id kind=kind/> } />
        <Button variant=crate::components::ButtonVariant::Secondary on:click=add>"Add step"</Button>
    </BreweryPanel> }
}
#[derive(Clone, Copy)]
enum StepNumber {
    Time,
    Temperature,
    Volume,
}
#[component]
fn StepFields(draft: RwSignal<RecipeDraft>, id: String, kind: StepKind) -> impl IntoView {
    let remove_id = id.clone();
    let remove = move |_| {
        let prefix = format!("{}[{remove_id}].", kind.prefix());
        draft.update(|d| {
            d.edit_clearing(&prefix, |doc| {
                kind.rows_mut(doc).retain(|r| r.row_id != remove_id)
            });
        });
    };
    let read_id = id.clone();
    let change_id = id.clone();
    let name_id = format!("{}-{id}-name", kind.prefix());
    let name = Signal::derive(move || {
        draft.with(|d| {
            kind.rows(d.document())
                .iter()
                .find(|r| r.row_id == read_id)
                .and_then(|r| r.name.clone())
                .unwrap_or_default()
        })
    });
    let change = Callback::new(move |text: String| {
        let id = change_id.clone();
        draft.update(|d| {
            d.edit(|doc| {
                if let Some(r) = kind.rows_mut(doc).iter_mut().find(|r| r.row_id == id) {
                    r.name = (!text.is_empty()).then_some(text)
                }
            })
        });
    });
    let time_id = id.clone();
    let temp_id = id.clone();
    let volume_id = id.clone();
    view! { <BreweryAddition><BreweryDraftField id=name_id label="Step name" value=name on_change=change/>
        <StepNumberField draft=draft id=time_id kind=kind field=StepNumber::Time label="Time" unit="min"/>
        <StepNumberField draft=draft id=temp_id kind=kind field=StepNumber::Temperature label="Temperature" unit="°C"/>
        <StepNumberField draft=draft id=volume_id kind=kind field=StepNumber::Volume label="Volume" unit="L"/>
        <Button variant=crate::components::ButtonVariant::GhostDestructive on:click=remove>"Remove step"</Button>
    </BreweryAddition> }
}
#[component]
fn StepNumberField(
    draft: RwSignal<RecipeDraft>,
    id: String,
    kind: StepKind,
    field: StepNumber,
    label: &'static str,
    unit: &'static str,
) -> impl IntoView {
    let suffix = match field {
        StepNumber::Time => "time_minutes",
        StepNumber::Temperature => "temperature_c",
        StepNumber::Volume => "volume_l",
    };
    let path = format!("{}[{id}].{suffix}", kind.prefix());
    let input_id = format!("{}-{id}-{suffix}", kind.prefix());
    let read_id = id.clone();
    let read_path = path.clone();
    let error_path = path.clone();
    let value = Signal::derive(move || {
        draft.with(|d| {
            let n = kind
                .rows(d.document())
                .iter()
                .find(|r| r.row_id == read_id)
                .and_then(|r| match field {
                    StepNumber::Time => r.time_minutes,
                    StepNumber::Temperature => r.temperature_c,
                    StepNumber::Volume => r.volume_l,
                });
            d.number_text(&read_path, n)
        })
    });
    let error =
        Signal::derive(move || draft.with(|d| d.number_error(&error_path).map(str::to_owned)));
    let change = Callback::new(move |text| {
        let id = id.clone();
        let path = path.clone();
        draft.update(|d| {
            d.edit_number(&path, text, |doc, n| {
                if let Some(r) = kind.rows_mut(doc).iter_mut().find(|r| r.row_id == id) {
                    match field {
                        StepNumber::Time => r.time_minutes = n,
                        StepNumber::Temperature => r.temperature_c = n,
                        StepNumber::Volume => r.volume_l = n,
                    }
                }
            })
        });
    });
    view! { <BreweryDraftField id=input_id label=label unit=unit value=value error=error on_change=change numeric=true/> }
}

#[component]
fn FormulationPreview(
    draft: RwSignal<RecipeDraft>,
    identity: RwSignal<EditorIdentity>,
    context_visible: RwSignal<bool>,
    gate: StoredValue<crate::pages::recipe_preview_gate::PreviewGate>,
    profile_generation: RwSignal<u64>,
) -> impl IntoView {
    use krausen_types::formulation::{Preview, PreviewOutcome, PreviewRequest};
    let result = RwSignal::new(None::<(u64, Preview)>);
    let message = RwSignal::new("Awaiting preview.".to_owned());
    let message_generation = RwSignal::new(0_u64);
    let trigger = Memo::new(move |_| {
        (
            draft.with(|d| d.generation),
            context_visible.get(),
            profile_generation.get(),
        )
    });
    Effect::new(move |_| {
        let (draft_generation, visible, _) = trigger.get();
        let mut generation = 0;
        gate.update_value(|g| generation = g.invalidate());
        result.set(None);
        message_generation.set(draft_generation);
        if !visible {
            return;
        }
        let Ok(document) = draft.with_untracked(|d| d.snapshot()) else {
            message.set("Complete invalid number fields to see estimates.".into());
            return;
        };
        message.set("Calculating…".into());
        let expected = identity.get_untracked();
        let request = PreviewRequest {
            draft_generation: generation.to_string(),
            intent: expected.clone(),
            draft: document,
            equipment_draft: Default::default(),
        };
        leptos::task::spawn_local(async move {
            #[cfg(target_arch = "wasm32")]
            gloo_timers::future::TimeoutFuture::new(250).await;
            if gate.try_with_value(|g| g.is_current(generation)) != Some(true)
                || context_visible.try_get_untracked() != Some(true)
                || draft.try_with_untracked(|d| d.generation) != Some(draft_generation)
            {
                return;
            }
            if !current_context_matches(&expected).await {
                if gate.try_with_value(|g| g.is_current(generation)) == Some(true) {
                    context_visible.set(false);
                }
                return;
            }
            if gate.try_with_value(|g| g.is_current(generation)) != Some(true)
                || context_visible.try_get_untracked() != Some(true)
                || draft.try_with_untracked(|d| d.generation) != Some(draft_generation)
            {
                return;
            }
            let response =
                crate::server_fns::formulation::preview_formulation_shared(request.clone()).await;
            #[cfg(target_arch = "wasm32")]
            let response = if response
                .as_ref()
                .err()
                .is_some_and(|e| crate::utils::auth_refresh::is_auth_error(&e.to_string()))
                && crate::utils::auth_refresh::try_refresh().await
            {
                if !current_context_matches(&expected).await {
                    context_visible.set(false);
                    return;
                }
                if gate.try_with_value(|g| g.is_current(generation)) != Some(true)
                    || context_visible.try_get_untracked() != Some(true)
                    || draft.try_with_untracked(|d| d.generation) != Some(draft_generation)
                {
                    return;
                }
                crate::server_fns::formulation::preview_formulation_shared(request.clone()).await
            } else {
                response
            };
            if gate.try_with_value(|g| g.is_current(generation)) != Some(true)
                || context_visible.try_get_untracked() != Some(true)
                || draft.try_with_untracked(|d| d.generation) != Some(draft_generation)
            {
                return;
            }
            match response {
                Ok(PreviewOutcome::Preview(preview))
                    if preview.draft_generation == request.draft_generation =>
                {
                    let context_matches = preview.context.as_ref().is_some_and(|context| {
                        context.workspace_id == expected.workspace_id
                            && context.draft == request.draft
                            && match &request.draft.equipment_profile_id {
                                Some(id) => {
                                    context.equipment_profile.as_ref().is_some_and(|profile| {
                                        profile.workspace_id == expected.workspace_id
                                            && profile.profile_id == *id
                                            && !profile.archived
                                            && context.equipment_draft == profile.settings
                                            && context.equipment_provenance == "saved_profile"
                                    })
                                }
                                None => {
                                    context.equipment_profile.is_none()
                                        && context.equipment_draft == request.equipment_draft
                                }
                            }
                    });
                    let mut accepted = false;
                    if context_matches {
                        if let Some(digest) = preview.context_digest.as_deref() {
                            gate.update_value(|g| accepted = g.accept(generation, digest, None));
                        }
                    }
                    if accepted {
                        message.set(match preview.validation.state {
                            krausen_types::validation::Readiness::Ready => "Ready".into(),
                            krausen_types::validation::Readiness::Incomplete => "Incomplete".into(),
                            krausen_types::validation::Readiness::Invalid => "Invalid".into(),
                        });
                        draft.update(|d| d.set_validation(preview.validation.clone()));
                        result.set(Some((draft_generation, preview)));
                    } else if preview.validation.state
                        == krausen_types::validation::Readiness::Invalid
                        && preview.context.is_none()
                        && preview.context_digest.is_none()
                    {
                        draft.update(|d| d.set_validation(preview.validation));
                        message.set("Invalid: correct the highlighted inputs.".into());
                    } else {
                        message.set("Preview context changed. Try again.".into());
                    }
                }
                Ok(PreviewOutcome::UnresolvedProfile) => message.set(
                    "Equipment profile unavailable. Use draft settings or select a valid profile."
                        .into(),
                ),
                Ok(PreviewOutcome::UnresolvedStyle) => message.set(
                    "Style reference unavailable. Choose a supported style or clear it.".into(),
                ),
                Ok(PreviewOutcome::ContextChanged | PreviewOutcome::Forbidden) => {
                    context_visible.set(false)
                }
                Ok(PreviewOutcome::Invalid) => {
                    message.set("Invalid draft. Correct the inputs before previewing.".into())
                }
                Err(e) if crate::utils::auth_refresh::is_auth_error(&e.to_string()) => {
                    context_visible.set(false)
                }
                _ => message.set("Preview unavailable. Your draft remains open.".into()),
            }
        });
    });
    view! { <BreweryPanel title="Your formulation" description="Estimates use the current draft.">
        <p role="status">{move || if draft.with(|d| d.generation) == message_generation.get() { message.get() } else { "Awaiting preview.".into() }}</p>
        {move || { let current = draft.with(|d| d.generation); result.get().filter(|(generation, preview)| *generation == current && context_visible.get() && gate.try_with_value(|g| g.accepted_context() == preview.context_digest.as_deref()).unwrap_or(false)).map(|(_, preview)| view! { <PreviewMetrics preview=preview/> }) }}
        <div role="status">{move || draft.with(|d| d.diagnostics().iter().map(|diagnostic| view! { <p>{diagnostic_message(diagnostic)}</p> }).collect_view())}</div>
    </BreweryPanel> }
}

#[component]
fn PreviewMetrics(preview: krausen_types::formulation::Preview) -> impl IntoView {
    let style = preview.style.as_ref();
    let comparison = preview.style_comparison.as_ref();
    view! { <div class="recipe-editor__metrics">
        {preview_metric("Original gravity", preview.targets.original_sg, style.map(|s| &s.original_sg), comparison.and_then(|c| c.original_sg), "SG", 3)}
        {preview_metric("Final gravity", preview.targets.final_sg, style.map(|s| &s.final_sg), comparison.and_then(|c| c.final_sg), "SG", 3)}
        {preview_metric("Alcohol", preview.targets.abv_percent, style.map(|s| &s.abv_percent), comparison.and_then(|c| c.abv_percent), "%", 1)}
        {preview_metric("Bitterness", preview.targets.ibu, style.map(|s| &s.ibu), comparison.and_then(|c| c.ibu), "IBU", 1)}
        {preview_metric("Colour", preview.targets.colour_ebc, style.map(|s| &s.colour_ebc), comparison.and_then(|c| c.colour_ebc), "EBC", 1)}
        {preview_metric("Total water", preview.targets.total_water_l, None, None, "L", 1)}
    </div>
    <BreweryDisclosure title="Water and gravity details"><div class="recipe-editor__metrics">
        {preview_metric("Mash water", preview.targets.mash_water_l, None, None, "L", 1)}
        {preview_metric("Sparge water", preview.targets.sparge_water_l, None, None, "L", 1)}
        {preview_metric("Kettle water", preview.targets.kettle_water_l, None, None, "L", 1)}
        {preview_metric("Grain absorption", preview.targets.grain_absorption_l, None, None, "L", 1)}
        {preview_metric("Pre-boil volume", preview.targets.pre_boil_l, None, None, "L", 1)}
        {preview_metric("Post-boil volume", preview.targets.post_boil_l, None, None, "L", 1)}
        {preview_metric("Pre-boil gravity", preview.targets.pre_boil_sg, None, None, "SG", 3)}
    </div></BreweryDisclosure>
    {preview.style.as_ref().map(|style| view! { <p class="recipe-editor__provenance">{format!("Style reference: {} · {} · {}", style.name, style.style_id, style.edition)}</p> })}
    {preview.context.as_ref().map(|context| {
        let assumptions = context.assumptions.iter().filter_map(|value| match value.as_str() {
            "volume_reference_20c" => Some("Volumes are referenced to 20 °C."),
            "solid_addition_volume_negligible" => Some("Solid additions have negligible volume."),
            "kettle_top_up_default_zero" => Some("No kettle top-up is assumed."),
            "whirlpool_duration_default_zero" => Some("No whirlpool duration is assumed."),
            _ => None,
        }).collect::<Vec<_>>();
        view! { <BreweryDisclosure title="Assumptions"><ul>{assumptions.into_iter().map(|text| view! { <li>{text}</li> }).collect_view()}</ul></BreweryDisclosure> }
    })}
    }
}
fn preview_metric(
    label: &'static str,
    value: Option<f64>,
    range: Option<&krausen_types::formulation::Range>,
    comparison: Option<krausen_types::formulation::RangeStatus>,
    unit: &'static str,
    digits: usize,
) -> AnyView {
    use krausen_types::formulation::RangeStatus;
    let formatted = value
        .map(|n| format!("{n:.digits$}"))
        .unwrap_or_else(|| "—".into());
    let range_view = range.map(|r| {
        let state = match comparison { Some(RangeStatus::Below) => crate::components::StyleState::Below, Some(RangeStatus::Within) => crate::components::StyleState::Within, Some(RangeStatus::Above) => crate::components::StyleState::Above, None => crate::components::StyleState::Awaiting };
        view! { <crate::components::StyleRange label=label min=r.min max=r.max value=value state=state unit=unit digits=digits/> }
    });
    view! { <crate::components::BreweryMetric label=label value=formatted unit=unit>{range_view}</crate::components::BreweryMetric> }.into_any()
}
