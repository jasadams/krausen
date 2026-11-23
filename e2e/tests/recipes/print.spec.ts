import { test, expect } from '@playwright/test';
import { DEFAULT_TEST_USER, loginUser } from '../../helpers/test-helpers';
import { expectTabContainment } from '../../helpers/focus-boundary';

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
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Print…' }).click();
  const preview = page.getByRole('dialog', { name: 'Print preview' });
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
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Print…' }).click();
  const preview = page.getByRole('dialog', { name: 'Print preview' });
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
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Print…' }).click();
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
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Print…' }).click();
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
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Print…' }).click();
  await requestStarted;
  // KRA-43: the print task is modal (native dialog), so the sidebar link that used to
  // be clicked here is blocked by it (it cannot be clicked); the page is left with browser Back.
  await expect(page.locator('#sidebar-recipes-link').click({ trial: true, timeout: 1500 })).rejects.toThrow(/intercepts pointer events/);
  await page.goBack();
  await expect(page).toHaveURL(/\/beerxml$/);
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
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Print…' }).click();
  await requestStarted;
  // KRA-43: the print task is modal (native dialog), so the sidebar link that used to
  // be clicked here is blocked by it (it cannot be clicked); the page is left with browser Back.
  await expect(page.locator('#sidebar-recipes-link').click({ trial: true, timeout: 1500 })).rejects.toThrow(/intercepts pointer events/);
  await page.goBack();
  await expect(page).toHaveURL(/\/beerxml$/);
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
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Print…' }).click();
  await requestStarted;
  const name = page.getByLabel('Recipe name', { exact: true });
  await name.fill('Edited during print');
  await expect(page.getByText('Access denied', { exact: true })).toBeVisible();
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
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Print…' }).click();
  await expect(page.locator('body')).toHaveClass(/krausen-printing/);
  await page.emulateMedia({ media: 'print' });
  expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe('rgb(255, 255, 255)');
  await page.emulateMedia({ media: 'screen' });
  await page.getByRole('button', { name: 'Back to editor' }).click();
  await expect(page.locator('body')).not.toHaveClass(/krausen-printing/);

  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Print…' }).click();
  await expect(page.locator('body')).toHaveClass(/krausen-printing/);
  await page.getByRole('button', { name: 'Back to editor' }).click();
  await expect(page.locator('body')).not.toHaveClass(/krausen-printing/);
});

const SIMPLE_RECIPE = (title: string) =>
  `<RECIPES><RECIPE><NAME>${title}</NAME><BATCH_SIZE>20</BATCH_SIZE><BOIL_TIME>60</BOIL_TIME><FERMENTABLES><FERMENTABLE><NAME>Pale malt</NAME><AMOUNT>4.2</AMOUNT><TYPE>Grain</TYPE><YIELD>80</YIELD><COLOR>5</COLOR></FERMENTABLE></FERMENTABLES></RECIPE></RECIPES>`;

async function openPrint(page: import('@playwright/test').Page) {
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Print…' }).click();
}

test('the print preview is a real boundary: heading focus, inert page, Escape returns to the opener with draft and scroll intact', async ({ page }) => {
  const title = `Print boundary ${Date.now()}`;
  await importRecipe(page, SIMPLE_RECIPE(title));
  const name = page.getByLabel('Recipe name', { exact: true });
  await page.evaluate(() => window.scrollTo(0, 120));
  const scrollBefore = await page.evaluate(() => window.scrollY);
  await openPrint(page);
  const preview = page.getByRole('dialog', { name: 'Print preview' });
  await expect(preview).toBeVisible();
  await expect(preview.getByRole('heading', { name: 'Print preview' }), 'focus moves to the preview heading').toBeFocused();
  await expect(preview, 'the hint is the dialog description').toHaveAccessibleDescription(/paper size and margins/);
  await expect(page.locator('.recipe-editor')).toHaveAttribute('inert', '');
  // Keyboard focus never reaches the page behind the overlay.
  await expectTabContainment(page, '.print-preview', 14);
  await preview.getByRole('button', { name: 'Print / Save as PDF' }).focus();
  await expectTabContainment(page, '.print-preview', 14, { shift: true });
  await page.keyboard.press('Escape');
  await expect(preview).toHaveCount(0);
  await expect(page.locator('.recipe-editor')).not.toHaveAttribute('inert', /.*/);
  await expect(page.getByRole('button', { name: 'Recipe actions' }), 'focus returns to the opener').toBeFocused();
  await expect(name).toHaveValue(title);
  expect(await page.evaluate(() => window.scrollY)).toBe(scrollBefore);
  await expect(page.locator('body')).not.toHaveClass(/krausen-printing/);
});

