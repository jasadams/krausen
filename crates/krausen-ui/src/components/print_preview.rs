// SPDX-License-Identifier: AGPL-3.0-or-later
//! The print preview task boundary (ACC-003, INT-008).
//!
//! A full-screen paper preview is a real modal task: on mount it makes
//! everything outside it inert, moves focus to its heading and installs the
//! print-only page state; Escape or Back close it; on close the caller returns
//! focus to its opener. The page behind is kept mounted, so draft, scroll and
//! selection are exactly as they were. The paper itself stays light in dark
//! mode (kit.css). Both the recipe preview and the frozen brew preview use this
//! one component; their sheet content and semantics stay with their owners.
use super::{Button, ButtonVariant};
use leptos::prelude::*;

const HEADING_ID: &str = "print-preview-heading";
const HINT_ID: &str = "print-preview-hint";
#[cfg(target_arch = "wasm32")]
const OWNER_ATTRIBUTE: &str = "data-print-inert-owner";

/// Make everything outside `keep`'s ancestor chain inert, marking each element
/// with `owner` so only its owner can release it. Elements that are inert for
/// another reason are left alone.
#[cfg(target_arch = "wasm32")]
fn isolate_page(keep: &web_sys::Element, owner: &str) -> Vec<web_sys::Element> {
    let mut changed = Vec::new();
    let mut current = keep.clone();
    while let Some(parent) = current.parent_element() {
        let siblings = parent.children();
        for index in 0..siblings.length() {
            let Some(sibling) = siblings.item(index) else { continue };
            let foreign = sibling.has_attribute("inert") && !sibling.has_attribute(OWNER_ATTRIBUTE);
            // The preview surface is the task itself and is never isolated, whatever
            // the order in which it and the isolation were mounted.
            let task_surface = sibling.matches(".print-preview").unwrap_or(false);
            if sibling.is_same_node(Some(&current)) || foreign || task_surface {
                continue;
            }
            if sibling.set_attribute("inert", "").is_ok() && sibling.set_attribute(OWNER_ATTRIBUTE, owner).is_ok() {
                changed.push(sibling);
            } else {
                tracing::warn!("Unable to make the page behind the print preview inert");
            }
        }
        if parent.tag_name() == "BODY" {
            break;
        }
        current = parent;
    }
    changed
}

