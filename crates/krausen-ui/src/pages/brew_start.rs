// SPDX-License-Identifier: AGPL-3.0-or-later
//! Start brew (INT-005): a bounded review of one saved source, then one explicit
//! commit over the shared atomic start operation. Preparation is automatic and
//! read-only; the original request survives dismissal, navigation and reload
//! (INT-004) through `start_recovery`, and every known result stays on screen
//! until it is acknowledged.
use crate::{
    components::{
        BreweryDialog, BreweryDisclosure, BreweryMetric, BreweryMetricList, BreweryNotice, Button,
        ButtonLink, ButtonVariant, MetricDensity,
        layout::{EditorGuard, GuardScope, GuardTaskCallbacks, GuardTaskKind, GuardTaskState},
        printable_recipe::trimmed,
    },
    pages::{
        recipe_editor::current_context_matches,
        recipe_equipment::{effective_lines, override_count},
        start_recovery,
    },
    server_fns::{
        brew::start_brew_shared, process_summary::request_process_summary_shared,
        recipe_library::library_command_shared,
    },
};
use krausen_types::{
    brew::{StartBrewOutcome, StartBrewRequest},
    brewery::{BrewStatusV1, EquipmentSettingsV1},
    formulation::Targets,
    process_summary::{
        ProcessSummary, ProcessSummaryOutcome, ProcessSummaryRequest, ProcessSummarySelection,
        SummarySource,
    },
    recipe_library::{LibraryCommand, LibraryOutcome, RevisionQuery, RevisionSummary},
    request_context::OperationIntent,
    validation::{Diagnostic, Readiness},
};
use leptos::prelude::*;

#[derive(Clone, Debug, PartialEq)]
struct StartedView {
    brew_id: String,
    selected_revision: i64,
    revision: i64,
    status: BrewStatusV1,
    targets: Targets,
}

/// Which source choices INT-005 requires before any review.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceChoice {
    /// Nothing saved yet: Save recipe and continue / Cancel.
    New,
    /// Unsaved edits: Save changes and continue / Use saved revision N / Cancel.
    Dirty,
    /// Clean saved head: review it directly.
    Clean,
}

fn source_choice(recipe_saved: bool, dirty: bool) -> SourceChoice {
    match (recipe_saved, dirty) {
        (false, _) => SourceChoice::New,
        (true, true) => SourceChoice::Dirty,
        (true, false) => SourceChoice::Clean,
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Phase {
    Idle,
    Choose(SourceChoice),
    Saving,
    Checking,
    /// The read-only review. `head` is the head the review was prepared on.
    Review {
        head: Option<i64>,
    },
    /// Not startable (Incomplete, Invalid, unavailable); nothing can be committed.
    Blocked(String),
    /// Head or full calculation context changed; a fresh review must be accepted.
    Changed(String),
    Starting,
    /// Result unknown; the original request is retained and replayable.
    Unknown(String),
    /// Retained request that cannot be replayed or identified: no new start until
    /// the user explicitly resolves it.
    Stuck(String),
    Started(StartedView),
    /// A brew already exists for the original request (its response expired).
    Existing(String),
}

impl Phase {
    /// The original source and request stay pinned while these are active.
    fn pinned(&self) -> bool {
        matches!(
            self,
            Phase::Starting
                | Phase::Unknown(_)
                | Phase::Stuck(_)
                | Phase::Started(_)
                | Phase::Existing(_)
        )
    }

    fn key(&self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Choose(_) => "choose",
            Phase::Saving => "saving",
            Phase::Checking => "checking",
            Phase::Review { .. } => "review",
            Phase::Blocked(_) => "blocked",
            Phase::Changed(_) => "changed",
            Phase::Starting => "starting",
            Phase::Unknown(_) => "unknown",
            Phase::Stuck(_) => "stuck",
            Phase::Started(_) => "started",
            Phase::Existing(_) => "existing",
        }
    }
}

/// What a typed start outcome means for the retained request and the task.
#[derive(Clone, Debug, PartialEq)]
enum Resolution {
    Started,
    /// A brew exists for the original request (expired response with its id).
    Existing(String),
    Changed(String),
    Blocked(String),
    /// Result not known: the original request stays retained and replayable.
    Unknown(String),
    /// Retained, but replay cannot resolve it: a brew may already exist.
    Stuck(String),
    /// The access context was lost or denied; the request stays retained.
    Hidden,
}

impl Resolution {
    /// Only a known result releases the retained request.
    fn releases_request(&self) -> bool {
        matches!(
            self,
            Resolution::Started
                | Resolution::Existing(_)
                | Resolution::Changed(_)
                | Resolution::Blocked(_)
        )
    }
}

fn diagnostic_text(diagnostic: &Diagnostic) -> String {
    crate::components::printable_recipe::diagnostic_message(diagnostic)
}

fn resolution(result: &Result<StartBrewOutcome, ServerFnError>) -> Resolution {
    use StartBrewOutcome as O;
    match result {
        Ok(O::Started { .. }) => Resolution::Started,
        Ok(O::HeadConflict { expected_head, actual_head, .. }) => Resolution::Changed(format!(
            "The saved recipe moved from revision {expected_head} to {actual_head}. No brew was started. Review the current revision before starting."
        )),
        Ok(O::ContextConflict { .. }) => Resolution::Changed(
            "The equipment or calculation context changed since this review. No brew was started. Review the refreshed targets, then start explicitly."
                .into(),
        ),
        Ok(O::NotReady(validation)) => Resolution::Blocked(format!(
            "Not started: {}",
            validation
                .diagnostics
                .first()
                .map(diagnostic_text)
                .unwrap_or_else(|| "the recipe is not Ready.".into())
        )),
        Ok(O::RecipeArchived) => Resolution::Blocked("This recipe is archived and cannot start a new brew.".into()),
        Ok(O::EquipmentProfileArchived) => Resolution::Blocked("The selected equipment profile is archived.".into()),
        Ok(O::UnresolvedEquipmentProfile) => Resolution::Blocked("The selected equipment profile is unavailable.".into()),
        Ok(O::UnresolvedStyle) => Resolution::Blocked("The selected style reference is unavailable.".into()),
        Ok(O::NotFound) => Resolution::Blocked("That saved revision is not available in this workspace.".into()),
        Ok(O::InvalidRequest) => Resolution::Blocked("The start request was invalid.".into()),
        Ok(O::RequestIdentityConflict) => Resolution::Stuck(
            "The original start request could not be verified, so a brew may already exist. Check this recipe's Brews before starting again."
                .into(),
        ),
        Ok(O::ResultExpired { resource_id: Some(id) }) => Resolution::Existing(id.clone()),
        Ok(O::ResultExpired { resource_id: None }) => Resolution::Stuck(
            "The saved start response expired, so a brew may already exist. Check this recipe's Brews before starting again."
                .into(),
        ),
        Ok(O::Forbidden | O::ContextChanged) => Resolution::Hidden,
        Ok(O::Retryable | O::Unavailable) => {
            Resolution::Unknown("Start is temporarily unavailable. The result is unconfirmed.".into())
        }
        Err(error) if crate::utils::auth_refresh::is_auth_error(&error.to_string()) => Resolution::Unknown(
            "Your sign-in expired. Sign in again; the original request is retained and its result is unconfirmed."
                .into(),
        ),
        Err(_) => Resolution::Unknown("The start result is unconfirmed.".into()),
    }
}

/// An unresolved retained request may take over only a task that has not
/// dispatched anything: never a live request, a result owed to the user or an
/// earlier takeover, and never a check that is mid-flight.
fn may_take_over(phase: &Phase, has_pending: bool) -> bool {
    !has_pending && matches!(phase, Phase::Idle | Phase::Choose(_) | Phase::Review { .. })
}

/// `Some(wanted)` when the requested source differs from the one this task is
/// reviewing or pinned to (`wanted` is `None` for the current saved head).
fn requested_source_change(
    requested: Option<&RevisionSummary>,
    current: Option<&RevisionSummary>,
) -> Option<Option<i64>> {
    let wanted = requested.map(|r| r.revision);
    (wanted != current.map(|c| c.revision)).then_some(wanted)
}

fn source_unavailable_message(wanted: Option<i64>) -> String {
    match wanted {
        Some(revision) => format!(
            "Starting revision {revision} is unavailable until the previous result is resolved."
        ),
        None => "Starting the current saved revision is unavailable until the previous result is resolved.".into(),
    }
}

/// Inside the cross-tab lock any other unresolved request, older or not, wins:
/// ordering of timestamps and writes then cannot matter. `mine` is `None` before
/// this task has retained anything.
fn must_yield(unresolved: &[StartBrewRequest], mine: Option<&StartBrewRequest>) -> bool {
    unresolved
        .iter()
        .any(|other| mine.is_none_or(|mine| other.request_id != mine.request_id))
}

/// Longest wait for the in-lock access confirmation before treating it as failed.
/// Invariant: strictly shorter than the cross-tab lock wait (`LOCK_WAIT_MS`, 10 s),
/// so a waiting tab never gives up and goes lock-free while the holder is still
/// inside its critical section.
const ACCESS_CHECK_MS: u32 = 5_000;

/// Shown when access could not be confirmed inside the start critical section:
/// nothing had been retained, so "nothing was started" is true.
const ACCESS_UNCONFIRMED: &str =
    "Access could not be confirmed before sending. Nothing was started.";

/// While access is hidden, only this exact message may replace the phase copy;
/// any other leftover message is unrelated and must not be shown there.
fn hidden_review_note<'a>(phase: &Phase, message: &'a str) -> Option<&'a str> {
    (matches!(phase, Phase::Review { .. }) && message == ACCESS_UNCONFIRMED).then_some(message)
}

