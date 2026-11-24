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
    await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
    await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
    await page.getByRole('tab', { name: 'Saved versions' }).click();
    await expect(page.locator('#editor-panel-versions li').filter({ hasText: 'Revision 1' })).toBeVisible();
    await title.fill(`Updated history ${Date.now()}`);
    await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
    await expect(page.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
    await expect(page.locator('#editor-panel-versions li').filter({ hasText: 'Revision 2' })).toBeVisible();
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
        await expect(page.getByRole('tab', { name: 'Saved versions' })).toBeFocused();
        // KRA-49: the same kit component that reserves the recipe's style band rows
        // before an estimate exists is rendered in the actual showcase.
        await expect(page.getByTestId('showcase-style-placeholder')).toContainText('Awaiting estimate');
        await expect(page.getByTestId('showcase-style-placeholder').locator('.k-style-range__limits')).toHaveText('——');
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
        await testInfo.attach(`showcase-${theme}-${viewport.width}`, { body: await page.screenshot({ fullPage: true }), contentType: 'image/png' });

        await page.goto('/recipe-editor/new');
        const recipeTab = page.getByRole('tab', { name: 'Recipe', exact: true });
        await expect(recipeTab).toBeVisible();
        await expect(page.getByRole('tab', { name: /Saved versions/ })).toBeVisible();
        await expect(page.getByRole('tab', { name: /Brews/ })).toBeVisible();
        await expect(page.getByRole('button', { name: 'Add fermentable' })).toBeVisible();
        for (const name of ['Save recipe', 'Undo', 'Start brew', 'Recipe actions']) {
          const control = page.getByRole('button', { name, exact: true }).first();
          await expect(control).toBeVisible();
          const box = await control.boundingBox();
          expect(box).not.toBeNull();
          expect(box!.x).toBeGreaterThanOrEqual(0);
          expect(box!.x + box!.width).toBeLessThanOrEqual(viewport.width);
        }
        await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).focus();
        await expect(page.getByRole('button', { name: /^Save (recipe|changes)$/ })).toBeFocused();
        await recipeTab.focus();
        await page.keyboard.press('ArrowRight');
        const historyTab = page.getByRole('tab', { name: /Saved versions/ });
        await expect(historyTab).toBeFocused();
        await expect(historyTab).toHaveAttribute('aria-selected', 'true');
        await expect(page.getByText('No saved versions yet')).toBeVisible();
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
        await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
        await expect(page.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
        await expect.poll(() => page.locator('input').evaluateAll(inputs => inputs.some(input => (input as HTMLInputElement).value === 'Joe White Signature Lager'))).toBe(true);
        const formulationHeading = page.getByRole('heading', { name: 'The beer, at a glance' });
        await expect(formulationHeading).toBeVisible();
        const portrait = page.locator('.recipe-editor__preview-summary .beer-portrait');
        if (viewport.width <= 600) await expect(portrait).toBeHidden();
        else await expect(portrait).toBeVisible();
        await expect(page.getByRole('heading', { name: 'Your equipment' })).toBeVisible();
        const waterRows = page.locator('.k-panel').filter({ has: page.getByRole('heading', { name: 'Water plan' }) }).locator('.k-metric-list > .k-metric');
        expect(await waterRows.count()).toBeGreaterThanOrEqual(8);
        const lefts = await waterRows.evaluateAll(rows => rows.map(row => Math.round(row.getBoundingClientRect().left)));
        expect(new Set(lefts).size, 'water plan is one aligned column').toBe(1);
        await expect(page.getByRole('heading', { name: 'Water plan' }).locator('xpath=ancestor::section[1]').getByText('Total water', { exact: true })).toBeVisible();
        await expect(page.locator('.recipe-editor__preview-summary').getByText('Total water', { exact: true })).toHaveCount(0);
        // UX-004: the water plan sits with the mash inputs, and boil minutes has one owner (Boil & whirlpool).
        await expect(page.locator('.recipe-editor__preview-details').getByRole('heading', { name: 'Water plan' })).toHaveCount(0);
        await expect(page.locator('#recipe-section-mash').getByRole('heading', { name: 'Water plan' })).toBeVisible();
        // The plan sits with the mash inputs: after the allocation row, inside the same section.
        const order = await page.evaluate(() => {
          const section = document.getElementById('recipe-section-mash')!;
          const method = document.getElementById('recipe-mash-method')!;
          const plan = [...section.querySelectorAll('h3')].find(h => h.textContent === 'Water plan')!;
          return { inside: section.contains(plan), after: !!(method.compareDocumentPosition(plan) & Node.DOCUMENT_POSITION_FOLLOWING) };
        });
        expect(order).toEqual({ inside: true, after: true });
        await expect(page.getByLabel('Boil time (min)', { exact: true })).toHaveCount(1);
        await expect(page.locator('#recipe-section-boil').getByLabel('Boil time (min)', { exact: true })).toBeVisible();
        await expect(page.locator('.recipe-editor__preview-summary').getByText('Into fermenter', { exact: true })).toBeVisible();
        // INT-005: no permanent Prepare/revision workflow; Start opens a bounded task.
        await expect(page.getByRole('button', { name: 'Prepare start' })).toHaveCount(0);
        await expect(page.locator('#start-brew')).toHaveCount(0);
        await page.getByRole('button', { name: 'Start brew', exact: true }).first().click();
        const startDialog = page.getByRole('dialog', { name: 'Start brew' });
        await expect(startDialog).toBeVisible();
        await expect(startDialog).toBeInViewport();
        await expect(startDialog.getByTestId('start-source').or(startDialog.getByRole('heading', { name: 'Not ready to start' }))).toBeVisible();
        await expect(startDialog.getByRole('button', { name: /^(Cancel|Close)$/ }).first()).toBeVisible();
        await page.keyboard.press('Escape');
        await expect(startDialog).toBeHidden();
        const save = page.getByRole('button', { name: /^Save (recipe|changes)$/ });
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

  test('recipe header keeps one editable identity, state-aware primary and no duplicate history entrance', async ({ page }) => {
    await page.goto('/recipe-editor/new');
    const title = page.getByLabel('Recipe name', { exact: true });
    const save = page.getByRole('button', { name: /^Save (recipe|changes)$/ });
    const start = page.getByRole('group', { name: 'Recipe commands' }).getByRole('button', { name: 'Start brew', exact: true });
    await expect(page.getByRole('heading', { level: 1 })).toHaveCount(1);
    await expect(page.getByRole('heading', { level: 1 })).toHaveText('Recipe editor');
    await expect(save).toHaveText('Save recipe');
    await expect(save).toHaveClass(/bg-primary/);
    await expect(start).not.toHaveClass(/bg-primary/);
    await expect(start).toHaveAccessibleDescription('Save this recipe before starting a brew.');
    await expect(page.getByRole('status').filter({ hasText: 'Not saved yet' })).toBeVisible();
    await expect(page.getByRole('link', { name: 'Brew history' })).toHaveCount(0);
    await expect(page.getByRole('tab')).toHaveText(['Recipe', 'Saved versions', 'Brews']);
    const name = `Header ${Date.now()}`;
    await title.fill(name);
    await expect(page.getByRole('heading', { level: 1 })).toHaveText(name);
    await expect(page.getByRole('status').filter({ hasText: 'Unsaved changes' })).toBeVisible();
    const before = await save.boundingBox();
    await save.click();
    await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
    await expect(save).toHaveText('Save changes');
    const after = await save.boundingBox();
    expect(after!.x).toBeCloseTo(before!.x, 0);
    expect(after!.y).toBeCloseTo(before!.y, 0);
    await expect(start).toHaveAccessibleDescription(/complete recipe|estimate/);
    await expect(start).not.toHaveClass(/bg-primary/);
    await expect(save, 'a clean saved recipe that is not Ready has nothing to save').not.toHaveClass(/bg-primary/);
  });

  test('estimates name their source and header commands work from every tab', async ({ page }) => {
    await page.goto('/beerxml');
    await page.getByLabel('Paste BeerXML').fill(munichHelles);
    await page.locator('#beerxml-colour').selectOption('srm');
    await page.locator('#beerxml-efficiency').selectOption('override');
    await page.getByRole('button', { name: 'Preview', exact: true }).click();
    await page.getByRole('button', { name: 'Import recipe' }).click();
    await page.getByRole('link', { name: 'Open imported recipe' }).click();
    const commands = page.getByRole('group', { name: 'Recipe commands' });
    const save = commands.getByRole('button', { name: /^Save (recipe|changes)$/ });
    const start = commands.getByRole('button', { name: 'Start brew', exact: true });
    await page.getByLabel('Beer style', { exact: true }).selectOption({ label: '4A · Munich Helles' });
    await expect(page.getByTestId('estimate-source')).toContainText('changes not yet in saved revision');
    await expect(page.getByTestId('estimate-source')).toContainText('Equipment:');
    await expect(save).toHaveClass(/bg-primary/);
    await save.click();
    await expect(page.getByRole('status').filter({ hasText: /Saved revision \d+/ })).toBeVisible();
    await page.reload();
    // A clean, freshly loaded saved recipe reports the settled state, not a blank status.
    await expect(page.getByRole('group', { name: 'Recipe commands' }).getByRole('status')).toHaveText('Saved');
    await expect(page.getByTestId('estimate-source')).toContainText('matching saved revision');
    await expect(save, 'clean saved but Incomplete: nothing to save, nothing to start').not.toHaveClass(/bg-primary/);
    await expect(start).not.toHaveClass(/bg-primary/);
    await page.getByLabel('Batch volume (L)').fill('21');
    await expect(save).toHaveClass(/bg-primary/);
    await expect(page.getByTestId('estimate-source')).toContainText('changes not yet in saved revision');
    await page.getByRole('tab', { name: 'Saved versions' }).click();
    await start.click();
    const startDialog = page.getByRole('dialog', { name: 'Start brew' });
    await expect(startDialog.getByRole('heading', { name: 'Choose what to start' })).toBeVisible();
    await expect(startDialog).toBeVisible();
    await expect(startDialog.getByRole('button', { name: 'Save changes and continue' })).toBeVisible();
    await expect(startDialog.getByRole('button', { name: /^Use saved revision \d+$/ })).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(startDialog).toBeHidden();
    await expect(start).toBeFocused();
    await page.getByRole('tab', { name: 'Saved versions' }).click();
    await page.getByRole('tab', { name: 'Brews' }).click();
    await page.getByRole('button', { name: 'Recipe actions' }).click();
    await page.getByRole('button', { name: 'Print…' }).click();
    await expect(page.getByRole('tab', { name: 'Recipe', exact: true })).toHaveAttribute('aria-selected', 'true');
    await expect(page.locator('#recipe-output-disclosure')).toHaveJSProperty('open', true);
    await expect(page.locator('#recipe-output-disclosure > summary')).toBeFocused();
  });

  test('Recipe actions is an ordinary popover that returns focus and keeps editing reachable', async ({ page }) => {
    await page.goto('/recipe-editor/new');
    await page.getByLabel('Recipe name', { exact: true }).fill(`Actions ${Date.now()}`);
    await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
    await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
    const trigger = page.getByRole('button', { name: 'Recipe actions' });
    await trigger.click();
    for (const name of ['Scale…', 'Duplicate…', 'Print…', 'Export…']) await expect(page.getByRole('button', { name })).toBeVisible();
    await expect(page.getByRole('menu')).toHaveCount(0);
    await page.keyboard.press('Escape');
    await expect(trigger).toBeFocused();
    await trigger.click();
    await page.getByRole('button', { name: 'Export…' }).click();
    await expect(page.locator('#recipe-output-disclosure')).toHaveJSProperty('open', true);
    await page.getByRole('button', { name: 'Change equipment' }).click();
    await expect(page.locator('#recipe-equipment-disclosure')).toHaveJSProperty('open', true);
    await expect(page.locator('#equipment-profile-select')).toBeFocused();
  });

  for (const width of [320, 390]) {
    test(`long recipe name stays directly editable and in bounds at ${width}px`, async ({ page }) => {
      await page.setViewportSize({ width, height: 700 });
      await page.goto('/recipe-editor/new');
      const long = `Very long recipe name ${'with many words '.repeat(8)}${Date.now()}`;
      const title = page.getByLabel('Recipe name', { exact: true });
      await title.fill(long);
      await expect(page.getByRole('heading', { level: 1 })).toHaveText(long);
      await expect(title).toHaveAttribute('title', long);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      const add = page.getByRole('button', { name: 'Add fermentable' });
      await add.scrollIntoViewIfNeeded();
      await expect(add).toBeInViewport();
      for (const name of [/^Save (recipe|changes)$/, 'Start brew', 'Recipe actions']) {
        const box = await page.getByRole('button', { name }).first().boundingBox();
        expect(box!.x).toBeGreaterThanOrEqual(0);
        expect(box!.x + box!.width).toBeLessThanOrEqual(width);
      }
    });
  }
});
