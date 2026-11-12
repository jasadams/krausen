// SPDX-License-Identifier: AGPL-3.0-or-later

//! Shared recipe controls and presentation components.
use leptos::prelude::*;

/// Appearance only: task state, validation and persistence remain caller-owned.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum PanelPresentation {
    #[default]
    Standard,
    Summary,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum MetricDensity {
    #[default]
    Headline,
    Compact,
    Quiet,
}

/// Persistent explanations are not live regions. Opt in only for settled outcomes.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum NoticeAnnouncement {
    #[default]
    Persistent,
    Settled,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum NoticeTone {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

/// Wrapping task actions; children choose their semantic Button variants.
/// Use one primary, secondary Add and quiet Cancel/Back on each task surface.
#[component]
pub fn BreweryActionGroup(
    #[prop(optional)] end: bool,
    #[prop(optional, into)] label: Option<String>,
    children: Children,
) -> impl IntoView {
    view! { <div class="k-action-group" class:k-action-group--end=end role=label.as_ref().map(|_| "group") aria-label=label>{children()}</div> }
}

/// A named section inside a panel, without another card around each field.
#[component]
pub fn BrewerySection(
    #[prop(into)] title: String,
    #[prop(optional)] actions: Option<ChildrenFn>,
    children: Children,
) -> impl IntoView {
    view! { <section class="k-section"><header class="k-section__header"><h3>{title}</h3>{actions.map(|slot| slot())}</header><div class="k-section__body">{children()}</div></section> }
}

/// Shared responsive field alignment, including space for associated messages.
#[component]
pub fn BreweryFieldRow(children: Children) -> impl IntoView {
    view! { <div class="k-field-row">{children()}</div> }
}

/// Compact semantic badge for contextual information, shared by kit consumers.
#[component]
pub fn BreweryBadge(
    #[prop(default = "neutral")] tone: &'static str,
    children: Children,
) -> impl IntoView {
    let class = match tone {
        "success" => "k-badge k-badge--success",
        "warning" => "k-badge k-badge--warning",
        "accent" => "k-badge k-badge--accent",
        _ => "k-badge",
    };
    view! { <span class=class>{children()}</span> }
}

/// Compact, titled surface shared by the editor and component showcase.
#[component]
pub fn BreweryPanel(
    #[prop(into)] title: String,
    #[prop(optional, into)] description: Option<String>,
    #[prop(optional, into)] eyebrow: Option<String>,
    #[prop(default = false)] mobile_compact: bool,
    #[prop(optional)] presentation: PanelPresentation,
    #[prop(optional)] actions: Option<ChildrenFn>,
    #[prop(optional)] footer: Option<ChildrenFn>,
    children: Children,
) -> impl IntoView {
    view! {
        <section class="k-panel" class:k-panel--mobile-compact=mobile_compact class:k-panel--summary=matches!(presentation, PanelPresentation::Summary)>
            <header class="k-panel__header"><div>{eyebrow.map(|text| view! { <span class="k-panel__eyebrow">{text}</span> })}<h2 class="k-panel__title">{title}</h2>{description.map(|text| view! { <p class="k-panel__description">{text}</p> })}</div>{actions.map(|slot| view! { <div class="k-panel__actions">{slot()}</div> })}</header>
            <div class="k-panel__body">{children()}</div>
            {footer.map(|slot| view! { <footer class="k-panel__footer">{slot()}</footer> })}
        </section>
    }
}

/// A labelled input that retains the user's raw text, including incomplete numbers.
#[component]
pub fn BreweryField(
    #[prop(into)] id: String,
    #[prop(into)] label: String,
    value: RwSignal<String>,
    #[prop(default = "text")] input_type: &'static str,
    #[prop(optional, into)] unit: Option<String>,
    #[prop(optional, into)] hint: Option<String>,
    #[prop(optional, into)] error: Option<String>,
    #[prop(default = false)] disabled: bool,
    #[prop(default = false)] numeric: bool,
    #[prop(default = false)] read_only: bool,
) -> impl IntoView {
    view! { <BreweryDraftField id=id label=label value=Signal::from(value)
        on_change=Callback::new(move |next| value.set(next)) input_type=input_type
        unit=unit.unwrap_or_default() hint=MaybeProp::from(hint)
        error=MaybeProp::from(error) disabled=disabled numeric=numeric read_only=read_only/> }

}

/// The application's style comparison, displayed without inferring validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleState {
    Below,
    Within,
    Above,
    Awaiting,
}
impl StyleState {
    fn label(self) -> &'static str {
        match self {
            Self::Below => "Below style",
            Self::Within => "Within style",
            Self::Above => "Above style",
            Self::Awaiting => "Awaiting estimate",
        }
    }
    fn class(self) -> &'static str {
        match self {
            Self::Below => "k-style-range k-style-range--below",
            Self::Within => "k-style-range k-style-range--within",
            Self::Above => "k-style-range k-style-range--above",
            Self::Awaiting => "k-style-range k-style-range--awaiting",
        }
    }
}

