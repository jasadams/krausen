// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared wayfinding: page title and the authorized breadcrumb trail (IA-007,
//! IA-011). Pages place these; they never restyle or re-implement them.

use leptos::prelude::*;

/// One breadcrumb step. `href: None` is the current page.
#[derive(Clone, Debug, PartialEq)]
pub struct Crumb {
    pub label: String,
    pub href: Option<String>,
}

impl Crumb {
    pub fn link(label: impl Into<String>, href: impl Into<String>) -> Self {
        Self { label: label.into(), href: Some(href.into()) }
    }
    pub fn current(label: impl Into<String>) -> Self {
        Self { label: label.into(), href: None }
    }
}

/// Meaningful document title, `"{text} · Krausen"`.
#[component]
pub fn PageTitle(#[prop(into)] text: Signal<String>) -> impl IntoView {
    view! { <leptos_meta::Title text=move || format!("{} · Krausen", text.get())/> }
}

/// Named breadcrumb landmark. The final step is the exact current page.
#[component]
pub fn Breadcrumb(
    #[prop(into)] items: Signal<Vec<Crumb>>,
    /// Distinguishes several trails on one page; the landmark name is stable.
    #[prop(optional, into)]
    id: Option<String>,
) -> impl IntoView {
    view! {
        <nav class="brewery-breadcrumb" aria-label="Breadcrumb" id=id>
            <ol class="brewery-breadcrumb__list">
                {move || {
                    let crumbs = items.get();
                    let last = crumbs.len().saturating_sub(1);
                    crumbs.into_iter().enumerate().map(|(index, crumb)| match crumb.href {
                        Some(href) if index != last => view! {
                            <li class="brewery-breadcrumb__item"><a href=href>{crumb.label}</a></li>
                        }.into_any(),
                        _ => view! {
                            <li class="brewery-breadcrumb__item"><span aria-current="page">{crumb.label}</span></li>
                        }.into_any(),
                    }).collect_view()
                }}
            </ol>
        </nav>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breadcrumb_is_a_named_landmark_with_one_current_page() {
        let owner = Owner::new();
        owner.with(|| {
            let html = view! {
                <Breadcrumb items=vec![Crumb::link("Recipes", "/recipes"), Crumb::link("Helles", "/recipe-editor/r"), Crumb::current("Brews")]/>
            }.to_html();
            assert!(html.contains("aria-label=\"Breadcrumb\""));
            assert!(html.contains("href=\"/recipes\""));
            assert!(html.contains("href=\"/recipe-editor/r\""));
            assert_eq!(html.matches("aria-current=\"page\"").count(), 1);
            assert!(html.contains(">Brews<"));
        });
    }

    #[test]
    fn a_trailing_link_is_still_rendered_as_the_current_page() {
        let owner = Owner::new();
        owner.with(|| {
            let html = view! { <Breadcrumb items=vec![Crumb::link("Recipes", "/recipes"), Crumb::link("Here", "/x")]/> }.to_html();
            assert_eq!(html.matches("aria-current=\"page\"").count(), 1);
            assert!(!html.contains("href=\"/x\""));
        });
    }
}
