import { test, expect } from '@playwright/test';
import { DEFAULT_TEST_USER, db, getDefaultUserId, getDefaultWorkspaceId, loginUser, walCheckpoint } from '../../helpers/test-helpers';

test.beforeEach(async ({ page }) => {
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
});

test('recipe library searches, shows retained history, archives and restores', async ({ page }) => {
  const title = `Library recipe ${Date.now()}`;
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(title);
  await page.getByLabel('Batch volume (L)').fill('20');
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  await page.goto('/recipes');
  await page.getByLabel('Search recipes').fill(title);
  await expect(page.getByRole('heading', { name: title })).toBeVisible();
  await page.getByRole('button', { name: `History for ${title}` }).click();
  await expect(page.getByRole('heading', { name: `History · ${title}` })).toBeVisible();
  await expect(page.getByText('Revision 1', { exact: false }).last()).toBeVisible();
  await page.getByRole('button', { name: 'Inspect revision 1' }).click();
  await expect(page.getByRole('heading', { name: `Historical revision 1 · ${title}` })).toBeVisible();
  await expect(page.getByText('Read-only saved content. The current recipe has not been changed.')).toBeVisible();
  await expect(page.getByText(/Batch: 20 L/)).toBeVisible();
  await page.getByRole('button', { name: `Archive ${title}` }).click();
  await expect(page.getByRole('heading', { name: title })).toHaveCount(0);
  await page.getByLabel('Show', { exact: true }).selectOption('archived');
  await expect(page.getByRole('heading', { name: title })).toBeVisible();
  await page.getByRole('button', { name: `History for ${title}` }).click();
  await expect(page.getByRole('heading', { name: `History · ${title}` })).toBeVisible();
  await page.getByRole('button', { name: 'Inspect revision 1' }).click();
  await expect(page.getByRole('heading', { name: `Historical revision 1 · ${title}` })).toBeVisible();
  await page.getByRole('button', { name: `Restore ${title}` }).click();
  await expect(page.getByRole('heading', { name: title })).toHaveCount(0);
  await page.getByLabel('Show', { exact: true }).selectOption('active');
  await expect(page.getByRole('heading', { name: title })).toBeVisible();
  await page.getByRole('link', { name: `Open ${title}` }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(title);
});

test('library controls remain usable at narrow width and by keyboard', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/recipes');
  const search = page.getByLabel('Search recipes');
  await search.focus();
  await page.keyboard.type('unlikely recipe name');
  await expect(page.getByText('No recipes found')).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.keyboard.press('Tab');
  await expect(page.getByLabel('Show', { exact: true })).toBeFocused();
});


test('library navigation uses the shared dirty-editor guard', async ({ page }) => {
  const title = `Guarded library recipe ${Date.now()}`;
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(title);
  const recipes = page.getByRole('link', { name: 'Recipes', exact: true }).first();
  await recipes.click();
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Stay' }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(title);
  await recipes.click();
  await guard.getByRole('button', { name: 'Save and leave' }).click();
  await expect(page).toHaveURL(/\/recipes$/);
  await page.getByLabel('Search recipes').fill(title);
  await expect(page.getByRole('heading', { name: title })).toBeVisible();
  await page.getByRole('link', { name: `Open ${title}` }).click();
  await page.getByLabel('Recipe name', { exact: true }).fill(`${title} discarded`);
  await recipes.click();
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Discard and leave' }).click();
  await expect(page).toHaveURL(/\/recipes$/);
  await page.getByLabel('Search recipes').fill(title);
  await page.getByRole('link', { name: `Open ${title}` }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(title);
});

test('delayed historical revision responses cannot replace a new selection or survive context recheck', async ({ page }) => {
  test.setTimeout(60_000);
  const a = `History A ${Date.now()}`;
  const b = `History B ${Date.now()}`;
  for (const title of [a, b]) {
    await page.goto('/recipe-editor/new');
    await page.getByLabel('Recipe name', { exact: true }).fill(title);
    await page.getByRole('button', { name: 'Save recipe' }).click();
    await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  }
  await page.goto('/recipes');
  await expect(page.getByRole('heading', { name: a })).toBeVisible();
  await expect(page.getByRole('heading', { name: b })).toBeVisible();
  let started!: () => void;
  let release!: () => void;
  let startedPromise = new Promise<void>(resolve => { started = resolve; });
  let releasePromise = new Promise<void>(resolve => { release = resolve; });
  let completed!: () => void;
  let completedPromise = new Promise<void>(resolve => { completed = resolve; });
  let delayNext = true;
  await page.route('**/leptos-api/execute_library_command*', async route => {
    const payload = new URLSearchParams(route.request().postData() ?? '').get('command_json');
    const command = payload ? JSON.parse(payload) : null;
    if (delayNext && command?.Revision) {
      delayNext = false;
      const response = await route.fetch();
      started();
      await releasePromise;
      await route.fulfill({ response });
      completed();
    } else {
      await route.continue();
    }
  });
  await page.getByRole('button', { name: `History for ${a}` }).click();
  await page.getByRole('button', { name: 'Inspect revision 1' }).click();
  await startedPromise;
  await page.getByRole('button', { name: `History for ${b}` }).click();
  await page.getByRole('button', { name: 'Inspect revision 1' }).click();
  await expect(page.getByRole('heading', { name: `Historical revision 1 · ${b}` })).toBeVisible();
  release();
  await completedPromise;
  await page.waitForTimeout(300);
  await expect(page.getByRole('heading', { name: `Historical revision 1 · ${a}` })).toHaveCount(0);
  await expect(page.getByRole('heading', { name: `Historical revision 1 · ${b}` })).toBeVisible();

  startedPromise = new Promise<void>(resolve => { started = resolve; });
  releasePromise = new Promise<void>(resolve => { release = resolve; });
  completedPromise = new Promise<void>(resolve => { completed = resolve; });
  delayNext = true;
  await page.getByRole('button', { name: 'Inspect revision 1' }).click();
  await startedPromise;
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByRole('heading', { name: `History · ${b}` })).toHaveCount(0);
  release();
  await completedPromise;
  await page.waitForTimeout(300);
  await expect(page.getByRole('heading', { name: `Historical revision 1 · ${b}` })).toHaveCount(0);
});


