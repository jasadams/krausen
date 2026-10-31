import { test, expect, type Page } from '@playwright/test';
import { DEFAULT_TEST_USER, db, loginUser, sqlText } from '../../helpers/test-helpers';

async function waitForCatalogueSearch(page: Page) {
  await page.waitForResponse(response => {
    if (!response.url().includes('execute_ingredient_command')) return false;
    const payload = new URLSearchParams(response.request().postData() ?? '').get('command_json');
    if (!payload) return false;
    try {
      return JSON.parse(payload)?.Search !== undefined;
    } catch {
      return false;
    }
  });
}

test.beforeEach(async ({ page }) => {
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
});

test('the starter catalogue search copies a resolved specification into the recipe', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  const title = `Catalogue recipe ${Date.now()}`;
  await page.getByLabel('Recipe name', { exact: true }).fill(title);
  await page.getByLabel('Batch volume (L)').fill('20');

  await page.getByRole('button', { name: 'Add fermentable' }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill' });
  await expect(picker).toBeVisible();
  await expect(picker.getByRole('button', { name: /Weyermann Vienna/ })).toHaveCount(1);
  await expect(
    picker.locator('.k-catalogue-item', { hasText: 'Weyermann Vienna' }).getByText('Starter catalogue'),
  ).toBeVisible();
  await expect(
    picker.getByText('Specifications are copied into your recipe, so later library changes cannot change what you brewed.'),
  ).toBeVisible();

  const search = page.getByLabel('Search fermentables');
  await search.fill('signature');
  await expect(picker.getByRole('button', { name: /Joe White Signature Lager/ })).toHaveCount(1);
  await expect(picker.getByRole('button', { name: /Weyermann Vienna/ })).toHaveCount(0);
  await search.fill('no such malt anywhere');
  await expect(picker.getByText('Make this library your own')).toBeVisible();

  await search.fill('vienna');
  await expect(picker.getByRole('button', { name: /Weyermann Vienna/ })).toHaveCount(1);
  await picker.getByRole('button', { name: /Weyermann Vienna/ }).click();
  await expect(picker.locator('.k-catalogue-selection strong')).toHaveText('Weyermann Vienna');
  await page.getByLabel('Amount in recipe (kg)').fill('4.5');
  await picker.getByRole('button', { name: 'Add ingredient' }).click();
  await expect(picker).toBeHidden();

  await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Weyermann Vienna');
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  await page.reload();
  await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Weyermann Vienna');
  await expect(page.getByLabel('Amount (kg)', { exact: true })).toHaveValue('4.5');
});

