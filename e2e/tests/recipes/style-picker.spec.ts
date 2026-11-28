import { test, expect } from '@playwright/test';
import { DEFAULT_TEST_USER, loginUser, setAccountTheme } from '../../helpers/test-helpers';

test.beforeEach(async ({ page }) => {
  setAccountTheme('light');
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
});

test('every BJCP 2021 style is searchable by number and name and shows its ranges', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  const style = page.getByLabel('Beer style', { exact: true });
  const search = page.getByLabel('Find a style', { exact: true });
  // "No style" plus all 123 BJCP 2021 styles and substyles.
  await expect(style.locator('option')).toHaveCount(124);
  await expect(page.getByText('123 to choose from')).toBeVisible();

  await search.fill('belgian ipa');
  await expect(style.locator('option')).toHaveCount(2);
  await expect(page.getByText('1 of 123 match')).toBeVisible();
  await style.selectOption({ label: '21B · Specialty IPA: Belgian IPA' });
  await expect(page.getByLabel(/Original gravity\. .*Style reference 1\.058 to 1\.080 SG/)).toBeVisible();
  await expect(page.getByLabel(/Bitterness\. .*Style reference 50\.0 to 100\.0 IBU/)).toBeVisible();

  // Accents and case do not matter: "kolsch" finds Kölsch, "marzen" finds Märzen.
  await search.fill('KOLSCH');
  await expect(style.getByRole('option', { name: '5B · Kölsch' })).toBeAttached();
  await search.fill('marzen');
  await expect(style.getByRole('option', { name: '6A · Märzen' })).toBeAttached();

  // Searching never drops the style that is chosen.
  await search.fill('27a');
  await expect(page.getByText('9 of 123 match')).toBeVisible();
  await expect(style.locator('option')).toHaveCount(11);
  await expect(style).toHaveValue('Specialty IPA: Belgian IPA');
  await style.selectOption({ label: '27A · Historical Beer: Sahti' });
  await expect(page.getByLabel(/Original gravity\. .*Style reference 1\.076 to 1\.120 SG/)).toBeVisible();

  await search.fill('zzzz');
  await expect(page.getByText('No match', { exact: true })).toBeVisible();
  await search.fill('');
  await expect(style.locator('option')).toHaveCount(124);
});

test('a style without numeric ranges can be chosen and says so', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('status').filter({ hasText: 'Incomplete' })).toBeVisible();
  const style = page.getByLabel('Beer style', { exact: true });
  await style.selectOption({ label: '34C · Experimental Beer' });
  await expect(page.getByText('No numeric ranges')).toHaveCount(5);
  await expect(page.getByText(/Unavailable: /)).toHaveCount(0);
  await expect(page.getByText(/style reference is unavailable/i)).toHaveCount(0);
  await expect(style).toHaveValue('Experimental Beer');
  // Switching to a numeric style replaces the notices with ranges.
  await style.selectOption({ label: '4A · Munich Helles' });
  await expect(page.getByLabel(/Original gravity\. .*Style reference 1\.044 to 1\.048 SG/)).toBeVisible();
  await expect(page.getByText('No numeric ranges')).toHaveCount(0);
});

test('the style search is usable on a phone', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/recipe-editor/new');
  await expect(page.getByRole('heading', { name: 'Recipe editor' })).toBeVisible();
  const style = page.getByLabel('Beer style', { exact: true });
  const search = page.getByLabel('Find a style', { exact: true });
  await search.scrollIntoViewIfNeeded();
  for (const control of [search, style]) {
    const box = await control.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.x).toBeGreaterThanOrEqual(0);
    expect(box!.x + box!.width).toBeLessThanOrEqual(390);
    expect(box!.height).toBeGreaterThanOrEqual(40);
  }
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await search.fill('pils');
  await style.selectOption({ label: '5D · German Pils' });
  await expect(style).toHaveValue('German Pils');
  await expect(page.getByLabel(/Original gravity\. .*Style reference 1\.044 to 1\.050 SG/)).toBeVisible();
});
