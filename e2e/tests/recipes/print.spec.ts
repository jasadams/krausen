import { test, expect } from '@playwright/test';
import { DEFAULT_TEST_USER, loginUser } from '../../helpers/test-helpers';

test.beforeEach(async ({ page }) => {
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
  await page.addInitScript(() => {
    (window as unknown as { __printed?: boolean }).__printed = false;
    Object.defineProperty(window, 'print', {
      configurable: true,
      value: () => {
        (window as unknown as { __printed?: boolean }).__printed = true;
      },
    });
  });
});

async function importRecipe(page: import('@playwright/test').Page, xml: string) {
  await page.goto('/beerxml');
  await page.getByLabel('Paste BeerXML').fill(xml);
  // The fixture carries a source colour, so the import requires an explicit
  // Lovibond/SRM choice before it can commit.
  await page.getByLabel('Source colour').selectOption('srm');
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Import preview' })).toBeVisible();
  await page.getByRole('button', { name: 'Import recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Recipe imported' })).toBeVisible();
  await page.getByRole('link', { name: 'Open imported recipe' }).click();
}

test('a saved revision prints the shared sheet and identifies its source', async ({ page }) => {
  const title = `Print saved ${Date.now()}`;
  await importRecipe(
    page,
    `<RECIPES><RECIPE><NAME>${title}</NAME><BATCH_SIZE>20</BATCH_SIZE><BOIL_TIME>60</BOIL_TIME><FERMENTABLES><FERMENTABLE><NAME>Pilsner malt</NAME><AMOUNT>4.2</AMOUNT><TYPE>Grain</TYPE><YIELD>80</YIELD><COLOR>3</COLOR></FERMENTABLE></FERMENTABLES><HOPS><HOP><NAME>Hallertauer</NAME><AMOUNT>30</AMOUNT><USE>Boil</USE><TIME>60</TIME><ALPHA>5</ALPHA></HOP></HOPS></RECIPE></RECIPES>`,
  );
  await page.getByRole('button', { name: 'Preview saved revision' }).click();
  const preview = page.getByRole('region', { name: 'Print preview' });
  await expect(preview).toBeVisible();
  await expect(preview).toContainText(`Saved revision 1`);
  await expect(preview).toContainText(title);
  await expect(preview).toContainText('Calculated targets');
  await expect(preview).toContainText('Equipment and water');
  await expect(preview).toContainText('Assumptions');
  await expect(preview).toContainText('Validation and unavailable data');
  await expect(preview).toContainText('Pilsner malt');

  await page.getByRole('button', { name: 'Print / Save as PDF' }).click();
  expect(await page.evaluate(() => (window as unknown as { __printed?: boolean }).__printed)).toBe(true);

  await page.emulateMedia({ media: 'print' });
  await expect(page.getByRole('button', { name: 'Print / Save as PDF' })).toBeHidden();
  await expect(page.locator('.print-sheet')).toBeVisible();
  await expect(page.locator('.print-hide').first()).toBeHidden();
  await page.emulateMedia({ media: 'screen' });
  await page.getByRole('button', { name: 'Back to editor' }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(title);
});

test('an unsaved draft is labelled and closing the preview preserves it', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  const name = page.getByLabel('Recipe name', { exact: true });
  await name.fill('Draft print');
  await page.getByRole('button', { name: 'Preview unsaved draft' }).click();
  const preview = page.getByRole('region', { name: 'Print preview' });
  await expect(preview).toBeVisible();
  await expect(preview).toContainText('Unsaved draft · not saved');
  await expect(preview).toContainText('Draft print');
  await expect(preview.getByText('Unavailable', { exact: true }).first()).toBeVisible();
  await page.getByRole('button', { name: 'Back to editor' }).click();
  await expect(name).toHaveValue('Draft print');
});

