// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recipe editor over the shared, authorized recipe operations.
use crate::{
    components::{
        BreweryAddition, BreweryDisclosure, BreweryDraftArea, BreweryDraftField,
        BreweryDraftSelect, BreweryNotice, BreweryPanel, BreweryTabs, Button, ButtonLink,
        ButtonVariant, SelectedSavedRevision, printable_recipe::diagnostic_message,
    },
    pages::recipe_draft::RecipeDraft,
    utils::nav_state::{RecipeSection, current_location, parse_revision, recipe_section_href},
    pages::recipe_scaling::{DuplicateDialog, ScaleDialog},
    server_fns::recipe_editor::{read_recipe, save_recipe},
};
use krausen_types::{
    brewery::RecipeDocumentV1,
    process_summary::{
        ProcessSummary, ProcessSummaryOutcome, ProcessSummaryRequest, ProcessSummarySelection,
    },
    recipe_editor::{RecipeReadOutcome, RecipeSaveOutcome, RecipeSnapshot},
};
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map, use_query_map};

#[path = "recipe_comparison.rs"]
mod recipe_comparison;

type EditorIdentity = krausen_types::request_context::OperationIntent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditorAccess {
    Verified,
    Checking,
    NetworkUnavailable,
    Expired,
    Mismatch,
    Forbidden,
}

#[derive(Clone, Copy)]
struct ContextCheckGeneration(RwSignal<u64>);

/// Existing editor authority presentation, shared with protected child tasks.
#[derive(Clone, Copy)]
pub(crate) struct EditorAuthorityHold {
    pub(crate) generation: RwSignal<u64>,
    pub(crate) on_failure: Callback<(u64, EditorAccess)>,
}

impl EditorAccess {
    fn title(self) -> &'static str {
        match self {
            Self::Verified => "Access verified",
            Self::Checking => "Checking access…",
            Self::NetworkUnavailable => "Network unavailable",
            Self::Expired => "Session expired",
            Self::Mismatch => "Recipe held in its original workspace",
            Self::Forbidden => "Access denied",
        }
    }
    fn guidance(self) -> &'static str {
        match self {
            Self::Verified => "",
            Self::Checking => "Your draft is hidden while access is checked.",
            Self::NetworkUnavailable => "Unable to check access. Your draft is retained in this page; retry when the connection is available.",
            Self::Expired => "Sign in again in another tab, then retry the check here. Keep this page open to retain your work.",
            Self::Mismatch => "Return to the account and workspace where you opened this draft, then retry the check.",
            Self::Forbidden => "Access to this workspace was denied. Keep this page open and reconnect with an authorized account before retrying.",
        }
    }
}

async fn check_editor_access(expected: &EditorIdentity) -> EditorAccess {
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
    match result {
        Ok(ctx) if ctx.user_id == expected.account_id
            && ctx.workspace_id.as_deref() == Some(expected.workspace_id.as_str()) => EditorAccess::Verified,
        Ok(_) => EditorAccess::Mismatch,
        Err(error) if crate::utils::auth_refresh::is_auth_error(&error.to_string()) => EditorAccess::Expired,
        Err(_) => EditorAccess::NetworkUnavailable,
    }
}

pub(crate) async fn current_context_matches(expected: &EditorIdentity) -> bool {
    check_editor_access(expected).await == EditorAccess::Verified
}

fn hold_editor(access: RwSignal<EditorAccess>, visible: RwSignal<bool>, outcome: EditorAccess) {
    if access.try_get_untracked().is_some() && visible.try_get_untracked().is_some() {
        access.set(outcome);
        visible.set(false);
    }
}