test('delayed legacy source responses respect selection and context recheck', async ({ page }) => {
  test.setTimeout(60_000);
  const workspace = getDefaultWorkspaceId();
  const user = getDefaultUserId();
  const suffix = Date.now();
  const a = `Legacy A ${suffix}`;
  const b = `Legacy B ${suffix}`;
  const sourceA = `source-A-${suffix}`;
  const sourceB = `source-B-${suffix}`;
  for (const [index, title, source] of [[1, a, sourceA], [2, b, sourceB]] as const) {
    db.exec(`INSERT INTO recipes (workspace_id,recipe_id,name,document,created_by,native_migration_error) VALUES ('${workspace}','legacy-${suffix}-${index}','${title}','${source}','${user}','unmapped legacy')`);
  }
  walCheckpoint();
  await page.goto('/recipes');
  await expect(page.getByRole('heading', { name: a })).toBeVisible();
  await expect(page.getByRole('heading', { name: b })).toBeVisible();
  let started!: () => void;
  let release!: () => void;
  let completed!: () => void;
  let startedPromise = new Promise<void>(resolve => { started = resolve; });
  let releasePromise = new Promise<void>(resolve => { release = resolve; });
  let completedPromise = new Promise<void>(resolve => { completed = resolve; });
  let delayNext = true;
  await page.route('**/leptos-api/execute_library_command*', async route => {
    const payload = new URLSearchParams(route.request().postData() ?? '').get('command_json');
    const command = payload ? JSON.parse(payload) : null;
    if (delayNext && command?.Recover) {
      delayNext = false;
      const response = await route.fetch();
      started();
      await releasePromise;
      await route.fulfill({ response });
      completed();
    } else {
      await route.continue();
    }
  });
  await page.getByRole('button', { name: `Recover stored source for ${a}` }).click();
  await startedPromise;
  await page.getByRole('button', { name: `Recover stored source for ${b}` }).click();
  await expect(page.getByText(sourceB, { exact: true })).toBeVisible();
  release();
  await completedPromise;
  await page.waitForTimeout(300);
  await expect(page.getByText(sourceA, { exact: true })).toHaveCount(0);
  await expect(page.getByText(sourceB, { exact: true })).toBeVisible();

  startedPromise = new Promise<void>(resolve => { started = resolve; });
  releasePromise = new Promise<void>(resolve => { release = resolve; });
  completedPromise = new Promise<void>(resolve => { completed = resolve; });
  delayNext = true;
  await page.getByRole('button', { name: `Recover stored source for ${b}` }).click();
  await startedPromise;
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  release();
  await completedPromise;
  await page.waitForTimeout(300);
  await expect(page.getByRole('heading', { name: 'Stored recipe source' })).toHaveCount(0);
});