test('a clean saved recipe prints its saved revision without asking for a source', async ({ page }) => {
  await importRecipe(page, SIMPLE_RECIPE(`Print known ${Date.now()}`));
  await openPrint(page);
  await expect(page.getByRole('dialog', { name: 'Print preview' })).toContainText('Saved revision 1');
  await expect(page.getByRole('dialog', { name: 'Print recipe' })).toHaveCount(0);
});

test('a dirty saved recipe asks once: saved revision or labelled unsaved draft', async ({ page }) => {
  const title = `Print choice ${Date.now()}`;
  await importRecipe(page, SIMPLE_RECIPE(title));
  const name = page.getByLabel('Recipe name', { exact: true });
  await name.fill(`${title} edited`);
  await openPrint(page);
  const task = page.getByRole('dialog', { name: 'Print recipe' });
  await expect(task).toContainText('Choose which source to print');
  await task.getByRole('button', { name: 'Preview saved revision 1' }).click();
  const preview = page.getByRole('dialog', { name: 'Print preview' });
  await expect(preview).toContainText('Saved revision 1');
  await expect(preview).toContainText(title);
  await expect(preview).not.toContainText(`${title} edited`);
  await page.getByRole('button', { name: 'Back to editor' }).click();
  await expect(name).toHaveValue(`${title} edited`);
  await openPrint(page);
  await page.getByRole('dialog', { name: 'Print recipe' }).getByRole('button', { name: 'Preview unsaved draft' }).click();
  const draftPreview = page.getByRole('dialog', { name: 'Print preview' });
  await expect(draftPreview).toContainText('Unsaved draft');
  await expect(draftPreview).toContainText(`${title} edited`);
});

test('a selected saved version supplies its own source and the draft is untouched', async ({ page }) => {
  const title = `Print selected ${Date.now()}`;
  await importRecipe(page, SIMPLE_RECIPE(title));
  const name = page.getByLabel('Recipe name', { exact: true });
  await name.fill(`${title} v2`);
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
  const id = page.url().match(/recipe-editor\/([0-9a-f-]+)/)![1];
  await page.goto(`/recipe-editor/${id}?section=versions&revision=1`);
  await name.fill('Local edit that stays');
  await openPrint(page);
  const preview = page.getByRole('dialog', { name: 'Print preview' });
  await expect(preview).toContainText('Saved revision 1');
  await expect(preview).toContainText(title);
  await expect(page.getByRole('dialog', { name: 'Print recipe' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Back to editor' }).click();
  await expect(name).toHaveValue('Local edit that stays');
});

test('dismissing the print task before its summary arrives opens no preview', async ({ page }) => {
  await importRecipe(page, SIMPLE_RECIPE(`Print dismissed ${Date.now()}`));
  let started!: () => void;
  const requestStarted = new Promise<void>((resolve) => { started = resolve; });
  await page.route('**/leptos-api/request_process_summary*', async (route) => {
    started();
    await new Promise((resolve) => setTimeout(resolve, 1200));
    await route.continue();
  });
  await openPrint(page);
  await requestStarted;
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog', { name: 'Print recipe' })).toBeHidden();
  await page.waitForTimeout(2000);
  await expect(page.getByRole('dialog', { name: 'Print preview' })).toHaveCount(0);
  await expect(page.locator('body')).not.toHaveClass(/krausen-printing/);
});

test('the permanent export and print disclosure is gone from the recipe', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByText('Export and print', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Preview saved revision' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Prepare unsaved draft' })).toHaveCount(0);
});

test('the page behind the preview stays inert when the editor remounts the preview during revalidation', async ({ page }) => {
  await importRecipe(page, SIMPLE_RECIPE(`Print remount ${Date.now()}`));
  await openPrint(page);
  const preview = page.getByRole('dialog', { name: 'Print preview' });
  await expect(preview).toBeVisible();
  // A window focus makes the editor revalidate, hide and restore the preview.
  for (let round = 0; round < 3; round++) {
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await expect(preview).toBeVisible();
    await expect(page.locator('.recipe-editor')).toHaveAttribute('inert', '');
    expect(await page.locator('#sidebar-recipes-link').evaluate((link) => !!link.closest('[inert]'))).toBe(true);
    await expect(page.locator('body'), 'the print-only page state survives a remount').toHaveClass(/krausen-printing/);
    await expect(preview.getByRole('heading', { name: 'Print preview' })).toBeFocused();
  }
  await expectTabContainment(page, '.print-preview', 10);
  await page.getByRole('button', { name: 'Back to editor' }).click();
  await expect(page.locator('.recipe-editor')).not.toHaveAttribute('inert', /.*/);
  expect(await page.locator('#sidebar-recipes-link').evaluate((link) => !!link.closest('[inert]'))).toBe(false);
});

