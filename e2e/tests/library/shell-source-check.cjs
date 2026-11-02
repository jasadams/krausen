const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { resolve } = require('node:path');
const root = resolve(__dirname, '../../..');
const layout = readFileSync(resolve(root, 'crates/krausen-ui/src/components/layout.rs'), 'utf8');
const library = readFileSync(resolve(root, 'crates/krausen-ui/src/pages/recipes.rs'), 'utf8');

test('shell composes grouped navigation, real showcase and existing theme mechanism', () => {
  assert.ok(layout.includes('aria-label="Your brewery"'));
  assert.ok(layout.includes('aria-label="The details"'));
  assert.ok(layout.includes('href="/design-system"'));
  assert.ok(layout.includes('crate::components::theme::set_theme('));
  assert.ok(!layout.includes('href="/equipment"'));
  assert.ok(!layout.includes('href="/brews"'));
});

test('library uses coherent rows without nested panels and real per-recipe brews', () => {
  assert.ok(library.includes('class="recipe-library__row"'));
  assert.ok(!library.includes('<li><BreweryPanel'));
  assert.ok(library.includes('href=format!("/recipes/{}/brews", item.recipe_id)'));
  for (const capability of ['library-search', 'library-filter', 'Load more recipes', 'Retry previous request', 'Recover stored source']) {
    assert.ok(library.includes(capability), capability);
  }
});

test('revision history delegates to one shared implementation and unmounts on parent invalidation', () => {
  assert.ok(library.includes('<RecipeRevisionHistory'));
  assert.ok(library.includes('on_authority_failure=invalidate_context'));
  assert.ok(!library.includes('LibraryCommand::History('));
  assert.ok(!library.includes('LibraryCommand::Revision('));
  assert.ok(!library.includes('fn HistoricalRecipe('));
});

test('workspace switching and logout still use the same editor guard', () => {
  assert.ok(layout.includes('editor_guard.request(EditorDestination::Workspace(ws_id))'));
  assert.ok(layout.includes('editor_guard.request(EditorDestination::Logout)'));
  assert.ok(layout.includes('open=editor_guard.notice'));
});

test('account modal dismisses on committed route changes and workspace list remounts per opening', () => {
  assert.ok(layout.includes('location.pathname.get();\n        account_dialog.set(false)'));
  assert.ok(layout.includes('<Show when=move || workspace_dialog.get()>'));
  assert.ok(!layout.includes('app-shell__desktop hidden md:block'));
  assert.ok(!layout.includes('app-shell__mobile md:hidden'));
});
