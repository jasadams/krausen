// SPDX-License-Identifier: AGPL-3.0-or-later
//! Deterministic successor focus after rows or dialogs change.

/// Focuses the first candidate that is rendered and focusable; later candidates
/// are only tried when an earlier one is absent, hidden or unfocusable.
#[cfg(target_arch = "wasm32")]
fn focus_first(document: &web_sys::Document, candidates: &[String]) {
    use wasm_bindgen::JsCast;
    for id in candidates {
        if let Some(element) = document.get_element_by_id(id).and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok()) {
            if element.get_client_rects().length() > 0 && element.focus().is_ok() {
                return;
            }
        }
    }
}

/// Moves focus to the first listed control that is actually available. Rows and
/// dialogs settle before the next frame, so succession is evaluated there; an
/// absent, hidden or unfocusable candidate falls through to the next.
pub fn focus_first_available(candidates: Vec<String>) {
    focus_first_available_unless_moved_inner(None, candidates);
}

/// Like [`focus_first_available`], but a deliberate focus move made after the
/// action wins: succession only happens while focus is still on the control
/// that triggered the action (`origins`, by id) or has fallen to the page body
/// because that control was removed (ACC-005: never steal a field the user
/// already moved to).
pub fn focus_first_available_unless_moved(origins: Vec<String>, candidates: Vec<String>) {
    focus_first_available_unless_moved_inner(Some(origins), candidates);
}

fn focus_first_available_unless_moved_inner(origins: Option<Vec<String>>, candidates: Vec<String>) {
    #[cfg(target_arch = "wasm32")]
    leptos::prelude::request_animation_frame(move || {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else { return };
        if let (Some(origins), Some(active)) = (&origins, document.active_element()) {
            let on_body = active.tag_name().eq_ignore_ascii_case("body");
            if !on_body && !origins.iter().any(|origin| *origin == active.id()) {
                return;
            }
        }
        focus_first(&document, &candidates);
    });
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (origins, candidates);
}

/// One focus guard for every deliberate editor focus move: visible, connected,
/// enabled and not inside a hidden/inert subtree.
#[cfg(target_arch = "wasm32")]
pub(crate) fn try_focus_element(element: &web_sys::Element) -> bool {
    use wasm_bindgen::JsCast;
    if element.closest("[hidden], [inert], :disabled").ok().flatten().is_some()
        || element.get_client_rects().length() == 0
        || web_sys::window().and_then(|w| w.get_computed_style(element).ok().flatten())
            .and_then(|style| style.get_property_value("visibility").ok()).is_some_and(|v| v == "hidden") { return false; }
    element.dyn_ref::<web_sys::HtmlElement>().is_some_and(|target| target.focus().is_ok())
        && web_sys::window().and_then(|w| w.document()).and_then(|d| d.active_element()).is_some_and(|active| active == *element)
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn focus_element(id: &str) {
    let focused = web_sys::window().and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(id))
        .is_some_and(|element| try_focus_element(&element));
    if !focused { tracing::warn!(target = id, "Unable to focus a recipe target"); }
}

/// Deliberate diagnostic jump: open every collapsed ancestor of the control,
/// then focus it, or the summary entry that explains it when the control is not
/// rendered (a conditional field, for instance). Called only from a user action.
#[cfg(target_arch = "wasm32")]
fn jump_to_diagnostic(target: &str, explanation: &str) {
    use wasm_bindgen::JsCast;
    let Some(document) = web_sys::window().and_then(|window| window.document()) else { return; };
    if let Some(element) = document.get_element_by_id(target) {
        let mut ancestor = element.parent_element();
        while let Some(node) = ancestor {
            if let Some(details) = node.dyn_ref::<web_sys::HtmlDetailsElement>() { details.set_open(true); }
            ancestor = node.parent_element();
        }
        if try_focus_element(&element) { return; }
    }
    focus_element(explanation);
}

/// Waits for the render, then runs the deliberate diagnostic jump. Shared by the
/// editor and the design-system showcase.
pub(crate) fn jump_after_render(target: String, explanation: String) {
    #[cfg(target_arch = "wasm32")]
    leptos::prelude::request_animation_frame(move || jump_to_diagnostic(&target, &explanation));
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (target, explanation);
}
