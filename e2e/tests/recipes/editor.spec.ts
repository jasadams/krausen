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
      await expect(page.getByRole('heading', { name: 'A more considered brew day.' })).toBeVisible();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      await expect(page.getByLabel('Amount (kg)', { exact: true })).toHaveAttribute('aria-invalid', 'true');
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
  await expect(page.getByText('Equipment profile unavailable')).toBeVisible();
  await expect.poll(() => previews.includes('UnresolvedProfile')).toBe(true);
  await page.getByRole('button', { name: 'Unarchive' }).click();
  await expect(page.getByText('Equipment profile unavailable')).toHaveCount(0);
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
