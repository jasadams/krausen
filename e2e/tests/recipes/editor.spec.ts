import { test, expect } from '@playwright/test';
import { DEFAULT_TEST_USER, createEquipmentProfile, createSecondUser, db, getDefaultUserId, getDefaultWorkspaceId, loginUser, openAccount, openSettings, seedWorkspace, setAccountTheme, setAppearance, walCheckpoint } from '../../helpers/test-helpers';

test.beforeEach(async ({ page }) => {
  // The saved appearance is shared account state: start every test from Light.
  setAccountTheme('light');
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
});

for (const unavailable of ['missing', 'disabled', 'idless'] as const) {
  test(`authorized recheck uses a visible safe successor for an ${unavailable} ordinary bookmark`, async ({ page }) => {
    await page.goto('/recipe-editor/new');
    await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
    const opener = unavailable === 'idless'
      ? page.getByRole('button', { name: /^Save (recipe|changes)$/ })
      : page.getByLabel('Batch volume (L)');
    await opener.focus();
    let release!: () => void;
    const held = new Promise<void>(resolve => { release = resolve; });
    let seen!: () => void;
    const started = new Promise<void>(resolve => { seen = resolve; });
    await page.route('**/leptos-api/get_user_context*', async route => {
      const response = await route.fetch();
      seen();
      await held;
      await route.fulfill({ response });
    });
    try {
      await page.evaluate(() => window.dispatchEvent(new Event('focus')));
      await started;
      await expect(opener).toBeHidden();
      if (unavailable === 'missing') await opener.evaluate(element => element.removeAttribute('id'));
      if (unavailable === 'disabled') await opener.evaluate(element => element.setAttribute('disabled', ''));
      release();
      await expect(page.getByLabel('Recipe name', { exact: true })).toBeVisible();
      await expect(page.getByRole('heading', { name: 'Recipe editor', exact: true })).toBeFocused();
    } finally { release(); await page.unrouteAll({ behavior: 'wait' }); }
  });
}

test('cleared print task returns to a visible editor successor after authorized retry', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Print…' }).click();
  const print = page.getByRole('dialog', { name: 'Print preview', exact: true });
  await expect(print).toBeVisible();
  await print.getByRole('button', { name: 'Back to editor', exact: true }).focus();
  await page.route('**/leptos-api/get_user_context*', async route => {
    const response = await route.fetch();
    const context = await response.json();
    await route.fulfill({ response, json: { ...context, workspace_id: 'cleared-print-context-fault' } });
  });
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByText('Recipe held in its original workspace', { exact: true })).toBeVisible();
  await expect(print).toHaveCount(0);
  await page.unrouteAll({ behavior: 'wait' });
  await page.getByRole('button', { name: 'Retry check', exact: true }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Recipe editor', exact: true })).toBeFocused();
});

test('obsolete initial recovery authorization cannot replace newer verified access', async ({ page }) => {
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  let seen!: () => void;
  const started = new Promise<void>(resolve => { seen = resolve; });
  let count = 0;
  await page.route('**/leptos-api/list_formulation_styles*', async route => {
    const index = ++count;
    const response = await route.fetch();
    if (index > 2) { await route.fulfill({ response }); return; }
    if (index === 2) seen();
    await held;
    await route.fulfill(index === 1 ? { response, json: 'Forbidden' } : { response });
  });
  try {
    await page.goto('/recipe-editor/new');
    await started;
    // Initialization and ordinary style loading both authorize before preview
    // is ready. The later recheck's third style response is not held.
    const newer = page.waitForResponse(response => response.url().includes('/list_formulation_styles') && count > 2);
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await newer;
    release();
    await expect(page.getByLabel('Recipe name', { exact: true })).toBeVisible();
    await expect(page.getByText('Access denied', { exact: true })).toHaveCount(0);
  } finally { release(); await page.unrouteAll({ behavior: 'wait' }); }
});

test('disposed initial recovery authorization cannot write into the retired editor', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  let seen!: () => void;
  const started = new Promise<void>(resolve => { seen = resolve; });
  // Only the first request is held; later ones are not intercepted, so no handler
  // is still fetching when the route is removed.
  await page.route('**/leptos-api/list_formulation_styles*', async route => {
    const response = await route.fetch();
    seen();
    await held;
    await route.fulfill({ response });
  }, { times: 1 });
  try {
    await page.goto('/recipe-editor/new');
    await started;
    await openSettings(page);
    await expect(page).toHaveURL(/settings\/profile/);
    // The held style response reaches the page when released; wait for that
    // delivery itself (the Profile page may keep other requests open, so network
    // idleness says nothing about it).
    const delivered = page.waitForResponse(response => response.url().includes('/list_formulation_styles'));
    release();
    await (await delivered).finished();
    await page.unrouteAll({ behavior: 'wait' });
    await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
    expect(errors).toEqual([]);
    await expect(page).toHaveURL(/settings\/profile/);
  } finally { release(); await page.unrouteAll({ behavior: 'wait' }); }
});

test('obsolete preview context preflight cannot destroy a newer print task', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  let seen!: () => void;
  const started = new Promise<void>(resolve => { seen = resolve; });
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  let completed!: () => void;
  const oldCompleted = new Promise<void>(resolve => { completed = resolve; });
  let first = true;
  await page.route('**/leptos-api/get_user_context*', async route => {
    if (!first) { await route.continue(); return; }
    first = false;
    const response = await route.fetch();
    const context = await response.json();
    seen();
    await held;
    await route.fulfill({ response, json: { ...context, workspace_id: 'obsolete-context-fault' } });
    completed();
  });
  try {
    const volume = page.getByLabel('Batch volume (L)');
    await volume.fill('21');
    await started;
    const newer = page.waitForResponse(async response => {
      if (!response.url().includes('/preview_formulation') || !response.ok()) return false;
      try { return (await response.json()).Preview?.context?.draft?.batch_size_l === 22; } catch { return false; }
    });
    await volume.fill('22');
    await newer;
    await page.getByRole('button', { name: 'Recipe actions' }).click();
    await page.getByRole('button', { name: 'Print…' }).click();
    const print = page.getByRole('dialog', { name: 'Print preview', exact: true });
    await expect(print).toBeVisible();
    release();
    await oldCompleted;
    // Synchronize with rendered reactive effects, not a time-based sleep.
    await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
    await expect(print).toBeVisible();
  } finally { release(); }
});

for (const continuation of ['disposed', 'superseded', 'denied-after-edit'] as const) {
test(`explicit save conflict follow-up cannot publish a ${continuation} task`, async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  await page.getByLabel('Recipe name', { exact: true }).fill(`Conflict disposal ${Date.now()}`);
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  const url = page.url();
  await page.getByLabel('Recipe name', { exact: true }).fill('Retained stale local work');
  const remote = await page.context().newPage();
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  let seen!: () => void;
  const started = new Promise<void>(resolve => { seen = resolve; });
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  try {
    await remote.goto(url);
    await remote.getByRole('button', { name: 'Start a separate draft', exact: true }).click();
    await remote.getByLabel('Boil time (min)').fill('65');
    await remote.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
    await expect(remote.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
    let reads = 0;
    await page.route('**/leptos-api/read_recipe*', async route => {
      if (++reads > 1) { await route.continue(); return; }
      const response = await route.fetch();
      seen();
      await held;
      await route.fulfill(continuation === 'denied-after-edit' ? { response, json: 'Forbidden' } : { response });
    });
    await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
    await started;
    await expect(page.getByRole('button', { name: /^Save (recipe|changes)$/ })).toBeEnabled();
    if (continuation === 'denied-after-edit') {
      await page.getByLabel('Recipe name', { exact: true }).fill('New local edit must not cancel current denial');
      release();
      await expect(page.getByText('Access denied', { exact: true })).toBeVisible();
      await expect(page.getByLabel('Recipe name', { exact: true })).toBeHidden();
      expect(errors).toEqual([]);
      return;
    }
    if (continuation === 'superseded') {
      await remote.getByLabel('Boil time (min)').fill('70');
      await remote.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
      await expect(remote.getByRole('status').filter({ hasText: 'Saved revision 3' })).toBeVisible();
      await page.evaluate(() => window.dispatchEvent(new Event('focus')));
      const conflict = page.getByRole('region', { name: 'Save conflict', exact: true });
      await expect(conflict).toContainText('the current saved revision is 3');
      const account = page.getByRole('button', { name: /^Account menu/ });
      await account.focus();
      release();
      await page.unrouteAll({ behavior: 'wait' });
      await page.waitForLoadState('networkidle');
      await expect(conflict).toContainText('the current saved revision is 3');
      await expect(account).toBeFocused();
      expect(errors).toEqual([]);
      return;
    }
    const settingsPopup = await openAccount(page, page.locator('.app-shell__desktop'));
    await settingsPopup.getByRole('link', { name: 'Settings', exact: true }).click();
    await page.getByRole('dialog', { name: 'Unsaved recipe changes' }).getByRole('button', { name: 'Discard and leave' }).click();
    await expect(page).toHaveURL(/settings\/profile/);
    release();
    await page.unrouteAll({ behavior: 'wait' });
    await page.waitForLoadState('networkidle');
    await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
    expect(errors).toEqual([]);
    await expect(page).toHaveURL(/settings\/profile/);
  } finally { release(); await page.unrouteAll({ behavior: 'wait' }); await remote.close(); }
});
}

test('a late committed save cannot leave or rewrite the address after confirmed context loss', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  const name = `Scoped pending save ${Date.now()}`;
  await page.getByLabel('Recipe name', { exact: true }).fill(name);
  const original = page.url();
  let seen!: () => void;
  const started = new Promise<void>(resolve => { seen = resolve; });
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  let completed!: () => void;
  const saveCompleted = new Promise<void>(resolve => { completed = resolve; });
  const requests: string[] = [];
  let committed: { recipe_id: string; revision: number } | undefined;
  await page.route('**/leptos-api/save_recipe*', async route => {
    requests.push(route.request().postData() ?? '');
    const response = await route.fetch();
    committed = (await response.json()).Saved;
    seen();
    await held;
    await route.fulfill({ response });
    completed();
  });
  try {
    await openSettings(page);
    const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
    await guard.getByRole('button', { name: 'Save and leave', exact: true }).click();
    await started;
    const submitted = JSON.parse(requests[0]);
    const retainedAttempt = () => page.evaluate(requestId => Object.keys(localStorage)
      .filter(key => key.startsWith('krausen-draft-v1:'))
      .map(key => JSON.parse(localStorage.getItem(key)!))
      .find(record => record.pending?.request_id === requestId), submitted.request_id);
    await expect.poll(retainedAttempt).toBeTruthy();
    const retained = await retainedAttempt();
    expect(retained.scope.account_id).toBe(submitted.account_id);
    expect(retained.scope.workspace_id).toBe(submitted.workspace_id);
    expect(retained.pending.request_id).toBe(submitted.request_id);
    expect(retained.pending.document).toEqual(submitted.document);
    expect(retained.pending.recipe_id).toBe(submitted.recipe_id);
    expect(retained.pending.expected).toBe(submitted.expected_revision);
    await page.route('**/leptos-api/get_user_context*', async route => {
      const response = await route.fetch();
      const context = await response.json();
      await route.fulfill({ response, json: { ...context, workspace_id: 'confirmed-context-fault' } });
    });
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await expect(page.getByText('Recipe held in its original workspace', { exact: true })).toBeVisible();
    release();
    await saveCompleted;
    // Observe the production receipt, not a frame evaluation that can throw
    // merely because the broken implementation navigates during it.
    await expect(page.locator('.recipe-header__status [role="status"]')).toContainText('Saved revision 1 in the original context.');
    await expect(page).toHaveURL(original);
    expect(requests).toHaveLength(1);
    expect(committed?.revision).toBe(1);
    expect(committed?.recipe_id).toBeTruthy();
    expect(submitted.account_id).toBe(getDefaultUserId());
    expect(submitted.workspace_id).toBe(getDefaultWorkspaceId());
    expect(submitted.request_id).toBeTruthy();
    expect(submitted.document.title).toBe(name);
    const saved = JSON.parse(db.exec(`SELECT json_object('recipe_id', recipe_id, 'workspace_id', workspace_id, 'created_by', created_by, 'revision', native_head_revision) FROM recipes WHERE name = '${name}'`));
    expect(saved).toEqual({ recipe_id: committed!.recipe_id, workspace_id: submitted.workspace_id,
      created_by: submitted.account_id, revision: committed!.revision });
    const revision = JSON.parse(db.exec(`SELECT json_object('revision', revision, 'workspace_id', workspace_id, 'saved_by', saved_by, 'document', json(document)) FROM recipe_revisions WHERE recipe_id = '${committed!.recipe_id}'`));
    expect(revision).toEqual({ revision: committed!.revision, workspace_id: submitted.workspace_id,
      saved_by: submitted.account_id, document: submitted.document });
    expect(db.exec(`SELECT COUNT(*) FROM recipes WHERE name = '${name}'`)).toBe('1');
  } finally { release(); }
});

test('typed print denial never labels a hidden editor as access verified', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  await page.route('**/leptos-api/request_process_summary*', async route => {
    const response = await route.fetch();
    await route.fulfill({ response, json: 'Forbidden' });
  });
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Print…' }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toBeHidden();
  await expect(page.getByText('Access denied', { exact: true })).toBeVisible();
  await expect(page.getByText('Access verified', { exact: true })).toHaveCount(0);
});

test('successful revalidation preserves textarea selection and backward direction', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  const notes = page.getByLabel('Notes', { exact: true });
  await notes.fill('First line\nSecond line');
  await notes.focus();
  await notes.evaluate((element: HTMLTextAreaElement) => element.setSelectionRange(2, 15, 'backward'));
  await Promise.all([
    page.waitForResponse(response => response.url().includes('/get_user_context') && response.ok()),
    page.evaluate(() => window.dispatchEvent(new Event('focus'))),
  ]);
  await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
  await expect(notes).toBeFocused();
  expect(await notes.evaluate((element: HTMLTextAreaElement) => [element.selectionStart, element.selectionEnd, element.selectionDirection]))
    .toEqual([2, 15, 'backward']);
});

test('revalidation never steals a newer deliberate account focus', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  const volume = page.getByLabel('Batch volume (L)');
  await volume.focus();
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  let seen!: () => void;
  const started = new Promise<void>(resolve => { seen = resolve; });
  let completed!: () => void;
  const finished = new Promise<void>(resolve => { completed = resolve; });
  // Only the first request is held. Later ones are not intercepted at all, so no
  // handler is still fetching when the route is removed at the end of the test.
  await page.route('**/leptos-api/get_user_context*', async route => {
    const response = await route.fetch();
    seen();
    await held;
    await route.fulfill({ response });
    completed();
  }, { times: 1 });
  try {
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await started;
    await expect(volume).toBeHidden();
    const account = page.getByRole('button', { name: /^Account menu/ });
    await account.focus();
    release();
    await finished;
    await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
    await expect(account).toBeFocused();
  } finally { release(); await page.unrouteAll({ behavior: 'wait' }); }
});

test('a disposed editor ignores a delayed focus read without reactive panic', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  await page.getByLabel('Recipe name', { exact: true }).fill(`Disposed check ${Date.now()}`);
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  let seen!: () => void;
  const started = new Promise<void>(resolve => { seen = resolve; });
  let completed!: () => void;
  const finished = new Promise<void>(resolve => { completed = resolve; });
  await page.route('**/leptos-api/read_recipe*', async route => {
    const response = await route.fetch();
    seen();
    await held;
    await route.fulfill({ response });
    completed();
  });
  try {
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await started;
    await openSettings(page);
    await expect(page).toHaveURL(/settings\/profile/);
    release();
    await finished;
    await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
    expect(errors).toEqual([]);
    await expect(page).toHaveURL(/settings\/profile/);
  } finally { release(); }
});

test('recovered older-base conflict does not invent an original document from the newer head', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  await page.getByLabel('Recipe name', { exact: true }).fill('Original recovery base');
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  const url = page.url();
  await page.getByLabel('Recipe name', { exact: true }).fill('Retained old-base work');
  await expect.poll(() => page.evaluate(() => Object.keys(localStorage).some(key => key.startsWith('krausen-draft-v1:')))).toBe(true);
  const remote = await page.context().newPage();
  try {
    await remote.goto(url);
    await remote.getByRole('button', { name: 'Start a separate draft', exact: true }).click();
    await remote.getByLabel('Recipe name', { exact: true }).fill('New remote head');
    await remote.getByLabel('Boil time (min)').fill('65');
    await remote.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
    await expect(remote.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
    page.once('dialog', dialog => dialog.accept());
    await page.reload();
    await page.getByRole('button', { name: 'Restore draft', exact: true }).click();
    await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
    const conflict = page.getByRole('region', { name: 'Save conflict', exact: true });
    await expect(conflict).toBeVisible();
    await conflict.getByText('Compare saved revision and your draft', { exact: true }).click();
    await expect(conflict).toContainText('The original revision is not available in this recovered draft.');
    await expect(conflict).not.toContainText('No changes from the original saved revision.');
    await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue('Retained old-base work');
  } finally { await remote.close(); }
});

test('raw formatting settles on blur and correction preserves precision and caret', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  const volume = page.getByLabel('Batch volume (L)');
  await volume.focus();
  await page.keyboard.type('-');
  await expect(volume).toHaveValue('-');
  await expect(volume).toHaveAttribute('aria-invalid', 'false');
  await expect(page.locator('#recipe-batch-description')).toHaveCount(0);
  await expect(page.getByLabel(/Original gravity.*Style reference/)).toHaveCount(0);
  await page.keyboard.press('Tab');
  await expect(volume).toHaveAttribute('aria-invalid', 'true');
  await expect(page.locator('#recipe-batch-description')).toContainText('Enter a number');
  await volume.focus();
  await page.keyboard.press('ControlOrMeta+A');
  const precise = '2.0123456789012345e1';
  await page.keyboard.type(precise);
  await expect(volume).toHaveValue(precise);
  await expect(volume).toHaveAttribute('aria-invalid', 'false');
  await expect(volume).toBeFocused();
  expect(await volume.evaluate((element: HTMLInputElement) => [element.selectionStart, element.selectionEnd]))
    .toEqual([precise.length, precise.length]);
  await page.keyboard.press('Tab');
  await volume.focus();
  await expect(volume).toHaveValue(precise);
  await volume.evaluate((element: HTMLInputElement) => element.setSelectionRange(2, 7));
  await Promise.all([
    page.waitForResponse(response => response.url().includes('/get_user_context') && response.ok()),
    page.evaluate(() => window.dispatchEvent(new Event('focus'))),
  ]);
  await expect(volume).toBeVisible();
  await expect(volume).toBeFocused();
  expect(await volume.evaluate((element: HTMLInputElement) => [element.selectionStart, element.selectionEnd]))
    .toEqual([2, 7]);
  await expect(volume).toHaveValue(precise);
  const original = await volume.elementHandle();
  await page.getByRole('tab', { name: 'Saved versions', exact: true }).click();
  await page.getByRole('tab', { name: 'Recipe', exact: true }).click();
  await expect(volume).toHaveValue(precise);
  expect(await original!.evaluate(element => element.isConnected)).toBe(true);
  for (const [text, native] of [['', null], ['0', 0], ['0.000000000000000', 0], ['.125', 0.125]] as const) {
    const observed = page.waitForRequest(request => {
      if (!request.url().includes('/preview_formulation')) return false;
      const json = new URLSearchParams(request.postData() ?? '').get('request_json');
      return json !== null && JSON.parse(json).draft.batch_size_l === native;
    });
    await volume.focus();
    await page.keyboard.press('ControlOrMeta+A');
    await page.keyboard.press('Backspace');
    if (text) await page.keyboard.type(text);
    await observed;
    await expect(volume).toHaveValue(text);
  }
  await volume.focus();
  await page.keyboard.press('ControlOrMeta+A');
  await page.keyboard.type(precise);
  await expect.poll(() => page.evaluate(() => Object.keys(localStorage)
    .filter(key => key.startsWith('krausen-draft-v1:'))
    .map(key => JSON.parse(localStorage.getItem(key)!))
    .some(record => record.draft.raw_numbers.batch_size_l === '2.0123456789012345e1'))).toBe(true);
  page.once('dialog', dialog => dialog.accept());
  await page.reload();
  await page.getByRole('button', { name: 'Restore draft', exact: true }).click();
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue(precise);
});

test('all numeric caller families settle format feedback on real blur without rewriting raw text', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  const check = async (field: ReturnType<typeof page.getByLabel>) => {
    await field.fill('-');
    await expect(field).toHaveValue('-');
    await expect(field).toHaveAttribute('aria-invalid', 'false');
    await field.press('Tab');
    await expect(field).toHaveAttribute('aria-invalid', 'true');
    const description = await field.getAttribute('aria-describedby');
    expect(description).toBeTruthy();
    // A control may name more than one description (for example its own note);
    // the error must be among them, not necessarily the only one.
    const described = page.locator(description!.split(/\s+/).map(id => `[id="${id}"]`).join(', ')).filter({ hasText: 'Enter a number' });
    await expect(described).toContainText('Enter a number');
    await field.fill('1.230000000000000');
    await field.press('Tab');
    await expect(field).toHaveValue('1.230000000000000');
    await expect(field).toHaveAttribute('aria-invalid', 'false');
  };
  await check(page.getByLabel('Batch volume (L)'));
  await page.getByRole('button', { name: 'Add fermentable', exact: true }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill', exact: true });
  await picker.locator('.k-catalogue-item').first().click();
  await picker.getByRole('button', { name: 'Add to recipe', exact: true }).click();
  await check(page.getByLabel('Amount (kg)', { exact: true }));
  const yeast = page.locator('.k-panel').filter({ has: page.getByRole('heading', { name: 'Yeast', exact: true }) });
  await yeast.getByRole('button', { name: 'Add yeast', exact: true }).click();
  await page.getByRole('dialog', { name: 'Add yeast', exact: true }).getByRole('button', { name: 'Add to recipe', exact: true }).click();
  await check(yeast.getByLabel('Amount', { exact: true }));
  await yeast.getByText('Addition details', { exact: true }).click();
  // Attenuation now sits in the row's details, so the amount check's Tab no
  // longer pre-settles it: it settles on its own real blur like every family.
  await check(yeast.getByLabel('Yeast attenuation (informational) (%)', { exact: true }));
  const mash = page.locator('.k-panel').filter({ has: page.getByRole('heading', { name: 'Mash steps', exact: true }) });
  await mash.getByRole('button', { name: 'Add step', exact: true }).click();
  await check(mash.getByLabel('Time (min)', { exact: true }));
});

test('local retention failure keeps exact work open without claiming durable recovery', async ({ page }) => {
  await page.addInitScript(() => {
    const setItem = Storage.prototype.setItem;
    Storage.prototype.setItem = function(key: string, value: string) {
      if (key.startsWith('krausen-draft-v1:')) throw new DOMException('Controlled draft storage failure', 'QuotaExceededError');
      return setItem.call(this, key, value);
    };
  });
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  await page.getByLabel('Recipe name', { exact: true }).fill('Keep this page open');
  await page.getByLabel('Batch volume (L)').fill('20.000000000000000');
  await expect(page.getByRole('status').filter({ hasText: 'Unable to retain this draft locally; keep this page open.' })).toBeVisible();
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue('20.000000000000000');
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue('Keep this page open');
  await expect(page.getByText('Unsaved changes · retained locally', { exact: true })).toHaveCount(0);
});

test('preview details and calculating text are not a per-keystroke live list', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  await expect(page.locator('.recipe-editor__preview-summary div[role="status"]').filter({ has: page.locator('p') })).toHaveCount(0);
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  let seen!: () => void;
  const started = new Promise<void>(resolve => { seen = resolve; });
  await page.route('**/leptos-api/preview_formulation*', async route => {
    const response = await route.fetch();
    seen();
    await held;
    await route.fulfill({ response });
  });
  try {
    await page.getByLabel('Batch volume (L)').fill('21');
    await started;
    await expect(page.getByText('Calculating…', { exact: true })).toBeVisible();
    await expect(page.getByRole('status').filter({ hasText: 'Calculating…' })).toHaveCount(0);
  } finally { release(); }
});

test('native input undo stays local while application undo restores removed raw rows', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  const title = page.getByLabel('Recipe name', { exact: true });
  await title.fill('Undo ownership');
  const volume = page.getByLabel('Batch volume (L)');
  await volume.focus();
  await page.keyboard.type('20.012345678901234');
  await expect(volume).toHaveValue('20.012345678901234');
  await page.keyboard.press('ControlOrMeta+Z');
  await expect(volume).not.toHaveValue('20.012345678901234');
  await expect(title).toHaveValue('Undo ownership');
  await page.getByRole('button', { name: 'Add fermentable', exact: true }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill', exact: true });
  await picker.locator('.k-catalogue-item').first().click();
  await picker.getByRole('button', { name: 'Add to recipe', exact: true }).click();
  const amount = page.getByLabel('Amount (kg)', { exact: true });
  const id = await amount.getAttribute('id');
  await amount.fill('1.230000000000000');
  await page.getByRole('button', { name: /Remove/ }).last().click();
  await expect(amount).toHaveCount(0);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(amount).toHaveAttribute('id', id!);
  await expect(amount).toHaveValue('1.230000000000000');
  await expect(title).toHaveValue('Undo ownership');
});