const YIELDED: &str =
    "Another start for this recipe was just handled. Review again before starting.";

/// Start needs a Ready review and no other unresolved request for this task.
fn may_start(phase: &Phase, has_pending: bool) -> bool {
    !has_pending && matches!(phase, Phase::Review { .. })
}

/// One reason per distinct diagnostic. Identity is the structured (path, code), so
/// two rows with the same problem stay two reasons; message text never decides it.
fn distinct_reasons(diagnostics: &[Diagnostic]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    diagnostics
        .iter()
        .filter(|d| seen.insert((d.path.as_str(), d.code.as_str())))
        .map(diagnostic_text)
        .collect()
}

/// What the dialog says about itself in each state; the static review wording was
/// wrong once a start had been sent or had finished.
fn dialog_description(phase: &Phase, access_visible: bool) -> &'static str {
    // While access is being rechecked the body shows only that notice, so the
    // description must not promise a review or a start either.
    if !access_visible {
        return "Access to this workspace is being rechecked.";
    }
    match phase {
        Phase::Idle | Phase::Checking | Phase::Review { .. } => {
            "Review the saved recipe, its equipment and its targets, then start explicitly."
        }
        Phase::Choose(SourceChoice::New) => {
            "Save this recipe first; a brew starts from a saved revision."
        }
        Phase::Choose(_) => "A brew starts from a saved revision; choose which one to review.",
        Phase::Saving => "Saving your recipe before the review.",
        Phase::Blocked(_) => "This revision cannot be started yet.",
        Phase::Changed(_) => "Something changed since this review, so it must be reviewed again.",
        Phase::Starting => "The start request was sent. Closing this does not cancel it.",
        Phase::Unknown(_) | Phase::Stuck(_) => "The result of the start request is not yet known.",
        Phase::Started(_) | Phase::Existing(_) => "The brew exists. Open it, or close this dialog.",
    }
}

/// Historical revisions start without the head guard; the head revision is
/// guarded so a moved head is reported instead of silently started.
fn request_head_guard(historical: bool, selected: i64) -> Option<i64> {
    (!historical).then_some(selected)
}

fn build_start_request(
    intent: OperationIntent,
    recipe_id: String,
    selected: i64,
    historical: bool,
    digest: Option<String>,
    request_id: String,
) -> Option<StartBrewRequest> {
    Some(StartBrewRequest {
        intent,
        recipe_id,
        selected_recipe_revision: selected,
        expected_head_revision: request_head_guard(historical, selected),
        expected_calculation_context: digest?,
        request_id,
    })
}

/// The source an original request pinned, for display while it is unresolved.
fn pinned_source_text(request: &StartBrewRequest, hint: Option<&RevisionSummary>) -> String {
    let revision = request.selected_recipe_revision;
    match request.expected_head_revision {
        Some(_) => format!("Revision {revision} · the saved head when you started"),
        None => format!(
            "{} · an older saved version, started without restoring it",
            revision_label(revision, hint.filter(|h| h.revision == revision))
        ),
    }
}

fn metric(label: &'static str, value: Option<f64>, unit: &'static str, digits: usize) -> AnyView {
    let formatted = value
        .map(|value| format!("{value:.digits$}"))
        .unwrap_or_else(|| "—".into());
    view! { <BreweryMetric label=label value=formatted unit=unit density=MetricDensity::Quiet>{""}</BreweryMetric> }.into_any()
}

/// One kit fact row (label left, value right), for use inside a `BreweryMetricList`.
fn fact(label: &'static str, value: String, testid: &'static str) -> AnyView {
    view! { <BreweryMetric label=label value=value density=MetricDensity::Quiet testid=testid>{""}</BreweryMetric> }.into_any()
}

/// Accepted targets as the kit's quiet metric rows, so the list can scroll between
/// rows instead of cutting a headline metric in half at narrow widths.
fn target_metrics(targets: &Targets) -> AnyView {
    // Rows are built first: the list's children must own their data.
    let rows = [
        metric("Original gravity", targets.original_sg, "SG", 3),
        metric("Final gravity", targets.final_sg, "SG", 3),
        metric("Alcohol", targets.abv_percent, "%", 1),
        metric("Bitterness", targets.ibu, "IBU", 1),
        metric("Colour", targets.colour_ebc, "EBC", 1),
        metric("Total water", targets.total_water_l, "L", 1),
    ];
    view! { <BreweryMetricList>{rows.into_iter().collect_view()}</BreweryMetricList> }.into_any()
}

/// A readable timestamp: `2026-11-16 23:54 UTC` from an RFC 3339 value; any other
/// text is shown as given. A non-UTC offset is shown as the stored wall-clock time
/// with no suffix (only `+00:00` is emitted by the backend today).
fn short_date(saved_at: &str) -> String {
    let utc = saved_at.ends_with('Z') || saved_at.ends_with("+00:00");
    match (saved_at.get(..10), saved_at.get(11..16)) {
        (Some(day), Some(time)) if saved_at.as_bytes().get(10) == Some(&b'T') && utc => {
            format!("{day} {time} UTC")
        }
        (Some(day), Some(time)) if saved_at.as_bytes().get(10) == Some(&b'T') => {
            format!("{day} {time}")
        }
        _ => saved_at.to_owned(),
    }
}

/// Heuristic, not a contract: the history query returns `recipe_revisions.saved_by`,
/// which today is always the saving account's id (`user-…`), never a display name,
/// and this client has no way to resolve it. An id is not a recognisable author, so
/// it is hidden; a value that does not look like an id is shown as given. The
/// `user-` prefix is a deliberate heuristic mirroring the id format generated in
/// `crates/krausen-auth/src/user_service.rs` (`format!("user-{}", …)`).
fn readable_author(saved_by: &str) -> Option<&str> {
    (!saved_by.is_empty() && !saved_by.starts_with("user-")).then_some(saved_by)
}

fn revision_label(revision: i64, hint: Option<&RevisionSummary>) -> String {
    match hint {
        Some(h) => {
            let mut parts = vec![format!("Revision {revision}")];
            if let Some(date) = &h.saved_at {
                parts.push(short_date(date));
            }
            if let Some(by) = h.saved_by.as_deref().and_then(readable_author) {
                parts.push(format!("by {by}"));
            }
            if let Some(from) = h.restored_from_revision {
                parts.push(format!("restored from revision {from}"));
            }
            parts.join(" · ")
        }
        None => format!("Revision {revision}"),
    }
}

