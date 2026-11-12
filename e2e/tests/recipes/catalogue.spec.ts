import { test, expect, type Page, type Route } from '@playwright/test';
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

test('short Navigation keeps naturally tabbed Account wholly visible', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 320, height: 360 });
  await page.emulateMedia({ colorScheme: 'light' });
  await page.goto('/recipe-editor/new');
  await page.getByRole('button', { name: 'Toggle menu', exact: true }).click();
  const navigation = page.getByRole('dialog', { name: 'Navigation', exact: true });
  await expect(navigation).toBeVisible();
  const account = navigation.getByRole('button', { name: 'Account menu', exact: true });
  for (let tab = 0; tab < 12 && !(await account.evaluate(element => element === document.activeElement)); tab++) {
    await page.keyboard.press('Tab');
  }
  await expect(account).toBeFocused();
  await page.screenshot({ path: testInfo.outputPath('account-focused.png') });
  await expect(account).toBeInViewport({ ratio: 1 });
  expect(await account.evaluate(element => {
    const action = element.getBoundingClientRect();
    const body = element.closest('.k-dialog__body')!.getBoundingClientRect();
    return action.top >= Math.max(0, body.top)
      && action.bottom <= Math.min(innerHeight, body.bottom)
      && action.left >= 0 && action.right <= innerWidth;
  })).toBe(true);
});

// These gates delay actual production continuations; no picker state is copied
// into a test reducer. Only the transport outcome and delivery order are varied.
function catalogueGate() {
  let release!: () => void;
  let started!: () => void;
  const ready = new Promise<void>(resolve => { started = resolve; });
  const gate = new Promise<void>(resolve => { release = resolve; });
  return { ready, release: () => release(), hold: async () => { started(); await gate; } };
}

function ingredientCommand(route: Route) {
  const raw = new URLSearchParams(route.request().postData() ?? '').get('command_json');
  return raw ? JSON.parse(raw) : null;
}

async function catalogueCreateForm(page: Page, name: string) {
  const picker = page.getByRole('dialog', { name: 'Build your grain bill' });
  await picker.getByRole('button', { name: 'Create fermentable', exact: true }).first().click();
  const create = page.getByRole('dialog', { name: 'A fermentable of your own' });
  await expect(create).toBeVisible();
  await create.getByLabel('Fermentable name').fill(name);
  await create.getByLabel('Colour (EBC)').fill('4.5');
  await create.getByLabel('Potential yield (%)').fill('80');
  return create;
}

async function catalogueAction(page: Page, kind: 'Search' | 'Save' | 'Archive') {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto('/recipe-editor/new');
  await page.getByRole('button', { name: 'Add fermentable' }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill' });
  await expect(picker).toBeVisible();
  await page.waitForLoadState('networkidle');
  const name = `E2E lifecycle ${kind} ${Date.now()}`;
  if (kind === 'Archive') {
    const create = await catalogueCreateForm(page, name);
    await create.getByRole('button', { name: 'Save to library' }).click();
    await expect(picker).toBeVisible();
    await page.waitForLoadState('networkidle');
    await expect(picker.getByRole('button', { name })).toHaveCount(1);
    await picker.getByRole('button', { name }).click();
  }
  const create = kind === 'Save' ? await catalogueCreateForm(page, name) : null;
  return {
    picker, create, name, errors,
    fire: async () => {
      if (kind === 'Search') await picker.getByLabel('Search fermentables').fill(name);
      else if (kind === 'Save') await create!.getByRole('button', { name: 'Save to library' }).click();
      else await picker.getByRole('button', { name: 'Archive', exact: true }).click();
    },
  };
}

for (const kind of ['Save', 'Archive'] as const) {
  test(`CAT-L1 ${kind} publishes current denial before a failed legacy postflight`, async ({ page }) => {
    const action = await catalogueAction(page, kind);
    const requests: any[] = [];
    let failNextContext = false;
    let failedPostflights = 0;
    await page.route('**/leptos-api/get_user_context*', async route => {
      if (failNextContext) {
        failNextContext = false;
        failedPostflights++;
        await route.fulfill({ status: 503, body: 'transient postflight failure' });
      } else await route.continue();
    });
    await page.route('**/leptos-api/execute_ingredient_command*', async route => {
      const command = ingredientCommand(route);
      if (!command?.[kind]) return route.continue();
      requests.push(command[kind]);
      failNextContext = true;
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify('Forbidden') });
    });
    await action.fire();
    try {
      await expect(page.getByText('Access denied', { exact: true })).toBeVisible();
      await expect(page.getByLabel('Recipe name', { exact: true })).toBeHidden();
      await expect(action.picker).toBeHidden();
      if (action.create) await expect(action.create).toBeHidden();
      expect(requests).toHaveLength(1);
      expect(action.errors).toEqual([]);
    } finally {
      await test.info().attach('command-denial-order', { body: JSON.stringify({ requests, failedPostflights }), contentType: 'application/json' });
    }
  });
}

for (const kind of ['Search', 'Save', 'Archive'] as const) {
  test(`CAT-L2 old ${kind} cannot redact or unlock a reopened successor command`, async ({ page }) => {
    const action = await catalogueAction(page, kind);
    const old = catalogueGate();
    const successor = catalogueGate();
    const requests: any[] = [];
    let capturedOld = false;
    await page.route('**/leptos-api/execute_ingredient_command*', async route => {
      const command = ingredientCommand(route);
      if (command?.[kind] && !capturedOld) {
        capturedOld = true;
        requests.push({ old: command });
        await old.hold();
        return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify('Forbidden') });
      }
      if (command?.Save) {
        const retry = requests.some(request => request.successor);
        requests.push({ successor: command.Save });
        if (retry) return route.continue();
        await successor.hold();
        return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify('Unavailable') });
      }
      await route.continue();
    });
    try {
      await action.fire();
      await old.ready;
      // Search keeps focus in its input; use the actual task Cancel control
      // rather than treating native Escape readiness as ownership evidence.
      if (kind === 'Search') await action.picker.getByRole('button', { name: 'Cancel', exact: true }).click();
      else await page.keyboard.press('Escape');
      if (action.create) await expect(action.create).toBeHidden();
      await expect(action.picker).toBeHidden();
      await page.getByRole('button', { name: 'Add fermentable' }).click();
      await expect(action.picker).toBeVisible();
      const name = `E2E successor ${kind} ${Date.now()}`;
      const create = await catalogueCreateForm(page, name);
      await create.getByRole('button', { name: 'Save to library' }).click();
      await successor.ready;
      await expect(create.getByLabel('Fermentable name')).toBeDisabled();
      await expect(create.getByRole('button', { name: 'Save to library' })).toBeDisabled();
      const oldResponse = page.waitForResponse(response => {
        const raw = new URLSearchParams(response.request().postData() ?? '').get('command_json');
        return response.url().includes('execute_ingredient_command') && !!raw && JSON.stringify(JSON.parse(raw)) === JSON.stringify(requests[0].old);
      });
      old.release();
      await (await oldResponse).finished();
      successor.release();
      // Both held transports have now completed. Drain any real postflights,
      // then verify the successor's observable retained state and full replay.
      await page.waitForLoadState('networkidle');
      await expect(create).toBeVisible();
      await expect(create.getByLabel('Fermentable name')).toHaveValue(name);
      await expect(create.getByLabel('Fermentable name')).toBeDisabled();
      await expect(page.getByText('Access denied', { exact: true })).toHaveCount(0);
      await expect(create.getByRole('button', { name: 'Retry previous request' })).toBeEnabled();
      await expect(create.getByLabel('Fermentable name')).toBeDisabled();
      await create.getByRole('button', { name: 'Retry previous request' }).click();
      await expect.poll(() => requests.length).toBe(3);
      expect(requests[2].successor).toEqual(requests[1].successor);
      await expect(action.picker).toBeVisible();
      await page.waitForLoadState('networkidle');
      expect(db.exec(`SELECT COUNT(*) FROM ingredient_entries WHERE name = ${sqlText(name)}`)).toBe('1');
      expect(action.errors).toEqual([]);
    } finally {
      old.release();
      successor.release();
      await test.info().attach('originating-and-successor-commands', { body: JSON.stringify(requests), contentType: 'application/json' });
    }
  });
}