test('context network failure has explicit retry and retains scoped raw work and focus', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  const volume = page.getByLabel('Batch volume (L)');
  await page.getByLabel('Recipe name', { exact: true }).fill('Retained during context outage');
  await volume.fill('20.012345678901234');
  await volume.focus();
  await volume.evaluate((element: HTMLInputElement) => element.setSelectionRange(3, 8));
  // Wait for the original preview to finish before failing context reads.
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  await page.route('**/leptos-api/get_user_context*', route => route.abort('failed'));
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(volume).toBeHidden();
  await expect(page.getByText('Network unavailable', { exact: true })).toBeVisible();
  await expect(page.getByText('Recipe held in its original workspace', { exact: true })).toHaveCount(0);
  const retry = page.getByRole('button', { name: 'Retry check', exact: true });
  await expect(retry).toBeVisible();
  await page.unroute('**/leptos-api/get_user_context*');
  await retry.click();
  await expect(volume).toBeVisible();
  await expect(volume).toHaveValue('20.012345678901234');
  await expect(volume).toBeFocused();
  expect(await volume.evaluate((element: HTMLInputElement) => [element.selectionStart, element.selectionEnd]))
    .toEqual([3, 8]);
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue('Retained during context outage');
});

test('calculation retry uses the same draft and a fresh preview generation', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  const requests: any[] = [];
  let failed = false;
  await page.route('**/leptos-api/preview_formulation*', async route => {
    const payload = new URLSearchParams(route.request().postData() ?? '').get('request_json');
    if (payload) requests.push(JSON.parse(payload));
    if (!failed) {
      failed = true;
      await route.abort('failed');
    } else await route.continue();
  });
  await page.getByLabel('Batch volume (L)').fill('20.012345678901234');
  await expect(page.getByRole('status').filter({ hasText: 'Preview unavailable' })).toBeVisible();
  await page.getByRole('button', { name: 'Retry calculation', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  expect(requests).toHaveLength(2);
  expect(requests[1].draft).toEqual(requests[0].draft);
  expect(requests[1].intent).toEqual(requests[0].intent);
  expect(requests[1].draft_generation).not.toBe(requests[0].draft_generation);
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue('20.012345678901234');
});

test('initial recipe load failure retries without leaving the requested recipe', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  const name = `Load retry ${Date.now()}`;
  await page.getByLabel('Recipe name', { exact: true }).fill(name);
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  const url = page.url();
  await page.route('**/leptos-api/read_recipe*', route => route.abort('failed'));
  await page.reload();
  await expect(page.getByText('Unable to load recipe', { exact: true })).toBeVisible();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveCount(0);
  await page.unroute('**/leptos-api/read_recipe*');
  await page.getByRole('button', { name: 'Retry', exact: true }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(name);
  await expect(page).toHaveURL(url);
});

test('authorized focus revalidation preserves an open print task and cleans up on return', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  await page.getByLabel('Recipe name', { exact: true }).fill('Print context continuity');
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Print…' }).click();
  const preview = page.getByRole('dialog', { name: 'Print preview', exact: true });
  await expect(preview).toBeVisible();
  await expect(preview).toContainText('Print context continuity');
  await preview.getByRole('button', { name: 'Back to editor', exact: true }).focus();
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  let seen!: () => void;
  const received = new Promise<void>(resolve => { seen = resolve; });
  await page.route('**/leptos-api/get_user_context*', async route => {
    const response = await route.fetch();
    seen();
    await held;
    await route.fulfill({ response });
  });
  try {
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await received;
    await expect(preview).toBeHidden();
    release();
    await expect(preview).toBeVisible();
    await expect(preview).toContainText('Print context continuity');
    expect(await preview.evaluate(element => element.contains(document.activeElement))).toBe(true);
    await preview.getByRole('button', { name: 'Back to editor', exact: true }).click();
    await expect(preview).toHaveCount(0);
    await expect(page.locator('body')).not.toHaveClass(/krausen-printing/);
    await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue('Print context continuity');
  } finally {
    release();
    await page.unrouteAll({ behavior: 'wait' });
  }
});

test('a delayed save freezes its payload and preserves newer edits without duplicate dispatch', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  const title = page.getByLabel('Recipe name', { exact: true });
  const volume = page.getByLabel('Batch volume (L)');
  const name = `Frozen save ${Date.now()}`;
  await title.fill(name);
  await volume.fill('20.012345678901234');
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  let seen!: () => void;
  const received = new Promise<void>(resolve => { seen = resolve; });
  const requests: string[] = [];
  await page.route('**/leptos-api/save_recipe*', async route => {
    requests.push(route.request().postData() ?? '');
    const response = await route.fetch();
    seen();
    await held;
    await route.fulfill({ response });
  });
  try {
    const save = page.getByRole('button', { name: /^Save (recipe|changes)$/ });
    await save.click();
    await received;
    await expect(save).toBeDisabled();
    await volume.fill('21.012345678901234');
    await title.fill(`${name} newer`);
    expect(requests).toHaveLength(1);
    expect(requests[0]).not.toContain('newer');
    release();
    await expect(page.getByRole('status').filter({ hasText: 'newer changes remain unsaved' })).toBeVisible();
    await expect(title).toHaveValue(`${name} newer`);
    await expect(volume).toHaveValue('21.012345678901234');
    const saved = await page.context().newPage();
    try {
      await saved.goto(page.url());
      // A retained candidate is local work, not the saved server revision.
      const separate = saved.getByRole('button', { name: 'Start a separate draft', exact: true });
      await expect(separate).toBeVisible();
      await separate.click();
      await expect(saved.getByLabel('Recipe name', { exact: true })).toHaveValue(name);
      await expect(saved.getByLabel('Batch volume (L)')).toHaveValue('20.012345678901234');
    } finally {
      await saved.close();
    }
    await page.getByRole('link', { name: 'Recipes', exact: true }).first().click();
    await expect(page.getByRole('dialog', { name: 'Unsaved recipe changes' })).toBeVisible();
    await page.getByRole('button', { name: 'Stay', exact: true }).click();
    await expect(title).toHaveValue(`${name} newer`);
  } finally {
    release();
  }
});

test('an explicit save keeps a numeric draft and reopens its native revision', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  const title = `Browser recipe ${Date.now()}`;
  await page.getByLabel('Recipe name', { exact: true }).fill(title);
  const volume = page.getByLabel('Batch volume (L)');
  await volume.fill('-');
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Complete invalid number fields before saving.' })).toBeVisible();
  await expect(volume).toHaveValue('-');
  await volume.fill('20');
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  await expect(page).toHaveURL(/\/recipe-editor\/(?!new)[^/]+$/);
  await page.reload();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(title);
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue('20');
  await page.getByLabel('Wort attenuation (%)').fill('75');
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
  await page.reload();
  await expect(page.getByLabel('Wort attenuation (%)')).toHaveValue('75');
  const id = page.url().split('/').pop()!;
  const before = Number(db.exec(`SELECT COUNT(*) FROM recipe_revisions WHERE recipe_id = '${id}'`));
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
  expect(Number(db.exec(`SELECT COUNT(*) FROM recipe_revisions WHERE recipe_id = '${id}'`))).toBe(before);
  expect(before).toBe(2);
  expect(db.exec(`SELECT json_extract(document, '$.boil_minutes') IS NULL FROM recipe_revisions WHERE recipe_id = '${id}' AND revision = 2`)).toBe('1');
});

test('process composes by brewing stage and keeps raw controls through save, reload and Undo', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  for (const name of ['Mash & water', 'Boil & whirlpool', 'Fermentation', 'Description & notes']) {
    await expect(page.getByRole('heading', { level: 2, name, exact: true })).toBeVisible();
  }
  await expect(page.getByText('Recipe process and notes', { exact: true })).toHaveCount(0);
  // UX-004: one boil control, owned by Boil & whirlpool rather than the identity header.
  await expect(page.getByLabel('Boil time (min)', { exact: true })).toHaveCount(1);
  await expect(page.locator('#recipe-section-boil').getByLabel('Boil time (min)', { exact: true })).toBeVisible();
  // Conditional water controls are absent until they can apply.
  await expect(page.locator('#recipe-mash-water')).toHaveCount(0);
  await expect(page.locator('#recipe-top-up')).toHaveCount(0);
  await page.getByLabel('Recipe name', { exact: true }).fill(`Process ${Date.now()}`);
  await page.getByLabel('Batch volume (L)').fill('20');
  await page.locator('#recipe-mash-method').selectOption('manual');
  await page.locator('#recipe-mash-water').fill('15.250');
  await page.locator('#recipe-top-up').fill('0');
  await page.locator('#recipe-boil').fill('60');
  await page.locator('#recipe-whirlpool').fill('15');
  await page.locator('#recipe-whirlpool-temp').fill('80.5');
  await page.getByLabel('Wort attenuation (%)').fill('77.5');
  await page.getByLabel('Description', { exact: true }).fill('A clean lager.');
  await page.getByLabel('Notes', { exact: true }).fill('Second line\nthird');
  const mash = page.locator('#recipe-section-mash');
  await mash.getByRole('button', { name: 'Add step', exact: true }).click();
  const step = mash.getByRole('group', { name: 'Mash step 1 of 1' });
  await step.getByLabel('Step name', { exact: true }).fill('Saccharification');
  await step.getByLabel('Time (min)', { exact: true }).fill('60.0');
  await step.getByLabel('Temperature (°C)', { exact: true }).fill('0');
  const ferment = page.locator('#recipe-section-fermentation');
  await ferment.getByRole('button', { name: 'Add step', exact: true }).click();
  await ferment.getByLabel('Temperature (°C)', { exact: true }).fill('18');
  // Clearing the method must not hide or erase a stored mash volume.
  await page.locator('#recipe-mash-method').selectOption('');
  await expect(page.locator('#recipe-mash-water')).toHaveValue('15.250');
  await page.locator('#recipe-mash-method').selectOption('manual');
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  await page.reload();
  await expect(page.locator('#recipe-mash-water')).toHaveValue('15.25');
  await expect(page.locator('#recipe-top-up')).toHaveValue('0');
  await expect(page.locator('#recipe-boil')).toHaveValue('60');
  await expect(page.locator('#recipe-whirlpool-temp')).toHaveValue('80.5');
  await expect(page.getByLabel('Wort attenuation (%)')).toHaveValue('77.5');
  await expect(page.getByLabel('Description', { exact: true })).toHaveValue('A clean lager.');
  await expect(page.getByLabel('Notes', { exact: true })).toHaveValue('Second line\nthird');
  const id = page.url().split('/').pop()!;
  const json = (path: string) => db.exec(`SELECT json_extract(document, '${path}') FROM recipe_revisions WHERE recipe_id = '${id}' AND revision = 1`);
  expect(json('$.boil_minutes')).toBe('60');
  expect(json('$.process.apparent_attenuation_fraction')).toBe('0.775');
  expect(json('$.process.kettle_top_up_l')).toBe('0');
  expect(json('$.process.mash_steps[0].temperature_c')).toBe('0');
  expect(json('$.process.mash_steps[0].volume_l') ?? '').toBe('');
  expect(json('$.process.fermentation_steps[0].time_minutes') ?? '').toBe('');
  expect(json('$.process.fermentation_steps[0].temperature_c')).toBe('18');
});

test('adding and removing steps keeps focus, unique context and Undo restores order and raw text', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  const mash = page.locator('#recipe-section-mash');
  const add = mash.getByRole('button', { name: 'Add step', exact: true });
  for (const name of ['Mash in', 'Rest', 'Rest']) {
    await add.click();
    const count = await mash.getByRole('group', { name: /^Mash step/ }).count();
    const added = mash.getByRole('group', { name: new RegExp(`^Mash step ${count} of ${count}`) });
    await expect(added.getByLabel('Step name', { exact: true })).toBeFocused();
    // The outcome is announced once; the next edit (typing the name) leaves it behind.
    await expect(mash.getByRole('status').filter({ hasText: `Added Mash step ${count} of ${count}.` })).toBeVisible();
    await added.getByLabel('Step name', { exact: true }).fill(name);
    await expect(mash.getByRole('status').filter({ hasText: 'Added Mash step' })).toHaveCount(0);
  }
  // Duplicate names stay distinguishable by position.
  await expect(mash.getByRole('group', { name: 'Mash step 2 of 3, Rest' })).toBeVisible();
  await expect(mash.getByRole('group', { name: 'Mash step 3 of 3, Rest' })).toBeVisible();
  const second = mash.getByRole('group', { name: 'Mash step 2 of 3, Rest' });
  await second.getByLabel('Time (min)', { exact: true }).fill('30.123456789012345678');
  await second.getByLabel('Temperature (°C)', { exact: true }).fill('6x');
  // Middle: focus moves to the next step's Remove action.
  await second.getByRole('button', { name: 'Remove step, Mash step 2 of 3, Rest' }).click();
  await expect(mash.getByRole('status').filter({ hasText: 'Removed Mash step 2 of 3, Rest.' })).toBeVisible();
  await expect(mash.getByRole('button', { name: 'Remove step, Mash step 2 of 2, Rest' })).toBeFocused();
  // Last: focus moves to the previous step's Remove action.
  await mash.getByRole('button', { name: 'Remove step, Mash step 2 of 2, Rest' }).click();
  await expect(mash.getByRole('button', { name: 'Remove step, Mash step 1 of 1, Mash in' })).toBeFocused();
  // Only: focus falls back to Add step.
  await mash.getByRole('button', { name: 'Remove step, Mash step 1 of 1, Mash in' }).click();
  await expect(add).toBeFocused();
  await expect(mash.getByRole('group', { name: /^Mash step/ })).toHaveCount(0);
  // Undo (most recent first) restores IDs, order and the exact typed text.
  const undo = mash.getByRole('button', { name: /^Undo removal of/ });
  await undo.click();
  await undo.click();
  await undo.click();
  await expect(mash.getByRole('group', { name: /^Mash step/ })).toHaveCount(3);
  await expect(mash.getByRole('group', { name: 'Mash step 1 of 3, Mash in' })).toBeVisible();
  const restored = mash.getByRole('group', { name: 'Mash step 2 of 3, Rest' });
  await expect(restored.getByLabel('Time (min)', { exact: true })).toHaveValue('30.123456789012345678');
  await expect(restored.getByLabel('Temperature (°C)', { exact: true })).toHaveValue('6x');
  await expect(mash.getByRole('button', { name: /^Undo removal of/ })).toHaveCount(0);
});

test('first step removal focuses the next step and a stale step Undo returns to Add step', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  const mash = page.locator('#recipe-section-mash');
  const add = mash.getByRole('button', { name: 'Add step', exact: true });
  for (const name of ['One', 'Two']) {
    await add.click();
    const count = await mash.getByRole('group', { name: /^Mash step/ }).count();
    await mash.getByRole('group', { name: new RegExp(`^Mash step ${count} of ${count}`) }).getByLabel('Step name', { exact: true }).fill(name);
  }
  // First of several: focus moves to the next step's Remove action.
  await mash.getByRole('button', { name: 'Remove step, Mash step 1 of 2, One' }).click();
  await expect(mash.getByRole('button', { name: 'Remove step, Mash step 1 of 1, Two' })).toBeFocused();
  // The step Undo's own message is dropped by the next draft change, not left claiming an old outcome.
  await expect(mash.getByRole('status').filter({ hasText: 'Removed Mash step 1 of 2, One.' })).toBeVisible();
  // Header Undo puts the step back; the step Undo is now redundant and must not drop focus onto the body.
  await page.getByRole('button', { name: 'Undo', exact: true }).first().click();
  await expect(mash.getByRole('group', { name: /^Mash step/ })).toHaveCount(2);
  await expect(mash.getByRole('status').filter({ hasText: 'Removed Mash step 1 of 2, One.' })).toHaveCount(0);
  const stepUndo = mash.getByRole('button', { name: /^Undo removal of/ });
  await stepUndo.focus();
  await stepUndo.click();
  await expect(mash.getByRole('status').filter({ hasText: 'is already in the recipe.' })).toBeVisible();
  await expect(add).toBeFocused();
  // Added message is likewise dropped by a later unrelated edit.
  await add.click();
  await expect(mash.getByRole('status').filter({ hasText: 'Added Mash step 3 of 3.' })).toBeVisible();
  await page.getByRole('button', { name: 'Undo', exact: true }).first().click();
  await expect(mash.getByRole('status').filter({ hasText: 'Added Mash step 3 of 3.' })).toHaveCount(0);
});

test('step focus succession never steals a field the user already moved to', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  // Add step, then (in the same task) move to Boil time, as a fast typist does before the next frame.
  await page.evaluate(() => {
    (document.getElementById('process.mash_steps-add') as HTMLElement).click();
    (document.getElementById('recipe-boil') as HTMLElement).focus();
  });
  await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
  await expect(page.locator('#recipe-boil')).toBeFocused();
  // Without a deliberate move, focus still lands on the new step's name.
  await page.locator('#recipe-section-mash').getByRole('button', { name: 'Add step', exact: true }).click();
  await expect(page.locator('#recipe-section-mash').getByRole('group', { name: 'Mash step 2 of 2' }).getByLabel('Step name', { exact: true })).toBeFocused();
});

async function recipeWithWaterPlan(page: import('@playwright/test').Page) {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  await page.getByLabel('Batch volume (L)').fill('20');
  await page.locator('#recipe-boil').fill('60');
  await page.locator('#recipe-mash-method').selectOption('manual');
  await page.locator('#recipe-mash-water').fill('15');
  await expect(page.locator('#recipe-section-mash').getByRole('heading', { name: 'Water plan' })).toBeVisible();
  await expect(page.getByTestId('estimate-stale')).toHaveText('');
}

// Mirrors STALE_ANNOUNCE_DELAY_MS in recipe_process.rs: how long an estimate stays out of
// date before it is announced. Tests hold or delay the preview response past or inside it.
const ANNOUNCE_DEBOUNCE_MS = 800;
const PROFILE_VALUES = { 'profile-mash-efficiency': '75', 'profile-boil-off': '3', 'profile-transfer-loss': '2', 'profile-deadspace': '0', 'profile-absorption': '0.8', 'profile-thickness': '2.5', 'profile-capacity': '30', 'profile-diameter': '35', 'profile-opening': '30', 'profile-boil-temperature': '100' };

// A recipe whose Water plan shows real numbers: grain, an equipment profile and manual mash water.
async function recipeWithNumbers(page: import('@playwright/test').Page, profileName = `Numbers ${Date.now()}`) {
  const profile = profileName;
  const profileId = await createEquipmentProfile(page, profile, PROFILE_VALUES, 'natural');
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  await page.getByLabel('Batch volume (L)').fill('20');
  await page.locator('#recipe-boil').fill('60');
  await page.getByRole('button', { name: 'Add fermentable' }).click();
  await page.getByLabel('Search fermentables').fill('Joe White Signature');
  await page.getByRole('button', { name: /Joe White Signature Lager/ }).click();
  await page.getByLabel('Amount in recipe (kg)').fill('4');
  await page.getByLabel('Build your grain bill').getByRole('button', { name: 'Add to recipe' }).click();
  await page.getByText('Equipment settings', { exact: true }).click();
  await chooseEquipmentProfile(page, profile);
  await page.locator('#recipe-mash-method').selectOption('manual');
  await page.locator('#recipe-mash-water').fill('15');
  await expect(page.locator('#recipe-section-mash .k-metric').filter({ hasText: 'Mash water' })).toContainText('15.0');
  await expect(page.getByTestId('estimate-stale')).toHaveText('');
  return profileId
}
const waterValues = (page: import('@playwright/test').Page) => page.locator('#recipe-section-mash .k-metric-list .k-metric__value').allTextContents();

for (const width of [1440, 390, 320]) {
  test(`typing in a process field never moves it (measured against the Mash panel) while the estimate recalculates at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await recipeWithNumbers(page);
    const before = await waterValues(page);
    expect(before.some(text => text.includes('15.0'))).toBe(true);
    // Hold every preview so the whole recalculation window is observable.
    let release!: () => void;
    const held = new Promise<void>(resolve => { release = resolve; });
    await page.route('**/leptos-api/preview_formulation*', async route => { await held; await route.continue(); });
    const whirlpool = page.locator('#recipe-whirlpool');
    await whirlpool.focus();
    // Sample the focused input's position on every animation frame for the whole window.
    await page.evaluate(() => {
      const samples: number[] = [];
      (window as any).__samples = samples;
      (window as any).__announced = [] as string[];
      const input = document.getElementById('recipe-whirlpool')!;
      const panel = document.getElementById('recipe-section-mash')!;
      // Relative to the mash panel: what sits above the process sections (the estimates summary) is not ours.
      const tick = () => { samples.push(Math.round((input.getBoundingClientRect().top - panel.getBoundingClientRect().top) * 100) / 100); requestAnimationFrame(tick); };
      requestAnimationFrame(tick);
      const target = document.querySelector('[data-testid="estimate-announce"]')!;
      new MutationObserver(() => { if (target.textContent) (window as any).__announced.push(target.textContent); }).observe(target, { childList: true, characterData: true, subtree: true });
    });
    const mash = page.locator('#recipe-section-mash');
    await page.keyboard.press('1');
    // INT-001: while recalculating the old numbers stay but are dimmed, named and hidden from
    // assistive technology at once; the announcement waits for the debounce.
    await expect(page.getByTestId('estimate-stale')).toHaveText('Out of date, recalculating');
    await expect(page.getByTestId('estimate-announce')).toHaveText('');
    await expect(mash.locator('.k-stale--active')).toHaveCount(1);
    await expect(mash.locator('.k-stale[aria-hidden="true"]')).toHaveCount(1);
    expect(await waterValues(page), 'old numbers stay (dimmed) while recalculating').toEqual(before);
    await expect(page.getByTestId('estimate-announce')).toHaveText('Out of date, recalculating', { timeout: ANNOUNCE_DEBOUNCE_MS + 2000 });
    // The outcome settling as invalid is announced once (the status must not keep saying 'recalculating').
    await page.keyboard.press('x');
    await expect(page.getByTestId('estimate-stale')).toHaveText('Estimate unavailable until the inputs are valid');
    await expect(page.getByTestId('estimate-announce')).toHaveText('Estimate unavailable until the inputs are valid');
    // Going back to calculating and typing on is silent.
    await page.keyboard.press('Backspace');
    await page.keyboard.press('2');
    await expect(page.getByTestId('estimate-stale')).toHaveText('Out of date, recalculating');
    await page.waitForTimeout(ANNOUNCE_DEBOUNCE_MS + 400);
    await expect(page.getByTestId('estimate-announce')).toHaveText('Estimate unavailable until the inputs are valid');
    release();
    await expect(page.getByTestId('estimate-stale')).toHaveText('');
    await expect(page.getByTestId('estimate-announce')).toHaveText('Estimate updated');
    await expect(page.getByTestId('estimate-announce')).toHaveText('', { timeout: 6000 });
    await expect(mash.locator('.k-stale--active')).toHaveCount(0);
    await expect(mash.locator('.k-stale[aria-hidden="true"]')).toHaveCount(0);
    await expect(whirlpool).toBeFocused();
    expect(await page.evaluate(() => (window as any).__announced), 'announced once, the settled outcome once, then the settle').toEqual(['Out of date, recalculating', 'Estimate unavailable until the inputs are valid', 'Estimate updated']);
    const samples: number[] = await page.evaluate(() => (window as any).__samples);
    expect(samples.length, 'frames sampled across the window').toBeGreaterThan(30);
    // Layout is stable to sub-pixel rounding (a hundredth of a pixel is font/rounding noise, not a shift).
    expect(Math.max(...samples) - Math.min(...samples), `focused input moved: ${[...new Set(samples)]}`).toBeLessThan(0.25);
    await page.unroute('**/leptos-api/preview_formulation*');
  });
}

test('a quick recalculation is not announced but a slow one is', async ({ page }) => {
  await recipeWithNumbers(page);
  const whirlpool = page.locator('#recipe-whirlpool');
  await whirlpool.focus();
  await page.evaluate(() => {
    const seen: string[] = [];
    (window as any).__announced = seen;
    const target = document.querySelector('[data-testid="estimate-announce"]')!;
    new MutationObserver(() => { if (target.textContent) seen.push(target.textContent); }).observe(target, { childList: true, characterData: true, subtree: true });
  });
  // A response that arrives well inside the debounce: the client's own 250ms wait, the access check and the
  // calculation already take part of it, so the route adds only a fixed 50ms.
  await page.route('**/leptos-api/preview_formulation*', async route => { await new Promise(resolve => setTimeout(resolve, 50)); await route.continue(); });
  await page.keyboard.type('12', { delay: 50 });
  await expect(page.getByTestId('estimate-stale')).toHaveText('');
  await page.waitForTimeout(ANNOUNCE_DEBOUNCE_MS + 400);
  expect(await page.evaluate(() => (window as any).__announced), 'announced during a fast recalculation').toEqual([]);
  await page.unroute('**/leptos-api/preview_formulation*');
  // A response slower than the debounce is announced exactly once, then the settle.
  await page.route('**/leptos-api/preview_formulation*', async route => { await new Promise(resolve => setTimeout(resolve, ANNOUNCE_DEBOUNCE_MS + 700)); await route.continue(); });
  await page.keyboard.type('3', { delay: 50 });
  await expect(page.getByTestId('estimate-announce')).toHaveText('Out of date, recalculating');
  await expect(page.getByTestId('estimate-announce')).toHaveText('Estimate updated');
  await expect(page.getByTestId('estimate-announce')).toHaveText('', { timeout: 6000 });
  expect(await page.evaluate(() => (window as any).__announced)).toEqual(['Out of date, recalculating', 'Estimate updated']);
  await page.unroute('**/leptos-api/preview_formulation*');
});

test('a failure after the announcement is announced once instead of leaving the status stale', async ({ page }) => {
  await recipeWithNumbers(page);
  const whirlpool = page.locator('#recipe-section-boil #recipe-whirlpool');
  await whirlpool.focus();
  await page.evaluate(() => {
    const seen: string[] = [];
    (window as any).__announced = seen;
    const target = document.querySelector('[data-testid="estimate-announce"]')!;
    new MutationObserver(() => { if (target.textContent) seen.push(target.textContent); }).observe(target, { childList: true, characterData: true, subtree: true });
  });
  // The response is slow enough to be announced, THEN it fails (not an immediate failure).
  await page.route('**/leptos-api/preview_formulation*', async route => { await new Promise(resolve => setTimeout(resolve, ANNOUNCE_DEBOUNCE_MS + 600)); await route.abort(); });
  await page.keyboard.type('7');
  await expect(page.getByTestId('estimate-announce')).toHaveText('Out of date, recalculating');
  await expect(page.getByTestId('estimate-announce')).toHaveText('Estimate unavailable');
  await expect(page.getByTestId('estimate-stale')).toHaveText('Estimate unavailable');
  await page.unroute('**/leptos-api/preview_formulation*');
  await page.getByRole('button', { name: 'Retry calculation' }).click();
  await expect(page.getByTestId('estimate-announce')).toHaveText('Estimate updated');
  await expect(page.getByTestId('estimate-announce')).toHaveText('', { timeout: 6000 });
  expect(await page.evaluate(() => (window as any).__announced)).toEqual(['Out of date, recalculating', 'Estimate unavailable', 'Estimate updated']);
  // The same when the held request is overtaken by an invalid value.
  await page.evaluate(() => { (window as any).__announced.length = 0; });
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  await page.route('**/leptos-api/preview_formulation*', async route => { await held; await route.continue(); });
  await whirlpool.focus(); // Retry took the focus
  await page.keyboard.type('8');
  await expect(page.getByTestId('estimate-announce')).toHaveText('Out of date, recalculating');
  await page.keyboard.type('-');
  await expect(page.getByTestId('estimate-announce')).toHaveText('Estimate unavailable until the inputs are valid');
  await page.keyboard.press('Backspace');
  release();
  await expect(page.getByTestId('estimate-announce')).toHaveText('Estimate updated');
  expect(await page.evaluate(() => (window as any).__announced)).toEqual(['Out of date, recalculating', 'Estimate unavailable until the inputs are valid', 'Estimate updated']);
  await page.unroute('**/leptos-api/preview_formulation*');
});

for (const width of [1440, 601, 390, 320]) {
  test(`an invalid or unavailable estimate shows dashes and its own outcome at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await recipeWithNumbers(page);
    const mash = page.locator('#recipe-section-mash');
    const whirlpool = page.locator('#recipe-whirlpool');
    const numbers = await waterValues(page);
    expect(numbers.some(text => text.includes('15.0'))).toBe(true);
    await whirlpool.focus();
    const offset = async () => (await whirlpool.boundingBox())!.y - (await mash.boundingBox())!.y;
    const top = await offset();
    const planHeight = (await mash.locator('.k-metric-list').boundingBox())!.height;
    await page.keyboard.type('-');
    // Settled as invalid: dashes replace every old number, the cue names the outcome, nothing moves.
    await expect(page.getByTestId('estimate-stale')).toHaveText('Estimate unavailable until the inputs are valid');
    await expect.poll(async () => (await waterValues(page)).every(text => text.startsWith('—'))).toBe(true);
    expect((await waterValues(page)).join(' ')).not.toMatch(/\d\.\d/);
    expect((await mash.locator('.k-metric-list').boundingBox())!.height, 'plan height').toBe(planHeight);
    expect(Math.abs((await offset()) - top), 'focused input moved within the process sections when the cue appeared').toBeLessThan(0.25);
    await expect(page.getByTestId('estimate-announce')).toHaveText('Estimate unavailable until the inputs are valid', { timeout: ANNOUNCE_DEBOUNCE_MS + 2000 });
    await expect(mash.locator('.k-stale[aria-hidden="true"]')).toHaveCount(1);
    await whirlpool.fill('10');
    await expect(page.getByTestId('estimate-stale')).toHaveText('');
    await expect.poll(() => waterValues(page)).toEqual(numbers);
    // A network failure is neutral, agrees with the preview panel's own message, and also shows dashes.
    await page.route('**/leptos-api/preview_formulation*', route => route.abort());
    await whirlpool.fill('12');
    await expect(page.getByText('Preview unavailable. Your draft remains open.')).toBeVisible();
    await expect(page.getByTestId('estimate-stale')).toHaveText('Estimate unavailable');
    await expect.poll(async () => (await waterValues(page)).every(text => text.startsWith('—'))).toBe(true);
    await page.unroute('**/leptos-api/preview_formulation*');
    await page.getByRole('button', { name: 'Retry calculation' }).click();
    await expect(page.getByTestId('estimate-stale')).toHaveText('');
    await expect.poll(() => waterValues(page)).toEqual(numbers);
    await expect(mash.locator('.k-stale--active')).toHaveCount(0);
  });
}