/// Visible equipment review: identity, fermenter volume and every recipe
/// override first; the full effective values follow in a disclosure.
fn equipment_view(summary: &ProcessSummary, overrides: &EquipmentSettingsV1) -> AnyView {
    let equipment = &summary.equipment;
    let has_profile = equipment.profile_id.is_some();
    let identity = match (&equipment.profile_name, equipment.profile_revision) {
        (Some(name), Some(revision)) => format!("{name} · profile revision {revision}"),
        (Some(name), None) => name.clone(),
        _ if has_profile => "Selected equipment profile".into(),
        _ => "Recipe-only equipment settings (no saved profile)".into(),
    };
    let fermenter = summary
        .batch_size_l
        .map(|v| format!("{} L into the fermenter", trimmed(v, 2)))
        .unwrap_or_else(|| "Not specified".into());
    let lines = effective_lines(&equipment.settings, overrides, has_profile);
    let overridden: Vec<String> = lines
        .iter()
        .filter(|(_, o)| *o)
        .map(|(l, _)| l.clone())
        .collect();
    let overrides_text = match override_count(overrides) {
        0 => "No recipe overrides".to_owned(),
        1 => "1 recipe override".to_owned(),
        n => format!("{n} recipe overrides"),
    };
    let all: Vec<String> = lines.into_iter().map(|(l, _)| l).collect();
    let override_rows: Vec<(String, String)> = overridden
        .into_iter()
        .map(|row| match row.split_once(": ") {
            Some((label, value)) => (label.to_owned(), value.to_owned()),
            None => (row.clone(), String::new()),
        })
        .collect();
    view! {
        <BreweryMetricList>
            {fact("Equipment", identity, "start-equipment")}
            {fact("Fermenter batch", fermenter, "start-fermenter")}
            {fact("Overrides", overrides_text, "start-overrides")}
        </BreweryMetricList>
        {(!override_rows.is_empty()).then(|| view! {
            <div data-testid="start-override-list">
                <BreweryMetricList>
                    {override_rows.into_iter().map(|(label, value)| view! {
                        <BreweryMetric label=label value=value density=MetricDensity::Quiet>{""}</BreweryMetric>
                    }).collect_view()}
                </BreweryMetricList>
            </div>
        })}
        {(!all.is_empty()).then(|| view! {
            <BreweryDisclosure title="Effective equipment settings">
                <ul>{all.into_iter().map(|row| view! { <li>{row}</li> }).collect_view()}</ul>
            </BreweryDisclosure>
        })}
    }
    .into_any()
}

