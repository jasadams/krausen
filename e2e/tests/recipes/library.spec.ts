import { test, expect } from '@playwright/test';
import { DEFAULT_TEST_USER, loginUser } from '../../helpers/test-helpers';

test.beforeEach(async ({ page }) => {
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
});

test('recipe library searches, shows retained history, archives and restores', async ({ page }) => {
  const title = `Library recipe ${Date.now()}`;
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(title);
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  await page.goto('/recipes');
  await page.getByLabel('Search recipes').fill(title);
  await expect(page.getByRole('heading', { name: title })).toBeVisible();
  await page.getByRole('button', { name: `History for ${title}` }).click();
  await expect(page.getByRole('heading', { name: `History · ${title}` })).toBeVisible();
  await expect(page.getByText('Revision 1', { exact: false }).last()).toBeVisible();
  await page.getByRole('button', { name: `Archive ${title}` }).click();
  await expect(page.getByRole('heading', { name: title })).toHaveCount(0);
  await page.getByLabel('Show', { exact: true }).selectOption('archived');
  await expect(page.getByRole('heading', { name: title })).toBeVisible();
  await page.getByRole('button', { name: `History for ${title}` }).click();
  await expect(page.getByRole('heading', { name: `History · ${title}` })).toBeVisible();
  await page.getByRole('button', { name: `Restore ${title}` }).click();
  await expect(page.getByRole('heading', { name: title })).toHaveCount(0);
  await page.getByLabel('Show', { exact: true }).selectOption('active');
  await expect(page.getByRole('heading', { name: title })).toBeVisible();
  await page.getByRole('link', { name: `Open ${title}` }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(title);
});

test('library controls remain usable at narrow width and by keyboard', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/recipes');
  const search = page.getByLabel('Search recipes');
  await search.focus();
  await page.keyboard.type('unlikely recipe name');
  await expect(page.getByText('No recipes found')).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.keyboard.press('Tab');
  await expect(page.getByLabel('Show', { exact: true })).toBeFocused();
});


test('library navigation uses the shared dirty-editor guard', async ({ page }) => {
  const title = `Guarded library recipe ${Date.now()}`;
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(title);
  const recipes = page.getByRole('link', { name: 'Recipes', exact: true }).first();
  await recipes.click();
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Stay' }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(title);
  await recipes.click();
  await guard.getByRole('button', { name: 'Save and leave' }).click();
  await expect(page).toHaveURL(/\/recipes$/);
  await page.getByLabel('Search recipes').fill(title);
  await expect(page.getByRole('heading', { name: title })).toBeVisible();
  await page.getByRole('link', { name: `Open ${title}` }).click();
  await page.getByLabel('Recipe name', { exact: true }).fill(`${title} discarded`);
  await recipes.click();
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Discard and leave' }).click();
  await expect(page).toHaveURL(/\/recipes$/);
  await page.getByLabel('Search recipes').fill(title);
  await page.getByRole('link', { name: `Open ${title}` }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(title);
});
