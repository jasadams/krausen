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
#[cfg(target_arch = "wasm32")]
const OWNER_ATTRIBUTE: &str = "data-print-inert-owner";

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
    children: Children,
) -> impl IntoView {
    let status = RwSignal::new(String::new());
    let root = NodeRef::<leptos::html::Section>::new();
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (root, print_state);
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
        // while the previous instance is still disposing. Each instance marks the
        // elements it made inert with its own owner token, and only removes
        // `inert` where it is still the owner, so an old instance's cleanup never
        // re-enables a page the new instance is isolating.
        static NEXT_OWNER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let owner = NEXT_OWNER.fetch_add(1, std::sync::atomic::Ordering::Relaxed).to_string();
        let setup_owner = owner.clone();
        Effect::new(move |_| {
            let Some(section) = root.get() else { return };
            let owner = setup_owner.clone();
            let mut changed = Vec::new();
            let mut current: web_sys::Element = section.unchecked_into();
            while let Some(parent) = current.parent_element() {
                let siblings = parent.children();
                for index in 0..siblings.length() {
                    let Some(sibling) = siblings.item(index) else { continue };
                    // Elements that are inert for another reason are not ours.
                    let foreign = sibling.has_attribute("inert") && !sibling.has_attribute(OWNER_ATTRIBUTE);
                    if sibling.is_same_node(Some(&current)) || foreign {
                        continue;
                    }
                    if sibling.set_attribute("inert", "").is_ok()
                        && sibling.set_attribute(OWNER_ATTRIBUTE, &owner).is_ok()
                    {
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
            inerted.set_value(SendWrapper::new(changed));
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
            for element in inerted.get_value().iter() {
                if element.get_attribute(OWNER_ATTRIBUTE).as_deref() != Some(owner.as_str()) {
                    continue;
                }
                if element.remove_attribute("inert").is_err() || element.remove_attribute(OWNER_ATTRIBUTE).is_err() {
                    tracing::warn!("Unable to restore the page behind the print preview");
                }
            }
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
        <section class="print-preview" role="dialog" aria-modal="true" aria-labelledby=HEADING_ID node_ref=root>
            <div class="print-preview__toolbar">
                <div>
                    <h2 id=HEADING_ID tabindex="-1" class="kit-showcase__eyebrow">{heading}</h2>
                    <p class="print-preview__hint">{hint}</p>
                </div>
                <div class="recipe-editor__actions">
                    <Button on:click=print>"Print / Save as PDF"</Button>
                    <Button variant=ButtonVariant::Secondary on:click=move |_| on_close.run(())>{back_label}</Button>
                </div>
                <p role="status">{move || status.get()}</p>
            </div>
            {children()}
        </section>
    }
}