/// The Start brew task. Its state is owned by the editor lifetime, so closing
/// the dialog never discards an in-flight, unconfirmed or unacknowledged start.
#[component]
pub fn StartBrewTask(
    open: RwSignal<bool>,
    /// A chosen saved version (history row or the inspected `?revision=`) instead
    /// of the current head. Ignored while an earlier request is unresolved.
    historical: RwSignal<Option<RevisionSummary>>,
    identity: RwSignal<OperationIntent>,
    context_visible: RwSignal<bool>,
    recipe_id: RwSignal<Option<String>>,
    /// The current saved head revision.
    revision: RwSignal<Option<i64>>,
    dirty: Signal<bool>,
    /// True while the editor's own Save is in flight.
    saving: Signal<bool>,
    /// Editor status text (save failures) shown when Save and continue falls back.
    save_status: Signal<String>,
    /// Dispatches the editor's Save; returns whether a save was actually sent.
    request_save: Callback<(), bool>,
) -> impl IntoView {
    let phase = RwSignal::new(Phase::Idle);
    let summary = RwSignal::new(None::<ProcessSummary>);
    let overrides = RwSignal::new(EquipmentSettingsV1::default());
    let pending = RwSignal::new(None::<StartBrewRequest>);
    let message = RwSignal::new(String::new());
    let refreshed = RwSignal::new(None::<Targets>);
    let generation = RwSignal::new(0_u64);
    // The source this task is reviewing; the `historical` input only requests one.
    let chosen = RwSignal::new(None::<RevisionSummary>);
    let pinned_source = RwSignal::new(String::new());
    // A result that arrived but has not been acknowledged yet.
    let unacknowledged = RwSignal::new(false);
    // The review generation whose start is being serialised and dispatched, if any.
    // It blocks a second click, and a reopened review has a newer generation, so a
    // stale entry can never disable it.
    let dispatching = RwSignal::new(None::<u64>);
    let is_dispatching = move || {
        dispatching
            .get()
            .is_some_and(|dispatched| dispatched == generation.get())
    };
    // The head a Changed result reported, so Review again starts from it.
    let renew_head = RwSignal::new(None::<i64>);
    let installation = GuardScope::installation();
    let editor_guard = expect_context::<EditorGuard>();
    let guard_scope = move || {
        let source = identity.get_untracked();
        GuardScope {
            installation: GuardScope::installation(),
            account_id: source.account_id,
            workspace_id: source.workspace_id,
            resource_id: recipe_id.get_untracked(),
            base_revision: revision.get_untracked(),
        }
    };
    let guard_task = editor_guard.register_task(
        GuardTaskKind::Start,
        guard_scope(),
        GuardTaskCallbacks {
            save_and_leave: None,
            return_to_actions: None,
            discard_unsent: Callback::new(move |()| true),
        },
    );
    on_cleanup(move || guard_task.unregister());
    // Only an unresolved dispatch blocks leaving, and its record is retained so
    // leaving never cancels it. A read-only review has nothing to protect.
    Effect::new(move |_| {
        let current = phase.get();
        guard_task.update(GuardTaskState {
            dirty: false,
            // Constant: a moving generation would invalidate other tasks' leave leases.
            generation: 0,
            in_flight: matches!(current, Phase::Starting),
            uncertain: matches!(current, Phase::Unknown(_) | Phase::Stuck(_)),
            // A load-error Stuck has no replayable request but its record is
            // retained all the same, so leaving is allowed and keeps it.
            recovery_retained: pending.with(|p| p.is_some()) || matches!(current, Phase::Stuck(_)),
            context_visible: context_visible.get(),
        });
    });

    // A dialog opened while the page is still hydrating can be reset by the
    // hydration; re-assert the request until the native dialog is really open.
    #[cfg(target_arch = "wasm32")]
    let ensure_open = move || {
        leptos::task::spawn_local(async move {
            use wasm_bindgen::JsCast;
            for delay in [150_u32, 400, 1_000, 2_500] {
                gloo_timers::future::TimeoutFuture::new(delay).await;
                if open.try_get_untracked() != Some(true) {
                    return;
                }
                let shown = web_sys::window()
                    .and_then(|window| window.document())
                    .and_then(|document| document.get_element_by_id("start-brew-dialog"))
                    .and_then(|element| element.dyn_into::<web_sys::HtmlDialogElement>().ok())
                    .is_some_and(|dialog| dialog.open());
                if shown {
                    return;
                }
                open.try_set(false);
                gloo_timers::future::TimeoutFuture::new(0).await;
                open.try_set(true);
            }
        });
    };
    #[cfg(not(target_arch = "wasm32"))]
    let ensure_open = move || {};
    // Loads the oldest unresolved retained request for this recipe, if any, and
    // pins it. Called on mount, after each resolution and on `storage` events.
    let load_pending = Callback::new({
        let installation = installation.clone();
        move |open_dialog: bool| {
            if !may_take_over(&phase.get_untracked(), pending.get_untracked().is_some()) {
                return;
            }
            let Some(id) = recipe_id.get_untracked() else {
                return;
            };
            match start_recovery::load(&installation, &identity.get_untracked(), &id) {
                Ok(found) => {
                    if let Some(request) = found.into_iter().next() {
                        // Any in-flight review is superseded by the unresolved request.
                        generation.update(|g| *g += 1);
                        pinned_source.set(pinned_source_text(
                            &request,
                            chosen.get_untracked().as_ref(),
                        ));
                        pending.set(Some(request));
                        phase.set(Phase::Unknown(
                            "A brew start was sent and its result is unconfirmed.".into(),
                        ));
                        unacknowledged.set(true);
                        if open_dialog {
                            open.set(true);
                            ensure_open();
                        }
                    }
                }
                Err(error) => {
                    generation.update(|g| *g += 1);
                    phase.set(Phase::Stuck(error));
                    unacknowledged.set(true);
                    if open_dialog {
                        open.set(true);
                        ensure_open();
                    }
                }
            }
        }
    });
    // A request retained by an earlier page load reopens the task unresolved.
    Effect::new(move |loaded: Option<bool>| {
        if loaded == Some(true) {
            return true;
        }
        if !context_visible.get() || recipe_id.get().is_none() {
            return false;
        }
        load_pending.run(true);
        true
    });
    #[cfg(target_arch = "wasm32")]
    {
        let listener = window_event_listener(leptos::ev::storage, move |event| {
            if event
                .key()
                .is_none_or(|key| key.starts_with(start_recovery::PREFIX))
            {
                // Another tab's request: surface it without taking the dialog over.
                load_pending.run(false);
            }
        });
        on_cleanup(move || listener.remove());
    }

    let check = Callback::new(move |(selected, is_historical): (i64, bool)| {
        let Some(id) = recipe_id.get_untracked() else {
            return;
        };
        generation.update(|g| *g += 1);
        let this = generation.get_untracked();
        let basis = revision.get_untracked();
        phase.set(Phase::Checking);
        summary.set(None);
        refreshed.set(None);
        let expected = identity.get_untracked();
        leptos::task::spawn_local(async move {
            let ok = current_context_matches(&expected).await;
            if generation.try_get_untracked() != Some(this) {
                return;
            }
            if !ok {
                context_visible.set(false);
                phase.set(Phase::Idle);
                return;
            }
            let build = |recipe_id: String| ProcessSummaryRequest {
                intent: expected.clone(),
                selection: ProcessSummarySelection::Saved {
                    recipe_id,
                    revision: selected,
                },
            };
            let result = request_process_summary_shared(build(id.clone())).await;
            #[cfg(target_arch = "wasm32")]
            let result = if result
                .as_ref()
                .err()
                .is_some_and(|e| crate::utils::auth_refresh::is_auth_error(&e.to_string()))
                && crate::utils::auth_refresh::try_refresh().await
                && current_context_matches(&expected).await
            {
                request_process_summary_shared(build(id.clone())).await
            } else {
                result
            };
            if generation.try_get_untracked() != Some(this) {
                return;
            }
            match result {
                Ok(ProcessSummaryOutcome::Summary(found)) => {
                    let ready =
                        found.readiness == Readiness::Ready && found.context.digest.is_some();
                    if !ready {
                        let reasons = distinct_reasons(&found.diagnostics);
                        phase.set(Phase::Blocked(if reasons.is_empty() {
                            "This revision is not Ready. Add the missing inputs and save a new revision.".into()
                        } else {
                            reasons.join(" ")
                        }));
                        summary.set(Some(found));
                        return;
                    }
                    // Recipe-local overrides are part of the equipment review.
                    let document =
                        library_command_shared(LibraryCommand::Revision(RevisionQuery {
                            intent: expected.clone(),
                            recipe_id: id.clone(),
                            revision: selected,
                        }))
                        .await;
                    if generation.try_get_untracked() != Some(this) {
                        return;
                    }
                    match document {
                        Ok(LibraryOutcome::Revision(document)) => {
                            overrides.set(document.equipment_overrides);
                            summary.set(Some(found));
                            // The saved head moved while this was being checked.
                            if !is_historical
                                && revision.get_untracked() != basis
                                && revision.get_untracked() != Some(selected)
                            {
                                phase.set(Phase::Changed(
                                    "The saved recipe changed while you were reviewing. Review the current revision before starting."
                                        .into(),
                                ));
                            } else {
                                phase.set(Phase::Review {
                                    head: (!is_historical).then_some(selected),
                                });
                            }
                        }
                        _ => phase.set(Phase::Blocked(
                            "Unable to read this revision's equipment overrides. Nothing was started; check again.".into(),
                        )),
                    }
                }
                Ok(ProcessSummaryOutcome::Forbidden | ProcessSummaryOutcome::ContextChanged) => {
                    context_visible.set(false);
                    phase.set(Phase::Idle);
                }
                Ok(ProcessSummaryOutcome::UnresolvedEquipmentProfile) => phase.set(Phase::Blocked(
                    "The selected equipment profile is unavailable.".into(),
                )),
                Ok(ProcessSummaryOutcome::UnresolvedStyle) => phase.set(Phase::Blocked(
                    "The selected style reference is unavailable.".into(),
                )),
                Ok(ProcessSummaryOutcome::UnsupportedRevision { schema_version }) => {
                    phase.set(Phase::Blocked(format!(
                        "Revision uses unsupported recipe schema version {schema_version}."
                    )))
                }
                Ok(ProcessSummaryOutcome::NotFound) => phase.set(Phase::Blocked(
                    "That saved revision is not available in this workspace.".into(),
                )),
                Ok(ProcessSummaryOutcome::Invalid(validation)) => phase.set(Phase::Blocked(
                    validation
                        .diagnostics
                        .first()
                        .map(diagnostic_text)
                        .unwrap_or_else(|| "Correct the recipe before starting.".into()),
                )),
                _ => phase.set(Phase::Blocked(
                    "Unable to check the saved recipe. Nothing was started; check again.".into(),
                )),
            }
        });
    });

    // Opening begins a fresh review unless an original request or an
    // unacknowledged result is still owed to the user.
    let begin = Callback::new(move |()| {
        if !context_visible.get_untracked() {
            return;
        }
        message.set(String::new());
        let current = phase.get_untracked();
        let change = requested_source_change(historical.get().as_ref(), chosen.get().as_ref());
        if current.pinned() || unacknowledged.get_untracked() {
            if let Some(wanted) = change {
                message.set(source_unavailable_message(wanted));
            }
            return;
        }
        if current == Phase::Saving {
            if change.is_some() {
                message.set("Finishing your save first; then choose the version again.".into());
            }
            return;
        }
        renew_head.set(None);
        generation.update(|g| *g += 1);
        chosen.set(historical.get_untracked());
        if let Some(picked) = chosen.get_untracked() {
            check.run((picked.revision, true));
            return;
        }
        match source_choice(
            recipe_id.get_untracked().is_some() && revision.get_untracked().is_some(),
            dirty.get_untracked(),
        ) {
            SourceChoice::Clean => match revision.get_untracked() {
                Some(head) => check.run((head, false)),
                None => phase.set(Phase::Choose(SourceChoice::New)),
            },
            choice => phase.set(Phase::Choose(choice)),
        }
    });
    Effect::new(move |was_open: Option<bool>| {
        let is_open = open.get();
        if is_open && was_open != Some(true) {
            begin.run(());
        }
        if !is_open && was_open == Some(true) {
            // Cancelling an unsent review is not cancelling a commit.
            let current = phase.get_untracked();
            if !current.pinned() && !unacknowledged.get_untracked() && current != Phase::Saving {
                generation.update(|g| *g += 1);
                phase.set(Phase::Idle);
                summary.set(None);
            }
        }
        is_open
    });
    // Access regained while the task is open resumes it instead of sticking.
    Effect::new(move |_| {
        if context_visible.get() && open.get() && phase.get_untracked() == Phase::Idle {
            begin.run(());
        }
    });

    let save_and_continue = Callback::new(move |()| {
        if saving.get_untracked()
            || !context_visible.get_untracked()
            || !matches!(phase.get_untracked(), Phase::Choose(_))
        {
            return;
        }
        phase.set(Phase::Saving);
        if !request_save.run(()) {
            phase.set(Phase::Choose(source_choice(
                recipe_id.get_untracked().is_some(),
                dirty.get_untracked(),
            )));
            message.set(format!(
                "Save could not start. Your changes are unchanged. {}",
                save_status.get_untracked()
            ));
        }
    });
    Effect::new(move |was_saving: Option<bool>| {
        let now = saving.get();
        if was_saving == Some(true) && !now && matches!(phase.get_untracked(), Phase::Choose(_)) {
            begin.run(());
            return now;
        }
        if was_saving == Some(true) && !now && phase.get_untracked() == Phase::Saving {
            if !dirty.get_untracked() && recipe_id.get_untracked().is_some() {
                if let Some(head) = revision.get_untracked() {
                    chosen.set(None);
                    check.run((head, false));
                    return now;
                }
            }
            phase.set(Phase::Choose(source_choice(
                recipe_id.get_untracked().is_some(),
                dirty.get_untracked(),
            )));
            message.set(format!(
                "Your changes were not saved completely, so nothing was reviewed. {}",
                save_status.get_untracked()
            ));
        }
        now
    });
    // The saved head moved while an unpinned review was open: renew, never accept
    // silently. Only a change observed while reviewing counts, not a stale signal.
    Effect::new(move |previous: Option<Option<i64>>| {
        let head = revision.get();
        if let (
            Some(_),
            Phase::Review {
                head: Some(reviewed),
            },
        ) = (previous, phase.get_untracked())
        {
            if head != Some(reviewed) {
                phase.set(Phase::Changed(
                    "The saved recipe changed while you were reviewing. Review the current revision before starting."
                        .into(),
                ));
            }
        }
        head
    });

    // After a known resolution is acknowledged, look for another unresolved record.
    let acknowledge = Callback::new(move |()| {
        unacknowledged.set(false);
        pending.set(None);
        pinned_source.set(String::new());
        message.set(String::new());
        renew_head.set(None);
        phase.set(Phase::Idle);
        summary.set(None);
        load_pending.run(true);
    });

    let send = Callback::new(move |(request, replay): (StartBrewRequest, bool)| {
        phase.set(Phase::Starting);
        message.set(String::new());
        unacknowledged.set(true);
        let installation = GuardScope::installation();
        let expected = request.intent.clone();
        let request_id = request.request_id.clone();
        leptos::task::spawn_local(async move {
            let current = move || {
                pending.try_with_untracked(|p| {
                    p.as_ref().map(|r| r.request_id.as_str()) == Some(request_id.as_str())
                }) == Some(true)
            };
            // A first send was access-checked inside the start critical section;
            // only a replay needs its own check here.
            if replay && !current_context_matches(&expected).await {
                if !current() {
                    return;
                }
                context_visible.set(false);
                phase.set(Phase::Unknown(
                    "Access was rechecked. The original request is retained and its result is still unconfirmed."
                        .into(),
                ));
                return;
            }
            let result = start_brew_shared(request.clone()).await;
            #[cfg(target_arch = "wasm32")]
            let result = if result
                .as_ref()
                .err()
                .is_some_and(|e| crate::utils::auth_refresh::is_auth_error(&e.to_string()))
                && crate::utils::auth_refresh::try_refresh().await
                && current_context_matches(&expected).await
            {
                start_brew_shared(request.clone()).await
            } else {
                result
            };
            if !current() {
                return;
            }
            let resolved = resolution(&result);
            if resolved.releases_request() {
                start_recovery::remove(&installation, &request);
            }
            match (resolved, result) {
                (
                    Resolution::Started,
                    Ok(StartBrewOutcome::Started {
                        brew_id,
                        status,
                        revision,
                        selected_recipe_revision,
                        plan,
                        ..
                    }),
                ) => phase.set(Phase::Started(StartedView {
                    brew_id,
                    selected_revision: selected_recipe_revision,
                    revision,
                    status,
                    targets: plan.original_targets,
                })),
                (Resolution::Existing(id), _) => phase.set(Phase::Existing(id)),
                (Resolution::Changed(text), outcome) => {
                    renew_head.set(match &outcome {
                        Ok(StartBrewOutcome::HeadConflict { actual_head, .. }) => {
                            Some(*actual_head)
                        }
                        _ => None,
                    });
                    refreshed.set(match outcome {
                        Ok(StartBrewOutcome::ContextConflict { refreshed }) => {
                            Some(refreshed.targets)
                        }
                        Ok(StartBrewOutcome::HeadConflict { refreshed, .. }) => {
                            refreshed.map(|p| p.targets)
                        }
                        _ => None,
                    });
                    phase.set(Phase::Changed(text));
                }
                (Resolution::Blocked(text), _) => phase.set(Phase::Blocked(text)),
                (Resolution::Unknown(text), _) => phase.set(Phase::Unknown(text)),
                (Resolution::Stuck(text), _) => phase.set(Phase::Stuck(text)),
                (Resolution::Hidden, _) => {
                    context_visible.set(false);
                    phase.set(Phase::Unknown(
                        "Access changed. The original request is retained and its result is unconfirmed; recheck access to continue."
                            .into(),
                    ));
                }
                (Resolution::Started, _) => {
                    phase.set(Phase::Unknown("The start result is unconfirmed.".into()))
                }
            }
        });
    });
    let start = Callback::new(move |()| {
        if !context_visible.get_untracked()
            || dispatching.get_untracked() == Some(generation.get_untracked())
            || !may_start(&phase.get_untracked(), pending.get_untracked().is_some())
        {
            return;
        }
        let (Some(found), Some(id)) = (summary.get_untracked(), recipe_id.get_untracked()) else {
            return;
        };
        let SummarySource::Saved {
            revision: selected, ..
        } = found.source
        else {
            return;
        };
        let Some(request_id) = crate::utils::request_id::new_request_id() else {
            message.set("Unable to create a secure start request.".into());
            return;
        };
        let picked = chosen.get_untracked();
        let intent = identity.get_untracked();
        let Some(request) = build_start_request(
            intent.clone(),
            id,
            selected,
            picked.is_some(),
            found.context.digest.clone(),
            request_id,
        ) else {
            return;
        };
        let this = generation.get_untracked();
        dispatching.set(Some(this));
        message.set(String::new());
        leptos::task::spawn_local(async move {
            // Check, retain and verify are one critical section per workspace and
            // recipe, so two tabs cannot both conclude that nothing is unresolved.
            let lock = start_recovery::acquire_lock(&intent, &request.recipe_id).await;
            let installation = GuardScope::installation();
            let still_reviewing = move || {
                generation.try_get_untracked() == Some(this)
                    && phase.try_with_untracked(|p| matches!(p, Phase::Review { .. })) == Some(true)
                    && pending.try_with_untracked(|p| p.is_none()) == Some(true)
            };
            let finish = move |lock: start_recovery::StartLock| {
                lock.release();
                if dispatching.try_get_untracked() == Some(Some(this)) {
                    dispatching.try_set(None);
                }
            };
            if !still_reviewing() {
                finish(lock);
                return;
            }
            let yield_to_other = move |own: Option<&StartBrewRequest>, unread: bool| {
                if let Some(own) = own {
                    start_recovery::remove(&GuardScope::installation(), own);
                }
                // A failed or non-empty read lands in the unresolved/Stuck path.
                load_pending.run(true);
                if matches!(phase.get_untracked(), Phase::Review { .. }) {
                    message.set(if unread {
                        "Unable to read start recovery; nothing was started.".into()
                    } else {
                        YIELDED.into()
                    });
                }
            };
            // Confirm access inside the critical section and before anything is
            // retained, so a failure leaves no record for another tab to pick up.
            let checked = intent.clone();
            let access = start_recovery::bounded(ACCESS_CHECK_MS, async move {
                current_context_matches(&checked).await
            })
            .await
            .unwrap_or(false);
            if !still_reviewing() {
                finish(lock);
                return;
            }
            if !access {
                context_visible.try_set(false);
                message.try_set(ACCESS_UNCONFIRMED.into());
                finish(lock);
                return;
            }
            match start_recovery::load(&installation, &intent, &request.recipe_id) {
                Ok(unresolved) if !must_yield(&unresolved, None) => {}
                other => {
                    yield_to_other(None, other.is_err());
                    finish(lock);
                    return;
                }
            }
            if let Err(error) = start_recovery::retain(&installation, &request) {
                message.set(format!("{error}. Nothing was started."));
                finish(lock);
                return;
            }
            match start_recovery::load(&installation, &intent, &request.recipe_id) {
                Ok(unresolved) if !must_yield(&unresolved, Some(&request)) => {}
                other => {
                    yield_to_other(Some(&request), other.is_err());
                    finish(lock);
                    return;
                }
            }
            pinned_source.set(pinned_source_text(&request, picked.as_ref()));
            pending.set(Some(request.clone()));
            send.run((request, false));
            finish(lock);
        });
    });
    // Replays the original request (same identity and payload); never a fresh one.
    let check_previous = Callback::new(move |()| {
        if !context_visible.get_untracked() || !matches!(phase.get_untracked(), Phase::Unknown(_)) {
            return;
        }
        if let Some(original) = pending.get_untracked() {
            // Another tab may have removed the record since this one took it over;
            // retaining the same payload is idempotent and restores it.
            if let Err(error) = start_recovery::retain(&GuardScope::installation(), &original) {
                phase.set(Phase::Stuck(error));
                return;
            }
            send.run((original, true));
        }
    });
    // Explicit resolution of a retained record that cannot be replayed.
    let discard_record = Callback::new({
        let installation = installation.clone();
        move |()| {
            if !matches!(phase.get_untracked(), Phase::Stuck(_)) {
                return;
            }
            if let Some(id) = recipe_id.get_untracked() {
                // Only the identified request, or only the broken entries; a
                // separate readable request is never touched.
                let request_id = pending.get_untracked().map(|p| p.request_id);
                start_recovery::discard(
                    &installation,
                    &identity.get_untracked(),
                    &id,
                    request_id.as_deref(),
                );
            }
            acknowledge.run(());
            if open.get_untracked() && phase.get_untracked() == Phase::Idle {
                begin.run(());
            }
        }
    });
    // A fresh review after an acknowledged result, never over an unacknowledged one.
    let review_again = Callback::new(move |()| {
        if phase.get_untracked().pinned() {
            return;
        }
        unacknowledged.set(false);
        pending.set(None);
        // Idle lets a record written by another tab surface immediately.
        phase.set(Phase::Idle);
        load_pending.run(true);
        if phase.get_untracked().pinned() {
            return;
        }
        if requested_source_change(
            historical.get_untracked().as_ref(),
            chosen.get_untracked().as_ref(),
        )
        .is_some()
        {
            // A different source was requested meanwhile: follow it, not the old head.
            begin.run(());
        } else if let Some(head) = renew_head.get_untracked() {
            // A moved head is reviewed at the head the server reported.
            renew_head.set(None);
            generation.update(|g| *g += 1);
            chosen.set(None);
            check.run((head, false));
        } else {
            begin.run(());
        }
    });

    // Focus follows the phase: the heading, or Open brew once a brew exists.
    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        let key = phase.with(|p| p.key());
        if !open.get() {
            return;
        }
        let this = generation.get_untracked();
        let target = if matches!(key, "started" | "existing") {
            "brew-start-open"
        } else {
            "start-phase-heading"
        };
        request_animation_frame(move || {
            if open.try_get_untracked() == Some(true)
                && phase.try_with_untracked(|p| p.key()) == Some(key)
                && generation.try_get_untracked() == Some(this)
            {
                crate::pages::recipe_editor::focus_element(target);
            }
        });
    });

    let close = Callback::new(move |()| open.set(false));
    let heading = move || match phase.get() {
        Phase::Choose(SourceChoice::New) => "Save this recipe to start a brew",
        Phase::Choose(_) => "Choose what to start",
        Phase::Saving => "Saving your recipe",
        Phase::Started(_) => "Brew started",
        Phase::Existing(_) => "Brew already started",
        Phase::Starting => "Starting brew",
        Phase::Unknown(_) | Phase::Stuck(_) => "Brew start unconfirmed",
        Phase::Blocked(_) => "Not ready to start",
        Phase::Changed(_) => "Review again before starting",
        Phase::Idle | Phase::Checking | Phase::Review { .. } => "Review before starting",
    };
    let pinned_line = move || {
        let text = pinned_source.get();
        (!text.is_empty()).then(|| {
            view! { <BreweryMetricList>{fact("Source", text, "start-pinned-source")}</BreweryMetricList> }
        })
    };
    let body = move || {
        if !context_visible.get() {
            let text = match phase.get() {
                Phase::Starting | Phase::Unknown(_) | Phase::Stuck(_) => {
                    "Access to this workspace is being rechecked. The original start request is retained and its result is unconfirmed."
                }
                Phase::Started(_) | Phase::Existing(_) => {
                    "Access to this workspace is being rechecked. A brew was started; recheck access to open it."
                }
                Phase::Saving => {
                    "Access to this workspace is being rechecked. Your save may still be completing; recheck access to see the result."
                }
                Phase::Blocked(_) | Phase::Changed(_) => {
                    "Access to this workspace is being rechecked. Recheck access to continue."
                }
                _ => {
                    "Start is unavailable until your workspace access is confirmed. Nothing was sent."
                }
            };
            // A message set by this task (e.g. access could not be confirmed before
            // anything was retained) is the more specific truth for a review.
            let note = message.get();
            let text = hidden_review_note(&phase.get(), &note)
                .map_or_else(|| text.to_owned(), str::to_owned);
            return view! { <BreweryNotice title="Access is being checked" body=text warning=true/> }.into_any();
        }
        let notice = move || {
            let text = message.get();
            (!text.is_empty()).then(|| view! { <p role="status">{text}</p> })
        };
        let inner = match phase.get() {
            Phase::Idle => view! { {notice()}<p role="status">"Preparing…"</p> }.into_any(),
            Phase::Choose(choice) => {
                let head = revision.get();
                view! {
                    {notice()}
                    <p>{match choice {
                        SourceChoice::New => "This recipe has not been saved yet. A brew starts from a saved revision, so save it first.".to_owned(),
                        _ => format!(
                            "You have unsaved changes. A brew starts from a saved revision: save your changes first, or start the last saved revision{} and keep your changes as they are.",
                            head.map(|h| format!(" ({h})")).unwrap_or_default()
                        ),
                    }}</p>
                }.into_any()
            }
            Phase::Saving => view! { <p role="status">"Saving…"</p> }.into_any(),
            Phase::Checking => view! { <p role="status">"Checking saved recipe…"</p> }.into_any(),
            Phase::Review { .. } => {
                let found = summary.get();
                let hint = chosen.get();
                view! {
                    {notice()}
                    {move || is_dispatching().then(|| view! { <p role="status">"Preparing to start…"</p> })}
                    {found.map(|found| {
                        let selected = match &found.source { SummarySource::Saved { revision, .. } => *revision, _ => 0 };
                        let dirty_note = dirty.get().then(|| view! { <p class="note-caption">"Your unsaved changes are not part of this brew."</p> });
                        let historic = hint.is_some();
                        let source_text = if historic {
                            revision_label(selected, hint.as_ref())
                        } else {
                            format!("Current saved revision {selected}")
                        };
                        let identity_rows = [
                            fact("Recipe", found.title.clone(), "start-recipe"),
                            fact("Source", source_text, "start-source"),
                        ]
                        .into_iter()
                        .collect_view();
                        view! {
                            <section aria-label="Source review">
                                <BreweryMetricList>{identity_rows}</BreweryMetricList>
                                {historic.then(|| view! { <p class="note-caption">"Started without restoring or editing this version. Equipment is resolved from your current profile; existing brews are unchanged."</p> })}
                                {dirty_note}
                                {equipment_view(&found, &overrides.get())}
                                {target_metrics(&found.targets)}
                                <BreweryDisclosure title="Assumptions and calculation context">
                                    <p class="note-caption">{format!("Engine {} · policy {} · context {}", found.context.engine_version, found.context.policy_version, found.context.digest.clone().unwrap_or_else(|| "unavailable".into()))}</p>
                                    <ul>{found.assumptions.iter().map(|a| view! { <li>{a.replace('_', " ")}</li> }).collect_view()}</ul>
                                </BreweryDisclosure>
                            </section>
                        }
                    })}
                }.into_any()
            }
            Phase::Blocked(text) => view! { {notice()}<BreweryNotice title="Not ready to start" body=text warning=true/> }.into_any(),
            Phase::Changed(text) => view! {
                {notice()}
                <BreweryNotice title="Review again before starting" body=text warning=true/>
                {refreshed.get().map(|targets| view! { <p>"Refreshed targets"</p>{target_metrics(&targets)} })}
            }.into_any(),
            Phase::Starting => view! { {pinned_line()}{notice()}<p role="status">"Starting brew…"</p><p class="note-caption">"Closing this does not cancel the start. The original request is retained."</p> }.into_any(),
            Phase::Unknown(text) => view! {
                {pinned_line()}{notice()}
                <BreweryNotice title="Result unconfirmed" body=format!("{text} Closing this or leaving the page does not cancel it, and whether a brew was created is not yet known. Check the previous result: the same request is sent again and cannot create a second brew.") warning=true/>
            }.into_any(),
            Phase::Stuck(text) => view! {
                {pinned_line()}{notice()}
                <BreweryNotice title="This start cannot be resolved automatically" body=format!("{text} Starting a new brew is blocked until you resolve this. {}", if pending.get().is_some() { "Discarding removes this request's record only." } else { "Discarding removes only records that cannot be read; if the browser storage itself cannot be read, this message returns until it can." }) warning=true/>
            }.into_any(),
            Phase::Started(done) => view! {
                {pinned_line()}{notice()}
                <BreweryNotice title="Brew started" body=format!("Started from revision {} (brew revision {}, {}).", done.selected_revision, done.revision, done.status.as_str())/>
                {target_metrics(&done.targets)}
            }.into_any(),
            Phase::Existing(_) => view! {
                {pinned_line()}{notice()}
                <BreweryNotice title="A brew was already created" body="This start was completed earlier; its saved response has expired. Open the brew to continue."/>
            }.into_any(),
        };
        view! {
            <header class="k-section__header"><h3 id="start-phase-heading" tabindex="-1">{heading}</h3></header>
            {inner}
        }
        .into_any()
    };
    let actions = move || {
        let current = phase.get();
        let hidden = !context_visible.get();
        match current {
            Phase::Choose(choice) => view! {
                <Button variant=ButtonVariant::Secondary on:click=move |_| close.run(())>"Cancel"</Button>
                {(choice == SourceChoice::Dirty).then(|| view! {
                    <Button variant=ButtonVariant::Secondary disabled=hidden on:click=move |_| {
                        if let Some(head) = revision.get_untracked() { chosen.set(None); check.run((head, false)); }
                    }>{move || format!("Use saved revision {}", revision.get().unwrap_or_default())}</Button>
                })}
                <Button disabled=Signal::derive(move || hidden || saving.get()) on:click=move |_| save_and_continue.run(())>
                    {if choice == SourceChoice::New { "Save recipe and continue" } else { "Save changes and continue" }}
                </Button>
            }.into_any(),
            Phase::Saving => view! { <Button variant=ButtonVariant::Secondary on:click=move |_| close.run(())>"Close"</Button><Button pending=true>"Saving…"</Button> }.into_any(),
            Phase::Checking => view! { <Button variant=ButtonVariant::Secondary on:click=move |_| close.run(())>"Cancel"</Button> }.into_any(),
            Phase::Idle => view! {
                <Button variant=ButtonVariant::Secondary on:click=move |_| close.run(())>"Cancel"</Button>
                <Button disabled=hidden on:click=move |_| begin.run(())>"Check again"</Button>
            }.into_any(),
            Phase::Review { .. } => view! {
                <Button variant=ButtonVariant::Secondary on:click=move |_| close.run(())>"Cancel"</Button>
                <Button disabled=Signal::derive(move || hidden || is_dispatching()) on:click=move |_| start.run(())>"Start brew"</Button>
            }.into_any(),
            Phase::Blocked(_) => view! {
                <Button variant=ButtonVariant::Secondary on:click=move |_| close.run(())>"Close"</Button>
                <Button on:click=move |_| review_again.run(())>"Check again"</Button>
            }.into_any(),
            Phase::Changed(_) => view! {
                <Button variant=ButtonVariant::Secondary on:click=move |_| close.run(())>"Cancel"</Button>
                <Button on:click=move |_| review_again.run(())>"Review again"</Button>
            }.into_any(),
            Phase::Starting => view! { <Button variant=ButtonVariant::Secondary on:click=move |_| close.run(())>"Close"</Button><Button pending=true>"Starting brew…"</Button> }.into_any(),
            Phase::Unknown(_) => view! {
                <Button variant=ButtonVariant::Secondary on:click=move |_| close.run(())>"Close"</Button>
                <Button disabled=hidden on:click=move |_| check_previous.run(())>"Check previous result"</Button>
            }.into_any(),
            Phase::Stuck(_) => view! {
                <Button variant=ButtonVariant::Secondary on:click=move |_| close.run(())>"Close"</Button>
                <Button variant=ButtonVariant::GhostDestructive on:click=move |_| discard_record.run(())>"I checked Brews; discard this record"</Button>
            }.into_any(),
            Phase::Started(done) => view! {
                <Button variant=ButtonVariant::Secondary on:click=move |_| { close.run(()); acknowledge.run(()); }>"Done"</Button>
                <ButtonLink id="brew-start-open" href=crate::utils::nav_state::brew_href(&done.brew_id, false)>"Open brew"</ButtonLink>
            }.into_any(),
            Phase::Existing(id) => view! {
                <Button variant=ButtonVariant::Secondary on:click=move |_| { close.run(()); acknowledge.run(()); }>"Done"</Button>
                <ButtonLink id="brew-start-open" href=crate::utils::nav_state::brew_href(&id, false)>"Open brew"</ButtonLink>
            }.into_any(),
        }
    };
    view! {
        // Another tab's unresolved request is surfaced here without taking over
        // the page; Start brew then opens the task on it.
        <Show when=move || !open.get() && matches!(phase.get(), Phase::Unknown(_) | Phase::Stuck(_))>
            <BreweryNotice
                title="A brew start is unconfirmed"
                body="A start for this recipe was sent and its result is not yet known. Use Start brew to check the previous result."
                warning=true
            />
        </Show>
        <BreweryDialog
            id="start-brew-dialog"
            title="Start brew"
            description=Signal::derive(move || dialog_description(&phase.get(), context_visible.get()).to_owned())
            open=open
            return_focus_fallback="recipe-start-command".to_string()
            actions=std::sync::Arc::new(move || (move || actions()).into_any()) as ChildrenFn
        >
            {body}
        </BreweryDialog>
    }
}

