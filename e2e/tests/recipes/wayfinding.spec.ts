import { test, expect, type Page } from '@playwright/test';
import { DEFAULT_TEST_USER, loginUser } from '../../helpers/test-helpers';

// KRA-48 acceptance: IA-006/007/010/011. Addressable saved versions, library
// return state, location/arrival semantics and neutral missing-parent recovery.

test.beforeEach(async ({ page }) => {
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
});

async function saveRecipe(page: Page, title: string, revisions = 1): Promise<string> {
  await page.goto('/recipe-editor/new');
  const name = page.getByLabel('Recipe name', { exact: true });
  await name.fill(title);
  await page.getByLabel('Batch volume (L)').fill('20');
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  for (let revision = 2; revision <= revisions; revision += 1) {
    await page.getByLabel('Batch volume (L)').fill(String(20 + revision));
    await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
    await expect(page.getByRole('status').filter({ hasText: `Saved revision ${revision}` })).toBeVisible();
  }
  await page.goto('/recipes');
  await page.getByLabel('Search recipes').fill(title);
  const link = page.getByRole('link', { name: `Open ${title}` });
  await expect(link).toBeVisible();
  const href = await link.getAttribute('href');
  expect(href).toMatch(/^\/recipe-editor\/[\w-]+$/);
  return href!.replace('/recipe-editor/', '');
}

test('a saved version is addressable, survives reload and Back, and never replaces a dirty draft', async ({ page }) => {
  const title = `Versioned ${Date.now()}`;
  const id = await saveRecipe(page, title, 2);

  await page.goto(`/recipe-editor/${id}?section=versions&revision=1`);
  await expect(page.getByRole('tab', { name: 'Saved versions', exact: true })).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByRole('heading', { name: `Historical revision 1 · ${title}` })).toBeVisible();
  await expect(page.getByText(/Batch: 20 L/)).toBeVisible();
  // The editable current recipe is a different object than the inspected version.
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue('22');

  await page.reload();
  await expect(page.getByRole('heading', { name: `Historical revision 1 · ${title}` })).toBeVisible();
  await expect(page).toHaveURL(new RegExp(`/recipe-editor/${id}\\?section=versions&revision=1$`));

  // Leaving and returning with Back restores the exact selection.
  await page.getByRole('navigation', { name: 'Your brewery' }).getByRole('link', { name: 'Recipes' }).click();
  await expect(page).toHaveURL(/\/recipes/);
  await page.goBack();
  await expect(page).toHaveURL(new RegExp(`revision=1`));
  await expect(page.getByRole('heading', { name: `Historical revision 1 · ${title}` })).toBeVisible();

  // Selecting another version with a dirty draft keeps the draft and prompts nothing.
  await page.getByLabel('Batch volume (L)').fill('31');
  await page.getByRole('button', { name: 'Inspect revision 2' }).click();
  await expect(page).toHaveURL(new RegExp(`revision=2$`));
  await expect(page.getByRole('heading', { name: `Historical revision 2 · ${title}` })).toBeVisible();
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue('31');
  await expect(page.getByRole('dialog')).toHaveCount(0);

  // Section and selection are additive; the bare path stays the editable recipe.
  await page.getByRole('tab', { name: 'Recipe', exact: true }).click();
  await expect(page).toHaveURL(new RegExp(`/recipe-editor/${id}$`));
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue('31');
});

test('an unknown saved version is a neutral recovery that keeps the editable recipe', async ({ page }) => {
  const title = `Missing version ${Date.now()}`;
  const id = await saveRecipe(page, title);
  await page.goto(`/recipe-editor/${id}?section=versions&revision=99`);
  await expect(page.getByText('Saved version 99 was not found for this recipe.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Close saved version' })).toBeVisible();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(title);
  await page.getByRole('button', { name: 'Close saved version' }).click();
  await expect(page.getByText('Saved version 99 was not found for this recipe.')).toHaveCount(0);
});

