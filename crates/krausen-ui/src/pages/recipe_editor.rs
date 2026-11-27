// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recipe editor over the shared, authorized recipe operations.
use crate::{
    components::{
        BreweryDisclosure, BreweryDraftField, BreweryNotice, BreweryPanel, BreweryStale, BreweryTabs,
        BreweryStyleRangePlaceholder, Button, ButtonVariant, SelectedSavedRevision,
        printable_recipe::diagnostic_message,
    },
    pages::recipe_draft::RecipeDraft,
    pages::recipe_header::{EditorStatus, RecipeActionSlots, RecipeHeader},
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
#[cfg(target_arch = "wasm32")]
use crate::utils::focus::{focus_element, try_focus_element, try_focus_element_in_place};
use crate::utils::focus::jump_after_render;
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
    on_failure: Callback<EditorAccess>,
}

impl EditorAuthorityHold {
    /// Reports an authority outcome seen by a task that began under check `epoch`. An outcome from
    /// before the current check is ignored (`false`); otherwise the editor presents it (`true`).
    pub(crate) fn report(self, epoch: u64, outcome: EditorAccess) -> bool {
        if self.generation.try_get_untracked() != Some(epoch) { return false; }
        self.on_failure.run(outcome);
        true
    }
}

/// How a saved-versions or brews panel reports a lost authority. Inside the editor the report goes
/// through [`EditorAuthorityHold`], stamped with the check epoch current when the panel's request
/// began, so an answer older than the current re-check is ignored like the editor's other authority
/// callbacks. A library page supplies its own callback. The host is chosen by the caller, so a
/// panel cannot be mounted without saying how its losses are reported.
#[derive(Clone, Copy)]
pub struct HistoryAuthority(HistoryHost);

#[derive(Clone, Copy)]
enum HistoryHost {
    Editor(EditorAuthorityHold),
    Callback(Callback<()>),
    Standalone,
}

/// One request's report, bound to the epoch current when the request began.
#[derive(Clone, Copy)]
pub(crate) struct HistoryAuthorityRequest {
    host: HistoryHost,
    epoch: Option<u64>,
}

impl HistoryAuthority {
    /// For panels inside the recipe editor; call it where the editor's context is available.
    pub(crate) fn editor() -> Self {
        Self(HistoryHost::Editor(expect_context::<EditorAuthorityHold>()))
    }
    /// For a host that redacts itself through its own callback.
    pub fn callback(callback: Callback<()>) -> Self { Self(HistoryHost::Callback(callback)) }
    /// For a page that has nothing to redact.
    pub fn standalone() -> Self { Self(HistoryHost::Standalone) }
    /// Called when a request starts, before anything is awaited.
    pub(crate) fn begin(self) -> HistoryAuthorityRequest {
        let epoch = match self.0 {
            HistoryHost::Editor(hold) => hold.generation.try_get_untracked(),
            _ => None,
        };
        HistoryAuthorityRequest { host: self.0, epoch }
    }
}

impl HistoryAuthorityRequest {
    /// Reports the loss. A report older than the current re-check is ignored, and `false` tells the
    /// caller to drop the answer instead of showing it. When the load that produced it is still the
    /// panel's current one (`current`), `reload` is bumped so the panel reloads against the access it
    /// has now; a load that was already superseded has a successor that does that itself.
    pub(crate) fn lost_or_reload(self, reload: RwSignal<u64>, current: bool) -> bool {
        let honoured = match (self.host, self.epoch) {
            // An editor that is gone (now, or already when the request began) has nothing to redact
            // and its panel is gone with it, so that is neither a stale report nor a reload.
            (HistoryHost::Editor(hold), Some(epoch)) => hold.generation.try_get_untracked().is_none() || hold.report(epoch, EditorAccess::Forbidden),
            (HistoryHost::Editor(_), None) => true,
            (HistoryHost::Callback(callback), _) => { let _ = callback.try_run(()); true }
            (HistoryHost::Standalone, _) => true,
        };
        if !honoured && current { reload.update(|v| *v += 1); }
        honoured
    }
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
        // Nothing was focused, so there is nothing to give back: moving focus to the heading
        // afterwards would also scroll the page to the top.
        if element.tag_name() == "BODY" || element.tag_name() == "HTML" { return None; }
        let id = element.id();
        let print_return = element.closest(".print-preview").ok().flatten().is_some();
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
        let try_focus = try_focus_element;
        let print = document.query_selector(".print-preview").ok().flatten();
        let original = document.get_element_by_id(&self.id)
            .filter(|_| print.is_none() || self.print_return)
            .filter(|element| try_focus_element_in_place(element));
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
        } else if let Some(heading) = document.get_element_by_id(crate::pages::recipe_header::EDITOR_HEADING_ID) {
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
            let seed = if id == "new" {
                crate::pages::equipment_profiles::load_default_profile(&identity)
                    .await
                    .map(|profile| profile.new_recipe_seed())
            } else {
                None
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
            Ok::<_, ServerFnError>((identity, result, seed))
        }
    });
    view! { <Suspense fallback=|| view! { <p>"Loading recipe…"</p> }>
        {move || Suspend::new(async move { match load.await {
            Ok((identity, None, seed)) => view! { <RecipeForm identity=identity initial=None seed=seed/> }.into_any(),
            Ok((identity, Some(RecipeReadOutcome::Found(snapshot)), _)) => view! { <RecipeForm identity=identity initial=Some(snapshot) seed=None/> }.into_any(),
            Ok((_, Some(RecipeReadOutcome::NotFound), _)) => view! { <BreweryNotice title="Recipe not found" body="It may have been archived or belongs to another workspace."/> }.into_any(),
            Ok((_, Some(RecipeReadOutcome::Forbidden), _)) => view! { <BreweryNotice title="Access denied" body="You no longer have access to this workspace." warning=true/> }.into_any(),
            Ok((_, Some(RecipeReadOutcome::ContextChanged), _)) => view! { <BreweryNotice title="Account or workspace changed" body="Return to the original context and reopen this recipe." warning=true/> }.into_any(),
            Err(error) if crate::utils::auth_refresh::is_auth_error(&error.to_string()) => view! { <BreweryNotice title="Session expired" body=EditorAccess::Expired.guidance() warning=true/><Button variant=ButtonVariant::Secondary on:click=move |_| load.refetch()>"Retry"</Button> }.into_any(),
            _ => view! { <BreweryNotice title="Unable to load recipe" body="Your recipe could not be loaded. Retry when the connection is available." warning=true/><Button variant=ButtonVariant::Secondary on:click=move |_| load.refetch()>"Retry"</Button> }.into_any(),
        } })}
    </Suspense> }
}

