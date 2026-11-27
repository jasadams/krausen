// SPDX-License-Identifier: AGPL-3.0-or-later
//! Cross-file composition guards for the recipe page and the shared kit.
//!
//! Runtime behaviour is covered by the selected browser suites and the unit
//! tests next to each component (UX-004, UX-007, IA-006/007, ACC-002/005).
//! These tests pin the ownership facts those suites cannot name: which file
//! owns a control, that a secondary task stays out of the content layout, that
//! the recipe and showcase consume the same kit, and that local-storage
//! failures keep their typed outcome. They run in the foundation workflow.

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use crate::test_support::extract_between;

    const EDITOR: &str = include_str!("pages/recipe_editor.rs");
    const HEADER: &str = include_str!("pages/recipe_header.rs");
    const PROCESS: &str = include_str!("pages/recipe_process.rs");
    const ADDITIONS: &str = include_str!("pages/ingredient_addition.rs");
    const SHOWCASE: &str = include_str!("pages/design_system.rs");
    const HISTORY: &str = include_str!("components/recipe_revision_history.rs");
    const KIT: &str = include_str!("components/brewery_kit.rs");
    const KIT_CSS: &str = include_str!("../style/kit.css");

    #[track_caller]
    fn assert_contains(haystack: &str, needle: &str) {
        assert!(haystack.contains(needle), "missing {needle:?}");
    }

    #[track_caller]
    fn assert_missing(haystack: &str, needle: &str) {
        assert!(!haystack.contains(needle), "unexpected {needle:?}");
    }

    // The editor history is the shared component. Paging, inspection and the
    // context fences are runtime behaviour in the library suites; the exact
    // affordances are pinned here because no selected browser test grows a
    // recipe past the page size or opens the complete-data disclosure.
    #[test]
    fn revision_history_keeps_pagination_inspection_and_context_fences() {
        for required in [
            "LibraryCommand::History",
            "LibraryCommand::Revision",
            "next_cursor",
            "Load more revisions",
            "Complete saved data",
            "current_context_matches",
            "refresh_key.try_get_untracked()",
            "authority.lost_or_reload(reload)",
        ] {
            assert_contains(HISTORY, required);
        }
    }

    // IA-006/007, VIS-002/003: one editable identity and one breadcrumb trail,
    // both owned by the extracted header. The recipe title is the shared
    // title variant, not a page-local mobile title.
    #[test]
    fn recipe_header_owns_the_topbar_breadcrumb_and_title_variant() {
        for required in [
            "class=\"recipe-header\"",
            "wayfinding::Breadcrumb",
            "Crumb::link(\"Recipes\", \"/recipes\")",
            "title_variant=true",
        ] {
            assert_contains(HEADER, required);
        }
        assert_missing(HEADER, "aria-label=\"Recipe breadcrumb\"");
        assert_missing(EDITOR, "class=\"recipe-editor__topbar\"");
        assert_missing(EDITOR, "mobile_title");
        assert_contains(KIT_CSS, ".k-field--title");
        assert_missing(KIT_CSS, ".k-field--mobile-title");
        assert_contains(SHOWCASE, "title_variant=true");
    }

    // UX-004/007, IA-009: each control has one owner. The editor composes the
    // shared additions and history surfaces; the process sections own the
    // water plan, boil time and stage copy; Print/Export stay header tasks, not
    // content; the preview rail keeps summary, content and details in order.
    #[test]
    fn editor_and_process_have_one_owner_per_control() {
        for required in [
            "<BreweryTabs id=\"editor\" label=\"Recipe sections\"",
            "role=\"tabpanel\"",
            "id=\"editor-panel-recipe\"",
            "id=\"editor-panel-versions\"",
            "id=\"editor-panel-brews\"",
            "aria-labelledby=\"editor-tab-recipe\"",
            "aria-labelledby=\"editor-tab-versions\"",
            "aria-labelledby=\"editor-tab-brews\"",
            "(\"recipe\".into(), \"Recipe\".into())",
            "(\"versions\".into(), \"Saved versions\".into())",
            "(\"brews\".into(), \"Brews\".into())",
            "<crate::components::RecipeRevisionHistory",
            "BrewHistoryView embedded=true recipe_id=id intent=identity.get_untracked()/>",
        ] {
            assert_contains(EDITOR, required);
        }
        assert_missing(EDITOR, "fn EditorHistory(");

        let layout = extract_between(
            EDITOR,
            "<div class=\"recipe-editor__layout\">",
            "</FormulationPreview></div>",
        );
        assert_contains(
            layout,
            "<crate::pages::ingredient_addition::IngredientAdditions",
        );
        assert_missing(layout, "<crate::pages::beerxml::BeerXmlExport");
        assert_contains(EDITOR, "<crate::pages::beerxml::BeerXmlExport");

        for section in [
            "Mash & water",
            "Boil & whirlpool",
            "Fermentation",
            "Description & notes",
        ] {
            assert_contains(PROCESS, &format!("title=\"{section}\""));
        }
        assert_contains(PROCESS, "title=\"Water plan\"");
        assert_missing(EDITOR, "title=\"Water plan\"");
        assert_missing(EDITOR, "<BreweryDisclosure title=\"Water plan details\">");
        assert_missing(EDITOR, "Recipe process and notes");
        assert_missing(EDITOR, "id=\"recipe-boil\"");
        assert_contains(EDITOR, "title=\"Your equipment\"");

        let preview = extract_between(EDITOR, "fn FormulationPreview(", "fn PreviewMetrics(");
        let summary = preview
            .find("recipe-editor__preview-summary")
            .expect("preview summary");
        let content = preview.find("{children()}").expect("preview content");
        let details = preview
            .find("recipe-editor__preview-details")
            .expect("preview details");
        assert!(summary < content, "summary must precede content");
        assert!(content < details, "details must follow content");
    }

    // ACC-005: a local-storage failure is retired only by a later storage
    // success. Every storage message therefore goes through fail_storage, not
    // the operation failure or a plain status set.
    #[test]
    fn local_draft_failures_use_the_storage_outcome() {
        for message in [
            "Unable to retain this draft locally",
            "An older retained draft could not be removed from this browser.",
            "The retained draft could not be removed from this browser.",
            "Could not remove the retained draft.",
            "Could not discard the retained local draft.",
            "Local draft recovery is unavailable in this browser.",
            "but the retained local draft could not be cleared",
        ] {
            let mut searched = 0;
            let mut found = false;
            while let Some(relative) = EDITOR[searched..].find(message) {
                let at = searched + relative;
                let mut start = at.saturating_sub(300);
                while !EDITOR.is_char_boundary(start) {
                    start += 1;
                }
                let before = &EDITOR[start..at];
                let writers = [
                    ("fail_storage", "fail_storage("),
                    ("fail", "status.fail("),
                    ("set", "status.set("),
                    ("fail_operation", "status.fail_operation("),
                ];
                let writer = writers
                    .iter()
                    .filter_map(|(name, needle)| {
                        before.rfind(needle).map(|position| (*name, position))
                    })
                    .max_by_key(|(_, position)| *position);
                assert_eq!(
                    writer.map(|(name, _)| name),
                    Some("fail_storage"),
                    "message {message:?} must use the storage outcome"
                );
                found = true;
                searched = at + 1;
            }
            assert!(
                found,
                "message {message:?} is no longer written by the editor"
            );
        }
    }

    // KRA-42: the editor composes one shared addition surface; the kit owns the
    // compact row grammar and exposes the detail/removal labels. The portrait
    // is the same approved decorative component in the editor and showcase.
    #[test]
    fn recipe_and_showcase_share_the_addition_and_portrait_surfaces() {
        assert_contains(ADDITIONS, "<BreweryCompactIngredientRow");
        assert_contains(SHOWCASE, "<BreweryCompactIngredientRow");
        for required in [
            "pub fn BreweryCompactIngredientRow(",
            "\"Addition details for {}\"",
            "\"Remove {}\"",
            "k-disclosure--compact",
        ] {
            assert_contains(KIT, required);
        }
        assert_contains(EDITOR, "<crate::components::BreweryBeerPortrait/>");
        assert_contains(SHOWCASE, "<crate::components::BreweryBeerPortrait/>");
        assert_contains(KIT, "class=\"beer-portrait\"");
        assert_contains(KIT, "class=\"beer-glass__liquid\"");
    }

    // UX-004, ACC-002: the process layout is one shared kit. Page-specific
    // selectors and hand-written wrappers must not return to the shared kit or
    // to a consumer.
    #[test]
    fn process_kit_styles_stay_shared_and_page_specific_rules_stay_out() {
        for required in [
            ".k-step {",
            ".k-step__index",
            ".k-step-group {",
            ".k-stale--active",
            ".k-stale__cue",
            ".k-panel--stacked",
            ".k-fields {",
            ".k-metric-list--total",
        ] {
            assert_contains(KIT_CSS, required);
        }
        for forbidden in [
            "[id^=\"recipe-section-\"]",
            ".recipe-editor__sections .k-section__header",
            ".k-live",
        ] {
            assert_missing(KIT_CSS, forbidden);
        }
        for component in [
            "BreweryStepGroup",
            "BreweryStepRow",
            "BreweryStale",
            "BreweryStaleCue",
            "BreweryFields",
        ] {
            assert_contains(KIT, &format!("pub fn {component}("));
            for consumer in [PROCESS, SHOWCASE] {
                assert_contains(consumer, &format!("<{component}"));
            }
        }
        assert_contains(PROCESS, "stacked=true");
        assert_contains(SHOWCASE, "stacked=true");
        for consumer in [PROCESS, SHOWCASE] {
            assert_missing(consumer, "class=\"k-step-group\"");
        }
        assert_contains(KIT, "class=\"k-step-group\"");

        // One mobile breakpoint for touch sizing; layout follows the group's
        // width through the container query. The compact grid is the default
        // and `lh` keeps its em fallback for engines without it.
        let step_css = &KIT_CSS[KIT_CSS.find(".k-step-group {").expect("step group styles")..];
        let mut breakpoints = std::collections::BTreeSet::new();
        for (index, _) in step_css.match_indices("@media (max-width: ") {
            let rest = &step_css[index + "@media (max-width: ".len()..];
            let end = rest.find("px)").expect("breakpoint width");
            breakpoints.insert(&rest[..end]);
        }
        assert_eq!(
            breakpoints,
            std::collections::BTreeSet::from(["600"]),
            "touch sizing must use one mobile breakpoint"
        );
        assert_contains(step_css, "@container (min-width: 40rem)");
        let base = extract_between(step_css, ".k-step {", "@container");
        assert_contains(
            base,
            "grid-template-columns: 1.25rem repeat(3, minmax(0, 1fr))",
        );
        assert_contains(step_css, "min-height: 3em; min-height: 2lh;");
    }
}