// KRA-49: the estimates summary is the fixed rail/stack above the sections. Its
// height must not change while an estimate is replaced or settles without one,
// or every control under it moves while a person is typing.
async function readyEstimateRecipe(page: import('@playwright/test').Page) {
  // A deliberately long profile name: the phone source line must stay one height
  // regardless of how long the effective equipment identity is.
  const profile = `Phone summary profile ${'x'.repeat(24)} ${Date.now()}`;
  const profileId = await recipeWithNumbers(page, profile);
  await page.locator('#recipe-attenuation').fill('75');
  await page.getByLabel('Beer style', { exact: true }).selectOption({ label: '4A · Munich Helles' });
  await expect(page.getByLabel(/Original gravity.*Style reference/)).toBeVisible();
  await expect(page.getByRole('status').filter({ hasText: 'Ready' }).first()).toBeVisible();
  // A saved Ready recipe is the realistic editing case: the first keystroke also
  // changes the estimate source line from "matching saved revision" to "changes
  // not yet in saved revision", and the summary height must not change with it.
  await page.getByLabel('Recipe name', { exact: true }).fill(`Summary ${Date.now()}`);
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  await expect(page.getByTestId('estimate-source')).toContainText('matching saved revision 1');
  await expect(page.getByRole('status').filter({ hasText: 'Ready' }).first()).toBeVisible();
  return profileId
}

type SummaryBox = { height: number; below: number };
// The summary's height and where the first section starts below it.
const measureSummary = (page: import('@playwright/test').Page) => page.evaluate(() => {
  const summary = document.querySelector('.recipe-editor__preview-summary')!;
  const below = document.querySelector('.recipe-editor__sections')!;
  const rect = summary.getBoundingClientRect();
  return { height: rect.height, below: below.getBoundingClientRect().top - rect.top };
});
const expectSame = (label: string, actual: SummaryBox, expected: SummaryBox) => {
  expect(Math.abs(actual.height - expected.height), `${label}: summary height ${actual.height} vs ${expected.height}`).toBeLessThanOrEqual(0.5);
  expect(Math.abs(actual.below - expected.below), `${label}: first control below moved to ${actual.below} vs ${expected.below}`).toBeLessThanOrEqual(0.5);
};

for (const width of [320, 390]) {
  test(`the estimates summary keeps one height from current through recalculating and a settled invalid or unavailable estimate at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    const profileId = await readyEstimateRecipe(page);
    const summary = page.locator('.recipe-editor__preview-summary');
    const values = summary.locator('.k-metric__value');
    const measure = () => measureSummary(page);
    const current = await measure();
    const status = summary.locator('[role="status"]');
    // The state text keeps one top edge in every state: the reserved space sits
    // below it, so "Ready" does not float and no outcome starts lower.
    const statusTop = async () => {
      const [summaryBox, statusBox] = await Promise.all([summary.boundingBox(), status.boundingBox()]);
      return statusBox!.y - summaryBox!.y;
    };
    const currentStatusTop = await statusTop();
    const staleRegion = summary.locator('.k-stale[aria-hidden="true"]');
    const disclosure = summary.locator('.k-disclosure--compact');
    const disclosureSummary = disclosure.locator('summary');
    // A long equipment name is hidden on phones; the header badge and equipment
    // panel still identify it, and the source line keeps its fixed two-line height.
    await expect(summary.locator('.recipe-editor__provenance-equipment')).toHaveCount(1);
    await expect(summary.locator('.recipe-editor__provenance-equipment')).toBeHidden();
    await expect(summary.locator('.k-metric')).toHaveCount(6);
    const currentValues = await values.allTextContents();
    expect(currentValues.some(text => /\d\.\d/.test(text)), 'a current estimate shows values').toBe(true);
    await expect(summary.getByRole('button', { name: 'Retry calculation' })).toHaveCount(0);

    // Hold every preview so the whole recalculation window is observable. The last
    // accepted values stay in place, dimmed and hidden from assistive technology.
    let release!: () => void;
    const held = new Promise<void>(resolve => { release = resolve; });
    await page.route('**/leptos-api/preview_formulation*', async route => { await held; await route.continue(); });
    await page.evaluate(() => {
      const samples: number[] = [];
      (window as any).__summarySamples = samples;
      const tick = () => {
        const summary = document.querySelector('.recipe-editor__preview-summary')!;
        const below = document.querySelector('.recipe-editor__sections')!;
        samples.push(Math.round((below.getBoundingClientRect().top - summary.getBoundingClientRect().top) * 100) / 100);
        requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    });
    const batch = page.getByLabel('Batch volume (L)');
    await batch.fill('21');
    await expect(page.getByTestId('estimate-stale')).toHaveText('Out of date, recalculating');
    // The first observable failure on the old behaviour: the summary loses the
    // whole estimate block the moment a replacement starts.
    expectSame('recalculating', await measure(), current);
    await expect(staleRegion, 'one stale region owns the metrics, source and reference').toHaveCount(1);
    expect(await staleRegion.evaluate(element => (element as HTMLElement).inert), 'stale region is inert').toBe(true);
    expect(await staleRegion.locator('.k-style-range').count(), 'the old style bands stay in the stale region').toBeGreaterThan(0);
    expect(await staleRegion.getByTestId('estimate-source').evaluate(element => getComputedStyle(element).fontStyle), 'stale source is marked').toBe('italic');
    expect(await disclosureSummary.evaluate(element => getComputedStyle(element).fontStyle), 'stale reference disclosure is marked').toBe('italic');
    expect(await staleRegion.locator('.k-style-range__limits').first().evaluate(element => getComputedStyle(element).fontStyle), 'stale band limits are marked').toBe('italic');
    expect(Number(await staleRegion.locator('.k-style-range__marker').first().evaluate(element => getComputedStyle(element).opacity)), 'stale band marker is dimmed').toBeLessThan(1);
    expect(await disclosureSummary.evaluate(element => { (element as HTMLElement).focus(); return document.activeElement === element; }), 'inert disclosure cannot take focus').toBe(false);
    expect(await values.allTextContents(), 'old values stay (dimmed) while recalculating, never dashes').toEqual(currentValues);
    await page.locator('#recipe-whirlpool').focus();
    await page.keyboard.type('5');
    await expect(page.getByTestId('estimate-stale')).toHaveText('Out of date, recalculating');
    expect(await values.allTextContents(), 'typing in a process field keeps the shown values').toEqual(currentValues);
    await page.waitForTimeout(400);
    expectSame('recalculating after typing in a process field', await measure(), current);

    release();
    await expect(page.getByTestId('estimate-stale')).toHaveText('');
    await expect.poll(async () => (await values.allTextContents()).some(text => /\d\.\d/.test(text))).toBe(true);
    await page.unroute('**/leptos-api/preview_formulation*');

    await batch.fill('-');
    await expect(page.getByTestId('estimate-stale')).toHaveText('Estimate unavailable until the inputs are valid');
    expectSame('settled invalid', await measure(), current);
    expect(Math.abs((await statusTop()) - currentStatusTop), 'invalid status text top').toBeLessThanOrEqual(0.5);
    await expect(staleRegion).toHaveCount(1);
    expect(await staleRegion.evaluate(element => (element as HTMLElement).inert)).toBe(true);
    expect(await staleRegion.locator('.k-style-range__limits').first().evaluate(element => getComputedStyle(element).fontStyle)).toBe('italic');
    expect(await staleRegion.locator('.k-style-range').count()).toBeGreaterThan(0);
    await expect(summary.locator('.k-metric')).toHaveCount(6);
    await expect.poll(async () => (await values.allTextContents()).every(text => text.startsWith('—'))).toBe(true);
    await expect(summary.getByRole('button', { name: 'Retry calculation' })).toHaveCount(0);
    const samples: number[] = await page.evaluate(() => (window as any).__summarySamples ?? []);
    expect(samples.length, 'frames sampled across the window').toBeGreaterThan(10);
    expect(Math.max(...samples) - Math.min(...samples), `first control below moved: ${[...new Set(samples)]}`).toBeLessThan(0.5);

    // A failed request settles without an estimate: the same height, dashes, and
    // one explicit retry target that is only in the tab order while unavailable.
    await page.route('**/leptos-api/preview_formulation*', route => route.abort());
    await batch.fill('23');
    await expect(page.getByTestId('estimate-stale')).toHaveText('Estimate unavailable');
    expectSame('settled unavailable', await measure(), current);
    expect(Math.abs((await statusTop()) - currentStatusTop), 'unavailable status text top').toBeLessThanOrEqual(0.5);
    await expect(staleRegion).toHaveCount(1);
    expect(await staleRegion.evaluate(element => (element as HTMLElement).inert)).toBe(true);
    await expect.poll(async () => (await values.allTextContents()).every(text => text.startsWith('—'))).toBe(true);
    const retry = summary.getByRole('button', { name: 'Retry calculation' });
    await expect(retry).toBeVisible();
    expect(await retry.getAttribute('title'), 'the retry has no extra hover tooltip').toBeNull();
    const retryBox = (await retry.boundingBox())!;
    expect(retryBox.width, 'retry touch target width').toBeGreaterThanOrEqual(44);
    expect(retryBox.height, 'retry touch target height').toBeGreaterThanOrEqual(44);
    const disclosureHandle = await disclosureSummary.elementHandle();
    await retry.focus();
    await expect(retry).toBeFocused();
    await page.keyboard.press('Tab');
    expect(await page.evaluate(element => document.activeElement !== element, disclosureHandle), 'Tab skips the inert disclosure').toBe(true);
    await retry.focus();
    await expect(retry).toBeFocused();
    await page.unroute('**/leptos-api/preview_formulation*');
    await page.keyboard.press('Enter');
    await expect(page.getByTestId('estimate-stale')).toHaveText('');
    await expect.poll(async () => (await values.allTextContents()).some(text => /\d\.\d/.test(text))).toBe(true);
    expectSame('current after retry', await measure(), current);
    await expect(summary.getByRole('button', { name: 'Retry calculation' })).toHaveCount(0);

    // The longest real settled message: an unresolvable equipment profile. Archive
    // the shared profile from another tab, then edit; the same outcome row must
    // hold the message without moving the estimate rows.
    const admin = await page.context().newPage();
    await admin.goto(`/equipment?profile=${profileId}`);
    await admin.getByRole('button', { name: 'Archive', exact: true }).click();
    await expect(admin.getByRole('status').filter({ hasText: 'Profile archived.' })).toBeVisible();
    await admin.close();
    await batch.fill('24');
    await expect(summary.getByRole('status')).toHaveText('Equipment profile unavailable. Use draft settings or select a valid profile.');
    expectSame('settled unavailable profile', await measure(), current);
    expect(Math.abs((await statusTop()) - currentStatusTop), 'unavailable profile status text top').toBeLessThanOrEqual(0.5);
    await expect(summary.getByRole('button', { name: 'Retry calculation' })).toHaveCount(0);
    await expect.poll(async () => (await values.allTextContents()).every(text => text.startsWith('—'))).toBe(true);
  });
}

// KRA-34 with KRA-49: the issues list lives inside the same summary. The
// estimate states change under a list of issues that is present throughout, and
// neither the list nor its "Out of date" cue may add or remove a row, or the
// controls below the summary move while a person is typing.
for (const width of [320, 390]) {
  test(`the estimates summary keeps one height with issues listed through current, recalculating, settled invalid and unavailable at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await readyEstimateRecipe(page);
    await addHop(page, 'Cascade', '60');
    const summary = page.locator('.recipe-editor__preview-summary');
    const list = issues(page);
    await expect(list.getByRole('button', { name: /Alpha acid is missing/ })).toHaveCount(1);
    await expect(page.getByTestId('estimate-stale')).toHaveText('');
    const measure = () => measureSummary(page);
    const current = await measure();
    const batch = page.getByLabel('Batch volume (L)');

    let release!: () => void;
    const held = new Promise<void>(resolve => { release = resolve; });
    await page.route('**/leptos-api/preview_formulation*', async route => { await held; await route.continue(); });
    await batch.fill('21');
    await expect(page.getByTestId('estimate-stale')).toHaveText('Out of date, recalculating');
    await expect(list.getByRole('button', { name: /Alpha acid is missing/ }), 'the issue stays listed while recalculating').toHaveCount(1);
    expectSame('recalculating with issues', await measure(), current);
    release();
    await expect(page.getByTestId('estimate-stale')).toHaveText('');
    await page.unroute('**/leptos-api/preview_formulation*');
    expectSame('current again with issues', await measure(), current);

    await batch.fill('-');
    await expect(page.getByTestId('estimate-stale')).toHaveText('Estimate unavailable until the inputs are valid');
    await expect(list.getByRole('button', { name: /Alpha acid is missing/ }), 'the issue stays listed when the draft settles invalid').toHaveCount(1);
    expectSame('settled invalid with issues', await measure(), current);

    await page.route('**/leptos-api/preview_formulation*', route => route.abort());
    await batch.fill('23');
    await expect(page.getByTestId('estimate-stale')).toHaveText('Estimate unavailable');
    await expect(list.getByRole('button', { name: /Alpha acid is missing/ }), 'the issue stays listed when the preview is unavailable').toHaveCount(1);
    expectSame('settled unavailable with issues', await measure(), current);
    await expect(summary.locator('.k-metric')).toHaveCount(6);
  });
}