/// `(DOM id, visible label)` of the batch volume control, declared once for the
/// control and the diagnostic registry.
pub(crate) const BATCH_FIELD: (&str, &str) = ("recipe-batch", "Batch volume");

/// Open a native disclosure from a header command, then move focus into it.
#[cfg(target_arch = "wasm32")]
fn reveal_disclosure(disclosure: &str, focus: &str) {
    use wasm_bindgen::JsCast;
    let Some(document) = web_sys::window().and_then(|window| window.document()) else { return; };
    if let Some(details) = document.get_element_by_id(disclosure).and_then(|element| element.dyn_into::<web_sys::HtmlDetailsElement>().ok()) {
        details.set_open(true);
        let summary = details.first_element_child().and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok());
        if focus == disclosure {
            if let Some(summary) = summary { if summary.focus().is_err() { tracing::warn!("Unable to focus a disclosure"); } }
            return;
        }
    }
    focus_element(focus);
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
fn RecipeForm(identity: EditorIdentity, initial: Option<RecipeSnapshot>, seed: Option<RecipeDocumentV1>) -> impl IntoView {
    let identity = RwSignal::new(identity);
    let access = RwSignal::new(EditorAccess::Verified);
    let check_generation = RwSignal::new(0_u64);
    provide_context(access);
    provide_context(ContextCheckGeneration(check_generation));
    let context_visible = RwSignal::new(true);
    let context_lost = RwSignal::new(false);
    let equipment_summary = RwSignal::new(initial.as_ref().map(|snapshot| crate::pages::recipe_equipment::EquipmentSummary::from_saved_document(&snapshot.document)).unwrap_or_default());
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
        on_failure: Callback::new(move |outcome| {
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
    // A new recipe starts from the workspace default equipment; a saved one from its document.
    let baseline = RwSignal::new(
        initial
            .as_ref()
            .map(|s| s.document.clone())
            .or_else(|| seed.clone())
            .unwrap_or_default(),
    );
    let draft = RwSignal::new(RecipeDraft::new(
        initial.map(|s| s.document).or(seed).unwrap_or_default(),
    ));
    #[cfg(target_arch = "wasm32")]
    let focus_before_check = StoredValue::new(None::<EditorFocus>);
    // The editor's height when a re-check began. It is held (as a minimum) until the first
    // estimate after a passing check has settled, because the estimate and its field hints are
    // cleared meanwhile and the shorter page would otherwise clamp the scroll position.
    let height_hold = RwSignal::new(None::<f64>);
    // A denied or lost check releases the hold at once; only a passing one keeps it.
    Effect::new(move |_| {
        if !context_visible.get() && access.get() != EditorAccess::Checking { height_hold.set(None); }
    });
    let recheck = Callback::new(move |()| {
        #[cfg(target_arch = "wasm32")]
        {
            let document = web_sys::window().and_then(|w| w.document());
            // A re-check entered through `context_visible.set(false)` starts with the editor already
            // hidden, but focus may still sit in it until the next frame; that focus is recorded.
            // With none there (a Retry starts from the held notice) the held check's bookmark is kept.
            let focus_in_editor = document.as_ref().and_then(|d| d.active_element())
                .is_some_and(|active| active.closest(".recipe-editor").ok().flatten().is_some());
            let visible = context_visible.get_untracked();
            if visible || focus_in_editor { focus_before_check.set_value(EditorFocus::capture()); }
            if visible {
                height_hold.set(document.and_then(|d| d.query_selector(".recipe-editor").ok().flatten()).map(|e| e.get_bounding_client_rect().height()));
            }
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
                        // A passing check spends its bookmark here, so a later re-check that
                        // records no focus of its own cannot restore it. A held outcome keeps it
                        // for the explicit retry, and a check hidden again before this frame keeps
                        // it for the next check.
                        if let Some(focus) = focus_before_check.try_update_value(Option::take).flatten() { focus.restore(); }
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
    let status = EditorStatus::new();
    let pending = RwSignal::new(false);
    let save_sequence = RwSignal::new(0_u64);
    let retry = RwSignal::new(None::<PendingRecipeSave>);
    let conflict = RwSignal::new(None::<RecipeSnapshot>);
    let print_preview = RwSignal::new(None::<ProcessSummary>);
    let scale_open = RwSignal::new(false);
    let duplicate_open = RwSignal::new(false);
    let print_open = RwSignal::new(false);
    let export_open = RwSignal::new(false);
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
    Effect::new(move |_| editor_guard.message.set(if context_visible.get() { status.text.get() } else { access.get().guidance().to_owned() }));
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
                status.fail_storage("Local draft recovery is unavailable in this browser.".into());
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
                        status.fail_storage("Local draft recovery is unavailable in this browser.".into())
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
                    status.fail_storage("Local draft recovery is unavailable in this browser.".into());
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
                    status.resolve_storage_failure();
                    status.fail_storage("Unable to retain this draft locally; keep this page open.".into());
                } else {
                    retained.set(true);
                    status.resolve_storage_failure();
                    if let Some(old) = recovery_storage_key
                        .get_untracked()
                        .filter(|old| old != &key)
                    {
                        if crate::pages::recipe_recovery::remove(&old).is_err() {
                            tracing::warn!("Unable to remove an older retained recipe draft");
                            status.fail_storage(
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
                    status.resolve_storage_failure();
                } else {
                    tracing::warn!("Unable to remove a retained recipe draft");
                    status.fail_storage("The retained draft could not be removed from this browser.".into());
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
                status.fail("Complete invalid number fields before saving.".into());
                saving_to_leave.set(false);
                return;
            };
            let Some(request_id) = crate::utils::request_id::new_request_id() else {
                status.fail("Unable to create a secure save request.".into());
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
                            status.fail_storage(format!("Saved revision {saved}, but the retained local draft could not be cleared."));
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
                        // Checked against the context this save started in. If access was lost and
                        // regained since, that check is no longer trusted: keep the list out of date.
                        if check_generation.try_get_untracked() == Some(save_epoch) {
                            draft.update(|d| { d.apply_validation(attempt.generation, validation); });
                        }
                        status.fail("Correct the invalid fields before saving.".into());
                    } else {
                        status.fail(
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
                                _ => status.fail("The current saved revision could not be loaded. Your draft is unchanged.".into()),
                            }
                        } else {
                            hold_editor(access, context_visible, outcome);
                        }
                    }
                }
                Ok(RecipeSaveOutcome::ResultExpired { resource_id }) => {
                    status.fail(match resource_id {
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
                    status.fail("The prior save request could not be verified. Reopen the recipe before saving again.".into());
                }
                Ok(RecipeSaveOutcome::NotFound) => {
                    retry.set(None);
                    status.fail("This recipe is no longer available in this workspace. Your draft is still open.".into());
                }
                Ok(RecipeSaveOutcome::InvalidRequest) => {
                    retry.set(None);
                    status.fail("The save request was invalid. Your draft is still open.".into());
                }
                Ok(RecipeSaveOutcome::ContextChanged) => {
                    access.set(EditorAccess::Mismatch);
                    context_visible.set(false);
                    status.set("Your account or workspace changed. The draft is held in its original context.".into());
                }
                Ok(RecipeSaveOutcome::Forbidden) => {
                    access.set(EditorAccess::Forbidden);
                    context_visible.set(false);
                    status.fail("You no longer have access to this workspace.".into())
                }
                Ok(RecipeSaveOutcome::UnresolvedEquipmentProfile) => {
                    retry.set(None);
                    status.fail("The selected equipment profile is unavailable.".into())
                }
                Ok(
                    RecipeSaveOutcome::UnresolvedIngredientSource
                    | RecipeSaveOutcome::InvalidProvenance,
                ) => {
                    retry.set(None);
                    status.fail("An ingredient or import reference could not be verified.".into());
                }
                Err(e) if crate::utils::auth_refresh::is_auth_error(&e.to_string()) => {
                    access.set(EditorAccess::Expired);
                    context_visible.set(false);
                    status.fail(
                        "Your session expired. Sign in again; your draft is still open.".into(),
                    );
                }
                _ => status.fail(
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
                    status.fail_storage("Could not discard the retained local draft.".into());
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
                // Request identity, not local draft storage: a later storage success
                // must not silently retire this outcome.
                status.fail(
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
            status.fail_storage("Could not remove the retained draft.".into());
            return;
        }
        status.resolve_storage_failure();
        recovery_choice.update(|records| records.retain(|(candidate, _)| candidate != &key));
    });
    Effect::new(move |_| {
        if matches!(access.get(), EditorAccess::Forbidden | EditorAccess::Mismatch) {
            print_preview.set(None);
        }
    });
    // Closing the preview returns focus to the opener (Recipe actions); the page
    // behind stayed mounted, so draft, scroll and selection are untouched.
    let close_print = Callback::new(move |()| {
        print_preview.set(None);
        #[cfg(target_arch = "wasm32")]
        request_animation_frame(move || focus_element("recipe-actions-trigger"));
    });
    // Section and inspected saved version live in additive URL state (see
    // utils::nav_state); the editable draft is never replaced by either.
    let route_params = use_params_map();
    let query = use_query_map();
    let navigate = use_navigate();
    let initial_section = query.with_untracked(|q| RecipeSection::parse(q.get("section").as_deref()));
    let active_tab = RwSignal::new(initial_section.key().to_owned());
    let actions_open = RwSignal::new(false);
    // Scale and Duplicate need a clean saved recipe; the dialogs explain this
    // when invoked and never save implicitly. The shared dialog returns focus.
    let transform_prerequisite_signal = Signal::derive(move || {
        crate::pages::recipe_output::transform_prerequisite(revision.get().is_some(), draft.with(|d| d.dirty))
    });
    let style_options = Signal::derive(move || {
        let mut options = vec![(String::new(), "No style".to_owned())];
        options.extend(styles.get().into_iter().map(|style| (style.name.clone(), format!("{} · {}", style.style_id.unwrap_or_default(), style.name))));
        options
    });
    let selected_revision = RwSignal::new(if initial_section == RecipeSection::Versions {
        query.with_untracked(|q| parse_revision(q.get("revision").as_deref()))
    } else {
        None
    });
    provide_context(SelectedSavedRevision(selected_revision));
    let start_open = RwSignal::new(false);
    let version_labels = RwSignal::new(Vec::<krausen_types::recipe_library::RevisionSummary>::new());
    let start_historical = RwSignal::new(None::<krausen_types::recipe_library::RevisionSummary>);
    let reveal = move |disclosure: &'static str, focus: &'static str| {
        active_tab.set("recipe".to_owned());
        #[cfg(target_arch = "wasm32")]
        request_animation_frame(move || reveal_disclosure(disclosure, focus));
        #[cfg(not(target_arch = "wasm32"))]
        let _ = (disclosure, focus);
    };
    let jump = Callback::new(move |(target, explanation): (String, String)| {
        active_tab.set("recipe".to_owned());
        jump_after_render(target, explanation);
    });
    provide_context(crate::components::diagnostic_list::DiagnosticJump(jump));
    let review = Callback::new(move |()| {
        // Explicit request: settle every touched field so its error shows, then go to the first one.
        draft.update(|d| d.settle_numbers());
        if let Some(first) = draft.with_untracked(crate::recipe_diagnostics::first_invalid) {
            let target = match first.route {
                crate::components::diagnostic_list::Route::Field { dom_id } => dom_id,
                crate::components::diagnostic_list::Route::Explain => first.key.clone(),
            };
            // A malformed entry has no summary row, so the page heading is the fallback.
            let explanation = if first.code == "malformed" { crate::pages::recipe_header::EDITOR_HEADING_ID.to_owned() } else { first.key };
            jump.run((target, explanation));
        }
    });
    let header_actions = RecipeActionSlots {
        review,
        save: Callback::new(move |()| save()),
        undo: Callback::new(move |()| draft.update(|d| d.undo())),
        // Start always opens the bounded review task; it never commits. A saved
        // version inspected through `?revision=N` is the default source; its list
        // entry is not needed, so the label falls back to the revision number.
        start: Callback::new(move |()| {
            let head = revision.get_untracked();
            start_historical.set(selected_revision.get_untracked().filter(|r| Some(*r) != head).map(|r| {
                version_labels.with_untracked(|known| known.iter().find(|v| v.revision == r).cloned())
                    .unwrap_or(krausen_types::recipe_library::RevisionSummary { revision: r, saved_by: None, saved_at: None, restored_from_revision: None })
            }));
            start_open.set(true);
        }),
        scale: Callback::new(move |()| scale_open.set(true)),
        duplicate: Callback::new(move |()| duplicate_open.set(true)),
        print: Callback::new(move |()| print_open.set(true)),
        export: Callback::new(move |()| export_open.set(true)),
        equipment: Callback::new(move |()| reveal("recipe-equipment-disclosure", "equipment-profile-select")),
    };

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
    // While access is being checked the content stays laid out but invisible (visibility:hidden:
    // not focusable, not clickable, not in the accessibility tree), so a
    // check that passes returns the person to the same scroll position. The notice is taken
    // out of the flow then (zero-height sticky host) so it cannot push the content down.
    let checking = Signal::derive(move || !context_visible.get() && access.get() == EditorAccess::Checking);
    let editor_ready = Signal::derive(move || recovery_ready.get() && recovery_choice.get().is_empty());
    let inert_now = Signal::derive(move || checking.get() && editor_ready.get());
    let editor_hidden = Signal::derive(move || (!context_visible.get() && !checking.get()) || !editor_ready.get());
    view! { <Show when=move || !context_visible.get()>
        <div class="recipe-editor-check-host" class:recipe-editor-check-host--checking=checking>
        {move || view! { <BreweryNotice title=access.get().title() body=access.get().guidance() warning=true/> }}
        <Button variant=ButtonVariant::Secondary disabled=Signal::derive(move || access.get() == EditorAccess::Checking) on:click=move |_| recheck.run(())>"Retry check"</Button>
        </div>
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
                <p role="status">{move || status.text.get()}</p></section>
        </Show>
        <div class="recipe-editor" class:recipe-editor--checking=inert_now style:min-height=move || height_hold.get().map(|h| format!("{h}px")) hidden=editor_hidden>
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
                        if crate::pages::recipe_recovery::remove(&key).is_err() { tracing::warn!("Unable to discard a retained recipe draft"); status.fail_storage("Could not discard the retained local draft.".into()); return; }
                        recovery_storage_key.set(None);
                    }
                    baseline.set(remote_for_reload.document.clone()); original_baseline_available.set(true); draft.update(|d| d.replace_saved(remote_for_reload.document.clone())); revision.set(Some(remote_for_reload.revision)); retry.set(None); conflict.set(None); status.set("Loaded the current saved revision; local changes were discarded.".into());
                }>"Discard draft and reload"</Button>
                <Button on:click=move |_| { baseline.set(remote_for_reapply.clone()); original_baseline_available.set(true); draft.update(|d| d.mark_earlier_document_saved(remote_for_reapply.clone())); revision.set(Some(revision_number)); retry.set(None); conflict.set(None); status.set("Draft retained against the current revision. Review it, then choose Save.".into()); }>"Reapply my draft"</Button>
            </section> }
        })}
        <crate::components::wayfinding::PageTitle text=Signal::derive(move || draft.with(|d| if d.document().title.is_empty() { "Recipe editor".to_owned() } else { d.document().title.clone() }))/>
        <RecipeHeader draft=draft revision=revision status=status retained=retained pending=pending conflict=Signal::derive(move || conflict.with(|c| c.is_some())) equipment=equipment_summary title=title on_title=title_change style_value=selected_style style_options=style_options on_style=change_style actions=header_actions actions_open=actions_open>
            <NumberField id=BATCH_FIELD.0 label=BATCH_FIELD.1 unit="L" described_by="recipe-batch-note" path="batch_size_l" draft=draft read=|d: &RecipeDocumentV1| d.batch_size_l write=|d: &mut RecipeDocumentV1,n| d.batch_size_l=n/>
        </RecipeHeader>
        <ScaleDialog open=scale_open identity=identity recipe_id=recipe_id revision=revision context_lost=context_lost uncertain=scale_uncertain pending=pending status=status on_applied=on_scaled prerequisite=transform_prerequisite_signal/>
        <PrintRecipe identity=identity context_visible=context_visible recipe_id=recipe_id revision=revision draft=draft preview=print_preview open=print_open selected=selected_revision/>
        <crate::pages::beerxml::BeerXmlExport identity=identity context_visible=context_visible recipe_id=recipe_id revision=revision draft=draft open=export_open selected=selected_revision/>
        <DuplicateDialog open=duplicate_open identity=identity recipe_id=recipe_id revision=revision context_lost=context_lost uncertain=duplicate_uncertain pending=pending status=status prerequisite=transform_prerequisite_signal/>
        <div class="recipe-editor__navigation"><BreweryTabs id="editor" label="Recipe sections" items=vec![("recipe".into(), "Recipe".into()), ("versions".into(), "Saved versions".into()), ("brews".into(), "Brews".into())] active=active_tab/></div>
        <crate::pages::brew_start::StartBrewTask
            open=start_open historical=start_historical identity=identity context_visible=context_visible
            recipe_id=recipe_id revision=revision
            dirty=Signal::derive(move || draft.with(|d| d.dirty))
            saving=Signal::derive(move || pending.get())
            save_status=Signal::derive(move || status.text.get())
            request_save=Callback::new(move |()| { if pending.get_untracked() { return false; } save(); pending.get_untracked() })
        />
        <div role="tabpanel" id="editor-panel-recipe" aria-labelledby="editor-tab-recipe" hidden=move || active_tab.get() != "recipe">
        <div class="recipe-editor__layout"><FormulationPreview draft=draft identity=identity context_visible=context_visible gate=preview_gate revision=revision height_hold=height_hold><div class="recipe-editor__sections">
            <crate::pages::ingredient_addition::IngredientAdditions draft=draft identity=identity context_visible=context_visible/>
            <crate::pages::recipe_process::ProcessSections draft=draft/>
            <BreweryDisclosure id="recipe-equipment-disclosure" title="Equipment settings">
                <crate::pages::recipe_equipment::RecipeEquipment draft=draft identity=identity context_visible=context_visible recipe_id=recipe_id summary=equipment_summary/>
</BreweryDisclosure>
        </div></FormulationPreview></div>
        </div>
        <div role="tabpanel" id="editor-panel-versions" aria-labelledby="editor-tab-versions" hidden=move || active_tab.get() != "versions">
            <Show when=move || active_tab.get() == "versions" && (context_visible.get() || checking.get())>
                {move || recipe_id.get().map(|id| view! { <crate::components::RecipeRevisionHistory intent=identity.get_untracked() recipe_id=id recipe_title=baseline.with_untracked(|d| d.title.clone()) refresh_key=Signal::derive(move || revision.get().unwrap_or_default()) authority=HistoryAuthority::editor() selected_revision=selected_revision on_select=Callback::new(move |next| selected_revision.set(next)) on_start=Callback::new(move |entry: krausen_types::recipe_library::RevisionSummary| { start_historical.set((Some(entry.revision) != revision.get_untracked()).then_some(entry)); start_open.set(true); }) loaded=version_labels/> }.into_any()).unwrap_or_else(|| view! { <BreweryNotice title="No saved versions yet" body="Save this recipe to create its first saved version."/> }.into_any())}
            </Show>
        </div>
        <div role="tabpanel" id="editor-panel-brews" aria-labelledby="editor-tab-brews" hidden=move || active_tab.get() != "brews">
            <Show when=move || active_tab.get() == "brews" && (context_visible.get() || checking.get())>
                {move || recipe_id.get().map(|id| view! { <BreweryPanel title="Brew history" description="Brews started from this recipe."><crate::pages::brew_history::BrewHistoryView embedded=true recipe_id=id intent=identity.get_untracked() authority=HistoryAuthority::editor()/></BreweryPanel> }.into_any()).unwrap_or_else(|| view! { <BreweryNotice title="No brews yet" body="Save this recipe before starting a brew."/> }.into_any())}
            </Show>
        </div>
    </div>
    // The page behind a print preview stays inert for as long as the task is open,
    // including while a revalidation unmounts the preview itself.
    // It is released while access is unavailable or expired, so the notice and its
    // Retry check stay usable; a denial clears the preview altogether.
    <crate::components::PageIsolation active=Signal::derive(move || print_preview.with(|preview| preview.is_some()) && (context_visible.get() || access.get() == EditorAccess::Checking))/>
    {
        move || {
            print_preview
                .get()
                .filter(|_| context_visible.get())
                .map(|summary| view! {
                    <crate::components::PrintPreviewTask heading="Print preview" hint="Check the sheet, then print or save it as a PDF. The browser dialog controls paper size and margins." isolate=false on_close=close_print>
                        <div class="print-preview__sheet"><crate::components::PrintableRecipe summary=summary/></div>
                    </crate::components::PrintPreviewTask>
                })
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
    open: RwSignal<bool>,
    selected: RwSignal<Option<i64>>,
) -> impl IntoView {
    use crate::pages::recipe_output::{OutputSource, SourceChoice, resolve_source};
    let access = expect_context::<RwSignal<EditorAccess>>();
    let ContextCheckGeneration(check_generation) = expect_context::<ContextCheckGeneration>();
    let status = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    // Closing the task retires an in-flight request: its late result must not
    // open a preview the user already dismissed.
    let task_generation = RwSignal::new(0_u64);
    let source = Signal::derive(move || resolve_source(revision.get(), draft.with(|d| d.dirty), selected.get()));
    let run = Callback::new(move |selection: ProcessSummarySelection| {
        if busy.get_untracked() || !context_visible.get_untracked() {
            return;
        }
        let task = task_generation.get_untracked();
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
            // A closed, superseded or revalidated run never touches the state of
            // the run that replaced it; the effect below owns `busy` for those.
            let mine = move || task_generation.try_get_untracked() == Some(task);
            let context_result = check_editor_access(&expected).await;
            if context_visible.try_get_untracked().is_none() {
                return;
            }
            if check_generation.try_get_untracked() != Some(context_epoch) {
                return;
            }
            if context_result != EditorAccess::Verified {
                hold_editor(access, context_visible, context_result);
                if mine() {
                    busy.set(false);
                }
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
                return;
            }
            if context_result != EditorAccess::Verified {
                hold_editor(access, context_visible, context_result);
                if mine() {
                    busy.set(false);
                }
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
                if mine() {
                    busy.set(false);
                }
                return;
            }
            if draft_generation.is_some_and(|generation| {
                draft.try_with_untracked(|draft| draft.generation) != Some(generation)
            }) {
                if mine() {
                    busy.set(false);
                    status.set("The draft changed. Preview the print summary again.".into());
                }
                return;
            }
            if !mine() {
                return;
            }
            busy.set(false);
            match result {
                Ok(ProcessSummaryOutcome::Summary(summary)) => {
                    status.set(String::new());
                    preview.set(Some(summary));
                    open.set(false);
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
    let prepare = Callback::new(move |chosen: OutputSource| {
        status.set(String::new());
        match chosen {
            OutputSource::Saved { revision: number } => {
                let Some(id) = recipe_id.get_untracked() else {
                    status.set("Save the recipe before printing a saved revision.".into());
                    return;
                };
                run.run(ProcessSummarySelection::Saved { recipe_id: id, revision: number });
            }
            OutputSource::Draft => {
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
            }
        }
    });
    // Opening resets the task; a known source is prepared at once, an ambiguous
    // one waits for the short choice. Closing retires any in-flight request.
    // It also runs again when a revalidation restores the editor, so a request
    // dropped by the check is prepared afresh instead of leaving the task stuck.
    Effect::new(move |_| {
        if open.get() && context_visible.get() {
            status.set(String::new());
            if let SourceChoice::Known(chosen) = source.get_untracked() {
                prepare.run(chosen);
            }
        } else {
            task_generation.update(|n| *n = n.wrapping_add(1));
            busy.set(false);
        }
    });
    let description = Signal::derive(move || match source.get() {
        SourceChoice::Known(chosen) => format!(
            "Source: {}. The sheet names its source and the resolved equipment and context.",
            chosen.label()
        ),
        SourceChoice::Choose { .. } => {
            "Your draft has changes that are not saved. Choose which source to print.".to_owned()
        }
    });
    view! {
        <crate::components::BreweryDialog
            id="print-task"
            title="Print recipe"
            description=description
            open=open
            wide=true
            return_focus_fallback="recipe-actions-trigger".to_owned()
            actions=std::sync::Arc::new(move || view! {
                <Button variant=crate::components::ButtonVariant::Ghost on:click=move |_| open.set(false)>"Close"</Button>
                {move || match source.get() {
                    SourceChoice::Choose { saved, draft: draft_source } => view! {
                        <Button variant=crate::components::ButtonVariant::Secondary disabled=busy on:click=move |_| prepare.run(saved)>{format!("Preview {}", saved.label().to_lowercase())}</Button>
                        <Button disabled=busy on:click=move |_| prepare.run(draft_source)>"Preview unsaved draft"</Button>
                    }.into_any(),
                    SourceChoice::Known(_) => ().into_any(),
                }}
            }.into_any())
        >
            <div class="space-y-4">
                <p>"The browser print dialog handles paper size, margins and saving as PDF. A draft is labelled Unsaved and never replaces saved history."</p>
                <p role="status">{move || if busy.get() { "Preparing the print summary…".to_owned() } else { status.get() }}</p>
            </div>
        </crate::components::BreweryDialog>
    }
}

#[component]
pub(crate) fn NumberField(
    id: &'static str,
    label: &'static str,
    unit: &'static str,
    path: &'static str,
    draft: RwSignal<RecipeDraft>,
    #[prop(default = 1.0)] scale: f64,
    #[prop(optional, into)] described_by: MaybeProp<String>,
    read: fn(&RecipeDocumentV1) -> Option<f64>,
    write: fn(&mut RecipeDocumentV1, Option<f64>),
) -> impl IntoView {
    let value = Signal::derive(move || {
        draft.with(|d| d.number_text(path, read(d.document()).map(|v| v * scale)))
    });
    let error = Signal::derive(move || draft.with(|d| d.number_error(path).map(str::to_owned)));
    let hint = Signal::derive(move || draft.with(|d| d.validation_hint(path).map(str::to_owned)));
    let change = Callback::new(move |text| {
        draft.update(|d| d.edit_number(path, text, |doc, n| write(doc, n.map(|v| v / scale))))
    });
    let blur = Callback::new(move |()| draft.update(|d| d.settle_number(path)));
    view! { <BreweryDraftField id=id label=label unit=unit value=value error=error hint=hint described_by=described_by on_change=change on_blur=blur numeric=true/> }
}

#[component]
fn FormulationPreview(
    draft: RwSignal<RecipeDraft>,
    identity: RwSignal<EditorIdentity>,
    context_visible: RwSignal<bool>,
    gate: StoredValue<crate::pages::recipe_preview_gate::PreviewGate>,
    revision: RwSignal<Option<i64>>,
    height_hold: RwSignal<Option<f64>>,
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
    // Why there is no current estimate, for the process sections' stale cue.
    let phase = RwSignal::new(crate::pages::recipe_process::EstimatePhase::Calculating);
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
        if draft.with_untracked(|d| d.readiness().is_some()) { draft.update(|d| d.clear_readiness()); }
        message_generation.set(draft_generation);
        if !visible {
            // No estimate is being worked on while the editor is hidden: do not keep the
            // last outcome for whoever sees the editor next.
            phase.set(crate::pages::recipe_process::EstimatePhase::Calculating);
            return;
        }
        let Ok(document) = draft.with_untracked(|d| d.snapshot()) else {
            phase.set(crate::pages::recipe_process::EstimatePhase::InvalidInput);
            message.set("Complete invalid number fields to see estimates.".into());
            return;
        };
        phase.set(crate::pages::recipe_process::EstimatePhase::Calculating);
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
                        message.set(crate::recipe_diagnostics::readiness_summary(&preview.validation));
                        draft.update(|d| { d.apply_validation(draft_generation, preview.validation.clone()); });
                        result.set(Some((draft_generation, preview)));
                    } else if preview.validation.state
                        == krausen_types::validation::Readiness::Invalid
                        && preview.context.is_none()
                        && preview.context_digest.is_none()
                    {
                        message.set(crate::recipe_diagnostics::readiness_summary(&preview.validation));
                        draft.update(|d| { d.apply_validation(draft_generation, preview.validation); });
                        phase.set(crate::pages::recipe_process::EstimatePhase::InvalidInput);
                    } else {
                        phase.set(crate::pages::recipe_process::EstimatePhase::Unavailable);
                        message.set("Preview context changed. Try again.".into());
                        retry_available.set(true);
                    }
                }
                Ok(PreviewOutcome::UnresolvedProfile) => {
                    phase.set(crate::pages::recipe_process::EstimatePhase::Unavailable);
                    message.set(
                        "Equipment profile unavailable. Use draft settings or select a valid profile."
                            .into(),
                    )
                }
                Ok(PreviewOutcome::UnresolvedStyle) => {
                    phase.set(crate::pages::recipe_process::EstimatePhase::Unavailable);
                    message.set(
                        "Style reference unavailable. Choose a supported style or clear it.".into(),
                    )
                }
                Ok(PreviewOutcome::ContextChanged) => {
                    access.set(EditorAccess::Mismatch);
                    context_visible.set(false)
                }
                Ok(PreviewOutcome::Forbidden) => {
                    access.set(EditorAccess::Forbidden);
                    context_visible.set(false)
                }
                Ok(PreviewOutcome::Invalid) => {
                    phase.set(crate::pages::recipe_process::EstimatePhase::InvalidInput);
                    message.set("Invalid draft. Correct the inputs before previewing.".into())
                }
                Err(e) if crate::utils::auth_refresh::is_auth_error(&e.to_string()) => {
                    access.set(EditorAccess::Expired);
                    context_visible.set(false)
                }
                _ => {
                    phase.set(crate::pages::recipe_process::EstimatePhase::Unavailable);
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
    // The first estimate after a passing re-check has settled (accepted, or invalid or unavailable;
    // `phase` stays Calculating while an accepted estimate is showing): the held editor height ends.
    Effect::new(move |_| {
        if context_visible.get() && (accepted.get().is_some() || phase.get() != crate::pages::recipe_process::EstimatePhase::Calculating) { height_hold.set(None); }
    });
    let settled_message = Signal::derive(move || {
        let text = message.get();
        if draft.with(|d| d.generation) != message_generation.get()
            || matches!(text.as_str(), "Calculating…" | "Awaiting preview.") { String::new() } else { text }
    });
    // The last accepted estimate keeps the reference rows (labels, style bands,
    // source and assumptions) in place while a replacement is calculated, so the
    // phone summary keeps one height. Everything derived from that estimate sits
    // in one stale region: dimmed, hidden from assistive technology and inert
    // while it is out of date, with dashes once the draft settles without one.
    let last_preview = RwSignal::new(None::<Preview>);
    Effect::new(move |_| {
        if let Some(next) = accepted.get() {
            last_preview.set(Some(next));
        }
    });
    // A retained estimate describes one draft document. A restore or a saved-recipe
    // replacement changes the draft epoch, so the old document's rows must not be
    // shown as the new draft's reference (UX-007).
    let draft_epoch = Memo::new(move |_| draft.with(|d| d.epoch()));
    Effect::new(move |_| {
        draft_epoch.get();
        last_preview.set(None);
    });
    let current_phase = Signal::derive(move || if draft.with(|d| d.generation) == message_generation.get() { phase.get() } else { crate::pages::recipe_process::EstimatePhase::Calculating });
    provide_context(crate::pages::recipe_process::AcceptedPreview {
        preview: accepted,
        phase: current_phase,
        visible: context_visible.into(),
    });
    view! { <div class="recipe-editor__preview-summary" class:recipe-editor__preview-summary--stale=move || accepted.get().is_none()><BreweryPanel title="The beer, at a glance" eyebrow="Your formulation" description="Estimates use the current draft." mobile_compact=true presentation=crate::components::PanelPresentation::Summary>
        <div class="recipe-editor__estimate-outcome">
            <p>{move || if draft.with(|d| d.generation) == message_generation.get() && message.get() == "Calculating…" { "Calculating…" } else { "" }}</p>
            <p role="status">{move || settled_message.get()}</p>
            <Show when=move || retry_available.get() && context_visible.get()>
                <Button variant=ButtonVariant::Secondary on:click=move |_| retry_generation.update(|generation| *generation = generation.checked_add(1).expect("preview retry generation exhausted"))>
                    <span class="recipe-editor__estimate-retry-wide">"Retry calculation"</span>
                    <span class="recipe-editor__estimate-retry-compact">"Retry"<span class="sr-only">" calculation"</span></span>
                </Button>
            </Show>
        </div>
        <crate::components::BreweryBeerPortrait/>
        {move || {
            let reference = accepted.get().or(last_preview.get());
            let current = accepted.get().is_some();
            // Values stay only while current or while their replacement is calculated.
            let show_values = current || (reference.is_some() && current_phase.get() == crate::pages::recipe_process::EstimatePhase::Calculating);
            let selected_style = draft.with(|d| d.document().style.clone());
            // The retained bands only describe the retained estimate when the same
            // style is still selected; a different style gets its own reserved rows.
            let reference_style_current = selected_style.as_ref().is_some_and(|selected| {
                reference.as_ref().and_then(|preview| preview.style.as_ref()).is_some_and(|ranges| ranges.name == selected.name)
            });
            let metrics = view! { <PreviewMetrics preview=reference.clone() current=show_values style_selected=selected_style.is_some() reference_style_current=reference_style_current/> }.into_any();
            let source = match reference {
                Some(preview) => view! { <PreviewSource preview=preview draft=draft revision=revision/> }.into_any(),
                None => view! {
                    <p class="note-caption recipe-editor__provenance recipe-editor__provenance--source" data-testid="estimate-source">"No estimate yet."</p>
                    <BreweryDisclosure title="Style reference and assumptions" compact=true>{""}</BreweryDisclosure>
                }.into_any(),
            };
            // One owner for everything read from the last estimate. Before any
            // estimate exists this is the inert placeholder that reserves the rows.
            if current {
                view! { <div class="recipe-editor__estimate-content">{metrics}{source}</div> }.into_any()
            } else {
                view! { <div class="recipe-editor__estimate-content"><BreweryStale stale=Signal::derive(|| true)>{metrics}{source}</BreweryStale></div> }.into_any()
            }
        }}
        <crate::recipe_diagnostics::DiagnosticSummary draft=draft/>
    </BreweryPanel></div>{children()}<div class="recipe-editor__preview-details">{move || accepted.get().map(|preview| view! { <PreviewDetails preview=preview/> })}</div> }
}

#[component]
fn PreviewMetrics(
    preview: Option<krausen_types::formulation::Preview>,
    /// Show the values in `preview`; false renders a dash in every cell (the
    /// draft settled without an estimate or none has arrived). The caller keeps
    /// the labels and style reference rows either way so the phone summary keeps
    /// one height.
    current: bool,
    /// A style is selected, so its band rows must be reserved even before the
    /// reference values are known.
    style_selected: bool,
    /// The retained estimate's bands describe the style still selected. A changed
    /// style reserves its own rows instead of showing the old limits.
    reference_style_current: bool,
) -> impl IntoView {
    let style = if reference_style_current { preview.as_ref().and_then(|preview| preview.style.as_ref()) } else { None };
    let comparison = if reference_style_current { preview.as_ref().and_then(|preview| preview.style_comparison.as_ref()) } else { None };
    // The fermenter batch is the draft that was actually calculated; it is a
    // different quantity from total water, which belongs to the water plan. The
    // preview's context repeats the submitted value even when the validator
    // rejected it, so presentation follows the validator's verdict (UX-007).
    let fermenter = current
        .then(|| preview.as_ref().and_then(|preview| crate::recipe_diagnostics::estimated_batch_volume(&preview.validation, preview.context.as_ref().and_then(|context| context.draft.batch_size_l))))
        .flatten();
    let value = |pick: fn(&krausen_types::formulation::Targets) -> Option<f64>| {
        current.then(|| preview.as_ref().and_then(|preview| pick(&preview.targets))).flatten()
    };
    view! { <div class="recipe-editor__metrics">
        {preview_metric_with_style("Original gravity", value(|t| t.original_sg), style.map(|s| &s.original_sg), comparison.and_then(|c| c.original_sg), "SG", 3, crate::components::MetricDensity::Headline, style_selected)}
        {preview_metric_with_style("Final gravity", value(|t| t.final_sg), style.map(|s| &s.final_sg), comparison.and_then(|c| c.final_sg), "SG", 3, crate::components::MetricDensity::Headline, style_selected)}
        {preview_metric_with_style("Alcohol", value(|t| t.abv_percent), style.map(|s| &s.abv_percent), comparison.and_then(|c| c.abv_percent), "%", 1, crate::components::MetricDensity::Headline, style_selected)}
        {preview_metric_with_style("Bitterness", value(|t| t.ibu), style.map(|s| &s.ibu), comparison.and_then(|c| c.ibu), "IBU", 1, crate::components::MetricDensity::Headline, style_selected)}
        {preview_metric_with_style("Colour", value(|t| t.colour_ebc), style.map(|s| &s.colour_ebc), comparison.and_then(|c| c.colour_ebc), "EBC", 1, crate::components::MetricDensity::Headline, style_selected)}
        {preview_metric("Into fermenter", fermenter, None, None, "L", 1, crate::components::MetricDensity::Headline)}
    </div> }
}

/// Which draft and equipment these estimates describe, kept out of the headline
/// numbers but one disclosure away (UX-007).
fn estimate_source(dirty: bool, revision: Option<i64>) -> String {
    match (dirty, revision) {
        (true, Some(n)) => format!("Current draft, with changes not yet in saved revision {n}."),
        (false, Some(n)) => format!("Current draft, matching saved revision {n}."),
        (_, None) => "Current draft, not saved yet.".to_owned(),
    }
}

fn effective_equipment(context: &krausen_types::formulation::CalculationContext) -> String {
    match context.equipment_profile.as_ref() {
        Some(profile) => format!("Equipment: profile “{}”, revision {}.", profile.name, profile.revision),
        None => "Equipment: recipe-only settings.".to_owned(),
    }
}

#[component]
fn PreviewSource(
    preview: krausen_types::formulation::Preview,
    draft: RwSignal<RecipeDraft>,
    revision: RwSignal<Option<i64>>,
) -> impl IntoView {
    let equipment = preview.context.as_ref().map(effective_equipment);
    let assumptions = preview.context.as_ref().map(|context| context.assumptions.iter().filter_map(|value| match value.as_str() {
        "volume_reference_20c" => Some("Volumes are referenced to 20 °C."),
        "solid_addition_volume_negligible" => Some("Solid additions have negligible volume."),
        "kettle_top_up_default_zero" => Some("No kettle top-up is assumed."),
        "whirlpool_duration_default_zero" => Some("No whirlpool duration is assumed."),
        _ => None,
    }).collect::<Vec<_>>()).unwrap_or_default();
    let style = preview.style.as_ref().map(|style| format!("Style reference: {} · {} · {}", style.name, style.style_id, style.edition));
    view! {
        // Always visible and quiet: which draft these numbers describe. The
        // equipment identity is separate so phones can keep this line at a fixed
        // two-line height while the header badge and equipment panel still name
        // the effective profile (KRA-49: no height change on the first keystroke).
        <p class="note-caption recipe-editor__provenance recipe-editor__provenance--source" data-testid="estimate-source">{move || estimate_source(draft.with(|d| d.dirty), revision.get())}{equipment.map(|text| view! { <span class="recipe-editor__provenance-equipment">{format!(" {text}")}</span> })}</p>
        {(style.is_some() || !assumptions.is_empty()).then(|| view! { <BreweryDisclosure title="Style reference and assumptions" compact=true>
            {style.map(|text| view! { <p class="recipe-editor__provenance">{text}</p> })}
            {(!assumptions.is_empty()).then(|| view! { <ul>{assumptions.into_iter().map(|text| view! { <li>{text}</li> }).collect_view()}</ul> })}
        </BreweryDisclosure> })}
    }
}

#[component]
fn PreviewDetails(preview: krausen_types::formulation::Preview) -> impl IntoView {
    use crate::components::{BreweryMetricList, MetricDensity::Quiet};
    view! {
    {preview.context.as_ref().map(|context| {
        let equipment = context.effective_equipment.clone();
        let name = context.equipment_profile.as_ref().map(|profile| format!("{} · revision {}", profile.name, profile.revision)).unwrap_or_else(|| "Recipe equipment settings".to_owned());
        view! { <BreweryPanel title="Your equipment" description=name>
            <BreweryMetricList>
                {preview_metric("Mash efficiency", equipment.efficiency_override_fraction.or(equipment.mash_efficiency_fraction).map(|value| value * 100.0), None, None, "%", 1, Quiet)}
                {preview_metric("Boil-off", equipment.boil_loss_l_per_hour, None, None, "L/h", 1, Quiet)}
                {preview_metric("Transfer loss", equipment.trub_and_chiller_loss_l, None, None, "L", 1, Quiet)}
            </BreweryMetricList>
            <p>"New brews use current settings; previous brews retain their original equipment."</p>
        </BreweryPanel> }
    })}
    }
}
pub(crate) fn preview_metric(
    label: &'static str,
    value: Option<f64>,
    range: Option<&krausen_types::formulation::Range>,
    comparison: Option<krausen_types::formulation::RangeStatus>,
    unit: &'static str,
    digits: usize,
    density: crate::components::MetricDensity,
) -> AnyView {
    preview_metric_with_style(label, value, range, comparison, unit, digits, density, false)
}

/// The summary variant: when a style is selected but its reference values are not
/// known yet, reserve the band rows with the shared placeholder so the first
/// estimate fills them in place. Water and equipment metrics never carry bands.
fn preview_metric_with_style(
    label: &'static str,
    value: Option<f64>,
    range: Option<&krausen_types::formulation::Range>,
    comparison: Option<krausen_types::formulation::RangeStatus>,
    unit: &'static str,
    digits: usize,
    density: crate::components::MetricDensity,
    style_selected: bool,
) -> AnyView {
    use krausen_types::formulation::RangeStatus;
    let formatted = value
        .map(|n| format!("{n:.digits$}"))
        .unwrap_or_else(|| "—".into());
    let range_view = match range {
        Some(r) => {
            let state = match comparison { Some(RangeStatus::Below) => crate::components::StyleState::Below, Some(RangeStatus::Within) => crate::components::StyleState::Within, Some(RangeStatus::Above) => crate::components::StyleState::Above, None => crate::components::StyleState::Awaiting };
            Some(view! { <crate::components::StyleRange label=label min=r.min max=r.max value=value state=state unit=unit digits=digits/> }.into_any())
        }
        None if style_selected => Some(view! { <BreweryStyleRangePlaceholder/> }.into_any()),
        None => None,
    };
    view! { <crate::components::BreweryMetric label=label value=formatted unit=unit density=density>{range_view}</crate::components::BreweryMetric> }.into_any()
}
