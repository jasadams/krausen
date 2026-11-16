import { test, expect, type Locator, type Page } from '@playwright/test';
import {
  DEFAULT_TEST_USER, getDefaultUserId, getDefaultWorkspaceId, db, loginUser, openAccount,
  seedWorkspace, setAppearance, walCheckpoint,
} from '../../helpers/test-helpers';

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.addInitScript(() => localStorage.setItem('krausen-theme', 'light'));
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
  await page.goto('/recipes');
});

test('approved shell uses real navigation, shared theme and a compact account popup', async ({ page }) => {
  const sidebar = page.locator('.app-shell__desktop');
  await expect(sidebar.getByRole('navigation', { name: 'Your brewery' })).toBeVisible();
  await expect(sidebar.getByRole('link', { name: 'Recipes', exact: true })).toHaveAttribute('aria-current', 'page');
  await expect(sidebar.getByRole('link', { name: 'Design system' })).toHaveCount(0);
  const geometry = await page.evaluate(() => {
    const box = (selector: string) => document.querySelector(selector)!.getBoundingClientRect();
    const sidebar = box('.app-shell__desktop');
    const topbar = box('.recipe-library__topbar');
    const intro = box('.recipe-library__intro h1');
    const toolbar = box('.recipe-library__toolbar');
    const actions = document.querySelector('.recipe-library__topbar .recipe-library__actions')!;
    const buttons = [...actions.querySelectorAll('a')].map(el => el.getBoundingClientRect());
    return { sidebar: sidebar.width, topbar: topbar.height, contentX: topbar.x, sidebarRight: sidebar.right,
      titleGutter: intro.x - topbar.x, toolbarX: toolbar.x, introX: intro.x,
      actionGap: buttons[1].left - buttons[0].right,
      buttonPadding: parseFloat(getComputedStyle(actions.querySelector('a')!).paddingLeft),
      actionAlignment: Math.abs(buttons[0].top - buttons[1].top),
      overflow: document.documentElement.scrollWidth > innerWidth };
  });
  expect(geometry.sidebar).toBeCloseTo(216, 0);
  expect(geometry.topbar).toBeCloseTo(60, 0);
  expect(geometry.contentX).toBeCloseTo(geometry.sidebarRight, 0);
  expect(geometry.titleGutter).toBeCloseTo(32, 0);
  expect(geometry.toolbarX).toBeGreaterThanOrEqual(geometry.introX);
  // Approved .topbar-actions uses space-3; internal button and panel gaps use space-2.
  expect(geometry.actionGap).toBeCloseTo(10, 0);
  expect(geometry.buttonPadding).toBeCloseTo(14, 0);
  expect(geometry.actionAlignment).toBeLessThanOrEqual(1);
  expect(geometry.overflow).toBe(false);
  await page.screenshot({ path: test.info().outputPath('library-desktop-light.png'), fullPage: true });
  // IA-002/003: no shell workspace control, binary theme shortcut or header furniture.
  await expect(sidebar.getByRole('button', { name: 'Switch workspace' })).toHaveCount(0);
  await expect(sidebar.getByRole('button', { name: /appearance/i })).toHaveCount(0);
  await expect(page.locator('.app-shell__workspace, .brewery-sidebar__workspace')).toHaveCount(0);
  await setAppearance(page, 'dark');
  await expect(page.getByRole('heading', { name: 'Good things start here.' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Import BeerXML' })).toHaveAttribute('href', '/beerxml');
  await expect(page.getByLabel('Search recipes')).toBeVisible();
  await expect(page.getByLabel('Show', { exact: true })).toBeVisible();
  await page.screenshot({ path: test.info().outputPath('library-desktop-dark.png'), fullPage: true });
});

test('mobile navigation is a focus-restoring modal and library does not overflow', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const menu = page.getByRole('button', { name: 'Toggle menu' });
  await menu.click();
  const navigation = page.getByRole('dialog', { name: 'Navigation', exact: true });
  await expect(navigation).toBeVisible();
  await expect(navigation.getByRole('link', { name: 'Recipes', exact: true })).toBeVisible();
  await expect(navigation.getByRole('link', { name: 'Design system' })).toHaveCount(0);
  await page.keyboard.press('Escape');
  await expect(navigation).not.toBeVisible();
  await expect(menu).toBeFocused();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: test.info().outputPath('library-mobile.png'), fullPage: true });
});