#[cfg(test)]
mod regression_tests {
    use super::*;

    fn unconfirmed() -> StartBrewOutcome {
        StartBrewOutcome::Retryable
    }

    #[test]
    fn a_retained_request_takes_over_only_an_undispatched_task() {
        let unresolved = |phase: Phase| may_take_over(&phase, false);
        assert!(unresolved(Phase::Idle) && unresolved(Phase::Choose(SourceChoice::Dirty)));
        assert!(unresolved(Phase::Review { head: Some(1) }));
        for owed in [
            Phase::Checking,
            Phase::Saving,
            Phase::Starting,
            Phase::Unknown(String::new()),
            Phase::Stuck(String::new()),
            Phase::Started(StartedView {
                brew_id: "b".into(),
                selected_revision: 1,
                revision: 1,
                status: BrewStatusV1::Started,
                targets: Targets::default(),
            }),
            Phase::Blocked(String::new()),
            Phase::Changed(String::new()),
        ] {
            assert!(!unresolved(owed.clone()), "{owed:?}");
        }
        assert!(!may_take_over(&Phase::Idle, true));
    }

    fn request_with(id: &str) -> StartBrewRequest {
        let intent = OperationIntent {
            account_id: "a".into(),
            workspace_id: "w".into(),
        };
        build_start_request(intent, "r".into(), 1, false, Some("d".into()), id.into()).unwrap()
    }