test('library search, filter and saved-version selection live in the URL and return on a plain visit', async ({ page }) => {
  const title = `Library URL ${Date.now()}`;
  const id = await saveRecipe(page, title);
  await expect(page).toHaveURL(new RegExp(`/recipes\\?q=${encodeURIComponent(title).replace(/%20/g, '%20')}`));

  await page.getByRole('button', { name: `Saved versions for ${title}` }).click();
  await page.getByRole('button', { name: 'Inspect revision 1' }).click();
  await expect(page).toHaveURL(new RegExp(`versions=${id}&revision=1$`));
  await page.reload();
  await expect(page.getByLabel('Search recipes')).toHaveValue(title);
  await expect(page.getByRole('heading', { name: `Historical revision 1 · ${title}` })).toBeVisible();

  // Following a recipe link and coming back through the ordinary Recipes link
  // resumes the search being left, once.
  await page.goto(`/recipes?q=${encodeURIComponent(title)}`);
  await page.getByRole('link', { name: `Open ${title}` }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(title);
  await page.getByRole('navigation', { name: 'Your brewery' }).getByRole('link', { name: 'Recipes' }).click();
  await expect(page.getByLabel('Search recipes')).toHaveValue(title);
  await expect(page.getByRole('heading', { name: title })).toBeVisible();
  await page.getByRole('navigation', { name: 'Your brewery' }).getByRole('link', { name: 'Equipment' }).click();
  await page.getByRole('navigation', { name: 'Your brewery' }).getByRole('link', { name: 'Recipes' }).click();
  await expect(page.getByLabel('Search recipes')).toHaveValue('');
});

test('the library URL carries the archive filter', async ({ page }) => {
  await page.goto('/recipes?show=archived');
  await expect(page.getByLabel('Show', { exact: true })).toHaveValue('archived');
  await page.getByLabel('Show', { exact: true }).selectOption('active');
  await expect(page).toHaveURL(/\/recipes$/);
});

test('location, landmarks, titles and exact current page', async ({ page }) => {
  const title = `Located ${Date.now()}`;
  const id = await saveRecipe(page, title);
  const primary = page.getByRole('navigation', { name: 'Your brewery' }).first();

  await page.goto('/recipes');
  await expect(page).toHaveTitle('Recipes · Krausen');
  await expect(primary.getByRole('link', { name: 'Recipes', exact: true })).toHaveAttribute('aria-current', 'page');

  // Inside a recipe the section is an ancestor, not the current page.
  await page.goto(`/recipe-editor/${id}`);
  await expect(page).toHaveTitle(`${title} · Krausen`);
  await expect(primary.getByRole('link', { name: 'Recipes', exact: true })).not.toHaveAttribute('aria-current', 'page');
  await expect(primary.getByRole('link', { name: 'Recipes', exact: true })).toHaveAttribute('data-ancestor', 'true');

  await page.goto('/beerxml');
  await expect(page).toHaveTitle('Import recipes · Krausen');
  await expect(primary.getByRole('link', { name: 'Recipes', exact: true })).toHaveAttribute('data-ancestor', 'true');
  await expect(page.getByRole('navigation', { name: 'Breadcrumb' }).getByText('Import recipes')).toHaveAttribute('aria-current', 'page');

  await page.goto('/equipment');
  await expect(primary.getByRole('link', { name: 'Equipment', exact: true })).toHaveAttribute('aria-current', 'page');
  await expect(page.locator('main#main-content')).toHaveCount(1);
});

test('skip link moves focus into main and arrival moves it to the new heading', async ({ page }) => {
  await page.goto('/recipes');
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.keyboard.press('Tab');
  const skip = page.getByRole('link', { name: 'Skip to main content' });
  await expect(skip).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.locator('main#main-content')).toBeFocused();

  // A committed new navigation focuses the arrival heading; a background
  // refetch afterwards does not move it.
  await page.getByRole('navigation', { name: 'Your brewery' }).getByRole('link', { name: 'Equipment' }).click();
  await expect(page.getByRole('heading', { name: 'Equipment', level: 1 })).toBeFocused();
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await page.waitForTimeout(300);
  await expect(page.getByRole('heading', { name: 'Equipment', level: 1 })).toBeFocused();

  // Back restores context instead of taking focus.
  await page.goBack();
  await expect(page).toHaveURL(/\/recipes/);
  await page.waitForTimeout(300);
  await expect(page.getByRole('heading', { level: 1 })).not.toBeFocused();
});

test('a missing brew offers a neutral trail and never discloses a recipe or switches workspace', async ({ page }) => {
  await page.goto('/brews/00000000-0000-4000-8000-000000000000');
  await expect(page.getByText('Brew not found in this workspace.')).toBeVisible();
  const trail = page.getByRole('navigation', { name: 'Breadcrumb' });
  await expect(trail.getByRole('link', { name: 'Recipes' })).toHaveAttribute('href', '/recipes');
  await expect(trail.getByText('Measurements', { exact: true })).toHaveAttribute('aria-current', 'page');
  await expect(trail.getByText('Recipe', { exact: true })).toHaveCount(0);
  await expect(page).toHaveURL(/\/brews\/0{8}-/);
  await expect(page).toHaveTitle('Brew measurements · Krausen');

  await page.goto('/recipes/00000000-0000-4000-8000-000000000000/brews');
  const history = page.getByRole('navigation', { name: 'Breadcrumb' });
  await expect(history.getByRole('link', { name: 'Recipe', exact: true })).toBeVisible();
  await expect(history.getByText('Brews', { exact: true })).toHaveAttribute('aria-current', 'page');
  await expect(page).toHaveTitle('Brews · Krausen');
});

