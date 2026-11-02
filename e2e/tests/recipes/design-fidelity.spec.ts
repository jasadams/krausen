import { expect, test } from '@playwright/test';
import { DEFAULT_TEST_USER, loginUser } from '../../helpers/test-helpers';

// Equivalent ingredients, not fixed prototype calculation results.
const munichHelles = `<RECIPES><RECIPE><NAME>Munich Helles</NAME><VERSION>1</VERSION><BATCH_SIZE>20.5</BATCH_SIZE><BOIL_TIME>60</BOIL_TIME><EFFICIENCY>78</EFFICIENCY><FERMENTABLES><FERMENTABLE><NAME>Joe White Signature Lager</NAME><VERSION>1</VERSION><TYPE>Grain</TYPE><AMOUNT>3.9</AMOUNT><YIELD>80</YIELD><COLOR>1.8</COLOR></FERMENTABLE><FERMENTABLE><NAME>Weyermann Munich I</NAME><VERSION>1</VERSION><TYPE>Grain</TYPE><AMOUNT>0.3</AMOUNT><YIELD>78</YIELD><COLOR>7.1</COLOR></FERMENTABLE></FERMENTABLES><HOPS><HOP><NAME>Hallertau Mittelfrüh</NAME><VERSION>1</VERSION><ALPHA>3.8</ALPHA><AMOUNT>0.025</AMOUNT><USE>Boil</USE><TIME>60</TIME></HOP><HOP><NAME>Hallertau Mittelfrüh</NAME><VERSION>1</VERSION><ALPHA>3.8</ALPHA><AMOUNT>0.025</AMOUNT><USE>Boil</USE><TIME>15</TIME></HOP></HOPS><YEASTS><YEAST><NAME>Fermentis S-23</NAME><VERSION>1</VERSION><TYPE>Lager</TYPE><FORM>Dry</FORM><AMOUNT>0.011</AMOUNT></YEAST></YEASTS><STYLE><NAME>Munich Helles</NAME><VERSION>1</VERSION><STYLE_GUIDE>BJCP 2021</STYLE_GUIDE></STYLE></RECIPE></RECIPES>`;