    #[test]
    fn inside_the_lock_any_other_unresolved_request_wins_regardless_of_order() {
        let (mine, other) = (request_with("mine"), request_with("other"));
        // Before retaining: anything unresolved blocks; nothing unresolved proceeds.
        assert!(must_yield(&[other.clone()], None) && !must_yield(&[], None));
        // After retaining: only a different request makes this task yield, whether
        // it is listed before or after its own, so timestamp ties cannot matter.
        assert!(!must_yield(&[mine.clone()], Some(&mine)));
        assert!(must_yield(&[other.clone(), mine.clone()], Some(&mine)));
        assert!(must_yield(&[mine.clone(), other], Some(&mine)));
    }

    #[test]
    fn a_different_requested_source_is_unavailable_until_resolved() {
        let rev = |revision| RevisionSummary {
            revision,
            saved_by: None,
            saved_at: None,
            restored_from_revision: None,
        };
        assert_eq!(requested_source_change(Some(&rev(2)), None), Some(Some(2)));
        assert_eq!(requested_source_change(None, Some(&rev(2))), Some(None));
        assert_eq!(
            requested_source_change(Some(&rev(2)), Some(&rev(3))),
            Some(Some(2))
        );
        assert_eq!(requested_source_change(Some(&rev(2)), Some(&rev(2))), None);
        assert_eq!(requested_source_change(None, None), None);
        assert!(source_unavailable_message(Some(2)).contains("revision 2"));
        assert!(source_unavailable_message(None).contains("current saved revision"));
    }