test('custom fermentables save to the workspace library with duplicate feedback', async ({ page }) => {
  const unique = Date.now();
  const entryName = `E2E local malt ${unique}`;
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(`Library recipe ${unique}`);
  await page.getByLabel('Batch volume (L)').fill('20');

  await page.getByRole('button', { name: 'Add fermentable' }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill' });
  const create = page.getByRole('dialog', { name: 'A fermentable of your own' });
  await expect(picker).toBeVisible();

  await page.getByLabel('Search fermentables').fill(`nothing-matches-${unique}`);
  await expect(picker.getByText('Make this library your own')).toBeVisible();
  await picker.getByRole('button', { name: 'Create fermentable', exact: true }).first().click();

  await expect(create).toBeVisible();
  await expect(
    create.getByText("Stored in your workspace's ingredient library. It will not modify the starter catalogue."),
  ).toBeVisible();
  await page.getByLabel('Fermentable name').fill(entryName);
  await page.getByLabel('Producer').fill('E2E maltster');
  await page.getByLabel('Type', { exact: true }).selectOption('Specialty malt');
  await page.getByLabel('Colour (EBC)').fill('4.5');
  await page.getByLabel('Potential yield (%)').fill('80');
  await page.getByRole('button', { name: 'Save to library' }).click();

  await expect(picker).toBeVisible();
  await expect(picker.getByRole('button', { name: entryName })).toHaveCount(1);
  await expect(
    picker.locator('.k-catalogue-item', { hasText: entryName }).getByText('Your library'),
  ).toBeVisible();
  await expect(
    picker.locator('.k-catalogue-item', { hasText: entryName }).getByText(/4\.5 EBC · 80% yield/),
  ).toBeVisible();
  await picker.getByRole('button', { name: entryName }).click();
  await expect(picker.locator('.k-catalogue-selection strong')).toHaveText(entryName);
  await expect(picker.getByRole('button', { name: 'Add ingredient' })).toBeEnabled();
  await picker.getByRole('button', { name: 'Add ingredient' }).click();
  await expect(picker).toBeHidden();
  await expect(page.getByLabel('Name', { exact: true })).toHaveValue(entryName);

  // Duplicate name and producer are rejected with feedback, case-insensitively.
  await page.getByRole('button', { name: 'Add fermentable' }).click();
  await picker.getByRole('button', { name: 'Create fermentable', exact: true }).first().click();
  await expect(create).toBeVisible();
  await create.getByLabel('Fermentable name').fill(entryName.toLowerCase());
  await create.getByLabel('Producer').fill('E2E MALTSTER');
  await create.getByLabel('Colour (EBC)').fill('5');
  await create.getByLabel('Potential yield (%)').fill('79');
  await page.getByRole('button', { name: 'Save to library' }).click();
  await expect(create.getByText('That fermentable and producer are already in your library.')).toBeVisible();

  await page.getByRole('button', { name: 'Back to catalogue' }).click();
  await expect(picker).toBeVisible();
  const libraryFilter = picker.getByRole('button', { name: 'Your library', exact: true });
  await libraryFilter.click();
  await expect(libraryFilter).toHaveAttribute('aria-pressed', 'true');
  await expect(picker.getByRole('button', { name: entryName })).toHaveCount(1);
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(picker).toBeHidden();

  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  await page.reload();
  await expect(page.getByLabel('Name', { exact: true })).toHaveValue(entryName);
});

test('library entries support edit, archive, restore and lost-response retry', async ({ page }) => {
  const unique = Date.now();
  const entryName = `E2E managed malt ${unique}`;
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(`Managed recipe ${unique}`);

  await page.getByRole('button', { name: 'Add fermentable' }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill' });
  const create = page.getByRole('dialog', { name: 'A fermentable of your own' });
  const edit = page.getByRole('dialog', { name: 'Edit your library entry' });
  await expect(picker).toBeVisible();

  // Create with a real lost response: the command commits server-side but the
  // client never receives the result. The form is locked and the retry must
  // replay the stored command, not a rebuild from editable signals.
  const createRequests: any[] = [];
  let createAttempt = 0;
  await page.route('**/leptos-api/execute_ingredient_command*', async route => {
    const payload = new URLSearchParams(route.request().postData() ?? '').get('command_json');
    const command = payload ? JSON.parse(payload) : null;
    if (command?.Save) createRequests.push(command.Save);
    if (createAttempt === 0 && command?.Save) {
      createAttempt += 1;
      await route.fetch();
      await route.fulfill({ status: 503, body: 'response lost' });
      return;
    }
    await route.continue();
  });
  await picker.getByRole('button', { name: 'Create fermentable', exact: true }).first().click();
  await expect(create).toBeVisible();
  await page.getByLabel('Fermentable name').fill(entryName);
  await page.getByLabel('Producer').fill('E2E managed maltster');
  await page.getByLabel('Colour (EBC)').fill('4.5');
  await page.getByLabel('Potential yield (%)').fill('80');
  await create.getByRole('button', { name: 'Save to library' }).click();
  await expect(create.getByText('Save may not have completed')).toBeVisible();
  await expect(create.getByLabel('Fermentable name')).toBeDisabled();
  await expect(create.getByLabel('Colour (EBC)')).toBeDisabled();
  await expect(create.getByLabel('Potential yield (%)')).toBeDisabled();
  await expect(create.getByRole('button', { name: 'Retry previous request' })).toBeEnabled();
  const retrySearch = waitForCatalogueSearch(page);
  await create.getByRole('button', { name: 'Retry previous request' }).click();
  await retrySearch;
  await expect(picker).toBeVisible();
  await page.unroute('**/leptos-api/execute_ingredient_command*');
  await expect(picker.getByRole('button', { name: entryName })).toHaveCount(1);
  expect(createRequests).toHaveLength(2);
  expect(createRequests[1].request_id).toBe(createRequests[0].request_id);
  expect(createRequests[1].document).toEqual(createRequests[0].document);

  // Edit with a lost response: retry must recover the original edit command.
  await picker.getByRole('button', { name: entryName }).click();
  await picker.getByRole('button', { name: 'Edit library entry' }).click();
  await expect(edit).toBeVisible();
  await edit.getByLabel('Colour (EBC)').fill('9.5');
  const editRequests: any[] = [];
  let editAttempt = 0;
  await page.route('**/leptos-api/execute_ingredient_command*', async route => {
    const payload = new URLSearchParams(route.request().postData() ?? '').get('command_json');
    const command = payload ? JSON.parse(payload) : null;
    if (command?.Save) editRequests.push(command.Save);
    if (editAttempt === 0 && command?.Save) {
      editAttempt += 1;
      await route.fetch();
      await route.fulfill({ status: 503, body: 'response lost' });
      return;
    }
    await route.continue();
  });
  await edit.getByRole('button', { name: 'Save changes' }).click();
  await expect(edit.getByText('Save may not have completed')).toBeVisible();
  await expect(edit.getByLabel('Colour (EBC)')).toBeDisabled();
  await expect(edit.getByRole('button', { name: 'Retry previous request' })).toBeEnabled();
  const editSearch = waitForCatalogueSearch(page);
  await edit.getByRole('button', { name: 'Retry previous request' }).click();
  await editSearch;
  await page.unroute('**/leptos-api/execute_ingredient_command*');
  await expect(picker).toBeVisible();
  await expect(
    picker.locator('.k-catalogue-item', { hasText: entryName }).getByText(/9\.5 EBC/),
  ).toBeVisible();
  expect(editRequests).toHaveLength(2);
  expect(editRequests[1].request_id).toBe(editRequests[0].request_id);
  expect(editRequests[1].document).toEqual(editRequests[0].document);

  // Archive with a lost response: the retry must stay enabled and recover the
  // same command without appending a second event.
  await picker.getByRole('button', { name: entryName }).click();
  const archiveRequests: any[] = [];
  let archiveAttempt = 0;
  await page.route('**/leptos-api/execute_ingredient_command*', async route => {
    const payload = new URLSearchParams(route.request().postData() ?? '').get('command_json');
    const command = payload ? JSON.parse(payload) : null;
    if (command?.Archive) archiveRequests.push(command.Archive);
    if (archiveAttempt === 0 && command?.Archive && command.Archive.archived === true) {
      archiveAttempt += 1;
      await route.fetch();
      await route.fulfill({ status: 503, body: 'response lost' });
      return;
    }
    await route.continue();
  });
  await picker.getByRole('button', { name: 'Archive', exact: true }).click();
  await expect(picker.getByText('Change may not have completed')).toBeVisible();
  const archiveRetry = picker.getByRole('button', { name: 'Retry previous request' });
  await expect(archiveRetry).toBeEnabled();
  const archiveSearch = waitForCatalogueSearch(page);
  await archiveRetry.click();
  await archiveSearch;
  await page.unroute('**/leptos-api/execute_ingredient_command*');
  await expect(picker.getByRole('button', { name: entryName })).toHaveCount(1);
  await expect(
    picker.locator('.k-catalogue-item', { hasText: entryName }).getByText('Archived'),
  ).toBeVisible();
  await expect(picker.getByRole('button', { name: 'Add ingredient' })).toBeDisabled();
  await expect(picker.getByRole('button', { name: 'Edit library entry' })).toBeDisabled();
  expect(archiveRequests).toHaveLength(2);
  expect(archiveRequests[1].request_id).toBe(archiveRequests[0].request_id);
  expect(archiveRequests[1]).toEqual(archiveRequests[0]);
  expect(
    db.exec(`SELECT COUNT(*) FROM ingredient_archive_events e JOIN ingredient_entries i ON i.workspace_id = e.workspace_id AND i.entry_id = e.entry_id WHERE i.name = ${sqlText(entryName)}`),
  ).toBe('1');

  // Restore with a lost response follows the same retained-command path.
  await picker.getByRole('button', { name: entryName }).click();
  const restoreRequests: any[] = [];
  let restoreAttempt = 0;
  await page.route('**/leptos-api/execute_ingredient_command*', async route => {
    const payload = new URLSearchParams(route.request().postData() ?? '').get('command_json');
    const command = payload ? JSON.parse(payload) : null;
    if (command?.Archive) restoreRequests.push(command.Archive);
    if (restoreAttempt === 0 && command?.Archive && command.Archive.archived === false) {
      restoreAttempt += 1;
      await route.fetch();
      await route.fulfill({ status: 503, body: 'response lost' });
      return;
    }
    await route.continue();
  });
  await picker.getByRole('button', { name: 'Restore' }).click();
  await expect(picker.getByText('Change may not have completed')).toBeVisible();
  const restoreRetry = picker.getByRole('button', { name: 'Retry previous request' });
  await expect(restoreRetry).toBeEnabled();
  const restoreSearch = waitForCatalogueSearch(page);
  await restoreRetry.click();
  await restoreSearch;
  await page.unroute('**/leptos-api/execute_ingredient_command*');
  await expect(
    picker.locator('.k-catalogue-item', { hasText: entryName }).getByText('Your library'),
  ).toBeVisible();
  expect(restoreRequests).toHaveLength(2);
  expect(restoreRequests[1].request_id).toBe(restoreRequests[0].request_id);
  expect(restoreRequests[1]).toEqual(restoreRequests[0]);
  expect(
    db.exec(`SELECT COUNT(*) FROM ingredient_archive_events e JOIN ingredient_entries i ON i.workspace_id = e.workspace_id AND i.entry_id = e.entry_id WHERE i.name = ${sqlText(entryName)}`),
  ).toBe('2');

  await page.getByRole('button', { name: 'Cancel' }).click();
  await page.getByLabel('Batch volume (L)').fill('20');
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
});

test('an unavailable create keeps the pending command locked and retries identically', async ({ page }) => {
  const unique = Date.now();
  const entryName = `E2E unavailable malt ${unique}`;
  await page.goto('/recipe-editor/new');
  await page.getByRole('button', { name: 'Add fermentable' }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill' });
  const create = page.getByRole('dialog', { name: 'A fermentable of your own' });
  await expect(picker).toBeVisible();
  await picker.getByRole('button', { name: 'Create fermentable', exact: true }).first().click();
  await expect(create).toBeVisible();
  await create.getByLabel('Fermentable name').fill(entryName);
  await create.getByLabel('Colour (EBC)').fill('4.5');
  await create.getByLabel('Potential yield (%)').fill('80');

  const requests: any[] = [];
  let unavailableAttempt = 0;
  await page.route('**/leptos-api/execute_ingredient_command*', async route => {
    const payload = new URLSearchParams(route.request().postData() ?? '').get('command_json');
    const command = payload ? JSON.parse(payload) : null;
    if (command?.Save) requests.push(command.Save);
    if (unavailableAttempt === 0 && command?.Save) {
      unavailableAttempt += 1;
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify('Unavailable'),
      });
      return;
    }
    await route.continue();
  });
  await create.getByRole('button', { name: 'Save to library' }).click();
  await expect(create.getByText('Save may not have completed')).toBeVisible();
  await expect(create.getByLabel('Fermentable name')).toBeDisabled();
  await expect(create.getByLabel('Colour (EBC)')).toBeDisabled();
  await expect(create.getByRole('button', { name: 'Retry previous request' })).toBeEnabled();
  const retrySearch = waitForCatalogueSearch(page);
  await create.getByRole('button', { name: 'Retry previous request' }).click();
  await retrySearch;
  await page.unroute('**/leptos-api/execute_ingredient_command*');
  await expect(picker.getByRole('button', { name: entryName })).toHaveCount(1);
  expect(requests).toHaveLength(2);
  expect(requests[1].request_id).toBe(requests[0].request_id);
  expect(requests[1].document).toEqual(requests[0].document);
});

test('a stale catalogue search denial still redacts the mounted editor', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto('/recipe-editor/new');
  let release!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  let held = false;
  let started!: () => void;
  const requestStarted = new Promise<void>(resolve => { started = resolve; });
  await page.route('**/leptos-api/execute_ingredient_command*', async route => {
    const payload = new URLSearchParams(route.request().postData() ?? '').get('command_json');
    const command = payload ? JSON.parse(payload) : null;
    if (command?.Search && !held) {
      held = true;
      started();
      await gate;
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify('Forbidden'),
      });
      return;
    }
    await route.continue();
  });
  await page.getByRole('button', { name: 'Add fermentable' }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill' });
  await expect(picker).toBeVisible();
  await requestStarted;
  const libraryFilter = picker.getByRole('button', { name: 'Your library', exact: true });
  await libraryFilter.click();
  await expect(libraryFilter).toHaveAttribute('aria-pressed', 'true');
  release();
  await expect(page.getByText('Recipe held in its original workspace')).toBeVisible();
  expect(errors).toEqual([]);
});