// KRA-50 with UX-007: the preview context repeats the submitted batch volume even
// when the validator rejected it. That rejected input is not an estimate and must
// never appear beside the calculated figures in any summary state.
for (const width of [320, 390]) {
  test(`an invalid batch volume shows a dash for Into fermenter in every summary state at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await readyEstimateRecipe(page);
    const summary = page.locator('.recipe-editor__preview-summary');
    const metric = (label: string) => summary.locator('.k-metric').filter({ hasText: label }).locator('.k-metric__value');
    const fermenter = metric('Into fermenter');
    const status = summary.locator('[role="status"]');
    // Bitterness is 0.0 because this recipe has no hops, which is a truthful
    // estimate even when the batch is rejected (CALCULATIONS: no hops is zero IBU).
    const estimated = ['Original gravity', 'Final gravity', 'Alcohol', 'Colour'].map(metric);
    const allDashes = async () => {
      const texts = await Promise.all([...estimated, fermenter].map(locator => locator.textContent()));
      return texts.every(text => text?.startsWith('—'));
    };
    const batch = page.getByLabel('Batch volume (L)');

    // A valid batch is a real estimate and keeps its value.
    await expect(status).toHaveText('Ready');
    await expect(fermenter).toHaveText('20.0L');

    // Current: the accepted preview reports the negative batch as invalid, so its
    // context's -5 is not a figure. Every metric, including Into fermenter, is a dash.
    await batch.fill('-5');
    await expect(status).toHaveText(/^Invalid:/);
    expect(await allDashes(), '-5: a rejected batch volume is not shown as a current estimate').toBe(true);
    await expect(fermenter).toHaveText('—L');
    await expect(summary.getByRole('button', { name: 'Retry calculation' })).toHaveCount(0);

    // Recalculating: the retained invalid preview stays behind the same rule, so
    // another edit cannot bring its rejected -5 back beside the old figures.
    let release!: () => void;
    const held = new Promise<void>(resolve => { release = resolve; });
    await page.route('**/leptos-api/preview_formulation*', async route => { await held; await route.continue(); });
    await page.locator('#recipe-whirlpool').focus();
    await page.keyboard.type('5');
    await expect(page.getByTestId('estimate-stale')).toHaveText('Out of date, recalculating');
    expect(await allDashes(), 'recalculating: the retained rejected batch volume is not shown').toBe(true);
    await page.waitForTimeout(400);
    expect(await allDashes(), 'recalculating: still a dash after the debounce').toBe(true);
    release();
    await expect(page.getByTestId('estimate-stale')).toHaveText('');
    await expect(status).toHaveText(/^Invalid:/);
    await page.unroute('**/leptos-api/preview_formulation*');

    // Unavailable: while the draft still holds -5, a failed request settles
    // without an estimate and without the context value the draft still carries.
    await page.route('**/leptos-api/preview_formulation*', route => route.abort());
    await page.locator('#recipe-whirlpool').fill('6');
    await expect(page.getByTestId('estimate-stale')).toHaveText('Estimate unavailable');
    expect(await allDashes(), 'unavailable: a rejected batch volume is not shown').toBe(true);
    await page.unroute('**/leptos-api/preview_formulation*');

    // Zero is invalid by the same validator verdict.
    await batch.fill('0');
    await expect(status).toHaveText(/^Invalid:/);
    expect(await allDashes(), 'zero: the validator rejects the batch volume, so every figure is a dash').toBe(true);

    // Malformed never reaches the validator, so there is no estimate to show.
    await batch.fill('-');
    await expect(page.getByTestId('estimate-stale')).toHaveText('Estimate unavailable until the inputs are valid');
    expect(await allDashes(), 'malformed: no estimate, so every figure is a dash').toBe(true);

    // A valid batch restores the figure unchanged.
    await batch.fill('24');
    await expect(status).toHaveText('Ready');
    await expect(fermenter).toHaveText('24.0L');
  });
}

test('a late preview response cannot replace a settled invalid estimate', async ({ page }) => {
  test.slow();
  await page.setViewportSize({ width: 320, height: 900 });
  await readyEstimateRecipe(page);
  const summary = page.locator('.recipe-editor__preview-summary');
  const values = summary.locator('.k-metric__value');
  const height = () => summary.evaluate(element => element.getBoundingClientRect().height);
  const currentHeight = await height();
  // Fetch the response once, then deliver it when released: a slow response that
  // is still in flight while the person types an invalid value.
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  let fetched!: () => void;
  const responseFetched = new Promise<void>(resolve => { fetched = resolve; });
  await page.route('**/leptos-api/preview_formulation*', async route => {
    const response = await route.fetch();
    fetched();
    await held;
    await route.fulfill({ response });
  });
  const batch = page.getByLabel('Batch volume (L)');
  await batch.fill('21');
  await expect(page.getByTestId('estimate-stale')).toHaveText('Out of date, recalculating');
  await responseFetched;
  await batch.fill('-');
  await expect(page.getByTestId('estimate-stale')).toHaveText('Estimate unavailable until the inputs are valid');
  const status = summary.locator('[role="status"]');
  await expect(status).toHaveText('Complete invalid number fields to see estimates.');
  const settledStatus = await status.textContent();
  await expect.poll(async () => (await values.allTextContents()).every(text => text.startsWith('—'))).toBe(true);
  const dashes = await values.allTextContents();
  release();
  // The late response belongs to an older draft generation. It must not clear the
  // settled invalid outcome, restore the old numbers, report the old estimate as
  // Ready, or restart recalculating.
  await page.waitForTimeout(1000);
  await expect(page.getByTestId('estimate-stale')).toHaveText('Estimate unavailable until the inputs are valid');
  await expect(status).toHaveText(settledStatus!);
  expect(await values.allTextContents()).toEqual(dashes);
  expect(Math.abs((await height()) - currentHeight), 'late response changed the summary height').toBeLessThanOrEqual(0.5);
  await page.unroute('**/leptos-api/preview_formulation*');
});

test('an editor that has never had an estimate reserves the summary before the first one at 320px', async ({ page }) => {
  test.slow();
  await page.setViewportSize({ width: 320, height: 900 });
  // A saved Ready styled recipe: the restored draft holds only an invalid raw
  // value, so its first valid estimate is Ready (no diagnostics) and must fill
  // the reserved rows exactly.
  await readyEstimateRecipe(page);
  await page.getByLabel('Batch volume (L)').fill('-');
  await expect.poll(() => page.evaluate(() => Object.keys(localStorage).some(key => key.startsWith('krausen-draft-v1:')))).toBe(true);
  let unload = false;
  page.once('dialog', async dialog => { unload = dialog.type() === 'beforeunload'; await dialog.accept(); });
  await page.reload();
  expect(unload).toBe(true);
  const source = page.getByTestId('estimate-source');
  // The hidden editor behind the recovery panel has already accepted the saved
  // recipe's estimate; restoring the retained draft changes the draft epoch and
  // must drop that retained estimate.
  await expect(source).toContainText('matching saved revision 1');
  await page.getByRole('button', { name: 'Restore draft' }).click();
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue('-');
  await expect(page.getByLabel('Beer style', { exact: true })).toHaveValue('Munich Helles');
  await expect(source).toHaveText('No estimate yet.');
  const summary = page.locator('.recipe-editor__preview-summary');
  await expect(summary.locator('.k-style-range'), 'the style band rows are reserved').toHaveCount(5);
  const disclosure = summary.locator('.k-disclosure--compact');
  await expect(disclosure).toHaveCount(1);
  expect(await disclosure.locator('summary').evaluate(element => { (element as HTMLElement).focus(); return document.activeElement === element; }), 'the placeholder reference row is inert').toBe(false);
  const measure = () => page.evaluate(() => {
    const summary = document.querySelector('.recipe-editor__preview-summary')!;
    const below = document.querySelector('.recipe-editor__sections')!;
    const rect = summary.getBoundingClientRect();
    return { height: rect.height, below: below.getBoundingClientRect().top - rect.top };
  });
  const placeholder = await measure();
  await page.getByLabel('Batch volume (L)').fill('20');
  await expect(page.getByRole('status').filter({ hasText: 'Ready' }).first()).toBeVisible();
  await expect(source).not.toHaveText('No estimate yet.');
  await expect(summary.locator('.k-style-range')).toHaveCount(5);
  const first = await measure();
  expect(Math.abs(first.height - placeholder.height), `first estimate inserted ${first.height - placeholder.height}px`).toBeLessThanOrEqual(0.5);
  expect(Math.abs(first.below - placeholder.below), 'the first control below moved when the first estimate arrived').toBeLessThanOrEqual(0.5);
});

test('switching recipes never shows the previous recipe\'s estimate', async ({ page }) => {
  const stamp = Date.now();
  const [first, second] = [`Switch A ${stamp}`, `Switch B ${stamp}`];
  const save = async (title: string) => {
    await page.getByLabel('Recipe name', { exact: true }).fill(title);
    await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
    await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  };
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Batch volume (L)').fill('30');
  await save(second);
  await recipeWithWaterPlan(page);
  await save(first);
  await expect(page.locator('#recipe-section-mash').getByRole('heading', { name: 'Water plan' })).toBeVisible();
  await page.evaluate(() => {
    (window as any).__navigationMarker = true;
    const frames: number[] = [];
    (window as any).__planFrames = frames;
    const tick = () => {
      const section = document.getElementById('recipe-section-mash');
      frames.push(section ? [...section.querySelectorAll('h3')].filter(h => h.textContent === 'Water plan').length : -1);
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  });
  await page.getByRole('link', { name: 'Recipes', exact: true }).first().click();
  await page.getByRole('link', { name: `Open ${second}` }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(second);
  // The switch was client-side (the page was not reloaded), so a carried-over estimate would be visible.
  expect(await page.evaluate(() => (window as any).__navigationMarker), 'navigation reloaded the page').toBe(true);
  const frames: number[] = await page.evaluate(() => (window as any).__planFrames);
  // After the old editor left, the new mash section's first frame has no Water plan: nothing carried over.
  const arrival = frames.slice(frames.lastIndexOf(-1) + 1);
  expect(arrival.length).toBeGreaterThan(0);
  expect(arrival[0], 'previous recipe estimate shown on arrival').toBe(0);
});

// KRA-59 (UX-007): a restored draft is a different document from the saved recipe the hidden
// editor behind the recovery panel has already estimated. Neither the Water plan nor the
// chilling note under Boil & whirlpool may keep that recipe's numbers, in any estimate state.
async function reloadToRetainedDraft(page: import('@playwright/test').Page, edit: () => Promise<void>) {
  await recipeWithNumbers(page);
  await page.getByLabel('Recipe name', { exact: true }).fill(`Retained plan ${Date.now()}`);
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  await edit();
  await expect.poll(() => page.evaluate(() => Object.keys(localStorage).some(key => key.startsWith('krausen-draft-v1:')))).toBe(true);
  page.once('dialog', async dialog => { await dialog.accept(); });
  await page.reload();
  await expect(page.getByTestId('estimate-source')).toContainText('matching saved revision 1');
}

test('a restored draft with no estimate shows no water plan or chilling note from the saved recipe', async ({ page }) => {
  await reloadToRetainedDraft(page, () => page.getByLabel('Batch volume (L)').fill('-'));
  await expect(page.getByTestId('chilling-context')).toHaveCount(1);
  await page.getByRole('button', { name: 'Restore draft' }).click();
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue('-');
  await expect(page.getByTestId('estimate-source')).toHaveText('No estimate yet.');
  await expect(page.locator('#recipe-section-mash').getByRole('heading', { name: 'Water plan' })).toHaveCount(0);
  await expect(page.locator('#recipe-section-mash .k-metric-list')).toHaveCount(0);
  await expect(page.getByTestId('chilling-context')).toHaveCount(0);
});

test('a restored draft that is recalculating shows no water plan or chilling note from the saved recipe', async ({ page }) => {
  await reloadToRetainedDraft(page, () => page.locator('#recipe-mash-water').fill('12'));
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  await page.route('**/leptos-api/preview_formulation*', async route => { await held; await route.continue(); });
  try {
    await page.getByRole('button', { name: 'Restore draft' }).click();
    await expect(page.locator('#recipe-mash-water')).toHaveValue('12');
    await expect(page.getByTestId('estimate-source')).toHaveText('No estimate yet.');
    await expect(page.locator('#recipe-section-mash').getByRole('heading', { name: 'Water plan' })).toHaveCount(0);
    await expect(page.getByTestId('chilling-context')).toHaveCount(0);
    release();
    await expect(page.locator('#recipe-section-mash .k-metric').filter({ hasText: 'Mash water' })).toContainText('12.0');
    await expect(page.getByTestId('chilling-context')).toHaveCount(1);
  } finally { release(); await page.unrouteAll({ behavior: 'wait' }); }
});

// An in-place switch to a newer saved revision (replace_saved, no remount) is also another
// document: the old revision's Water plan and chilling note must not stay while the new estimate
// is calculated. Held with a promise the test releases, so nothing depends on timing.
test('a focus refetch to a newer saved revision shows no water plan or chilling note from the old revision', async ({ page }) => {
  await recipeWithNumbers(page);
  await page.getByLabel('Recipe name', { exact: true }).fill(`Refetch ${Date.now()}`);
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  await expect(page.getByTestId('chilling-context')).toHaveCount(1);
  const other = await page.context().newPage();
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  try {
    await other.goto(page.url());
    await other.locator('#recipe-mash-water').fill('12');
    await other.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
    await expect(other.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
    await page.route('**/leptos-api/preview_formulation*', async route => { await held; await route.continue(); });
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await expect(page.locator('#recipe-mash-water')).toHaveValue('12');
    await expect(page.getByTestId('estimate-source')).toHaveText('No estimate yet.');
    await expect(page.locator('#recipe-section-mash').getByRole('heading', { name: 'Water plan' })).toHaveCount(0);
    await expect(page.getByTestId('chilling-context')).toHaveCount(0);
    release();
    await expect(page.locator('#recipe-section-mash .k-metric').filter({ hasText: 'Mash water' })).toContainText('12.0');
    await expect(page.getByTestId('chilling-context')).toHaveCount(1);
  } finally { release(); await page.unrouteAll({ behavior: 'wait' }); await other.close(); }
});

// KRA-59 (ACC-005): the editor is hidden while access is checked or lost. An estimate episode
// that starts there is not something the person can see, so it is never announced, and access
// returning does not end it with "Estimate updated". The page clock is paused so the
// announcement timers (mirrored by ANNOUNCE_DEBOUNCE_MS and UPDATED_ANNOUNCE_MS) run only when
// the test advances them: no outcome depends on how fast the server or the machine is.
const UPDATED_ANNOUNCE_MS = 3000;
async function pauseAndRecordAnnouncements(page: import('@playwright/test').Page) {
  await page.clock.install();
  await page.clock.pauseAt(new Date(Date.now() + 1000));
  await page.evaluate(() => {
    const announced: string[] = [];
    (window as any).__announced = announced;
    const target = document.querySelector('[data-testid="estimate-announce"]')!;
    new MutationObserver(() => { if (target.textContent) announced.push(target.textContent); }).observe(target, { childList: true, characterData: true, subtree: true });
  });
}
const announced = (page: import('@playwright/test').Page) => page.evaluate(() => (window as any).__announced as string[]);

test('losing and regaining access announces nothing for an episode that started while hidden', async ({ page }) => {
  await recipeWithNumbers(page);
  await pauseAndRecordAnnouncements(page);
  const volume = page.getByLabel('Batch volume (L)');
  try {
    await page.route('**/leptos-api/get_user_context*', route => route.abort('failed'));
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await expect(volume).toBeHidden();
    // Past the announcement debounce: an episode running while hidden would be announced now.
    await page.clock.runFor(ANNOUNCE_DEBOUNCE_MS + 700);
    expect(await announced(page)).toEqual([]);
    await page.unroute('**/leptos-api/get_user_context*');
    await page.getByRole('button', { name: 'Retry check', exact: true }).click();
    await expect(volume).toBeVisible();
    // The new estimate is calculated after the preview's own 250 ms delay.
    await page.clock.runFor(300);
    await expect(page.getByTestId('estimate-stale')).toHaveText('');
    await expect(page.locator('#recipe-section-mash .k-metric').filter({ hasText: 'Mash water' })).toContainText('15.0');
    await page.clock.runFor(UPDATED_ANNOUNCE_MS + 500);
    expect(await announced(page)).toEqual([]);
    await expect(page.getByTestId('estimate-announce')).toHaveText('');
  } finally { await page.unrouteAll({ behavior: 'wait' }); }
});

// The debounce timer of an episode that was pending when access was lost must not fire later.
test('losing access while an out-of-date announcement is pending announces nothing', async ({ page }) => {
  await recipeWithNumbers(page);
  await pauseAndRecordAnnouncements(page);
  try {
    await page.locator('#recipe-whirlpool').fill('10');
    // The episode is running and its announcement is pending (the clock is paused) when access is lost.
    await expect(page.getByTestId('estimate-stale')).toHaveText('Out of date, recalculating');
    await page.route('**/leptos-api/get_user_context*', route => route.abort('failed'));
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await expect(page.getByLabel('Batch volume (L)')).toBeHidden();
    await page.clock.runFor(ANNOUNCE_DEBOUNCE_MS + 700);
    expect(await announced(page)).toEqual([]);
    await page.unroute('**/leptos-api/get_user_context*');
    await page.getByRole('button', { name: 'Retry check', exact: true }).click();
    await expect(page.getByLabel('Batch volume (L)')).toBeVisible();
    await page.clock.runFor(300);
    await expect(page.getByTestId('estimate-stale')).toHaveText('');
    await page.clock.runFor(UPDATED_ANNOUNCE_MS + 500);
    expect(await announced(page)).toEqual([]);
  } finally { await page.unrouteAll({ behavior: 'wait' }); }
});

test('the water plan emphasises its total and the process panels keep the Equipment titles unchanged', async ({ page }) => {
  await recipeWithWaterPlan(page);
  const rows = page.locator('#recipe-section-mash .k-metric-list .k-metric');
  const total = rows.filter({ hasText: 'Total water' });
  await expect(total).toHaveCSS('font-weight', '600');
  await expect(total).toHaveCSS('border-top-width', '1px');
  await expect(rows.filter({ hasText: 'Mash water' })).toHaveCSS('font-weight', '400');
  // Equipment is owned by another ticket: its section headings are the shared default, ours are smaller.
  await page.getByText('Equipment settings', { exact: true }).click();
  const equipmentHeading = page.locator('#recipe-equipment-disclosure h3').first();
  const processHeading = page.locator('#recipe-section-mash h3').first();
  const size = (heading: import('@playwright/test').Locator) => heading.evaluate(el => parseFloat(getComputedStyle(el).fontSize));
  expect(await size(equipmentHeading)).toBeGreaterThan(await size(processHeading));
  expect(await size(equipmentHeading)).toBeCloseTo(await page.locator('#recipe-section-mash h2').evaluate(el => parseFloat(getComputedStyle(el).fontSize)), 1);
});

test('the chilling note states the effective chilling and the model limitation', async ({ page }) => {
  const profile = `Natural ${Date.now()}`;
  await createEquipmentProfile(page, profile, PROFILE_VALUES, 'natural');
  const title = `Chilling ${Date.now()}`;
  const xml = `<RECIPES><RECIPE><NAME>${title}</NAME><VERSION>1</VERSION><BATCH_SIZE>20</BATCH_SIZE><BOIL_TIME>60</BOIL_TIME><FERMENTABLES><FERMENTABLE><NAME>Pale malt</NAME><VERSION>1</VERSION><TYPE>Grain</TYPE><AMOUNT>4</AMOUNT><YIELD>80</YIELD><COLOR>3</COLOR></FERMENTABLE></FERMENTABLES><HOPS><HOP><NAME>Saaz</NAME><VERSION>1</VERSION><ALPHA>4</ALPHA><AMOUNT>0.02</AMOUNT><USE>Boil</USE><TIME>60</TIME></HOP></HOPS></RECIPE></RECIPES>`;
  await page.goto('/beerxml');
  await page.getByLabel('Paste BeerXML').fill(xml);
  await page.locator('#beerxml-colour').selectOption('srm');
  await page.locator('#beerxml-efficiency').selectOption('override');
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await page.getByRole('button', { name: 'Import recipe' }).click();
  await page.getByRole('link', { name: 'Open imported recipe' }).click();
  const note = page.getByTestId('chilling-context');
  const limitation = 'a hop first added after the wort has cooled is not modelled';
  // No equipment chilling choice and a hop addition: bitterness needs a choice.
  await expect(note).toContainText('Effective chilling: not specified. A bitterness estimate needs a chilling choice in Equipment.');
  await expect(note).toContainText(limitation);
  await page.getByText('Equipment settings', { exact: true }).click();
  await page.locator('#equipment-profile-select').selectOption({ label: profile });
  await expect(note).toContainText('Effective chilling: natural cooling after flameout.');
  await expect(note).toContainText(limitation);
  await expect(note).not.toContainText('not specified');
  // While the new estimate is in flight the note (read from the old estimate) is dimmed and hidden from
  // assistive technology like the Water plan, in place so nothing moves, and is restored when accepted.
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  await page.route('**/leptos-api/preview_formulation*', async route => { await held; await route.continue(); });
  await page.locator('#equipment-profile-select').selectOption('');
  await expect(page.getByTestId('estimate-stale')).toHaveText('Out of date, recalculating');
  await expect(page.locator('.k-stale--active[aria-hidden="true"]').getByTestId('chilling-context')).toContainText('natural cooling after flameout');
  release();
  await page.unroute('**/leptos-api/preview_formulation*');
  await expect(page.getByTestId('estimate-stale')).toHaveText('');
  await expect(note).toContainText('Effective chilling: not specified.');
  await expect(page.locator('.k-stale--active').getByTestId('chilling-context')).toHaveCount(0);
  await expect(page.locator('.k-stale[aria-hidden="true"]').getByTestId('chilling-context')).toHaveCount(0);
  await page.locator('#equipment-profile-select').selectOption({ label: profile });
  await expect(note).toContainText('Effective chilling: natural cooling after flameout.');
  // When the estimate becomes unavailable the note is not left describing the old equipment as current.
  await page.route('**/leptos-api/preview_formulation*', route => route.abort());
  await page.locator('#equipment-profile-select').selectOption('');
  await expect(page.getByTestId('estimate-stale')).toHaveText('Estimate unavailable');
  await expect(note).toHaveCount(0);
  await page.unroute('**/leptos-api/preview_formulation*');
  await page.getByRole('button', { name: 'Retry calculation' }).click();
  await expect(note).toContainText('Effective chilling: not specified.');
});

test('the process kit example shows the step row, stale outcomes and out-of-flow live line', async ({ page }) => {
  await page.goto('/design-system');
  const step = page.getByRole('group', { name: 'Mash step 1 of 1, Saccharification' });
  await expect(step).toBeVisible();
  await expect(step.getByLabel('Step name', { exact: true })).toHaveValue('Saccharification');
  await expect(page.getByTestId('estimate-stale')).toHaveText('');
  for (const [button, cue] of [['In flight', 'Out of date, recalculating'], ['Invalid input', 'Estimate unavailable until the inputs are valid'], ['Unavailable', 'Estimate unavailable']] as const) {
    await page.getByRole('button', { name: button, exact: true }).click();
    await expect(page.getByTestId('estimate-stale')).toHaveText(cue);
    await expect(page.getByTestId('estimate-announce')).toHaveText(cue);
    await expect(page.locator('.k-stale[aria-hidden="true"]')).toHaveCount(1);
  }
  await page.getByRole('button', { name: 'Current estimate', exact: true }).click();
  await expect(page.getByTestId('estimate-stale')).toHaveText('');
  const live = page.getByTestId('showcase-live');
  const marker = page.getByRole('button', { name: 'Current estimate', exact: true });
  // Distance from the group heading above the live line to a control below it (scroll-independent).
  const heading = page.getByRole('heading', { name: 'Mash steps', exact: true }).last();
  const gap = async () => (await marker.boundingBox())!.y - (await heading.boundingBox())!.y;
  const before = await gap();
  await step.getByRole('button', { name: /^Remove step/ }).click();
  await expect(live).toHaveText('Example removal requested; demonstration row retained.');
  await page.getByRole('button', { name: 'Add step', exact: true }).click();
  await expect(live).toHaveText('Added Mash step 2 of 2.');
  await page.getByRole('button', { name: /^Undo removal/ }).click();
  await expect(live).toHaveText('Restored Mash step 1 of 1.');
  expect(await gap(), 'announcing does not move following content').toBe(before);
});

test('unsupported imported process details are named and kept, never silently dropped', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  await expect(page.getByTestId('unsupported-process')).toHaveCount(0);
  // A legacy recipe with yeast and a mash block maps to preserved-but-unmodelled process data.
  const title = `Legacy process ${Date.now()}`;
  const xml = `<RECIPES><RECIPE><NAME>${title}</NAME><VERSION>1</VERSION><BATCH_SIZE>20</BATCH_SIZE><BOIL_TIME>60</BOIL_TIME><FERMENTABLES><FERMENTABLE><NAME>Pale malt</NAME><VERSION>1</VERSION><TYPE>Grain</TYPE><AMOUNT>4</AMOUNT><YIELD>80</YIELD><COLOR>3</COLOR></FERMENTABLE></FERMENTABLES><YEASTS><YEAST><NAME>Lager yeast</NAME><VERSION>1</VERSION><TYPE>Lager</TYPE><FORM>Dry</FORM><AMOUNT>0.011</AMOUNT></YEAST></YEASTS></RECIPE></RECIPES>`;
  await page.goto('/beerxml');
  await page.getByLabel('Paste BeerXML').fill(xml);
  await page.locator('#beerxml-colour').selectOption('srm');
  await page.locator('#beerxml-efficiency').selectOption('override');
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Import preview' })).toContainText(title);
  await page.getByRole('button', { name: 'Import recipe' }).click();
  await page.getByRole('link', { name: 'Open imported recipe' }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(title);
  await expect(page.getByTestId('unsupported-process')).toHaveText('Imported process details that Krausen does not model are kept with this recipe and do not affect estimates.');
});

test('wort attenuation is independent of the informational yeast attenuation', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  const yeast = page.locator('.k-panel').filter({ has: page.getByRole('heading', { name: 'Yeast', exact: true }) });
  await yeast.getByRole('button', { name: 'Add yeast', exact: true }).click();
  await page.getByRole('dialog', { name: 'Add yeast', exact: true }).getByRole('button', { name: 'Add to recipe', exact: true }).click();
  await yeast.getByText('Addition details', { exact: true }).click();
  const informational = yeast.getByLabel('Yeast attenuation (informational) (%)', { exact: true });
  const effective = page.getByLabel('Wort attenuation (%)');
  await informational.fill('82');
  await expect(effective).toHaveValue('');
  await effective.fill('70');
  await expect(informational).toHaveValue('82');
  await page.getByLabel('Recipe name', { exact: true }).fill(`Attenuation ${Date.now()}`);
  await page.getByLabel('Batch volume (L)').fill('20');
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  await page.reload();
  await expect(page.getByLabel('Wort attenuation (%)')).toHaveValue('70');
  await yeast.getByText('Addition details', { exact: true }).click();
  await expect(yeast.getByLabel('Yeast attenuation (informational) (%)', { exact: true })).toHaveValue('82');
  await expect(page.locator('#recipe-section-fermentation').getByText('never copied here', { exact: false })).toBeVisible();
});

for (const width of [1440, 1280, 1100, 900, 601, 390, 320]) {
  test(`process sections stay reachable and aligned at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto('/recipe-editor/new');
    await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
    for (const section of ['mash', 'boil', 'fermentation', 'notes']) {
      const region = page.locator(`#recipe-section-${section}`);
      await region.scrollIntoViewIfNeeded();
      await expect(region).toBeVisible();
    }
    const mash = page.locator('#recipe-section-mash');
    await mash.getByRole('button', { name: 'Add step', exact: true }).click();
    const step = mash.getByRole('group', { name: /^Mash step 1 of 1/ });
    const nameInput = step.getByLabel('Step name', { exact: true });
    await nameInput.fill('Saccharification');
    const [time, temperature, volume] = ['Time (min)', 'Temperature (°C)', 'Volume (L)'].map(label => step.getByLabel(label, { exact: true }));
    const boxes = await Promise.all([time, temperature, volume].map(input => input.boundingBox()));
    for (const box of boxes) {
      expect(box!.x).toBeGreaterThanOrEqual(0);
      expect(box!.x + box!.width).toBeLessThanOrEqual(width);
    }
    // Inputs sit level whatever the labels wrap to.
    expect(new Set(boxes.map(box => Math.round(box!.y))).size, `input tops ${boxes.map(box => box!.y)}`).toBe(1);
    const remove = step.getByRole('button', { name: /^Remove step/ });
    const add = mash.getByRole('button', { name: 'Add step', exact: true });
    const nameBox = (await nameInput.boundingBox())!;
    const removeBox = (await remove.boundingBox())!;
    // Remove follows the name at every width (DOM order = visual order).
    expect(removeBox.x, 'Remove is after the name').toBeGreaterThan(nameBox.x + nameBox.width - 1);
    expect(Math.abs(removeBox.y + removeBox.height / 2 - (nameBox.y + nameBox.height / 2)), 'Remove sits on the name row').toBeLessThanOrEqual(nameBox.height);
    const groupWidth = await mash.locator('.k-step-group').evaluate(el => el.getBoundingClientRect().width);
    const wide = groupWidth >= 640;
    test.info().annotations.push({ type: 'layout', description: `${width}px viewport: step group ${Math.round(groupWidth)}px -> ${wide ? 'one row' : 'compact rows'}` });
    // The layout is decided by the group's own width (container query), so it is pinned per viewport: one row from 1280px, compact rows below, including the mid width where the page sidebar leaves a narrow group.
    expect(wide, `${width}px viewport: step group ${Math.round(groupWidth)}px`).toBe(width >= 1280);
    if (wide) {
      expect(removeBox.x + removeBox.width, 'one row: measurements follow Remove').toBeLessThanOrEqual(boxes[0]!.x + 1);
      expect(Math.abs(boxes[0]!.y - nameBox.y), 'one row').toBeLessThanOrEqual(2);
    } else {
      expect(boxes[0]!.y, 'measurements are on the second row').toBeGreaterThan(nameBox.y + nameBox.height - 1);
    }
    if (width <= 600) {
      const targets = [time, temperature, volume, nameInput, remove, add];
      for (const target of targets) expect((await target.boundingBox())!.height, 'touch target height').toBeGreaterThanOrEqual(44);
      await remove.click();
      expect((await mash.getByRole('button', { name: /^Undo removal of/ }).boundingBox())!.height).toBeGreaterThanOrEqual(44);
    }
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  });
}

