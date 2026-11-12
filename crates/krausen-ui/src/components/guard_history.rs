// SPDX-License-Identifier: AGPL-3.0-or-later
//! Capability-detected native entry traversal. Classic history.state is never
//! read, wrapped or written. Legacy popstate cannot supply a lossless fallback.
use super::{EditorDestination, EditorGuard};
use leptos::prelude::*;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};

fn get(object: &JsValue, property: &str) -> Option<JsValue> {
    js_sys::Reflect::get(object, &property.into())
        .ok()
        .filter(|value| !value.is_null() && !value.is_undefined())
}
fn key(navigation: &JsValue) -> Option<String> {
    get(&get(navigation, "currentEntry")?, "key")?
        .as_string()
        .filter(|key| !key.is_empty())
}
fn traverse(
    navigation: &JsValue,
    destination: &str,
    state: Rc<RefCell<State>>,
    guard: EditorGuard,
    id: u64,
) -> Result<(), ()> {
    let (invocation, committing) = {
        let mut current = state.borrow_mut();
        current.invocation += 1;
        let committing = matches!(&current.phase, Phase::Committing(_));
        (current.invocation, committing)
    };
    let method = get(navigation, "traverseTo")
        .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
        .ok_or(())?;
    let result = method
        .call1(navigation, &destination.into())
        .map_err(|_| ())?;
    let finished = get(&result, "finished")
        .and_then(|value| value.dyn_into::<js_sys::Promise>().ok())
        .ok_or(())?;
    let promises = js_sys::Array::new();
    promises.push(&finished);
    if let Some(committed) = get(&result, "committed") {
        promises.push(&committed);
    }
    let finished = js_sys::Promise::all(&promises);
    leptos::task::spawn_local(async move {
        if wasm_bindgen_futures::JsFuture::from(finished)
            .await
            .is_err()
        {
            let mut state = state.borrow_mut();
            let owned = state.invocation == invocation
                && match &state.phase {
                    Phase::Restoring(attempt) if !committing => attempt.id == id,
                    Phase::Committing(attempt) if committing => attempt.id == id,
                    _ => false,
                };
            if owned {
                state.hold();
                report_failure(
                    &state,
                    guard,
                    "The browser rejected the original history traversal. Your work remains held open; no replacement navigation was made.",
                );
            }
        }
    });
    Ok(())
}

#[derive(Clone)]
struct Bookmark {
    element: Option<web_sys::HtmlElement>,
    selection: Option<(u32, u32, String)>,
    scroll: Vec<(web_sys::Element, i32, i32)>,
}
impl Bookmark {
    fn capture() -> Self {
        let document = web_sys::window().and_then(|w| w.document());
        let element = document
            .as_ref()
            .and_then(|d| d.active_element())
            .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok());
        let selection = element.as_ref().and_then(|e| {
            Some((
                get(e.as_ref(), "selectionStart")?.as_f64()? as u32,
                get(e.as_ref(), "selectionEnd")?.as_f64()? as u32,
                get(e.as_ref(), "selectionDirection")
                    .and_then(|v| v.as_string())
                    .unwrap_or_else(|| "none".into()),
            ))
        });
        let mut scroll = Vec::new();
        if let Some(document) = document {
            if let Ok(Some(main)) = document.query_selector(".app-shell__main") {
                scroll.push((main.clone(), main.scroll_left(), main.scroll_top()));
            }
        }
        Self {
            element,
            selection,
            scroll,
        }
    }
    fn restore(&self) {
        let Some(element) = &self.element else {
            return;
        };
        if !element.is_connected() {
            return;
        }
        if element
            .closest("[hidden], [inert], :disabled")
            .ok()
            .flatten()
            .is_some()
            || element.get_client_rects().length() == 0
        {
            return;
        }
        if web_sys::window()
            .and_then(|w| w.get_computed_style(element).ok().flatten())
            .and_then(|style| style.get_property_value("visibility").ok())
            .as_deref()
            == Some("hidden")
        {
            return;
        }
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        if !document.has_focus().unwrap_or(false)
            || document
                .query_selector("dialog[open]")
                .ok()
                .flatten()
                .is_some()
            || document
                .query_selector("[popover]:popover-open")
                .ok()
                .flatten()
                .is_some()
        {
            return;
        }
        // The dialog may have returned focus to this SAME element but lost its
        // range. Restore that range too; respect a deliberate different focus.
        if document.active_element().is_some_and(|active| {
            active != **element && active.tag_name() != "BODY" && active.tag_name() != "HTML"
        }) {
            return;
        }
        if let Some(focus) = get(element.as_ref(), "focus")
            .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
        {
            let options = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&options, &"preventScroll".into(), &JsValue::TRUE);
            let _ = focus.call1(element.as_ref(), &options);
        }
        if !document
            .active_element()
            .is_some_and(|active| active == **element)
        {
            return;
        }
        if let Some((start, end, direction)) = &self.selection {
            if let Some(set_range) = get(element.as_ref(), "setSelectionRange")
                .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
            {
                let _ = set_range.call3(
                    element.as_ref(),
                    &(*start).into(),
                    &(*end).into(),
                    &direction.as_str().into(),
                );
            }
        }
        for (element, left, top) in &self.scroll {
            element.set_scroll_left(*left);
            element.set_scroll_top(*top);
        }
    }
}

