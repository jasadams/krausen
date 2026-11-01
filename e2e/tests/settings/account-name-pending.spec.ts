import { test, expect } from '@playwright/test';
import { gotoAuthenticated, isPersonalMode } from '../../helpers/test-helpers';

test('name cannot be edited while an autosave is pending', async ({ page }) => {
  test.skip(isPersonalMode(), 'profile name editing is not shown in personal mode');
  await gotoAuthenticated(page, '/settings/profile');
  const input = page.getByPlaceholder('Your name');
  await expect(input).toBeVisible();
  let markDispatched!: () => void;
  const dispatched = new Promise<void>(resolve => { markDispatched = resolve; });
  let release!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  await page.route('**/leptos-api/update_profile_name*', async route => {
    markDispatched();
    await gate;
    await route.continue();
  });
  await input.fill(`Pending-${Date.now()}`);
  await input.blur();
  await dispatched;
  try {
    await expect(input).toBeDisabled({ timeout: 2_000 });
  } finally {
    release();
  }
  await expect(page.getByText('Saved', { exact: true })).toBeVisible();
  await expect(input).toBeEnabled();
});
