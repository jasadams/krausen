// SPDX-License-Identifier: AGPL-3.0-or-later
//! Deterministic successor focus after rows or dialogs change.

/// Moves focus to the first listed control that is actually available. Rows and
/// dialogs settle before the next frame, so succession is evaluated there; an
/// absent, hidden or unfocusable candidate falls through to the next.
pub fn focus_first_available(candidates: Vec<String>) {
    #[cfg(target_arch = "wasm32")]
    leptos::prelude::request_animation_frame(move || {
        use wasm_bindgen::JsCast;
        let Some(document) = web_sys::window().and_then(|window| window.document()) else { return };
        for id in &candidates {
            if let Some(element) = document
                .get_element_by_id(id)
                .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
            {
                if element.get_client_rects().length() > 0 && element.focus().is_ok() {
                    return;
                }
            }
        }
    });
    #[cfg(not(target_arch = "wasm32"))]
    let _ = candidates;
}