#[component]
pub fn StyleRange(
    #[prop(into)] label: String,
    min: f64,
    max: f64,
    value: Option<f64>,
    state: StyleState,
    #[prop(optional, into)] unit: String,
    #[prop(default = 1)] digits: usize,
) -> impl IntoView {
    let valid = min.is_finite() && max.is_finite() && min < max;
    let estimate = value
        .filter(|v| v.is_finite())
        .filter(|_| valid && state != StyleState::Awaiting);
    let status = if estimate.is_some() {
        state
    } else {
        StyleState::Awaiting
    };
    let marker = estimate.map(|v| {
        format!(
            "--k-style-range-position:{}%",
            (25.0 + (v - min) / (max - min) * 50.0).clamp(2.0, 98.0)
        )
    });
    let mut precision = digits.min(6);
    if let Some(current) = estimate {
        let boundary = match status {
            StyleState::Below => Some(min),
            StyleState::Above => Some(max),
            _ => None,
        };
        if let Some(boundary) = boundary {
            while precision < 6
                && format!("{current:.precision$}") == format!("{boundary:.precision$}")
            {
                precision += 1;
            }
        }
    }
    let estimate_text = estimate
        .map(|v| format!("Estimate {v:.precision$} {unit}. "))
        .unwrap_or_default();
    let accessible = if valid {
        format!(
            "{label}. {estimate_text}Style reference {min:.precision$} to {max:.precision$} {unit}. {}.",
            status.label()
        )
    } else {
        format!("{label}. Style reference unavailable. Awaiting estimate.")
    };
    view! {
        <div class=status.class() role="img" aria-label=accessible>
            <div class="k-style-range__track" aria-hidden="true">{valid.then(|| view! { <span class="k-style-range__band"></span> })}{marker.map(|style| view! { <span class="k-style-range__marker" style=style></span> })}</div>
            <div class="k-style-range__limits" aria-hidden="true"><span>{if valid { format!("{min:.precision$}") } else { "—".into() }}</span><span>{if valid { format!("{max:.precision$}") } else { "—".into() }}</span></div>
            <span class="k-style-range__status" aria-hidden="true">{status.label()}</span>
        </div>
    }
}

#[component]
pub fn BreweryMetric(
    #[prop(into)] label: String,
    #[prop(into)] value: String,
    #[prop(optional, into)] unit: String,
    #[prop(optional, into)] hint: Option<String>,
    #[prop(optional)] density: MetricDensity,
    children: Children,
) -> impl IntoView {
    view! { <div class="k-metric" class:k-metric--compact=matches!(density, MetricDensity::Compact) class:k-metric--quiet=matches!(density, MetricDensity::Quiet)><div class="k-metric__label">{label}</div><div class="k-metric__value">{value}<span class="k-metric__unit">{unit}</span></div>{children()}{hint.map(|h| view! { <div class="k-metric__hint">{h}</div> })}</div> }
}

#[component]
pub fn BreweryNotice(
    #[prop(into)] title: String,
    #[prop(into)] body: String,
    #[prop(default = false)] warning: bool,
    #[prop(optional)] announcement: NoticeAnnouncement,
    #[prop(optional)] tone: NoticeTone,
) -> impl IntoView {
    let class = match if warning { NoticeTone::Warning } else { tone } {
        NoticeTone::Info => "k-notice",
        NoticeTone::Success => "k-notice k-notice--success",
        NoticeTone::Warning => "k-notice k-notice--warning",
        NoticeTone::Error => "k-notice k-notice--error",
    };
    view! { <div class=class role=matches!(announcement, NoticeAnnouncement::Settled).then_some("status")><div><div class="k-notice__title">{title}</div><div class="k-notice__body">{body}</div></div></div> }
}

/// A return bookmark owned by the logical task, not by a replaceable dialog view.
#[derive(Clone, Copy)]
pub struct BreweryDialogReturnFocus {
    _generation: StoredValue<u64>,
    content_generation: StoredValue<u64>,
}

impl BreweryDialogReturnFocus {
    /// Invalidate a deferred content handoff when its task/context is retired.
    pub fn cancel_content_focus(self) {
        self.content_generation.update_value(|value| *value = value.checked_add(1).expect("dialog content generation exhausted"));
    }