test('a long recipe prints every row with page-break controls and no width overflow', async ({ page }) => {
  const title = `Print long ${Date.now()}`;
  const hops = Array.from({ length: 40 }, (_, index) =>
    `<HOP><NAME>Hop ${String(index + 1).padStart(2, '0')}</NAME><AMOUNT>10</AMOUNT><USE>Boil</USE><TIME>${60 - (index % 30)}</TIME><ALPHA>5</ALPHA></HOP>`,
  ).join('');
  await importRecipe(
    page,
    `<RECIPES><RECIPE><NAME>${title}</NAME><BATCH_SIZE>20</BATCH_SIZE><BOIL_TIME>60</BOIL_TIME><FERMENTABLES><FERMENTABLE><NAME>Pale malt</NAME><AMOUNT>4.2</AMOUNT><TYPE>Grain</TYPE><YIELD>80</YIELD><COLOR>5</COLOR></FERMENTABLE></FERMENTABLES><HOPS>${hops}</HOPS></RECIPE></RECIPES>`,
  );
  await page.getByRole('button', { name: 'Preview saved revision' }).click();
  const hopsSection = page
    .locator('.print-sheet__section')
    .filter({ has: page.getByRole('heading', { name: 'Hops' }) });
  await expect(hopsSection.locator('tbody tr')).toHaveCount(40);
  await expect(hopsSection).toContainText('Hop 40');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);

  await page.emulateMedia({ media: 'print' });
  expect(await hopsSection.locator('tbody tr').first().evaluate((row) => getComputedStyle(row).breakInside)).toBe('avoid');
  expect(await hopsSection.locator('thead').evaluate((head) => getComputedStyle(head).display)).toBe('table-header-group');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});

test('leaving the editor unmounts the preview and clears the print class', async ({ page }) => {
  const title = `Print back ${Date.now()}`;
  await importRecipe(
    page,
    `<RECIPES><RECIPE><NAME>${title}</NAME><BATCH_SIZE>20</BATCH_SIZE><BOIL_TIME>60</BOIL_TIME><FERMENTABLES><FERMENTABLE><NAME>Pale malt</NAME><AMOUNT>4.2</AMOUNT><TYPE>Grain</TYPE><YIELD>80</YIELD><COLOR>5</COLOR></FERMENTABLE></FERMENTABLES></RECIPE></RECIPES>`,
  );
  await page.getByRole('button', { name: 'Preview saved revision' }).click();
  await expect(page.locator('body')).toHaveClass(/krausen-printing/);
  await page.goBack();
  await expect(page).toHaveURL(/\/beerxml$/);
  await expect(page.locator('body')).not.toHaveClass(/krausen-printing/);
});

test('a delayed summary cannot touch a disposed editor', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  const title = `Print delayed ${Date.now()}`;
  await importRecipe(
    page,
    `<RECIPES><RECIPE><NAME>${title}</NAME><BATCH_SIZE>20</BATCH_SIZE><BOIL_TIME>60</BOIL_TIME><FERMENTABLES><FERMENTABLE><NAME>Pale malt</NAME><AMOUNT>4.2</AMOUNT><TYPE>Grain</TYPE><YIELD>80</YIELD><COLOR>5</COLOR></FERMENTABLE></FERMENTABLES></RECIPE></RECIPES>`,
  );
  let started!: () => void;
  const requestStarted = new Promise<void>((resolve) => { started = resolve; });
  await page.route('**/leptos-api/request_process_summary*', async (route) => {
    started();
    try {
      const response = await route.fetch();
      await new Promise((resolve) => setTimeout(resolve, 1500));
      await route.fulfill({ response });
    } catch {
      // Navigation may cancel the in-flight request; the app still must not
      // touch disposed signals when its future resolves.
    }
  });
  await page.getByRole('button', { name: 'Preview saved revision' }).click();
  await requestStarted;
  await page.getByRole('link', { name: 'Recipes', exact: true }).first().click();
  await expect(page).toHaveURL(/\/recipes$/);
  await page.waitForTimeout(2000);
  await expect(page.locator('.print-sheet')).toHaveCount(0);
  await expect(page.locator('body')).not.toHaveClass(/krausen-printing/);
  expect(errors).toEqual([]);
});

