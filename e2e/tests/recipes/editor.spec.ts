import { test, expect } from '@playwright/test';
import { DEFAULT_TEST_USER, createSecondUser, db, getDefaultUserId, getDefaultWorkspaceId, loginUser, seedWorkspace, walCheckpoint } from '../../helpers/test-helpers';

test.beforeEach(async ({ page }) => {
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
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
  await page.getByRole('button', { name: 'Add ingredient' }).click();
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
  await page.getByRole('button', { name: new RegExp(DEFAULT_TEST_USER.email) }).click();
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
  await conflict.getByRole('button', { name: 'Reapply my draft' }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue('My retained edit');
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
  await page.route('**/leptos-api/save_recipe*', async route => {
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
