// SPDX-License-Identifier: AGPL-3.0-or-later
//! Authorized parent wayfinding for recipe-local Brews, Measurements and
//! Compare (IA-007). The parent recipe ID is the route's or the authorized brew
//! view's own `recipe_id`, never derived from a brew ID. Its title is read
//! through the same shared library operations as the library and MCP; while it
//! loads, or when it is denied/missing, the trail shows only the neutral word
//! "Recipe" and never a title.

use crate::{
    components::wayfinding::{Breadcrumb, Crumb},
    server_fns::recipe_library::library_command_shared,
    utils::nav_state::{RecipeSection, recipe_section_href},
};
use krausen_types::{
    recipe_library::{HistoryQuery, LibraryCommand, LibraryOutcome, LibraryQuery, RevisionQuery},
    request_context::OperationIntent,
};
use leptos::prelude::*;

/// What is known about the parent recipe.
#[derive(Clone, Debug, PartialEq)]
pub enum ParentState {
    /// Not yet resolved, denied or missing: no title is shown.
    Unknown,
    Known { title: String, archived: bool },
}

/// Breadcrumb label and destination for a parent state: an archived recipe is
/// labelled as such and links to its saved versions rather than a dead end.
pub fn parent_crumb(recipe_id: &str, state: &ParentState) -> Crumb {
    match state {
        ParentState::Unknown => Crumb::link("Recipe", recipe_section_href(recipe_id, RecipeSection::Recipe, None)),
        ParentState::Known { title, archived: false } => {
            Crumb::link(title.clone(), recipe_section_href(recipe_id, RecipeSection::Recipe, None))
        }
        ParentState::Known { title, archived: true } => Crumb::link(
            format!("{title} (archived)"),
            recipe_section_href(recipe_id, RecipeSection::Versions, None),
        ),
    }
}

async fn resolve_parent(intent: OperationIntent, recipe_id: String) -> ParentState {
    let head = library_command_shared(LibraryCommand::History(HistoryQuery {
        intent: intent.clone(),
        recipe_id: recipe_id.clone(),
        cursor: None,
        limit: 1,
    }))
    .await;
    let Ok(LibraryOutcome::History(page)) = head else { return ParentState::Unknown };
    let Some(revision) = page.items.first().map(|entry| entry.revision) else { return ParentState::Unknown };
    let document = library_command_shared(LibraryCommand::Revision(RevisionQuery {
        intent: intent.clone(),
        recipe_id: recipe_id.clone(),
        revision,
    }))
    .await;
    let Ok(LibraryOutcome::Revision(document)) = document else { return ParentState::Unknown };
    let title = document.title;
    // Archived is only reported by the library list: look for this exact ID
    // among archived matches for its title.
    let archived = library_command_shared(LibraryCommand::List(LibraryQuery {
        intent,
        search: title.clone(),
        archived: true,
        cursor: None,
        limit: 50,
    }))
    .await;
    let archived = matches!(
        archived,
        Ok(LibraryOutcome::Recipes(page)) if page.items.iter().any(|item| item.recipe_id == recipe_id)
    );
    ParentState::Known { title, archived }
}

/// Which step of the trail is the current page.
#[derive(Clone, Debug, PartialEq)]
pub enum TrailEnd {
    /// `/recipes/:id/brews`.
    Brews,
    /// `/brews/:id`.
    Measurements { brew_id: String, brew_label: String },
    /// `/brews/:id/analysis`.
    Compare { brew_id: String, brew_label: String },
}

/// Recipes → recipe → Brews → selected brew → Measurements/Compare.
#[component]
pub fn RecipeTrail(
    intent: OperationIntent,
    /// `None` until an authorized view reports its parent.
    #[prop(into)]
    recipe_id: Signal<Option<String>>,
    #[prop(into)] end: Signal<TrailEnd>,
) -> impl IntoView {
    let intent = StoredValue::new(intent);
    let parent = RwSignal::new(ParentState::Unknown);
    let generation = RwSignal::new(0_u64);
    Effect::new(move |_| {
        let Some(id) = recipe_id.get() else {
            parent.set(ParentState::Unknown);
            return;
        };
        parent.set(ParentState::Unknown);
        generation.update(|n| *n += 1);
        let sequence = generation.get_untracked();
        let captured = intent.get_value();
        leptos::task::spawn_local(async move {
            let state = resolve_parent(captured, id).await;
            if generation.try_get_untracked() == Some(sequence) {
                parent.set(state);
            }
        });
    });
    let items = Signal::derive(move || {
        let mut crumbs = vec![Crumb::link("Recipes", "/recipes")];
        match recipe_id.get() {
            None => {
                crumbs.push(Crumb::current("Recipe"));
                return crumbs;
            }
            Some(id) => {
                crumbs.push(parent_crumb(&id, &parent.get()));
                let brews = format!("/recipes/{id}/brews");
                match end.get() {
                    TrailEnd::Brews => crumbs.push(Crumb::current("Brews")),
                    TrailEnd::Measurements { brew_id, brew_label } => {
                        crumbs.push(Crumb::link("Brews", brews));
                        crumbs.push(Crumb::link(brew_label, format!("/brews/{brew_id}")));
                        crumbs.push(Crumb::current("Measurements"));
                    }
                    TrailEnd::Compare { brew_id, brew_label } => {
                        crumbs.push(Crumb::link("Brews", brews));
                        crumbs.push(Crumb::link(brew_label, format!("/brews/{brew_id}")));
                        crumbs.push(Crumb::current("Compare"));
                    }
                }
            }
        }
        crumbs
    });
    view! { <Breadcrumb items=items/> }
}

/// Sibling links between one brew's Measurements and Compare pages.
#[component]
pub fn BrewSiblingLinks(#[prop(into)] brew_id: String, compare: bool) -> impl IntoView {
    let measurements = format!("/brews/{brew_id}");
    let comparison = format!("/brews/{brew_id}/analysis");
    view! {
        <nav class="brewery-breadcrumb" aria-label="This brew">
            <ul class="brewery-breadcrumb__list">
                <li class="brewery-breadcrumb__item"><a href=measurements aria-current=(!compare).then_some("page")>"Measurements"</a></li>
                <li class="brewery-breadcrumb__item"><a href=comparison aria-current=compare.then_some("page")>"Compare"</a></li>
            </ul>
        </nav>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_parent_never_shows_a_title() {
        let crumb = parent_crumb("r1", &ParentState::Unknown);
        assert_eq!(crumb.label, "Recipe");
        assert_eq!(crumb.href.as_deref(), Some("/recipe-editor/r1"));
    }

    #[test]
    fn archived_parent_is_labelled_and_links_to_saved_versions() {
        let crumb = parent_crumb("r1", &ParentState::Known { title: "Helles".into(), archived: true });
        assert_eq!(crumb.label, "Helles (archived)");
        assert_eq!(crumb.href.as_deref(), Some("/recipe-editor/r1?section=versions"));
        let active = parent_crumb("r1", &ParentState::Known { title: "Helles".into(), archived: false });
        assert_eq!(active.label, "Helles");
        assert_eq!(active.href.as_deref(), Some("/recipe-editor/r1"));
    }
}