/// Release the elements `owner` still owns; a newer owner's isolation is kept.
#[cfg(target_arch = "wasm32")]
fn release_page(elements: &[web_sys::Element], owner: &str) {
    for element in elements {
        if element.get_attribute(OWNER_ATTRIBUTE).as_deref() != Some(owner) {
            continue;
        }
        if element.remove_attribute("inert").is_err() || element.remove_attribute(OWNER_ATTRIBUTE).is_err() {
            tracing::warn!("Unable to restore the page behind the print preview");
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn next_owner() -> String {
    static NEXT_OWNER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT_OWNER.fetch_add(1, std::sync::atomic::Ordering::Relaxed).to_string()
}

/// Keeps the page around this component's position inert while `active`.
///
/// A task whose preview is temporarily unmounted (the editor hides protected
/// content while it revalidates access) mounts this alongside it, so the page
/// stays isolated across the gap instead of becoming reachable. Place it where
/// the preview mounts: every sibling of this marker except the preview surface
/// (`.print-preview`) is isolated, whichever of them mounted first.
#[component]
pub fn PageIsolation(active: Signal<bool>) -> impl IntoView {
    let marker = NodeRef::<leptos::html::Span>::new();
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (active, marker);
    #[cfg(target_arch = "wasm32")]
    {
        use send_wrapper::SendWrapper;
        use wasm_bindgen::JsCast;
        let owner = next_owner();
        let held = StoredValue::new(SendWrapper::new(Vec::<web_sys::Element>::new()));
        let effect_owner = owner.clone();
        Effect::new(move |_| {
            let isolate = active.get();
            let element = marker.get();
            release_page(&held.get_value(), &effect_owner);
            held.set_value(SendWrapper::new(Vec::new()));
            if let (true, Some(element)) = (isolate, element) {
                held.set_value(SendWrapper::new(isolate_page(&element.unchecked_into(), &effect_owner)));
            }
        });
        on_cleanup(move || release_page(&held.get_value(), &owner));
    }
    view! { <span hidden node_ref=marker></span> }
}

#[component]
pub fn PrintPreviewTask(
    /// Visible heading and dialog name, e.g. "Print preview".
    heading: &'static str,
    /// One sentence under the heading.
    #[prop(into)]
    hint: String,
    /// Back/Escape. The caller owns state clearing and return focus.
    on_close: Callback<()>,
    /// Label of the Back action.
    #[prop(default = "Back to editor")]
    back_label: &'static str,
    /// Install the `krausen-printing` page state that hides the application when
    /// printing. The frozen brew print keeps its existing print behaviour.
    #[prop(default = true)]
    print_state: bool,
    /// Isolate the page behind this preview while it is mounted. A caller that
    /// keeps the page isolated across remounts itself passes `false` and
    /// renders [`PageIsolation`].
    #[prop(default = true)]
    isolate: bool,
    children: Children,
) -> impl IntoView {
    let status = RwSignal::new(String::new());
    let root = NodeRef::<leptos::html::Section>::new();
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (root, print_state, isolate);
    #[cfg(target_arch = "wasm32")]
    {
        use send_wrapper::SendWrapper;
        use wasm_bindgen::JsCast;
        let inerted = StoredValue::new(SendWrapper::new(Vec::<web_sys::Element>::new()));
        let body = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.body());
        // The print-only page state is shared by every live preview instance, so
        // a remount that mounts before the old instance is disposed cannot lose
        // it: it is removed only when the last instance goes away.
        static LIVE_PRINT_STATES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        if print_state {
            LIVE_PRINT_STATES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        if let Some(body) = body.as_ref().filter(|_| print_state) {
            if body.class_list().add_1("krausen-printing").is_err() {
                tracing::warn!("Unable to prepare the print-only page state");
                status.set("The print layout may include the application controls.".into());
            }
        }
        // The preview can be remounted (for example by a context revalidation)
        // while the previous instance is still disposing, so each instance
        // isolates and releases the page with its own owner token.
        let owner = next_owner();
        let setup_owner = owner.clone();
        Effect::new(move |_| {
            let Some(section) = root.get() else { return };
            if isolate {
                inerted.set_value(SendWrapper::new(isolate_page(&section.unchecked_into::<web_sys::Element>(), &setup_owner)));
            }
            let heading = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.get_element_by_id(HEADING_ID))
                .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok());
            if heading.is_none_or(|heading| heading.focus().is_err()) {
                tracing::warn!("Unable to focus the print preview heading");
            }
        });
        let escape = window_event_listener(leptos::ev::keydown, move |event| {
            if event.key() == "Escape" {
                event.prevent_default();
                on_close.run(());
            }
        });
        on_cleanup(move || {
            escape.remove();
            release_page(&inerted.get_value(), &owner);
            if print_state && LIVE_PRINT_STATES.fetch_sub(1, std::sync::atomic::Ordering::SeqCst) == 1 {
                if let Some(body) = body {
                    if body.class_list().remove_1("krausen-printing").is_err() {
                        tracing::warn!("Unable to clear the print-only page state");
                    }
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
        <section class="print-preview" role="dialog" aria-modal="true" aria-labelledby=HEADING_ID aria-describedby=HINT_ID node_ref=root>
            <div class="print-preview__toolbar">
                <div>
                    <h2 id=HEADING_ID tabindex="-1" class="kit-showcase__eyebrow">{heading}</h2>
                    <p id=HINT_ID class="print-preview__hint">{hint}</p>
                </div>
                <div class="recipe-editor__actions">
                    <Button on:click=print>"Print / Save as PDF"</Button>
                    <Button class="print-preview__back" variant=ButtonVariant::Secondary on:click=move |_| on_close.run(())>{back_label}</Button>
                </div>
                <p role="status">{move || status.get()}</p>
            </div>
            {children()}
        </section>
    }
}