#[derive(Clone)]
struct Traversal {
    id: u64,
    source: String,
    destination: String,
    bookmark: Bookmark,
    owners: Vec<(u64, super::GuardScope, u64)>,
    guard_navigation: Option<u64>,
    origin_navigation: u64,
    cancelled: bool,
    cancellation: Option<(u64, u64)>,
    // An explicit source-only Retry owns its own interaction frame; it never
    // changes the original cancellation/selection provenance.
    source_retry_interaction: Option<u64>,
}

impl Traversal {
    fn shell_owned(&self, guard: EditorGuard) -> bool {
        if self.cancelled {
            return false;
        }
        match self.guard_navigation {
            Some(navigation) => {
                guard.history_shell_owned(navigation, Some((self.id, &self.destination)))
            }
            None => guard.history_shell_owned(self.origin_navigation, None),
        }
    }
    fn source_recovery_owned(&self, guard: EditorGuard, explicit_retry: bool) -> bool {
        let Some((epoch, interaction)) = self.cancellation else {
            return false;
        };
        if !self.cancelled || !guard.history_shell_owned(epoch, None) {
            return false;
        }
        let current = guard.tasks.get_untracked();
        if !self.owners.iter().all(|(id, scope, generation)| {
            current.iter().any(|task| {
                task.id == *id
                    && task.scope == *scope
                    && task.state.generation == *generation
                    && task.state.context_visible
            })
        }) {
            return false;
        }
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return false;
        };
        if !document.has_focus().unwrap_or(false) {
            return false;
        }
        let active = document.active_element();
        let in_notice = active.as_ref().is_some_and(|element| {
            element
                .closest("#workspace-draft-warning")
                .ok()
                .flatten()
                .is_some()
        }) && guard.notice.get_untracked();
        if explicit_retry && in_notice {
            return true;
        }
        guard.interaction_epoch() == self.source_retry_interaction.unwrap_or(interaction)
            && active.is_none_or(|element| {
                matches!(element.tag_name().as_str(), "BODY" | "HTML") || in_notice
                // Closing the owned notice can return focus to the original
                // field without a new input interaction.
                || self.bookmark.element.as_ref().is_some_and(|original| element == **original)
            })
    }
}

fn report_failure(state: &State, guard: EditorGuard, message: &str) {
    let attempt = match &state.phase {
        Phase::Restoring(attempt)
        | Phase::Choosing(attempt)
        | Phase::Committing(attempt)
        | Phase::Held(Some(attempt)) => Some(attempt),
        _ => None,
    };
    if let Some(attempt) = attempt {
        if attempt.source_recovery_owned(guard, false) {
            guard
                .history_restore_ready
                .set(matches!(state.phase, Phase::Held(Some(_))));
            guard.history_failure(attempt.cancellation.expect("owned cancellation").0, None,
                "The browser could not restore the original source entry after Stay. Retry restores only that source; the cancelled destination will not be reopened.");
        } else if attempt.shell_owned(guard) {
            guard
                .history_restore_ready
                .set(matches!(state.phase, Phase::Held(Some(_))));
            guard.history_failure(
                attempt
                    .guard_navigation
                    .unwrap_or(attempt.origin_navigation),
                attempt
                    .guard_navigation
                    .map(|_| (attempt.id, attempt.destination.as_str())),
                message,
            );
        }
    }
}