for (const width of [1440, 390]) {
  test(`keyboard order follows visual order at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto('/recipe-editor/new');
    await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
    const mash = page.locator('#recipe-section-mash');
    await mash.getByRole('button', { name: 'Add step', exact: true }).click();
    const step = mash.getByRole('group', { name: /^Mash step 1 of 1/ });
    await expect(step.getByLabel('Step name', { exact: true })).toBeFocused();
    const order: string[] = [];
    const points: { top: number; left: number }[] = [];
    for (let i = 0; i < 5; i++) {
      const info = await page.evaluate(() => { const e = document.activeElement as HTMLElement; const r = e.getBoundingClientRect(); return { label: e.getAttribute('aria-label') ?? (e as HTMLInputElement).labels?.[0]?.textContent ?? e.textContent ?? '', top: r.top, left: r.left }; });
      order.push(info.label.trim()); points.push({ top: info.top, left: info.left });
      await page.keyboard.press('Tab');
    }
    expect(order.map(label => label.split(',')[0].replace(/\s*\(.*\)/, ''))).toEqual(['Step name', 'Remove step', 'Time', 'Temperature', 'Volume']);
    // Reading order (rows top to bottom, then left to right) is the Tab order.
    for (let i = 1; i < points.length; i++) {
      const sameRow = Math.abs(points[i].top - points[i - 1].top) < 12;
      if (sameRow) expect(points[i].left, `focus moved left at step ${i}`).toBeGreaterThan(points[i - 1].left);
      else expect(points[i].top, `focus moved up at step ${i}`).toBeGreaterThan(points[i - 1].top);
    }
  });
}

test('shared preview exposes style ranges and rejects an older response', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  const style = page.getByLabel('Beer style');
  await expect(style.getByRole('option', { name: '4A · Munich Helles' })).toBeAttached();
  await style.selectOption({ label: '4A · Munich Helles' });
  await expect(page.getByLabel(/Original gravity.*Style reference/)).toBeVisible();

  let firstStarted!: () => void;
  const started = new Promise<void>(resolve => { firstStarted = resolve; });
  let delayed = false;
  await page.route('**/leptos-api/preview_formulation*', async route => {
    const response = await route.fetch();
    if (!delayed) {
      delayed = true;
      firstStarted();
      await new Promise(resolve => setTimeout(resolve, 1200));
    }
    await route.fulfill({ response });
  });
  const volume = page.getByLabel('Batch volume (L)');
  await volume.fill('-1');
  await started;
  await volume.fill('20');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  await page.waitForTimeout(1500);
  await expect(page.getByRole('status').filter({ hasText: 'Invalid' })).toHaveCount(0);
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
});

test('editor controls remain usable on mobile and in dark mode', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  setAccountTheme('dark');
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('heading', { name: 'Recipe editor' })).toBeVisible();
  await expect(page.locator('html')).toHaveClass(/dark/);
  await page.getByRole('button', { name: 'Add fermentable' }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill' });
  await expect(picker).toBeVisible();
  await expect(page.getByLabel('Search fermentables')).toBeVisible();
  await picker.locator('.k-catalogue-item').first().click();
  await picker.getByRole('button', { name: 'Add to recipe', exact: true }).click();
  await expect(picker).toBeHidden();
  await expect(page.getByLabel('Name', { exact: true })).toBeVisible();
  await page.getByText('Addition details', { exact: true }).click();
  await expect(page.getByLabel('Type', { exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.getByRole('button', { name: 'Toggle menu' }).click();
  await expect(page.getByRole('dialog', { name: 'Navigation' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: 'Toggle menu' })).toBeFocused();
});

test('late editor load never steals a deliberate focus move', async ({ page }) => {
  const title = `Late load ${Date.now()}`;
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(title);
  await page.getByLabel('Batch volume (L)').fill('20');
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: /Saved revision \d+/ })).toBeVisible();
  await page.goto('/recipes');
  await page.getByLabel('Search recipes').fill(title);
  const open = page.getByRole('link', { name: `Open ${title}` });
  await expect(open).toBeVisible();
  // Hold the editor's data so the heading mounts well inside the arrival window.
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  await page.route('**/leptos-api/read_recipe*', async route => { await held; await route.continue(); });
  await open.click();
  const skip = page.getByRole('link', { name: 'Skip to main content' });
  await skip.focus();
  release();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText(title);
  await expect(skip, 'arrival focus must not override a deliberate focus move').toBeFocused();
});

test('a focused recipe heading is visible and not clipped', async ({ page }) => {
  const title = `Arrival ${Date.now()}`;
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(title);
  await page.getByLabel('Batch volume (L)').fill('20');
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: /Saved revision \d+/ })).toBeVisible();
  await page.goto('/recipes');
  await page.getByLabel('Search recipes').fill(title);
  const open = page.getByRole('link', { name: `Open ${title}` });
  await expect(open).toBeVisible();
  await open.focus();
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/\/recipe-editor\/[0-9a-f-]+/);
  const heading = page.getByRole('heading', { level: 1 });
  await expect(heading).toHaveText(title);
  // The shared successor paths (catalogue return, cleared print task) focus this
  // heading; whenever it carries focus it must reveal itself rather than stay
  // clipped. Arrival focus on this asynchronously mounted page is the shared
  // layout's contract (KRA-48) and is not what this test asserts.
  await heading.focus();
  await expect(heading).toBeFocused();
  const visible = await heading.evaluate(element => {
    const rect = element.getBoundingClientRect();
    const style = getComputedStyle(element);
    return rect.width > 1 && rect.height > 1 && style.clipPath !== 'inset(50%)' && style.visibility !== 'hidden';
  });
  expect(visible, 'focused heading must not stay clipped').toBe(true);
});

test('recipe header stays usable under browser page zoom and enlarged text', async ({ page, browserName }, testInfo) => {
  test.skip(browserName !== 'chromium', 'Page-scale emulation is Chromium-only');
  await testInfo.attach('zoom-evidence-limits', { contentType: 'application/json', body: JSON.stringify({
    browser: page.context().browser()?.version(), hostPlatform: process.platform,
    scope: 'Chromium page-scale (pinch/zoom) emulation and CSS text enlargement. This is not an operating-system screen-reader session; native AT is recorded separately as a limitation.',
  }) });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(`Zoom ${Date.now()}`);
  const client = await page.context().newCDPSession(page);
  for (const factor of [2, 4]) {
    await client.send('Emulation.setPageScaleFactor', { pageScaleFactor: factor });
    const save = page.getByRole('button', { name: /^Save (recipe|changes)$/ });
    await expect(save).toBeVisible();
    // Page scale shrinks the visual viewport; the control must stay reachable
    // and the layout must not gain horizontal document overflow.
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await save.scrollIntoViewIfNeeded();
    await expect(save).toBeInViewport();
    await client.send('Emulation.setPageScaleFactor', { pageScaleFactor: 1 });
    await save.click();
    await expect(page.getByRole('status').filter({ hasText: /Saved revision \d+/ })).toBeVisible();
  }
  // Text-only enlargement (root 15px → 30px/60px) is the WCAG resize-text path.
  for (const root of ['30px', '60px']) {
    await page.evaluate(size => { document.documentElement.style.fontSize = size; }, root);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await expect(page.getByRole('button', { name: /^Save (recipe|changes)$/ })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Start brew' }).first()).toBeVisible();
  }
});

test('editor state controls keep raw labels and focus across representative presentation settings', async ({ page }, testInfo) => {
  await testInfo.attach('browser-and-evidence-limits', { contentType: 'application/json', body: JSON.stringify({
    browser: page.context().browser()?.version(), hostPlatform: process.platform,
    scope: 'Headless desktop browser with viewport/theme/media/controlled-font settings; not native AT, OS zoom or physical device certification.',
  }) });
  for (const setting of [
    { width: 1440, height: 900, theme: 'light', fallback: false },
    { width: 390, height: 844, theme: 'light', fallback: false },
    { width: 320, height: 700, theme: 'dark', fallback: true },
  ]) {
    await page.setViewportSize({ width: setting.width, height: setting.height });
    await page.emulateMedia({ reducedMotion: setting.fallback ? 'reduce' : 'no-preference' });
    setAccountTheme(setting.theme as 'light' | 'dark');
    await page.goto('/recipe-editor/new');
    await expect(page.locator('html')).toHaveClass(setting.theme === 'dark' ? /\bdark\b/ : /^(?!.*\bdark\b).*$/);
    await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
    if (setting.fallback) {
      // Controlled browser-font fallback, not an OS zoom/device/AT claim.
      await page.addStyleTag({ content: 'input, textarea, button { font-family: Arial, sans-serif !important; }' });
    }
    const volume = page.getByLabel('Batch volume (L)');
    await volume.fill('-');
    await expect(volume).toHaveAttribute('aria-invalid', 'false');
    await volume.press('Tab');
    await expect(volume).toHaveAttribute('aria-invalid', 'true');
    await expect(page.locator('#recipe-batch-description')).toContainText('Enter a number');
    await volume.fill('20.000000000000000');
    await volume.press('Tab');
    await volume.focus();
    await expect(volume).toHaveValue('20.000000000000000');
    await expect(volume).toBeFocused();
    const bounds = await volume.boundingBox();
    expect(bounds!.x).toBeGreaterThanOrEqual(0);
    expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(setting.width);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await testInfo.attach(`editor-state-${setting.width}-${setting.theme}`, {
      body: await page.screenshot({ fullPage: true }), contentType: 'image/png',
    });
    await page.getByRole('link', { name: 'Recipes', exact: true }).first().click();
    await page.getByRole('dialog', { name: 'Unsaved recipe changes' })
      .getByRole('button', { name: 'Discard and leave', exact: true }).click();
    await expect(page).toHaveURL(/\/recipes$/);
  }
});

test('showcase kit supports themes, screen sizes and keyboard dialogs', async ({ page }) => {
  for (const theme of ['light', 'dark']) {
    for (const width of [1440, 390]) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto('/design-system');
      await expect(page.getByRole('heading', { name: 'Good design, on repeat.' })).toBeVisible();
      // Each test starts from Light (beforeEach). Exercise the real appearance
      // control and assert the actual theme.
      if (await page.locator('html').evaluate(element => element.classList.contains('dark')) !== (theme === 'dark')) {
        // Appearance is chosen in Profile; use the desktop shell for it.
        const restore = page.viewportSize()!;
        await page.setViewportSize({ width: 1440, height: 900 });
        await setAppearance(page, theme as 'light' | 'dark');
        await page.setViewportSize(restore);
      }
      await expect(page.locator('html')).toHaveClass(theme === 'dark' ? /\bdark\b/ : /^(?!.*\bdark\b).*$/);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      await expect(page.getByLabel('Amount (kg)', { exact: true }).and(page.locator('#show-invalid'))).toHaveAttribute('aria-invalid', 'true');
      await page.getByRole('tab', { name: 'Recipe' }).focus();
      await page.keyboard.press('ArrowRight');
      await expect(page.getByRole('tab', { name: 'Saved versions' })).toBeFocused();
      const opener = page.getByRole('button', { name: 'Open dialog' });
      await opener.click();
      const dialog = page.getByRole('dialog', { name: 'Keep your changes?' });
      await expect(dialog).toBeVisible();
      await page.keyboard.press('Escape');
      await expect(dialog).toBeHidden();
      await expect(opener).toBeFocused();
      await opener.click();
      await dialog.getByRole('button', { name: 'Close' }).click();
      await opener.click();
      await page.waitForTimeout(100);
      await expect(dialog).toBeVisible();
      await page.keyboard.press('Escape');
    }
  }
});

test('shared kit native popovers preserve ordinary focus and raw field states', async ({ page }) => {
  await page.goto('/design-system');
  const opener = page.getByRole('button', { name: 'Recipe actions', exact: true });
  const popup = page.locator('#showcase-actions');
  await opener.focus();
  await page.keyboard.press('Enter');
  await expect(popup).toBeVisible();
  await expect(opener).toHaveAttribute('aria-expanded', 'true');
  expect(await popup.evaluate(element => element.matches(':popover-open'))).toBe(true);
  await page.keyboard.press('Tab');
  await expect(popup.getByRole('link', { name: 'View source context' })).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(opener).toBeFocused();
  await page.keyboard.press('Tab');
  await page.keyboard.press('Tab');
  await expect(popup.getByRole('button', { name: 'Use selected source' })).toBeFocused();
  await page.keyboard.press('Tab');
  expect(await popup.evaluate(element => element.contains(document.activeElement))).toBe(false);
  await popup.getByRole('link').focus();
  await page.keyboard.press('Escape');
  await expect(popup).toBeHidden();
  await expect(opener).toBeFocused();
  await page.keyboard.press('Space');
  await expect(popup).toBeVisible();
  await page.getByRole('heading', { name: 'After actions' }).click();
  await expect(popup).toBeHidden();
  await expect(opener).toHaveAttribute('aria-expanded', 'false');
  await expect(page.getByText('Example action selected', { exact: true })).toHaveCount(0);

  const raw = page.getByLabel('Raw quantity (kg)', { exact: true });
  await raw.fill('1e-');
  await raw.evaluate((element: HTMLInputElement) => element.setSelectionRange(1, 2));
  const identity = await raw.evaluate(element => { (window as any).__kra44RawField = element; return element.id; });
  await raw.blur();
  await expect(raw).toHaveValue('1e-');
  await expect(raw).toHaveAttribute('aria-invalid', 'true');
  await expect(raw).toHaveAttribute('aria-describedby', `${identity}-description`);
  await raw.focus();
  expect(await raw.evaluate(element => element === (window as any).__kra44RawField)).toBe(true);
  expect(await raw.evaluate((element: HTMLInputElement) => [element.selectionStart, element.selectionEnd])).toEqual([1, 2]);
  await expect(page.getByLabel('Read-only source amount (kg)', { exact: true })).toHaveAttribute('readonly', '');
  await expect(page.getByLabel('Unavailable amount (kg)', { exact: true })).toBeDisabled();
  await expect(page.getByText('Add a batch volume to see the affected estimates.', { exact: true }).locator('..').locator('..')).not.toHaveAttribute('role', 'status');
});

test('shared kit native popovers remain usable inside a native modal and viewport', async ({ page }) => {
  for (const width of [320, 390, 1440]) {
    await page.setViewportSize({ width, height: 560 });
    await page.goto('/design-system');
    await page.getByRole('button', { name: 'Open Navigation example', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Keep your changes?' });
    const opener = dialog.getByRole('button', { name: 'Account actions example' });
    await opener.click();
    const popup = page.locator('#showcase-nested-account');
    await expect(popup).toBeVisible();
    expect(await popup.evaluate(element => !!element.closest('dialog[open]') && element.matches(':popover-open'))).toBe(true);
    await page.keyboard.press('Tab');
    await expect(popup.getByRole('link', { name: 'Profile example' })).toBeFocused();
    const bounds = await popup.boundingBox();
    expect(bounds).not.toBeNull();
    expect(bounds!.x).toBeGreaterThanOrEqual(0);
    expect(bounds!.y).toBeGreaterThanOrEqual(0);
    expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(width);
    expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(560);
    const last = popup.getByRole('button', { name: 'Workspace example 30', exact: true });
    await last.focus();
    // Capture scroll and ResizeObserver work must settle before asserting that
    // the focused action is still inside the native popup's scrollport.
    await page.evaluate(async () => {
      for (let frame = 0; frame < 6; frame++) await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
    });
    await expect(last).toBeFocused();
    const target = await last.boundingBox();
    expect(target!.height).toBeGreaterThanOrEqual(44);
    const settled = await popup.boundingBox();
    const focusOutset = await last.evaluate(element => {
      const style = getComputedStyle(element);
      return style.outlineStyle === 'none' ? 0 : parseFloat(style.outlineWidth) + Math.max(0, parseFloat(style.outlineOffset));
    });
    expect(target!.y - focusOutset).toBeGreaterThanOrEqual(settled!.y);
    expect(target!.y + target!.height + focusOutset).toBeLessThanOrEqual(settled!.y + settled!.height);
    expect(target!.x - focusOutset).toBeGreaterThanOrEqual(settled!.x);
    expect(target!.x + target!.width + focusOutset).toBeLessThanOrEqual(settled!.x + settled!.width);
    await page.keyboard.press('Escape');
    await expect(popup).toBeHidden();
    await expect(dialog).toBeVisible();
    await expect(opener).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
  }
});

test('shared kit native popovers expose the actual schedule variant and pending action', async ({ page }) => {
  for (const width of [320, 390, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    await page.goto('/design-system');
    const minutes = page.getByLabel('Contact time (min)', { exact: true });
    const row = minutes.locator('xpath=ancestor::*[contains(concat(" ",normalize-space(@class)," ")," k-compact-ingredient ")]');
    await expect(minutes).toBeVisible();
    expect(await minutes.evaluate(element => element.closest('details'))).toBeNull();
    await minutes.fill('60.125');
    await expect(minutes).toHaveValue('60.125');
    const stage = page.getByLabel('Hop stage', { exact: true });
    await stage.selectOption('whirlpool');
    await expect(stage).toHaveValue('whirlpool');
    const remove = row.getByRole('button', { name: /^Remove Hallertauer/ });
    await expect(remove).toBeVisible();
    const rowBounds = await row.boundingBox();
    const target = await remove.boundingBox();
    expect(rowBounds!.x).toBeGreaterThanOrEqual(0);
    expect(rowBounds!.x + rowBounds!.width).toBeLessThanOrEqual(width);
    expect(target!.x + target!.width).toBeLessThanOrEqual(width);
    if (width < 760) expect(target!.height).toBeGreaterThanOrEqual(44);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    const pending = page.getByRole('button', { name: 'Save changes', exact: true });
    await expect(pending).toBeDisabled();
    await expect(pending).toHaveAttribute('aria-busy', 'true');
  }
});

test('shared kit native popovers return to a safe successor when the opener is unavailable', async ({ page }, testInfo) => {
  for (const unavailable of ['hidden', 'disabled', 'inert']) {
    await page.goto('/design-system');
    const opener = page.getByRole('button', { name: 'Recipe actions', exact: true });
    await opener.click();
    await page.locator('#showcase-actions').getByRole('link').focus();
    // Controlled native DOM fault: the real mounted opener becomes unavailable.
    // This is not a claim that a production Navigation breakpoint was tested.
    await opener.evaluate((element, attribute) => element.setAttribute(attribute, ''), unavailable);
    await page.keyboard.press('Escape');
    await expect(page.locator('#showcase-actions')).toBeHidden();
    await testInfo.attach(`focus-after-${unavailable}`, { body: JSON.stringify(await page.evaluate(() => ({ active: document.activeElement?.outerHTML, successor: document.querySelector('#showcase-popover-successor')?.outerHTML }))), contentType: 'application/json' });
    await expect(page.locator('#showcase-popover-successor')).toBeFocused();
  }
  await page.goto('/design-system');
  await page.getByRole('button', { name: 'Temporary actions example', exact: true }).click();
  await page.getByRole('button', { name: 'Finish temporary example', exact: true }).click();
  await expect(page.locator('#showcase-temporary-actions')).toHaveCount(0);
  await expect(page.locator('#showcase-popover-successor')).toBeFocused();
});

test('shared kit native popovers refresh geometry after content grows while open', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 700 });
  await page.goto('/design-system');
  await page.getByRole('button', { name: 'Changing actions example', exact: true }).click();
  const popup = page.locator('#showcase-changing-actions');
  await expect(popup).toBeVisible();
  const initial = await popup.boundingBox();
  await popup.getByRole('button', { name: 'Load example memberships' }).click();
  const last = popup.getByRole('button', { name: 'Loaded membership 20', exact: true });
  await expect(last).toBeAttached();
  await testInfo.attach('content-growth-bounds', { body: JSON.stringify({ initial, after: await popup.boundingBox(), content: await popup.evaluate(element => ({ clientHeight: element.clientHeight, scrollHeight: element.scrollHeight })) }), contentType: 'application/json' });
  await expect.poll(async () => (await popup.boundingBox())!.height).toBeGreaterThan(initial!.height + 44);
  await last.focus();
  await page.evaluate(async () => {
    for (let frame = 0; frame < 6; frame++) await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
  });
  await expect(last).toBeFocused();
  const area = await popup.boundingBox();
  const action = await last.boundingBox();
  expect(action!.y).toBeGreaterThanOrEqual(area!.y);
  expect(action!.y + action!.height).toBeLessThanOrEqual(area!.y + area!.height);
  await page.keyboard.press('Escape');
  await expect(popup).toBeHidden();
});

test('shared kit native popovers fit the real changed visual viewport', async ({ page, browserName }, testInfo) => {
  expect(browserName).toBe('chromium');
  await page.setViewportSize({ width: 390, height: 700 });
  await page.goto('/design-system');
  await page.getByRole('button', { name: 'Recipe actions', exact: true }).click();
  const popup = page.locator('#showcase-actions');
  await expect(popup).toBeVisible();
  const session = await page.context().newCDPSession(page);
  try {
    await session.send('Emulation.setPageScaleFactor', { pageScaleFactor: 2 });
    await expect.poll(() => page.evaluate(() => window.visualViewport!.scale)).toBeGreaterThan(1);
    // Actual browser visual scaling, not CSS resize or a mocked viewport API.
    // Native browser zoom/OS keyboard/device/AT coverage is recorded separately.
    const visible = await page.evaluate(() => {
      const v = window.visualViewport!;
      return { left: v.offsetLeft, top: v.offsetTop, width: v.width, height: v.height, scale: v.scale };
    });
    expect(visible.width).toBeLessThan(390);
    await testInfo.attach('actual-visual-viewport-bounds', { body: JSON.stringify({ visible, popup: await popup.boundingBox(), userAgent: await page.evaluate(() => navigator.userAgent) }), contentType: 'application/json' });
    await expect.poll(async () => {
      const bounds = (await popup.boundingBox())!;
      return bounds.x >= visible.left - 1 && bounds.y >= visible.top - 1 &&
        bounds.x + bounds.width <= visible.left + visible.width + 1 &&
        bounds.y + bounds.height <= visible.top + visible.height + 1;
    }).toBe(true);
    const action = popup.getByRole('button', { name: 'Use selected source' });
    await action.focus();
    await expect(action).toBeFocused();
    const target = (await action.boundingBox())!;
    expect(target.x + target.width).toBeLessThanOrEqual(visible.left + visible.width + 1);
  } finally {
    await session.send('Emulation.setPageScaleFactor', { pageScaleFactor: 1 });
    await session.detach();
  }
});

test('shared kit native popovers preserve the existing workspace error announcement', async ({ page }) => {
  await page.goto('/recipes');
  await page.route('**/leptos-api/list_user_workspaces*', route => route.fulfill({ status: 503, body: 'workspace lookup unavailable' }));
  const popup = await openAccount(page, page.locator('.app-shell__desktop'));
  await expect(popup.getByText('Unable to load workspaces', { exact: true })).toBeVisible();
  await expect(popup.getByRole('status').filter({ hasText: 'Unable to load workspaces' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(popup).toBeHidden();
});

test('a dirty new recipe remains private to its original account and workspace', async ({ page }) => {
  const ownerId = getDefaultUserId();
  const originalWorkspace = getDefaultWorkspaceId();
  const otherName = `Editor workspace ${Date.now()}`;
  const otherWorkspace = seedWorkspace({ ownerId, name: otherName });
  db.exec(`UPDATE users SET last_workspace_id = '${originalWorkspace}' WHERE user_id = '${ownerId}'`);
  const secondEmail = `editor-switch-${Date.now()}@example.test`;
  const secondPassword = 'EditorSwitchTest1234';
  await createSecondUser(null, { email: secondEmail, name: 'Second brewer', password: secondPassword, workspaceId: originalWorkspace });
  walCheckpoint();

  await page.goto('/recipe-editor/new');
  const title = page.getByLabel('Recipe name', { exact: true });
  await title.fill('Private unsaved switch draft');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  let watchForDraft = false;
  const leakedRequests: string[] = [];
  page.on('request', request => {
    if (watchForDraft && request.postData()?.includes('Private unsaved switch draft')) leakedRequests.push(request.url());
  });
  const accountPopup = await openAccount(page, page.locator('.app-shell__desktop'));
  await accountPopup.getByRole('button', { name: otherName, exact: true }).click();
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Stay' }).click();
  await expect(title).toHaveValue('Private unsaved switch draft');

  const other = await page.context().newPage();
  // Establish independently signed sessions for each selected workspace. The
  // legacy workspace switch is covered by the account feature's own tests.
  db.exec(`UPDATE users SET last_workspace_id = '${otherWorkspace}' WHERE user_id = '${ownerId}'`);
  walCheckpoint();
  await loginUser(other, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
  const [otherContext] = await Promise.all([
    other.waitForResponse(response => response.url().includes('/get_user_context') && response.ok()),
    other.goto('/recipe-editor/new'),
  ]);
  expect((await otherContext.json()).workspace_id).toBe(otherWorkspace);
  watchForDraft = true;
  await page.bringToFront();
  await expect(page.getByText('Recipe held in its original workspace')).toBeVisible();
  await expect(title).toBeHidden();
  expect(leakedRequests).toEqual([]);
  watchForDraft = false;

  await other.bringToFront();
  db.exec(`UPDATE users SET last_workspace_id = '${originalWorkspace}' WHERE user_id = '${ownerId}'`);
  walCheckpoint();
  await loginUser(other, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
  const [originalContext] = await Promise.all([
    other.waitForResponse(response => response.url().includes('/get_user_context') && response.ok()),
    other.goto('/recipe-editor/new'),
  ]);
  expect((await originalContext.json()).workspace_id).toBe(originalWorkspace);
  await page.bringToFront();
  await expect(title).toBeVisible();
  await expect(title).toHaveValue('Private unsaved switch draft');

  await other.bringToFront();
  await loginUser(other, secondEmail, secondPassword);
  watchForDraft = true;
  await page.bringToFront();
  await expect(page.getByText('Recipe held in its original workspace')).toBeVisible();
  await expect(title).toBeHidden();
  expect(db.exec("SELECT COUNT(*) FROM recipes WHERE name = 'Private unsaved switch draft'")).toBe('0');
  expect(leakedRequests).toEqual([]);
  watchForDraft = false;
  await other.bringToFront();
  await loginUser(other, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
  await page.bringToFront();
  await expect(title).toBeVisible();
  await expect(title).toHaveValue('Private unsaved switch draft');
  await other.close();
});

test('equipment profiles can be saved, selected, edited and recovered', async ({ page }) => {
  const previews: any[] = [];
  page.on('response', async response => {
    if (response.url().includes('/preview_formulation') && response.ok()) {
      try { previews.push(await response.json()); } catch { /* Ignore non-JSON transport failures. */ }
    }
  });
  const name = `Profile ${Date.now()}`;
  await page.goto('/equipment');
  await page.getByLabel('Profile name').fill(name);
  await page.getByLabel('Mash efficiency (%)').fill('75');
  await page.getByLabel('Boil-off (L/h)', { exact: true }).fill('3');
  await page.getByRole('button', { name: 'Save profile' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Profile saved.' })).toBeVisible();
  const profileId = await page.getByLabel('Manage profile').inputValue();
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(`Profile recipe ${Date.now()}`);
  await page.getByText('Equipment settings', { exact: true }).click();
  const selector = page.getByLabel('Equipment profile', { exact: true });
  await expect(selector.getByRole('option', { name })).toBeAttached();
  await selector.selectOption({ label: name });
  expect(await selector.inputValue()).toBe(profileId);
  await expect.poll(() => previews.some(result => result.Preview?.context?.equipment_profile?.profile_id === profileId && result.Preview.context.equipment_profile.revision === 1)).toBe(true);
  const originalDigest = previews.find(result => result.Preview?.context?.equipment_profile?.profile_id === profileId && result.Preview.context.equipment_profile.revision === 1).Preview.context_digest;
  await expect(page.getByText('Equipment profile unavailable')).toHaveCount(0);
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  const recipeUrl = page.url();
  await page.reload();
  await page.getByText('Equipment settings', { exact: true }).click();
  await expect(page.getByLabel('Equipment profile', { exact: true })).toHaveValue(profileId);
  // Edit the shared profile in the independent manager; the saved recipe is untouched.
  await page.goto(`/equipment?profile=${profileId}`);
  await expect(page.getByLabel('Manage profile')).toHaveValue(profileId);
  await page.getByLabel('Mash efficiency (%)').fill('80');
  await page.getByRole('button', { name: 'Save profile' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Profile saved.' })).toBeVisible();
  await expect(page.getByLabel('Mash efficiency (%)')).toHaveValue('80');
  await page.goto(recipeUrl);
  await expect.poll(() => previews.some(result => result.Preview?.context?.equipment_profile?.profile_id === profileId && result.Preview.context.equipment_profile.revision === 2 && result.Preview.context_digest !== originalDigest)).toBe(true);
  await page.goto(`/equipment?profile=${profileId}`);
  await page.getByRole('button', { name: 'Archive', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Profile archived.' })).toBeVisible();
  await page.goto(recipeUrl);
  await page.getByText('Equipment settings', { exact: true }).click();
  await expect(page.getByText('Equipment profile unavailable', { exact: true })).toBeVisible();
  await expect.poll(() => previews.includes('UnresolvedProfile')).toBe(true);
  await page.goto(`/equipment?profile=${profileId}`);
  await page.getByRole('button', { name: 'Unarchive' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Profile saved.' })).toBeVisible();
  await page.goto(recipeUrl);
  await page.getByText('Equipment settings', { exact: true }).click();
  await expect(page.getByText('Equipment profile unavailable', { exact: true })).toHaveCount(0);
});

test('Equipment opens from customer navigation without a recipe and keeps the manager independent', async ({ page }) => {
  await page.goto('/recipes');
  await page.getByRole('navigation', { name: 'Your brewery' }).getByRole('link', { name: 'Equipment' }).click();
  await expect(page).toHaveURL(/\/equipment$/);
  await expect(page.getByRole('heading', { name: 'Equipment', level: 1 })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Equipment' })).toHaveAttribute('aria-current', 'page');
  await expect(page.getByText('Editing shared profile')).toHaveCount(0);
  await expect(page.getByText('New equipment profile')).toBeVisible();
  await expect(page.getByLabel('Equipment profile', { exact: true })).toHaveCount(0);
  await expect(page.getByLabel('Recipe name')).toHaveCount(0);
});

test('a dirty profile draft is guarded on profile switch and navigation, then discarded or kept on request', async ({ page }) => {
  const first = await createEquipmentProfile(page, `Guard first ${Date.now()}`, { 'profile-boil-off': '3' });
  const second = await createEquipmentProfile(page, `Guard second ${Date.now()}`, { 'profile-boil-off': '5' });
  await page.goto(`/equipment?profile=${first}`);
  await expect(page.locator('#profile-boil-off')).toHaveValue('3');
  await page.locator('#profile-boil-off').fill('4');
  await page.locator('#equipment-profile-edit').selectOption(second);
  const choice = page.getByRole('group', { name: 'Unsaved profile changes' });
  await expect(choice).toBeVisible();
  // The edit is neither lost nor replaced while the choice is open.
  await expect(page.locator('#profile-boil-off')).toHaveValue('4');
  await expect(page.locator('#equipment-profile-edit')).toHaveValue(first);
  await choice.getByRole('button', { name: 'Stay on profile' }).click();
  await expect(page.locator('#equipment-profile-edit')).toBeFocused();
  await expect(page.locator('#profile-boil-off')).toHaveValue('4');
  await expect(page.getByRole('button', { name: 'Archive' })).toBeDisabled();
  // Leaving through the shell uses the shared guard; Stay keeps the draft.
  await page.getByRole('navigation', { name: 'Your brewery' }).getByRole('link', { name: 'Recipes' }).click();
  const dialog = page.getByRole('dialog', { name: 'Unsaved task changes' });
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: 'Stay' }).click();
  await expect(page).toHaveURL(/\/equipment/);
  await expect(page.locator('#profile-boil-off')).toHaveValue('4');
  // Discard and switch drops only the profile draft.
  await page.locator('#equipment-profile-edit').selectOption(second);
  await page.getByRole('group', { name: 'Unsaved profile changes' }).getByRole('button', { name: 'Discard changes and switch' }).click();
  await expect(page.locator('#equipment-profile-edit')).toHaveValue(second);
  await expect(page.locator('#profile-boil-off')).toHaveValue('5');
  // Save profile and switch persists the first profile revision before moving.
  await page.locator('#equipment-profile-edit').selectOption(first);
  await page.locator('#profile-boil-off').fill('6');
  await page.locator('#equipment-profile-edit').selectOption(second);
  await page.getByRole('group', { name: 'Unsaved profile changes' }).getByRole('button', { name: 'Save profile and switch' }).click();
  await expect(page.locator('#equipment-profile-edit')).toHaveValue(second);
  await page.locator('#equipment-profile-edit').selectOption(first);
  await expect(page.locator('#profile-boil-off')).toHaveValue('6');
  // Save and leave from the shell guard saves the profile, then navigates once.
  await page.locator('#profile-boil-off').fill('7');
  await page.getByRole('navigation', { name: 'Your brewery' }).getByRole('link', { name: 'Recipes' }).click();
  await page.getByRole('dialog', { name: 'Unsaved task changes' }).getByRole('button', { name: 'Save and leave' }).click();
  await expect(page).toHaveURL(/\/recipes$/);
  await page.goto(`/equipment?profile=${first}`);
  await expect(page.locator('#profile-boil-off')).toHaveValue('7');
});

test('a profile conflict keeps the raw edits and never overwrites the newer revision', async ({ page }) => {
  const id = await createEquipmentProfile(page, `Conflict ${Date.now()}`, { 'profile-boil-off': '3' });
  await page.goto(`/equipment?profile=${id}`);
  await expect(page.locator('#profile-boil-off')).toHaveValue('3');
  await page.locator('#profile-boil-off').fill('4');
  const other = await page.context().newPage();
  await other.goto(`/equipment?profile=${id}`);
  await expect(other.locator('#profile-boil-off')).toHaveValue('3');
  await other.locator('#profile-boil-off').fill('9');
  await other.getByRole('button', { name: 'Save profile' }).click();
  await expect(other.getByRole('status').filter({ hasText: 'Profile saved.' })).toBeVisible();
  await other.close();
  await page.getByRole('button', { name: 'Save profile' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Profile changed elsewhere' })).toBeVisible();
  await expect(page.locator('#profile-boil-off')).toHaveValue('4');
  await page.getByRole('button', { name: 'Load latest and keep my edits' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Loaded the latest profile' })).toBeVisible();
  await expect(page.locator('#profile-boil-off')).toHaveValue('4');
  await page.getByRole('button', { name: 'Save profile' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Profile saved.' })).toBeVisible();
  await page.goto(`/equipment?profile=${id}`);
  await expect(page.locator('#profile-boil-off')).toHaveValue('4');
});

test('a conflict notice does not follow the user to another profile', async ({ page }) => {
  const first = await createEquipmentProfile(page, `Conflict A ${Date.now()}`, { 'profile-boil-off': '3' });
  const second = await createEquipmentProfile(page, `Conflict B ${Date.now()}`, { 'profile-boil-off': '5' });
  await page.goto(`/equipment?profile=${first}`);
  await expect(page.locator('#profile-boil-off')).toHaveValue('3');
  await page.locator('#profile-boil-off').fill('4');
  const other = await page.context().newPage();
  await other.goto(`/equipment?profile=${first}`);
  await expect(other.locator('#profile-boil-off')).toHaveValue('3');
  await other.locator('#profile-boil-off').fill('9');
  await other.getByRole('button', { name: 'Save profile' }).click();
  await expect(other.getByRole('status').filter({ hasText: 'Profile saved.' })).toBeVisible();
  await other.close();
  await page.getByRole('button', { name: 'Save profile' }).click();
  await expect(page.getByRole('button', { name: 'Load latest and keep my edits' })).toBeVisible();
  await page.locator('#equipment-profile-edit').selectOption(second);
  await page.getByRole('group', { name: 'Unsaved profile changes' }).getByRole('button', { name: 'Discard changes and switch' }).click();
  await expect(page.locator('#equipment-profile-edit')).toHaveValue(second);
  await expect(page.getByRole('button', { name: 'Load latest and keep my edits' })).toHaveCount(0);
});

test('recipe overrides show inherited and effective values and reset deletes the override', async ({ page }) => {
  const name = `Inherit ${Date.now()}`;
  const profileId = await createEquipmentProfile(page, name, { 'profile-boil-off': '3', 'profile-mash-efficiency': '75', 'profile-capacity': '30' }, 'natural');
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(`Override recipe ${Date.now()}`);
  await page.getByText('Equipment settings', { exact: true }).click();
  await page.getByLabel('Equipment profile', { exact: true }).selectOption(profileId);
  await expect(page.getByText(`Using profile “${name}” for this recipe`)).toBeVisible();
  const boil = page.locator('#recipe-boil-off');
  await expect(boil).toHaveValue('');
  await expect(page.getByText(`Inherited: effective 3 L/h from ${name}.`)).toBeVisible();
  await boil.fill('4');
  await expect(page.getByText(`Overridden for this recipe: effective 4 L/h. Inherited: 3 L/h from ${name}.`)).toBeVisible();
  // The header badge also says "1 recipe override"; the field hint is the intended element.
  await expect(page.getByText(/^1 recipe override\./)).toBeVisible();
  await page.locator('#recipe-efficiency').fill('70');
  await expect(page.locator('#recipe-efficiency-override')).toHaveValue('');
  await page.locator('#recipe-chilling').selectOption('forced');
  await page.locator('#recipe-chilling-rate').fill('0.5');
  await page.getByRole('button', { name: 'Reset Boil-off to inherited' }).click();
  // Reset removes the override rather than copying the profile value into it.
  await expect(boil).toHaveValue('');
  await expect(page.getByText(`Inherited: effective 3 L/h from ${name}.`)).toBeVisible();
  await expect(page.getByRole('button', { name: 'Reset Boil-off to inherited' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Reset Chilling to inherited' }).click();
  await expect(page.locator('#recipe-chilling')).toHaveValue('');
  await expect(page.locator('#recipe-chilling-rate')).toHaveCount(0);
  // A raw unfinished override is still an override the user can reset.
  await boil.fill('-');
  await expect(page.getByRole('button', { name: 'Reset Boil-off to inherited' })).toBeVisible();
  await page.getByRole('button', { name: 'Reset Boil-off to inherited' }).click();
  await expect(boil).toHaveValue('');
  // Switching the selected profile never writes into the recipe overrides.
  await page.getByLabel('Equipment profile', { exact: true }).selectOption('');
  await expect(page.getByText('Using recipe-only equipment')).toBeVisible();
  await expect(page.locator('#recipe-capacity')).toHaveValue('');
});

test('Manage equipment from a recipe returns to the retained recipe', async ({ page }) => {
  const name = `Manage ${Date.now()}`;
  const profileId = await createEquipmentProfile(page, name, { 'profile-boil-off': '3' });
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(`Retained ${Date.now()}`);
  await page.getByText('Equipment settings', { exact: true }).click();
  await page.getByLabel('Equipment profile', { exact: true }).selectOption(profileId);
  await page.getByRole('link', { name: 'Manage equipment profiles' }).click();
  // The dirty recipe is protected by its own guard; nothing is discarded implicitly.
  const dialog = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: 'Stay' }).click();
  await expect(page).toHaveURL(/recipe-editor\/new/);
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  const recipeUrl = page.url();
  await page.getByRole('link', { name: 'Manage equipment profiles' }).click();
  await expect(page).toHaveURL(/\/equipment\?profile=/);
  await expect(page.getByLabel('Manage profile')).toHaveValue(profileId);
  await page.getByRole('link', { name: 'Back to recipe' }).click();
  await expect(page).toHaveURL(recipeUrl);
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(/^Retained /);
});

test('an uncertain equipment save retries the identical request', async ({ page }) => {
  await page.goto('/equipment');
  const name = `Retry profile ${Date.now()}`;
  await page.getByLabel('Profile name').fill(name);
  const requests: { profile_id: string, request_id: string }[] = [];
  await page.route('**/leptos-api/execute_equipment_command*', async route => {
    const payload = new URLSearchParams(route.request().postData() ?? '').get('command_json');
    const command = payload ? JSON.parse(payload) : null;
    if (command?.Save) {
      requests.push(command.Save);
      if (requests.length === 1) {
        const response = await route.fetch();
        await route.fulfill({ response, body: JSON.stringify('Unavailable') });
        return;
      }
    }
    await route.continue();
  });
  await page.getByRole('button', { name: 'Save profile' }).click();
  await expect(page.getByText('The result is uncertain. Retry the same request.')).toBeVisible();
  // The pinned request locks the form and survives a reload with the same identity.
  await expect(page.getByLabel('Profile name')).toBeDisabled();
  await page.reload();
  await expect(page.getByText('A previous profile request was not confirmed')).toBeVisible();
  await expect(page.getByLabel('Profile name')).toHaveValue(name);
  await expect(page.getByLabel('Profile name')).toBeDisabled();
  await page.getByRole('button', { name: 'Retry previous request' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Profile saved.' })).toBeVisible();
  expect(requests).toHaveLength(2);
  expect(requests[1].request_id).toBe(requests[0].request_id);
  expect(requests[1].profile_id).toBe(requests[0].profile_id);
  await expect(page.getByLabel('Manage profile').getByRole('option', { name })).toHaveCount(1);
});

test('profile pagination exposes later active and archived profiles', async ({ page }) => {
  const workspace = getDefaultWorkspaceId();
  const user = getDefaultUserId();
  db.exec(
    `UPDATE users SET last_workspace_id = '${workspace}' WHERE user_id = '${user}'`,
  );
  walCheckpoint();
  const refreshed = await page.evaluate(
    async () => (await fetch('/api/v1/auth/refresh', { method: 'POST' })).status,
  );
  expect(refreshed).toBe(200);
  const inserts: string[] = ['BEGIN'];
  for (let i = 0; i < 102; i++) {
    const id = `00000000-0000-4000-8000-${String(i).padStart(12, '0')}`;
    const name = `Paged profile ${i}`;
    const archived = i === 101 ? 1 : 0;
    inserts.push(`INSERT INTO equipment_profiles(workspace_id,profile_id,name,current_revision,metadata_revision,archived) VALUES ('${workspace}','${id}','${name}',1,1,${archived})`);
    inserts.push(`INSERT INTO equipment_revisions(workspace_id,profile_id,revision,name,schema_version,settings_json,provenance_json,content_digest,actor_id,recorded_at) VALUES ('${workspace}','${id}',1,'${name}',1,'{"schema_version":1}','{"source":"test","assumptions":[]}','fixture','${user}',1700000000)`);
  }
  inserts.push('COMMIT');
  db.exec(inserts.join(';'));
  walCheckpoint();
  await page.goto('/equipment');
  const manage = page.getByLabel('Manage profile');
  await expect(manage.getByRole('option', { name: 'Paged profile 101 (archived)' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Load more profiles' }).click();
  await expect(manage.getByRole('option', { name: 'Paged profile 101 (archived)' })).toBeAttached();
  await page.goto('/recipe-editor/new');
  await page.getByText('Equipment settings', { exact: true }).click();
  await page.getByRole('button', { name: 'Load more profiles' }).click();
  await expect(page.getByLabel('Equipment profile', { exact: true }).getByRole('option', { name: 'Paged profile 100' })).toBeAttached();
  await expect(page.getByLabel('Equipment profile', { exact: true }).getByRole('option', { name: 'Paged profile 101 (archived)' })).toHaveCount(0);
});

test('undo and retained raw draft survive reload without becoming saved history', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  const title = page.getByLabel('Recipe name', { exact: true });
  const volume = page.getByLabel('Batch volume (L)');
  await title.fill('Recover unfinished recipe');
  await volume.fill('-');
  await expect.poll(() => page.evaluate(() => Object.keys(localStorage).some(k => k.startsWith('krausen-draft-v1:')))).toBe(true);
  let unload = false;
  page.once('dialog', async dialog => { unload = dialog.type() === 'beforeunload'; await dialog.accept(); });
  await page.reload();
  expect(unload).toBe(true);
  await expect(page.getByText('A retained draft is available')).toBeVisible();
  await page.getByRole('button', { name: 'Restore draft' }).click();
  await expect(title).toHaveValue('Recover unfinished recipe');
  await expect(volume).toHaveValue('-');
  await expect(page.getByRole('status').filter({ hasText: 'Complete invalid number fields to see estimates.' })).toBeVisible();
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Complete invalid number fields before saving.' })).toBeVisible();
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(title).toHaveValue('');
});

test('navigation offers stay, blocks failed save, and discards only on request', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill('Navigation draft');
  await page.getByLabel('Batch volume (L)').fill('-');
  await openSettings(page);
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Save and leave' }).click();
  await expect(guard.getByRole('status').filter({ hasText: 'Complete invalid number fields' })).toContainText('Complete invalid number fields');
  await expect(page).toHaveURL(/recipe-editor/);
  await guard.getByRole('button', { name: 'Stay' }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue('Navigation draft');
  await page.getByLabel('Batch volume (L)').fill('20');
  await page.route('**/leptos-api/save_recipe*', route => route.abort('failed'));
  await openSettings(page);
  await guard.getByRole('button', { name: 'Save and leave' }).click();
  await expect(guard.getByRole('status').filter({ hasText: 'Save failed' })).toContainText('Save failed');
  await expect(page).toHaveURL(/recipe-editor/);
  // A lost save response leaves the outcome uncertain, so discarding is blocked until it is resolved.
  await expect(guard.getByRole('button', { name: 'Discard and leave' })).toBeDisabled();
  await guard.getByRole('button', { name: 'Stay' }).click();
  await page.unroute('**/leptos-api/save_recipe*');
  await openSettings(page);
  await guard.getByRole('button', { name: 'Save and leave' }).click();
  await expect(page).toHaveURL(/settings\/profile/);
});

test('stale saves retain edits and require deliberate reapply against current head', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill('Conflict base');
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  const url = page.url();
  const other = await page.context().newPage();
  await other.goto(url);
  await other.getByLabel('Recipe name', { exact: true }).fill('Remote edit');
  await page.getByLabel('Recipe name', { exact: true }).fill('My retained edit');
  await page.getByLabel('Batch volume (L)').fill('20.000000000000000');
  await other.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(other.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByRole('region', { name: 'Save conflict' })).toBeVisible();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue('My retained edit');
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  const conflict = page.getByRole('region', { name: 'Save conflict' });
  await expect(conflict).toBeVisible();
  await conflict.getByText('Compare saved revision and your draft').click();
  await expect(conflict).toContainText('Remote edit');
  await expect(conflict).toContainText('My retained edit');
  await expect(conflict).toContainText('20.000000000000000');
  await expect(conflict.getByRole('heading', { name: 'Local changes', exact: true })).toBeVisible();
  await expect(conflict.getByRole('heading', { name: 'Remote changes', exact: true })).toBeVisible();
  await conflict.getByRole('button', { name: 'Reapply my draft' }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue('My retained edit');
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue('20.000000000000000');
  await expect(page.getByRole('status').filter({ hasText: 'Review it, then choose Save' })).toBeVisible();
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 3' })).toBeVisible();
  await other.close();
});

test('browser back keeps a dirty draft until an explicit navigation choice', async ({ page }) => {
  await page.goto('/recipes');
  await page.getByRole('link', { name: 'New recipe' }).click();
  await page.getByLabel('Recipe name', { exact: true }).fill('Back navigation draft');
  await page.evaluate(() => history.back());
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Stay' }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue('Back navigation draft');
  await page.evaluate(() => history.back());
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Discard and leave' }).click();
  await expect(page).toHaveURL(/\/recipes/);
});

test('retained draft stays isolated across sign-out and a different account', async ({ page }) => {
  const secondEmail = `recovery-${Date.now()}@example.test`;
  const password = 'RecoveryTest1234';
  await createSecondUser(null, { email: secondEmail, name: 'Second brewer', password, workspaceId: getDefaultWorkspaceId() });
  walCheckpoint();
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill('Private retained draft');
  await expect.poll(() => page.evaluate(() => Object.keys(localStorage).some(k => k.startsWith('krausen-draft-v1:')))).toBe(true);
  const other = await page.context().newPage();
  await loginUser(other, secondEmail, password);
  await other.goto('/recipe-editor/new');
  await expect(other.getByLabel('Recipe name', { exact: true })).toHaveValue('');
  await expect(other.getByText('A retained draft is available')).toHaveCount(0);
  await loginUser(other, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
  await other.goto('/recipe-editor/new');
  await expect(other.getByText('A retained draft is available')).toBeVisible();
  await other.getByRole('button', { name: 'Restore draft' }).click();
  await expect(other.getByLabel('Recipe name', { exact: true })).toHaveValue('Private retained draft');
  await other.close();
});

test('save and leave waits for a successful revision before navigating', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  const name = `Save before leave ${Date.now()}`;
  await page.getByLabel('Recipe name', { exact: true }).fill(name);
  await openSettings(page);
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Save and leave' }).click();
  await expect(page).toHaveURL(/settings\/profile/);
  expect(db.exec(`SELECT COUNT(*) FROM recipes WHERE name = '${name}' AND native_head_revision = 1`)).toBe('1');
});

test('a revoked session cannot read the retained draft until reauthentication', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill('Draft after revocation');
  await expect.poll(() => page.evaluate(() => Object.keys(localStorage).some(k => k.startsWith('krausen-draft-v1:')))).toBe(true);
  const userId = getDefaultUserId();
  // Sign out everywhere: advance the cutoff and revoke the refresh tokens, so the
  // server cannot silently refresh the session either.
  // One transaction on the database's own clock: the editor's preview recalculation
  // can refresh the session at any moment, and a refresh that lands between two
  // separate statements (or between reading the host clock and writing it) mints a
  // token newer than the cutoff, which stays valid and defeats the revocation.
  db.exec(`BEGIN IMMEDIATE; UPDATE users SET sessions_valid_from = CAST(unixepoch('subsec') * 1000000 AS INTEGER) WHERE user_id = '${userId}'; UPDATE refresh_tokens SET is_active = 0 WHERE user_id = '${userId}'; COMMIT;`);
  walCheckpoint();
  const other = await page.context().newPage();
  await other.goto('/recipe-editor/new');
  await expect(other.getByText('A retained draft is available')).toHaveCount(0);
  await expect(other.getByLabel('Recipe name', { exact: true })).toHaveCount(0);
  // The newly opened route can be redirected by the shared authentication
  // boundary. The existing original-scope editor owns the held draft notice.
  await page.bringToFront();
  // Nothing else is guaranteed to ask the server again after the revocation (the
  // editor's own recalculation may have finished just before it, and bringToFront
  // raises no focus event when the browser already treats this window as active),
  // so raise the window focus event the editor's access recheck listens for.
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByText('Session expired', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Retry check', exact: true })).toBeVisible();
  // The shared authentication boundary redirects the new window to sign-in; let that
  // navigation finish before signing in there, or the two navigations abort each other.
  await expect(other).toHaveURL(/\/login/);
  await loginUser(other, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
  await other.goto('/recipe-editor/new');
  await expect(other.getByText('A retained draft is available')).toBeVisible();
  await other.getByRole('button', { name: 'Restore draft' }).click();
  await expect(other.getByLabel('Recipe name', { exact: true })).toHaveValue('Draft after revocation');
  await other.close();
});

test('recovered uncertain create retries its original request without a duplicate', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  const name = `Uncertain create ${Date.now()}`;
  await page.getByLabel('Recipe name', { exact: true }).fill(name);
  let intercepted = false;
  const submitted: string[] = [];
  await page.route('**/leptos-api/save_recipe*', async route => {
    submitted.push(route.request().postData() ?? '');
    if (!intercepted) {
      intercepted = true;
      await route.fetch();
      await route.abort('failed');
    } else await route.continue();
  });
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Save failed' })).toBeVisible();
  await expect.poll(() => db.exec(`SELECT COUNT(*) FROM recipes WHERE name = '${name}'`)).toBe('1');
  await expect.poll(() => page.evaluate(() => Object.keys(localStorage).some(k => k.startsWith('krausen-draft-v1:')))).toBe(true);
  page.once('dialog', dialog => dialog.accept());
  await page.reload();
  await page.getByRole('button', { name: 'Restore draft' }).click();
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  expect(db.exec(`SELECT COUNT(*) FROM recipes WHERE name = '${name}'`)).toBe('1');
  expect(submitted).toHaveLength(2);
  expect(submitted[1]).toBe(submitted[0]);
});

test('separate local drafts remain individually recoverable', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill('First local draft');
  await expect.poll(() => page.evaluate(() => Object.keys(localStorage).filter(k => k.startsWith('krausen-draft-v1:')).length)).toBe(1);
  const other = await page.context().newPage();
  await other.goto('/recipe-editor/new');
  await expect(other.getByText('A retained draft is available')).toBeVisible();
  await other.getByRole('button', { name: 'Start a separate draft' }).click();
  await other.getByLabel('Recipe name', { exact: true }).fill('Second local draft');
  await expect.poll(() => other.evaluate(() => Object.keys(localStorage).filter(k => k.startsWith('krausen-draft-v1:')).length)).toBe(2);
  other.once('dialog', dialog => dialog.accept());
  await other.reload();
  await expect(other.getByText(/First local draft.*based on/)).toBeVisible();
  await expect(other.getByText(/Second local draft.*based on/)).toBeVisible();
  await other.close();
});

test('a pending save cannot be discarded while its result is uncertain', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill('Pending navigation save');
  await page.route('**/leptos-api/save_recipe*', async route => {
    const response = await route.fetch();
    await new Promise(resolve => setTimeout(resolve, 1200));
    await route.fulfill({ response });
  });
  await openSettings(page);
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await guard.getByRole('button', { name: 'Save and leave' }).click();
  await expect(guard.getByRole('button', { name: 'Discard and leave' })).toBeDisabled();
  await guard.getByRole('button', { name: 'Stay' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  await expect(page).toHaveURL(/recipe-editor/);
});

test('clean focus refetch invalidates preview before showing a newer saved recipe', async ({ page }) => {
  const previews: any[] = [];
  page.on('response', async response => {
    if (response.url().includes('/preview_formulation') && response.ok()) {
      try { previews.push(await response.json()); } catch { /* Transport error, not a preview. */ }
    }
  });
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill('Clean original');
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  const url = page.url();
  await page.reload();
  await expect.poll(() => previews.some(result => result.Preview?.context?.draft?.title === 'Clean original')).toBe(true);
  const other = await page.context().newPage();
  await other.goto(url);
  await other.getByLabel('Recipe name', { exact: true }).fill('Clean remote update');
  await other.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(other.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue('Clean remote update');
  await expect.poll(() => previews.some(result => result.Preview?.context?.draft?.title === 'Clean remote update')).toBe(true);
  await other.close();
});

test('recovered draft retains saved profile selection and resolves its preview', async ({ page }) => {
  const previews: any[] = [];
  page.on('response', async response => {
    if (response.url().includes('/preview_formulation') && response.ok()) {
      try { previews.push(await response.json()); } catch { /* Transport errors are not previews. */ }
    }
  });
  const profileName = `Recovery profile ${Date.now()}`;
  await createEquipmentProfile(page, profileName);
  await page.goto('/recipe-editor/new');
  await page.getByText('Equipment settings', { exact: true }).click();
  const selector = page.getByLabel('Equipment profile', { exact: true });
  if (await selector.getByRole('option', { name: profileName }).count() === 0 && await page.getByRole('button', { name: 'Load more profiles' }).isVisible()) {
    await page.getByRole('button', { name: 'Load more profiles' }).click();
  }
  await expect(selector.getByRole('option', { name: profileName })).toBeAttached();
  await selector.selectOption({ label: profileName });
  const profileId = await selector.inputValue();
  await page.getByLabel('Recipe name', { exact: true }).fill('Profile recovery draft');
  await expect.poll(() => page.evaluate(() => Object.keys(localStorage).some(k => k.startsWith('krausen-draft-v1:')))).toBe(true);
  page.once('dialog', dialog => dialog.accept());
  await page.reload();
  await page.getByRole('button', { name: 'Restore draft' }).click();
  await page.getByText('Equipment settings', { exact: true }).click();
  await expect(page.getByLabel('Equipment profile', { exact: true })).toHaveValue(profileId);
  await expect.poll(() => previews.some(result => result.Preview?.context?.draft?.title === 'Profile recovery draft' && result.Preview.context.equipment_profile?.profile_id === profileId)).toBe(true);
});

test('a failed workspace switch is reported after choosing to discard', async ({ page }) => {
  const owner = getDefaultUserId();
  const original = getDefaultWorkspaceId();
  const name = `Unavailable workspace ${Date.now()}`;
  seedWorkspace({ ownerId: owner, name });
  db.exec(`UPDATE users SET last_workspace_id = '${original}' WHERE user_id = '${owner}'`);
  walCheckpoint();
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill('Draft before switch');
  await page.route('**/leptos-api/switch_workspace*', route => route.abort('failed'));
  const switchPopup = await openAccount(page, page.locator('.app-shell__desktop'));
  await switchPopup.getByRole('button', { name, exact: true }).click();
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await guard.getByRole('button', { name: 'Discard and leave' }).click();
  await expect(page.getByRole('alert')).toContainText('Could not switch workspaces.');
  await expect(page).toHaveURL(/recipe-editor/);
});

test('a failed sign-out is reported after choosing to discard', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill('Draft before sign-out');
  await page.route('**/leptos-api/logout*', route => route.abort('failed'));
  const signOutPopup = await openAccount(page, page.locator('.app-shell__desktop'));
  await signOutPopup.getByRole('button', { name: 'Sign Out', exact: true }).click();
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await guard.getByRole('button', { name: 'Discard and leave' }).click();
  await expect(page.getByRole('alert')).toContainText('Could not sign out.');
  await expect(page).toHaveURL(/recipe-editor/);
});

test('repeated guarded Back Forward Stay preserves native entries and the exact draft position', async ({ page }) => {
  await page.goto('/recipes');
  await page.getByRole('link', { name: 'New recipe', exact: true }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toBeVisible();
  await openSettings(page);
  await expect(page).toHaveURL(/\/settings\/profile(\?return=[^#]*)?$/);
  await page.evaluate(() => history.back());
  await expect(page).toHaveURL(/\/recipe-editor\/new$/);
  const name = page.getByLabel('Recipe name', { exact: true });
  await name.fill('Preserve the original history draft');
  const before = await name.evaluate((element: HTMLInputElement) => {
    element.setSelectionRange(2, 7);
    const main = document.querySelector<HTMLElement>('.app-shell__main')!;
    main.scrollTop = 180;
    return { href: location.href, historyLength: history.length, scroll: main.scrollTop,
      value: element.value, start: element.selectionStart, end: element.selectionEnd };
  });
  expect(before.scroll).toBeGreaterThan(0);
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes', exact: true });
  for (const direction of ['forward', 'back', 'forward'] as const) {
    await page.evaluate(direction => history[direction](), direction);
    await expect(guard).toBeVisible();
    await guard.getByRole('button', { name: 'Stay', exact: true }).click();
    await expect(guard).not.toBeVisible();
    await expect(page).toHaveURL(before.href);
    await expect(name).toHaveValue(before.value);
    await expect(name).toBeFocused();
    await expect.poll(() => name.evaluate((element: HTMLInputElement) => ({
      historyLength: history.length,
      scroll: document.querySelector<HTMLElement>('.app-shell__main')!.scrollTop,
      start: element.selectionStart, end: element.selectionEnd,
    }))).toEqual({ historyLength: before.historyLength, scroll: before.scroll, start: before.start, end: before.end });
  }
  await page.evaluate(() => history.forward());
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Discard and leave', exact: true }).click();
  await expect(page).toHaveURL(/\/settings\/profile(\?return=[^#]*)?$/);
  expect(await page.evaluate(() => history.length)).toBe(before.historyLength);
});

test('recipe discard publishes its reset generation before the native departure lease is captured', async ({ page }) => {
  await page.goto('/recipes');
  await page.getByRole('link', { name: 'New recipe', exact: true }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toBeVisible();
  await openSettings(page);
  await expect(page).toHaveURL(/\/settings\/profile(\?return=[^#]*)?$/);
  const target = await page.evaluate(() => ({ key: (window as any).navigation.currentEntry.key, length: history.length }));
  await page.evaluate(() => history.back());
  await expect(page).toHaveURL(/\/recipe-editor\/new$/);
  await page.getByLabel('Recipe name', { exact: true }).fill('Discard only this unsent draft');
  const saves: string[] = [];
  page.on('request', request => {
    if (request.method() === 'POST' && /\/leptos-api\/save_recipe/.test(request.url())) saves.push(request.url());
  });
  await page.evaluate(() => history.forward());
  const guard = page.locator('#workspace-draft-warning');
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Discard and leave', exact: true }).click();
  await expect(page).toHaveURL(/\/settings\/profile(\?return=[^#]*)?$/);
  await expect(guard).not.toBeVisible();
  expect(await page.evaluate(() => ({ key: (window as any).navigation.currentEntry.key, length: history.length }))).toEqual(target);
  expect(saves).toHaveLength(0);
});

test('native asynchronous restoration rejection exposes an owned retry without replacing the first destination', async ({ page }) => {
  await page.goto('/recipes');
  await page.getByRole('link', { name: 'New recipe', exact: true }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toBeVisible();
  await openSettings(page);
  await expect(page).toHaveURL(/\/settings\/profile(\?return=[^#]*)?$/);
  const destinationKey = await page.evaluate(() => (window as any).navigation.currentEntry.key);
  await page.evaluate(() => history.back());
  await expect(page).toHaveURL(/\/recipe-editor\/new$/);
  const name = page.getByLabel('Recipe name', { exact: true });
  await name.fill('Keep the original native recovery attempt');
  const before = await name.evaluate((element: HTMLInputElement) => {
    element.setSelectionRange(3, 8, 'backward');
    return { href: location.href, length: history.length, key: (window as any).navigation.currentEntry.key };
  });
  await page.evaluate(sourceKey => {
    const navigation = (window as any).navigation;
    const traverseTo = navigation.traverseTo.bind(navigation);
    let rejectOriginalRestoration = true;
    navigation.traverseTo = (key: string, options?: unknown) => {
      if (key === sourceKey && rejectOriginalRestoration) {
        rejectOriginalRestoration = false;
        return { committed: Promise.reject(new DOMException('Controlled original-key rejection', 'AbortError')), finished: Promise.reject(new DOMException('Controlled original-key rejection', 'AbortError')) };
      }
      return traverseTo(key, options);
    };
  }, before.key);
  await page.evaluate(() => history.forward());
  const guard = page.locator('#workspace-draft-warning');
  await expect(guard).toBeVisible();
  await expect(guard.getByText(/browser rejected the original history traversal/i)).toBeVisible();
  await guard.getByRole('button', { name: 'Retry original history restoration', exact: true }).click();
  await expect(page).toHaveURL(before.href);
  await guard.getByRole('button', { name: 'Stay', exact: true }).click();
  await expect(guard).not.toBeVisible();
  await expect(name).toHaveValue('Keep the original native recovery attempt');
  await expect(name).toBeFocused();
  expect(await name.evaluate((element: HTMLInputElement) => ({ length: history.length, start: element.selectionStart, end: element.selectionEnd, direction: element.selectionDirection })))
    .toEqual({ length: before.length, start: 3, end: 8, direction: 'backward' });
  await page.evaluate(() => history.forward());
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Discard and leave', exact: true }).click();
  await expect(page).toHaveURL(/\/settings\/profile(\?return=[^#]*)?$/);
  expect(await page.evaluate(() => (window as any).navigation.currentEntry.key)).toBe(destinationKey);
  expect(await page.evaluate(() => history.length)).toBe(before.length);
});

for (const successor of ['source-only retry', 'new destination', 'new focus'] as const) {
test(`Stay during held native restoration fences ${successor} after rejection`, async ({ page }) => {
  await page.goto('/recipes');
  await page.getByRole('link', { name: 'New recipe', exact: true }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toBeVisible();
  await openSettings(page);
  await expect(page).toHaveURL(/\/settings\/profile(\?return=[^#]*)?$/);
  await page.evaluate(() => history.back());
  await expect(page).toHaveURL(/\/recipe-editor\/new$/);
  const name = page.getByLabel('Recipe name', { exact: true });
  await name.fill('Cancelled destination must never reopen');
  const source = await name.evaluate((element: HTMLInputElement) => {
    element.setSelectionRange(2, 9, 'backward');
    return { href: location.href, key: (window as any).navigation.currentEntry.key, length: history.length };
  });
  await page.evaluate(sourceKey => {
    const navigation = (window as any).navigation;
    const traverseTo = navigation.traverseTo.bind(navigation);
    (window as any).__sourceRestorationCalls = [];
    navigation.traverseTo = (key: string, options?: unknown) => {
      (window as any).__sourceRestorationCalls.push(key);
      const calls = (window as any).__sourceRestorationCalls.length;
      if (key === sourceKey && calls === 1) {
        return { committed: Promise.reject(new DOMException('Initial controlled source failure', 'AbortError')), finished: Promise.reject(new DOMException('Initial controlled source failure', 'AbortError')) };
      }
      if (key === sourceKey && calls === 2) {
        const held = new Promise((_, reject) => { (window as any).__rejectHeldSourceRestoration = reject; });
        return { committed: held, finished: held };
      }
      return traverseTo(key, options);
    };
  }, source.key);
  await page.evaluate(() => history.forward());
  const guard = page.locator('#workspace-draft-warning');
  const retry = guard.getByRole('button', { name: 'Retry original history restoration', exact: true });
  await expect(retry).toBeVisible();
  await retry.click();
  await expect.poll(() => page.evaluate(() => (window as any).__sourceRestorationCalls.length)).toBe(2);
  await guard.getByRole('button', { name: 'Stay', exact: true }).click();
  await expect(guard).not.toBeVisible();
  if (successor === 'new destination') {
    await page.getByRole('link', { name: 'Recipes', exact: true }).first().click();
    await expect(guard).toBeVisible();
  } else if (successor === 'new focus') {
    await page.getByRole('button', { name: /^Account menu/ }).focus();
  }
  await page.evaluate(() => (window as any).__rejectHeldSourceRestoration(new DOMException('Controlled failure after Stay', 'AbortError')));
  // A settled animation frame follows the real rejected-promise continuation.
  await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => resolve())));
  if (successor !== 'source-only retry') {
    await expect(guard.getByText(/cancelled destination will not be reopened/i)).toHaveCount(0);
    await expect(retry).toHaveCount(0);
    if (successor === 'new destination') await expect(guard).toBeVisible();
    else {
      await expect(guard).not.toBeVisible();
      await expect(page.getByRole('button', { name: /^Account menu/ })).toBeFocused();
    }
    expect(await page.evaluate(() => (window as any).__sourceRestorationCalls)).toEqual([source.key, source.key]);
    return;
  }
  await expect(guard.getByText(/cancelled destination will not be reopened/i)).toBeVisible();
  await expect(retry).toBeVisible();
  await retry.click();
  await expect(page).toHaveURL(source.href);
  await expect(guard).not.toBeVisible();
  await expect(name).toHaveValue('Cancelled destination must never reopen');
  await expect(name).toBeFocused();
  expect(await page.evaluate(() => ({ key: (window as any).navigation.currentEntry.key, length: history.length, calls: (window as any).__sourceRestorationCalls })))
    .toEqual({ key: source.key, length: source.length, calls: [source.key, source.key, source.key] });
  expect(await name.evaluate((element: HTMLInputElement) => ({ start: element.selectionStart, end: element.selectionEnd, direction: element.selectionDirection })))
    .toEqual({ start: 2, end: 9, direction: 'backward' });
});
}

// KNOWN LIMITATION (KRA-46, accepted by Jason): with the Navigation API masked before
// app startup, dirty editor, Forward, Stay leaves the URL on Settings instead of
// restoring the editor URL. Kept visible as fixme; no history workaround is applied.
test.fixme('feature-disabled history must preserve actual Forward Stay entry and draft position', async ({ page }) => {
  // Disable before app initialization. This exercises production's absent-native-
  // entry branch, not a copied guard or a patched History implementation.
  await page.addInitScript(() => Object.defineProperty(window, 'navigation', { value: undefined, configurable: true }));
  await page.goto('/recipes');
  await page.getByRole('link', { name: 'New recipe', exact: true }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toBeVisible();
  await openSettings(page);
  await expect(page).toHaveURL(/\/settings\/profile(\?return=[^#]*)?$/);
  await page.evaluate(() => history.back());
  await expect(page).toHaveURL(/\/recipe-editor\/new$/);
  const name = page.getByLabel('Recipe name', { exact: true });
  await name.fill('Unsupported history acceptance is not a banner PASS');
  const before = await name.evaluate((element: HTMLInputElement) => {
    element.setSelectionRange(3, 8, 'backward');
    return { href: location.href, length: history.length, value: element.value };
  });
  await page.evaluate(() => history.forward());
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes', exact: true });
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Stay', exact: true }).click();
  await expect(page).toHaveURL(before.href);
  await expect(name).toHaveValue(before.value);
  expect(await name.evaluate((element: HTMLInputElement) => ({ length: history.length, start: element.selectionStart, end: element.selectionEnd, direction: element.selectionDirection })))
    .toEqual({ length: before.length, start: 3, end: 8, direction: 'backward' });
});

test('an unconfirmed recipe save cannot discard its identity before save and leave checks the original result', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  const title = `Unconfirmed guarded recipe ${Date.now()}`;
  await page.getByLabel('Recipe name', { exact: true }).fill(title);
  const requests: string[] = [];
  await page.route('**/leptos-api/save_recipe*', async route => {
    requests.push(route.request().postData() ?? '');
    if (requests.length === 1) {
      const response = await route.fetch();
      expect(response.ok()).toBeTruthy();
      expect(db.exec(`SELECT COUNT(*) FROM recipes WHERE name = '${title}' AND native_head_revision = 1`)).toBe('1');
      await route.abort('failed');
      return;
    }
    await route.continue();
  });
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Save failed' })).toBeVisible();
  expect(requests).toHaveLength(1);
  await openSettings(page);
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes', exact: true });
  await expect(guard).toBeVisible();
  // The transport has finished, but the server already committed. Local discard
  // must not erase the request and let an identical create become a fresh retry.
  await expect(guard.getByRole('button', { name: 'Discard and leave', exact: true })).toBeDisabled();
  await guard.getByRole('button', { name: 'Save and leave', exact: true }).click();
  await expect(page).toHaveURL(/\/settings\/profile(\?return=[^#]*)?$/);
  expect(requests).toHaveLength(2);
  expect(requests[1]).toBe(requests[0]);
  expect(db.exec(`SELECT COUNT(*) FROM recipes WHERE name = '${title}' AND native_head_revision = 1`)).toBe('1');
});

// ---- KRA-34: actionable diagnostics, stable field jumps and settled announcements ----

// The equipment select lists a page of profiles; once a workspace holds more (the pagination spec
// creates over a hundred), later ones are behind "Load more profiles".
async function chooseEquipmentProfile(page: import('@playwright/test').Page, label: string) {
  const select = page.locator('#equipment-profile-select');
  const option = select.getByRole('option', { name: label, exact: true });
  await expect.poll(async () => {
    if (await option.count()) return true;
    const more = page.getByRole('button', { name: 'Load more profiles', disabled: false });
    if (await more.count()) await more.click();
    return false;
  }, { message: `profile ${label} never appeared in the list` }).toBe(true);
  await select.selectOption({ label });
}

async function addHop(page: import('@playwright/test').Page, name: string, minutes: string) {
  await page.getByRole('button', { name: 'Add hop', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Add hop', exact: true });
  await dialog.getByLabel('Name', { exact: true }).fill(name);
  await dialog.getByLabel('Amount (g)', { exact: true }).fill('20');
  await dialog.getByLabel('Use', { exact: true }).selectOption('boil');
  await dialog.getByLabel('Contact time (min)', { exact: true }).fill(minutes);
  await dialog.getByRole('button', { name: 'Add to recipe', exact: true }).click();
  await expect(dialog).toBeHidden();
}

const focusedId = (page: import('@playwright/test').Page) => page.evaluate(() => document.activeElement?.id ?? '');
const issues = (page: import('@playwright/test').Page) => page.getByRole('region', { name: 'Recipe issues', exact: true });

test('diagnostic summary names each repeated hop and jumps to its hidden alpha field by row id', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await addHop(page, 'Cascade', '60');
  await addHop(page, 'Cascade', '30');
  const alphaLinks = issues(page).getByRole('button', { name: /Alpha acid is missing/ });
  await expect(alphaLinks).toHaveCount(2);
  const texts = await alphaLinks.allTextContents();
  expect(new Set(texts).size, `two rows, two distinct messages: ${texts}`).toBe(2);
  expect(texts[0]).toMatch(/60 min, 1 of 2/);
  expect(texts[1]).toMatch(/30 min, 2 of 2/);
  // The control is in the collapsed "Addition details" of its row before the jump.
  expect(await page.locator('[id^="hops-"][id$="-yield-alpha"]').first().evaluate(el => (el.closest('details') as HTMLDetailsElement).open)).toBe(false);
  await alphaLinks.nth(1).click();
  await expect.poll(() => focusedId(page)).toMatch(/^hops-.+-yield-alpha$/);
  const second = await focusedId(page);
  expect(await page.locator(`#${second}`).evaluate(el => (el.closest('details') as HTMLDetailsElement).open)).toBe(true);
  expect(await page.locator(`#${second}`).evaluate(el => el.closest('[role=group]')?.getAttribute('aria-label') ?? '')).toMatch(/30 min, 2 of 2/);
  await expect(page.locator(`#${second}`)).toHaveAttribute('aria-describedby', `${second}-description`);
  await expect(page.locator(`#${second}-description`)).toHaveText('Needed to start a brew');
  // Removing the first row changes every position; the remaining entry still reaches the same control.
  await page.getByRole('button', { name: /^Remove Cascade, hop, Boil, 60 min/ }).click();
  await expect(alphaLinks).toHaveCount(1);
  await alphaLinks.click();
  await expect.poll(() => focusedId(page)).toBe(second);
  await expect(alphaLinks).toHaveText(/1 of 1/);
});

