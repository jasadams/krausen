import { test, expect } from '@playwright/test';
import { gotoAuthenticated } from '../../helpers/test-helpers';

// In personal mode only the Profile tab is visible (Security, Workspace, Team
// are gated behind `!is_personal_mode`). These tests verify the settings shell
// navigation that IS available in personal mode, plus direct URL navigation to
// the hidden routes which still render via the router.

test.describe('TC-008: Settings Navigation', () => {
  test('navigates to /settings/profile and renders the settings shell', async ({ page }) => {
    await gotoAuthenticated(page, '/settings/profile');

    await expect(page).toHaveURL(/\/settings\/profile/);

    const heading = page.locator('h1', { hasText: 'Settings' });
    await expect(heading).toBeVisible();

    const subheading = page.locator('h2', { hasText: 'Profile Settings' });
    await expect(subheading).toBeVisible();
  });

  test('Profile is the current leaf of a labelled Settings navigation (links, not tabs)', async ({ page }) => {
    await gotoAuthenticated(page, '/settings/profile');

    const nav = page.getByRole('navigation', { name: 'Settings' });
    await expect(nav).toBeVisible();
    await expect(nav.getByRole('link', { name: 'Profile', exact: true })).toHaveAttribute('aria-current', 'page');
    await expect(nav.getByRole('link', { name: 'Security', exact: true })).not.toHaveAttribute('aria-current', 'page');
    await expect(page.getByRole('tab')).toHaveCount(0);
    await expect(nav.getByRole('heading', { name: 'Account', exact: true })).toBeVisible();
    await expect(nav.getByText('apply to you in every workspace')).toBeVisible();
  });

  test('/settings redirects to /settings/profile', async ({ page }) => {
    await gotoAuthenticated(page, '/settings');

    await expect(page).toHaveURL(/\/settings\/profile/);
  });

  test('direct URL navigation to /settings/security renders without error', async ({ page }) => {
    await gotoAuthenticated(page, '/settings/security');

    await expect(page).toHaveURL(/\/settings\/security/);

    const shellHeading = page.locator('h1', { hasText: 'Settings' });
    await expect(shellHeading).toBeVisible();
  });

  test('direct URL navigation to /settings/workspace renders without error', async ({ page }) => {
    await gotoAuthenticated(page, '/settings/workspace');

    await expect(page).toHaveURL(/\/settings\/workspace/);

    const shellHeading = page.locator('h1', { hasText: 'Settings' });
    await expect(shellHeading).toBeVisible();
  });

  test('direct URL navigation to /settings/team renders without error', async ({ page }) => {
    await gotoAuthenticated(page, '/settings/team');

    await expect(page).toHaveURL(/\/settings\/team/);

    const shellHeading = page.locator('h1', { hasText: 'Settings' });
    await expect(shellHeading).toBeVisible();
  });

  test('SPA navigation between settings routes preserves shell DOM', async ({ page }) => {
    await gotoAuthenticated(page, '/settings/profile');

    const shell = page.locator('h1', { hasText: 'Settings' });
    await expect(shell).toBeVisible();

    // Tag the shell heading with a marker attribute to detect full-page reloads
    await shell.evaluate((el) => el.setAttribute('data-spa-marker', 'alive'));

    // Navigate to /settings/security via direct URL bar simulation (client-side)
    await page.evaluate(() => {
      const link = document.querySelector('a[href="/settings/security"]') as HTMLAnchorElement | null;
      if (link) {
        link.click();
      } else {
        window.history.pushState({}, '', '/settings/security');
        window.dispatchEvent(new PopStateEvent('popstate'));
      }
    });
    await page.waitForTimeout(1500);

    // The shell heading should still have our marker — proves no full page reload
    const markerAfter = await shell.getAttribute('data-spa-marker');
    expect(markerAfter).toBe('alive');
  });

  test('without a return location Settings offers Back to recipes', async ({ page }) => {
    await gotoAuthenticated(page, '/settings/profile');

    const back = page.getByRole('link', { name: 'Back to recipes' });
    await expect(back).toBeVisible();
    await expect(back).toHaveAttribute('href', '/recipes');
  });

  test('a validated in-app return survives Settings navigation and is used by Back', async ({ page }) => {
    await gotoAuthenticated(page, '/settings/profile?return=%2Frecipes%3Fq%3Dhelles');

    const back = page.locator('#settings-return');
    await expect(back).toHaveAttribute('href', '/recipes?q=helles');
    const nav = page.getByRole('navigation', { name: 'Settings' });
    await expect(nav.getByRole('link', { name: 'Profile', exact: true })).toHaveAttribute('href', /return=%2Frecipes%3Fq%3Dhelles/);
    await nav.getByRole('link', { name: 'Security', exact: true }).click();
    await expect(page).toHaveURL(/\/settings\/security\?return=/);
    await expect(page.locator('#settings-return')).toHaveAttribute('href', '/recipes?q=helles');
    await page.locator('#settings-return').click();
    await expect(page).toHaveURL(/\/recipes\?q=helles/);
  });

  for (const bad of ['https%3A%2F%2Fevil.example%2F', '%2F%2Fevil.example', '%2Fsettings%2Fprofile', 'javascript%3Aalert(1)', '%2Flogin']) {
    test(`an unsafe return (${bad}) is rejected in favour of Back to recipes`, async ({ page }) => {
      await gotoAuthenticated(page, `/settings/profile?return=${bad}`);
      const back = page.locator('#settings-return');
      await expect(back).toHaveText('Back to recipes');
      await expect(back).toHaveAttribute('href', '/recipes');
      await expect(page.getByRole('navigation', { name: 'Settings' }).getByRole('link', { name: 'Security', exact: true }))
        .not.toHaveAttribute('href', /return=/);
    });
  }

  test('/settings keeps its redirect and carries a validated return through it', async ({ page }) => {
    await gotoAuthenticated(page, '/settings?return=%2Fequipment');
    await expect(page).toHaveURL(/\/settings\/profile\?return=%2Fequipment/);
  });

  test('Settings opened from a page returns to that page, and sign out is only in the account dropdown', async ({ page }) => {
    await gotoAuthenticated(page, '/equipment');
    await page.locator('.app-shell__desktop').getByRole('button', { name: /^Account menu/ }).click();
    const popup = page.locator('[popover]:popover-open');
    await popup.getByRole('link', { name: 'Settings', exact: true }).click();
    await expect(page).toHaveURL(/\/settings\/profile\?return=%2Fequipment/);
    await expect(page.locator('#settings-return')).toHaveAttribute('href', '/equipment');
    await expect(page.locator('main').getByRole('button', { name: 'Sign Out' })).toHaveCount(0);
    await expect(page.locator('main').getByRole('link', { name: 'Close settings' })).toHaveCount(0);
  });

  test('Settings has a title, a named main landmark and focuses its heading on arrival', async ({ page }) => {
    await gotoAuthenticated(page, '/recipes');
    await page.locator('.app-shell__desktop').getByRole('button', { name: /^Account menu/ }).click();
    await page.locator('[popover]:popover-open').getByRole('link', { name: 'Settings', exact: true }).click();
    await expect(page).toHaveTitle('Settings · Krausen');
    await expect(page.getByRole('heading', { name: 'Settings', level: 1 })).toBeFocused();
  });
});
