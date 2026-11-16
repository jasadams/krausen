from pathlib import Path
import unittest

# Source-composition guard: the recipe page composes the shared kit in the order
# and vocabulary the approved targets require. Behaviour is covered by the Rust
# unit tests and the Playwright suites; this keeps the composition itself honest.


class RecipeComposition(unittest.TestCase):
    def setUp(self):
        self.root = Path(__file__).parents[1]
        self.editor = (self.root / "src/pages/recipe_editor.rs").read_text()
        self.header = (self.root / "src/pages/recipe_header.rs").read_text()
        self.kit = (self.root / "src/components/brewery_kit.rs").read_text()
        self.showcase = (self.root / "src/pages/design_system.rs").read_text()
        self.css = (self.root / "style/kit.css").read_text()

    def test_ingredients_precede_secondary_utilities(self):
        editor = self.editor[self.editor.index('<div class="recipe-editor__layout"'):]
        self.assertLess(
            editor.index('<crate::pages::ingredient_addition::IngredientAdditions'),
            editor.index('<crate::pages::beerxml::BeerXmlExport'),
        )
        self.assertIn('<BreweryTabs id="editor" label="Recipe sections"', self.editor)
        self.assertIn('role="tabpanel"', self.editor)
        for name in ("recipe", "versions", "brews"):
            self.assertIn(f'id="editor-panel-{name}"', self.editor)
            self.assertIn(f'aria-labelledby="editor-tab-{name}"', self.editor)
        for key, label in (("recipe", "Recipe"), ("versions", "Saved versions"), ("brews", "Brews")):
            self.assertIn(f'("{key}".into(), "{label}".into())', self.editor)
        self.assertIn('<crate::components::RecipeRevisionHistory', self.editor)
        self.assertNotIn('fn EditorHistory(', self.editor)
        self.assertIn(
            'BrewHistoryView embedded=true recipe_id=id intent=identity.get_untracked() on_authority_failure=history_authority_failure',
            self.editor,
        )
        self.assertIn('title="Water plan"', self.editor)
        self.assertIn('title="Your equipment"', self.editor)

    def test_shared_history_has_pagination_inspection_and_context_fences(self):
        source = (self.root / "src/components/recipe_revision_history.rs").read_text()
        for required in (
            "LibraryCommand::History",
            "LibraryCommand::Revision",
            "next_cursor",
            "Load more revisions",
            "Complete saved data",
            "current_context_matches",
            "refresh_key.try_get_untracked()",
            "on_authority_failure.try_run(())",
        ):
            self.assertIn(required, source)

    def test_formulation_uses_approved_decorative_portrait(self):
        preview = self.editor[self.editor.index("fn FormulationPreview("):self.editor.index("fn preview_metric(")]
        self.assertIn("<crate::components::BreweryBeerPortrait/>", preview)
        self.assertIn("<crate::components::BreweryBeerPortrait/>", self.showcase)
        self.assertIn('class="beer-portrait"', self.kit)
        self.assertIn('class="beer-glass__liquid"', self.kit)
        self.assertIn("<PreviewMetrics preview=preview.clone()/>", preview)

    def test_ingredient_composition_uses_one_shared_addition_surface(self):
        # KRA-42 owns the four addition flows; the editor composes that one surface
        # and the kit owns the compact row grammar.
        additions = (self.root / "src/pages/ingredient_addition.rs").read_text()
        self.assertIn("<crate::pages::ingredient_addition::IngredientAdditions", self.editor)
        self.assertIn("<BreweryCompactIngredientRow", additions)
        self.assertIn("<BreweryCompactIngredientRow", self.showcase)
        self.assertIn("fn BreweryCompactIngredientRow(", self.kit)
        self.assertIn("Addition details for {}", self.kit)
        self.assertIn("Remove {}", self.kit)
        self.assertIn("k-disclosure--compact", self.kit)

    def test_header_owns_topbar_breadcrumb_and_water_rail_order(self):
        # The topbar moved into the extracted header; the trail is the shared
        # wayfinding component, never a local copy (IA-007, VIS-002).
        self.assertIn('class="recipe-header"', self.header)
        self.assertIn("wayfinding::Breadcrumb", self.header)
        self.assertIn("wayfinding::Crumb::link(\"Recipes\", \"/recipes\")", self.header)
        self.assertNotIn('aria-label="Recipe breadcrumb"', self.header)
        self.assertNotIn('class="recipe-editor__topbar"', self.editor)
        self.assertNotIn('<BreweryDisclosure title="Water plan details">', self.editor)
        preview = self.editor[self.editor.index("fn FormulationPreview("):self.editor.index("fn PreviewMetrics(")]
        self.assertLess(
            preview.index("recipe-editor__preview-summary"),
            preview.index("{children()}"),
        )
        self.assertLess(
            preview.index("{children()}"),
            preview.index("recipe-editor__preview-details"),
        )

    def test_local_storage_failures_are_typed_as_storage(self):
        # A storage failure must be retired only by a later storage success, so the
        # editor must route these messages through fail_storage, not fail.
        for message in (
            "Unable to retain this draft locally",
            "An older retained draft could not be removed from this browser.",
            "The retained draft could not be removed from this browser.",
            "Could not remove the retained draft.",
            "Could not discard the retained local draft.",
            "Local draft recovery is unavailable in this browser.",
            "but the retained local draft could not be cleared",
        ):
            self.assertNotIn(f'status.fail("{message}', self.editor)
        self.assertIn("status.fail_storage(", self.editor)

    def test_recipe_name_is_the_shared_editable_title_variant(self):
        self.assertIn("title_variant=true", self.header)
        self.assertNotIn("mobile_title", self.editor)
        self.assertIn(".k-field--title", self.css)
        self.assertNotIn(".k-field--mobile-title", self.css)
        # The recipe-used title variant is demonstrated in the real showcase.
        self.assertIn("title_variant=true", self.showcase)

    def test_stored_measurements_use_the_shared_formatting_helper(self):
        presentation = (self.root / "src/pages/brew_history_presentation.rs").read_text()
        self.assertIn("pub(crate) fn format_measurement_value(value: f64, unit: &str)", presentation)
        for unit in ('"sg"', '"l"', '"c"', '"kg"', '"g"', '"min"', '"ebc"'):
            self.assertIn(f"{unit} =>", presentation)


if __name__ == "__main__":
    unittest.main()