test('CAT-v2-L1 initial Archive retains its selection and exact request across nonmatching Search', async ({ page }) => {
  const action = await catalogueAction(page, 'Archive');
  const initial = catalogueGate();
  const requests: any[] = [];
  let emptySearch: any = null;
  await page.route('**/leptos-api/execute_ingredient_command*', async route => {
    const command = ingredientCommand(route);
    if (command?.Archive) {
      requests.push(command.Archive);
      if (requests.length === 1) {
        await initial.hold();
        return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify('Unavailable') });
      }
    }
    await route.continue();
  });
  try {
    await action.fire();
    await initial.ready;
    const query = `no-matching-inflight-archive-${Date.now()}`;
    const searched = page.waitForResponse(async response => {
      if (!response.url().includes('/execute_ingredient_command')) return false;
      const raw = new URLSearchParams(response.request().postData() ?? '').get('command_json');
      if (!raw || JSON.parse(raw)?.Search?.query !== query) return false;
      emptySearch = await response.json();
      return true;
    });
    await action.picker.getByLabel('Search fermentables').fill(query);
    await searched;
    expect(emptySearch.Entries.items).toEqual([]);
    // This is actual rendered application of the nonmatching Entries page,
    // not merely delivery of an intercepted response.
    await expect(action.picker.getByText('Make this library your own')).toBeVisible();
    await expect(action.picker.locator('.k-catalogue-selection strong')).toHaveText(action.name);
    initial.release();
    await page.waitForLoadState('networkidle');
    const retry = action.picker.getByRole('button', { name: 'Retry previous request' });
    await expect(retry).toBeEnabled();
    await expect(action.picker.getByRole('button', { name: 'Edit library entry' })).toBeDisabled();
    await expect(action.picker.locator('.k-catalogue-selection strong')).toHaveText(action.name);
    await retry.click();
    await expect.poll(() => requests.length).toBe(2);
    expect(requests[1]).toEqual(requests[0]);
    await page.waitForLoadState('networkidle');
    expect(db.exec(`SELECT COUNT(*) FROM ingredient_archive_events WHERE entry_id = ${sqlText(requests[0].entry_id)}`)).toBe('1');
    expect(db.exec(`SELECT archived FROM ingredient_entries WHERE entry_id = ${sqlText(requests[0].entry_id)}`)).toBe('1');
    expect(action.errors).toEqual([]);
  } finally {
    initial.release();
    await test.info().attach('initial-archive-and-nonmatching-search', { body: JSON.stringify({ requests, emptySearch }), contentType: 'application/json' });
  }
});

for (const kind of ['Save', 'Archive'] as const) {
  test(`CAT-L3 newer epoch preserves locked exact ${kind} replay after old denial`, async ({ page }) => {
    const action = await catalogueAction(page, kind);
    const old = catalogueGate();
    const requests: any[] = [];
    await page.route('**/leptos-api/execute_ingredient_command*', async route => {
      const command = ingredientCommand(route);
      if (!command?.[kind]) return route.continue();
      requests.push(command[kind]);
      if (requests.length === 1) {
        await old.hold();
        return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify('Forbidden') });
      }
      await route.continue();
    });
    try {
      await action.fire();
      await old.ready;
      const reauthorized = page.waitForResponse(response => response.url().includes('/list_formulation_styles') && response.ok());
      await page.evaluate(() => window.dispatchEvent(new Event('focus')));
      await reauthorized;
      const active = action.create ?? action.picker;
      await expect(active).toBeVisible();
      const delivered = page.waitForResponse(response => {
        const raw = new URLSearchParams(response.request().postData() ?? '').get('command_json');
        return response.url().includes('execute_ingredient_command') && !!raw && JSON.parse(raw)?.[kind] !== undefined;
      });
      old.release();
      await delivered;
      const retry = active.getByRole('button', { name: 'Retry previous request' });
      await expect(retry).toBeEnabled();
      await expect(page.getByText('Access denied', { exact: true })).toHaveCount(0);
      if (action.create) {
        await expect(action.create.getByLabel('Fermentable name')).toBeDisabled();
        await expect(action.create.getByLabel('Colour (EBC)')).toBeDisabled();
        await expect(action.create.getByLabel('Potential yield (%)')).toBeDisabled();
      } else {
        await expect(action.picker.getByRole('button', { name: 'Edit library entry' })).toBeDisabled();
        // The real result remains clickable, but cannot retarget the retained
        // archive. Filter changes also must not clear its selected entry.
        await action.picker.getByRole('button', { name: 'All fermentables', exact: true }).click();
        await expect(action.picker.getByRole('button', { name: /Weyermann Vienna/ })).toHaveCount(1);
        await action.picker.getByRole('button', { name: /Weyermann Vienna/ }).click();
        await expect(action.picker.locator('.k-catalogue-selection strong')).toHaveText(action.name);
      }
      await retry.click();
      await expect.poll(() => requests.length).toBe(2);
      expect(requests[1]).toEqual(requests[0]);
      await expect(action.picker).toBeVisible();
      await page.waitForLoadState('networkidle');
      expect(db.exec(`SELECT COUNT(*) FROM ingredient_entries WHERE name = ${sqlText(action.name)}`)).toBe('1');
      if (kind === 'Archive') expect(db.exec(`SELECT COUNT(*) FROM ingredient_archive_events WHERE entry_id = ${sqlText(requests[0].entry_id)}`)).toBe('1');
      expect(action.errors).toEqual([]);
    } finally {
      old.release();
      await test.info().attach('original-and-replayed-command', { body: JSON.stringify(requests), contentType: 'application/json' });
    }
  });
}

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
  await expect(create.getByRole('status').filter({ hasText: 'That fermentable and producer are already in your library.' })).toBeVisible();

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
  await expect(create.getByRole('status').filter({ hasText: 'Save may not have completed' })).toBeVisible();
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
  // Do not confuse an older initial/all search with the post-save library
  // refresh. Bind the actual returned row to this retained command's entry ID.
  const retrySearch = page.waitForResponse(async response => {
    if (!response.url().includes('execute_ingredient_command')) return false;
    const payload = new URLSearchParams(response.request().postData() ?? '').get('command_json');
    if (!payload || JSON.parse(payload)?.Search?.filter !== 'workspace') return false;
    const result = await response.json();
    return result.Entries?.items?.some((entry: any) => entry.entry_id === requests[0].entry_id) === true;
  });
  await create.getByRole('button', { name: 'Retry previous request' }).click();
  await retrySearch;
  await page.unroute('**/leptos-api/execute_ingredient_command*');
  await page.waitForLoadState('networkidle');
  await expect(picker.getByRole('button', { name: entryName })).toHaveCount(1);
  expect(requests).toHaveLength(2);
  expect(requests[1].entry_id).toBe(requests[0].entry_id);
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
  await expect(page.getByText('Access denied', { exact: true })).toBeVisible();
  await expect(page.getByLabel('Recipe name', { exact: true })).toBeHidden();
  await expect(picker).toBeHidden();
  expect(errors).toEqual([]);
});

