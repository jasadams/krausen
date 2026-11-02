// SPDX-License-Identifier: AGPL-3.0-or-later

//! Ordinary native popover: normal Tab order, no menu roles or focus trap.
//! Render in place so a popup inside a native Navigation dialog remains in its
//! usable subtree. The browser owns light dismissal and invoker Tab ordering.
//! Requires the native HTML Popover API; no legacy-browser disclosure fallback
//! is supplied. Acceptance records the browser versions actually exercised.
use leptos::prelude::*;

use super::popover::Placement;
#[cfg(target_arch = "wasm32")]
use super::popover::{Rect, compute_position};
use super::{Button, ButtonVariant};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

#[cfg(target_arch = "wasm32")]
fn return_popover_focus(trigger: Option<&web_sys::HtmlElement>, successor: Option<&str>) {
    use wasm_bindgen::JsCast;
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let successor = successor
        .and_then(|id| document.get_element_by_id(id))
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok());
    for element in trigger.into_iter().chain(successor.as_ref()) {
        if !element.is_connected()
            || element.get_client_rects().length() == 0
            || element
                .matches(":disabled, [aria-disabled='true']")
                .unwrap_or(true)
            || element
                .closest("[inert], [hidden]")
                .ok()
                .flatten()
                .is_some()
        {
            continue;
        }
        let visible = match window.get_computed_style(element) {
            Ok(Some(style)) => !matches!(
                style.get_property_value("visibility").as_deref(),
                Ok("hidden" | "collapse")
            ),
            Ok(None) => false,
            Err(error) => {
                tracing::warn!(?error, "Unable to inspect popover focus target");
                false
            }
        };
        if !visible {
            continue;
        }
        if let Err(error) = element.focus() {
            tracing::warn!(?error, "Unable to return popover focus");
        } else if document
            .active_element()
            .is_some_and(|active| active.is_same_node(Some(element)))
        {
            return;
        }
    }
}

