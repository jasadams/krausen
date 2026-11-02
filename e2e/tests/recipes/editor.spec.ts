import { test, expect } from '@playwright/test';
import { DEFAULT_TEST_USER, createSecondUser, db, getDefaultUserId, getDefaultWorkspaceId, loginUser, seedWorkspace, walCheckpoint } from '../../helpers/test-helpers';

test.beforeEach(async ({ page }) => {
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
});

for (const unavailable of ['missing', 'disabled', 'idless'] as const) {
  test(`authorized recheck uses a visible safe successor for an ${unavailable} ordinary bookmark`, async ({ page }) => {
    await page.goto('/recipe-editor/new');
    await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
    const opener = unavailable === 'idless'
      ? page.getByRole('button', { name: 'Save recipe', exact: true })
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
  await page.getByText('Export and print', { exact: true }).click();
  await page.getByRole('button', { name: 'Preview unsaved draft', exact: true }).click();
  const print = page.getByRole('region', { name: 'Print preview', exact: true });
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
  await page.route('**/leptos-api/list_formulation_styles*', async route => {
    const response = await route.fetch();
    seen();
    await held;
    await route.fulfill({ response });
  });
  try {
    await page.goto('/recipe-editor/new');
    await started;
    await page.getByRole('link', { name: 'Settings', exact: true }).first().click();
    await expect(page).toHaveURL(/settings\/profile/);
    release();
    await page.unrouteAll({ behavior: 'wait' });
    await page.waitForLoadState('networkidle');
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
    await page.getByText('Export and print', { exact: true }).click();
    await page.getByRole('button', { name: 'Preview unsaved draft', exact: true }).click();
    const print = page.getByRole('region', { name: 'Print preview', exact: true });
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
  await page.getByRole('button', { name: 'Save recipe', exact: true }).click();
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
    await remote.getByRole('button', { name: 'Save recipe', exact: true }).click();
    await expect(remote.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
    let reads = 0;
    await page.route('**/leptos-api/read_recipe*', async route => {
      if (++reads > 1) { await route.continue(); return; }
      const response = await route.fetch();
      seen();
      await held;
      await route.fulfill(continuation === 'denied-after-edit' ? { response, json: 'Forbidden' } : { response });
    });
    await page.getByRole('button', { name: 'Save recipe', exact: true }).click();
    await started;
    await expect(page.getByRole('button', { name: 'Save recipe', exact: true })).toBeEnabled();
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
      await remote.getByRole('button', { name: 'Save recipe', exact: true }).click();
      await expect(remote.getByRole('status').filter({ hasText: 'Saved revision 3' })).toBeVisible();
      await page.evaluate(() => window.dispatchEvent(new Event('focus')));
      const conflict = page.getByRole('region', { name: 'Save conflict', exact: true });
      await expect(conflict).toContainText('the current saved revision is 3');
      const account = page.getByRole('button', { name: 'Account menu', exact: true });
      await account.focus();
      release();
      await page.unrouteAll({ behavior: 'wait' });
      await page.waitForLoadState('networkidle');
      await expect(conflict).toContainText('the current saved revision is 3');
      await expect(account).toBeFocused();
      expect(errors).toEqual([]);
      return;
    }
    await page.getByRole('link', { name: 'Settings', exact: true }).first().click();
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
    await page.getByRole('link', { name: 'Settings', exact: true }).first().click();
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
    await expect(page.locator('.recipe-editor__actions [role="status"]')).toContainText('Saved revision 1 in the original context.');
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
  await page.getByText('Export and print', { exact: true }).click();
  await page.getByRole('button', { name: 'Preview unsaved draft', exact: true }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toBeHidden();
  await expect(page.getByText('Access denied', { exact: true })).toBeVisible();
  await expect(page.getByText('Access verified', { exact: true })).toHaveCount(0);
});

test('successful revalidation preserves textarea selection and backward direction', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  await page.getByText('Recipe process and notes', { exact: true }).click();
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
  await page.route('**/leptos-api/get_user_context*', async route => {
    const response = await route.fetch();
    seen();
    await held;
    await route.fulfill({ response });
    completed();
  });
  try {
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await started;
    await expect(volume).toBeHidden();
    const account = page.getByRole('button', { name: 'Account menu', exact: true });
    await account.focus();
    release();
    await finished;
    await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
    await expect(account).toBeFocused();
  } finally { release(); }
});

test('a disposed editor ignores a delayed focus read without reactive panic', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  await page.getByLabel('Recipe name', { exact: true }).fill(`Disposed check ${Date.now()}`);
  await page.getByRole('button', { name: 'Save recipe', exact: true }).click();
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
    await page.getByRole('link', { name: 'Settings', exact: true }).first().click();
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
  await page.getByRole('button', { name: 'Save recipe', exact: true }).click();
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
    await remote.getByRole('button', { name: 'Save recipe', exact: true }).click();
    await expect(remote.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
    page.once('dialog', dialog => dialog.accept());
    await page.reload();
    await page.getByRole('button', { name: 'Restore draft', exact: true }).click();
    await page.getByRole('button', { name: 'Save recipe', exact: true }).click();
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
  await page.getByRole('tab', { name: 'History', exact: true }).click();
  await page.getByRole('tab', { name: 'Recipe', exact: true }).click();
  await page.getByText('Recipe process and notes', { exact: true }).click();
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
  const check = async (field: ReturnType<typeof page.getByLabel>, alreadySettled = false) => {
    await field.fill('-');
    await expect(field).toHaveValue('-');
    await expect(field).toHaveAttribute('aria-invalid', alreadySettled ? 'true' : 'false');
    await field.press('Tab');
    await expect(field).toHaveAttribute('aria-invalid', 'true');
    const description = await field.getAttribute('aria-describedby');
    expect(description).toBeTruthy();
    await expect(page.locator(`[id="${description}"]`)).toContainText('Enter a number');
    await field.fill('1.230000000000000');
    await field.press('Tab');
    await expect(field).toHaveValue('1.230000000000000');
    await expect(field).toHaveAttribute('aria-invalid', 'false');
  };
  await check(page.getByLabel('Batch volume (L)'));
  await page.getByRole('button', { name: 'Add fermentable', exact: true }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill', exact: true });
  await picker.locator('.k-catalogue-item').first().click();
  await picker.getByRole('button', { name: 'Add ingredient', exact: true }).click();
  await check(page.getByLabel('Amount (kg)', { exact: true }));
  const yeast = page.locator('.k-panel').filter({ has: page.getByRole('heading', { name: 'Yeast', exact: true }) });
  await yeast.getByRole('button', { name: 'Add ingredient', exact: true }).click();
  await check(yeast.getByLabel('Amount', { exact: true }));
  // The amount check's Tab lands here; refocusing amount to correct it already
  // blurs attenuation. Its feedback must now remain current while correcting.
  await check(yeast.getByLabel('Yeast attenuation (informational) (%)', { exact: true }), true);
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
  await picker.getByRole('button', { name: 'Add ingredient', exact: true }).click();
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
  await page.getByRole('button', { name: 'Save recipe', exact: true }).click();
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
  await page.getByText('Export and print', { exact: true }).click();
  await page.getByRole('button', { name: 'Preview unsaved draft', exact: true }).click();
  const preview = page.getByRole('region', { name: 'Print preview', exact: true });
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
    const save = page.getByRole('button', { name: 'Save recipe', exact: true });
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
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Complete invalid number fields before saving.' })).toBeVisible();
  await expect(volume).toHaveValue('-');
  await volume.fill('20');
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  await expect(page).toHaveURL(/\/recipe-editor\/(?!new)[^/]+$/);
  await page.reload();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(title);
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue('20');
  await page.getByText('Recipe process and notes', { exact: true }).click();
  await page.getByLabel('Whole-wort apparent attenuation (%)').fill('75');
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
  await page.reload();
  await page.getByText('Recipe process and notes', { exact: true }).click();
  await expect(page.getByLabel('Whole-wort apparent attenuation (%)')).toHaveValue('75');
  const id = page.url().split('/').pop()!;
  const before = Number(db.exec(`SELECT COUNT(*) FROM recipe_revisions WHERE recipe_id = '${id}'`));
  await page.getByRole('button', { name: 'Save recipe', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
  expect(Number(db.exec(`SELECT COUNT(*) FROM recipe_revisions WHERE recipe_id = '${id}'`))).toBe(before);
  expect(before).toBe(2);
  expect(db.exec(`SELECT json_extract(document, '$.boil_minutes') IS NULL FROM recipe_revisions WHERE recipe_id = '${id}' AND revision = 2`)).toBe('1');
});

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
  await page.addInitScript(() => localStorage.setItem('krausen-theme', 'dark'));
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('heading', { name: 'Recipe editor' })).toBeVisible();
  await expect(page.locator('html')).toHaveClass(/dark/);
  await page.getByRole('button', { name: 'Add fermentable' }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill' });
  await expect(picker).toBeVisible();
  await expect(page.getByLabel('Search fermentables')).toBeVisible();
  await picker.locator('.k-catalogue-item').first().click();
  await picker.getByRole('button', { name: 'Add ingredient', exact: true }).click();
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
    await page.addInitScript(theme => localStorage.setItem('krausen-theme', theme), setting.theme);
    await page.goto('/recipe-editor/new');
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
      await page.evaluate(value => localStorage.setItem('krausen-theme', value), theme);
      await page.goto('/design-system');
      await expect(page.getByRole('heading', { name: 'Good design, on repeat.' })).toBeVisible();
      // Existing login isolation initializes Light on navigation. Exercise the
      // real appearance control and assert the actual theme, not stored intent.
      if (await page.locator('html').evaluate(element => element.classList.contains('dark')) !== (theme === 'dark')) {
        if (width < 768) await page.getByRole('button', { name: 'Toggle menu', exact: true }).click();
        await page.getByRole('button', { name: theme === 'dark' ? 'Dark appearance' : 'Light appearance', exact: true }).click();
        if (width < 768) await page.keyboard.press('Escape');
      }
      await expect(page.locator('html')).toHaveClass(theme === 'dark' ? /\bdark\b/ : /^(?!.*\bdark\b).*$/);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      await expect(page.getByLabel('Amount (kg)', { exact: true }).and(page.locator('#show-invalid'))).toHaveAttribute('aria-invalid', 'true');
      await page.getByRole('tab', { name: 'Recipe' }).focus();
      await page.keyboard.press('ArrowRight');
      await expect(page.getByRole('tab', { name: 'History' })).toBeFocused();
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
  await page.getByRole('button', { name: 'Switch workspace', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Your workspaces', exact: true });
  await expect(dialog.getByText('Unable to load workspaces', { exact: true })).toBeVisible();
  await expect(dialog.getByRole('status').filter({ hasText: 'Unable to load workspaces' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
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
  await page.getByRole('button', { name: 'Switch workspace', exact: true }).click();
  await page.getByRole('button', { name: otherName }).click();
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
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(`Profile recipe ${Date.now()}`);
  await page.getByText('Equipment settings', { exact: true }).click();
  const name = `Profile ${Date.now()}`;
  await page.getByLabel('Profile name').fill(name);
  await page.getByLabel('Mash efficiency (%)').first().fill('75');
  await page.getByLabel('Boil-off (L/h)', { exact: true }).first().fill('3');
  await page.getByRole('button', { name: 'Save profile' }).click();
  const selector = page.getByLabel('Equipment profile', { exact: true });
  await expect(selector.getByRole('option', { name })).toBeAttached();
  await selector.selectOption({ label: name });
  const profileId = await selector.inputValue();
  await expect.poll(() => previews.some(result => result.Preview?.context?.equipment_profile?.profile_id === profileId && result.Preview.context.equipment_profile.revision === 1)).toBe(true);
  const originalDigest = previews.find(result => result.Preview?.context?.equipment_profile?.profile_id === profileId && result.Preview.context.equipment_profile.revision === 1).Preview.context_digest;
  await expect(page.getByText('Equipment profile unavailable')).toHaveCount(0);
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  await page.reload();
  await page.getByText('Equipment settings', { exact: true }).click();
  await expect(page.getByLabel('Equipment profile', { exact: true })).toHaveValue(profileId);
  const manage = page.getByLabel('Manage profile');
  await manage.selectOption({ label: name });
  await page.getByLabel('Mash efficiency (%)').first().fill('80');
  await page.getByRole('button', { name: 'Save profile' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Profile saved.' })).toBeVisible();
  await expect(page.getByLabel('Mash efficiency (%)').first()).toHaveValue('80');
  await expect.poll(() => previews.some(result => result.Preview?.context?.equipment_profile?.profile_id === profileId && result.Preview.context.equipment_profile.revision === 2 && result.Preview.context_digest !== originalDigest)).toBe(true);
  await page.getByRole('button', { name: 'Archive', exact: true }).click();
  await expect(page.getByText('Equipment profile unavailable', { exact: true })).toBeVisible();
  await expect.poll(() => previews.includes('UnresolvedProfile')).toBe(true);
  await page.getByRole('button', { name: 'Unarchive' }).click();
  await expect(page.getByText('Equipment profile unavailable', { exact: true })).toHaveCount(0);
});


test('an uncertain equipment save retries the identical request', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await page.getByText('Equipment settings', { exact: true }).click();
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
  await page.getByRole('button', { name: 'Retry previous request' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Profile saved.' })).toBeVisible();
  expect(requests).toHaveLength(2);
  expect(requests[1].request_id).toBe(requests[0].request_id);
  expect(requests[1].profile_id).toBe(requests[0].profile_id);
  const selector = page.getByLabel('Equipment profile', { exact: true });
  await expect(selector.getByRole('option', { name })).toHaveCount(1);
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
  await page.goto('/recipe-editor/new');
  await page.getByText('Equipment settings', { exact: true }).click();
  const manage = page.getByLabel('Manage profile');
  await expect(manage.getByRole('option', { name: 'Paged profile 101 (archived)' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Load more profiles' }).click();
  await expect(manage.getByRole('option', { name: 'Paged profile 101 (archived)' })).toBeAttached();
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
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Complete invalid number fields before saving.' })).toBeVisible();
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(title).toHaveValue('');
});

test('navigation offers stay, blocks failed save, and discards only on request', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill('Navigation draft');
  await page.getByLabel('Batch volume (L)').fill('-');
  await page.getByRole('link', { name: 'Settings', exact: true }).first().click();
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Save and leave' }).click();
  await expect(guard.getByRole('status')).toContainText('Complete invalid number fields');
  await expect(page).toHaveURL(/recipe-editor/);
  await guard.getByRole('button', { name: 'Stay' }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue('Navigation draft');
  await page.getByLabel('Batch volume (L)').fill('20');
  await page.route('**/leptos-api/save_recipe*', route => route.abort('failed'));
  await page.getByRole('link', { name: 'Settings', exact: true }).first().click();
  await guard.getByRole('button', { name: 'Save and leave' }).click();
  await expect(guard.getByRole('status')).toContainText('Save failed');
  await expect(page).toHaveURL(/recipe-editor/);
  await guard.getByRole('button', { name: 'Stay' }).click();
  await page.unroute('**/leptos-api/save_recipe*');
  await page.getByRole('link', { name: 'Settings', exact: true }).first().click();
  await guard.getByRole('button', { name: 'Discard and leave' }).click();
  await expect(page).toHaveURL(/settings\/profile/);
  await page.goto('/recipe-editor/new');
  await expect(page.getByText('A retained draft is available')).toHaveCount(0);
});

test('stale saves retain edits and require deliberate reapply against current head', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill('Conflict base');
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  const url = page.url();
  const other = await page.context().newPage();
  await other.goto(url);
  await other.getByLabel('Recipe name', { exact: true }).fill('Remote edit');
  await page.getByLabel('Recipe name', { exact: true }).fill('My retained edit');
  await page.getByLabel('Batch volume (L)').fill('20.000000000000000');
  await other.getByRole('button', { name: 'Save recipe' }).click();
  await expect(other.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByRole('region', { name: 'Save conflict' })).toBeVisible();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue('My retained edit');
  await page.getByRole('button', { name: 'Save recipe' }).click();
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
  await page.getByRole('button', { name: 'Save recipe' }).click();
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
  await page.getByRole('link', { name: 'Settings', exact: true }).first().click();
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Save and leave' }).click();
  await expect(page).toHaveURL(/settings\/profile/);
  expect(db.exec(`SELECT COUNT(*) FROM recipes WHERE name = '${name}' AND native_head_revision = 1`)).toBe('1');
});

test('revoked credentials cannot read the retained draft until reauthentication', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill('Draft after revocation');
  await expect.poll(() => page.evaluate(() => Object.keys(localStorage).some(k => k.startsWith('krausen-draft-v1:')))).toBe(true);
  const userId = getDefaultUserId();
  db.exec(`UPDATE auth_credentials SET revoked_at = ${Math.floor(Date.now() / 1000)} WHERE user_id = '${userId}' AND revoked_at IS NULL`);
  walCheckpoint();
  const other = await page.context().newPage();
  await other.goto('/recipe-editor/new');
  await expect(other.getByText('A retained draft is available')).toHaveCount(0);
  await expect(other.getByLabel('Recipe name', { exact: true })).toHaveCount(0);
  // The newly opened route can be redirected by the shared authentication
  // boundary. The existing original-scope editor owns the held draft notice.
  await page.bringToFront();
  await expect(page.getByText('Session expired', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Retry check', exact: true })).toBeVisible();
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
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Save failed' })).toBeVisible();
  await expect.poll(() => db.exec(`SELECT COUNT(*) FROM recipes WHERE name = '${name}'`)).toBe('1');
  await expect.poll(() => page.evaluate(() => Object.keys(localStorage).some(k => k.startsWith('krausen-draft-v1:')))).toBe(true);
  page.once('dialog', dialog => dialog.accept());
  await page.reload();
  await page.getByRole('button', { name: 'Restore draft' }).click();
  await page.getByRole('button', { name: 'Save recipe' }).click();
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
  await page.getByRole('link', { name: 'Settings', exact: true }).first().click();
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
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  const url = page.url();
  await page.reload();
  await expect.poll(() => previews.some(result => result.Preview?.context?.draft?.title === 'Clean original')).toBe(true);
  const other = await page.context().newPage();
  await other.goto(url);
  await other.getByLabel('Recipe name', { exact: true }).fill('Clean remote update');
  await other.getByRole('button', { name: 'Save recipe' }).click();
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
  await page.goto('/recipe-editor/new');
  await page.getByText('Equipment settings', { exact: true }).click();
  const profileName = `Recovery profile ${Date.now()}`;
  await page.getByLabel('Profile name').fill(profileName);
  await page.getByRole('button', { name: 'Save profile' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Profile saved.' })).toBeVisible();
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
  await page.getByRole('button', { name: new RegExp(DEFAULT_TEST_USER.email) }).click();
  await page.getByRole('button', { name, exact: true }).click();
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await guard.getByRole('button', { name: 'Discard and leave' }).click();
  await expect(page.getByRole('alert')).toContainText('Could not switch workspaces.');
  await expect(page).toHaveURL(/recipe-editor/);
});

test('a failed sign-out is reported after choosing to discard', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill('Draft before sign-out');
  await page.route('**/leptos-api/logout*', route => route.abort('failed'));
  await page.getByRole('button', { name: new RegExp(DEFAULT_TEST_USER.email) }).click();
  await page.getByRole('button', { name: 'Sign Out', exact: true }).click();
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await guard.getByRole('button', { name: 'Discard and leave' }).click();
  await expect(page.getByRole('alert')).toContainText('Could not sign out.');
  await expect(page).toHaveURL(/recipe-editor/);
});