test('a delayed catalogue search denial cannot touch a disposed picker', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  const marker = `catalogue-search-${Date.now()}`;
  await page.goto('/recipe-editor/new');
  await page.evaluate(value => {
    (window as any).__krausenLifecycleMarker = value;
  }, marker);
  let release!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  let delivered!: () => void;
  const deliveredPromise = new Promise<void>(resolve => { delivered = resolve; });
  let started!: () => void;
  const requestStarted = new Promise<void>(resolve => { started = resolve; });
  await page.route('**/leptos-api/execute_ingredient_command*', async route => {
    const payload = new URLSearchParams(route.request().postData() ?? '').get('command_json');
    const command = payload ? JSON.parse(payload) : null;
    if (command?.Search) {
      started();
      await gate;
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify('Forbidden'),
      });
      delivered();
      return;
    }
    await route.continue();
  });
  await page.getByRole('button', { name: 'Add fermentable' }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill' });
  await expect(picker).toBeVisible();
  await requestStarted;
  await page.keyboard.press('Escape');
  await expect(picker).toBeHidden();
  // Real SPA navigation disposes the picker without unloading the JS realm.
  await page.getByRole('link', { name: 'Recipes', exact: true }).first().click();
  await expect(page).toHaveURL(/\/recipes$/);
  expect(await page.evaluate(() => (window as any).__krausenLifecycleMarker)).toBe(marker);
  // Deliver the held denial only after unmount; it must actually complete.
  release();
  await deliveredPromise;
  await page.waitForTimeout(200);
  await expect(page.getByRole('dialog', { name: 'Build your grain bill' })).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('a delayed catalogue save response cannot copy data into a disposed editor', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  const unique = Date.now();
  const entryName = `E2E disposed malt ${unique}`;
  const marker = `catalogue-save-${unique}`;
  await page.goto('/recipe-editor/new');
  await page.evaluate(value => {
    (window as any).__krausenLifecycleMarker = value;
  }, marker);
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
  let release!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  let delivered!: () => void;
  const deliveredPromise = new Promise<void>(resolve => { delivered = resolve; });
  let started!: () => void;
  const requestStarted = new Promise<void>(resolve => { started = resolve; });
  await page.route('**/leptos-api/execute_ingredient_command*', async route => {
    const payload = new URLSearchParams(route.request().postData() ?? '').get('command_json');
    const command = payload ? JSON.parse(payload) : null;
    if (command?.Save) {
      started();
      const response = await route.fetch();
      await gate;
      await route.fulfill({ response });
      delivered();
      return;
    }
    await route.continue();
  });
  await create.getByRole('button', { name: 'Save to library' }).click();
  await requestStarted;
  await page.keyboard.press('Escape');
  await expect(picker).toBeHidden();
  // Handle the dirty Save/Discard/Stay guard with Stay, then clear the draft
  // back to clean so the actual navigation remains a same-document SPA move.
  await page.getByRole('link', { name: 'Recipes', exact: true }).first().click();
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Stay' }).click();
  await expect(guard).toBeHidden();
  await expect(page).toHaveURL(/\/recipe-editor\/new/);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Unsaved changes' })).toHaveCount(0);
  await page.getByRole('link', { name: 'Recipes', exact: true }).first().click();
  await expect(page).toHaveURL(/\/recipes$/);
  expect(await page.evaluate(() => (window as any).__krausenLifecycleMarker)).toBe(marker);
  // Deliver the held mutation response only after unmount; it must complete.
  release();
  await deliveredPromise;
  await page.waitForTimeout(200);
  await expect(page.getByRole('dialog', { name: 'Build your grain bill' })).toHaveCount(0);
  // The mutation committed the library entry but no recipe was created/copied.
  expect(
    db.exec(`SELECT COUNT(*) FROM ingredient_entries WHERE name = ${sqlText(entryName)}`),
  ).toBe('1');
  expect(
    db.exec(`SELECT COUNT(*) FROM recipes WHERE name = ${sqlText(`Disposed recipe ${unique}`)}`),
  ).toBe('0');
  expect(errors).toEqual([]);
});

// KRA-37 shared dialog regressions, appended after the accepted lifecycle tests.
// Reuses that file's existing test/expect/Page imports and loginUser beforeEach.
// No editor.spec.ts/recipe_editor.rs edits or new harness/config/fixture model.

async function kra37DialogSnapshot(page: Page, id: string) {
  return page.locator(`#${id}`).evaluate(element => {
    const dialog = element as HTMLDialogElement;
    const style = getComputedStyle(dialog);
    const body = dialog.querySelector('.k-dialog__body') as HTMLElement;
    const footer = dialog.querySelector('.k-dialog__actions') as HTMLElement;
    return {
      rect: dialog.getBoundingClientRect().toJSON(),
      body: { rect: body.getBoundingClientRect().toJSON(), scrollTop: body.scrollTop, scrollHeight: body.scrollHeight, clientHeight: body.clientHeight, overflowY: getComputedStyle(body).overflowY },
      footer: footer.getBoundingClientRect().toJSON(),
      margin: [style.marginTop, style.marginRight, style.marginBottom, style.marginLeft],
      position: style.position, inset: [style.top, style.right, style.bottom, style.left],
      maxHeight: style.maxHeight, overflowY: style.overflowY,
      native: { open: dialog.open, modal: dialog.matches(':modal') },
      focus: { active: document.activeElement?.outerHTML, inside: dialog.contains(document.activeElement) },
      main: (() => {
        const main = document.querySelector('.app-shell__main') as HTMLElement;
        return { scrollTop: main.scrollTop, scrollHeight: main.scrollHeight, clientHeight: main.clientHeight };
      })(),
      viewport: { width: innerWidth, height: innerHeight, scrollX, scrollY, documentWidth: document.documentElement.scrollWidth,
        visual: window.visualViewport && { width: visualViewport!.width, height: visualViewport!.height, top: visualViewport!.offsetTop, left: visualViewport!.offsetLeft } },
    };
  });
}