fn restore_bookmark(
    attempt: &Traversal,
    guard: EditorGuard,
    navigation: Option<JsValue>,
    expected_navigation: u64,
    interaction: u64,
) {
    let bookmark = send_wrapper::SendWrapper::new(attempt.bookmark.clone());
    let owners = attempt.owners.clone();
    let source = attempt.source.clone();
    let native = send_wrapper::SendWrapper::new(navigation);
    request_animation_frame(move || {
        if guard.navigation.try_get_untracked() != Some(expected_navigation)
            || guard.interaction_epoch() != interaction
            || native.as_ref().and_then(key).as_deref() != Some(&source)
        {
            return;
        }
        let current = guard.tasks.get_untracked();
        if owners.iter().all(|(id, scope, generation)| {
            current.iter().any(|task| {
                task.id == *id
                    && task.scope == *scope
                    && task.state.generation == *generation
                    && task.state.context_visible
            })
        }) {
            bookmark.restore();
        }
    });
}
enum Phase {
    Idle,
    Restoring(Traversal),
    Choosing(Traversal),
    Committing(Traversal),
    Held(Option<Traversal>),
}
struct State {
    phase: Phase,
    serial: u64,
    source: Option<String>,
    bookmark: Option<Bookmark>,
    invocation: u64,
}
impl State {
    fn hold(&mut self) {
        let attempt = match &self.phase {
            Phase::Restoring(attempt) | Phase::Choosing(attempt) | Phase::Committing(attempt) => {
                Some(attempt.clone())
            }
            Phase::Held(attempt) => attempt.clone(),
            Phase::Idle => None,
        };
        self.phase = Phase::Held(attempt);
    }
}