    /// Call before replacing an owned task's content. Native opening focus is
    /// insufficient when the renderer retains an already-open dialog element.
    pub fn focus_after_content_change(
        self,
        from_id: &str,
        to_id: &'static str,
        target_id: &'static str,
        current: impl FnOnce() -> bool + 'static,
    ) {
        self.cancel_content_focus();
        #[cfg(not(target_arch = "wasm32"))]
        let _ = (from_id, to_id, target_id, current);
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::{JsCast, closure::Closure};
            let Some(window) = web_sys::window() else { return; };
            let Some(document) = window.document() else { return; };
            let Some(native) = document.get_element_by_id(from_id)
                .and_then(|element| element.dyn_into::<web_sys::HtmlDialogElement>().ok()) else { return; };
            if !native.is_connected() || !native.open() || !native.matches(":modal").unwrap_or(false)
                || !document.has_focus().unwrap_or(false) { return; }
            let retired = document.active_element();
            if retired.as_ref().is_some_and(|active| !native.contains(Some(active))
                && active.tag_name() != "BODY" && active.tag_name() != "HTML") { return; }
            let generation = self.content_generation.get_value();
            let task_generation = self._generation.get_value();
            // Even a briefly focused newer destination takes precedence if it
            // disappears again before the handoff. Removal happens on the frame
            // or scheduling failure, not automatically on owner cancellation.
            let newer_focus = std::rc::Rc::new(std::cell::Cell::new(false));
            let observed = newer_focus.clone();
            let focus = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
                if event.target().and_then(|target| target.dyn_into::<web_sys::Element>().ok())
                    .is_some_and(|target| target.tag_name() != "BODY" && target.tag_name() != "HTML") {
                    observed.set(true);
                }
            });
            // JS owns the callback lifetime: even a native removal error cannot
            // leave a document listener referring to a dropped Rust Closure.
            let focus = focus.into_js_value();
            let cleanup = {
                let document = document.clone();
                let focus = focus.clone();
                move || {
                    if let Err(error) = document.remove_event_listener_with_callback_and_bool("focusin", focus.unchecked_ref(), true) {
                        tracing::warn!(error = ?error, "Unable to remove dialog content focus observer");
                    }
                }
            };
            if let Err(error) = document.add_event_listener_with_callback_and_bool("focusin", focus.unchecked_ref(), true) {
                cleanup();
                tracing::warn!(error = ?error, "Unable to observe dialog content focus");
                return;
            }
            let scheduling_failed_cleanup = cleanup.clone();
            // Reactive content commits before this one frame; no polling/retry.
            if let Err(error) = request_animation_frame_with_handle(move || {
                cleanup();
                if self.content_generation.try_get_value() != Some(generation)
                    || self._generation.try_get_value() != Some(task_generation)
                    || !current() || newer_focus.get()
                    || !document.has_focus().unwrap_or(false)
                    || !native.is_connected() || !native.open() || !native.matches(":modal").unwrap_or(false)
                    || document.get_element_by_id(to_id) != Some(native.clone().into()) { return; }
                // Do not overwrite a surviving owned control or newer/foreign
                // focus. Only retired owned focus or its BODY fallback is ours.
                if retired.as_ref().is_some_and(|element| element.tag_name() != "BODY" && element.tag_name() != "HTML" && element.is_connected())
                    || document.active_element().is_some_and(|active| active.tag_name() != "BODY" && active.tag_name() != "HTML") { return; }
                let Ok(layers) = document.query_selector_all("dialog:modal, [popover]:popover-open") else { return; };
                if (0..layers.length()).any(|index| layers.item(index).is_some_and(|layer| layer != web_sys::Node::from(native.clone()))) { return; }
                let Some(target) = document.get_element_by_id(target_id)
                    .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok()) else { return; };
                let Ok(Some(style)) = window.get_computed_style(&target) else { return; };
                if !native.contains(Some(&target)) || !target.is_connected() || target.get_client_rects().length() == 0
                    || target.closest("[hidden], [inert], :disabled, [aria-disabled='true']").ok().flatten().is_some()
                    || !style.get_property_value("visibility").is_ok_and(|value| value == "visible") { return; }
                if let Err(error) = target.focus() {
                    tracing::warn!(error = ?error, "Unable to initialize changed dialog content focus");
                }
            }) {
                scheduling_failed_cleanup();
                tracing::warn!(error = ?error, "Unable to schedule dialog content focus");
            }
        }
    }
}

/// Register once in the stable task owner when dialog views can be replaced.
/// Ordinary dialogs register the same bookmark locally by default.
pub fn use_brewery_dialog_return_focus(
    task_open: RwSignal<bool>,
    return_focus_allowed: Option<Signal<bool>>,
    return_focus_fallback: Option<String>,
) -> BreweryDialogReturnFocus {
    let task_generation = StoredValue::new(0_u64);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (task_open, return_focus_allowed, return_focus_fallback);
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let was_open = StoredValue::new(false);
        let opener = StoredValue::new_local(None::<web_sys::HtmlElement>);
        Effect::new(move |_| {
            let logical = task_open.get();
            let previous = was_open.get_value();
            was_open.set_value(logical);
            if logical && !previous {
                task_generation.update_value(|value| *value = value.checked_add(1).expect("dialog task generation exhausted"));
                opener.set_value(web_sys::window().and_then(|window| window.document())
                    .and_then(|document| document.active_element())
                    .filter(|element| element.tag_name() != "BODY" && element.tag_name() != "HTML")
                    .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok()));
            } else if !logical && previous {
                let target = opener.get_value();
                opener.set_value(None);
                let generation = task_generation.get_value();
                let fallback = return_focus_fallback.clone();
                request_animation_frame(move || {
                    if task_open.try_get_untracked() != Some(false)
                        || task_generation.try_get_value() != Some(generation)
                        || return_focus_allowed.is_some_and(|allowed| allowed.try_get_untracked() != Some(true)) {
                        return;
                    }
                    let Some(window) = web_sys::window() else { return; };
                    let Some(document) = window.document() else { return; };
                    let Ok(active_layer) = document.query_selector("dialog:modal, [popover]:popover-open") else { return; };
                    if !document.has_focus().unwrap_or(false) || active_layer.is_some() { return; }
                    // Native return or newer deliberate focus takes priority. A
                    // stale scroll-container focus reference inside a now-closed
                    // shared dialog with no layout is neither of those.
                    if document.active_element().is_some_and(|element| {
                        let stale_closed_dialog = element.get_client_rects().length() == 0
                            && element.closest("dialog.k-dialog:not([open])").is_ok_and(|dialog| dialog.is_some());
                        element.tag_name() != "BODY" && element.tag_name() != "HTML" && !stale_closed_dialog
                    }) { return; }
                    let try_focus = |target: &web_sys::HtmlElement| {
                        let Ok(Some(style)) = window.get_computed_style(target) else { return false; };
                        if !target.is_connected() || target.get_client_rects().length() == 0
                            || target.closest("[hidden], [inert], :disabled, [aria-disabled='true']").ok().flatten().is_some()
                            || !style.get_property_value("visibility").is_ok_and(|value| value == "visible") { return false; }
                        target.focus().is_ok() && document.active_element().is_some_and(|active| active == web_sys::Element::from(target.clone()))
                    };
                    if target.as_ref().is_some_and(try_focus) { return; }
                    if let Some(successor) = fallback.and_then(|id| document.get_element_by_id(&id))
                        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok()) {
                        if !try_focus(&successor) { tracing::warn!("Unable to return focus to the dialog successor"); }
                    }
                });
            }
        });
    }
    BreweryDialogReturnFocus { _generation: task_generation, content_generation: StoredValue::new(0_u64) }
}

