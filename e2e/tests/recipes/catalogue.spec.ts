import { test, expect } from '@playwright/test';
import { DEFAULT_TEST_USER, loginUser } from '../../helpers/test-helpers';

test.beforeEach(async ({ page }) => {
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
});

test('the starter catalogue search copies a resolved specification into the recipe', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  const title = `Catalogue recipe ${Date.now()}`;
  await page.getByLabel('Recipe name', { exact: true }).fill(title);
  await page.getByLabel('Batch volume (L)').fill('20');

  await page.getByRole('button', { name: 'Add fermentable' }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill' });
  await expect(picker).toBeVisible();
  await expect(picker.getByRole('button', { name: /Weyermann Vienna/ })).toHaveCount(1);
  await expect(
    picker.locator('.k-catalogue-item', { hasText: 'Weyermann Vienna' }).getByText('Starter catalogue'),
  ).toBeVisible();
  await expect(
    picker.getByText('Specifications are copied into your recipe, so later library changes cannot change what you brewed.'),
  ).toBeVisible();

  const search = page.getByLabel('Search fermentables');
  await search.fill('signature');
  await expect(picker.getByRole('button', { name: /Joe White Signature Lager/ })).toHaveCount(1);
  await expect(picker.getByRole('button', { name: /Weyermann Vienna/ })).toHaveCount(0);
  await search.fill('no such malt anywhere');
  await expect(picker.getByText('Make this library your own')).toBeVisible();

  await search.fill('vienna');
  await expect(picker.getByRole('button', { name: /Weyermann Vienna/ })).toHaveCount(1);
  await picker.getByRole('button', { name: /Weyermann Vienna/ }).click();
  await expect(picker.locator('.k-catalogue-selection strong')).toHaveText('Weyermann Vienna');
  await page.getByLabel('Amount in recipe (kg)').fill('4.5');
  await page.getByRole('button', { name: 'Add ingredient' }).click();
  await expect(picker).toBeHidden();

  await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Weyermann Vienna');
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  await page.reload();
  await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Weyermann Vienna');
  await expect(page.getByLabel('Amount (kg)', { exact: true })).toHaveValue('4.5');
});

test('custom fermentables save to the workspace library with duplicate feedback', async ({ page }) => {
  const unique = Date.now();
  const entryName = `E2E local malt ${unique}`;
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(`Library recipe ${unique}`);
  await page.getByLabel('Batch volume (L)').fill('20');

  await page.getByRole('button', { name: 'Add fermentable' }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill' });
  const create = page.getByRole('dialog', { name: 'A fermentable of your own' });
  await expect(picker).toBeVisible();

  await page.getByLabel('Search fermentables').fill(`nothing-matches-${unique}`);
  await expect(picker.getByText('Make this library your own')).toBeVisible();
  await picker.getByRole('button', { name: 'Create fermentable', exact: true }).first().click();

  await expect(create).toBeVisible();
  await expect(
    create.getByText("Stored in your workspace's ingredient library. It will not modify the starter catalogue."),
  ).toBeVisible();
  await page.getByLabel('Fermentable name').fill(entryName);
  await page.getByLabel('Producer').fill('E2E maltster');
  await page.getByLabel('Type', { exact: true }).selectOption('Specialty malt');
  await page.getByLabel('Colour (EBC)').fill('4.5');
  await page.getByLabel('Potential yield (%)').fill('80');
  await page.getByRole('button', { name: 'Save to library' }).click();

  await expect(picker).toBeVisible();
  await expect(picker.getByRole('button', { name: entryName })).toHaveCount(1);
  await expect(
    picker.locator('.k-catalogue-item', { hasText: entryName }).getByText('Your library'),
  ).toBeVisible();
  await expect(picker.getByRole('button', { name: /4\.5 EBC · 80% yield/ })).toHaveCount(1);
  await picker.getByRole('button', { name: entryName }).click();
  await expect(picker.locator('.k-catalogue-selection strong')).toHaveText(entryName);
  await expect(picker.getByRole('button', { name: 'Add ingredient' })).toBeEnabled();
  await page.getByRole('button', { name: 'Add ingredient' }).click();
  await expect(picker).toBeHidden();
  await expect(page.getByLabel('Name', { exact: true })).toHaveValue(entryName);

  // Duplicate name and producer are rejected with feedback, case-insensitively.
  await page.getByRole('button', { name: 'Add fermentable' }).click();
  await picker.getByRole('button', { name: 'Create fermentable', exact: true }).first().click();
  await expect(create).toBeVisible();
  await create.getByLabel('Fermentable name').fill(entryName.toLowerCase());
  await create.getByLabel('Producer').fill('E2E MALTSTER');
  await create.getByLabel('Colour (EBC)').fill('5');
  await create.getByLabel('Potential yield (%)').fill('79');
  await page.getByRole('button', { name: 'Save to library' }).click();
  await expect(create.getByText('That fermentable and producer are already in your library.')).toBeVisible();

  await page.getByRole('button', { name: 'Back to catalogue' }).click();
  await expect(picker).toBeVisible();
  const libraryFilter = picker.getByRole('button', { name: 'Your library', exact: true });
  await libraryFilter.click();
  await expect(libraryFilter).toHaveAttribute('aria-pressed', 'true');
  await expect(picker.getByRole('button', { name: entryName })).toHaveCount(1);
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(picker).toBeHidden();

  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  await page.reload();
  await expect(page.getByLabel('Name', { exact: true })).toHaveValue(entryName);
});
