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