fn same_leave_destination(
    original: &Option<crate::components::layout::EditorDestination>,
    current: &Option<crate::components::layout::EditorDestination>,
) -> bool {
    use crate::components::layout::EditorDestination;
    match (original, current) {
        (Some(EditorDestination::Href(a)), Some(EditorDestination::Href(b))) => a == b,
        (Some(EditorDestination::Workspace(a)), Some(EditorDestination::Workspace(b))) => a == b,
        (Some(EditorDestination::Logout), Some(EditorDestination::Logout)) => true,
        (Some(EditorDestination::History { key: a, attempt: x }), Some(EditorDestination::History { key: b, attempt: y })) => a == b && x == y,
        _ => false,
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
struct EditorFocus {
    id: String,
    print_return: bool,
    selection: Option<(u32, u32, String)>,
}

#[cfg(target_arch = "wasm32")]
impl EditorFocus {
    fn capture() -> Option<Self> {
        let element = web_sys::window()?.document()?.active_element()?;
        let id = element.id();
        let print_return = element.closest("[aria-label='Print preview']").ok().flatten().is_some();
        // Input and textarea share these DOM properties; use the native API
        // without changing value or synthesizing input/Undo events.
        let property = |name: &str| js_sys::Reflect::get(element.as_ref(), &name.into()).ok();
        let selection = property("selectionStart").and_then(|v| v.as_f64())
            .zip(property("selectionEnd").and_then(|v| v.as_f64()))
            .map(|(start, end)| (start as u32, end as u32,
                property("selectionDirection").and_then(|v| v.as_string()).unwrap_or_else(|| "none".into())));
        Some(Self { id, print_return, selection })
    }
    fn restore(self) {
        use wasm_bindgen::JsCast;
        let Some(document) = web_sys::window().and_then(|w| w.document()) else { return; };
        // A deliberate focus move while the asynchronous check was pending
        // takes precedence over our old bookmark.
        if document.active_element().is_some_and(|active| active.tag_name() != "BODY" && active.tag_name() != "HTML") { return; }
        let try_focus = |element: &web_sys::Element| {
            if element.closest("[hidden], [inert], :disabled").ok().flatten().is_some()
                || element.get_client_rects().length() == 0
                || web_sys::window().and_then(|w| w.get_computed_style(element).ok().flatten())
                    .and_then(|style| style.get_property_value("visibility").ok()).is_some_and(|v| v == "hidden") { return false; }
            element.dyn_ref::<web_sys::HtmlElement>().is_some_and(|target| target.focus().is_ok())
                && document.active_element().is_some_and(|active| active == *element)
        };
        let print = document.query_selector("[aria-label='Print preview']").ok().flatten();
        let original = document.get_element_by_id(&self.id)
            .filter(|_| print.is_none() || self.print_return)
            .filter(|element| try_focus(element));
        if let Some(element) = original {
          if let Some((start, end, direction)) = self.selection {
            if let Some(set_range) = js_sys::Reflect::get(element.as_ref(), &"setSelectionRange".into()).ok().and_then(|v| v.dyn_into::<js_sys::Function>().ok()) {
                if set_range.call3(element.as_ref(), &start.into(), &end.into(), &direction.into()).is_err() { tracing::warn!("Unable to restore editor selection"); }
            }
          }
          return;
        }
        // Never send focus behind a still-active print task. If that task was
        // cleared by confirmed denial, return to the existing editor heading.
        if let Some(print) = print {
            if let Some(button) = print.query_selector("button").ok().flatten() { try_focus(&button); }
        } else if let Some(heading) = document.get_element_by_id("recipe-editor-heading") {
            try_focus(&heading);
        }
    }
}

fn comparison_changes(result: Result<Vec<recipe_comparison::RecipeChange>, serde_json::Error>) -> AnyView {
    comparison_changes_from(result, "Original", "No changes from the original saved revision.")
}

fn comparison_changes_from(result: Result<Vec<recipe_comparison::RecipeChange>, serde_json::Error>, before_label: &'static str, empty: &'static str) -> AnyView {
    match result {
        Ok(changes) if changes.is_empty() => view! { <p>{empty}</p> }.into_any(),
        Ok(changes) => view! { <ul>{changes.into_iter().map(|change| view! { <li><strong>{change.label}</strong><p>{format!("{before_label}: {}", change.before)}</p><p>{format!("Changed to: {}", change.after)}</p></li> }).collect_view()}</ul> }.into_any(),
        Err(error) => {
            tracing::warn!(%error, "Unable to prepare recipe comparison");
            view! { <p>"Comparison is unavailable. Your complete original draft is still retained; no changes were merged."</p> }.into_any()
        }
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
            Err(error) if crate::utils::auth_refresh::is_auth_error(&error.to_string()) => view! { <BreweryNotice title="Session expired" body=EditorAccess::Expired.guidance() warning=true/><Button variant=ButtonVariant::Secondary on:click=move |_| load.refetch()>"Retry"</Button> }.into_any(),
            _ => view! { <BreweryNotice title="Unable to load recipe" body="Your recipe could not be loaded. Retry when the connection is available." warning=true/><Button variant=ButtonVariant::Secondary on:click=move |_| load.refetch()>"Retry"</Button> }.into_any(),
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
    let access = RwSignal::new(EditorAccess::Verified);
    let check_generation = RwSignal::new(0_u64);
    provide_context(access);
    provide_context(ContextCheckGeneration(check_generation));
    let context_visible = RwSignal::new(true);
    let context_lost = RwSignal::new(false);
    let equipment_summary = RwSignal::new(crate::pages::recipe_equipment::EquipmentSummary::default());
    let preview_gate = StoredValue::new(crate::pages::recipe_preview_gate::PreviewGate::default());
    let history_authority_failure = Callback::new(move |()| {
        if context_visible.try_get_untracked().is_none() { return; }
        context_visible.set(false);
        access.set(EditorAccess::Forbidden);
        context_lost.set(true);
        preview_gate.update_value(|gate| { gate.invalidate(); });
    });
    provide_context(EditorAuthorityHold {
        generation: check_generation,
        on_failure: Callback::new(move |(epoch, outcome)| {
            if check_generation.try_get_untracked() != Some(epoch) { return; }
            if outcome == EditorAccess::Forbidden {
                history_authority_failure.run(());
            } else {
                hold_editor(access, context_visible, outcome);
                if outcome == EditorAccess::Mismatch {
                    context_lost.set(true);
                    preview_gate.update_value(|gate| { gate.invalidate(); });
                }
            }
        }),
    });
    let recipe_id = RwSignal::new(initial.as_ref().map(|s| s.recipe_id.clone()));
    let revision = RwSignal::new(initial.as_ref().map(|s| s.revision));
    let original_baseline_available = RwSignal::new(true);
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
    let focus_before_check = StoredValue::new(None::<EditorFocus>);
    let recheck = Callback::new(move |()| {
        #[cfg(target_arch = "wasm32")]
        if context_visible.get_untracked() {
            focus_before_check.set_value(EditorFocus::capture());
        }
        access.set(EditorAccess::Checking);
        context_visible.set(false);
        context_lost.set(false);
        preview_gate.update_value(|gate| { gate.invalidate(); });
        let generation = check_generation.get_untracked().checked_add(1).expect("context check generation exhausted");
        check_generation.set(generation);
        let expected = identity.get_untracked();
        leptos::task::spawn_local(async move {
            let mut outcome = check_editor_access(&expected).await;
            if outcome == EditorAccess::Verified {
                outcome = match crate::server_fns::formulation::list_formulation_styles(expected.account_id.clone(), expected.workspace_id.clone()).await {
                    Ok(krausen_types::formulation::StyleListOutcome::Styles(_)) => EditorAccess::Verified,
                    Ok(krausen_types::formulation::StyleListOutcome::Forbidden) => EditorAccess::Forbidden,
                    Ok(krausen_types::formulation::StyleListOutcome::ContextChanged) => EditorAccess::Mismatch,
                    Ok(krausen_types::formulation::StyleListOutcome::Unavailable) => EditorAccess::NetworkUnavailable,
                    Err(error) if crate::utils::auth_refresh::is_auth_error(&error.to_string()) => EditorAccess::Expired,
                    Err(_) => EditorAccess::NetworkUnavailable,
                };
            }
            if check_generation.try_get_untracked() != Some(generation) { return; }
            access.set(outcome);
            if outcome == EditorAccess::Verified {
                context_visible.set(true);
                #[cfg(target_arch = "wasm32")]
                request_animation_frame(move || {
                    if context_visible.try_get_untracked() == Some(true)
                        && check_generation.try_get_untracked() == Some(generation) {
                        if let Some(focus) = focus_before_check.get_value() { focus.restore(); }
                    }
                });
            } else {
                context_lost.set(true);
            }
        });
    });
    // Legacy children still expose only a fail-closed visibility signal. Check
    // the original scope through existing authorized calls rather than inventing
    // a mismatch from that boolean. Explicit typed denials never take this path.
    Effect::new(move |_| {
        if !context_visible.get() && access.get() == EditorAccess::Verified {
            recheck.run(());
        }
    });
    #[cfg(target_arch = "wasm32")]
    {
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
    let save_sequence = RwSignal::new(0_u64);
    let retry = RwSignal::new(None::<PendingRecipeSave>);
    let conflict = RwSignal::new(None::<RecipeSnapshot>);
    let print_preview = RwSignal::new(None::<ProcessSummary>);
    let scale_open = RwSignal::new(false);
    let duplicate_open = RwSignal::new(false);
    let scale_uncertain = RwSignal::new(false);
    let duplicate_uncertain = RwSignal::new(false);
    let on_scaled = Callback::new(
        move |(document, revision_number): (RecipeDocumentV1, i64)| {
            baseline.set(document.clone());
            original_baseline_available.set(true);
            draft.update(|draft| draft.replace_saved(document));
            revision.set(Some(revision_number));
            retry.set(None);
            conflict.set(None);
            status.set(format!(
                "Scaled recipe saved as revision {revision_number}."
            ));
        },
    );
    let editor_guard = expect_context::<crate::components::layout::EditorGuard>();
    use crate::components::layout::{GuardTaskKind, GuardScope, GuardTaskState, GuardTaskCallbacks, GuardLeaveAttempt, GuardSaveResolution};
    let saving_to_leave = RwSignal::new(false);
    let leave_attempt = StoredValue::new(None::<GuardLeaveAttempt>);
    let guard_save = StoredValue::new(None::<Callback<GuardLeaveAttempt>>);
    let guard_discard = StoredValue::new(None::<Callback<(), bool>>);
    let source = identity.get_untracked();
    let guard_task = editor_guard.register_task(GuardTaskKind::Recipe, GuardScope {
        installation: GuardScope::installation(), account_id: source.account_id,
        workspace_id: source.workspace_id, resource_id: recipe_id.get_untracked(), base_revision: revision.get_untracked(),
    }, GuardTaskCallbacks {
        save_and_leave: Some(Callback::new(move |attempt| { if let Some(save) = guard_save.get_value() { save.run(attempt); } })),
        return_to_actions: None,
        discard_unsent: Callback::new(move |()| guard_discard.get_value().is_some_and(|discard| discard.run(()))),
    });
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
                if editor_guard.should_warn_unload(&event) {
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
    Effect::new(move |_| editor_guard.message.set(if context_visible.get() { status.get() } else { access.get().guidance().to_owned() }));
    Effect::new(move |_| {
        if matches!(access.get(), EditorAccess::Mismatch | EditorAccess::Forbidden) {
            saving_to_leave.set(false);
        }
    });
    Effect::new(move |_| {
        guard_task.update(GuardTaskState {
            dirty: draft.with(|d| d.dirty), generation: draft.with(|d| d.generation),
            in_flight: pending.get(), uncertain: retry.get().is_some() || scale_uncertain.get() || duplicate_uncertain.get(),
            recovery_retained: false, context_visible: context_visible.get(),
        });
    });
    on_cleanup(move || guard_task.unregister());
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
        let checking_recovery = RwSignal::new(None::<u64>);
        Effect::new(move |_| {
            let epoch = check_generation.get();
            if recovery_ready.get() || !context_visible.get() || checking_recovery.get() == Some(epoch) {
                return;
            }
            if initial_scope.installation.is_empty() {
                status.set("Local draft recovery is unavailable in this browser.".into());
                recovery_ready.set(true);
                return;
            }
            checking_recovery.set(Some(epoch));
            let expected = expected.clone();
            let initial_scope = initial_scope.clone();
            leptos::task::spawn_local(async move {
                let owns_task = move || checking_recovery.try_get_untracked() == Some(Some(epoch))
                    && check_generation.try_get_untracked() == Some(epoch)
                    && recovery_ready.try_get_untracked() == Some(false)
                    && context_visible.try_get_untracked() == Some(true);
                let retire = move || {
                    if checking_recovery.try_get_untracked() == Some(Some(epoch)) {
                        checking_recovery.set(None);
                    }
                };
                let outcome = check_editor_access(&expected).await;
                if !owns_task() { retire(); return; }
                if outcome != EditorAccess::Verified {
                    hold_editor(access, context_visible, outcome);
                    retire();
                    return;
                }
                let authorized = crate::server_fns::formulation::list_formulation_styles(
                    expected.account_id.clone(),
                    expected.workspace_id.clone(),
                )
                .await;
                if !owns_task() { retire(); return; }
                let outcome = match authorized {
                    Ok(krausen_types::formulation::StyleListOutcome::Styles(_)) => check_editor_access(&expected).await,
                    Ok(krausen_types::formulation::StyleListOutcome::Forbidden) => EditorAccess::Forbidden,
                    Ok(krausen_types::formulation::StyleListOutcome::ContextChanged) => EditorAccess::Mismatch,
                    Ok(krausen_types::formulation::StyleListOutcome::Unavailable) => EditorAccess::NetworkUnavailable,
                    Err(error) if crate::utils::auth_refresh::is_auth_error(&error.to_string()) => EditorAccess::Expired,
                    Err(_) => EditorAccess::NetworkUnavailable,
                };
                if !owns_task() { retire(); return; }
                if outcome != EditorAccess::Verified {
                    hold_editor(access, context_visible, outcome);
                    retire();
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
                retire();
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
            let epoch = check_generation.get_untracked();
            leptos::task::spawn_local(async move {
                let outcome = check_editor_access(&expected).await;
                if check_generation.try_get_untracked() != Some(epoch) { return; }
                if outcome != EditorAccess::Verified {
                    hold_editor(access, context_visible, outcome);
                    return;
                }
                let result = read_recipe(
                    expected.account_id.clone(),
                    expected.workspace_id.clone(),
                    id.clone(),
                )
                .await;
                let outcome = check_editor_access(&expected).await;
                if recipe_id.try_get_untracked() != Some(Some(id))
                    || revision.try_get_untracked() != Some(Some(observed))
                    || pending.try_get_untracked() != Some(false)
                    || check_generation.try_get_untracked() != Some(epoch)
                {
                    return;
                }
                if outcome != EditorAccess::Verified {
                    hold_editor(access, context_visible, outcome);
                    return;
                }
                if matches!(&result, Ok(RecipeReadOutcome::Forbidden)) {
                    hold_editor(access, context_visible, EditorAccess::Forbidden);
                    return;
                }
                if matches!(&result, Ok(RecipeReadOutcome::ContextChanged)) {
                    hold_editor(access, context_visible, EditorAccess::Mismatch);
                    return;
                }
                if let Err(error) = &result {
                    let outcome = if crate::utils::auth_refresh::is_auth_error(&error.to_string()) { EditorAccess::Expired } else { EditorAccess::NetworkUnavailable };
                    hold_editor(access, context_visible, outcome);
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
                            original_baseline_available.set(true);
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
                Ok(krausen_types::formulation::StyleListOutcome::ContextChanged) => {
                    access.set(EditorAccess::Mismatch);
                    context_visible.set(false);
                }
                Ok(krausen_types::formulation::StyleListOutcome::Forbidden) => {
                    access.set(EditorAccess::Forbidden);
                    context_visible.set(false);
                }
                Err(e) if crate::utils::auth_refresh::is_auth_error(&e.to_string()) => {
                    access.set(EditorAccess::Expired);
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

    let save = move || {
        let identity_signal = identity;
        if pending.get_untracked()
            || !context_visible.get_untracked()
            || !recovery_ready.get_untracked()
            || !recovery_choice.get_untracked().is_empty()
        {
            saving_to_leave.set(false);
            return;
        }
        let attempt = if let Some(attempt) = retry.get_untracked() {
            attempt
        } else {
            draft.update(|d| d.settle_numbers());
            let snapshot = draft.with_untracked(|d| (d.snapshot(), d.generation));
            let Ok(document) = snapshot.0 else {
                status.set("Complete invalid number fields before saving.".into());
                saving_to_leave.set(false);
                return;
            };
            let Some(request_id) = crate::utils::request_id::new_request_id() else {
                status.set("Unable to create a secure save request.".into());
                saving_to_leave.set(false);
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
        let submitted_generation = attempt.generation;
        pending.set(true);
        save_sequence.update(|sequence| *sequence = sequence.checked_add(1).expect("save sequence exhausted"));
        let save_task = save_sequence.get_untracked();
        let save_epoch = check_generation.get_untracked();
        status.set("Saving…".into());
        let leave_destination = if saving_to_leave.get_untracked() {
            editor_guard.destination.get_untracked()
        } else { None };
        leptos::task::spawn_local(async move {
            #[cfg(target_arch = "wasm32")]
            let creating = attempt.recipe_id.is_none();
            let identity = identity.get_untracked();
            let outcome = check_editor_access(&identity).await;
            if pending.try_get_untracked().is_none() {
                return;
            }
            if check_generation.try_get_untracked() != Some(save_epoch)
                || context_visible.try_get_untracked() != Some(true)
                || outcome != EditorAccess::Verified {
                if check_generation.try_get_untracked() == Some(save_epoch) && outcome != EditorAccess::Verified {
                    hold_editor(access, context_visible, outcome);
                }
                pending.set(false);
                saving_to_leave.set(false);
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
                if pending.try_get_untracked().is_none() { return; }
                let outcome = check_editor_access(&identity).await;
                if pending.try_get_untracked().is_none() { return; }
                if check_generation.try_get_untracked() != Some(save_epoch)
                    || context_visible.try_get_untracked() != Some(true)
                    || outcome != EditorAccess::Verified {
                    if check_generation.try_get_untracked() == Some(save_epoch) && outcome != EditorAccess::Verified {
                        hold_editor(access, context_visible, outcome);
                    }
                    pending.set(false);
                    saving_to_leave.set(false);
                    return;
                }
                save_recipe(
                    identity.account_id.clone(),
                    identity.workspace_id.clone(),
                    attempt.recipe_id.clone(),
                    attempt.expected,
                    attempt.document.clone(),
                    attempt.request_id.clone(),
                )
                .await
            } else {
                result
            };
            if pending.try_get_untracked().is_none() {
                return;
            }
            // A known commit still belongs to its original scope. Its receipt
            // does not authorize an address change or a cancelled leave action.
            let mut may_apply_scope = false;
            if matches!(&result, Ok(RecipeSaveOutcome::Saved { .. }))
                && context_visible.try_get_untracked() == Some(true)
                && access.try_get_untracked() == Some(EditorAccess::Verified) {
                let Some(epoch) = check_generation.try_get_untracked() else { return; };
                let outcome = check_editor_access(&identity).await;
                if pending.try_get_untracked().is_none() { return; }
                if check_generation.try_get_untracked() == Some(epoch) {
                    if outcome != EditorAccess::Verified { hold_editor(access, context_visible, outcome); }
                    may_apply_scope = outcome == EditorAccess::Verified
                        && context_visible.try_get_untracked() == Some(true)
                        && access.try_get_untracked() == Some(EditorAccess::Verified)
                        && identity_signal.try_with_untracked(|current| current.account_id == identity.account_id && current.workspace_id == identity.workspace_id) == Some(true);
                }
            }
            let may_leave = may_apply_scope
                && saving_to_leave.get_untracked()
                && same_leave_destination(&leave_destination, &editor_guard.destination.get_untracked());
            if !may_leave { saving_to_leave.set(false); }
            pending.set(false);
            if !matches!(&result, Ok(RecipeSaveOutcome::Saved { .. })) {
                saving_to_leave.set(false);
            }
            match result {
                Ok(RecipeSaveOutcome::Saved {
                    recipe_id: id,
                    revision: saved,
                }) => {
                    retry.set(None);
                    baseline.set(attempt.document.clone());
                    original_baseline_available.set(true);
                    #[cfg(target_arch = "wasm32")]
                    if creating && may_apply_scope {
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
                        status.set(if may_apply_scope { format!("Saved revision {saved}.") } else { format!("Saved revision {saved} in the original context. Recheck access before continuing; no leave action was performed.") });
                        #[cfg(target_arch = "wasm32")]
                        let local_cleared = may_apply_scope && recovery_storage_key
                            .get_untracked()
                            .is_none_or(|key| crate::pages::recipe_recovery::remove(&key).is_ok());
                        #[cfg(not(target_arch = "wasm32"))]
                        let local_cleared = may_apply_scope;
                        if local_cleared {
                            recovery_storage_key.set(None);
                            if may_leave && saving_to_leave.get_untracked() {
                                guard_task.update(GuardTaskState { dirty: false, generation: draft.with_untracked(|d| d.generation), in_flight: false, uncertain: false, recovery_retained: false, context_visible: true });
                                if let Some(leave_token) = leave_attempt.get_value() {
                                    guard_task.complete_save(leave_token, GuardSaveResolution::Saved { submitted_generation });
                                }
                            }
                        } else if may_apply_scope {
                            tracing::warn!("Unable to clear the retained draft after saving");
                            saving_to_leave.set(false);
                            status.set(format!("Saved revision {saved}, but the retained local draft could not be cleared."));
                        }
                    } else {
                        saving_to_leave.set(false);
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
                    if let Some(id) = attempt.recipe_id.clone() {
                        let expected = identity.clone();
                        let epoch = check_generation.get_untracked();
                        let observed_revision = revision.get_untracked();
                        let observed_draft = draft.with_untracked(|d| d.generation);
                        let guard_id = id.clone();
                        let guard_identity = expected.clone();
                        let authority_current = move || save_sequence.try_get_untracked() == Some(save_task)
                            && check_generation.try_get_untracked() == Some(epoch)
                            && context_visible.try_get_untracked() == Some(true)
                            && pending.try_get_untracked() == Some(false)
                            && recipe_id.try_get_untracked() == Some(Some(guard_id.clone()))
                            && identity_signal.try_with_untracked(|current| current.account_id == guard_identity.account_id && current.workspace_id == guard_identity.workspace_id) == Some(true);
                        let content_current = {
                            let authority_current = authority_current.clone();
                            move || authority_current()
                                && revision.try_get_untracked() == Some(observed_revision)
                                && draft.try_with_untracked(|d| d.generation) == Some(observed_draft)
                        };
                        if !authority_current() { return; }
                        let outcome = check_editor_access(&expected).await;
                        if !authority_current() { return; }
                        if outcome == EditorAccess::Verified {
                            let response = read_recipe(
                                expected.account_id.clone(),
                                expected.workspace_id.clone(),
                                id,
                            )
                            .await;
                            if !authority_current() { return; }
                            let outcome = check_editor_access(&expected).await;
                            if !authority_current() { return; }
                            if outcome != EditorAccess::Verified {
                                hold_editor(access, context_visible, outcome);
                                return;
                            }
                            match response {
                                Ok(RecipeReadOutcome::Found(snapshot)) => {
                                    if !content_current() { return; }
                                    conflict.set(Some(snapshot));
                                    #[cfg(target_arch = "wasm32")]
                                    request_animation_frame(move || {
                                        use wasm_bindgen::JsCast;
                                        if content_current() {
                                            if let Some(target) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id("recipe-save-conflict")).and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok()) {
                                                if target.focus().is_err() { tracing::warn!("Unable to focus the save conflict response"); }
                                            }
                                        }
                                    });
                                }
                                Ok(RecipeReadOutcome::Forbidden) => {
                                    access.set(EditorAccess::Forbidden);
                                    context_visible.set(false);
                                }
                                Ok(RecipeReadOutcome::ContextChanged) => {
                                    access.set(EditorAccess::Mismatch);
                                    context_visible.set(false);
                                }
                                Err(error) if crate::utils::auth_refresh::is_auth_error(&error.to_string()) => hold_editor(access, context_visible, EditorAccess::Expired),
                                _ => status.set("The current saved revision could not be loaded. Your draft is unchanged.".into()),
                            }
                        } else {
                            hold_editor(access, context_visible, outcome);
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
                    access.set(EditorAccess::Mismatch);
                    context_visible.set(false);
                    status.set("Your account or workspace changed. The draft is held in its original context.".into());
                }
                Ok(RecipeSaveOutcome::Forbidden) => {
                    access.set(EditorAccess::Forbidden);
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
                    access.set(EditorAccess::Expired);
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
            if saving_to_leave.get_untracked() && draft.with_untracked(|d| d.dirty) {
                saving_to_leave.set(false);
            }
        });
    };
    guard_save.set_value(Some(Callback::new(move |attempt| {
        leave_attempt.set_value(Some(attempt)); saving_to_leave.set(true); save();
    })));
    guard_discard
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
            // Publish the reset generation before discard_active captures its
            // departure lease; the reactive mirror runs after this callback.
            guard_task.update(GuardTaskState {
                dirty: draft.with_untracked(|d| d.dirty),
                generation: draft.with_untracked(|d| d.generation),
                in_flight: pending.get_untracked(),
                uncertain: retry.get_untracked().is_some()
                    || scale_uncertain.get_untracked()
                    || duplicate_uncertain.get_untracked(),
                recovery_retained: false,
                context_visible: context_visible.get_untracked(),
            });
            true
        })));
    on_cleanup(move || {
        guard_save.set_value(None);
        guard_discard.set_value(None);
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
            original_baseline_available.set(original_baseline_available.get_untracked() && revision.get_untracked() == record.scope.base_revision);
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
    Effect::new(move |_| {
        if matches!(access.get(), EditorAccess::Forbidden | EditorAccess::Mismatch) {
            print_preview.set(None);
        }
    });
    let close_print = Callback::new(move |()| print_preview.set(None));
    // Section and inspected saved version live in additive URL state (see
    // utils::nav_state); the editable draft is never replaced by either.
    let route_params = use_params_map();
    let query = use_query_map();
    let navigate = use_navigate();
    let initial_section = query.with_untracked(|q| RecipeSection::parse(q.get("section").as_deref()));
    let active_tab = RwSignal::new(initial_section.key().to_owned());
    let selected_revision = RwSignal::new(if initial_section == RecipeSection::Versions {
        query.with_untracked(|q| parse_revision(q.get("revision").as_deref()))
    } else {
        None
    });
    provide_context(SelectedSavedRevision(selected_revision));
    Effect::new(move |_| {
        let (section, revision) = query.with(|q| {
            let section = RecipeSection::parse(q.get("section").as_deref());
            let revision = (section == RecipeSection::Versions)
                .then(|| parse_revision(q.get("revision").as_deref()))
                .flatten();
            (section, revision)
        });
        if active_tab.get_untracked() != section.key() {
            active_tab.set(section.key().to_owned());
        }
        if selected_revision.get_untracked() != revision {
            selected_revision.set(revision);
        }
    });
    Effect::new(move |_| {
        let section = RecipeSection::parse(Some(active_tab.get().as_str()));
        let revision = selected_revision.get();
        let id = route_params.with(|p| p.get("id").unwrap_or_default());
        if id.is_empty() || id == "new" {
            return;
        }
        let revision = (section == RecipeSection::Versions).then_some(revision).flatten();
        let target = recipe_section_href(&id, section, revision);
        let current = leptos_router::hooks::use_location();
        let here = current_location(&current.pathname.get_untracked(), &current.search.get_untracked());
        let here_path = here.split('?').next().unwrap_or_default().to_owned();
        if here_path == target.split('?').next().unwrap_or_default() && here != target {
            // Replace, never push: an in-page section/version change must not add
            // entries that dirty-draft history guards would have to arbitrate.
            navigate(&target, leptos_router::NavigateOptions { replace: true, ..Default::default() });
        }
    });
    view! { <Show when=move || !context_visible.get()>
        {move || view! { <BreweryNotice title=access.get().title() body=access.get().guidance() warning=true/> }}
        <Button variant=ButtonVariant::Secondary disabled=Signal::derive(move || access.get() == EditorAccess::Checking) on:click=move |_| recheck.run(())>"Retry check"</Button>
    </Show>
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
            let known_original = original_baseline_available.get_untracked();
            let comparison_base = if known_original { baseline.get_untracked() } else { current.document.clone() };
            let remote_display = if known_original {
                comparison_changes(recipe_comparison::changes(&comparison_base, &current.document, &Default::default()))
            } else {
                view! { <p>"The original revision is not available in this recovered draft."</p><p>"Compare the current saved revision with your retained local draft below. Remote changes since the original revision cannot be determined here."</p> }.into_any()
            };
            view! { <section aria-label="Save conflict" class="k-panel print-hide">
                <h2 id="recipe-save-conflict" tabindex="-1">"A newer saved revision is available"</h2>
                <p>{format!("Your draft is based on revision {}; the current saved revision is {}.", revision.get().map(|v| v.to_string()).unwrap_or_else(|| "new".into()), revision_number)}</p>
                <details><summary>"Compare saved revision and your draft"</summary><h3>{if known_original { "Remote changes" } else { "Original comparison unavailable" }}</h3>{remote_display}<h3>{if known_original { "Local changes" } else { "Current saved revision versus your draft" }}</h3>{move || draft.with(|d| {
                    let changes = recipe_comparison::changes(&comparison_base, d.document(), d.raw_numbers());
                    if known_original { comparison_changes(changes) } else { comparison_changes_from(changes, "Current saved", "No differences from the current saved revision.") }
                })}</details>
                <Button variant=crate::components::ButtonVariant::Secondary on:click=move |_| {
                    #[cfg(target_arch = "wasm32")]
                    if let Some(key) = recovery_storage_key.get_untracked() {
                        if crate::pages::recipe_recovery::remove(&key).is_err() { tracing::warn!("Unable to discard a retained recipe draft"); status.set("Could not discard the retained local draft.".into()); return; }
                        recovery_storage_key.set(None);
                    }
                    baseline.set(remote_for_reload.document.clone()); original_baseline_available.set(true); draft.update(|d| d.replace_saved(remote_for_reload.document.clone())); revision.set(Some(remote_for_reload.revision)); retry.set(None); conflict.set(None); status.set("Loaded the current saved revision; local changes were discarded.".into());
                }>"Discard draft and reload"</Button>
                <Button on:click=move |_| { baseline.set(remote_for_reapply.clone()); original_baseline_available.set(true); draft.update(|d| d.mark_earlier_document_saved(remote_for_reapply.clone())); revision.set(Some(revision_number)); retry.set(None); conflict.set(None); status.set("Draft retained against the current revision. Review it, then choose Save.".into()); }>"Reapply my draft"</Button>
            </section> }
        })}
        <crate::components::wayfinding::PageTitle text=Signal::derive(move || draft.with(|d| if d.document().title.is_empty() { "Recipe editor".to_owned() } else { d.document().title.clone() }))/>
        <header class="recipe-editor__header recipe-editor__recipe-header"><div class="recipe-editor__topbar"><nav class="recipe-editor__breadcrumb" aria-label="Recipe breadcrumb"><a href="/recipes">"Recipes"</a><span aria-hidden="true">"/"</span><span>{move || draft.with(|d| if d.document().title.is_empty() { "Recipe editor".to_owned() } else { d.document().title.clone() })}</span></nav><div class="recipe-editor__actions"><span role="status">{move || {
            let message = status.get();
            if draft.with(|d| d.dirty) && (message.is_empty() || message.starts_with("Saved revision")) {
                let local = if retained.get() { "Unsaved changes · retained locally" } else { "Unsaved changes" };
                if message.is_empty() { local.to_owned() } else { format!("{message} {local}") }
            } else { message }
        }}</span><Button variant=ButtonVariant::Secondary disabled=Signal::derive(move || pending.get() || revision.get().is_none() || draft.with(|d| d.dirty)) on:click=move |_| scale_open.set(true)>"Scale…"</Button><Button variant=ButtonVariant::Secondary disabled=Signal::derive(move || pending.get() || revision.get().is_none() || draft.with(|d| d.dirty)) on:click=move |_| duplicate_open.set(true)>"Duplicate…"</Button>{move || recipe_id.get().map(|id| view! { <ButtonLink href=format!("/recipes/{id}/brews") variant=ButtonVariant::Secondary>"Brew history"</ButtonLink> })}<Button variant=crate::components::ButtonVariant::Secondary disabled=Signal::derive(move || pending.get() || !draft.with(|d| d.can_undo())) on:click=move |_| draft.update(|d| d.undo())>"Undo"</Button><Button disabled=pending on:click=move |_| save()>"Save recipe"</Button></div></div></header>
        <div class="recipe-editor__heading"><p class="kit-showcase__eyebrow">"Your recipe"</p><h1 id="recipe-editor-heading" tabindex="-1">{move || draft.with(|d| if d.document().title.is_empty() { "Recipe editor".to_owned() } else { d.document().title.clone() })}</h1><p>"Changes are saved only when you choose Save."</p></div>
        <ScaleDialog open=scale_open identity=identity recipe_id=recipe_id revision=revision context_lost=context_lost uncertain=scale_uncertain pending=pending status=status on_applied=on_scaled/>
        <DuplicateDialog open=duplicate_open identity=identity recipe_id=recipe_id revision=revision context_lost=context_lost uncertain=duplicate_uncertain pending=pending status=status/>
        <div class="recipe-editor__summary recipe-editor__identity-summary">
            <BreweryDraftField id="recipe-title" label="Recipe name" value=title on_change=title_change mobile_title=true/>
            <NumberField id="recipe-batch" label="Batch volume" unit="L" path="batch_size_l" draft=draft read=|d: &RecipeDocumentV1| d.batch_size_l write=|d: &mut RecipeDocumentV1,n| d.batch_size_l=n/>
            <NumberField id="recipe-boil" label="Boil time" unit="min" path="boil_minutes" draft=draft read=|d: &RecipeDocumentV1| d.boil_minutes write=|d: &mut RecipeDocumentV1,n| d.boil_minutes=n/>
            {move || { let mut options = vec![(String::new(), "No style".to_owned())]; options.extend(styles.get().into_iter().map(|style| (style.name.clone(), format!("{} · {}", style.style_id.unwrap_or_default(), style.name)))); view! { <BreweryDraftSelect id="recipe-style" label="Beer style" value=selected_style on_change=change_style options=options/> } }}
        </div>
        <div class="recipe-editor__navigation"><BreweryTabs id="editor" label="Recipe sections" items=vec![("recipe".into(), "Recipe".into()), ("versions".into(), "Saved versions".into()), ("brews".into(), "Brews".into())] active=active_tab/>
        <Show when=move || active_tab.get() == "recipe"><Button variant=ButtonVariant::Secondary on:click=move |_| {
            #[cfg(target_arch = "wasm32")]
            {
                use wasm_bindgen::JsCast;
                if let Some(target) = web_sys::window().and_then(|window| window.document()).and_then(|document| document.get_element_by_id("start-brew")).and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok()) {
                    if target.focus().is_err() { tracing::warn!("Unable to focus the start-brew workflow"); }
                }
            }
        }>"Start brew"</Button></Show></div>
        <div role="tabpanel" id="editor-panel-recipe" aria-labelledby="editor-tab-recipe" hidden=move || active_tab.get() != "recipe">
        <div class="recipe-editor__layout"><FormulationPreview draft=draft identity=identity context_visible=context_visible gate=preview_gate><div class="recipe-editor__sections">
            <crate::pages::ingredient_addition::IngredientAdditions draft=draft identity=identity context_visible=context_visible/>
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
                <crate::pages::recipe_equipment::RecipeEquipment draft=draft identity=identity context_visible=context_visible recipe_id=recipe_id summary=equipment_summary/>
</BreweryDisclosure>
            <BreweryDisclosure title="Export and print">
                <crate::pages::beerxml::BeerXmlExport identity=identity context_visible=context_visible recipe_id=recipe_id revision=revision draft=draft/>
                <PrintRecipe identity=identity context_visible=context_visible recipe_id=recipe_id revision=revision draft=draft preview=print_preview/>
            </BreweryDisclosure>
        </div></FormulationPreview></div>
        <section id="start-brew" aria-label="Start brew workflow" tabindex="-1"><crate::pages::brew_start::StartBrewPanel identity=identity context_visible=context_visible recipe_id=recipe_id revision=revision/></section>
        </div>
        <div role="tabpanel" id="editor-panel-versions" aria-labelledby="editor-tab-versions" hidden=move || active_tab.get() != "versions">
            <Show when=move || active_tab.get() == "versions" && context_visible.get()>
                {move || recipe_id.get().map(|id| view! { <crate::components::RecipeRevisionHistory intent=identity.get_untracked() recipe_id=id recipe_title=baseline.with_untracked(|d| d.title.clone()) refresh_key=Signal::derive(move || revision.get().unwrap_or_default()) on_authority_failure=history_authority_failure selected_revision=selected_revision on_select=Callback::new(move |next| selected_revision.set(next))/> }.into_any()).unwrap_or_else(|| view! { <BreweryNotice title="No saved versions yet" body="Save this recipe to create its first saved version."/> }.into_any())}
            </Show>
        </div>
        <div role="tabpanel" id="editor-panel-brews" aria-labelledby="editor-tab-brews" hidden=move || active_tab.get() != "brews">
            <Show when=move || active_tab.get() == "brews" && context_visible.get()>
                {move || recipe_id.get().map(|id| view! { <BreweryPanel title="Brew history" description="Brews started from this recipe."><crate::pages::brew_history::BrewHistoryView embedded=true recipe_id=id intent=identity.get_untracked() on_authority_failure=history_authority_failure/></BreweryPanel> }.into_any()).unwrap_or_else(|| view! { <BreweryNotice title="No brews yet" body="Save this recipe before starting a brew."/> }.into_any())}
            </Show>
        </div>
    </div>
    {
        move || {
            print_preview
                .get()
                .filter(|_| context_visible.get())
                .map(|summary| view! { <PrintPreview summary=summary on_close=close_print/> })
        }
    }
    }
}

#[component]
fn PrintRecipe(
    identity: RwSignal<EditorIdentity>,
    context_visible: RwSignal<bool>,
    recipe_id: RwSignal<Option<String>>,
    revision: RwSignal<Option<i64>>,
    draft: RwSignal<RecipeDraft>,
    preview: RwSignal<Option<ProcessSummary>>,
) -> impl IntoView {
    let access = expect_context::<RwSignal<EditorAccess>>();
    let ContextCheckGeneration(check_generation) = expect_context::<ContextCheckGeneration>();
    let status = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let run = Callback::new(move |selection: ProcessSummarySelection| {
        if busy.get_untracked() || !context_visible.get_untracked() {
            return;
        }
        let draft_generation = match &selection {
            ProcessSummarySelection::Draft { .. } => {
                Some(draft.with_untracked(|draft| draft.generation))
            }
            ProcessSummarySelection::Saved { .. } => None,
        };
        let expected = identity.get_untracked();
        let context_epoch = check_generation.get_untracked();
        let request = ProcessSummaryRequest {
            intent: expected.clone(),
            selection,
        };
        busy.set(true);
        status.set("Preparing the print summary…".into());
        leptos::task::spawn_local(async move {
            let context_result = check_editor_access(&expected).await;
            if context_visible.try_get_untracked().is_none() {
                return;
            }
            if check_generation.try_get_untracked() != Some(context_epoch) {
                busy.set(false);
                return;
            }
            if context_result != EditorAccess::Verified {
                hold_editor(access, context_visible, context_result);
                busy.set(false);
                return;
            }
            let result =
                crate::server_fns::process_summary::request_process_summary_shared(request.clone())
                    .await;
            if context_visible.try_get_untracked().is_none() {
                return;
            }
            #[cfg(target_arch = "wasm32")]
            let result = if result
                .as_ref()
                .err()
                .is_some_and(|e| crate::utils::auth_refresh::is_auth_error(&e.to_string()))
                && crate::utils::auth_refresh::try_refresh().await
                && current_context_matches(&expected).await
            {
                crate::server_fns::process_summary::request_process_summary_shared(request).await
            } else {
                result
            };
            if context_visible.try_get_untracked().is_none() {
                return;
            }
            let context_result = check_editor_access(&expected).await;
            if context_visible.try_get_untracked().is_none() {
                return;
            }
            if check_generation.try_get_untracked() != Some(context_epoch) {
                busy.set(false);
                return;
            }
            if context_result != EditorAccess::Verified {
                hold_editor(access, context_visible, context_result);
                busy.set(false);
                return;
            }
            // Authority denial must invalidate a mounted editor even when the
            // draft changed during the request. Stale successful data may be
            // ignored, but forbidden/context-changed may not.
            if matches!(
                &result,
                Ok(ProcessSummaryOutcome::Forbidden | ProcessSummaryOutcome::ContextChanged)
            ) {
                let outcome = if matches!(&result, Ok(ProcessSummaryOutcome::Forbidden)) { EditorAccess::Forbidden } else { EditorAccess::Mismatch };
                hold_editor(access, context_visible, outcome);
                busy.set(false);
                return;
            }
            if draft_generation.is_some_and(|generation| {
                draft.try_with_untracked(|draft| draft.generation) != Some(generation)
            }) {
                busy.set(false);
                status.set("The draft changed. Prepare the print summary again.".into());
                return;
            }
            busy.set(false);
            match result {
                Ok(ProcessSummaryOutcome::Summary(summary)) => {
                    status.set("Review the sheet, then print or save it as a PDF.".into());
                    preview.set(Some(summary));
                }
                Ok(ProcessSummaryOutcome::NotFound) => status.set(
                    "This saved revision is not available in this workspace. Save the recipe before printing it."
                        .into(),
                ),
                Ok(ProcessSummaryOutcome::Forbidden) => hold_editor(access, context_visible, EditorAccess::Forbidden),
                Ok(ProcessSummaryOutcome::ContextChanged) => hold_editor(access, context_visible, EditorAccess::Mismatch),
                Ok(ProcessSummaryOutcome::UnresolvedEquipmentProfile) => status.set(
                    "The selected equipment profile is unavailable. Select a valid profile or use draft settings."
                        .into(),
                ),
                Ok(ProcessSummaryOutcome::UnresolvedStyle) => status.set(
                    "The selected style reference is unavailable. Choose a supported style or clear it."
                        .into(),
                ),
                Ok(ProcessSummaryOutcome::Invalid(validation)) => {
                    let detail = validation
                        .diagnostics
                        .first()
                        .map(diagnostic_message)
                        .unwrap_or_else(|| "Correct the invalid inputs.".into());
                    status.set(format!("{detail} Correct the recipe before printing."));
                }
                Ok(ProcessSummaryOutcome::InvalidRequest) => {
                    status.set("The print request was invalid.".into())
                }
                Ok(ProcessSummaryOutcome::UnsupportedRevision { schema_version }) => status.set(
                    format!(
                        "This saved revision uses unsupported recipe schema version {schema_version}."
                    ),
                ),
                Err(error) if crate::utils::auth_refresh::is_auth_error(&error.to_string()) => hold_editor(access, context_visible, EditorAccess::Expired),
                _ => status.set(
                    "Unable to prepare the print summary. Your draft remains open; try again when the connection is available."
                        .into(),
                ),
            }
        });
    });
    let saved = move |_| {
        let (Some(id), Some(number)) = (recipe_id.get_untracked(), revision.get_untracked()) else {
            status.set("Save the recipe before printing a saved revision.".into());
            return;
        };
        run.run(ProcessSummarySelection::Saved {
            recipe_id: id,
            revision: number,
        });
    };
    let unsaved = move |_| {
        let document = match draft.with_untracked(|draft| draft.snapshot()) {
            Ok(document) => document,
            Err(_) => {
                status.set("Complete invalid number fields before printing the draft.".into());
                return;
            }
        };
        run.run(ProcessSummarySelection::Draft {
            recipe_id: recipe_id.get_untracked(),
            base_revision: revision.get_untracked(),
            document,
        });
    };
    view! {
        <BreweryPanel
            title="Print recipe"
            description="Print the current saved revision or this unsaved draft. The sheet names its source and resolved equipment and context."
        >
            <p>"The browser print dialog handles paper size, margins and saving as PDF. A draft is labelled Unsaved and never replaces saved history."</p>
            <div class="recipe-editor__actions">
                <Button on:click=saved disabled=busy>"Preview saved revision"</Button>
                <Button variant=crate::components::ButtonVariant::Secondary on:click=unsaved disabled=busy>"Preview unsaved draft"</Button>
            </div>
            <p role="status">{move || status.get()}</p>
        </BreweryPanel>
    }
}

#[component]
fn PrintPreview(summary: ProcessSummary, on_close: Callback<()>) -> impl IntoView {
    let status = RwSignal::new(String::new());
    #[cfg(target_arch = "wasm32")]
    {
        let body = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.body());
        if let Some(body) = &body {
            if body.class_list().add_1("krausen-printing").is_err() {
                tracing::warn!("Unable to prepare the print-only page state");
                status.set("The print layout may include the application controls.".into());
            }
        }
        on_cleanup(move || {
            if let Some(body) = body {
                if body.class_list().remove_1("krausen-printing").is_err() {
                    tracing::warn!("Unable to clear the print-only page state");
                }
            }
        });
    }
    let print = move |_| {
        #[cfg(target_arch = "wasm32")]
        if let Some(window) = web_sys::window() {
            if window.print().is_err() {
                status.set("The browser print dialog could not open.".into());
            }
        }
    };
    view! {
        <section class="print-preview" aria-label="Print preview">
            <div class="print-preview__toolbar">
                <div>
                    <p class="kit-showcase__eyebrow">"Print preview"</p>
                    <p class="print-preview__hint">"Check the sheet, then print or save it as a PDF. The browser dialog controls paper size and margins."</p>
                </div>
                <div class="recipe-editor__actions">
                    <Button on:click=print>"Print / Save as PDF"</Button>
                    <Button variant=crate::components::ButtonVariant::Secondary on:click=move |_| on_close.run(())>"Back to editor"</Button>
                </div>
                <p role="status">{move || status.get()}</p>
            </div>
            <div class="print-preview__sheet">
                <crate::components::PrintableRecipe summary=summary/>
            </div>
        </section>
    }
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
    let blur = Callback::new(move |()| draft.update(|d| d.settle_number(path)));
    view! { <BreweryDraftField id=id label=label unit=unit value=value error=error on_change=change on_blur=blur numeric=true/> }
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
    let blur_path = path.clone();
    let blur = Callback::new(move |()| draft.update(|d| d.settle_number(&blur_path)));
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
    view! { <BreweryDraftField id=input_id label=label unit=unit value=value error=error on_change=change on_blur=blur numeric=true/> }
}

#[component]
fn FormulationPreview(
    draft: RwSignal<RecipeDraft>,
    identity: RwSignal<EditorIdentity>,
    context_visible: RwSignal<bool>,
    gate: StoredValue<crate::pages::recipe_preview_gate::PreviewGate>,
    children: Children,
) -> impl IntoView {
    use krausen_types::formulation::{Preview, PreviewOutcome, PreviewRequest};
    let access = expect_context::<RwSignal<EditorAccess>>();
    let ContextCheckGeneration(check_generation) = expect_context::<ContextCheckGeneration>();
    let retry_generation = RwSignal::new(0_u64);
    let retry_available = RwSignal::new(false);
    let result = RwSignal::new(None::<(u64, Preview)>);
    let message = RwSignal::new("Awaiting preview.".to_owned());
    let message_generation = RwSignal::new(0_u64);
    let trigger = Memo::new(move |_| {
        (
            draft.with(|d| d.generation),
            context_visible.get(),
            retry_generation.get(),
        )
    });
    Effect::new(move |_| {
        let (draft_generation, visible, _) = trigger.get();
        retry_available.set(false);
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
        let context_epoch = check_generation.get_untracked();
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
                || check_generation.try_get_untracked() != Some(context_epoch)
                || draft.try_with_untracked(|d| d.generation) != Some(draft_generation)
            {
                return;
            }
            let context_result = check_editor_access(&expected).await;
            if gate.try_with_value(|g| g.is_current(generation)) != Some(true)
                || context_visible.try_get_untracked() != Some(true)
                || check_generation.try_get_untracked() != Some(context_epoch)
                || draft.try_with_untracked(|d| d.generation) != Some(draft_generation)
            {
                return;
            }
            if context_result != EditorAccess::Verified {
                hold_editor(access, context_visible, context_result);
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
                let context_result = check_editor_access(&expected).await;
                if gate.try_with_value(|g| g.is_current(generation)) != Some(true)
                    || context_visible.try_get_untracked() != Some(true)
                    || check_generation.try_get_untracked() != Some(context_epoch)
                    || draft.try_with_untracked(|d| d.generation) != Some(draft_generation)
                {
                    return;
                }
                if context_result != EditorAccess::Verified {
                    hold_editor(access, context_visible, context_result);
                    return;
                }
                crate::server_fns::formulation::preview_formulation_shared(request.clone()).await
            } else {
                response
            };
            if gate.try_with_value(|g| g.is_current(generation)) != Some(true)
                || context_visible.try_get_untracked() != Some(true)
                || check_generation.try_get_untracked() != Some(context_epoch)
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
                        retry_available.set(true);
                    }
                }
                Ok(PreviewOutcome::UnresolvedProfile) => message.set(
                    "Equipment profile unavailable. Use draft settings or select a valid profile."
                        .into(),
                ),
                Ok(PreviewOutcome::UnresolvedStyle) => message.set(
                    "Style reference unavailable. Choose a supported style or clear it.".into(),
                ),
                Ok(PreviewOutcome::ContextChanged) => {
                    access.set(EditorAccess::Mismatch);
                    context_visible.set(false)
                }
                Ok(PreviewOutcome::Forbidden) => {
                    access.set(EditorAccess::Forbidden);
                    context_visible.set(false)
                }
                Ok(PreviewOutcome::Invalid) => {
                    message.set("Invalid draft. Correct the inputs before previewing.".into())
                }
                Err(e) if crate::utils::auth_refresh::is_auth_error(&e.to_string()) => {
                    access.set(EditorAccess::Expired);
                    context_visible.set(false)
                }
                _ => {
                    message.set("Preview unavailable. Your draft remains open.".into());
                    retry_available.set(true);
                }
            }
        });
    });
    let accepted = Signal::derive(move || {
        let current = draft.with(|d| d.generation);
        result.get().filter(|(generation, preview)| *generation == current && context_visible.get() && gate.try_with_value(|g| g.accepted_context() == preview.context_digest.as_deref()).unwrap_or(false)).map(|(_, preview)| preview)
    });
    let settled_message = Signal::derive(move || {
        let text = message.get();
        if draft.with(|d| d.generation) != message_generation.get()
            || matches!(text.as_str(), "Calculating…" | "Awaiting preview.") { String::new() } else { text }
    });
    view! { <div class="recipe-editor__preview-summary"><BreweryPanel title="The beer, at a glance" eyebrow="Your formulation" description="Estimates use the current draft." mobile_compact=true presentation=crate::components::PanelPresentation::Summary>
        <p>{move || if draft.with(|d| d.generation) == message_generation.get() && message.get() == "Calculating…" { "Calculating…" } else { "" }}</p>
        <p role="status">{move || settled_message.get()}</p>
        <Show when=move || retry_available.get() && context_visible.get()>
            <Button variant=ButtonVariant::Secondary on:click=move |_| retry_generation.update(|generation| *generation = generation.checked_add(1).expect("preview retry generation exhausted"))>"Retry calculation"</Button>
        </Show>
        <crate::components::BreweryBeerPortrait/>
        {move || accepted.get().map(|preview| view! { <PreviewMetrics preview=preview/> })}
        <div>{move || draft.with(|d| d.diagnostics().iter().map(|diagnostic| view! { <p>{diagnostic_message(diagnostic)}</p> }).collect_view())}</div>
    </BreweryPanel></div>{children()}<div class="recipe-editor__preview-details">{move || accepted.get().map(|preview| view! { <PreviewDetails preview=preview/> })}</div> }
}

#[component]
fn PreviewMetrics(preview: krausen_types::formulation::Preview) -> impl IntoView {
    let style = preview.style.as_ref();
    let comparison = preview.style_comparison.as_ref();
    view! { <div class="recipe-editor__metrics">
        {preview_metric("Original gravity", preview.targets.original_sg, style.map(|s| &s.original_sg), comparison.and_then(|c| c.original_sg), "SG", 3, crate::components::MetricDensity::Headline)}
        {preview_metric("Final gravity", preview.targets.final_sg, style.map(|s| &s.final_sg), comparison.and_then(|c| c.final_sg), "SG", 3, crate::components::MetricDensity::Headline)}
        {preview_metric("Alcohol", preview.targets.abv_percent, style.map(|s| &s.abv_percent), comparison.and_then(|c| c.abv_percent), "%", 1, crate::components::MetricDensity::Headline)}
        {preview_metric("Bitterness", preview.targets.ibu, style.map(|s| &s.ibu), comparison.and_then(|c| c.ibu), "IBU", 1, crate::components::MetricDensity::Headline)}
        {preview_metric("Colour", preview.targets.colour_ebc, style.map(|s| &s.colour_ebc), comparison.and_then(|c| c.colour_ebc), "EBC", 1, crate::components::MetricDensity::Headline)}
        {preview_metric("Total water", preview.targets.total_water_l, None, None, "L", 1, crate::components::MetricDensity::Headline)}
    </div>
    {preview.style.as_ref().map(|style| view! { <p class="recipe-editor__provenance">{format!("Style reference: {} · {} · {}", style.name, style.style_id, style.edition)}</p> })}
    }
}

#[component]
fn PreviewDetails(preview: krausen_types::formulation::Preview) -> impl IntoView {
    view! {
    <BreweryPanel title="Water plan"><div class="recipe-editor__metrics">
        {preview_metric("Mash water", preview.targets.mash_water_l, None, None, "L", 1, crate::components::MetricDensity::Quiet)}
        {preview_metric("Sparge water", preview.targets.sparge_water_l, None, None, "L", 1, crate::components::MetricDensity::Quiet)}
        {preview_metric("Kettle water", preview.targets.kettle_water_l, None, None, "L", 1, crate::components::MetricDensity::Quiet)}
        {preview_metric("Grain absorption", preview.targets.grain_absorption_l, None, None, "L", 1, crate::components::MetricDensity::Quiet)}
        {preview_metric("Pre-boil volume", preview.targets.pre_boil_l, None, None, "L", 1, crate::components::MetricDensity::Quiet)}
        {preview_metric("Post-boil volume", preview.targets.post_boil_l, None, None, "L", 1, crate::components::MetricDensity::Quiet)}
        {preview_metric("Pre-boil gravity", preview.targets.pre_boil_sg, None, None, "SG", 3, crate::components::MetricDensity::Quiet)}
    </div></BreweryPanel>
    {preview.context.as_ref().map(|context| {
        let equipment = context.effective_equipment.clone();
        let name = context.equipment_profile.as_ref().map(|profile| format!("{} · revision {}", profile.name, profile.revision)).unwrap_or_else(|| "Recipe equipment settings".to_owned());
        view! { <BreweryPanel title="Your equipment" description=name>
            <div class="recipe-editor__metrics">
                {preview_metric("Mash efficiency", equipment.efficiency_override_fraction.or(equipment.mash_efficiency_fraction).map(|value| value * 100.0), None, None, "%", 1, crate::components::MetricDensity::Quiet)}
                {preview_metric("Boil-off", equipment.boil_off_l_per_hour, None, None, "L/h", 1, crate::components::MetricDensity::Quiet)}
                {preview_metric("Transfer loss", equipment.transfer_loss_l, None, None, "L", 1, crate::components::MetricDensity::Quiet)}
            </div>
            <p>"New brews use current settings; previous brews retain their original equipment."</p>
        </BreweryPanel> }
    })}
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
    density: crate::components::MetricDensity,
) -> AnyView {
    use krausen_types::formulation::RangeStatus;
    let formatted = value
        .map(|n| format!("{n:.digits$}"))
        .unwrap_or_else(|| "—".into());
    let range_view = range.map(|r| {
        let state = match comparison { Some(RangeStatus::Below) => crate::components::StyleState::Below, Some(RangeStatus::Within) => crate::components::StyleState::Within, Some(RangeStatus::Above) => crate::components::StyleState::Above, None => crate::components::StyleState::Awaiting };
        view! { <crate::components::StyleRange label=label min=r.min max=r.max value=value state=state unit=unit digits=digits/> }
    });
    view! { <crate::components::BreweryMetric label=label value=formatted unit=unit density=density>{range_view}</crate::components::BreweryMetric> }.into_any()
}