    #[test]
    fn start_is_refused_while_another_request_is_unresolved() {
        assert!(may_start(&Phase::Review { head: None }, false));
        assert!(!may_start(&Phase::Review { head: None }, true));
        assert!(!may_start(&Phase::Checking, false));
        assert!(!may_start(&Phase::Unknown(String::new()), false));
    }

    #[test]
    fn only_the_access_unconfirmed_message_replaces_the_hidden_review_copy() {
        let review = Phase::Review { head: Some(1) };
        assert_eq!(
            hidden_review_note(&review, ACCESS_UNCONFIRMED),
            Some(ACCESS_UNCONFIRMED)
        );
        for unrelated in [
            YIELDED,
            "Unable to read start recovery; nothing was started.",
            "Unable to create a secure start request.",
            "",
        ] {
            assert_eq!(hidden_review_note(&review, unrelated), None, "{unrelated}");
        }
        assert_eq!(
            hidden_review_note(&Phase::Checking, ACCESS_UNCONFIRMED),
            None
        );
    }

    #[test]
    fn blocking_reasons_are_distinct_by_row_and_code_not_by_text() {
        let d = |path: &str, code: &str| Diagnostic { code: code.into(), path: path.into(), severity: Readiness::Incomplete, affected_targets: vec![] };
        let found = [
            d("hops[a].specification.alpha_fraction", "missing"),
            d("hops[a].specification.alpha_fraction", "missing"),
            d("hops[b].specification.alpha_fraction", "missing"),
            d("batch_size_l", "missing"),
        ];
        assert_eq!(
            distinct_reasons(&found),
            vec!["Hop alpha acid is missing.", "Hop alpha acid is missing.", "Batch volume is missing."]
        );
        assert!(distinct_reasons(&[]).is_empty());
    }

    #[test]
    fn the_dialog_describes_each_state_and_never_asks_for_a_review_after_a_start() {
        let visible = |phase: &Phase| dialog_description(phase, true);
        assert!(visible(&Phase::Review { head: None }).contains("Review"));
        for sent in [
            Phase::Starting,
            Phase::Unknown(String::new()),
            Phase::Existing("b".into()),
        ] {
            assert!(
                !visible(&sent).contains("then start explicitly"),
                "{sent:?}"
            );
        }
        assert_eq!(
            visible(&Phase::Starting),
            "The start request was sent. Closing this does not cancel it."
        );
        assert!(visible(&Phase::Unknown(String::new())).contains("not yet known"));
        // A new recipe is told to save first, not to choose a revision.
        let new = visible(&Phase::Choose(SourceChoice::New));
        assert!(new.contains("Save this recipe") && !new.contains("choose which one"));
        assert!(visible(&Phase::Choose(SourceChoice::Dirty)).contains("choose which one"));
    }