/// A native modal dialog supplies focus containment, Escape and focus return.
#[component]
pub fn BreweryDialog(
    #[prop(into)] id: String,
    #[prop(into)] title: String,
    #[prop(into)] description: String,
    open: RwSignal<bool>,
    /// Wider content column for composed pickers and forms.
    #[prop(default = false)]
    wide: bool,
    /// Custom action row; the default is a single Close button.
    #[prop(optional)]
    actions: Option<ChildrenFn>,
    /// Logical task lifetime when native visibility may be temporarily hidden.
    #[prop(optional)]
    logical_open: Option<RwSignal<bool>>,
    /// Caller authority gate; a hidden/denied task must not focus protected UI.
    #[prop(optional)]
    return_focus_allowed: Option<Signal<bool>>,
    /// Visible task-owned successor if the original opener was removed.
    #[prop(optional)]
    return_focus_fallback: Option<String>,
    /// Reuse a bookmark registered in the caller's stable logical-task owner.
    #[prop(optional)]
    return_focus: Option<BreweryDialogReturnFocus>,
    children: Children,
) -> impl IntoView {
    let dialog = NodeRef::<leptos::html::Dialog>::new();
    let programmatic_closes = RwSignal::new(0_u32);
    let title_id = format!("{id}-title");
    let description_id = format!("{id}-description");
    if return_focus.is_none() {
        use_brewery_dialog_return_focus(logical_open.unwrap_or(open), return_focus_allowed, return_focus_fallback);
    }
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        Effect::new(move |_| {
            let is_open = open.get();
            if let Some(element) = dialog.get() {
                let native: &web_sys::HtmlDialogElement = element.unchecked_ref();
                if is_open && !native.open() {
                    if let Err(error) = native.show_modal() {
                        tracing::warn!(error = ?error, "Unable to open dialog");
                    }
                }
                if !is_open && native.open() {
                    programmatic_closes.update(|count| *count += 1);
                    native.close();
                }
            }
        });
    }
    let action_row = match actions {
        Some(actions) => actions(),
        None => view! {
            <crate::components::Button variant=crate::components::ButtonVariant::Secondary on:click=move |_| open.set(false)>"Close"</crate::components::Button>
        }
        .into_any(),
    };
    view! {
        <dialog id=id class=if wide { "k-dialog k-dialog--wide" } else { "k-dialog" } node_ref=dialog aria-labelledby=title_id.clone() aria-describedby=description_id.clone() on:close=move |_| {
            if programmatic_closes.get_untracked() > 0 { programmatic_closes.update(|count| *count -= 1); } else { open.set(false); }
        }>
            <header class="k-dialog__header"><div><h2 id=title_id.clone()>{title}</h2><p id=description_id.clone()>{description}</p></div></header>
            <div class="k-dialog__body">{children()}</div>
            <div class="k-dialog__actions">{action_row}</div>
        </dialog>
    }
}

/// Shared compact layout; callers retain ownership of their controlled inputs.
#[component]
pub fn BreweryCompactIngredientRow(
    label: Signal<String>,
    description: Signal<String>,
    name_field: AnyView,
    amount_field: AnyView,
    /// Routine schedule controls stay visible; details are for secondary specs.
    #[prop(optional)] schedule: Option<AnyView>,
    details: AnyView,
    on_remove: Callback<()>,
) -> impl IntoView {
    view! { <div class="k-compact-ingredient" role="group" aria-label=move || label.get()>
        <div class="k-compact-ingredient__main">{name_field}{amount_field}{schedule.map(|slot| view! { <div class="k-compact-ingredient__schedule">{slot}</div> })}<crate::components::Button variant=crate::components::ButtonVariant::GhostDestructive aria_label=Signal::derive(move || format!("Remove {}", label.get())) on:click=move |_| on_remove.run(())>"Remove addition"</crate::components::Button></div>
        {move || { let text = description.get(); (!text.is_empty()).then(|| view! { <p class="k-compact-ingredient__description">{text}</p> }) }}
        <BreweryDisclosure title="Addition details" compact=true aria_label=Signal::derive(move || format!("Addition details for {}", label.get()))>{details}</BreweryDisclosure>
    </div> }
}

