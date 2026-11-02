import { expect, test, type Page, type TestInfo } from '@playwright/test';
import { DEFAULT_TEST_USER, loginUser } from '../../helpers/test-helpers';

const RECIPE_ID = 'bfe52fa7-16cc-47c3-86c5-d93d40688bc6';

async function attachScreenshot(page: Page, testInfo: TestInfo, name: string) {
  await page.evaluate(() =>
    Promise.race([
      document.fonts.ready.then(() => true),
      new Promise((resolve) => setTimeout(() => resolve(false), 5000)),
    ]),
  );
  try {
    const body = await page.screenshot({ fullPage: true, timeout: 60_000, animations: 'disabled' });
    await testInfo.attach(name, { body, contentType: 'image/png' });
  } catch {
    const client = await page.context().newCDPSession(page);
    const metrics = await client.send('Page.getLayoutMetrics');
    const { width, height } = metrics.cssContentSize;
    const shot = await client.send('Page.captureScreenshot', {
      format: 'png',
      captureBeyondViewport: true,
      clip: { x: 0, y: 0, width, height, scale: 1 },
    });
    await testInfo.attach(name, {
      body: Buffer.from(shot.data, 'base64'),
      contentType: 'image/png',
    });
  }
}


const FIRST_FERMENTABLE = 'Joe White Signature Lager';
const SECOND_FERMENTABLE = 'Weyermann Munich I';

test.setTimeout(120_000);

test.beforeEach(async ({ page }) => {
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
});