test('shared dialogs center the real fermentable picker after open scroll and resize', async ({ page }, testInfo) => {
  for (const theme of ['light', 'dark']) {
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.addInitScript(value => localStorage.setItem('krausen-theme', value), theme);
    await page.goto('/recipe-editor/new');
    // Existing login isolation can set Light. Use the actual appearance control.
    if (await page.locator('html').evaluate(e => e.classList.contains('dark')) !== (theme === 'dark')) {
      await page.getByRole('button', { name: theme === 'dark' ? 'Dark appearance' : 'Light appearance', exact: true }).click();
    }
    await expect(page.locator('html')).toHaveClass(theme === 'dark' ? /\bdark\b/ : /^(?!.*\bdark\b).*$/);
    const opener = page.getByRole('button', { name: 'Add fermentable', exact: true });
    await opener.click();
    const picker = page.getByRole('dialog', { name: 'Build your grain bill', exact: true });
    await expect(picker).toBeVisible();
    await expect(picker.locator('.k-catalogue-item').first()).toBeVisible();
    for (const phase of ['open', 'scrolled-open', 'resize'] as const) {
      if (phase === 'scrolled-open') {
        await picker.getByRole('button', { name: 'Cancel', exact: true }).click();
        await expect(picker).toBeHidden();
        await expect(opener).toBeFocused();
        const main = page.locator('.app-shell__main');
        await main.evaluate(element => { element.scrollTop = Math.min(120, element.scrollHeight - element.clientHeight); });
        await expect.poll(() => main.evaluate(element => element.scrollTop)).toBeGreaterThan(0);
        await testInfo.attach(`main-${theme}-before-open`, {
          body: JSON.stringify(await main.evaluate(element => ({ scrollTop: element.scrollTop, scrollHeight: element.scrollHeight, clientHeight: element.clientHeight }))),
          contentType: 'application/json',
        });
        await opener.click();
        await expect(picker).toBeVisible();
      }
      if (phase === 'resize') await page.setViewportSize({ width: 1920, height: 1080 });
      await page.evaluate(async () => { for (let i = 0; i < 3; i++) await new Promise<void>(resolve => requestAnimationFrame(() => resolve())); });
      const snapshot = await kra37DialogSnapshot(page, 'ingredient-picker');
      await testInfo.attach(`dialog-${theme}-${phase}`, { body: JSON.stringify(snapshot), contentType: 'application/json' });
      if (phase === 'scrolled-open') expect(snapshot.main.scrollTop).toBeGreaterThan(0);
      expect(snapshot.native.modal).toBe(true);
      expect.soft(snapshot.rect.left).toBeGreaterThanOrEqual(16);
      expect.soft(snapshot.rect.right).toBeLessThanOrEqual(snapshot.viewport.width - 16);
      expect.soft(snapshot.rect.top).toBeGreaterThanOrEqual(16);
      expect.soft(snapshot.rect.bottom).toBeLessThanOrEqual(snapshot.viewport.height - 16);
      expect.soft(Math.abs(snapshot.rect.left + snapshot.rect.width / 2 - snapshot.viewport.width / 2)).toBeLessThanOrEqual(2);
      expect.soft(Math.abs(snapshot.rect.top + snapshot.rect.height / 2 - snapshot.viewport.height / 2)).toBeLessThanOrEqual(2);
      expect(snapshot.viewport.documentWidth).toBeLessThanOrEqual(snapshot.viewport.width);
    }
    await picker.getByRole('button', { name: 'Cancel', exact: true }).click();
    await expect(picker).toBeHidden();
    await expect(opener).toBeFocused();
  }
});

test('shared dialogs keep real catalogue and library footer controls reachable in a short viewport', async ({ page }, testInfo) => {
  for (const viewport of [{ width: 320, height: 360 }, { width: 390, height: 320 }]) {
    await page.setViewportSize(viewport);
    await page.goto('/recipe-editor/new');
    await page.getByRole('button', { name: 'Add fermentable', exact: true }).click();
    const picker = page.getByRole('dialog', { name: 'Build your grain bill', exact: true });
    await expect(picker).toBeVisible();
    await expect(picker.locator('.k-catalogue-item').first()).toBeVisible();
    await picker.getByRole('button', { name: /Weyermann Vienna/ }).click();
    await picker.getByLabel('Amount in recipe (kg)').fill('4.2500001');
    for (const label of ['Cancel', 'Add ingredient']) {
      const action = picker.getByRole('button', { name: label, exact: true });
      await action.scrollIntoViewIfNeeded();
      await action.focus();
      const bounds = await action.boundingBox();
      await testInfo.attach(`picker-footer-${viewport.width}-${label}`, { body: JSON.stringify({ action: bounds, dialog: await kra37DialogSnapshot(page, 'ingredient-picker') }), contentType: 'application/json' });
      expect(bounds!.x).toBeGreaterThanOrEqual(0);
      expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(viewport.width);
      expect(bounds!.y).toBeGreaterThanOrEqual(0);
      expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(viewport.height);
      await expect(action).toBeFocused();
    }
    await picker.getByRole('button', { name: 'Create fermentable', exact: true }).first().click();
    const create = page.getByRole('dialog', { name: 'A fermentable of your own', exact: true });
    await expect(create).toBeVisible();
    await create.getByLabel('Fermentable name').fill('Unsaved long-form example');
    await create.getByLabel('Colour (EBC)').fill('4.5000001');
    await create.getByLabel('Potential yield (%)').fill('80.123456');
    for (const label of ['Back to catalogue', 'Save to library']) {
      const action = create.getByRole('button', { name: label, exact: true });
      await action.scrollIntoViewIfNeeded(); await action.focus();
      const bounds = await action.boundingBox();
      await testInfo.attach(`create-footer-${viewport.width}-${label}`, { body: JSON.stringify({ action: bounds, dialog: await kra37DialogSnapshot(page, 'ingredient-create') }), contentType: 'application/json' });
      expect(bounds!.y).toBeGreaterThanOrEqual(0);
      expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(viewport.height);
      expect(bounds!.x).toBeGreaterThanOrEqual(0);
      expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(viewport.width);
      await expect(action).toBeFocused();
    }
    await create.getByRole('button', { name: 'Back to catalogue', exact: true }).click();
    await expect(picker).toBeVisible();
    await picker.getByRole('button', { name: 'Cancel', exact: true }).click();
    await expect(page.getByLabel('Name', { exact: true })).toHaveCount(0);
  }
});

test('shared dialogs preserve raw recipe input and native modal focus on cancel', async ({ page }, testInfo) => {
  await page.goto('/recipe-editor/new');
  await page.waitForLoadState('networkidle');
  const raw = page.getByLabel('Batch volume (L)', { exact: true });
  // Malformed 1e- prevents the existing recipe/catalogue boundary from opening;
  // that separate KRA-42 case is recorded externally, not a dialog focus failure.
  await raw.fill('20.1234500');
  await raw.evaluate(element => { (window as any).__kra37Raw = element; });
  const opener = page.getByRole('button', { name: 'Add fermentable', exact: true });
  await opener.click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill', exact: true });
  const search = picker.getByLabel('Search fermentables', { exact: true });
  await expect(picker).toBeVisible();
  await expect(search).toBeFocused();
  const title = await picker.getAttribute('aria-labelledby');
  const description = await picker.getAttribute('aria-describedby');
  expect(title).toBeTruthy(); expect(description).toBeTruthy();
  await expect(page.locator(`[id="${title}"]`)).toHaveText('Build your grain bill');
  await expect(page.locator(`[id="${description}"]`)).not.toBeEmpty();
  const focusState = () => page.locator('#ingredient-picker').evaluate(element => {
    const dialog = element as HTMLDialogElement;
    return {
      documentFocused: document.hasFocus(), nativeOpen: dialog.open, modal: dialog.matches(':modal'),
      inside: dialog.contains(document.activeElement), active: document.activeElement?.outerHTML.slice(0, 800),
      checking: document.body.innerText.includes('Checking access…'),
    };
  });
  let containedChecks = 0;
  for (const key of ['Tab', 'Shift+Tab']) for (let step = 0; step < 12; step++) {
    await page.keyboard.press(key);
    const state = await focusState();
    await testInfo.attach(`focus-${key}-${step}`, { body: JSON.stringify(state), contentType: 'application/json' });
    if (!state.documentFocused) {
      // Native Tab may leave the document for browser chrome. Return by actual
      // keyboard traversal, not by forcing a search/native-open/focus operation.
      for (let back = 0; back < 4 && !(await focusState()).documentFocused; back++) {
        await page.keyboard.press(key);
        await testInfo.attach(`return-${key}-${step}-${back}`, { body: JSON.stringify(await focusState()), contentType: 'application/json' });
      }
      await expect.poll(async () => (await focusState()).documentFocused).toBe(true);
      // Window focus legitimately starts Checking and temporarily hides native
      // visibility. This unchanged authorized fixture must settle Verified and
      // reopen the retained logical task before traversal continues.
      await expect.poll(async () => {
        const settled = await focusState();
        return settled.documentFocused && settled.nativeOpen && settled.modal && settled.inside && !settled.checking;
      }).toBe(true);
      await testInfo.attach(`settled-${key}-${step}`, { body: JSON.stringify(await focusState()), contentType: 'application/json' });
    } else {
      expect(state.nativeOpen).toBe(true);
      expect(state.modal).toBe(true);
      expect(state.inside).toBe(true);
    }
    containedChecks++;
  }
  expect(containedChecks).toBe(24);
  // Native focus attempt on an inert background control must not move focus out.
  await raw.evaluate(element => element.focus());
  expect(await picker.evaluate(element => element.contains(document.activeElement))).toBe(true);
  await testInfo.attach('native-modal-focus', { body: JSON.stringify(await kra37DialogSnapshot(page, 'ingredient-picker')), contentType: 'application/json' });
  await page.keyboard.press('Escape');
  await expect(picker).toBeHidden();
  await testInfo.attach('after-context-escape', { body: JSON.stringify(await focusState()), contentType: 'application/json' });
  await expect.soft(opener).toBeFocused();
  await expect(raw).toHaveValue('20.1234500');
  expect(await raw.evaluate(element => element === (window as any).__kra37Raw)).toBe(true);
  await expect(page.getByLabel('Name', { exact: true })).toHaveCount(0);
});