test('a delayed catalogue search denial cannot touch a disposed picker', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto('/recipe-editor/new');
  let started!: () => void;
  const requestStarted = new Promise<void>(resolve => { started = resolve; });
  await page.route('**/leptos-api/execute_ingredient_command*', async route => {
    const payload = new URLSearchParams(route.request().postData() ?? '').get('command_json');
    const command = payload ? JSON.parse(payload) : null;
    if (command?.Search) {
      started();
      try {
        await new Promise(resolve => setTimeout(resolve, 1500));
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify('Forbidden'),
        });
      } catch {
        // Navigation may cancel the in-flight request; the app still must not
        // touch disposed signals when its future resolves.
      }
      return;
    }
    await route.continue();
  });
  await page.getByRole('button', { name: 'Add fermentable' }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill' });
  await expect(picker).toBeVisible();
  await requestStarted;
  await page.goto('/recipes');
  await expect(page).toHaveURL(/\/recipes$/);
  await page.waitForTimeout(2000);
  await expect(page.getByRole('dialog', { name: 'Build your grain bill' })).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('a delayed catalogue save response cannot copy data into a disposed editor', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  const unique = Date.now();
  const entryName = `E2E disposed malt ${unique}`;
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(`Disposed recipe ${unique}`);
  await page.getByRole('button', { name: 'Add fermentable' }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill' });
  const create = page.getByRole('dialog', { name: 'A fermentable of your own' });
  await expect(picker).toBeVisible();
  await picker.getByRole('button', { name: 'Create fermentable', exact: true }).first().click();
  await expect(create).toBeVisible();
  await create.getByLabel('Fermentable name').fill(entryName);
  await create.getByLabel('Colour (EBC)').fill('4.5');
  await create.getByLabel('Potential yield (%)').fill('80');
  let started!: () => void;
  const requestStarted = new Promise<void>(resolve => { started = resolve; });
  await page.route('**/leptos-api/execute_ingredient_command*', async route => {
    const payload = new URLSearchParams(route.request().postData() ?? '').get('command_json');
    const command = payload ? JSON.parse(payload) : null;
    if (command?.Save) {
      started();
      try {
        const response = await route.fetch();
        await new Promise(resolve => setTimeout(resolve, 1500));
        await route.fulfill({ response });
      } catch {
        // Navigation may cancel the in-flight request; see the search denial.
      }
      return;
    }
    await route.continue();
  });
  await create.getByRole('button', { name: 'Save to library' }).click();
  await requestStarted;
  await page.goto('/recipes');
  await expect(page).toHaveURL(/\/recipes$/);
  await page.waitForTimeout(2000);
  await expect(page.getByRole('dialog', { name: 'Build your grain bill' })).toHaveCount(0);
  // The mutation response must not be applied to a disposed editor: the
  // library entry exists, but no recipe was created or copied.
  expect(
    db.exec(`SELECT COUNT(*) FROM ingredient_entries WHERE name = ${sqlText(entryName)}`),
  ).toBe('1');
  expect(
    db.exec(`SELECT COUNT(*) FROM recipes WHERE name = ${sqlText(`Disposed recipe ${unique}`)}`),
  ).toBe('0');
  expect(errors).toEqual([]);
});