test('populated recipe uses compact two-row fermentables and keeps Hops in view', async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto(`/recipe-editor/${RECIPE_ID}`, { timeout: 60_000 });
  await expect(page.getByRole('heading', { name: 'Fermentables' })).toBeVisible({ timeout: 30_000 });
  await attachScreenshot(page, testInfo, 'compact-density-before-1440');

  const fermentables = page
    .locator('.recipe-editor__sections .k-panel')
    .filter({ has: page.getByRole('heading', { name: 'Fermentables' }) });
  const rows = fermentables.locator('.k-compact-ingredient');
  await expect(rows).toHaveCount(2);
  await expect(rows.nth(1)).toContainText('≈ 13.99 EBC');
  await expect(rows.nth(1)).not.toContainText('13.986999999999998');

  const panelBox = await fermentables.boundingBox();
  expect(panelBox, 'Fermentables panel must be visible').not.toBeNull();

  const geometry: Record<string, unknown> = {
    panelHeight: panelBox!.height,
    rows: [] as unknown[],
  };

  expect.soft(
    panelBox!.height,
    'two compact fermentable rows must fit the approved panel density',
  ).toBeLessThanOrEqual(320);

  for (let index = 0; index < 2; index += 1) {
    const row = rows.nth(index);
    const expectedName = index === 0 ? FIRST_FERMENTABLE : SECOND_FERMENTABLE;
    const rowBox = await row.boundingBox();
    expect(rowBox, `fermentable row ${index + 1} must be visible`).not.toBeNull();
    expect.soft(rowBox!.height, `fermentable row ${index + 1} must be compact`).toBeLessThanOrEqual(80);

    const name = row.locator('input').first();
    const amount = row.locator('input').nth(1);
    const details = row.locator('details summary');
    const remove = row.getByRole('button', { name: `Remove ${expectedName}` });
    const nameBox = await name.boundingBox();
    const amountBox = await amount.boundingBox();
    const detailsBox = await details.boundingBox();
    const removeBox = await remove.boundingBox();
    expect(nameBox, `row ${index + 1} name control`).not.toBeNull();
    expect(amountBox, `row ${index + 1} amount control`).not.toBeNull();
    expect(detailsBox, `row ${index + 1} disclosure`).not.toBeNull();
    expect(removeBox, `row ${index + 1} remove action`).not.toBeNull();
    expect(await name.inputValue()).toBe(expectedName);
    expect(
      Math.abs(removeBox!.y - nameBox!.y),
      `row ${index + 1} remove action must share the compact row line`,
    ).toBeLessThanOrEqual(24);
    expect(
      Math.abs(amountBox!.y - nameBox!.y),
      `row ${index + 1} amount must share the name line`,
    ).toBeLessThanOrEqual(8);
    expect(
      Math.abs(detailsBox!.y - nameBox!.y),
      `row ${index + 1} disclosure must share the compact row line`,
    ).toBeLessThanOrEqual(24);

    (geometry.rows as unknown[]).push({
      index,
      name: await name.inputValue(),
      rowHeight: rowBox!.height,
      nameTop: nameBox!.y,
      amountTop: amountBox!.y,
      removeTop: removeBox!.y,
      disclosureTop: detailsBox!.y,
      disclosureHeight: detailsBox!.height,
    });
  }

  await expect(fermentables.getByText('Grain bill total and shares unavailable.')).toBeVisible();
  await expect(fermentables.getByText('Grain bill', { exact: true })).toHaveCount(0);
  await expect(fermentables).not.toContainText('4.20 kg');

  const hops = page
    .locator('.recipe-editor__sections .k-panel')
    .filter({ has: page.getByRole('heading', { name: 'Hops' }) });
  const firstHop = hops.locator('.k-compact-ingredient').first();
  const firstHopBox = await firstHop.boundingBox();
  expect(firstHopBox, 'first hop row must be rendered').not.toBeNull();
  (geometry as Record<string, unknown>).firstHopBottom = firstHopBox!.y + firstHopBox!.height;
  expect.soft(
    firstHopBox!.y + firstHopBox!.height,
    'compact fermentables must keep the full first hop row inside the desktop viewport',
  ).toBeLessThanOrEqual(1000);

  await testInfo.attach('compact-density-geometry-1440', {
    body: JSON.stringify(geometry, null, 2),
    contentType: 'application/json',
  });

  await page.setViewportSize({ width: 390, height: 844 });
  await page.reload({ waitUntil: 'domcontentloaded', timeout: 60_000 });
  await expect(page.getByRole('heading', { name: 'Fermentables' })).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('.recipe-editor__preview-summary').getByText('Ready', { exact: true })).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('.recipe-editor__preview-summary').getByText('Original gravity', { exact: true })).toBeVisible();
  const mobileOriginalGravity = page.locator('.recipe-editor__preview-summary .k-metric').filter({ has: page.getByText('Original gravity', { exact: true }) });
  await expect(mobileOriginalGravity).toHaveCount(1);
  await expect(mobileOriginalGravity).toBeVisible();
  await expect(mobileOriginalGravity.locator('.k-metric__value')).toBeVisible();
  await expect(mobileOriginalGravity.locator('.k-metric__value')).toHaveText(/^1\.045\s*SG$/);
  const recipeTabs = page.getByRole('tablist', { name: 'Recipe sections' }).getByRole('tab');
  await expect(recipeTabs).toHaveCount(3);
  for (const tab of await recipeTabs.all()) {
    const bounds = await tab.boundingBox();
    expect(bounds).not.toBeNull();
    expect(bounds!.height).toBeGreaterThanOrEqual(44);
  }
  const recipeName = page.getByLabel('Recipe name', { exact: true });
  await expect(recipeName).toBeVisible();
  await recipeName.focus();
  await expect(recipeName).toBeFocused();
  const viewportStart = await page.getByRole('heading', { name: 'Fermentables' }).evaluate((heading) => {
    window.scrollTo(0, 0);
    const main = document.querySelector('main');
    if (!main) throw new Error('Main content is missing');
    const ancestors: HTMLElement[] = [];
    for (let current = heading.parentElement; current; current = current.parentElement) {
      const overflow = getComputedStyle(current).overflowY;
      if (/(auto|scroll)/.test(overflow) && current.scrollHeight > current.clientHeight) {
        current.scrollTop = 0;
        ancestors.push(current);
      }
    }
    const box = heading.getBoundingClientRect();
    return {
      windowY: window.scrollY,
      ancestorsAtTop: ancestors.every((element) => element.scrollTop === 0),
      unclipped: ancestors.every((element) => { const bounds = element.getBoundingClientRect(); return box.top >= bounds.top && box.bottom <= bounds.bottom; }),
    };
  });
  expect(viewportStart.windowY).toBe(0);
  expect(viewportStart.ancestorsAtTop).toBe(true);
  expect(viewportStart.unclipped).toBe(true);
  const batchBox = await page.getByLabel('Batch volume (L)', { exact: true }).boundingBox();
  const boilBox = await page.getByLabel('Boil time (min)', { exact: true }).boundingBox();
  expect(batchBox).not.toBeNull();
  expect(boilBox).not.toBeNull();
  expect(Math.abs(batchBox!.y - boilBox!.y)).toBeLessThanOrEqual(8);
  const grainHeading = await page.getByRole('heading', { name: 'Fermentables' }).boundingBox();
  expect(grainHeading).not.toBeNull();
  expect(grainHeading!.y).toBeGreaterThanOrEqual(0);
  expect(grainHeading!.y + grainHeading!.height).toBeLessThanOrEqual(844);
  await expect(rows).toHaveCount(2);
  await expect(rows.locator('input').first()).toBeVisible();
  await expect(rows.locator('input').nth(1)).toBeVisible();
  await expect(rows.locator('details summary').first()).toBeVisible();
  await expect(
    rows.getByRole('button', { name: `Remove ${FIRST_FERMENTABLE}` }).first(),
  ).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await attachScreenshot(page, testInfo, 'compact-density-after-390');
});