test('diagnostic jump reaches a select and a field inside a collapsed section', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await addHop(page, 'Saaz', '45');
  const row = page.getByRole('group', { name: /^Saaz, hop/ });
  await row.getByLabel('Use', { exact: true }).selectOption('');
  const useLink = issues(page).getByRole('button', { name: /Use needs a supported use/ });
  await expect(useLink).toBeVisible();
  await useLink.click();
  await expect.poll(() => focusedId(page)).toMatch(/^hops-.+-stage$/);
  const select = page.locator(`#${await focusedId(page)}`);
  await expect(select).toHaveAttribute('aria-describedby', /-stage-description$/);
  await expect(page.locator(`#${await focusedId(page)}-description`)).toContainText('Choose a supported stage');
  expect(await page.locator('#recipe-equipment-disclosure').evaluate(el => (el as HTMLDetailsElement).open)).toBe(false);
  await issues(page).getByRole('button', { name: /Chilling is missing/ }).click();
  await expect.poll(() => focusedId(page)).toBe('recipe-chilling');
  expect(await page.locator('#recipe-equipment-disclosure').evaluate(el => (el as HTMLDetailsElement).open)).toBe(true);
  await expect(page.locator('#recipe-chilling')).toHaveAttribute('aria-describedby', 'recipe-chilling-description');
  await expect(page.locator('#recipe-chilling-description')).toContainText('Needed to start a brew');
});