test('denied archive hides displayed history and rejects a late revision response', async ({ page }) => {
  test.setTimeout(60_000);
  const title = `Denied history ${Date.now()}`;
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(title);
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  await page.goto('/recipes');
  await page.getByLabel('Search recipes').fill(title);
  await page.getByRole('button', { name: `History for ${title}` }).click();
  await page.getByRole('button', { name: 'Inspect revision 1' }).click();
  await expect(page.getByRole('heading', { name: `Historical revision 1 · ${title}` })).toBeVisible();
  let started!: () => void;
  let release!: () => void;
  let completed!: () => void;
  const startedPromise = new Promise<void>(resolve => { started = resolve; });
  const releasePromise = new Promise<void>(resolve => { release = resolve; });
  const completedPromise = new Promise<void>(resolve => { completed = resolve; });
  let delayNextRevision = false;
  await page.route('**/leptos-api/execute_library_command*', async route => {
    const payload = new URLSearchParams(route.request().postData() ?? '').get('command_json');
    const command = payload ? JSON.parse(payload) : null;
    if (command?.Revision && delayNextRevision) {
      delayNextRevision = false;
      const response = await route.fetch();
      started();
      await releasePromise;
      await route.fulfill({ response });
      completed();
    } else if (command?.Archive) {
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify('Forbidden') });
    } else {
      await route.continue();
    }
  });
  // A typed denial must immediately remove content that is already visible.
  await page.getByRole('button', { name: `Archive ${title}` }).click();
  await expect(page.getByText('Workspace changed', { exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: `Historical revision 1 · ${title}` })).toHaveCount(0);

  // Reopen in the same account and prove a response already in flight cannot
  // restore retained content after a later denial.
  await page.reload();
  await page.getByLabel('Search recipes').fill(title);
  await page.getByRole('button', { name: `History for ${title}` }).click();
  await page.getByRole('button', { name: 'Inspect revision 1' }).click();
  await expect(page.getByRole('heading', { name: `Historical revision 1 · ${title}` })).toBeVisible();
  delayNextRevision = true;
  await page.getByRole('button', { name: 'Inspect revision 1' }).click();
  await startedPromise;
  await page.getByRole('button', { name: `Archive ${title}` }).click();
  await expect(page.getByText('Workspace changed', { exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: `Historical revision 1 · ${title}` })).toHaveCount(0);
  release();
  await completedPromise;
  await page.waitForTimeout(300);
  await expect(page.getByRole('heading', { name: `Historical revision 1 · ${title}` })).toHaveCount(0);
  await expect(page.getByText('Workspace changed', { exact: true })).toBeVisible();
});

test('a stale denied revision response still redacts the current selection', async ({ page }) => {
  test.setTimeout(60_000);
  const a = `Denied stale A ${Date.now()}`;
  const b = `Denied stale B ${Date.now()}`;
  for (const title of [a, b]) {
    await page.goto('/recipe-editor/new');
    await page.getByLabel('Recipe name', { exact: true }).fill(title);
    await page.getByRole('button', { name: 'Save recipe' }).click();
    await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  }
  await page.goto('/recipes');
  await expect(page.getByRole('heading', { name: a })).toBeVisible();
  await expect(page.getByRole('heading', { name: b })).toBeVisible();
  let started!: () => void;
  let release!: () => void;
  const startedPromise = new Promise<void>(resolve => { started = resolve; });
  const releasePromise = new Promise<void>(resolve => { release = resolve; });
  let first = true;
  await page.route('**/leptos-api/execute_library_command*', async route => {
    const payload = new URLSearchParams(route.request().postData() ?? '').get('command_json');
    const command = payload ? JSON.parse(payload) : null;
    if (first && command?.Revision) {
      first = false;
      started();
      await releasePromise;
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify('Forbidden') });
    } else {
      await route.continue();
    }
  });
  await page.getByRole('button', { name: `History for ${a}` }).click();
  await page.getByRole('button', { name: 'Inspect revision 1' }).click();
  await startedPromise;
  await page.getByRole('button', { name: `History for ${b}` }).click();
  await page.getByRole('button', { name: 'Inspect revision 1' }).click();
  await expect(page.getByRole('heading', { name: `Historical revision 1 · ${b}` })).toBeVisible();
  release();
  await expect(page.getByText('Workspace changed', { exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: `Historical revision 1 · ${b}` })).toHaveCount(0);
});