test('shared dialogs scroll the real long form and dispatch wrapped footer actions', async ({ page }, testInfo) => {
  test.setTimeout(60_000);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.route('**/public/fonts/**', route => route.abort());
  await page.setViewportSize({ width: 390, height: 640 });
  await page.goto('/recipe-editor/new');
  await page.waitForLoadState('networkidle');
  const opener = page.getByRole('button', { name: 'Add fermentable', exact: true });
  await opener.click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill', exact: true });
  await expect(picker).toBeVisible();
  await picker.getByRole('button', { name: 'Create fermentable', exact: true }).first().click();
  const form = page.getByRole('dialog', { name: 'A fermentable of your own', exact: true });
  await expect(form).toBeVisible();
  const body = form.locator('.k-dialog__body');
  const before = await body.evaluate(element => ({ top: element.scrollTop, height: element.scrollHeight, client: element.clientHeight }));
  expect(before.height).toBeGreaterThan(before.client);
  expect(before.client).toBeGreaterThan(0);
  const bounds = (await body.boundingBox())!;
  await page.mouse.move(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
  await page.mouse.wheel(0, 240);
  await expect.poll(() => body.evaluate(element => element.scrollTop)).toBeGreaterThan(before.top);
  await testInfo.attach('actual-form-body-scroll', { body: JSON.stringify({ before, after: await kra37DialogSnapshot(page, 'ingredient-create') }), contentType: 'application/json' });
  const name = `KRA37 footer ${Date.now()}`;
  await form.getByLabel('Fermentable name').fill(name);
  await form.getByLabel('Colour (EBC)').fill('4.5');
  await form.getByLabel('Potential yield (%)').fill('80.123456');
  await page.setViewportSize({ width: 320, height: 360 });
  // CSS text enlargement, explicitly NOT native OS/browser zoom evidence.
  await page.addStyleTag({ content: 'html { font-size: 200%; }' });
  const actions = form.locator('.k-dialog__actions button');
  const wraps = await actions.evaluateAll(elements => elements.map(element => element.getBoundingClientRect().top));
  expect(new Set(wraps).size).toBeGreaterThan(1);
  for (const label of ['Back to catalogue', 'Save to library']) {
    const action = form.getByRole('button', { name: label, exact: true });
    await action.scrollIntoViewIfNeeded();
    await action.focus();
    await expect(action).toBeFocused();
    const box = (await action.boundingBox())!;
    await testInfo.attach(`wrapped-before-check-${label}`, { body: JSON.stringify(await action.evaluate(element => {
      const style = getComputedStyle(element);
      return {
        button: element.getBoundingClientRect().toJSON(), whiteSpace: style.whiteSpace, minWidth: style.minWidth,
        maxWidth: style.maxWidth, fontSize: style.fontSize, scrollWidth: element.scrollWidth,
        actions: element.parentElement!.getBoundingClientRect().toJSON(),
        dialog: element.closest('dialog')!.getBoundingClientRect().toJSON(),
      };
    })), contentType: 'application/json' });
    expect(box.height).toBeGreaterThanOrEqual(44);
    expect(box.width).toBeGreaterThanOrEqual(44);
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(320);
    expect(box.y).toBeGreaterThanOrEqual(0);
    expect(box.y + box.height).toBeLessThanOrEqual(360);
    expect(await action.evaluate(element => {
      const r = element.getBoundingClientRect();
      return element.contains(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2));
    })).toBe(true);
    await testInfo.attach(`wrapped-${label}`, { body: JSON.stringify({ box, modal: await kra37DialogSnapshot(page, 'ingredient-create') }), contentType: 'application/json' });
  }
  await form.getByRole('button', { name: 'Save to library', exact: true }).press('Enter');
  await expect(picker).toBeVisible();
  expect(db.exec(`SELECT COUNT(*) FROM ingredient_entries WHERE name = ${sqlText(name)}`)).toBe('1');
  await expect(page.getByLabel('Name', { exact: true })).toHaveCount(0);
  const createAgain = picker.getByRole('button', { name: 'Create fermentable', exact: true }).first();
  try {
    await createAgain.click();
  } finally {
    if (await picker.isVisible()) await testInfo.attach('post-save-body-reachability', { body: JSON.stringify(await createAgain.evaluate(element => {
      const body = element.closest('.k-dialog__body') as HTMLElement;
      const dialog = element.closest('dialog') as HTMLDialogElement;
      const r = element.getBoundingClientRect();
      return {
        target: r.toJSON(), hit: document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2)?.outerHTML.slice(0, 500),
        body: { rect: body.getBoundingClientRect().toJSON(), client: body.clientHeight, scroll: body.scrollHeight, top: body.scrollTop,
          minHeight: getComputedStyle(body).minHeight, flexShrink: getComputedStyle(body).flexShrink, overflowY: getComputedStyle(body).overflowY },
        header: dialog.querySelector('.k-dialog__header')!.getBoundingClientRect().toJSON(),
        actions: dialog.querySelector('.k-dialog__actions')!.getBoundingClientRect().toJSON(),
        dialog: { rect: dialog.getBoundingClientRect().toJSON(), client: dialog.clientHeight, scroll: dialog.scrollHeight, top: dialog.scrollTop,
          maxHeight: getComputedStyle(dialog).maxHeight, overflowY: getComputedStyle(dialog).overflowY,
          paddingTop: getComputedStyle(dialog).paddingTop, paddingBottom: getComputedStyle(dialog).paddingBottom },
      };
    })), contentType: 'application/json' });
  }
  await expect(form).toBeVisible();
  await form.getByRole('button', { name: 'Back to catalogue', exact: true }).click();
  await expect(picker).toBeVisible();
  await picker.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(picker).toBeHidden();
  await expect.poll(async () => page.evaluate(() => {
    const active = document.activeElement;
    return active?.textContent === 'Add fermentable' || active?.id === 'recipe-editor-heading';
  })).toBe(true);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test('shared dialogs retain omitted-prop shell returns and mobile nested layers', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto('/recipes');
  await page.waitForLoadState('networkidle');
  for (const [openerName, title] of [['Switch workspace', 'Your workspaces'], ['Account menu', 'Your account']]) {
    const opener = page.getByRole('button', { name: openerName, exact: true });
    await opener.click();
    const dialog = page.getByRole('dialog', { name: title, exact: true });
    await expect(dialog).toBeVisible();
    expect(await dialog.evaluate(element => element.matches(':modal') && element.contains(document.activeElement))).toBe(true);
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
    await expect(opener).toBeFocused();
  }
  await page.goto('/recipe-editor/new');
  await page.waitForLoadState('networkidle');
  const rawTitle = `KRA37 retained ${Date.now()}`;
  await page.getByLabel('Recipe name', { exact: true }).fill(rawTitle);
  const recipes = page.getByRole('link', { name: 'Recipes', exact: true }).first();
  await recipes.click();
  const warning = page.getByRole('dialog', { name: 'Unsaved recipe changes', exact: true });
  await expect(warning).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(warning).toBeHidden();
  await expect(recipes).toBeFocused();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(rawTitle);
  await page.setViewportSize({ width: 390, height: 800 });
  const menu = page.getByRole('button', { name: 'Toggle menu', exact: true });
  await menu.click();
  const navigation = page.getByRole('dialog', { name: 'Navigation', exact: true });
  await expect(navigation).toBeVisible();
  const switcherTrigger = navigation.getByRole('button', { name: 'Switch workspace', exact: true });
  await switcherTrigger.click();
  const switcher = page.getByRole('dialog', { name: 'Your workspaces', exact: true });
  await expect(switcher).toBeVisible();
  expect(await switcher.evaluate(element => element.matches(':modal') && element.contains(document.activeElement))).toBe(true);
  await testInfo.attach('mobile-default-layer', { body: JSON.stringify(await page.evaluate(() => ({
    modals: [...document.querySelectorAll('dialog:modal')].map(element => element.id), active: document.activeElement?.outerHTML,
  }))), contentType: 'application/json' });
  await page.keyboard.press('Escape');
  await expect(switcher).toBeHidden();
  await expect(navigation).toBeVisible();
  await expect(switcherTrigger).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(navigation).toBeHidden();
  await expect(menu).toBeFocused();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test('shared dialogs preserve a rapid successor task across the previous close', async ({ page }, testInfo) => {
  await page.goto('/recipe-editor/new');
  await page.waitForLoadState('networkidle');
  const opener = page.getByRole('button', { name: 'Add fermentable', exact: true });
  await opener.click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill', exact: true });
  await expect(picker).toBeVisible();
  await page.evaluate(() => {
    (window as any).__kra37RapidCloseEvents = [];
    document.addEventListener('close', event => {
      const element = event.target as HTMLDialogElement;
      if (element.id === 'ingredient-picker') (window as any).__kra37RapidCloseEvents.push({
        time: performance.now(), nativeOpenAtDelivery: element.open,
        modalAtDelivery: element.matches(':modal'), active: document.activeElement?.outerHTML.slice(0, 500),
      });
    }, true);
  });
  const cancel = picker.getByRole('button', { name: 'Cancel', exact: true });
  await cancel.evaluate(element => {
    const originalOpener = [...document.querySelectorAll('button')].find(button => button.textContent === 'Add fermentable')!;
    element.addEventListener('click', () => requestAnimationFrame(() => originalOpener.click()), { once: true });
  });
  await cancel.click();
  await expect(picker).toBeVisible();
  await page.evaluate(async () => { for (let frame = 0; frame < 4; frame++) await new Promise<void>(resolve => requestAnimationFrame(() => resolve())); });
  expect(await picker.evaluate(element => element.matches(':modal') && element.contains(document.activeElement))).toBe(true);
  await page.keyboard.press('Escape');
  await expect(picker).toBeHidden();
  await expect(opener).toBeFocused();
  await expect.poll(() => page.evaluate(() => (window as any).__kra37RapidCloseEvents.length)).toBeGreaterThanOrEqual(2);
  const events = await page.evaluate(() => (window as any).__kra37RapidCloseEvents);
  await testInfo.attach('rapid-successor-native-close-delivery', { body: JSON.stringify(events), contentType: 'application/json' });
  expect(events.length).toBeGreaterThanOrEqual(2);
  // Only a recorded open-at-delivery event certifies delayed close-after-reopen.
});

for (const invalidTrigger of ['removed', 'disabled'] as const) {
  test(`shared dialogs use a safe successor when the actual opener is ${invalidTrigger}`, async ({ page }, testInfo) => {
    await page.goto('/recipe-editor/new');
    await page.waitForLoadState('networkidle');
    const raw = page.getByLabel('Batch volume (L)', { exact: true });
    await raw.fill('20.1234500');
    const opener = page.getByRole('button', { name: 'Add fermentable', exact: true });
    await opener.click();
    const picker = page.getByRole('dialog', { name: 'Build your grain bill', exact: true });
    await expect(picker).toBeVisible();
    // Controlled actual-trigger disposal/disable, not a copied controller or
    // claim that navigation removed it; no new production fixture is installed.
    await opener.evaluate((element, state) => {
      (window as any).__kra37RemovedOpener = element;
      if (state === 'removed') element.remove();
      else (element as HTMLButtonElement).disabled = true;
    }, invalidTrigger);
    expect(await page.evaluate(state => state === 'removed'
      ? !(window as any).__kra37RemovedOpener.isConnected
      : (window as any).__kra37RemovedOpener.matches(':disabled'), invalidTrigger)).toBe(true);
    await page.keyboard.press('Escape');
    await expect(picker).toBeHidden();
    await expect(page.locator('#recipe-editor-heading')).toBeFocused();
    await expect(raw).toHaveValue('20.1234500');
    await testInfo.attach(`${invalidTrigger}-trigger-successor`, { body: JSON.stringify(await page.evaluate(() => ({
      removedConnected: (window as any).__kra37RemovedOpener.isConnected, active: document.activeElement?.outerHTML,
      focused: document.hasFocus(), nativeModals: [...document.querySelectorAll('dialog:modal')].map(element => element.id),
    }))), contentType: 'application/json' });
  });
}

test('shared dialogs respect deliberate newer focus and a successor native layer', async ({ page }, testInfo) => {
  await page.goto('/recipe-editor/new');
  await page.waitForLoadState('networkidle');
  const opener = page.getByRole('button', { name: 'Add fermentable', exact: true });
  const picker = page.getByRole('dialog', { name: 'Build your grain bill', exact: true });
  for (const successor of ['heading', 'account']) {
    await opener.click();
    await expect(picker).toBeVisible();
    const cancel = picker.getByRole('button', { name: 'Cancel', exact: true });
    // Real Cancel plus a deliberate next action before delayed return. Does not
    // suppress production close handlers or force search/native open state.
    await cancel.evaluate((element, destination) => {
      element.addEventListener('click', () => requestAnimationFrame(() => {
        if (destination === 'heading') (document.getElementById('recipe-editor-heading') as HTMLElement).focus();
        else {
          (window as any).__kra37ForeignReturn = document.activeElement;
          (document.querySelector('[aria-label="Account menu"]') as HTMLButtonElement).click();
        }
      }), { once: true });
    }, successor);
    await cancel.click();
    await expect(picker).toBeHidden();
    if (successor === 'heading') await expect(page.locator('#recipe-editor-heading')).toBeFocused();
    else {
      const account = page.getByRole('dialog', { name: 'Your account', exact: true });
      await expect(account).toBeVisible();
      expect(await account.evaluate(element => element.matches(':modal') && element.contains(document.activeElement))).toBe(true);
    }
    await page.evaluate(async () => { for (let frame = 0; frame < 3; frame++) await new Promise<void>(resolve => requestAnimationFrame(() => resolve())); });
    if (successor === 'heading') await expect(page.locator('#recipe-editor-heading')).toBeFocused();
    else {
      const account = page.getByRole('dialog', { name: 'Your account', exact: true });
      expect(await account.evaluate(element => element.matches(':modal') && element.contains(document.activeElement))).toBe(true);
      await page.keyboard.press('Escape');
      await expect(account).toBeHidden();
      // Programmatic activation does not focus its trigger. Require the actual
      // pre-layer focused element, not an invented Account-trigger bookmark.
      await expect.poll(() => page.evaluate(() => document.activeElement === (window as any).__kra37ForeignReturn)).toBe(true);
    }
    await testInfo.attach(`newer-${successor}`, { body: JSON.stringify(await page.evaluate(() => ({ active: document.activeElement?.outerHTML, modals: [...document.querySelectorAll('dialog:modal')].map(element => element.id) }))), contentType: 'application/json' });
  }
});

test('shared dialogs never restore a protected opener after current context denial', async ({ page }, testInfo) => {
  const action = await catalogueAction(page, 'Search');
  await page.evaluate(() => {
    (window as any).__kra37DeniedOpener = [...document.querySelectorAll('button')].find(element => element.textContent === 'Add fermentable');
    (window as any).__kra37DeniedFallback = document.getElementById('recipe-editor-heading');
  });
  const requests: unknown[] = [];
  await page.route('**/leptos-api/execute_ingredient_command*', async route => {
    const command = ingredientCommand(route);
    if (!command?.Search) return route.continue();
    requests.push(command.Search);
    await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify('Forbidden') });
  });
  await action.fire();
  await expect(page.getByText('Access denied', { exact: true })).toBeVisible();
  await expect(action.picker).toBeHidden();
  await expect(page.getByLabel('Recipe name', { exact: true })).toBeHidden();
  await page.evaluate(async () => { for (let frame = 0; frame < 3; frame++) await new Promise<void>(resolve => requestAnimationFrame(() => resolve())); });
  const state = await page.evaluate(() => ({
    active: document.activeElement?.outerHTML,
    protectedFocus: document.activeElement === (window as any).__kra37DeniedOpener || document.activeElement === (window as any).__kra37DeniedFallback,
    modals: [...document.querySelectorAll('dialog:modal')].map(element => element.id),
  }));
  await testInfo.attach('denied-context-focus', { body: JSON.stringify(state), contentType: 'application/json' });
  expect(state.protectedFocus).toBe(false);
  expect(state.modals).not.toContain('ingredient-picker');
  expect(requests).toHaveLength(1);
  expect(action.errors).toEqual([]);
});

