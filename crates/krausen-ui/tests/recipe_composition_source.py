from pathlib import Path
import unittest


class RecipeComposition(unittest.TestCase):
    def test_ingredients_precede_secondary_utilities(self):
        source = (Path(__file__).parents[1] / "src/pages/recipe_editor.rs").read_text()
        editor = source[source.index('<div class="recipe-editor__layout"'):]
        self.assertLess(editor.index('<IngredientEditor'), editor.index('<crate::pages::beerxml::BeerXmlExport'))
        self.assertIn('<BreweryTabs id="editor" label="Recipe sections"', source)
        self.assertIn('role="tabpanel"', source)
        for name in ("recipe", "history", "brews"):
            self.assertIn(f'id="editor-panel-{name}"', source)
            self.assertIn(f'aria-labelledby="editor-tab-{name}"', source)
        self.assertIn('<crate::components::RecipeRevisionHistory', source)
        self.assertNotIn('fn EditorHistory(', source)
        self.assertIn('BrewHistoryView embedded=true recipe_id=id intent=identity.get_untracked() on_authority_failure=history_authority_failure', source)
        self.assertIn('title="Water plan"', source)
        self.assertIn('title="Your equipment"', source)

    def test_shared_history_has_pagination_inspection_and_context_fences(self):
        source = (Path(__file__).parents[1] / "src/components/recipe_revision_history.rs").read_text()
        for required in ('LibraryCommand::History', 'LibraryCommand::Revision',
                         'next_cursor', 'Load more revisions', 'Complete saved data',
                         'current_context_matches', 'refresh_key.try_get_untracked()',
                         'on_authority_failure.try_run(())'):
            self.assertIn(required, source)

    def test_formulation_uses_approved_decorative_portrait(self):
        source = (Path(__file__).parents[1] / "src/pages/recipe_editor.rs").read_text()
        preview = source[source.index('fn FormulationPreview('):source.index('fn preview_metric(')]
        kit = (Path(__file__).parents[1] / "src/components/brewery_kit.rs").read_text()
        showcase = (Path(__file__).parents[1] / "src/pages/design_system.rs").read_text()
        self.assertIn('<crate::components::BreweryBeerPortrait/>', preview)
        self.assertIn('<crate::components::BreweryBeerPortrait/>', showcase)
        self.assertIn('class="beer-portrait"', kit)
        self.assertIn('class="beer-glass__liquid"', kit)
        self.assertIn('<PreviewMetrics preview=preview/>', preview)

    def test_ingredient_editor_uses_shared_compact_rows(self):
        root = Path(__file__).parents[1] / "src"
        editor = (root / "pages/recipe_editor.rs").read_text()
        kit = (root / "components/brewery_kit.rs").read_text()
        showcase = (root / "pages/design_system.rs").read_text()
        self.assertIn('<BreweryCompactIngredientRow', editor)
        self.assertIn('<BreweryCompactIngredientRow', showcase)
        self.assertIn('fn BreweryCompactIngredientRow(', kit)
        self.assertIn('Addition details for {}', kit)
        self.assertIn('Remove {}', kit)
        self.assertIn('k-disclosure--compact', kit)

    def test_editor_topbar_and_water_summary_are_primary(self):
        source = (Path(__file__).parents[1] / "src/pages/recipe_editor.rs").read_text()
        self.assertIn('class="recipe-editor__topbar"', source)
        self.assertIn('aria-label="Recipe breadcrumb"', source)
        self.assertNotIn('<BreweryDisclosure title="Water plan details">', source)
        preview = source[source.index('fn FormulationPreview('):source.index('fn PreviewMetrics(')]
        self.assertLess(preview.index('recipe-editor__preview-summary'), preview.index('{children()}'))
        self.assertLess(preview.index('{children()}'), preview.index('recipe-editor__preview-details'))

    def test_mobile_title_and_stored_descriptor_formatting(self):
        root = Path(__file__).parents[1]
        editor = (root / "src/pages/recipe_editor.rs").read_text()
        css = (root / "style/kit.css").read_text()
        self.assertIn('on_change=title_change mobile_title=true', editor)
        self.assertIn('format_measurement_value(value, "ebc")', editor)
        self.assertIn('format_measurement_value(value, "min")', editor)
        self.assertIn('.k-field--mobile-title', css)


if __name__ == "__main__":
    unittest.main()