test('a delayed summary denial cannot touch a disposed editor', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  const title = `Print denied ${Date.now()}`;
  await importRecipe(
    page,
    `<RECIPES><RECIPE><NAME>${title}</NAME><BATCH_SIZE>20</BATCH_SIZE><BOIL_TIME>60</BOIL_TIME><FERMENTABLES><FERMENTABLE><NAME>Pale malt</NAME><AMOUNT>4.2</AMOUNT><TYPE>Grain</TYPE><YIELD>80</YIELD><COLOR>5</COLOR></FERMENTABLE></FERMENTABLES></RECIPE></RECIPES>`,
  );
  let started!: () => void;
  const requestStarted = new Promise<void>((resolve) => { started = resolve; });
  await page.route('**/leptos-api/request_process_summary*', async (route) => {
    started();
    try {
      await new Promise((resolve) => setTimeout(resolve, 1500));
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify('Forbidden') });
    } catch {
      // Navigation may cancel the in-flight request; see the success case.
    }
  });
  await page.getByRole('button', { name: 'Preview saved revision' }).click();
  await requestStarted;
  await page.getByRole('link', { name: 'Recipes', exact: true }).first().click();
  await expect(page).toHaveURL(/\/recipes$/);
  await page.waitForTimeout(2000);
  await expect(page.locator('.print-sheet')).toHaveCount(0);
  await expect(page.locator('body')).not.toHaveClass(/krausen-printing/);
  expect(errors).toEqual([]);
});

test('a delayed summary denial redacts a mounted editor even after a draft edit', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  const title = `Print retained denial ${Date.now()}`;
  await importRecipe(
    page,
    `<RECIPES><RECIPE><NAME>${title}</NAME><BATCH_SIZE>20</BATCH_SIZE><BOIL_TIME>60</BOIL_TIME><FERMENTABLES><FERMENTABLE><NAME>Pale malt</NAME><AMOUNT>4.2</AMOUNT><TYPE>Grain</TYPE><YIELD>80</YIELD><COLOR>5</COLOR></FERMENTABLE></FERMENTABLES></RECIPE></RECIPES>`,
  );
  let started!: () => void;
  const requestStarted = new Promise<void>((resolve) => { started = resolve; });
  await page.route('**/leptos-api/request_process_summary*', async (route) => {
    started();
    try {
      await new Promise((resolve) => setTimeout(resolve, 1500));
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify('Forbidden') });
    } catch {
      // The assertion below still checks the mounted-editor redaction if the
      // request is cancelled.
    }
  });
  await page.getByRole('button', { name: 'Preview saved revision' }).click();
  await requestStarted;
  const name = page.getByLabel('Recipe name', { exact: true });
  await name.fill('Edited during print');
  await expect(page.getByText('Recipe held in its original workspace')).toBeVisible();
  await expect(name).toBeHidden();
  await expect(page.locator('.print-sheet')).toHaveCount(0);
  await expect(page.locator('body')).not.toHaveClass(/krausen-printing/);
  expect(errors).toEqual([]);
});

test('print uses the light paper palette in dark mode and the print class never leaks', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('krausen-theme', 'dark'));
  await page.goto('/recipe-editor/new');
  await expect(page.locator('html')).toHaveClass(/dark/);
  await page.getByLabel('Recipe name', { exact: true }).fill('Dark print');
  await page.getByRole('button', { name: 'Preview unsaved draft' }).click();
  await expect(page.locator('body')).toHaveClass(/krausen-printing/);
  await page.emulateMedia({ media: 'print' });
  expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe('rgb(255, 255, 255)');
  await page.emulateMedia({ media: 'screen' });
  await page.getByRole('button', { name: 'Back to editor' }).click();
  await expect(page.locator('body')).not.toHaveClass(/krausen-printing/);

  await page.getByRole('button', { name: 'Preview unsaved draft' }).click();
  await expect(page.locator('body')).toHaveClass(/krausen-printing/);
  await page.getByRole('button', { name: 'Back to editor' }).click();
  await expect(page.locator('body')).not.toHaveClass(/krausen-printing/);
});