// Proposed append to the existing catalogue spec. NOT APPLIED / NOT RUN.
// ACC-006: distinguish a working centre hit from readable horizontal access.
test('shared dialogs expose the complete enlarged Create label through the real body scrollport', async ({ page }, testInfo) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.route('**/public/fonts/**', route => route.abort());
  await page.setViewportSize({ width: 320, height: 360 });
  await page.goto('/recipe-editor/new');
  await page.getByRole('button', { name: 'Add fermentable', exact: true }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill', exact: true });
  await expect(picker).toBeVisible();
  await expect(picker.locator('.k-catalogue-item').first()).toBeVisible();
  await page.addStyleTag({ content: 'html { font-size: 200%; }' }); // NOT native zoom.
  const create = picker.getByRole('button', { name: 'Create fermentable', exact: true }).first();
  const body = picker.locator('.k-dialog__body');
  await create.scrollIntoViewIfNeeded();
  const measure = () => create.evaluate(element => {
    const body = element.closest('.k-dialog__body') as HTMLElement;
    const range = document.createRange();
    range.selectNodeContents(element);
    const r = element.getBoundingClientRect();
    return {
      target: r.toJSON(), text: range.getBoundingClientRect().toJSON(),
      body: body.getBoundingClientRect().toJSON(), clientWidth: body.clientWidth,
      clientHeight: body.clientHeight, scrollWidth: body.scrollWidth,
      scrollLeft: body.scrollLeft, scrollTop: body.scrollTop,
      overflowX: getComputedStyle(body).overflowX, direction: getComputedStyle(body).direction,
      whiteSpace: getComputedStyle(element).whiteSpace,
      hit: element.contains(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2)),
      nativeOpen: (element.closest('dialog') as HTMLDialogElement).open,
      documentWidth: document.documentElement.scrollWidth, viewportWidth: innerWidth,
    };
  });
  // Zero is the LTR start boundary, not a force click or a substituted layout.
  await body.evaluate(element => { element.scrollLeft = 0; });
  const start = await measure();
  await testInfo.attach('create-label-start-boundary', { body: JSON.stringify(start), contentType: 'application/json' });
  expect(start.direction).toBe('ltr');
  expect(start.clientHeight).toBeGreaterThan(0);
  if (start.scrollWidth > start.clientWidth) {
    const b = start.body;
    const top = Math.max(0, b.top), bottom = Math.min(360, b.bottom);
    expect(bottom - top).toBeGreaterThan(0);
    await page.mouse.move(b.x + b.width / 2, (top + bottom) / 2);
    await page.mouse.wheel(start.scrollWidth * 2, 0);
    await expect.poll(() => body.evaluate(element => element.scrollLeft)).toBeGreaterThan(0);
    await expect.poll(() => body.evaluate(element => element.scrollWidth - element.clientWidth - element.scrollLeft)).toBeLessThanOrEqual(1);
  }
  const end = await measure();
  await testInfo.attach('create-label-end-boundary', { body: JSON.stringify(end), contentType: 'application/json' });
  await testInfo.attach('create-label-visible-state', { body: await picker.screenshot(), contentType: 'image/png' });
  // Safe internal scrolling is allowed: both text ends must be reachable.
  // Soft assertions retain both measured boundary failures if the label is clipped.
  expect.soft(start.text.left).toBeGreaterThanOrEqual(start.body.left - 1);
  expect.soft(end.text.right).toBeLessThanOrEqual(end.body.right + 1);
  expect(start.nativeOpen && end.nativeOpen).toBe(true);
  expect(end.documentWidth).toBeLessThanOrEqual(end.viewportWidth);
});

