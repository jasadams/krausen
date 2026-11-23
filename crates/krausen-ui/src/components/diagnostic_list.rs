// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared kit list of resolved diagnostics (UX-006). The recipe editor and the
//! design-system showcase render this one component.
use crate::components::{Button, ButtonSize, ButtonVariant};
use krausen_types::validation::Readiness;
use leptos::prelude::*;

/// Where a diagnostic can be repaired.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    /// An editable control with this stable DOM id.
    Field { dom_id: String },
    /// Preserved data this editor cannot correct: an honest explanation, no link.
    Explain,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Presented {
    /// Stable DOM id of this message in the summary; the fallback focus target.
    pub key: String,
    pub path: String,
    pub code: String,
    pub severity: Readiness,
    /// Full sentence, naming the row and field.
    pub message: String,
    pub route: Route,
}

/// Moves to a control on the user's explicit request: the owner selects the
/// Recipe tab, opens collapsed ancestors, waits for the render, then focuses the
/// control (first id) or, when it is not rendered, the explanation (second id).
#[derive(Clone, Copy)]
pub struct DiagnosticJump(pub Callback<(String, String)>);

/// The detailed list of what is wrong. Deliberately not a live region: the one
/// concise readiness status announces the settled count (ACC-005). While `stale`
/// the entries belong to an earlier draft and say so; they stay readable and
/// focusable, never hidden from assistive technology.
#[component]
pub fn DiagnosticList(
    items: Signal<Vec<Presented>>,
    #[prop(optional, into)] stale: Signal<bool>,
    /// Unique per page when more than one list is shown (the showcase).
    #[prop(default = "recipe-diagnostics-heading")] heading_id: &'static str,
) -> impl IntoView {
    let jump = use_context::<DiagnosticJump>();
    let group = move |severity: Readiness, heading: &'static str| {
        move || {
            let list: Vec<Presented> = items.get().into_iter().filter(|p| p.severity == severity).collect();
            (!list.is_empty()).then(|| {
                let count = list.len();
                view! {
                    <h4 class="k-diagnostics__heading">{format!("{heading} ({count})")}</h4>
                    <ul class="k-diagnostics__list">
                        {list.into_iter().map(|p| {
                            let Presented { key, message, route, .. } = p;
                            let body = match (route, jump) {
                                (Route::Field { dom_id }, Some(DiagnosticJump(jump))) => {
                                    let fallback = key.clone();
                                    view! { <Button variant=ButtonVariant::Link size=ButtonSize::Wrap on:click=move |_| jump.run((dom_id.clone(), fallback.clone()))>{message}</Button> }.into_any()
                                }
                                _ => view! { <span>{message}</span> }.into_any(),
                            };
                            view! { <li id=key tabindex="-1">{body}</li> }
                        }).collect_view()}
                    </ul>
                }
            })
        }
    };
    view! {
        <Show when=move || items.with(|list| !list.is_empty())>
            <section class="k-diagnostics k-stale" class:k-stale--active=move || stale.get() aria-labelledby=heading_id>
                <h3 id=heading_id class="k-diagnostics__title">"Recipe issues"<span class="k-stale__cue" data-testid="diagnostics-stale">{move || stale.get().then_some(" · Out of date")}</span></h3>
                {group(Readiness::Invalid, "Correct before saving")}
                {group(Readiness::Incomplete, "Needed to start a brew")}
            </section>
        </Show>
    }
}