test('recipe editor header and formulation follow the approved hierarchy', async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto(`/recipe-editor/${RECIPE_ID}`, { timeout: 60_000 });
  await expect(page.getByRole('heading', { name: 'Fermentables' })).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('.recipe-editor__preview-summary')).toContainText('Ready');
  await attachScreenshot(page, testInfo, 'header-formulation-before-1440');

  const header = page.locator('.recipe-editor__header');
  const headerBox = await header.boundingBox();
  expect(headerBox, 'recipe header must be visible').not.toBeNull();
  expect(
    headerBox!.height,
    'desktop recipe header must use the approved compact topbar',
  ).toBeLessThanOrEqual(64);
  await expect(header.getByText('Recipes', { exact: true })).toBeVisible();
  await expect(header.getByText('Munich Helles', { exact: true })).toBeVisible();
  await expect(header.getByRole('status')).toHaveCount(1);
  await expect(header.getByRole('button', { name: 'Save recipe' })).toBeVisible();
  await expect(header.getByRole('link', { name: /history/i })).toBeVisible();

  const previewSummary = page.locator('.recipe-editor__preview-summary');
  const previewDetails = page.locator('.recipe-editor__preview-details');
  await expect(previewSummary.getByText('The beer, at a glance')).toBeVisible();
  await expect(previewSummary.getByText('Live', { exact: true })).toHaveCount(0);
  await expect(previewSummary.getByText('Ready', { exact: true })).toBeVisible();
  await expect(previewSummary.getByText('Original gravity')).toBeVisible();
  await expect(previewSummary.getByText('1.045')).toBeVisible();
  await expect(previewSummary.getByText('Final gravity')).toBeVisible();
  await expect(previewSummary.getByText('1.008')).toBeVisible();
  await expect(previewSummary.getByText('4.9')).toBeVisible();
  await expect(previewSummary.getByText('20.5')).toBeVisible();
  await expect(previewSummary.getByText('6.8')).toBeVisible();
  await expect(previewSummary.getByText('28.9')).toBeVisible();
  await expect(previewSummary.locator('.beer-portrait')).toBeVisible();

  await expect(previewDetails.getByRole('heading', { name: 'Water plan' })).toBeVisible();
  await expect(previewDetails.getByRole('heading', { name: 'Your equipment' })).toBeVisible();
  await expect(previewDetails.getByText('Grainfather G30 · revision 1')).toBeVisible();
  await expect(previewDetails.getByText('Mash efficiency')).toBeVisible();
  await expect(previewDetails.getByText('78.0%')).toBeVisible();
  await expect(previewDetails.getByText('Boil-off')).toBeVisible();
  await expect(previewDetails.getByText('3.0L/h')).toBeVisible();
  const summaryBox = await previewSummary.boundingBox();
  const detailsBox = await previewDetails.boundingBox();
  expect(summaryBox).not.toBeNull();
  expect(detailsBox).not.toBeNull();
  const railGap = detailsBox!.y - (summaryBox!.y + summaryBox!.height);
  expect(railGap).toBeGreaterThanOrEqual(0);
  expect(railGap).toBeLessThanOrEqual(24);

  await page.setViewportSize({ width: 390, height: 844 });
  await page.reload({ waitUntil: 'domcontentloaded', timeout: 60_000 });
  await expect(page.getByRole('heading', { name: 'Fermentables' })).toBeVisible({ timeout: 30_000 });
  await expect(header.getByText('Recipes', { exact: true })).toBeVisible();
  await expect(header.getByText('Munich Helles', { exact: true })).toBeVisible();
  await expect(header.getByRole('status')).toHaveCount(1);
  await expect(header.getByRole('button', { name: 'Save recipe' })).toBeVisible();
  await expect(previewSummary.getByRole('heading', { name: 'The beer, at a glance' })).toBeVisible();
  await expect(previewSummary.getByText('Estimates use the current draft.')).toBeVisible();
  await expect(previewSummary.getByText('Ready', { exact: true })).toBeVisible();
  await expect(previewDetails.getByRole('heading', { name: 'Water plan' })).toBeVisible();
  await expect(previewDetails.getByRole('heading', { name: 'Your equipment' })).toBeVisible();
  await expect(previewSummary.locator('.beer-portrait')).toBeHidden();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);

  const mobileOrder = await page.evaluate(() => {
    const summary = document.querySelector('.recipe-editor__preview-summary')!;
    const sections = document.querySelector('.recipe-editor__sections')!;
    const details = document.querySelector('.recipe-editor__preview-details')!;
    const layout = document.querySelector('.recipe-editor__layout')!;
    const follows = (first: Element, second: Element) =>
      Boolean(first.compareDocumentPosition(second) & Node.DOCUMENT_POSITION_FOLLOWING);
    return {
      layoutChildren: [...layout.children].map((child) => child.className),
      domSummaryBeforeSections: follows(summary, sections),
      domSectionsBeforeDetails: follows(sections, details),
      summaryTop: summary.getBoundingClientRect().top,
      sectionsTop: sections.getBoundingClientRect().top,
      detailsTop: details.getBoundingClientRect().top,
    };
  });
  expect(mobileOrder.domSummaryBeforeSections).toBe(true);
  expect(mobileOrder.domSectionsBeforeDetails).toBe(true);
  expect(mobileOrder.summaryTop).toBeLessThan(mobileOrder.sectionsTop);
  expect(mobileOrder.sectionsTop).toBeLessThan(mobileOrder.detailsTop);

  await testInfo.attach('header-formulation-mobile-order', {
    body: JSON.stringify(mobileOrder, null, 2),
    contentType: 'application/json',
  });
  await attachScreenshot(page, testInfo, 'header-formulation-after-390');
});