// Rendered evidence complements, rather than replaces, the existing save,
// recovery, preview, start-brew and keyboard regression suites.
test.describe('approved recipe and shared kit composition', () => {
  test.beforeEach(async ({ page }) => {
    await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
  });

  test('history refreshes after saving while its tab is selected', async ({ page }) => {
    await page.goto('/recipe-editor/new');
    const title = page.getByLabel('Recipe name', { exact: true });
    await title.fill(`History refresh ${Date.now()}`);
    await page.getByRole('button', { name: 'Save recipe' }).click();
    await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
    await page.getByRole('tab', { name: 'History' }).click();
    await expect(page.locator('#editor-panel-history li').filter({ hasText: 'Revision 1' })).toBeVisible();
    await title.fill(`Updated history ${Date.now()}`);
    await page.getByRole('button', { name: 'Save recipe' }).click();
    await expect(page.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
    await expect(page.locator('#editor-panel-history li').filter({ hasText: 'Revision 2' })).toBeVisible();
    await page.getByRole('tab', { name: 'Brews' }).click();
    await expect(page.getByText('No brews have been started for this recipe.')).toBeVisible();
    await expect(page.getByText('Recipe not found.', { exact: true })).toHaveCount(0);
  });

  for (const theme of ['light', 'dark'] as const) {
    for (const viewport of [{ width: 1440, height: 1000 }, { width: 390, height: 844 }]) {
      test(`${theme} ${viewport.width}px`, async ({ page }, testInfo) => {
        await page.setViewportSize(viewport);
        await page.addInitScript(value => localStorage.setItem('krausen-theme', value), theme);
        await page.goto('/design-system');
        await expect(page.getByRole('tab', { name: 'Recipe' })).toBeVisible();
        await page.getByRole('tab', { name: 'Recipe' }).focus();
        await page.keyboard.press('ArrowRight');
        await expect(page.getByRole('tab', { name: 'History' })).toBeFocused();
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
        await testInfo.attach(`showcase-${theme}-${viewport.width}`, { body: await page.screenshot({ fullPage: true }), contentType: 'image/png' });

        await page.goto('/recipe-editor/new');
        const recipeTab = page.getByRole('tab', { name: 'Recipe', exact: true });
        await expect(recipeTab).toBeVisible();
        await expect(page.getByRole('tab', { name: /History/ })).toBeVisible();
        await expect(page.getByRole('tab', { name: /Brews/ })).toBeVisible();
        await expect(page.getByRole('button', { name: 'Add fermentable' })).toBeVisible();
        for (const name of ['Save recipe', 'Undo']) {
          const control = page.getByRole('button', { name, exact: true });
          await expect(control).toBeVisible();
          const box = await control.boundingBox();
          expect(box).not.toBeNull();
          expect(box!.x).toBeGreaterThanOrEqual(0);
          expect(box!.x + box!.width).toBeLessThanOrEqual(viewport.width);
        }
        await page.getByRole('button', { name: 'Save recipe', exact: true }).focus();
        await expect(page.getByRole('button', { name: 'Save recipe', exact: true })).toBeFocused();
        await recipeTab.focus();
        await page.keyboard.press('ArrowRight');
        const historyTab = page.getByRole('tab', { name: /History/ });
        await expect(historyTab).toBeFocused();
        await expect(historyTab).toHaveAttribute('aria-selected', 'true');
        await expect(page.getByText('No saved history yet')).toBeVisible();
        await recipeTab.click();
        await expect(page.getByRole('button', { name: 'Add fermentable' })).toBeVisible();
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
        await testInfo.attach(`recipe-${theme}-${viewport.width}`, { body: await page.screenshot({ fullPage: true }), contentType: 'image/png' });
      });
      test(`populated Munich Helles ${theme} ${viewport.width}px`, async ({ page }, testInfo) => {
        await page.setViewportSize(viewport);
        await page.addInitScript(value => localStorage.setItem('krausen-theme', value), theme);
        await page.goto('/beerxml');
        await page.getByLabel('Paste BeerXML').fill(munichHelles);
        await page.locator('#beerxml-colour').selectOption('srm');
        await page.locator('#beerxml-efficiency').selectOption('override');
        await page.getByRole('button', { name: 'Preview', exact: true }).click();
        await expect(page.getByRole('region', { name: 'Import preview' })).toContainText('Munich Helles');
        await page.getByRole('button', { name: 'Import recipe' }).click();
        await page.getByRole('link', { name: 'Open imported recipe' }).click();
        await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue('Munich Helles');
        await page.getByLabel('Beer style', { exact: true }).selectOption({ label: '4A · Munich Helles' });
        await page.getByRole('button', { name: 'Save recipe', exact: true }).click();
        await expect(page.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
        await expect.poll(() => page.locator('input').evaluateAll(inputs => inputs.some(input => (input as HTMLInputElement).value === 'Joe White Signature Lager'))).toBe(true);
        const formulationHeading = page.getByRole('heading', { name: 'The beer, at a glance' });
        await expect(formulationHeading).toBeVisible();
        const portrait = page.locator('.recipe-editor__preview-summary .beer-portrait');
        if (viewport.width <= 600) await expect(portrait).toBeHidden();
        else await expect(portrait).toBeVisible();
        await expect(page.getByRole('heading', { name: 'Your equipment' })).toBeVisible();
        await expect(page.getByRole('button', { name: 'Prepare start' })).toBeVisible();
        const ingredients = await page.getByRole('heading', { name: 'Fermentables' }).boundingBox();
        const startWorkflow = await page.locator('#start-brew').boundingBox();
        expect(ingredients).not.toBeNull();
        expect(startWorkflow).not.toBeNull();
        expect(ingredients!.y).toBeLessThan(startWorkflow!.y);
        await page.getByRole('button', { name: 'Start brew', exact: true }).first().click();
        await expect(page.getByRole('button', { name: 'Prepare start' })).toBeInViewport();
        await expect(page.locator('#start-brew')).toBeFocused();
        const save = page.getByRole('button', { name: 'Save recipe', exact: true });
        const saveBox = await save.boundingBox();
        expect(saveBox).not.toBeNull();
        expect(saveBox!.x).toBeGreaterThanOrEqual(0);
        expect(saveBox!.x + saveBox!.width).toBeLessThanOrEqual(viewport.width);
        await save.focus();
        await expect(save).toBeFocused();
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
        await testInfo.attach(`populated-recipe-${theme}-${viewport.width}`, { body: await page.screenshot({ fullPage: true }), contentType: 'image/png' });
      });
    }
  }
});
