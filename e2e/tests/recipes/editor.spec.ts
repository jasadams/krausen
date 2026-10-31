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
  const guard = page.getByRole('dialog', { name: 'Save your recipe first' });
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Close' }).click();
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