for (const [width, height] of [[320, 568], [390, 844]]) {
  test(`the print preview fits ${width}x${height} in dark mode with reachable Back and paper palette`, async ({ page }) => {
    await page.addInitScript(() => localStorage.setItem('krausen-theme', 'dark'));
    await page.setViewportSize({ width, height });
    await importRecipe(page, SIMPLE_RECIPE(`Print narrow ${width} ${Date.now()}`));
    await openPrint(page);
    const preview = page.getByRole('dialog', { name: 'Print preview' });
    await expect(preview).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    const back = preview.getByRole('button', { name: 'Back to editor' });
    await back.scrollIntoViewIfNeeded();
    const box = await back.boundingBox();
    expect(box!.x).toBeGreaterThanOrEqual(0);
    expect(box!.x + box!.width).toBeLessThanOrEqual(width + 0.5);
    await page.emulateMedia({ media: 'print' });
    expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe('rgb(255, 255, 255)');
    await page.emulateMedia({ media: 'screen' });
    await back.click();
    await expect(page.locator('body')).not.toHaveClass(/krausen-printing/);
  });
}

test('a revalidation during the print request prepares it again instead of sticking on Preparing', async ({ page }) => {
  await importRecipe(page, SIMPLE_RECIPE(`Print revalidate ${Date.now()}`));
  let first = true;
  await page.route('**/leptos-api/request_process_summary*', async (route) => {
    if (first) {
      first = false;
      await new Promise((resolve) => setTimeout(resolve, 1200));
    }
    await route.continue();
  });
  await openPrint(page);
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByRole('dialog', { name: 'Print preview' })).toBeVisible({ timeout: 15000 });
  await expect(page.getByRole('dialog', { name: 'Print recipe' })).toHaveCount(0);
});

test('closing and reopening the print task while a request is in flight runs only the new request', async ({ page }) => {
  const title = `Print reopen ${Date.now()}`;
  await importRecipe(page, SIMPLE_RECIPE(title));
  await page.getByLabel('Recipe name', { exact: true }).fill(`${title} edited`);
  let calls = 0;
  await page.route('**/leptos-api/request_process_summary*', async (route) => {
    calls += 1;
    await new Promise((resolve) => setTimeout(resolve, 1200));
    await route.continue();
  });
  await openPrint(page);
  const task = page.getByRole('dialog', { name: 'Print recipe' });
  await task.getByRole('button', { name: 'Preview saved revision 1' }).click();
  await page.keyboard.press('Escape');
  await expect(task).toBeHidden();
  await openPrint(page);
  await expect(task.getByRole('button', { name: 'Preview saved revision 1' }), 'a closed run does not leave the task busy').toBeEnabled();
  await task.getByRole('button', { name: 'Preview unsaved draft' }).click();
  await page.waitForTimeout(2200);
  const preview = page.getByRole('dialog', { name: 'Print preview' });
  await expect(preview).toContainText('Unsaved draft');
  await expect(preview).not.toContainText('Saved revision 1');
  expect(calls).toBe(2);
});

test('raw numeric draft text survives opening and closing the print preview', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  const batch = page.getByLabel('Batch volume (L)');
  await page.getByLabel('Recipe name', { exact: true }).fill('Raw draft print');
  await batch.fill('20.000');
  await openPrint(page);
  await expect(page.getByRole('dialog', { name: 'Print preview' })).toContainText('Unsaved draft');
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog', { name: 'Print preview' })).toHaveCount(0);
  await expect(batch, 'the raw text, not a reformatted number').toHaveValue('20.000');
  await batch.fill('1e1');
  await openPrint(page);
  await page.getByRole('button', { name: 'Back to editor' }).click();
  await expect(batch).toHaveValue('1e1');
});

test('the design system showcase demonstrates the print preview boundary', async ({ page }) => {
  await page.goto('/design-system');
  await page.getByText('Printable recipe and process summary', { exact: true }).click();
  const open = page.getByRole('button', { name: 'Open print preview example' });
  await open.click();
  const preview = page.getByRole('dialog', { name: 'Print preview' });
  await expect(preview).toBeVisible();
  await expect(preview.getByRole('heading', { name: 'Print preview' })).toBeFocused();
  await expect(preview).toHaveAccessibleDescription(/paper size and margins/);
  await page.keyboard.press('Escape');
  await expect(preview).toHaveCount(0);
  await expect(open).toBeFocused();
});