test('repeated diagnostics clear one at a time while the summary keeps its place during typing', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await addHop(page, 'Cascade', '60');
  await addHop(page, 'Cascade', '30');
  const alphaLinks = issues(page).getByRole('button', { name: /Alpha acid is missing/ });
  await expect(alphaLinks).toHaveCount(2);
  await page.evaluate(() => {
    (window as any).__summaryRemovals = 0;
    new MutationObserver(records => records.forEach(record => record.removedNodes.forEach(node => {
      if (node instanceof Element && (node.matches('.k-diagnostics') || node.querySelector('.k-diagnostics'))) (window as any).__summaryRemovals++;
    }))).observe(document.body, { childList: true, subtree: true });
  });
  const alpha = page.getByLabel('Alpha acid (%)');
  await page.locator('[id^="hops-"][id$="-yield-alpha"]').first().evaluate(el => (el.closest('details') as HTMLDetailsElement).open = true);
  await alpha.first().focus();
  await page.keyboard.type('5.', { delay: 120 });
  const settledCheck = page.waitForResponse(response => response.url().includes('/preview_formulation') && response.ok());
  await page.keyboard.type('5', { delay: 120 });
  await expect(alphaLinks).toHaveCount(1);
  await expect(alpha.first()).toHaveValue('5.5');
  expect(await alpha.first().evaluate((el: HTMLInputElement) => [el.selectionStart, el.selectionEnd]), 'the caret stays where typing left it while the hint drops').toEqual([3, 3]);
  await expect(alphaLinks).toHaveText(/30 min, 2 of 2/);
  await settledCheck;
  await expect(page.getByRole('status').filter({ hasText: /^Incomplete: \d+ inputs? needed to start a brew$/ })).toBeVisible();
  expect(await page.evaluate(() => (window as any).__summaryRemovals), 'typing must not tear the summary down').toBe(0);
  await page.locator('[id^="hops-"][id$="-yield-alpha"]').nth(1).evaluate(el => (el.closest('details') as HTMLDetailsElement).open = true);
  await alpha.nth(1).fill('4.5');
  await expect(alphaLinks).toHaveCount(0);
});