// Proposed append to existing catalogue spec. NOT APPLIED / NOT RUN.
// This is new short-edit acceptance, not replay of the lost-response edit case.
test('shared dialogs keep real edit form labels and short footer actions readable', async ({ page }, testInfo) => {
  const name = `KRA37 short edit ${Date.now()}`;
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto('/recipe-editor/new');
  await page.getByRole('button', { name: 'Add fermentable', exact: true }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill', exact: true });
  await picker.getByRole('button', { name: 'Create fermentable', exact: true }).first().click();
  const create = page.getByRole('dialog', { name: 'A fermentable of your own', exact: true });
  await create.getByLabel('Fermentable name').fill(name);
  await create.getByLabel('Colour (EBC)').fill('4.5');
  await create.getByLabel('Potential yield (%)').fill('80.123456');
  const createdResponse = page.waitForResponse(response => {
    if (!response.url().includes('execute_ingredient_command')) return false;
    const payload = new URLSearchParams(response.request().postData() ?? '').get('command_json');
    return payload ? JSON.parse(payload)?.Save?.document?.specification?.name === name : false;
  });
  await create.getByRole('button', { name: 'Save to library', exact: true }).click();
  const created = (await (await createdResponse).json()).Entry;
  expect(created).toBeTruthy();
  await expect(picker).toBeVisible();
  const entryButton = picker.getByRole('button').filter({ has: page.getByText(name, { exact: true }) });
  await expect(entryButton).toHaveCount(1);
  await entryButton.click();
  const edit = page.getByRole('dialog', { name: 'Edit your library entry', exact: true });
  const cases = [{ width: 320, height: 360, theme: 'light' }, { width: 390, height: 320, theme: 'dark' }];
  for (const state of cases) {
    // Change appearance through the real control outside the modal.
    await picker.getByRole('button', { name: 'Cancel', exact: true }).click();
    await expect(picker).toBeHidden();
    await page.setViewportSize({ width: 1440, height: 1000 });
    if (await page.locator('html').evaluate(e => e.classList.contains('dark')) !== (state.theme === 'dark')) {
      await page.getByRole('button', { name: state.theme === 'dark' ? 'Dark appearance' : 'Light appearance', exact: true }).click();
    }
    await expect(page.locator('html')).toHaveClass(state.theme === 'dark' ? /\bdark\b/ : /^(?!.*\bdark\b).*$/);
    await page.setViewportSize({ width: state.width, height: state.height });
    await page.getByRole('button', { name: 'Add fermentable', exact: true }).click();
    await expect(entryButton).toHaveCount(1);
    await entryButton.click();
    await picker.getByRole('button', { name: 'Edit library entry', exact: true }).click();
    await expect(edit).toBeVisible();
    // Regression: assert arrival BEFORE fill/scroll/focus helpers can hide the
    // reproduced retained-native-dialog / removed-focused-child BODY failure.
    await expect(edit.getByLabel('Fermentable name', { exact: true })).toBeFocused();
    expect(await edit.evaluate(element => document.hasFocus()
      && element.matches(':modal') && element.contains(document.activeElement))).toBe(true);
    await expect(edit.getByLabel('Colour (EBC)')).toHaveValue('4.5');
    await page.evaluate(() => document.fonts.ready);
    const labels = ['Fermentable name', 'Producer', 'Type', 'Colour (EBC)', 'Potential yield (%)'];
    for (const label of labels) {
      const field = edit.getByLabel(label, { exact: true });
      await expect(field).toBeEnabled();
      await field.scrollIntoViewIfNeeded();
      const association = await field.evaluate(element => {
        const input = element as HTMLInputElement;
        const ids = (input.getAttribute('aria-describedby') ?? '').split(/\s+/).filter(Boolean);
        return {
          id: input.id, idCount: [...document.querySelectorAll('[id]')].filter(e => e.id === input.id).length,
          labels: [...(input.labels ?? [])].map(l => l.textContent),
          descriptions: ids.map(id => ({ id, text: document.getElementById(id)?.textContent ?? null })),
          rect: input.getBoundingClientRect().toJSON(),
        };
      });
      await testInfo.attach(`edit-${state.theme}-${label}`, { body: JSON.stringify(association), contentType: 'application/json' });
      expect(association.id).not.toBe(''); expect(association.idCount).toBe(1);
      expect(association.labels.length).toBeGreaterThan(0);
      for (const description of association.descriptions) expect(description.text).toBeTruthy();
    }
    for (const label of ['Back to catalogue', 'Save changes']) {
      const action = edit.getByRole('button', { name: label, exact: true });
      await action.scrollIntoViewIfNeeded();
      await action.focus();
      await expect(action).toBeFocused();
      const result = await action.evaluate(element => {
        const r = element.getBoundingClientRect(), s = getComputedStyle(element);
        return { rect: r.toJSON(), hit: element.contains(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2)),
          outline: s.outline, boxShadow: s.boxShadow, color: s.color, background: s.backgroundColor };
      });
      await testInfo.attach(`edit-footer-${state.theme}-${label}`, { body: JSON.stringify(result), contentType: 'application/json' });
      expect(result.rect.width).toBeGreaterThanOrEqual(44); expect(result.rect.height).toBeGreaterThanOrEqual(44);
      expect(result.rect.left).toBeGreaterThanOrEqual(0); expect(result.rect.right).toBeLessThanOrEqual(state.width);
      expect(result.rect.top).toBeGreaterThanOrEqual(0); expect(result.rect.bottom).toBeLessThanOrEqual(state.height);
      expect(result.hit).toBe(true);
      // Outline/shadow values are recorded for actual focus-paint review, not
      // equated to a visible ring merely because an arbitrary CSS value exists.
    }
    await testInfo.attach(`edit-${state.theme}-readability`, { body: JSON.stringify(await edit.evaluate(element => ({
      fontFaces: [...document.fonts].map(f => ({ family: f.family, status: f.status, weight: f.weight })),
      samples: [...element.querySelectorAll('label,input,select,button,h2,p')].map(node => ({
        text: node.textContent?.slice(0, 120), tag: node.tagName,
        styles: [node, ...function* () { let p = node.parentElement; while (p) { yield p; p = p.parentElement; } }()].map(e => {
          const s = getComputedStyle(e); return { color: s.color, background: s.backgroundColor, opacity: s.opacity,
            fontFamily: s.fontFamily, fontSize: s.fontSize, fontWeight: s.fontWeight };
        }),
      })),
    }))), contentType: 'application/json' });
    await testInfo.attach(`edit-${state.theme}-viewport`, { body: await page.screenshot(), contentType: 'image/png' });
    await edit.getByLabel('Colour (EBC)').fill('9.5');
    await edit.getByRole('button', { name: 'Back to catalogue', exact: true }).click();
    await expect(picker).toBeVisible();
    await picker.getByRole('button', { name: 'Edit library entry', exact: true }).click();
    await expect(edit.getByLabel('Colour (EBC)')).toHaveValue('4.5');
    await edit.getByRole('button', { name: 'Back to catalogue', exact: true }).click();
  }
  await picker.getByRole('button', { name: 'Edit library entry', exact: true }).click();
  await edit.getByLabel('Colour (EBC)').fill('9.5');
  const savedResponse = page.waitForResponse(response => {
    if (!response.url().includes('execute_ingredient_command')) return false;
    const payload = new URLSearchParams(response.request().postData() ?? '').get('command_json');
    return payload ? JSON.parse(payload)?.Save?.entry_id === created.entry_id : false;
  });
  await edit.getByRole('button', { name: 'Save changes', exact: true }).click();
  const saved = (await (await savedResponse).json()).Entry;
  expect(saved.entry_id).toBe(created.entry_id);
  expect(saved.revision).toBe(created.revision + 1);
  await expect(picker).toBeVisible();
  expect(db.exec(`SELECT COUNT(*) FROM ingredient_entries WHERE name = ${sqlText(name)}`)).toBe('1');
  await expect(page.getByLabel('Name', { exact: true })).toHaveCount(0);
  await picker.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(picker).toBeHidden();
  await expect.poll(() => page.evaluate(() => document.activeElement?.textContent === 'Add fermentable' || document.activeElement?.id === 'recipe-editor-heading')).toBe(true);
});
