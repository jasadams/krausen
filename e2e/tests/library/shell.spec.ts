import { test, expect } from '@playwright/test';
import { DEFAULT_TEST_USER, loginUser } from '../../helpers/test-helpers';

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.addInitScript(() => localStorage.setItem('krausen-theme', 'light'));
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
  await page.goto('/recipes');
});

test('approved shell uses real navigation, shared theme and accessible workspace controls', async ({ page }) => {
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
  const darkBefore = await page.locator('html').evaluate(el => el.classList.contains('dark'));
  await sidebar.getByRole('button', { name: /appearance/ }).click();
  await expect(page.locator('html')).toHaveClass(darkBefore ? /^(?!.*\bdark\b).*$/ : /\bdark\b/);
  await sidebar.getByRole('button', { name: 'Switch workspace' }).click();
  const workspaces = page.getByRole('dialog', { name: 'Your workspaces' });
  await expect(workspaces).toBeVisible();
  await expect(workspaces.locator('button:disabled').first()).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(workspaces).not.toBeVisible();
  await expect(sidebar.getByRole('button', { name: 'Switch workspace' })).toBeFocused();
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
    if (theme === 'dark') {
      await page.locator('.app-shell__desktop').getByRole('button', { name: /appearance/ }).click();
    }
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
      await expect(navigation.getByRole('button', { name: 'Account menu' })).toBeVisible();
      await expect(navigation.getByRole('button', { name: /appearance/ })).toBeVisible();
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
    if (theme === 'dark') {
      await page.locator('.app-shell__desktop').getByRole('button', { name: /appearance/ }).click();
    }
    await expect(page.locator('html')).toHaveClass(theme === 'dark' ? /\bdark\b/ : /^(?!.*\bdark\b).*$/);
    // Exercise a real shared kit field, not just the route's static heading.
    await page.getByLabel('Batch volume (L)', { exact: true }).fill('21.5');
    await expect(page.getByLabel('Batch volume (L)', { exact: true })).toHaveValue('21.5');
    await page.locator('.app-shell__desktop').getByRole('button', { name: 'Account menu' }).click();
    await page.getByRole('dialog', { name: 'Your account', exact: true }).getByRole('link', { name: 'Settings', exact: true }).click();
    await expect(page).toHaveURL(/\/settings\/profile$/);
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

test('account Settings closes the modal only after navigation commits', async ({ page }) => {
  const account = page.getByRole('dialog', { name: 'Your account', exact: true });
  await page.locator('.app-shell__desktop').getByRole('button', { name: 'Account menu' }).click();
  await account.getByRole('link', { name: 'Settings', exact: true }).click();
  await expect(page).toHaveURL(/\/settings\/profile$/);
  await expect(account).not.toBeVisible();
  await expect(page.getByRole('heading', { name: 'Profile Settings', exact: true })).toBeVisible();

  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill('Guarded account navigation');
  await page.locator('.app-shell__desktop').getByRole('button', { name: 'Account menu' }).click();
  await account.getByRole('link', { name: 'Settings', exact: true }).click();
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Stay', exact: true }).click();
  await expect(page).toHaveURL(/\/recipe-editor\/new$/);
  await expect(account).toBeVisible();
  await account.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue('Guarded account navigation');
});

test('workspace picker refreshes changed choices and recovers a failed fetch on reopen', async ({ page }) => {
  let requests = 0;
  await page.route('**/leptos-api/list_user_workspaces*', async route => {
    requests++;
    if (requests === 1) {
      await route.abort('failed');
      return;
    }
    await route.fulfill({ contentType: 'application/json', body: JSON.stringify([
      { workspace_id: 'current-test-workspace', name: `Current brewery ${requests}`, is_current: true },
    ]) });
  });
  const open = page.locator('.app-shell__desktop').getByRole('button', { name: 'Switch workspace' });
  const picker = page.getByRole('dialog', { name: 'Your workspaces', exact: true });
  await open.click();
  await expect(picker.getByText('Unable to load workspaces', { exact: true })).toBeVisible();
  await picker.getByRole('button', { name: 'Close', exact: true }).click();
  await open.click();
  await expect(picker.getByRole('button', { name: 'Current brewery 2' })).toBeVisible();
  await picker.getByRole('button', { name: 'Close', exact: true }).click();
  await open.click();
  await expect(picker.getByRole('button', { name: 'Current brewery 3' })).toBeVisible();
  expect(requests).toBe(3);
});