test('invalid save blockers are separate from start requirements and Review highlighted fields reaches the first one', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(`Diagnostics ${Date.now()}`);
  await addHop(page, 'Cascade', '60');
  await page.getByLabel('Batch volume (L)').fill('-5');
  await expect(page.getByRole('status').filter({ hasText: 'Invalid: 1 value to correct' })).toBeVisible();
  await expect(issues(page).getByRole('heading', { name: /^Correct before saving \(1\)$/ })).toBeVisible();
  await expect(issues(page).getByRole('heading', { name: /^Needed to start a brew \(\d+\)$/ })).toBeVisible();
  await page.getByLabel('Recipe name', { exact: true }).focus();
  await page.getByRole('button', { name: 'Review highlighted fields', exact: true }).click();
  await expect.poll(() => focusedId(page)).toBe('recipe-batch');
  await expect(page.locator('#recipe-batch')).toHaveAttribute('aria-invalid', 'true');
  await expect(page.locator('#recipe-batch-description')).toHaveText('Enter a non-negative value');
  await page.getByLabel('Batch volume (L)').fill('20');
  await expect(page.getByRole('button', { name: 'Review highlighted fields', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  // Saved, and still Incomplete: the start requirements are listed, not hidden or promoted to errors.
  await expect(issues(page).getByRole('heading', { name: /^Needed to start a brew/ })).toBeVisible();
  await expect(issues(page).getByRole('heading', { name: /^Correct before saving/ })).toHaveCount(0);
  await expect(page.getByText('Starting needs a complete recipe', { exact: false })).toBeVisible();
});

test('preview results never move focus or collapse sections and the live status carries only the count', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await addHop(page, 'Cascade', '60');
  await addHop(page, 'Cascade', '30');
  await issues(page).getByRole('button', { name: /60 min, 1 of 2: Alpha acid is missing/ }).click();
  await expect.poll(() => focusedId(page)).toMatch(/-yield-alpha$/);
  const id = await focusedId(page);
  await page.evaluate(() => {
    (window as any).__statusTexts = [];
    const watch = () => (window as any).__statusTexts.push([...document.querySelectorAll('[role=status]')].map(node => node.textContent ?? '').join('|'));
    new MutationObserver(watch).observe(document.body, { childList: true, subtree: true, characterData: true });
  });
  await page.keyboard.type('5.5', { delay: 150 });
  const settledCheck = page.waitForResponse(response => response.url().includes('/preview_formulation') && response.ok());
  await page.keyboard.type('5', { delay: 150 });
  await settledCheck;
  await expect(page.getByRole('status').filter({ hasText: /^Incomplete: \d+ inputs? needed to start a brew$/ })).toBeVisible();
  expect(await focusedId(page)).toBe(id);
  expect(await page.locator(`#${id}`).evaluate(el => (el.closest('details') as HTMLDetailsElement).open)).toBe(true);
  const spoken: string[] = await page.evaluate(() => (window as any).__statusTexts);
  expect(spoken.some(text => /Alpha acid is missing/.test(text)), 'the detailed list is not in any live region').toBe(false);
  await expect(page.locator('.k-diagnostics [role=status], .k-diagnostics [aria-live], .k-diagnostics [role=alert]')).toHaveCount(0);
  await expect(page.getByRole('status').filter({ hasText: /^Incomplete: \d+ inputs? needed to start a brew$/ })).toBeVisible();
});

test('preserved data this editor cannot edit is explained and never linked', async ({ page }) => {
  const title = `Legacy diagnostics ${Date.now()}`;
  const xml = `<RECIPES><RECIPE><NAME>${title}</NAME><VERSION>1</VERSION><BATCH_SIZE>20</BATCH_SIZE><BOIL_TIME>60</BOIL_TIME><FERMENTABLES><FERMENTABLE><NAME>Pale malt</NAME><VERSION>1</VERSION><TYPE>Grain</TYPE><AMOUNT>4</AMOUNT><YIELD>80</YIELD><COLOR>3</COLOR></FERMENTABLE></FERMENTABLES><YEASTS><YEAST><NAME>Lager yeast</NAME><VERSION>1</VERSION><TYPE>Lager</TYPE><FORM>Dry</FORM><AMOUNT>0.011</AMOUNT></YEAST></YEASTS></RECIPE></RECIPES>`;
  await page.goto('/beerxml');
  await page.getByLabel('Paste BeerXML').fill(xml);
  await page.locator('#beerxml-colour').selectOption('srm');
  await page.locator('#beerxml-efficiency').selectOption('override');
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await page.getByRole('button', { name: 'Import recipe' }).click();
  await page.getByRole('link', { name: 'Open imported recipe' }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(title);
  const note = issues(page).getByText(/process data this app cannot edit/);
  await expect(note).toBeVisible();
  await expect(issues(page).getByRole('button', { name: /process data this app cannot edit/ })).toHaveCount(0);
  await expect(issues(page).getByRole('link', { name: /process data this app cannot edit/ })).toHaveCount(0);
});

for (const { width, scheme } of [{ width: 320, scheme: 'light' }, { width: 390, scheme: 'dark' }, { width: 1440, scheme: 'light' }] as const) {
  test(`diagnostic summary stays in bounds with long names and whole touch targets at ${width}px ${scheme}`, async ({ page }) => {
    await page.setViewportSize({ width, height: 800 });
    await page.emulateMedia({ colorScheme: scheme });
    await page.goto('/recipe-editor/new');
    await addHop(page, 'A very long hop variety name that must wrap rather than overflow the narrow summary list', '60');
    await addHop(page, 'Cascade', '30');
    const links = issues(page).getByRole('button', { name: /Alpha acid is missing/ });
    await expect(links).toHaveCount(2);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
    for (const link of await links.all()) {
      const box = (await link.boundingBox())!;
      expect(box.height, 'whole target at least 44px tall').toBeGreaterThanOrEqual(44);
      expect(box.x + box.width).toBeLessThanOrEqual(width);
    }
    expect(await links.first().evaluate(el => getComputedStyle(el).whiteSpace)).toBe('normal');
    await links.first().click();
    await expect.poll(() => focusedId(page)).toMatch(/-yield-alpha$/);
    const focused = (await page.locator(`#${await focusedId(page)}`).boundingBox())!;
    expect(focused.x).toBeGreaterThanOrEqual(0);
    expect(focused.x + focused.width).toBeLessThanOrEqual(width);
  });
}

test('Review highlighted fields does not appear on a first character and reaches a settled invalid field', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  const review = page.getByRole('button', { name: 'Review highlighted fields', exact: true });
  await page.evaluate(() => {
    (window as any).__reviewSeen = 0;
    const look = () => { if ([...document.querySelectorAll('button')].some(b => b.textContent === 'Review highlighted fields')) (window as any).__reviewSeen++; };
    new MutationObserver(look).observe(document.body, { childList: true, subtree: true, characterData: true });
  });
  await page.getByLabel('Batch volume (L)').focus();
  await page.keyboard.press('-');
  await expect(page.getByText('Complete invalid number fields to see estimates.')).toBeVisible();
  expect(await page.evaluate(() => (window as any).__reviewSeen), 'a lone "-" is transitional, not a highlighted field').toBe(0);
  await expect(review).toHaveCount(0);
  await page.keyboard.press('Tab');
  await expect(page.locator('#recipe-batch')).toHaveAttribute('aria-invalid', 'true');
  await expect(review).toBeVisible();
  await page.getByLabel('Recipe name', { exact: true }).focus();
  await review.click();
  await expect.poll(() => focusedId(page)).toBe('recipe-batch');
  await expect(page.locator('#recipe-batch')).toHaveAttribute('aria-describedby', 'recipe-batch-description recipe-batch-note');
  await expect(page.locator('#recipe-batch-description')).toHaveText('Enter a number');
});

for (const width of [1440, 390]) {
  test(`Review highlighted fields keeps the button's natural width, left-aligned with the header fields at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto('/recipe-editor/new');
    await page.getByLabel('Batch volume (L)').fill('-5');
    const review = page.getByRole('button', { name: 'Review highlighted fields', exact: true });
    await expect(review).toBeVisible();
    // Natural width: forcing max-content changes nothing, and it never spans the header.
    const measured = await review.evaluate(el => {
      const own = el.getBoundingClientRect();
      const previous = (el as HTMLElement).style.width;
      (el as HTMLElement).style.width = 'max-content';
      const natural = el.getBoundingClientRect().width;
      (el as HTMLElement).style.width = previous;
      const header = el.closest('header')!.getBoundingClientRect();
      const name = document.getElementById('recipe-title')!.getBoundingClientRect();
      return { own: own.width, left: own.left, natural, header: header.width, nameLeft: name.left };
    });
    expect(Math.abs(measured.own - measured.natural), `width ${measured.own} vs natural ${measured.natural}`).toBeLessThanOrEqual(1);
    expect(measured.own).toBeLessThan(measured.header / 2);
    expect(Math.abs(measured.left - measured.nameLeft)).toBeLessThanOrEqual(1);
  });
}

test('Review highlighted fields started from Saved versions selects the Recipe tab and focuses the exact control', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Batch volume (L)').fill('-5');
  await page.getByLabel('Recipe name', { exact: true }).focus();
  const review = page.getByRole('button', { name: 'Review highlighted fields', exact: true });
  await expect(review).toBeVisible();
  await page.getByRole('tab', { name: 'Saved versions', exact: true }).click();
  await expect(page.getByRole('tab', { name: 'Saved versions', exact: true })).toHaveAttribute('aria-selected', 'true');
  await review.click();
  await expect(page.getByRole('tab', { name: 'Recipe', exact: true })).toHaveAttribute('aria-selected', 'true');
  await expect.poll(() => focusedId(page)).toBe('recipe-batch');
});

test('Review highlighted fields never blinks and nothing moves while another field is edited with an Invalid outstanding', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Batch volume (L)').fill('-5');
  await page.getByLabel('Recipe name', { exact: true }).focus();
  const review = page.getByRole('button', { name: 'Review highlighted fields', exact: true });
  await expect(review).toBeVisible();
  await expect(issues(page).getByRole('button', { name: 'Batch volume cannot be negative.', exact: true })).toBeVisible();
  await page.locator('#recipe-boil').focus();
  // Sample every frame: the command is present and the tab strip below it never moves (measured from the command, so scrolling of the page does not count).
  await page.evaluate(() => {
    const w = window as any;
    w.__frames = 0; w.__missing = 0; w.__tops = new Set<number>();
    const tick = () => {
      const button = [...document.querySelectorAll('button')].find(b => b.textContent === 'Review highlighted fields');
      const tabs = document.querySelector('[role=tablist]');
      w.__frames++;
      if (!button) w.__missing++;
      if (tabs && button) w.__tops.add(Math.round(tabs.getBoundingClientRect().top - button.getBoundingClientRect().top));
      w.__raf = requestAnimationFrame(tick);
    };
    w.__raf = requestAnimationFrame(tick);
  });
  for (const key of ['6', '0', '1', '2', '3']) {
    await page.keyboard.press(key);
    await page.waitForTimeout(120);
  }
  await expect(page.locator('#recipe-boil')).toHaveValue('60123');
  await page.waitForTimeout(1500);
  const seen = await page.evaluate(() => { const w = window as any; cancelAnimationFrame(w.__raf); return { frames: w.__frames, missing: w.__missing, tops: [...w.__tops] }; });
  expect(seen.frames).toBeGreaterThan(30);
  expect(seen.missing, 'the Review command was absent in some frame').toBe(0);
  expect(seen.tops, `the tab strip moved: ${seen.tops}`).toHaveLength(1);
  await review.click();
  await expect.poll(() => focusedId(page)).toBe('recipe-batch');
});

test('a link to a conditional control goes to the control that governs it, never to an absent input', async ({ page }) => {
  // Mash water follows the thickness method, so the Mash water field is not on the page.
  const mashProfile = `Thickness ${Date.now()}`;
  await createEquipmentProfile(page, mashProfile, PROFILE_VALUES, 'natural');
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Batch volume (L)').fill('1');
  await page.locator('#recipe-boil').fill('60');
  await page.getByRole('button', { name: 'Add fermentable' }).click();
  await page.getByLabel('Search fermentables').fill('Joe White Signature');
  await page.getByRole('button', { name: /Joe White Signature Lager/ }).click();
  await page.getByLabel('Amount in recipe (kg)').fill('4');
  await page.getByLabel('Build your grain bill').getByRole('button', { name: 'Add to recipe' }).click();
  await page.getByText('Equipment settings', { exact: true }).click();
  await chooseEquipmentProfile(page, mashProfile);
  await page.locator('#recipe-mash-method').selectOption('profile_thickness');
  await expect(page.locator('#recipe-mash-water')).toHaveCount(0);
  const sparge = issues(page).getByRole('button', { name: /Mash water is more than the water available/ });
  await expect(sparge).toBeVisible();
  await sparge.click();
  await expect.poll(() => focusedId(page)).toBe('recipe-mash-method');
  await expect(page.locator('#recipe-mash-method')).toBeFocused();
});

test('a link to an inherited chilling value goes to the Chilling override, never to an absent input', async ({ page }) => {
  // An inherited forced chilling with no rate: the rate field only exists once Chilling is overridden.
  const profile = `Forced ${Date.now()}`;
  await createEquipmentProfile(page, profile, PROFILE_VALUES, 'forced');
  await page.goto('/recipe-editor/new');
  await addHop(page, 'Cascade', '60');
  await page.getByText('Equipment settings', { exact: true }).click();
  await chooseEquipmentProfile(page, profile);
  await page.getByText('Equipment settings', { exact: true }).click();
  expect(await page.locator('#recipe-equipment-disclosure').evaluate(el => (el as HTMLDetailsElement).open)).toBe(false);
  await expect(page.locator('#recipe-chilling-rate')).toHaveCount(0);
  const rate = issues(page).getByRole('button', { name: /Cooling rate is missing/ });
  await expect(rate).toBeVisible();
  await rate.click();
  await expect.poll(() => focusedId(page)).toBe('recipe-chilling');
  expect(await page.locator('#recipe-equipment-disclosure').evaluate(el => (el as HTMLDetailsElement).open)).toBe(true);
  await page.locator('#recipe-chilling').selectOption('forced');
  await expect(page.locator('#recipe-chilling-rate')).toBeVisible();
  await expect(issues(page).getByRole('button', { name: /Cooling rate is missing/ })).toBeVisible();
  await issues(page).getByRole('button', { name: /Cooling rate is missing/ }).click();
  await expect.poll(() => focusedId(page)).toBe('recipe-chilling-rate');
});

test('inline messages are associated with every field family', async ({ page }) => {
  await recipeWithNumbers(page);
  const described = async (id: string, state: 'true' | 'false', text: RegExp | string) => {
    const field = page.locator(`[id="${id}"]`);
    await expect(field).toHaveAttribute('aria-invalid', state);
    await expect(field).toHaveAttribute('aria-describedby', new RegExp(`${id}-description`));
    await expect(page.locator(`[id="${id}-description"]`)).toContainText(text);
  };
  // Mash method select (start requirement, not an error) once the method is cleared.
  await page.locator('#recipe-mash-method').selectOption('');
  await described('recipe-mash-method', 'false', 'Choose a mash water method');
  // Fermentable stage and type selects.
  const row = page.getByRole('group', { name: /^Joe White Signature Lager/ });
  await row.getByLabel('Stage', { exact: true }).selectOption('');
  await expect(row.locator('[id$="-stage"]')).toHaveAttribute('aria-describedby', /-stage-description$/);
  await expect(page.locator(`#${await row.locator('[id$="-stage"]').getAttribute('id')}-description`)).toContainText('Choose a supported stage');
  // Fermentable type select: an extract in the Mash stage is a real mismatch.
  await row.getByLabel('Stage', { exact: true }).selectOption('mash');
  await row.getByText('Addition details').click();
  const type = row.locator('[id$="-type"]');
  await type.selectOption('extract');
  const typeId = (await type.getAttribute('id'))!;
  await expect(type).toHaveAttribute('aria-describedby', new RegExp(`${typeId}-description$`));
  await expect(page.locator(`[id="${typeId}-description"]`)).toContainText('Choose a type that matches the stage');
  await issues(page).getByRole('button', { name: /needs a type that matches its stage/ }).click();
  await expect.poll(() => focusedId(page)).toBe(typeId);
  // A mash step field and an equipment override field report a real error.
  await page.locator('#recipe-section-mash').getByRole('button', { name: 'Add step', exact: true }).click();
  const stepTime = page.locator('#recipe-section-mash').getByLabel('Time (min)', { exact: true });
  await stepTime.fill('-5');
  const stepId = await stepTime.getAttribute('id');
  await described(stepId!, 'true', 'Enter a non-negative value');
  await page.locator('#recipe-boil-off').fill('-1');
  await described('recipe-boil-off', 'true', 'Enter a non-negative value');
  // Yeast and other-ingredient amounts, where the raw and validator paths differ.
  for (const [opener, dialogName, name] of [['Add yeast', 'Add yeast', 'US-05'], ['Add other ingredient', 'Add other ingredient', 'Irish moss']] as const) {
    await page.getByRole('button', { name: opener, exact: true }).click();
    const dialog = page.getByRole('dialog', { name: dialogName, exact: true });
    await dialog.getByLabel('Name', { exact: true }).fill(name);
    await dialog.getByLabel('Amount', { exact: true }).fill('1');
    await dialog.getByRole('button', { name: 'Add to recipe', exact: true }).click();
    const amount = page.getByRole('group', { name: new RegExp(`^${name}`) }).getByLabel('Amount', { exact: true });
    await amount.fill('-1');
    await described((await amount.getAttribute('id'))!, 'true', 'Enter a non-negative value');
    await amount.fill('2');
    await expect(amount).toHaveAttribute('aria-invalid', 'false');
  }
});

test('diagnostics from an earlier draft are marked out of date and never counted while previews are paused', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await page.locator('#recipe-boil').fill('60');
  await addHop(page, 'Cascade', '90');
  await expect(issues(page).getByRole('button', { name: /is longer than the boil time/ })).toBeVisible();
  await expect(page.getByTestId('diagnostics-stale')).toHaveText('');
  // A malformed value pauses previews, so the hop's entry cannot be rechecked after the boil time changes.
  await page.locator('#recipe-boil').fill('120');
  const volume = page.getByLabel('Batch volume (L)');
  await volume.fill('7x');
  await page.keyboard.press('Tab');
  await expect(page.getByText('Complete invalid number fields to see estimates.')).toBeVisible();
  await expect(page.getByTestId('diagnostics-stale')).toContainText('Out of date');
  await expect(issues(page).getByRole('button', { name: /is longer than the boil time/ })).toBeVisible();
  await page.getByRole('button', { name: 'Review highlighted fields', exact: true }).click();
  await expect.poll(() => focusedId(page), 'Review goes to the settled malformed field, not the stale hop entry').toBe('recipe-batch');
  await volume.fill('20');
  await expect(page.getByTestId('diagnostics-stale')).toHaveText('');
  await expect(issues(page).getByRole('button', { name: /is longer than the boil time/ })).toHaveCount(0);
});

test('an empty draft links Add at least one fermentable to the add control', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).focus();
  await issues(page).getByRole('button', { name: 'Add at least one fermentable.', exact: true }).click();
  await expect.poll(() => focusedId(page)).toBe('fermentables-add');
});

test('summary links reach a step field, a select, an override and yeast and other amounts by their exact ids', async ({ page }) => {
  await recipeWithNumbers(page);
  // A mash step with a negative time.
  await page.locator('#recipe-section-mash').getByRole('button', { name: 'Add step', exact: true }).click();
  const stepTime = page.locator('#recipe-section-mash').getByLabel('Time (min)', { exact: true });
  await stepTime.fill('-5');
  const stepId = (await stepTime.getAttribute('id'))!;
  await issues(page).getByRole('button', { name: /^Mash step 1 of 1: Time cannot be negative\.$/ }).click();
  await expect.poll(() => focusedId(page)).toBe(stepId);
  // A fermentable stage select.
  const row = page.getByRole('group', { name: /^Joe White Signature Lager/ });
  const stageId = (await row.locator('[id$="-stage"]').getAttribute('id'))!;
  await row.getByLabel('Stage', { exact: true }).selectOption('');
  await issues(page).getByRole('button', { name: /Stage needs a supported stage/ }).click();
  await expect.poll(() => focusedId(page)).toBe(stageId);
  // An equipment override.
  await page.locator('#recipe-boil-off').fill('-1');
  await issues(page).getByRole('button', { name: /Boil-off cannot be negative/ }).click();
  await expect.poll(() => focusedId(page)).toBe('recipe-boil-off');
  expect(await page.locator('#recipe-equipment-disclosure').evaluate(el => (el as HTMLDetailsElement).open)).toBe(true);
  // Yeast and other-ingredient amounts, whose raw and validator paths differ.
  for (const [opener, dialogName, name] of [['Add yeast', 'Add yeast', 'US-05'], ['Add other ingredient', 'Add other ingredient', 'Irish moss']] as const) {
    await page.getByRole('button', { name: opener, exact: true }).click();
    const dialog = page.getByRole('dialog', { name: dialogName, exact: true });
    await dialog.getByLabel('Name', { exact: true }).fill(name);
    await dialog.getByLabel('Amount', { exact: true }).fill('1');
    await dialog.getByRole('button', { name: 'Add to recipe', exact: true }).click();
    const amount = page.getByRole('group', { name: new RegExp(`^${name}`) }).getByLabel('Amount', { exact: true });
    const amountId = (await amount.getAttribute('id'))!;
    await amount.fill('-1');
    await issues(page).getByRole('button', { name: new RegExp(`${name}.*Amount cannot be negative`) }).click();
    await expect.poll(() => focusedId(page)).toBe(amountId);
  }
});

for (const scheme of ['light', 'dark'] as const) {
  test(`an out-of-date issues list keeps at least 4.5:1 text contrast in ${scheme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.goto('/design-system');
    if (scheme === 'dark') await page.evaluate(() => document.documentElement.classList.add('dark'));
    const list = page.locator('.k-diagnostics.k-stale--active .k-diagnostics__list').first();
    await expect(list).toBeVisible();
    const ratios = await list.evaluate(el => {
      const rgb = (css: string) => (css.match(/[\d.]+/g) ?? []).map(Number);
      const lum = ([r, g, b]: number[]) => { const f = (c: number) => { const v = c / 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
      // Effective colour: the text colour blended with the list's own opacity over the first opaque background.
      let opacity = 1;
      for (let node: Element | null = el; node; node = node.parentElement) opacity *= Number(getComputedStyle(node).opacity);
      let bg = [255, 255, 255];
      for (let node: Element | null = el; node; node = node.parentElement) {
        const c = rgb(getComputedStyle(node).backgroundColor);
        if (c.length === 3 || (c.length === 4 && c[3] > 0.99)) { bg = c.slice(0, 3); break; }
      }
      return [...el.querySelectorAll('li, button')].map(entry => {
        const fg = rgb(getComputedStyle(entry).color).slice(0, 3).map((c, i) => c * opacity + bg[i] * (1 - opacity));
        const [hi, lo] = [lum(fg), lum(bg)].sort((a, b) => b - a);
        return (hi + 0.05) / (lo + 0.05);
      });
    });
    expect(ratios.length).toBeGreaterThan(0);
    for (const ratio of ratios) expect(ratio, `contrast ${ratio}`).toBeGreaterThanOrEqual(4.5);
  });
}

test('wrapping belongs to the Wrap size: the showcase keeps ordinary labels on one line and wraps the Wrap link', async ({ page }) => {
  for (const width of [320, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    await page.goto('/design-system');
    const whiteSpace = (name: string | RegExp) => page.getByRole('button', { name }).first().evaluate(el => getComputedStyle(el).whiteSpace);
    expect(await whiteSpace('Add ingredient')).toBe('nowrap');
    expect(await whiteSpace('View history')).toBe('nowrap');
    const wrap = page.getByRole('button', { name: /^Wrap · a whole-row link/ });
    expect(await wrap.evaluate(el => getComputedStyle(el).whiteSpace)).toBe('normal');
    const box = (await wrap.boundingBox())!;
    expect(box.height).toBeGreaterThanOrEqual(44);
    expect(box.x + box.width).toBeLessThanOrEqual(width);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
  }
});
