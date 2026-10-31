import { test, expect } from '@playwright/test';

test('an explicit save keeps a numeric draft and reopens its native revision', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  const title = `Browser recipe ${Date.now()}`;
  await page.getByLabel('Recipe name', { exact: true }).fill(title);
  const volume = page.getByLabel('Batch volume (L)');
  await volume.fill('-');
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Complete invalid number fields' })).toBeVisible();
  await expect(volume).toHaveValue('-');
  await volume.fill('20');
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  await expect(page).toHaveURL(/\/recipe-editor\/(?!new)[^/]+$/);
  await page.reload();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(title);
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue('20');
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