test('returning to a paged, scrolled library restores its pages and scroll position once', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 600 });
  const total = 120;
  const row = (index: number) => ({
    recipe_id: `00000000-0000-4000-8000-${String(index).padStart(12, '0')}`,
    title: `Paged recipe ${String(index).padStart(3, '0')}`,
    style: null, batch_size_l: 20, revision: 1, metadata_revision: 1,
    archived: false, needs_recovery: false, created_at: '2026-01-01T00:00:00Z',
  });
  await page.route('**/leptos-api/execute_library_command*', async route => {
    const payload = new URLSearchParams(route.request().postData() ?? '').get('command_json');
    const command = payload ? JSON.parse(payload) : null;
    if (!command?.List) return route.continue();
    const offset = command.List.cursor ? Number(command.List.cursor) : 0;
    const end = Math.min(offset + 50, total);
    const items = Array.from({ length: end - offset }, (_, i) => row(offset + i));
    await route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({ Recipes: { items, next_cursor: end < total ? String(end) : null } }),
    });
  });
  await page.goto('/recipes');
  await expect(page.getByRole('heading', { name: 'Paged recipe 000' })).toBeVisible();
  await page.getByRole('button', { name: 'Load more recipes' }).click();
  await expect(page.getByRole('heading', { name: 'Paged recipe 050' })).toBeAttached();
  await page.getByRole('button', { name: 'Load more recipes' }).click();
  await expect(page.getByRole('heading', { name: 'Paged recipe 119' })).toBeAttached();
  const link = page.getByRole('link', { name: 'Open Paged recipe 090' });
  await link.scrollIntoViewIfNeeded();
  const before = await page.locator('.app-shell__main').evaluate(el => el.scrollTop);
  expect(before).toBeGreaterThan(500);
  await link.click();
  await expect(page).toHaveURL(/\/recipe-editor\//);
  await page.getByRole('navigation', { name: 'Your brewery' }).getByRole('link', { name: 'Recipes' }).click();
  await expect(page.getByRole('heading', { name: 'Paged recipe 119' })).toBeAttached();
  await expect.poll(() => page.locator('.app-shell__main').evaluate(el => el.scrollTop), { timeout: 10_000 })
    .toBeGreaterThan(before - 80);
  // Consumed by that one return: a later plain visit starts at the top, first page.
  await page.getByRole('navigation', { name: 'Your brewery' }).getByRole('link', { name: 'Equipment' }).click();
  await page.getByRole('navigation', { name: 'Your brewery' }).getByRole('link', { name: 'Recipes' }).click();
  await expect(page.getByRole('heading', { name: 'Paged recipe 000' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Paged recipe 119' })).toHaveCount(0);
});

test('typing quickly in library search keeps every character and the URL agrees', async ({ page }) => {
  await page.goto('/recipes');
  const search = page.getByLabel('Search recipes');
  await search.pressSequentially('quick brown fox', { delay: 5 });
  await expect(search).toHaveValue('quick brown fox');
  await expect(page).toHaveURL(/q=quick%20brown%20fox$/);
});

test('a hash-only Back does not suppress the next arrival focus', async ({ page }) => {
  await page.goto('/recipes');
  await page.locator('.app-shell__desktop').getByRole('button', { name: /^Account menu/ }).click();
  await page.locator('[popover]:popover-open').getByRole('link', { name: 'Appearance', exact: true }).click();
  await expect(page).toHaveURL(/#appearance$/);
  await page.evaluate(() => { history.pushState({}, '', location.pathname + location.search + '#x'); history.back(); });
  await page.waitForTimeout(1200);
  await page.getByRole('navigation', { name: 'Your brewery' }).getByRole('link', { name: 'Equipment' }).click();
  await expect(page.getByRole('heading', { name: 'Equipment', level: 1 })).toBeFocused();
});

test('changing the library filter closes an open saved-versions panel', async ({ page }) => {
  const title = `Filter closes ${Date.now()}`;
  await saveRecipe(page, title);
  await page.getByRole('button', { name: `Saved versions for ${title}` }).click();
  await page.getByRole('button', { name: 'Inspect revision 1' }).click();
  await expect(page.getByRole('heading', { name: `Historical revision 1 · ${title}` })).toBeVisible();
  await page.getByLabel('Search recipes').fill(`${title} x`);
  await expect(page.getByRole('heading', { name: `Saved versions · ${title}` })).toHaveCount(0);
  await expect(page).not.toHaveURL(/versions=/);
});
