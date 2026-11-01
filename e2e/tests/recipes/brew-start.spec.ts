import { test, expect } from '@playwright/test';
import { DEFAULT_TEST_USER, loginUser } from '../../helpers/test-helpers';

test.beforeEach(async ({ page }) => {
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
});

async function importRecipe(page: import('@playwright/test').Page, title: string) {
  await page.goto('/beerxml');
  await page.getByLabel('Paste BeerXML').fill(
    `<RECIPES><RECIPE><NAME>${title}</NAME><BATCH_SIZE>20</BATCH_SIZE><BOIL_TIME>60</BOIL_TIME><FERMENTABLES><FERMENTABLE><NAME>Pilsner malt</NAME><AMOUNT>4.2</AMOUNT><TYPE>Grain</TYPE><YIELD>80</YIELD><COLOR>3</COLOR></FERMENTABLE></FERMENTABLES><HOPS><HOP><NAME>Hallertauer</NAME><AMOUNT>30</AMOUNT><USE>Boil</USE><TIME>60</TIME><ALPHA>5</ALPHA></HOP></HOPS></RECIPE></RECIPES>`,
  );
  await page.getByLabel('Source colour').selectOption('srm');
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Import preview' })).toBeVisible();
  await page.getByRole('button', { name: 'Import recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Recipe imported' })).toBeVisible();
  await page.getByRole('link', { name: 'Open imported recipe' }).click();
}

async function makeSavedRevisionReady(page: import('@playwright/test').Page) {
  await page.getByText('Equipment settings', { exact: true }).click();
  await page.locator('#equipment-profile-edit').selectOption('');
  await page.locator('#equipment-profile-name').fill('E2E kettle');
  await page.locator('#profile-mash-efficiency').fill('75');
  await page.locator('#profile-boil-off').fill('3');
  await page.locator('#profile-transfer-loss').fill('2');
  await page.locator('#profile-absorption').fill('0.8');
  await page.locator('#profile-thickness').fill('2.5');
  await page.locator('#profile-capacity').fill('30');
  await page.locator('#profile-diameter').fill('35');
  await page.locator('#profile-opening').fill('30');
  await page.locator('#profile-boil-temperature').fill('100');
  await page.locator('#profile-chilling').selectOption('natural');
  await page.getByRole('button', { name: 'Save profile' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Profile saved' })).toBeVisible();
  await page.locator('#equipment-profile-select').selectOption({ label: 'E2E kettle' });
  await page.getByText('Recipe process and notes', { exact: true }).click();
  await page.locator('#recipe-attenuation').fill('78');
  await page.locator('#recipe-mash-method').selectOption('manual');
  await page.locator('#recipe-mash-water').fill('15');
  await page.locator('#recipe-whirlpool').fill('15');
  await page.locator('#recipe-top-up').fill('0');
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
}

async function prepareAndStart(page: import('@playwright/test').Page, revision: string) {
  await page.locator('#brew-start-revision').fill(revision);
  await page.getByRole('button', { name: 'Prepare start' }).click();
  await expect(
    page.getByText('Ready to start the selected saved revision.'),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Start brew' }).click();
  await expect(page.getByText('Brew started', { exact: true })).toBeVisible();
}

test('starts the prepared saved revision with frozen targets', async ({ page }) => {
  const title = `Brew start ${Date.now()}`;
  await importRecipe(page, title);
  await makeSavedRevisionReady(page);
  await prepareAndStart(page, '2');
  await expect(page.getByText(`from revision 2`)).toBeVisible();
});

test('starts an explicitly selected historical revision without restoring it', async ({
  page,
}) => {
  const title = `Brew historical ${Date.now()}`;
  await importRecipe(page, title);
  await makeSavedRevisionReady(page);
  await page.getByLabel('Recipe name', { exact: true }).fill(`${title} head three`);
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 3' })).toBeVisible();
  await prepareAndStart(page, '2');
  await expect(page.getByText('from revision 2')).toBeVisible();
});