pub(super) fn install(guard: EditorGuard) {
    let window = web_sys::window().expect("browser window");
    let navigation = get(window.as_ref(), "navigation")
        .filter(|navigation| key(navigation).is_some() && get(navigation, "traverseTo").is_some());
    let state = Rc::new(RefCell::new(State {
        source: navigation.as_ref().and_then(key),
        serial: 0,
        phase: Phase::Idle,
        bookmark: None,
        invocation: 0,
    }));
    let failure = move |state: &State| {
        report_failure(
            state,
            guard,
            "This browser could not restore the original history entry. The form remains held open, but its address and exact Back/Forward protection could not be restored. Keep this page open.",
        );
    };
    let commit_state = state.clone();
    let commit_navigation = navigation.clone();
    guard.history_commit.set_value(Some(Callback::new({
        let shared = send_wrapper::SendWrapper::new((commit_state, commit_navigation));
        move |(id, destination, guard_navigation): (u64, String, u64)| {
            let (state, navigation) = &*shared;
            let mut current = state.borrow_mut();
            let Phase::Choosing(attempt) = &current.phase else {
                failure(&current);
                return false;
            };
            let Some(navigation) = navigation else {
                failure(&current);
                return false;
            };
            if attempt.id != id
                || attempt.destination != destination
                || key(navigation).as_deref() != Some(&attempt.source)
                || attempt.guard_navigation != Some(guard_navigation)
            {
                current.hold();
                failure(&current);
                return false;
            }
            let attempt = attempt.clone();
            current.phase = Phase::Committing(attempt);
            drop(current);
            if traverse(navigation, &destination, state.clone(), guard, id).is_err() {
                state.borrow_mut().hold();
                failure(&state.borrow());
                return false;
            }
            true
        }
    })));
    let cancel_state = state.clone();
    let cancel_navigation = navigation.clone();
    guard.history_cancel.set_value(Some(Callback::new({
        let shared = send_wrapper::SendWrapper::new((cancel_state, cancel_navigation));
        move |()| {
            let (state, navigation) = &*shared;
            let mut state = state.borrow_mut();
            match &mut state.phase {
                Phase::Held(Some(attempt))
                | Phase::Restoring(attempt)
                | Phase::Choosing(attempt)
                | Phase::Committing(attempt) => {
                    if !attempt.cancelled && attempt.shell_owned(guard) {
                        attempt.cancelled = true;
                        attempt.cancellation = Some((
                            guard.navigation.get_untracked() + 1,
                            guard.interaction_epoch(),
                        ));
                    }
                }
                _ => {}
            }
            if let Phase::Choosing(attempt) = &state.phase {
                if !attempt.cancelled {
                    return;
                }
                if navigation.as_ref().and_then(key).as_deref() == Some(&attempt.source) {
                    let expected_navigation = guard.navigation.get_untracked() + 1;
                    let interaction = guard.interaction_epoch();
                    restore_bookmark(
                        attempt,
                        guard,
                        navigation.clone(),
                        expected_navigation,
                        interaction,
                    );
                    state.phase = Phase::Idle;
                } else {
                    state.hold();
                    failure(&state);
                }
            }
        }
    })));
    if navigation.is_some() {
        let shared = send_wrapper::SendWrapper::new((state.clone(), navigation.clone()));
        guard
            .history_restore
            .set_value(Some(Callback::new(move |()| {
                let (state, navigation) = &*shared;
                let mut current = state.borrow_mut();
                let Phase::Held(Some(attempt)) = &current.phase else {
                    return false;
                };
                let mut attempt = attempt.clone();
                let source_only = attempt.source_recovery_owned(guard, true);
                if !source_only && !attempt.shell_owned(guard) {
                    return false;
                }
                if source_only {
                    attempt.source_retry_interaction = Some(guard.interaction_epoch());
                }
                let Some(navigation) = navigation.as_ref() else {
                    return false;
                };
                if key(navigation).as_deref() == Some(&attempt.source) {
                    if attempt.cancelled {
                        if let Some((expected, interaction)) = attempt.cancellation {
                            restore_bookmark(
                                &attempt,
                                guard,
                                Some(navigation.clone()),
                                expected,
                                attempt.source_retry_interaction.unwrap_or(interaction),
                            );
                        }
                        current.phase = Phase::Idle;
                        guard.navigation_error.set(String::new());
                        guard.history_restore_ready.set(false);
                        guard.notice.set(false);
                        return true;
                    }
                    let destination = EditorDestination::History {
                        key: attempt.destination.clone(),
                        attempt: attempt.id,
                    };
                    current.phase = Phase::Choosing(attempt);
                    drop(current);
                    guard.request(destination);
                    guard.notice.set(true);
                    if let Phase::Choosing(attempt) = &mut state.borrow_mut().phase {
                        let navigation = guard.navigation.get_untracked();
                        if guard.history_shell_owned(
                            navigation,
                            Some((attempt.id, &attempt.destination)),
                        ) {
                            attempt.guard_navigation = Some(navigation);
                        }
                    }
                    true
                } else {
                    let source = attempt.source.clone();
                    let id = attempt.id;
                    current.phase = Phase::Restoring(attempt);
                    drop(current);
                    if traverse(navigation, &source, state.clone(), guard, id).is_err() {
                        state.borrow_mut().hold();
                        failure(&state.borrow());
                        false
                    } else {
                        true
                    }
                }
            })));
    }
    let navigate_state = state.clone();
    let navigate = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
        if let Some(navigation) = guard.issued_document.get_untracked() {
            let target = get(event.as_ref(), "destination")
                .and_then(|destination| get(&destination, "url"))
                .and_then(|url| url.as_string());
            if target == guard.document_target.get_value()
                && guard.document_abort.with_value(|binding| binding.is_none())
            {
                if let Some(signal) = get(event.as_ref(), "signal")
                    .and_then(|signal| signal.dyn_into::<web_sys::EventTarget>().ok())
                {
                    let issuance = guard.document_issuance.get_untracked();
                    if get(signal.as_ref(), "aborted").and_then(|value| value.as_bool())
                        == Some(true)
                    {
                        guard.document_aborted(navigation, issuance);
                        return;
                    }
                    let abort = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                        guard.document_aborted(navigation, issuance)
                    });
                    if signal
                        .add_event_listener_with_callback("abort", abort.as_ref().unchecked_ref())
                        .is_ok()
                    {
                        guard
                            .document_abort
                            .set_value(Some(send_wrapper::SendWrapper::new((signal, abort))));
                    }
                }
            }
        }
        if get(event.as_ref(), "navigationType")
            .and_then(|value| value.as_string())
            .as_deref()
            == Some("traverse")
            && guard.dirty.get_untracked()
        {
            let mut state = navigate_state.borrow_mut();
            if matches!(state.phase, Phase::Idle) {
                state.bookmark = Some(Bookmark::capture());
            }
        }
    });
    let entry_state = state.clone();
    let entry_navigation = navigation.clone();
    let entry = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
        let traversal = get(event.as_ref(), "navigationType")
            .and_then(|value| value.as_string())
            .as_deref()
            == Some("traverse");
        let mut state = entry_state.borrow_mut();
        if matches!(state.phase, Phase::Idle) && (!guard.dirty.get_untracked() || !traversal) {
            state.source = entry_navigation.as_ref().and_then(key);
        }
    });
    let pop_state = state.clone();
    let pop_navigation = navigation.clone();
    let pop = Closure::<dyn FnMut(web_sys::PopStateEvent)>::new(
        move |event: web_sys::PopStateEvent| {
            let mut state = pop_state.borrow_mut();
            let current = pop_navigation.as_ref().and_then(key);
            match &state.phase {
                Phase::Committing(attempt) if current.as_deref() == Some(&attempt.destination) => {
                    if attempt.cancelled
                        || attempt
                            .guard_navigation
                            .is_none_or(|navigation| !guard.history_arrival_owned(navigation))
                    {
                        event.stop_immediate_propagation();
                        let mut original = attempt.clone();
                        let captured_navigation = original.guard_navigation;
                        if !original.cancelled && original.shell_owned(guard) {
                            original.cancellation = Some((
                                guard.navigation.get_untracked() + 1,
                                guard.interaction_epoch(),
                            ));
                        }
                        original.cancelled = true;
                        let source = original.source.clone();
                        let id = original.id;
                        state.phase = Phase::Restoring(original);
                        drop(state);
                        if let Some(navigation) = captured_navigation {
                            guard.cancel_leave_if(navigation);
                        }
                        if let Some(navigation) = pop_navigation.as_ref() {
                            if traverse(navigation, &source, pop_state.clone(), guard, id).is_err()
                            {
                                pop_state.borrow_mut().hold();
                                failure(&pop_state.borrow());
                            }
                        } else {
                            pop_state.borrow_mut().hold();
                            failure(&pop_state.borrow());
                        }
                        return;
                    }
                    let navigation = attempt.guard_navigation;
                    state.source = current;
                    state.phase = Phase::Idle;
                    drop(state);
                    if let Some(navigation) = navigation {
                        guard.history_arrived(navigation);
                    }
                    return; // this exact key, once
                }
                Phase::Restoring(attempt) if current.as_deref() == Some(&attempt.source) => {
                    event.stop_immediate_propagation();
                    let attempt = attempt.clone();
                    if attempt.cancelled {
                        let source_owned = attempt.source_recovery_owned(guard, false);
                        if let Some((expected, interaction)) =
                            attempt.cancellation.filter(|_| source_owned)
                        {
                            restore_bookmark(
                                &attempt,
                                guard,
                                pop_navigation.clone(),
                                expected,
                                attempt.source_retry_interaction.unwrap_or(interaction),
                            );
                        }
                        if source_owned {
                            guard.navigation_error.set(String::new());
                            guard.history_restore_ready.set(false);
                            guard.notice.set(false);
                        }
                        state.source = Some(attempt.source);
                        state.phase = Phase::Idle;
                        return;
                    }
                    let tasks = guard.tasks.get_untracked();
                    if !attempt.owners.iter().all(|(id, scope, _)| {
                        tasks
                            .iter()
                            .any(|task| task.id == *id && task.scope == *scope)
                    }) {
                        state.hold();
                        failure(&state);
                        return;
                    }
                    let destination = EditorDestination::History {
                        key: attempt.destination.clone(),
                        attempt: attempt.id,
                    };
                    let id = attempt.id;
                    if !attempt.shell_owned(guard) {
                        state.phase = Phase::Idle;
                        return;
                    }
                    state.phase = Phase::Choosing(attempt);
                    guard.navigation_error.set(String::new());
                    drop(state);
                    let had_destination = guard.destination.get_untracked().is_some();
                    guard.request(destination);
                    if had_destination {
                        if matches!(
                            guard.transition.get_untracked(),
                            super::GuardTransition::Pending(_)
                        ) {
                            guard.transition.set(super::GuardTransition::Failed(
                                guard.navigation.get_untracked(),
                            ));
                            guard.refresh_tasks();
                        }
                        guard.notice.set(true);
                    }
                    if let Phase::Choosing(attempt) = &mut pop_state.borrow_mut().phase {
                        if attempt.id == id {
                            let navigation = guard.navigation.get_untracked();
                            if guard.history_shell_owned(
                                navigation,
                                Some((attempt.id, &attempt.destination)),
                            ) {
                                attempt.guard_navigation = Some(navigation);
                            }
                        }
                    }
                    return;
                }
                Phase::Idle if !guard.dirty.get_untracked() => {
                    state.source = current;
                    return;
                }
                Phase::Idle => {}
                _ => {
                    event.stop_immediate_propagation();
                    let attempt = match &state.phase {
                        Phase::Restoring(attempt)
                        | Phase::Choosing(attempt)
                        | Phase::Committing(attempt) => Some(attempt.clone()),
                        Phase::Held(attempt) => attempt.clone(),
                        Phase::Idle => None,
                    };
                    if let (Some(attempt), Some(navigation)) = (attempt, pop_navigation.as_ref()) {
                        let source = attempt.source.clone();
                        let id = attempt.id;
                        state.phase = Phase::Restoring(attempt);
                        drop(state);
                        if traverse(navigation, &source, pop_state.clone(), guard, id).is_err() {
                            pop_state.borrow_mut().hold();
                            failure(&pop_state.borrow());
                        }
                    } else {
                        state.hold();
                        failure(&state);
                    }
                    return;
                }
            }
            event.stop_immediate_propagation();
            let (Some(navigation), Some(source), Some(destination)) =
                (&pop_navigation, state.source.clone(), current)
            else {
                state.hold();
                guard.history_failure(guard.navigation.get_untracked(), None, "This browser could not restore the original history entry. Keep this page open.");
                return;
            };
            if source == destination {
                return;
            }
            state.serial += 1;
            let attempt = Traversal {
                id: state.serial,
                source: source.clone(),
                destination,
                bookmark: state.bookmark.take().unwrap_or_else(Bookmark::capture),
                owners: guard
                    .tasks
                    .get_untracked()
                    .iter()
                    .filter(|task| task.state.blocks())
                    .map(|task| (task.id, task.scope.clone(), task.state.generation))
                    .collect(),
                guard_navigation: None,
                origin_navigation: guard.navigation.get_untracked(),
                cancelled: false,
                cancellation: None,
                source_retry_interaction: None,
            };
            let id = attempt.id;
            state.phase = Phase::Restoring(attempt);
            drop(state);
            if traverse(navigation, &source, pop_state.clone(), guard, id).is_err() {
                pop_state.borrow_mut().hold();
                failure(&pop_state.borrow());
            }
        },
    );
    if let Some(navigation) = &navigation {
        if let Some(target) = navigation.dyn_ref::<web_sys::EventTarget>() {
            let _ = target
                .add_event_listener_with_callback("navigate", navigate.as_ref().unchecked_ref());
            let _ = target.add_event_listener_with_callback(
                "currententrychange",
                entry.as_ref().unchecked_ref(),
            );
        }
    }
    if window
        .add_event_listener_with_callback("popstate", pop.as_ref().unchecked_ref())
        .is_err()
    {
        guard.history_failure(
            guard.navigation.get_untracked(),
            None,
            "History event reporting is unavailable. Keep this page open.",
        );
    }
    let listeners = send_wrapper::SendWrapper::new((window, navigation, navigate, entry, pop));
    on_cleanup(move || {
        let (window, navigation, navigate, entry, pop) = &*listeners;
        let _ =
            window.remove_event_listener_with_callback("popstate", pop.as_ref().unchecked_ref());
        if let Some(target) = navigation
            .as_ref()
            .and_then(|n| n.dyn_ref::<web_sys::EventTarget>())
        {
            let _ = target
                .remove_event_listener_with_callback("navigate", navigate.as_ref().unchecked_ref());
            let _ = target.remove_event_listener_with_callback(
                "currententrychange",
                entry.as_ref().unchecked_ref(),
            );
        }
    });
}