    #[test]
    fn while_access_is_hidden_the_description_promises_nothing() {
        let all = [
            Phase::Idle,
            Phase::Choose(SourceChoice::New),
            Phase::Saving,
            Phase::Checking,
            Phase::Review { head: None },
            Phase::Blocked(String::new()),
            Phase::Changed(String::new()),
            Phase::Starting,
            Phase::Unknown(String::new()),
            Phase::Stuck(String::new()),
            Phase::Existing("b".into()),
        ];
        for phase in all {
            let text = dialog_description(&phase, false);
            assert_eq!(
                text, "Access to this workspace is being rechecked.",
                "{phase:?}"
            );
            assert!(!text.contains("start explicitly") && !text.contains("Open it"));
        }
    }

    #[test]
    fn source_choice_covers_new_dirty_and_clean() {
        assert_eq!(source_choice(false, false), SourceChoice::New);
        assert_eq!(source_choice(false, true), SourceChoice::New);
        assert_eq!(source_choice(true, true), SourceChoice::Dirty);
        assert_eq!(source_choice(true, false), SourceChoice::Clean);
    }

    #[test]
    fn only_a_known_result_releases_the_original_request() {
        for outcome in [
            StartBrewOutcome::NotFound,
            StartBrewOutcome::InvalidRequest,
            StartBrewOutcome::RecipeArchived,
            StartBrewOutcome::ResultExpired {
                resource_id: Some("b".into()),
            },
        ] {
            assert!(resolution(&Ok(outcome)).releases_request());
        }
        for outcome in [
            unconfirmed(),
            StartBrewOutcome::Unavailable,
            StartBrewOutcome::Forbidden,
            StartBrewOutcome::ContextChanged,
            StartBrewOutcome::ResultExpired { resource_id: None },
            StartBrewOutcome::RequestIdentityConflict,
        ] {
            assert!(
                !resolution(&Ok(outcome.clone())).releases_request(),
                "{outcome:?}"
            );
        }
        assert!(!resolution(&Err(ServerFnError::new("lost response"))).releases_request());
    }

    #[test]
    fn an_expired_response_with_a_brew_opens_it_and_an_unknown_one_is_never_a_fresh_start() {
        assert_eq!(
            resolution(&Ok(StartBrewOutcome::ResultExpired {
                resource_id: Some("b1".into())
            })),
            Resolution::Existing("b1".into())
        );
        for outcome in [
            StartBrewOutcome::ResultExpired { resource_id: None },
            StartBrewOutcome::RequestIdentityConflict,
        ] {
            let Resolution::Stuck(text) = resolution(&Ok(outcome)) else {
                panic!("stuck")
            };
            assert!(text.contains("may already exist"), "{text}");
        }
        assert!(Phase::Stuck(String::new()).pinned() && Phase::Existing("b".into()).pinned());
    }

    #[test]
    fn unconfirmed_wording_never_claims_nothing_was_started() {
        for result in [Ok(unconfirmed()), Err(ServerFnError::new("lost"))] {
            let Resolution::Unknown(text) = resolution(&result) else {
                panic!("unknown")
            };
            let lower = text.to_lowercase();
            assert!(
                !lower.contains("nothing")
                    && !lower.contains("not created")
                    && !lower.contains("no brew"),
                "{text}"
            );
        }
    }

    #[test]
    fn historical_start_omits_the_head_guard_and_current_keeps_it() {
        let intent = OperationIntent {
            account_id: "a".into(),
            workspace_id: "w".into(),
        };
        let current = build_start_request(
            intent.clone(),
            "r".into(),
            3,
            false,
            Some("d".into()),
            "id".into(),
        )
        .unwrap();
        assert_eq!(current.expected_head_revision, Some(3));
        let old = build_start_request(
            intent.clone(),
            "r".into(),
            1,
            true,
            Some("d".into()),
            "id".into(),
        )
        .unwrap();
        assert_eq!(
            (old.selected_recipe_revision, old.expected_head_revision),
            (1, None)
        );
        assert!(build_start_request(intent, "r".into(), 1, true, None, "id".into()).is_none());
    }

    #[test]
    fn the_pinned_source_names_the_original_revision_and_whether_it_was_historical() {
        let intent = OperationIntent {
            account_id: "a".into(),
            workspace_id: "w".into(),
        };
        let hint = RevisionSummary {
            revision: 2,
            saved_by: Some("Jason".into()),
            saved_at: Some("2026-11-02".into()),
            restored_from_revision: None,
        };
        let old = build_start_request(
            intent.clone(),
            "r".into(),
            2,
            true,
            Some("d".into()),
            "i".into(),
        )
        .unwrap();
        let text = pinned_source_text(&old, Some(&hint));
        assert!(
            text.contains("Revision 2")
                && text.contains("2026-11-02")
                && text.contains("older saved version"),
            "{text}"
        );
        let head = build_start_request(intent, "r".into(), 5, false, Some("d".into()), "i".into())
            .unwrap();
        assert!(pinned_source_text(&head, None).contains("Revision 5"));
    }

    #[test]
    fn a_saved_version_is_labelled_with_a_readable_date_and_no_internal_ids() {
        assert_eq!(
            short_date("2026-11-16T23:54:39.985065921+00:00"),
            "2026-11-16 23:54 UTC"
        );
        assert_eq!(short_date("2026-11-16T23:54:39Z"), "2026-11-16 23:54 UTC");
        assert_eq!(short_date("2026-11-16T23:54:39+10:00"), "2026-11-16 23:54");
        assert_eq!(short_date("2026-11-02"), "2026-11-02");
        // Non-ASCII, short and empty input never panic and fall back to the text as given.
        for odd in [
            "é2026-11-16T23:54",
            "",
            "2026",
            "2026-11-16é23:54Z",
            "ééééééééééééé",
        ] {
            assert_eq!(short_date(odd), odd);
        }
        assert_eq!(readable_author("Jason"), Some("Jason"));
        assert_eq!(readable_author("user-ed15820d-8bc6"), None);
        let hint = RevisionSummary {
            revision: 2,
            saved_by: Some("user-ed15".into()),
            saved_at: Some("2026-11-16T23:54:39.985065921+00:00".into()),
            restored_from_revision: None,
        };
        let label = revision_label(2, Some(&hint));
        assert_eq!(label, "Revision 2 · 2026-11-16 23:54 UTC");
    }

    #[test]
    fn revision_label_identifies_a_historical_version() {
        let hint = RevisionSummary {
            revision: 2,
            saved_by: Some("Jason".into()),
            saved_at: Some("2026-11-02".into()),
            restored_from_revision: None,
        };
        assert_eq!(
            revision_label(2, Some(&hint)),
            "Revision 2 · 2026-11-02 · by Jason"
        );
        assert_eq!(revision_label(4, None), "Revision 4");
    }

    #[test]
    fn the_source_stays_pinned_from_dispatch_until_the_result_is_acknowledged() {
        for pinned in [
            Phase::Starting,
            Phase::Unknown(String::new()),
            Phase::Stuck(String::new()),
        ] {
            assert!(pinned.pinned());
        }
        assert!(!Phase::Checking.pinned() && !Phase::Review { head: None }.pinned());
        assert!(!Phase::Changed(String::new()).pinned() && !Phase::Blocked(String::new()).pinned());
    }

    #[test]
    fn effective_equipment_values_are_trimmed_and_attributed() {
        let effective = EquipmentSettingsV1 {
            mash_efficiency_fraction: Some(0.57999999999999996),
            boil_off_l_per_hour: Some(3.0),
            ..Default::default()
        };
        let overrides = EquipmentSettingsV1 {
            mash_efficiency_fraction: Some(0.57999999999999996),
            ..Default::default()
        };
        let lines = effective_lines(&effective, &overrides, true);
        let mash = lines
            .iter()
            .find(|(l, _)| l.starts_with("Mash efficiency"))
            .unwrap();
        assert_eq!(
            mash,
            &("Mash efficiency: 58 % · recipe override".to_owned(), true)
        );
        let boil = lines
            .iter()
            .find(|(l, _)| l.starts_with("Boil-off"))
            .unwrap();
        assert_eq!(boil, &("Boil-off: 3 L/h · from profile".to_owned(), false));
    }
}
