import { test, expect } from '@playwright/test';
import { DEFAULT_TEST_USER, createSecondUser, db, getDefaultWorkspaceId, loginUser, walCheckpoint } from '../../helpers/test-helpers';

test.beforeEach(async ({ page }) => {
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
});

test('cancelling after preview creates no recipe', async ({ page }) => {
  const title = `Cancel import ${Date.now()}`;
  await page.goto('/beerxml');
  await page.getByLabel('Paste BeerXML').fill(`<RECIPES><RECIPE><NAME>${title}</NAME></RECIPE></RECIPES>`);
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Import preview' })).toContainText(title);
  await page.getByRole('link', { name: 'Cancel' }).click();
  expect(db.exec(`SELECT COUNT(*) FROM recipes WHERE name = '${title}'`)).toBe('0');
});

test('an older preview cannot replace a changed source', async ({ page }) => {
  await page.goto('/beerxml');
  let started!: () => void;
  const firstRequest = new Promise<void>(resolve => { started = resolve; });
  let delayed = false;
  await page.route('**/leptos-api/preview_beerxml*', async route => {
    const response = await route.fetch();
    if (!delayed) { delayed = true; started(); await new Promise(resolve => setTimeout(resolve, 800)); }
    await route.fulfill({ response });
  });
  const input = page.getByLabel('Paste BeerXML');
  await input.fill('<RECIPES><RECIPE><NAME>Old preview</NAME></RECIPE></RECIPES>');
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await firstRequest;
  await input.fill('<RECIPES><RECIPE><NAME>New preview</NAME></RECIPE></RECIPES>');
  await page.waitForTimeout(1000);
  await expect(page.getByRole('region', { name: 'Import preview' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Import preview' })).toContainText('New preview');
});

test('an uncertain import retries the same request without creating another recipe', async ({ page }) => {
  const title = `Retry import ${Date.now()}`;
  await page.goto('/beerxml');
  await page.getByLabel('Paste BeerXML').fill(`<RECIPES><RECIPE><NAME>${title}</NAME></RECIPE></RECIPES>`);
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Import preview' })).toContainText(title);
  const requestIds: string[] = [];
  await page.route('**/leptos-api/commit_beerxml*', async route => {
    const payload = route.request().postDataJSON();
    requestIds.push(payload.request_id);
    const response = await route.fetch();
    if (requestIds.length === 1) await route.fulfill({ status: 503, body: 'response lost' });
    else await route.fulfill({ response });
  });
  await page.getByRole('button', { name: 'Import recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Retry to recover' })).toBeVisible();
  await page.getByRole('button', { name: 'Import recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Recipe imported' })).toBeVisible();
  expect(requestIds).toHaveLength(2);
  expect(requestIds[1]).toBe(requestIds[0]);
  expect(db.exec(`SELECT COUNT(*) FROM recipes WHERE name = '${title}'`)).toBe('1');
});

test('import remains usable on a narrow dark display by keyboard', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.addInitScript(() => localStorage.setItem('krausen-theme', 'dark'));
  await page.goto('/beerxml');
  await expect(page.locator('html')).toHaveClass(/dark/);
  await page.getByLabel('Paste BeerXML').focus();
  await page.keyboard.type('<RECIPES><RECIPE><NAME>Keyboard</NAME></RECIPE></RECIPES>');
  await page.getByRole('button', { name: 'Preview', exact: true }).focus();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('region', { name: 'Import preview' })).toContainText('Keyboard');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test('an account switch hides a retained import and sends no source under the new account', async ({ page }) => {
  const email = `beerxml-switch-${Date.now()}@example.test`;
  const password = 'BeerXmlSwitchTest1234';
  await createSecondUser(null, { email, name: 'Other brewer', password, workspaceId: getDefaultWorkspaceId() });
  walCheckpoint();
  await page.goto('/beerxml');
  const source = page.getByLabel('Paste BeerXML');
  const privateText = 'Private BeerXML context check';
  await source.fill(`<RECIPES><RECIPE><NAME>${privateText}</NAME></RECIPE></RECIPES>`);
  const other = await page.context().newPage();
  await loginUser(other, email, password);
  const leaked: string[] = [];
  page.on('request', request => { if (request.postData()?.includes(privateText)) leaked.push(request.url()); });
  await page.bringToFront();
  await expect(page.getByText('Account or workspace changed')).toBeVisible();
  await expect(source).toBeHidden();
  expect(leaked).toEqual([]);
  await other.close();
});

test('an unsaved export is invalidated when the draft changes', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  const name = page.getByLabel('Recipe name', { exact: true });
  await name.fill('First draft');
  await page.getByRole('button', { name: 'Prepare unsaved draft' }).click();
  const download = page.getByRole('link', { name: 'Download BeerXML', exact: true });
  await expect(download).toBeVisible();
  await name.fill('Changed draft');
  await expect(download).toHaveCount(0);
  await expect(page.getByRole('status').filter({ hasText: 'The draft changed' })).toBeVisible();
});

test('file preview, explicit import and saved/original downloads share native values', async ({ page }) => {
  const title = `BeerXML browser ${Date.now()}`;
  const xml = `<RECIPES vendor="example"><RECIPE><NAME>${title}</NAME><EST_OG>1.055 SG</EST_OG><EFFICIENCY>75</EFFICIENCY><FERMENTABLES><FERMENTABLE><NAME>Malt</NAME><COLOR>3</COLOR></FERMENTABLE></FERMENTABLES></RECIPE></RECIPES>`;
  await page.goto('/beerxml');
  await page.getByLabel('Choose a UTF-8 BeerXML file').setInputFiles({ name: 'recipe.xml', mimeType: 'application/xml', buffer: Buffer.from(xml) });
  await expect(page.getByLabel('Paste BeerXML')).toHaveValue(xml);
  await page.getByLabel('Source colour').selectOption('srm');
  await page.getByLabel('Source efficiency').selectOption('override');
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Import preview' })).toContainText(title);
  await expect(page.getByRole('region', { name: 'Import preview' })).toContainText('attributes');
  await expect(page.getByRole('region', { name: 'Import preview' })).toContainText('1.055 SG');
  await page.getByRole('button', { name: 'Import recipe' }).click();
  const status = page.getByRole('status').filter({ hasText: 'Recipe imported' });
  await expect(status).toBeVisible();
  await page.getByRole('link', { name: 'Open imported recipe' }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(title);
  const editedTitle = `${title} edited`;
  await page.getByLabel('Recipe name', { exact: true }).fill(editedTitle);
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
  await page.getByLabel('Saved revision').fill('2');
  await page.getByRole('button', { name: 'Prepare saved revision' }).click();
  await expect(page.getByRole('button', { name: 'Acknowledge omissions and prepare download' })).toBeVisible();
  await page.getByRole('button', { name: 'Acknowledge omissions and prepare download' }).click();
  const href = await page.getByRole('link', { name: 'Download BeerXML', exact: true }).getAttribute('href');
  const rendered = Buffer.from(href!.split(',')[1], 'base64').toString();
  expect(rendered).toContain(`<NAME>${editedTitle}</NAME>`);
  expect(rendered).not.toContain(`<NAME>${title}</NAME>`);
  await page.getByRole('button', { name: 'Prepare unedited original' }).click();
  const original = await page.getByRole('link', { name: 'Download unedited original' }).getAttribute('href');
  expect(Buffer.from(original!.split(',')[1], 'base64').toString()).toBe(xml);
});
