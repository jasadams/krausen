// SPDX-License-Identifier: AGPL-3.0-or-later

//! Shared recipe controls and presentation components.
use leptos::prelude::*;

/// Compact, titled surface shared by the editor and component showcase.
#[component]
pub fn BreweryPanel(
    #[prop(into)] title: String,
    #[prop(optional, into)] description: Option<String>,
    children: Children,
) -> impl IntoView {
    view! {
        <section class="k-panel">
            <header class="k-panel__header"><div><h2 class="k-panel__title">{title}</h2>{description.map(|text| view! { <p class="k-panel__description">{text}</p> })}</div></header>
            <div class="k-panel__body">{children()}</div>
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
) -> impl IntoView {
    view! { <BreweryDraftField id=id label=label value=Signal::from(value)
        on_change=Callback::new(move |next| value.set(next)) input_type=input_type
        unit=unit.unwrap_or_default() hint=MaybeProp::from(hint)
        error=MaybeProp::from(error) disabled=disabled/> }

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
    children: Children,
) -> impl IntoView {
    view! { <div class="k-metric"><div class="k-metric__label">{label}</div><div class="k-metric__value">{value}<span class="k-metric__unit">{unit}</span></div>{children()}{hint.map(|h| view! { <div class="k-metric__hint">{h}</div> })}</div> }
}

#[component]
pub fn BreweryNotice(
    #[prop(into)] title: String,
    #[prop(into)] body: String,
    #[prop(default = false)] warning: bool,
) -> impl IntoView {
    view! { <div class=if warning { "k-notice k-notice--warning" } else { "k-notice" } role="status"><div><div class="k-notice__title">{title}</div><div class="k-notice__body">{body}</div></div></div> }
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
    children: Children,
) -> impl IntoView {
    let dialog = NodeRef::<leptos::html::Dialog>::new();
    let programmatic_closes = RwSignal::new(0_u32);
    let title_id = format!("{id}-title");
    let description_id = format!("{id}-description");
    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        use wasm_bindgen::JsCast;
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
pub fn BrewerySaveStatus(state: SaveState) -> impl IntoView {
    let (class, label) = match state {
        SaveState::Saved => ("k-save-status k-save-status--saved", "Saved"),
        SaveState::Unsaved => ("k-save-status k-save-status--unsaved", "Unsaved changes"),
        SaveState::Saving => ("k-save-status", "Saving"),
        SaveState::Conflict => ("k-save-status k-save-status--conflict", "Changed elsewhere"),
        SaveState::Error => ("k-save-status k-save-status--error", "Save failed"),
    };
    view! { <span class=class role="status"><span class="k-save-status__dot" aria-hidden="true"></span>{label}</span> }
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

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use leptos::tachys::view::RenderHtml;

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
    #[prop(default = "text")] input_type: &'static str,
) -> impl IntoView {
    let description_id = format!("{id}-description");
    let described = description_id.clone();
    let label_unit = (!unit.is_empty()).then(|| format!(" ({unit})"));
    let display_unit = (!unit.is_empty()).then_some(unit);
    view! {
        <div class="k-field">
            <label class="k-field__label" for=id.clone()>{label}{label_unit}</label>
            <div class="k-field__control">
                <input id=id type=input_type inputmode=if numeric { "decimal" } else { "text" } prop:value=move || value.get() on:input=move |ev| on_change.run(event_target_value(&ev)) aria-invalid=move || error.get().is_some().to_string() aria-describedby=move || (error.get().is_some() || hint.get().is_some()).then(|| described.clone()) disabled=disabled />
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
pub fn BreweryBrand() -> impl IntoView {
    view! { <a href="/" class="k-brand"><img src="/public/brewery-mark.svg" alt=""/><span>"krausen"<span class="k-brand__dot">"."</span></span></a> }
}

/// A compact, keyboard-operable disclosure for secondary settings.
#[component]
pub fn BreweryDisclosure(#[prop(into)] title: String, children: Children) -> impl IntoView {
    view! { <details class="k-disclosure"><summary>{title}</summary><div class="k-disclosure__body">{children()}</div></details> }
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
