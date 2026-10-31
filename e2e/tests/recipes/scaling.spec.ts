import { test, expect } from '@playwright/test';
import { DEFAULT_TEST_USER, loginUser } from '../../helpers/test-helpers';

test.beforeEach(async ({ page }) => {
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
});

async function createScalableRecipe(page: import('@playwright/test').Page, title: string) {
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(title);
  await page.getByLabel('Batch volume (L)').fill('20');
  await page.getByLabel('Boil time (min)').fill('60');
  await page.getByRole('button', { name: 'Add fermentable' }).click();
  await page.getByLabel('Name', { exact: true }).fill('Pale malt');
  await page.getByLabel('Amount (kg)', { exact: true }).fill('4');
  await page.getByText('Addition details', { exact: true }).click();
  await page.getByLabel('Type', { exact: true }).selectOption('grain');
  await page.getByLabel('Stage', { exact: true }).selectOption('mash');
  await page.getByLabel('Yield (%)', { exact: true }).fill('80');
  await page.getByLabel('Colour (EBC)', { exact: true }).fill('5');
  await page.getByText('Recipe process and notes', { exact: true }).click();
  await page.locator('#recipe-attenuation').fill('80');
  await page.locator('#recipe-mash-method').selectOption('manual');
  await page.locator('#recipe-mash-water').fill('15');
  await page.getByText('Equipment settings', { exact: true }).click();
  await page.locator('#recipe-efficiency').fill('75');
  await page.locator('#recipe-boil-off').fill('3');
  await page.locator('#recipe-loss').fill('2');
  await page.locator('#recipe-deadspace').fill('0');
  await page.locator('#recipe-absorption').fill('0.8');
  await page.locator('#recipe-capacity').fill('30');
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
}

test('scaling previews fixed-loss target changes and applies one revision', async ({ page }) => {
  const title = `Scaled recipe ${Date.now()}`;
  await createScalableRecipe(page, title);

  await page.getByRole('button', { name: 'Scale…' }).click();
  await expect(page.getByRole('heading', { name: 'Scale batch' })).toBeVisible();
  await page.getByLabel('Target fermenter volume (L)').fill('40');
  await page.getByRole('button', { name: 'Preview scaling' }).click();
  await expect(page.getByText(/20 L to 40 L/)).toBeVisible();
  await expect(page.getByText(/Kettle capacity 30\.00 L is below the scaled post-boil/)).toBeVisible();
  await expect(page.getByText(/Fixed losses stay unchanged/)).toBeVisible();

  await page.getByRole('button', { name: 'Apply scaling' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Scaled recipe saved as revision 2' })).toBeVisible();
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue(/^40(\.0+)?$/);
  await expect(page.getByLabel('Amount (kg)', { exact: true })).toHaveValue(/^8(\.0+)?$/);
});

test('scaling cancel writes nothing and duplicate opens an independent recipe', async ({ page }) => {
  const title = `Duplicate recipe ${Date.now()}`;
  await createScalableRecipe(page, title);

  await page.getByRole('button', { name: 'Scale…' }).click();
  await page.getByLabel('Target fermenter volume (L)').fill('30');
  await page.getByRole('button', { name: 'Preview scaling' }).click();
  await expect(page.getByText(/20 L to 30 L/)).toBeVisible();
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(page.getByRole('heading', { name: 'Scale batch' })).toHaveCount(0);
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue('20');

  await page.getByRole('button', { name: 'Duplicate…' }).click();
  await expect(page.getByRole('heading', { name: 'Duplicate recipe', exact: true })).toBeVisible();
  await page.getByLabel('New recipe name (optional)').fill(`${title} copy`);
  await page.getByRole('button', { name: 'Duplicate', exact: true }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(`${title} copy`);
  await expect(page).toHaveURL(/\/recipe-editor\/[0-9a-f-]+$/);
  await expect(page.getByLabel('Amount (kg)', { exact: true })).toHaveValue('4');
});