/// Ingredient identity and quantity controls retain an accessible name on mobile.
#[component]
pub fn BreweryIngredientRow(
    #[prop(into)] name: String,
    #[prop(into)] description: String,
    #[prop(into)] unit: String,
    amount: RwSignal<String>,
    on_remove: Callback<()>,
) -> impl IntoView {
    let input_label = format!("{name} amount ({unit})");
    let remove_label = format!("Remove {name}");
    view! {
        <div class="k-ingredient-row k-ingredient-row--grain">
            <div class="k-ingredient-row__identity"><span class="k-ingredient-row__icon" aria-hidden="true"><span class="k-ingredient-row__swatch"></span></span><div><div class="k-ingredient-row__name">{name}</div><div class="k-ingredient-row__subtitle">{description}</div></div></div>
            <div class="k-ingredient-row__amount"><input type="text" inputmode="decimal" aria-label=input_label prop:value=move || amount.get() on:input=move |ev| amount.set(event_target_value(&ev))/><span class="k-ingredient-row__unit">{unit}</span></div>
            <div class="k-ingredient-row__percent" aria-hidden="true"></div>
            <div class="k-ingredient-row__actions"><crate::components::Button variant=crate::components::ButtonVariant::GhostDestructive size=crate::components::ButtonSize::Icon aria_label=remove_label on:click=move |_| on_remove.run(())>"×"</crate::components::Button></div>
        </div>
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveState {
    Saved,
    Unsaved,
    Saving,
    Conflict,
    Error,
}
#[component]
pub fn BrewerySaveStatus(state: SaveState, #[prop(optional)] quiet: bool) -> impl IntoView {
    let (class, label) = match state {
        SaveState::Saved => ("k-save-status k-save-status--saved", "Saved"),
        SaveState::Unsaved => ("k-save-status k-save-status--unsaved", "Unsaved changes"),
        SaveState::Saving => ("k-save-status", "Saving"),
        SaveState::Conflict => ("k-save-status k-save-status--conflict", "Changed elsewhere"),
        SaveState::Error => ("k-save-status k-save-status--error", "Save failed"),
    };
    view! { <span class=class role=(!quiet).then_some("status")><span class="k-save-status__dot" aria-hidden="true"></span>{label}</span> }
}

/// Labelled select used by recipe and showcase; values remain caller-owned.
#[component]
pub fn BrewerySelect(
    #[prop(into)] id: String,
    #[prop(into)] label: String,
    value: RwSignal<String>,
    options: Vec<(String, String)>,
    #[prop(optional, into)] hint: Option<String>,
) -> impl IntoView {
    view! { <BreweryDraftSelect id=id label=label value=Signal::from(value)
        on_change=Callback::new(move |next| value.set(next)) options=options
        hint=MaybeProp::from(hint)/> }

}

#[component]
pub fn BreweryTextArea(
    #[prop(into)] id: String,
    #[prop(into)] label: String,
    value: RwSignal<String>,
    #[prop(optional, into)] hint: Option<String>,
) -> impl IntoView {
    view! { <BreweryDraftArea id=id label=label value=Signal::from(value)
        on_change=Callback::new(move |next| value.set(next)) hint=MaybeProp::from(hint)/> }

}

/// Decorative portrait shared by formulation and the component showcase.
#[component]
pub fn BreweryBeerPortrait() -> impl IntoView {
    view! { <div class="beer-portrait" aria-hidden="true"><span class="beer-portrait__halo"></span><svg viewBox="0 0 100 140" fill="none"><path class="beer-glass__outline" d="M28 19h44l-4 91c-.3 8-8 14-18 14s-17.7-6-18-14L28 19Z"/><path class="beer-glass__liquid" d="M31 41h38l-3 67c-.3 7-7 12-16 12s-15.7-5-16-12l-3-67Z"/><path class="beer-glass__foam" d="M30 23h40l-.8 18c-5 2-8-1-12 1s-7-1-12 1-9-1-14-1L30 23Z"/><path class="beer-glass__shine" d="m37 51 2 52M40 112l4 3"/><circle class="beer-glass__bubble" cx="55" cy="66" r="1.2"/><circle class="beer-glass__bubble" cx="46" cy="89" r="1"/><circle class="beer-glass__bubble" cx="58" cy="101" r="1.2"/><path class="beer-glass__base" d="M36 130h28"/></svg></div> }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use leptos::tachys::view::RenderHtml;

    #[test]
    fn dialog_logical_lifetime_does_not_expose_native_closed_markup() {
        let owner = Owner::new();
        owner.with(|| {
            let native = RwSignal::new(false);
            let logical = RwSignal::new(true);
            let allowed = RwSignal::new(false);
            let html = view! {
                <BreweryDialog id="contract-dialog" title="Retained task" description="Checking its original context."
                    open=native logical_open=logical return_focus_allowed=Signal::from(allowed)
                    return_focus_fallback="task-successor".to_owned()>
                    <p>"Caller-owned content"</p>
                </BreweryDialog>
            }.to_html();
            assert!(html.contains("<dialog"));
            assert!(html.contains("aria-labelledby=\"contract-dialog-title\""));
            assert!(html.contains("aria-describedby=\"contract-dialog-description\""));
            assert!(html.contains("k-dialog__body"));
            assert!(html.contains("k-dialog__actions"));
            assert!(!html.contains(" open"));
            assert!(!html.contains("aria-modal"));
            assert!(logical.get_untracked());
            assert!(!native.get_untracked());
            assert!(!allowed.get_untracked());
            let focus_return = use_brewery_dialog_return_focus(logical, Some(Signal::from(allowed)), Some("task-successor".to_owned()));
            let replacement = view! {
                <BreweryDialog id="replacement-dialog" title="Same logical task" description="A replaceable view."
                    open=native return_focus=focus_return>
                    <p>"Same task-owned return bookmark"</p>
                </BreweryDialog>
            }.to_html();
            assert!(replacement.contains("aria-labelledby=\"replacement-dialog-title\""));
            assert!(!replacement.contains(" open"));
            assert!(logical.get_untracked());
            assert!(!native.get_untracked());
        });
    }

    #[test]
    fn panel_slots_and_field_states_are_shared_semantic_contracts() {
        let owner = Owner::new();
        owner.with(|| {
            let raw = RwSignal::new("1e-".to_owned());
            let html = view! {
                <BreweryPanel title="Equipment context" presentation=PanelPresentation::Summary
                    actions=std::sync::Arc::new(|| view! { <BreweryBadge>"Recipe only"</BreweryBadge> }.into_any())
                    footer=std::sync::Arc::new(|| view! { <BreweryActionGroup><span>"Source retained"</span></BreweryActionGroup> }.into_any())>
                    <BreweryFieldRow><BreweryDraftField id="raw-contract" label="Quantity" unit="kg" value=Signal::from(raw)
                        on_change=Callback::new(move |value| raw.set(value)) numeric=true read_only=true
                        hint="Original raw entry" on_blur=Callback::new(|()| {})/></BreweryFieldRow>
                </BreweryPanel>
            }.to_html();
            assert!(html.contains("k-panel__actions"));
            assert!(html.contains("k-panel__footer"));
            assert!(html.contains("Recipe only"));
            assert!(html.contains("k-field--numeric"));
            assert!(html.contains("type=\"text\""));
            assert!(html.contains("inputmode=\"decimal\""));
            assert!(html.contains("readonly"));
            assert!(!html.contains("disabled"));
            assert!(html.contains("aria-describedby=\"raw-contract-description\""));
            assert_eq!(raw.get_untracked(), "1e-");
        });
    }

    #[test]
    fn persistent_help_is_not_an_automatic_live_announcement() {
        let persistent = view! { <BreweryNotice title="Source help" body="This remains available while editing."/> }.to_html();
        assert!(!persistent.contains("role=\"status\""));
        let settled = view! { <BreweryNotice title="Saved" body="Revision 4 saved." announcement=NoticeAnnouncement::Settled/> }.to_html();
        assert!(settled.contains("role=\"status\""));
    }

    #[test]
    fn ordinary_popover_exposes_native_relationship_without_menu_roles() {
        let owner = Owner::new();
        owner.with(|| {
            let open = RwSignal::new(false);
            let html = view! { <crate::components::BreweryPopover id="contract-actions" label="Recipe actions" open=open>
                <a href="/recipes">"Recipes"</a>
            </crate::components::BreweryPopover> }.to_html();
            assert!(html.contains("popovertarget=\"contract-actions\""));
            assert!(html.contains("aria-controls=\"contract-actions\""));
            assert!(html.contains("aria-expanded=\"false\""));
            assert!(html.contains("popover=\"auto\""));
            assert!(!html.contains("role=\"menu"));
            assert!(!html.contains("dialog"));
        });
    }

    #[test]
    fn fields_expose_units_and_associated_errors() {
        let owner = Owner::new();
        owner.with(|| {
            let value = RwSignal::new("-1".to_owned());
            let html = view! { <BreweryField id="amount" label="Amount" value=value unit="kg" error="Enter a non-negative amount"/> }.to_html();
            assert!(html.contains("for=\"amount\""));
            assert!(html.contains("aria-invalid=\"true\""));
            assert!(html.contains("aria-describedby=\"amount-description\""));
            assert!(html.contains("Enter a non-negative amount"));
            assert!(html.contains("kg"));
        });
    }

    #[test]
    fn controlled_choices_and_notes_expose_descriptions_and_disabled_state() {
        let owner = Owner::new();
        owner.with(|| {
            let value = RwSignal::new("a".to_owned());
            let html = view! {
                <BreweryDraftSelect id="style" label="Style" value=Signal::from(value)
                    on_change=Callback::new(move |next| value.set(next))
                    options=vec![("a".into(), "A".into())] hint="Choose a style" disabled=true/>
                <BreweryDraftArea id="notes" label="Notes" value=Signal::from(value)
                    on_change=Callback::new(move |next| value.set(next)) error="Notes required" disabled=true/>
            }.to_html();
            assert!(html.contains("aria-describedby=\"style-description\""));
            assert!(html.contains("Choose a style"));
            assert!(html.contains("aria-describedby=\"notes-description\""));
            assert!(html.contains("Notes required"));
            assert!(html.contains("aria-invalid=\"true\""));
            assert!(html.matches("disabled").count() >= 2);
        });
    }

    #[test]
    fn unavailable_estimate_never_renders_a_stale_marker() {
        let html = view! { <StyleRange label="Gravity" min=1.044 max=1.048 value=None state=StyleState::Awaiting unit="SG" digits=3/> }.to_html();
        assert!(html.contains("Awaiting estimate"));
        assert!(!html.contains("k-style-range__marker"));
        let outside = view! { <StyleRange label="Gravity" min=1.044 max=1.048 value=Some(1.04804) state=StyleState::Above unit="SG" digits=3/> }.to_html();
        assert!(outside.contains("Above style"));
        assert!(outside.contains("1.04804"));
        assert!(outside.contains("k-style-range__marker"));
    }
}

/// Tabs use roving focus; callers provide the corresponding labelled panels.
#[component]
pub fn BreweryTabs(
    #[prop(into)] id: String,
    #[prop(into)] label: String,
    items: Vec<(String, String)>,
    active: RwSignal<String>,
) -> impl IntoView {
    let prefix = id.clone();
    view! { <div class="k-tabs" role="tablist" aria-label=label>
        {items.iter().enumerate().map(|(index, (key, text))| {
            let key_for_selected = key.clone();
            let key_for_tabindex = key.clone();
            let key_for_click = key.clone();
            let tab_id = format!("{prefix}-tab-{key}");
            let panel_id = format!("{prefix}-panel-{key}");
            let choices = items.clone();
            let focus_prefix = prefix.clone();
            let text = text.clone();
            view! { <button type="button" id=tab_id role="tab" aria-controls=panel_id
                aria-selected=move || (active.get() == key_for_selected).to_string()
                tabindex=move || if active.get() == key_for_tabindex { 0 } else { -1 }
                on:click=move |_| active.set(key_for_click.clone())
                on:keydown=move |event| {
                    let next = match event.key().as_str() { "ArrowRight" => Some((index + 1) % choices.len()), "ArrowLeft" => Some((index + choices.len() - 1) % choices.len()), "Home" => Some(0), "End" => Some(choices.len() - 1), _ => None };
                    if let Some(next) = next {
                        event.prevent_default();
                        let target = choices[next].0.clone();
                        active.set(target.clone());
                        #[cfg(target_arch = "wasm32")]
                        if let Some(element) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id(&format!("{focus_prefix}-tab-{target}"))) {
                            use wasm_bindgen::JsCast;
                            if let Some(element) = element.dyn_ref::<web_sys::HtmlElement>() { let _ = element.focus(); }
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        let _ = &focus_prefix;
                    }
                }>{text}</button> }
        }).collect_view()}
    </div> }
}

/// Search result with text and selection semantics independent of colour.
///
/// `on_select` (picker) runs instead of the default toggle so a caller can own
/// single-selection state; the showcase keeps the toggle behaviour.
#[component]
pub fn CatalogueItem(
    #[prop(into)] name: String,
    #[prop(into)] producer: String,
    #[prop(into)] details: String,
    #[prop(into)] source: String,
    #[prop(into)] selected: Signal<bool>,
    #[prop(optional)] on_select: Option<Callback<()>>,
) -> impl IntoView {
    view! { <button type="button" class="k-catalogue-item" class:k-catalogue-item--selected=move || selected.get() aria-pressed=move || selected.get().to_string() on:click=move |_| match on_select { Some(callback) => callback.run(()), None => {} }>
        <span class="k-catalogue-item__identity"><span class="k-catalogue-item__icon" aria-hidden="true">"✳"</span><span class="k-catalogue-item__text"><span class="k-catalogue-item__name">{name}</span><span class="k-catalogue-item__producer">{producer}</span></span></span>
        <span class="k-catalogue-item__meta"><span class="k-catalogue-item__specs">{details}</span><span class="k-badge">{source}</span></span>
        <span class="k-catalogue-item__selection" aria-hidden="true">{move || if selected.get() { "✓" } else { "" }}</span>
    </button> }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwatchRole {
    Background,
    Surface,
    Ink,
    Copper,
    Sage,
    Border,
}
impl SwatchRole {
    fn details(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Background => (
                "Background",
                "--k-color-background",
                "k-swatch__colour k-swatch__colour--background",
            ),
            Self::Surface => (
                "Surface",
                "--k-color-surface",
                "k-swatch__colour k-swatch__colour--surface",
            ),
            Self::Ink => (
                "Ink",
                "--k-color-foreground",
                "k-swatch__colour k-swatch__colour--ink",
            ),
            Self::Copper => (
                "Copper",
                "--k-color-primary",
                "k-swatch__colour k-swatch__colour--copper",
            ),
            Self::Sage => (
                "Sage",
                "--k-color-olive",
                "k-swatch__colour k-swatch__colour--sage",
            ),
            Self::Border => (
                "Border",
                "--k-color-border",
                "k-swatch__colour k-swatch__colour--border",
            ),
        }
    }
}
#[component]
pub fn TokenSwatch(role: SwatchRole) -> impl IntoView {
    let (label, token, class) = role.details();
    view! { <div class="k-swatch"><div class=class aria-hidden="true"></div><div class="k-swatch__label">{label}</div><div class="k-swatch__token">{token}</div></div> }
}

/// Controlled field for a caller-owned transient draft value and live diagnostics.
#[component]
pub fn BreweryDraftField(
    #[prop(into)] id: String,
    #[prop(into)] label: String,
    value: Signal<String>,
    on_change: Callback<String>,
    #[prop(optional, into)] unit: String,
    #[prop(optional, into)] error: MaybeProp<String>,
    #[prop(optional, into)] hint: MaybeProp<String>,
    #[prop(default = false)] numeric: bool,
    #[prop(default = false)] disabled: bool,
    #[prop(default = false)] mobile_title: bool,
    #[prop(default = false)] read_only: bool,
    /// Event notification only; formatting/validation policy stays with the caller.
    #[prop(optional)] on_blur: Option<Callback<()>>,
    #[prop(default = "text")] input_type: &'static str,
) -> impl IntoView {
    let description_id = format!("{id}-description");
    let described = description_id.clone();
    let label_unit = (!unit.is_empty()).then(|| format!(" ({unit})"));
    let display_unit = (!unit.is_empty()).then_some(unit);
    view! {
        <div class="k-field" class:k-field--mobile-title=mobile_title class:k-field--numeric=numeric>
            <label class="k-field__label" for=id.clone()>{label}{label_unit}</label>
            <div class="k-field__control">
                <input id=id type=input_type inputmode=if numeric { "decimal" } else { "text" } prop:value=move || value.get() on:input=move |ev| on_change.run(event_target_value(&ev)) on:blur=move |_| { if let Some(callback) = on_blur { callback.run(()); } } aria-invalid=move || error.get().is_some().to_string() aria-describedby=move || (error.get().is_some() || hint.get().is_some()).then(|| described.clone()) disabled=disabled readonly=read_only />
                {display_unit.map(|text| view! { <span class="k-field__unit" aria-hidden="true">{text}</span> })}
            </div>
            {move || error.get().map(|text| (text, true)).or_else(|| hint.get().map(|text| (text, false))).map(|(text, is_error)| view! { <span id=description_id.clone() class=if is_error { "k-field__error" } else { "k-field__hint" }>{text}</span> })}
        </div>
    }

}

/// Controlled choice without coupling a caller's model to presentation state.
#[component]
pub fn BreweryDraftSelect(
    #[prop(into)] id: String,
    #[prop(into)] label: String,
    value: Signal<String>,
    on_change: Callback<String>,
    options: Vec<(String, String)>,
    #[prop(optional, into)] hint: MaybeProp<String>,
    #[prop(optional, into)] error: MaybeProp<String>,
    #[prop(default = false)] disabled: bool,
) -> impl IntoView {
    let known = options.iter().map(|(v, _)| v.clone()).collect::<Vec<_>>();
    let description_id = format!("{id}-description");
    let described = description_id.clone();
    view! { <div class="k-field"><label class="k-field__label" for=id.clone()>{label}</label>
        <div class="k-field__control k-select"><select id=id prop:value=move || value.get() on:change=move |ev| on_change.run(event_target_value(&ev)) disabled=disabled aria-invalid=move || error.get().is_some().to_string() aria-describedby=move || (error.get().is_some() || hint.get().is_some()).then(|| described.clone())>
            {move || { let selected = value.get(); (!selected.is_empty() && !known.contains(&selected)).then(|| view! { <option value=selected.clone()>{format!("Unrecognized: {selected}")}</option> }) }}
            {options.into_iter().map(|(v, label)| view! { <option value=v>{label}</option> }).collect_view()}
        </select><span class="kit-select-chevron" aria-hidden="true">"⌄"</span></div>
        {move || error.get().map(|text| (text, true)).or_else(|| hint.get().map(|text| (text, false))).map(|(text, is_error)| view! { <span id=description_id.clone() class=if is_error { "k-field__error" } else { "k-field__hint" }>{text}</span> })}
    </div> }

}

/// Approved local mark and wordmark, shared by desktop and mobile navigation.
#[component]
pub fn BreweryBrand(#[prop(optional)] inverse: bool) -> impl IntoView {
    view! { <a href="/" class="k-brand" class:k-brand--inverse=inverse><img src="/public/brewery-mark.svg" alt=""/><span>"krausen"<span class="k-brand__dot">"."</span></span></a> }
}

/// A compact, keyboard-operable disclosure for secondary settings.
#[component]
pub fn BreweryDisclosure(
    #[prop(into)] title: String,
    #[prop(optional)] compact: bool,
    #[prop(optional, into)] aria_label: MaybeProp<String>,
    children: Children,
) -> impl IntoView {
    view! { <details class="k-disclosure" class:k-disclosure--compact=compact><summary aria-label=move || aria_label.try_get().flatten()>{title}</summary><div class="k-disclosure__body">{children()}</div></details> }
}

/// Controlled multiline text for descriptions and notes.
#[component]
pub fn BreweryDraftArea(
    #[prop(into)] id: String,
    #[prop(into)] label: String,
    value: Signal<String>,
    on_change: Callback<String>,
    #[prop(optional, into)] hint: MaybeProp<String>,
    #[prop(optional, into)] error: MaybeProp<String>,
    #[prop(default = false)] disabled: bool,
) -> impl IntoView {
    let description_id = format!("{id}-description");
    let described = description_id.clone();
    view! { <div class="k-field"><label class="k-field__label" for=id.clone()>{label}</label><div class="k-field__control"><textarea id=id rows="3" prop:value=move || value.get() on:input=move |ev| on_change.run(event_target_value(&ev)) disabled=disabled aria-invalid=move || error.get().is_some().to_string() aria-describedby=move || (error.get().is_some() || hint.get().is_some()).then(|| described.clone())></textarea></div>
        {move || error.get().map(|text| (text, true)).or_else(|| hint.get().map(|text| (text, false))).map(|(text, is_error)| view! { <span id=description_id.clone() class=if is_error { "k-field__error" } else { "k-field__hint" }>{text}</span> })}
    </div> }

}

/// A compact editable addition with shared spacing and separation.
#[component]
pub fn BreweryAddition(children: Children) -> impl IntoView {
    view! { <div class="k-addition">{children()}</div> }
}