/// Shared account/actions disclosure. Keep `id` unique and stable. Consumers own
/// links, selection text, commands and pending state; opening never dispatches a
/// command. Escape returns to the visible trigger. If disposing an open popup,
/// provide a visible successor ID (for example the new task's heading).
#[component]
pub fn BreweryPopover(
    #[prop(into)] id: String,
    #[prop(into)] label: String,
    open: RwSignal<bool>,
    #[prop(default = Placement::BOTTOM_END)] placement: Placement,
    #[prop(optional, into)] return_focus_id: Option<String>,
    children: Children,
) -> impl IntoView {
    let trigger = NodeRef::<leptos::html::Button>::new();
    let content = NodeRef::<leptos::html::Div>::new();
    let intrinsic = NodeRef::<leptos::html::Div>::new();
    let focus_successor = StoredValue::new(return_focus_id);

    #[cfg(not(target_arch = "wasm32"))]
    let _ = (placement, focus_successor);

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{JsCast, closure::Closure};

        let reposition = move || {
            let (Some(trigger), Some(content), Some(window)) = (
                trigger.get_untracked(),
                content.get_untracked(),
                web_sys::window(),
            ) else {
                return;
            };
            if !content.matches(":popover-open").unwrap_or(false) {
                return;
            }
            let native: &web_sys::HtmlElement = content.unchecked_ref();
            let style = native.style();
            let viewport = window.visual_viewport();
            let width = viewport.as_ref().map(|v| v.width()).unwrap_or_else(|| {
                window
                    .inner_width()
                    .ok()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
            });
            let height = viewport.as_ref().map(|v| v.height()).unwrap_or_else(|| {
                window
                    .inner_height()
                    .ok()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
            });
            let left = viewport.as_ref().map(|v| v.offset_left()).unwrap_or(0.0);
            let top = viewport.as_ref().map(|v| v.offset_top()).unwrap_or(0.0);
            // Constrain before measuring, then use visible-viewport coordinates
            // throughout the existing geometry helper. Fixed CSS coordinates are
            // translated back to the layout viewport only at the final write.
            if let Err(error) =
                style.set_property("max-width", &format!("{}px", (width - 16.0).max(0.0)))
            {
                tracing::warn!(?error, "Unable to constrain popover width");
            }
            // scrollHeight retains the content extent while capped. Keep the
            // active scrollport constrained: expanding it even temporarily can
            // clamp scrollTop and clip the focused action when the cap returns.
            let rect = trigger.get_bounding_client_rect();
            let position = compute_position(
                Rect {
                    top: rect.top() - top,
                    left: rect.left() - left,
                    width: rect.width(),
                    height: rect.height(),
                },
                (
                    content.get_bounding_client_rect().width(),
                    content.scroll_height() as f64,
                ),
                width,
                height,
                placement,
            );
            // An anchor can be outside the panned visible rectangle. Preserve
            // helper placement, but never let that offscreen anchor enlarge the
            // usable rectangle beyond the actual visible viewport.
            let max_height = position.max_height.min((height - 16.0).max(0.0));
            let visible_top = position
                .top
                .clamp(8.0, (height - 8.0 - max_height).max(8.0));
            for (name, value) in [
                ("top", visible_top + top),
                ("left", position.left + left),
                ("max-height", max_height),
            ] {
                if let Err(error) = style.set_property(name, &format!("{value}px")) {
                    tracing::warn!(?error, "Unable to position popover");
                }
            }
        };

        // Controlled closing is useful when selecting an action. Native toggle
        // events keep state accurate for light dismissal and Escape too.
        Effect::new(move |_| {
            let requested = open.get();
            if let Some(element) = content.get() {
                let actual = element.matches(":popover-open").unwrap_or(false);
                if requested != actual {
                    let method = if requested {
                        "showPopover"
                    } else {
                        "hidePopover"
                    };
                    match js_sys::Reflect::get(element.as_ref(), &method.into())
                        .and_then(|method| method.dyn_into::<js_sys::Function>())
                        .and_then(|method| method.call0(element.as_ref()))
                    {
                        Ok(_) => {}
                        Err(error) => tracing::warn!(?error, "Unable to change native popover"),
                    }
                }
                if requested {
                    reposition();
                }
            }
        });

        // Native popovers stay mounted: field values/caret and callbacks are not
        // recreated on open, viewport changes or background preview updates.
        let resize = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| reposition());
        if let Some(window) = web_sys::window() {
            for event in ["resize", "scroll"] {
                if let Err(error) = window.add_event_listener_with_callback_and_bool(
                    event,
                    resize.as_ref().unchecked_ref(),
                    true,
                ) {
                    tracing::warn!(?error, "Unable to observe popover viewport");
                }
            }
            let visual = window.visual_viewport();
            if let Some(visual) = visual.as_ref() {
                for event in ["resize", "scroll"] {
                    if let Err(error) = visual
                        .add_event_listener_with_callback(event, resize.as_ref().unchecked_ref())
                    {
                        tracing::warn!(?error, "Unable to observe visual viewport");
                    }
                }
            }
            let observer = send_wrapper::SendWrapper::new((window, visual, resize));
            on_cleanup(move || {
                let (window, visual, resize) = observer.take();
                for event in ["resize", "scroll"] {
                    if let Err(error) = window.remove_event_listener_with_callback_and_bool(
                        event,
                        resize.as_ref().unchecked_ref(),
                        true,
                    ) {
                        tracing::warn!(?error, "Unable to release popover viewport observer");
                    }
                }
                if let Some(visual) = visual {
                    for event in ["resize", "scroll"] {
                        if let Err(error) = visual.remove_event_listener_with_callback(
                            event,
                            resize.as_ref().unchecked_ref(),
                        ) {
                            tracing::warn!(?error, "Unable to release visual viewport observer");
                        }
                    }
                }
            });
        }

        // Observe the uncapped inner content rather than the capped popup box:
        // membership/error/font changes must not retain the initial short height.
        // Measured style writes affect only the outer box, avoiding resize loops.
        Effect::new(move |_| {
            let (Some(inner), Some(anchor)) = (intrinsic.get(), trigger.get()) else {
                return;
            };
            let callback =
                Closure::<dyn FnMut(js_sys::Array, web_sys::ResizeObserver)>::new(move |_, _| {
                    reposition()
                });
            match web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()) {
                Ok(observer) => {
                    observer.observe(&inner);
                    observer.observe(&anchor);
                    let observer = send_wrapper::SendWrapper::new((observer, callback));
                    on_cleanup(move || {
                        let (observer, _callback) = observer.take();
                        observer.disconnect();
                    });
                }
                Err(error) => tracing::warn!(?error, "Unable to observe popover content"),
            }
        });

        on_cleanup(move || {
            let Some(document) = web_sys::window().and_then(|window| window.document()) else {
                return;
            };
            let focused_inside = content.get_untracked().is_some_and(|element| {
                document
                    .active_element()
                    .is_some_and(|active| element.contains(Some(&active)))
            });
            if focused_inside {
                focus_successor.with_value(|id| return_popover_focus(None, id.as_deref()));
            }
        });
    }

    view! {
        <Button variant=ButtonVariant::Secondary node_ref=trigger aria_controls=id.clone()
            aria_expanded=Signal::derive(move || open.get()) popover_target=id.clone()>{label}</Button>
        <div id=id class="k-popover" node_ref=content popover="auto"
            on:toggle=move |_| {
                #[cfg(target_arch = "wasm32")]
                if let Some(element) = content.get_untracked() {
                    open.set(element.matches(":popover-open").unwrap_or(false));
                }
            }
            on:keydown=move |event| {
                if event.key() == "Escape" && open.get_untracked() {
                    // Do not let the same Escape also cancel an ancestor drawer.
                    event.prevent_default();
                    event.stop_propagation();
                    open.set(false);
                    #[cfg(target_arch = "wasm32")]
                    {
                        let opener = trigger.get_untracked();
                        focus_successor.with_value(|id| return_popover_focus(opener.as_ref().map(|element| element.unchecked_ref::<web_sys::HtmlElement>()), id.as_deref()));
                    }
                }
            }><div class="k-popover__content" node_ref=intrinsic>{children()}</div></div>
    }
}