for (const theme of ['light', 'dark']) {
  test(`customer navigation excludes showcase but authenticated direct access remains in ${theme}`, async ({ page }) => {
    if (theme === 'dark') await setAppearance(page, 'dark');
    await expect(page.locator('html')).toHaveClass(theme === 'dark' ? /\bdark\b/ : /^(?!.*\bdark\b).*$/);
    for (const viewport of [
      { width: 1440, height: 1000 }, { width: 1920, height: 1080 },
      { width: 320, height: 568 }, { width: 390, height: 844 },
      { width: 667, height: 375 },
    ]) {
      await page.setViewportSize(viewport);
      const mobile = viewport.width < 768;
      const menu = page.getByRole('button', { name: 'Toggle menu' });
      if (mobile) await menu.click();
      const navigation = mobile
        ? page.getByRole('dialog', { name: 'Navigation', exact: true })
        : page.locator('.app-shell__desktop');
      await expect(navigation.getByRole('link', { name: 'Recipes', exact: true })).toBeVisible();
      // Check the DOM too: hiding a customer link from sight/AT is not removal.
      await expect(navigation.locator('a[href="/design-system"]')).toHaveCount(0);
      await expect(navigation.getByRole('button', { name: /^Account menu/ })).toBeVisible();
      await expect(navigation.getByRole('button', { name: /appearance/i })).toHaveCount(0);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      await page.screenshot({ path: test.info().outputPath(`navigation-${theme}-${viewport.width}.png`), fullPage: true });
      if (mobile) {
        await page.keyboard.press('Escape');
        await expect(navigation).not.toBeVisible();
        await expect(menu).toBeFocused();
      }
    }
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.goto('/design-system');
    await expect(page).toHaveURL(/\/design-system$/);
    await expect(page.getByRole('heading', { name: 'Good design, on repeat.' })).toBeVisible();
    // Full navigation reapplies this spec's light init preference. Establish the
    // requested theme through the real appearance control on the showcase too.
    if (theme === 'dark') await setAppearance(page, 'dark');
    await expect(page.locator('html')).toHaveClass(theme === 'dark' ? /\bdark\b/ : /^(?!.*\bdark\b).*$/);
    // Exercise a real shared kit field, not just the route's static heading.
    await page.getByLabel('Batch volume (L)', { exact: true }).fill('21.5');
    await expect(page.getByLabel('Batch volume (L)', { exact: true })).toHaveValue('21.5');
    const accountPopup = await openAccount(page, page.locator('.app-shell__desktop'));
    await accountPopup.getByRole('link', { name: 'Settings', exact: true }).click();
    await expect(page).toHaveURL(/\/settings\/profile(\?return=[^#]*)?$/);
    await expect(page.getByRole('heading', { name: 'Profile Settings', exact: true })).toBeVisible();
  });
}

test('navigation remains available across the desktop/mobile breakpoint seam', async ({ page }) => {
  for (const width of [760, 761, 767, 768]) {
    await page.setViewportSize({ width, height: 1000 });
    const desktop = page.locator('.app-shell__desktop');
    const menu = page.getByRole('button', { name: 'Toggle menu' });
    if (width < 768) {
      await expect(desktop).not.toBeVisible();
      await expect(menu).toBeVisible();
    } else {
      await expect(desktop).toBeVisible();
      await expect(menu).not.toBeVisible();
    }
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  }
});


const accountTrigger = (scope: Page | Locator) => scope.getByRole('button', { name: /^Account menu/ });

// IA-002 / ACC-002: one anchored nonmodal popup of ordinary links and buttons.
test('account popup is ordinary nonmodal navigation with natural keyboard, Escape and outside dismissal', async ({ page }) => {
  // Isolate from memberships other specs leave behind in a shared database:
  // this test is about a verified SINGLE membership.
  await page.route('**/leptos-api/list_user_workspaces*', route => route.fulfill({ contentType: 'application/json', body: JSON.stringify([
    { workspace_id: 'only', name: 'Only brewery', is_current: true },
  ]) }));
  const desktop = page.locator('.app-shell__desktop');
  const trigger = accountTrigger(desktop);
  await expect(trigger).toHaveAttribute('aria-expanded', 'false');
  await expect(trigger).toHaveAttribute('aria-controls', 'account-popover');
  await trigger.focus();
  await page.keyboard.press('Enter');
  const popup = page.locator('#account-popover');
  await expect(popup).toBeVisible();
  await expect(trigger).toHaveAttribute('aria-expanded', 'true');
  // Not a dialog or ARIA menu, and the page behind it is not inert.
  expect(await popup.evaluate(element => element.matches(':modal') || element.getAttribute('role') === 'menu' || !!element.querySelector('[role^="menuitem"]'))).toBe(false);
  await expect(page.locator('dialog:modal')).toHaveCount(0);
  await expect(popup.getByRole('link', { name: 'Settings', exact: true })).toHaveAttribute('href', /^\/settings\/profile(\?return=[^#]*)?$/);
  await expect(popup.getByRole('link', { name: 'Appearance', exact: true })).toHaveAttribute('href', /^\/settings\/profile(\?return=[^#]*)?#appearance$/);
  await expect(popup.getByRole('button', { name: 'Sign Out', exact: true })).toBeVisible();
  // Single membership: no workspace identity/choices or switching entry anywhere.
  await expect(popup.getByRole('heading', { name: 'Workspaces' })).toHaveCount(0);
  await expect(popup.getByText('(current)')).toHaveCount(0);
  // Natural Tab order from the trigger through the actions, then out with no trap.
  await page.keyboard.press('Tab');
  await expect(popup.getByRole('link', { name: 'Settings', exact: true })).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(popup.getByRole('link', { name: 'Appearance', exact: true })).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(popup.getByRole('button', { name: 'Sign Out', exact: true })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(popup).toBeHidden();
  await expect(trigger).toBeFocused();
  await expect(trigger).toHaveAttribute('aria-expanded', 'false');
  // Outside pointer dismisses without activating the underlying control.
  await trigger.click();
  await expect(popup).toBeVisible();
  // The underlying control receives the click (dismissal does not swallow it).
  await page.getByLabel('Search recipes').click({ position: { x: 5, y: 5 } });
  await expect(popup).toBeHidden();
  await expect(page.getByLabel('Search recipes')).toBeFocused();
  await expect(page).toHaveURL(/\/recipes$/);
});

test('account Settings closes the popup only after navigation commits and a dirty draft stays protected', async ({ page }) => {
  const desktop = page.locator('.app-shell__desktop');
  const popup = await openAccount(page, desktop);
  await popup.getByRole('link', { name: 'Settings', exact: true }).click();
  await expect(page).toHaveURL(/\/settings\/profile(\?return=[^#]*)?$/);
  await expect(popup).toBeHidden();
  await expect(page.getByRole('heading', { name: 'Profile Settings', exact: true })).toBeVisible();

  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  await page.getByLabel('Recipe name', { exact: true }).fill('Guarded account navigation');
  const dirtyPopup = await openAccount(page, desktop);
  await dirtyPopup.getByRole('link', { name: 'Settings', exact: true }).click();
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await expect(guard).toBeVisible();
  // The guard is the single modal; the account popup yields to it.
  await expect(page.locator('dialog:modal')).toHaveCount(1);
  await expect(page.locator('[popover]:popover-open')).toHaveCount(0);
  await guard.getByRole('button', { name: 'Stay', exact: true }).click();
  await expect(page).toHaveURL(/\/recipe-editor\/new$/);
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue('Guarded account navigation');
  await expect(guard).toBeHidden();
  // Focus returns to the visible account trigger, not the document body.
  await expect(desktop.getByRole('button', { name: /^Account menu/ })).toBeFocused();
});

test('Settings and Appearance on the Profile route close the popup, and the mobile drawer too', async ({ page }) => {
  await page.goto('/settings/profile');
  const desktop = page.locator('.app-shell__desktop');
  const popup = await openAccount(page, desktop);
  await popup.getByRole('link', { name: 'Appearance', exact: true }).click();
  await expect(popup).toBeHidden();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole('button', { name: 'Toggle menu' }).click();
  const navigation = page.getByRole('dialog', { name: 'Navigation', exact: true });
  const mobilePopup = await openAccount(page, navigation);
  await mobilePopup.getByRole('link', { name: 'Settings', exact: true }).click();
  await expect(navigation).toBeHidden();
  await expect(page.locator('[popover]:popover-open')).toHaveCount(0);
});

test('an open desktop popup does not linger when the window shrinks to mobile', async ({ page }) => {
  await openAccount(page, page.locator('.app-shell__desktop'));
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.locator('[popover]:popover-open')).toHaveCount(0);
  await page.setViewportSize({ width: 1440, height: 1000 });
  await expect(page.locator('[popover]:popover-open')).toHaveCount(0);
});

// IA-003: choices only after a successful current list with MORE than one membership.
for (const [state, list] of [
  ['one membership', [{ workspace_id: 'only', name: 'Only brewery', is_current: true }]],
  ['zero memberships', []],
] as const) {
  test(`${state} offers no workspace switching`, async ({ page }) => {
    await page.route('**/leptos-api/list_user_workspaces*', route => route.fulfill({ contentType: 'application/json', body: JSON.stringify(list) }));
    const popup = await openAccount(page, page.locator('.app-shell__desktop'));
    await expect(popup.getByRole('link', { name: 'Settings', exact: true })).toBeVisible();
    await expect(popup.getByRole('button', { name: 'Sign Out', exact: true })).toBeEnabled();
    await expect(popup.getByText('Checking workspaces…')).toHaveCount(0);
    await expect(popup.getByRole('heading', { name: 'Workspaces' })).toHaveCount(0);
    await expect(popup.getByText('Only brewery')).toHaveCount(0);
    if (list.length === 0) await expect(popup.getByRole('status').filter({ hasText: 'No workspace is available' })).toBeVisible();
  });
}

test('a loading or failed membership check is truthful, keeps account actions and retries', async ({ page }) => {
  let requests = 0;
  let releaseFirst!: () => void;
  const first = new Promise<void>(resolve => { releaseFirst = resolve; });
  await page.route('**/leptos-api/list_user_workspaces*', async route => {
    requests++;
    if (requests === 1) { await first; await route.abort('failed'); return; }
    await route.fulfill({ contentType: 'application/json', body: JSON.stringify([
      { workspace_id: 'current-test-workspace', name: 'Current brewery', is_current: true },
      { workspace_id: 'second-test-workspace', name: 'Second brewery', is_current: false },
    ]) });
  });
  const popup = await openAccount(page, page.locator('.app-shell__desktop'));
  await expect(popup.getByRole('status').filter({ hasText: 'Checking workspaces…' })).toBeVisible();
  await expect(popup.getByText('Current brewery')).toHaveCount(0);
  await expect(popup.getByRole('link', { name: 'Settings', exact: true })).toBeVisible();
  releaseFirst();
  await expect(popup.getByText('Unable to load workspaces', { exact: true })).toBeVisible();
  await expect(popup.getByText('Current brewery')).toHaveCount(0);
  await expect(popup.getByRole('button', { name: 'Sign Out', exact: true })).toBeEnabled();
  await popup.getByRole('button', { name: 'Retry', exact: true }).click();
  await expect(popup.getByRole('heading', { name: 'Workspaces' })).toBeVisible();
  await expect(popup.getByText('Current brewery (current)')).toHaveAttribute('aria-current', 'true');
  await expect(popup.getByRole('button', { name: 'Second brewery', exact: true })).toBeVisible();
  // A fresh opening re-checks instead of showing stale choices.
  await page.keyboard.press('Escape');
  await accountTrigger(page.locator('.app-shell__desktop')).click();
  await expect.poll(() => requests).toBeGreaterThanOrEqual(3);
});

test.describe('with a second membership', () => {
  let ownerId: string;
  let originalWorkspace: string;
  let otherWorkspace: string;
  const otherName = `Second shell brewery ${Date.now()}`;

  test.beforeEach(async ({ page }) => {
    ownerId = getDefaultUserId();
    originalWorkspace = getDefaultWorkspaceId();
    otherWorkspace = seedWorkspace({ ownerId, name: otherName });
    db.exec(`UPDATE users SET last_workspace_id = '${originalWorkspace}' WHERE user_id = '${ownerId}'`);
    walCheckpoint();
    await page.goto('/recipes');
  });
  test.afterEach(() => {
    db.exec(`UPDATE users SET last_workspace_id = '${originalWorkspace}' WHERE user_id = '${ownerId}'`);
    db.exec(`DELETE FROM workspace_users WHERE workspace_id = '${otherWorkspace}'`);
    db.exec(`DELETE FROM workspaces WHERE workspace_id = '${otherWorkspace}'`);
    walCheckpoint();
  });

  test('choices appear inside the popup and a successful switch lands on destination Recipes', async ({ page }) => {
    await page.goto('/design-system');
    const popup = await openAccount(page, page.locator('.app-shell__desktop'));
    await expect(popup.getByRole('heading', { name: 'Workspaces' })).toBeVisible();
    await expect(popup.locator('[aria-current="true"]')).toContainText('(current)');
    await popup.getByRole('button', { name: otherName, exact: true }).click();
    await expect(page).toHaveURL(/\/recipes$/);
    expect(db.query(`SELECT last_workspace_id FROM users WHERE user_id = '${ownerId}'`)[0]).toBe(otherWorkspace);
    const next = await openAccount(page, page.locator('.app-shell__desktop'));
    await expect(next.locator('[aria-current="true"]')).toContainText(otherName);
  });

  test('a slow switch is pending, locks competing actions and cannot be doubled', async ({ page }) => {
    await page.goto('/design-system');
    let switches = 0;
    let release!: () => void;
    const held = new Promise<void>(resolve => { release = resolve; });
    await page.route('**/leptos-api/switch_workspace*', async route => {
      switches++;
      await held;
      await route.continue();
    });
    const desktop = page.locator('.app-shell__desktop');
    const popup = await openAccount(page, desktop);
    await popup.getByRole('button', { name: otherName, exact: true }).click();
    await expect(desktop.getByRole('status').filter({ hasText: 'Switching workspace…' })).toBeVisible();
    const again = await openAccount(page, desktop);
    await expect(again.getByRole('button', { name: otherName, exact: true })).toBeDisabled();
    await expect(again.getByRole('button', { name: 'Sign Out', exact: true })).toBeDisabled();
    expect(switches).toBe(1);
    release();
    await expect(page).toHaveURL(/\/recipes$/);
    expect(switches).toBe(1);
  });

  test('a failed switch keeps the original context and the dirty draft', async ({ page }) => {
    await page.route('**/leptos-api/switch_workspace*', route => route.abort('failed'));
    await page.goto('/recipe-editor/new');
    await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
    await page.getByLabel('Recipe name', { exact: true }).fill('Draft kept through failed switch');
    const popup = await openAccount(page, page.locator('.app-shell__desktop'));
    await popup.getByRole('button', { name: otherName, exact: true }).click();
    const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
    await guard.getByRole('button', { name: 'Discard and leave' }).click();
    await expect(page.getByRole('alert')).toContainText('Could not switch workspaces.');
    await expect(page).toHaveURL(/recipe-editor\/new/);
    expect(db.query(`SELECT last_workspace_id FROM users WHERE user_id = '${ownerId}'`)[0]).toBe(originalWorkspace);
  });

  test('mobile drawer account lists the same choices inside the drawer and switching closes it', async ({ page }) => {
    await page.goto('/design-system');
    await page.setViewportSize({ width: 390, height: 844 });
    await page.getByRole('button', { name: 'Toggle menu' }).click();
    const navigation = page.getByRole('dialog', { name: 'Navigation', exact: true });
    const popup = await openAccount(page, navigation);
    // The popup is within the drawer's usable subtree, not an inert body portal.
    expect(await popup.evaluate(element => !!element.closest('dialog#mobile-navigation'))).toBe(true);
    await popup.getByRole('button', { name: otherName, exact: true }).click();
    await expect(page).toHaveURL(/\/recipes$/);
    await expect(page.locator('dialog:modal')).toHaveCount(0);
  });
});

// IA-004: mobile has one modal owner; closure follows committed arrival only.
test.describe('mobile navigation drawer', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
  });

  test('Settings from the drawer account closes the drawer after the route commits', async ({ page }) => {
    const menu = page.getByRole('button', { name: 'Toggle menu' });
    await menu.click();
    const navigation = page.getByRole('dialog', { name: 'Navigation', exact: true });
    const popup = await openAccount(page, navigation);
    await popup.getByRole('link', { name: 'Settings', exact: true }).click();
    await expect(page).toHaveURL(/\/settings\/profile(\?return=[^#]*)?$/);
    await expect(navigation).toBeHidden();
    await expect(page.locator('dialog:modal')).toHaveCount(0);
    await expect(page.getByRole('heading', { name: 'Profile Settings', exact: true })).toBeVisible();
    // Reopening does not resurrect the popup.
    await menu.click();
    await expect(page.locator('[popover]:popover-open')).toHaveCount(0);
  });

  test('a recipe link from the drawer closes it on arrival and the same route closes it too', async ({ page }) => {
    const menu = page.getByRole('button', { name: 'Toggle menu' });
    await menu.click();
    const navigation = page.getByRole('dialog', { name: 'Navigation', exact: true });
    await navigation.getByRole('link', { name: 'Recipes', exact: true }).click();
    await expect(navigation).toBeHidden();
    await page.goto('/design-system');
    await menu.click();
    await navigation.getByRole('link', { name: 'Recipes', exact: true }).click();
    await expect(page).toHaveURL(/\/recipes$/);
    await expect(navigation).toBeHidden();
  });

  test('Recipes and Equipment close the drawer when already the current route (Settings is covered separately)', async ({ page }) => {
    const menu = page.getByRole('button', { name: 'Toggle menu' });
    const navigation = page.getByRole('dialog', { name: 'Navigation', exact: true });
    for (const [path, link] of [['/recipes', 'Recipes'], ['/equipment', 'Equipment']]) {
      await page.goto(path);
      await menu.click();
      await expect(navigation).toBeVisible();
      await navigation.getByRole('link', { name: link, exact: true }).click();
      await expect(page).toHaveURL(new RegExp(`${path}$`));
      await expect(navigation).toBeHidden();
      await expect(page.locator('dialog:modal')).toHaveCount(0);
    }
  });

  test('a dirty draft turns the drawer into the single guard, and Stay never navigates behind it', async ({ page }) => {
    await page.goto('/recipe-editor/new');
    await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
    await page.getByLabel('Recipe name', { exact: true }).fill('Guarded mobile navigation');
    const menu = page.getByRole('button', { name: 'Toggle menu' });
    await menu.click();
    const navigation = page.getByRole('dialog', { name: 'Navigation', exact: true });
    const popup = await openAccount(page, navigation);
    await popup.getByRole('link', { name: 'Settings', exact: true }).click();
    const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
    await expect(guard).toBeVisible();
    await expect(navigation).toBeHidden();
    await expect(page.locator('dialog:modal')).toHaveCount(1);
    await expect(page.locator('[popover]:popover-open')).toHaveCount(0);
    await expect(page).toHaveURL(/\/recipe-editor\/new$/);
    await guard.getByRole('button', { name: 'Stay', exact: true }).click();
    await expect(guard).toBeHidden();
    await expect(page).toHaveURL(/\/recipe-editor\/new$/);
    await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue('Guarded mobile navigation');
    await expect(menu).toBeFocused();
    // The captured destination is executed exactly once after a deliberate Discard.
    await menu.click();
    const again = await openAccount(page, navigation);
    await again.getByRole('link', { name: 'Settings', exact: true }).click();
    await guard.getByRole('button', { name: 'Discard and leave' }).click();
    await expect(page).toHaveURL(/\/settings\/profile(\?return=[^#]*)?$/);
  });

  test('Escape closes the popup first and the drawer on a later Escape', async ({ page }) => {
    const menu = page.getByRole('button', { name: 'Toggle menu' });
    await menu.click();
    const navigation = page.getByRole('dialog', { name: 'Navigation', exact: true });
    const trigger = accountTrigger(navigation);
    const popup = await openAccount(page, navigation);
    await page.keyboard.press('Escape');
    await expect(popup).toBeHidden();
    await expect(navigation).toBeVisible();
    await expect(trigger).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(navigation).toBeHidden();
    await expect(menu).toBeFocused();
  });

  test('crossing the desktop breakpoint closes the drawer and leaves focus on a visible control', async ({ page }) => {
    await page.getByRole('button', { name: 'Toggle menu' }).click();
    const navigation = page.getByRole('dialog', { name: 'Navigation', exact: true });
    await expect(navigation).toBeVisible();
    await page.setViewportSize({ width: 1024, height: 800 });
    await expect(navigation).toBeHidden();
    await expect(page.locator('dialog:modal')).toHaveCount(0);
    expect(await page.evaluate(() => {
      const active = document.activeElement;
      return !!active && active !== document.body && active.getClientRects().length > 0;
    })).toBe(true);
    await page.setViewportSize({ width: 390, height: 844 });
    await expect(page.getByRole('button', { name: 'Toggle menu' })).toBeVisible();
  });

  test('an open account popup does not survive its drawer or leave two interactive copies', async ({ page }) => {
    await page.getByRole('button', { name: 'Toggle menu' }).click();
    const navigation = page.getByRole('dialog', { name: 'Navigation', exact: true });
    await openAccount(page, navigation);
    await page.setViewportSize({ width: 1024, height: 800 });
    await expect(navigation).toBeHidden();
    await expect(page.locator('[popover]:popover-open')).toHaveCount(0);
    await expect(page.getByRole('button', { name: /^Account menu/ })).toHaveCount(1);
  });
});