test('compact ingredient controls preserve focus, invalid raw input, disclosure, undo and reload', async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto(`/recipe-editor/${RECIPE_ID}`, { timeout: 60_000 });
  await expect(page.getByRole('heading', { name: 'Fermentables' })).toBeVisible({ timeout: 30_000 });

  const fermentables = page
    .locator('.recipe-editor__sections .k-panel')
    .filter({ has: page.getByRole('heading', { name: 'Fermentables' }) });
  const rows = fermentables.locator('.k-compact-ingredient');
  await expect(rows).toHaveCount(2);

  const header = page.locator('.recipe-editor__header');
  const status = header.getByRole('status');
  const undo = header.getByRole('button', { name: 'Undo' });
  const save = header.getByRole('button', { name: 'Save recipe' });
  await expect(save).toBeEnabled();
  await expect(undo).toBeDisabled();
  await expect(status).toHaveText('');

  const firstRow = rows.first();
  const name = firstRow.locator('input').first();
  const amount = firstRow.locator('input').nth(1);
  const details = firstRow.locator('details');
  const summary = details.locator('summary');

  await summary.focus();
  await page.keyboard.press('Enter');
  await expect(details).toHaveAttribute('open', '');
  await expect(details.locator('.k-disclosure__body')).toBeVisible();
  await page.keyboard.press('Enter');
  await expect(details).not.toHaveAttribute('open', '');

  await name.click();
  await name.press('End');
  await name.type('X', { delay: 30 });
  await expect(name).toHaveValue(`${FIRST_FERMENTABLE}X`);
  expect(
    await page.evaluate(() => document.activeElement?.tagName === 'INPUT'),
    'typing must keep focus in the ingredient name input',
  ).toBe(true);
  await expect(status).toContainText('Unsaved changes');
  await expect(undo).toBeEnabled();
  await undo.click();
  await expect(name).toHaveValue(FIRST_FERMENTABLE);
  await expect(undo).toBeDisabled();
  await expect(status).toHaveText('');

  await page.reload({ waitUntil: 'domcontentloaded', timeout: 60_000 });
  await expect(page.getByRole('heading', { name: 'Fermentables' })).toBeVisible({ timeout: 30_000 });
  await expect(rows).toHaveCount(2);
  await expect(rows.first().locator('input').first()).toHaveValue(FIRST_FERMENTABLE);
  await expect(rows.first().locator('input').nth(1)).toHaveValue('3.9');

  await rows.first().getByRole('button', { name: `Remove ${FIRST_FERMENTABLE}` }).click();
  await expect(rows).toHaveCount(1);
  await undo.click();
  await expect(rows).toHaveCount(2);
  await expect(rows.first().locator('input').first()).toHaveValue(FIRST_FERMENTABLE);

  await amount.fill('abc');
  await expect(amount).toHaveValue('abc');
  await expect(fermentables.getByText('Enter a number')).toBeVisible();
  const previewSummary = page.locator('.recipe-editor__preview-summary');
  await expect(previewSummary).toContainText('Complete invalid number fields to see estimates.');
  await expect(previewSummary.getByText('Original gravity')).toHaveCount(0);
  await expect(previewSummary.getByText('1.045')).toHaveCount(0);
  await attachScreenshot(page, testInfo, 'compact-ingredient-invalid-raw');
  await page.getByRole('navigation', { name: 'Recipe breadcrumb' }).getByRole('link', { name: 'Recipes' }).click();
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Stay' }).click();
  await expect(amount).toHaveValue('abc');
  await page.getByRole('navigation', { name: 'Recipe breadcrumb' }).getByRole('link', { name: 'Recipes' }).click();
  await guard.getByRole('button', { name: 'Discard and leave' }).click();
  await expect(page).toHaveURL(/\/recipes\/?$/);
  await page.goto(`/recipe-editor/${RECIPE_ID}`);
  await expect(page.locator('.recipe-editor__sections .k-panel').filter({ has: page.getByRole('heading', { name: 'Fermentables' }) }).locator('.k-compact-ingredient').first().locator('input').nth(1)).toHaveValue('3.9');
});

test('recipe topbar saves and guards a new draft', async ({ page }) => {
  await page.goto('/recipe-editor/new', { timeout: 60_000 });
  const title = page.getByLabel('Recipe name', { exact: true });
  const original = `Compact guard ${Date.now()}`;
  await title.fill(original);
  await page.getByRole('button', { name: 'Save recipe', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  const savedUrl = page.url();
  await title.fill(`${original} draft`);
  await page.getByRole('navigation', { name: 'Recipe breadcrumb' }).getByRole('link', { name: 'Recipes' }).click();
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Stay' }).click();
  await expect(title).toHaveValue(`${original} draft`);
  await page.getByRole('navigation', { name: 'Recipe breadcrumb' }).getByRole('link', { name: 'Recipes' }).click();
  await guard.getByRole('button', { name: 'Save and leave' }).click();
  await expect(page).toHaveURL(/\/recipes\/?$/);
  await page.goto(savedUrl);
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(`${original} draft`);
});
