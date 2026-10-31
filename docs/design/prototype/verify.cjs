// SPDX-License-Identifier: AGPL-3.0-or-later
// Meaningful browser checks for this interactive design artifact. Uses the
// repository's existing Playwright dependency; no second test stack is installed.
const assert = require('node:assert/strict');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || require.resolve('@playwright/test', {
  paths: [path.resolve(__dirname, '../../../e2e')],
}));
const base = process.env.PREVIEW_URL || 'http://127.0.0.1:18040';

(async () => {
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  let checks = 0;
  const check = (condition, message) => { assert.ok(condition, message); checks++; };
  try {
    await page.goto(`${base}/#recipe`);
    await page.waitForSelector('#metric-og');
    check(await page.locator('#metric-og [data-metric-value]').textContent() === '1.045', 'Rust fixture gravity');
    check(await page.locator('#metric-og [data-style-state]').getAttribute('data-style-state') === 'within', 'Initial gravity is within Helles style');
    check(await page.locator('#metric-batch [data-style-state]').count() === 0, 'Batch size has no fabricated style range');
    const rangeCases = await page.evaluate(async () => {
      const { StyleRange, Metric } = await import('./components.js');
      const parse = html => new DOMParser().parseFromString(html, 'text/html');
      const inputs = [null, undefined, '', NaN, Infinity, 0, 1, 2, 3];
      const states = inputs.map(value => parse(StyleRange({ label: 'Test', value, min: 1, max: 2 })).querySelector('[data-style-state]').dataset.styleState);
      const roundedMarkup = parse(Metric({ label: 'OG', value: '1.048', range: { min: 1.044, max: 1.048, value: 1.04804, digits: 3 } }));
      const rounded = roundedMarkup.querySelector('[data-style-state]').dataset.styleState;
      const expandedValue = roundedMarkup.querySelector('[data-metric-value]').textContent;
      const emptyMarkers = parse(StyleRange({ value: null, min: 1, max: 2 })).querySelectorAll('.k-style-range__marker').length;
      return { states, rounded, expandedValue, emptyMarkers };
    });
    check(JSON.stringify(rangeCases.states) === JSON.stringify(['awaiting', 'awaiting', 'awaiting', 'awaiting', 'awaiting', 'below', 'within', 'within', 'above']), 'Range boundaries and missing/non-finite estimates');
    check(rangeCases.rounded === 'above' && rangeCases.expandedValue === '1.04804' && rangeCases.emptyMarkers === 0, 'Range exposes boundary precision and hides missing marker');
    await page.locator('[name="style"]').selectOption('German Pils');
    check(await page.locator('#metric-ibu [data-style-state]').getAttribute('data-style-state') === 'below', 'Changing style updates current recipe comparison');
    check(await page.locator('#metric-ibu [data-style-state]').getAttribute('data-min') === '22', 'Pils reference comes from its own catalogue record');
    check(Math.abs(Number(await page.locator('#metric-ebc [data-style-state]').getAttribute('data-min')) - 3.94) < 1e-9, 'SRM reference converts consistently to EBC');
    await page.locator('[name="style"]').selectOption('Märzen');
    check(await page.locator('#metric-og [data-style-state]').getAttribute('data-style-state') === 'below', 'Märzen uses its higher gravity band');
    await page.locator('[name="style"]').selectOption('');
    check(await page.locator('#metric-og [data-style-state]').count() === 0, 'Experimental recipe has no invented guideline');
    await page.locator('[name="style"]').selectOption('Munich Helles');
    await page.locator('[name="grain-lager-amount"]').fill('3.8');
    check((await page.locator('#metric-og [data-metric-value]').textContent()).startsWith('1.043') && await page.locator('#metric-og [data-style-state]').getAttribute('data-style-state') === 'below', 'Near-boundary estimate does not look equal to a limit while outside');
    await page.locator('[name="grain-lager-amount"]').fill('4.5');
    check(await page.locator('#metric-og [data-metric-value]').textContent() !== '1.045', 'Ingredient changes recalculate');
    check(await page.locator('#metric-og [data-style-state]').getAttribute('data-style-state') === 'above', 'Grain edit moves marker above style');
    check(await page.locator('[data-action="save"]').isEnabled(), 'Outside-style guidance does not block saving');
    await page.locator('[name="hop-bittering-amount"]').fill('40');
    check((await page.locator('#hop-total').innerText()).includes('65 g total'), 'Hop total refreshes');
    await page.locator('[name="attenuation"]').fill('');
    check(await page.locator('#metric-og [data-metric-value]').textContent() === '—', 'Incomplete fields clear stale estimates');
    check(await page.locator('#metric-og [data-style-state]').getAttribute('data-style-state') === 'awaiting', 'Incomplete recipe clears stale range state');
    await page.locator('[name="attenuation"]').fill('82');
    await page.getByRole('tab', { name: /History/ }).click();
    await page.locator('#unsaved-dialog').waitFor({ state: 'visible' });
    await page.getByRole('button', { name: 'Stay here', exact: true }).click();
    check(await page.locator('[name="grain-lager-amount"]').inputValue() === '4.5', 'Stay preserves draft');
    await page.locator('[data-action="navigate:system"]').click();
    await page.getByRole('button', { name: 'Discard changes', exact: true }).click();
    await page.waitForURL('**/#system');
    check(await page.getByRole('heading', { name: 'Good design, on repeat.' }).count() === 1, 'Live component showcase');
    await page.locator('[data-action="navigate:recipe"]').first().click();
    check(await page.locator('[name="grain-lager-amount"]').inputValue() === '3.9', 'Discard restores saved values');
    await page.locator('[name="grain-lager-amount"]').fill('4.5');
    await page.locator('[data-action="navigate:library"]').first().click();
    await page.getByRole('button', { name: 'Save & continue', exact: true }).click();
    await page.waitForURL('**/#library');
    const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('krausen.design.recipe.v1')));
    check(saved.revision === 4 && saved.recipe.grains[0].kg === 4.5, 'Explicit save creates a revision');
    await page.locator('[data-action="navigate:recipe"]').first().click();
    await page.reload();
    await page.waitForSelector('#metric-og');
    check(await page.locator('[name="grain-lager-amount"]').inputValue() === '4.5', 'Saved recipe survives reload');
    await page.locator('[name="notes"]').fill('Unsaved recovery check');
    page.once('dialog', dialog => dialog.accept());
    await page.reload();
    await page.waitForSelector('#metric-og');
    check(await page.locator('[name="notes"]').inputValue() === 'Unsaved recovery check', 'Draft recovered after reload');
    await page.locator('[data-action="save"]').click();
    await page.waitForFunction(() => !document.querySelector('[data-save-state]').textContent.includes('Saving'));
    await page.locator('[data-action="start-brew"]').click();
    await page.waitForURL('**/#brews');
    const frozen = await page.evaluate(() => JSON.parse(localStorage.getItem('krausen.design.brews.v1'))[0]);
    await page.locator('[data-action="actuals:1"]').click();
    await page.locator('[name="actual-og"]').fill('1.050');
    await page.locator('[data-action="save-actuals:1"]').click();
    const actuals = await page.evaluate(() => JSON.parse(localStorage.getItem('krausen.design.brews.v1'))[0].actuals);
    check(actuals.og === 1.050 && actuals.fg === null, 'Actuals preserve missing readings');
    await page.locator('[data-action="navigate:recipe"]').first().click();
    await page.locator('[data-action="equipment"]').first().click();
    await page.locator('[name="profile-efficiency"]').fill('70');
    await page.locator('[data-action="profile-save"]').click();
    check(await page.locator('#metric-og [data-metric-value]').textContent() !== frozen.derived.og.toFixed(3), 'Profile updates preview');
    const stillFrozen = await page.evaluate(() => JSON.parse(localStorage.getItem('krausen.design.brews.v1'))[0]);
    check(stillFrozen.derived.og === frozen.derived.og && stillFrozen.equipment.efficiency === 78, 'Brew snapshot remains unchanged');
    await page.getByRole('button', { name: 'Add fermentable', exact: true }).click();
    await page.getByRole('button', { name: 'Add ingredient', exact: true }).click();
    check(await page.locator('.recipe-main .k-ingredient-row--grain').count() === 3, 'Ingredient picker uses editable rows');
    await page.locator('.recipe-main .k-ingredient-row--grain').last().getByRole('button').click();
    check(await page.locator('[data-action="save"]').isDisabled(), 'Reverted edits do not create a spurious revision');
    await page.getByRole('button', { name: 'Add fermentable', exact: true }).click();
    await page.locator('[name="library-search"]').fill('Weyermann');
    check(await page.locator('#catalogue-results .k-catalogue-item').count() === 3, 'Search matches fermentable name and producer');
    await page.locator('#catalogue-results .k-catalogue-item').first().focus();
    await page.keyboard.press('Enter');
    check(await page.evaluate(() => document.activeElement.matches('.k-catalogue-item[aria-pressed="true"]')), 'Keyboard selection preserves focus on the catalogue result');
    await page.locator('[name="library-search"]').fill('Neighbourhood Pale Malt');
    check(await page.locator('#catalogue-results .k-catalogue-item').count() === 0 && await page.locator('[data-action="add-selected-fermentable"]').isDisabled(), 'Empty search cannot add a hidden previous selection');
    await page.locator('[data-action="create-fermentable"]').first().click();
    await page.locator('[data-action="save-fermentable"]').click();
    check((await page.locator('#library-error').innerText()).includes('Enter a colour'), 'Incomplete custom specifications are rejected');
    await page.locator('[name="library-producer"]').fill('Local Maltster');
    await page.locator('[name="library-colour"]').fill('4.2');
    await page.locator('[name="library-yield"]').fill('81');
    await page.locator('[data-action="save-fermentable"]').click();
    check(await page.locator('#catalogue-results .k-catalogue-item').count() === 1 && await page.locator('#catalogue-selected').textContent() === 'Neighbourhood Pale Malt', 'Custom ingredient is saved and selected in Your library');
    check(await page.locator('.recipe-main .k-ingredient-row--grain').count() === 2, 'Saving a library entry does not silently add it to recipe');
    await page.locator('[name="library-amount"]').fill('0');
    await page.locator('[data-action="add-selected-fermentable"]').click();
    check((await page.locator('#catalogue-error').innerText()).includes('greater than zero'), 'Invalid amount is shown inside the open picker');
    await page.locator('[name="library-amount"]').fill('0.35');
    await page.locator('[data-action="add-selected-fermentable"]').click();
    check((await page.locator('.recipe-main .k-ingredient-row--grain').last().innerText()).includes('Neighbourhood Pale Malt'), 'Custom ingredient can be added to recipe');
    await page.locator('[data-action="save"]').click();
    await page.waitForFunction(() => !document.querySelector('[data-save-state]').textContent.includes('Saving'));
    const customRow = await page.evaluate(() => JSON.parse(localStorage.getItem('krausen.design.recipe.v1')).recipe.grains.at(-1));
    check(Math.abs(customRow.ppg - 46.214 * 0.81) < 1e-9 && customRow.ebc === 4.2 && customRow.catalogueVersion === 1, 'Recipe stores resolved custom yield/colour and provenance');
    await page.reload();
    await page.waitForSelector('#metric-og');
    await page.getByRole('button', { name: 'Add fermentable', exact: true }).click();
    await page.locator('[name="library-search"]').fill('Neighbourhood');
    check(await page.locator('#catalogue-results .k-catalogue-item').count() === 1, 'Custom library survives reload and remains searchable');
    await page.locator('[data-action="create-fermentable"]').first().click();
    await page.locator('[name="library-name"]').fill('Neighbourhood Pale Malt');
    await page.locator('[name="library-producer"]').fill('Local Maltster');
    await page.locator('[name="library-colour"]').fill('4.2');
    await page.locator('[name="library-yield"]').fill('81');
    await page.locator('[data-action="save-fermentable"]').click();
    check((await page.locator('#library-error').innerText()).includes('already in your library'), 'Duplicate library entries are not created accidentally');
    await page.locator('#custom-fermentable-dialog [data-action="close-dialog"]').click();
    await page.evaluate(() => {
      const key = 'krausen.design.workspace.home.fermentables.v1';
      const entries = JSON.parse(localStorage.getItem(key)); entries[0].ebc = 35; entries[0].yieldPercent = 75; entries[0].version = 2;
      localStorage.setItem(key, JSON.stringify(entries));
    });
    await page.reload();
    await page.waitForSelector('#metric-og');
    check(await page.evaluate(() => JSON.parse(localStorage.getItem('krausen.design.recipe.v1')).recipe.grains.at(-1).ebc) === 4.2, 'Library changes do not rewrite saved recipe specifications');
    for (const width of [390, 768, 1440]) {
      await page.setViewportSize({ width, height: 950 });
      const sizes = await page.evaluate(() => [document.documentElement.scrollWidth, innerWidth]);
      check(sizes[0] <= sizes[1], `No recipe overflow at ${width}px`);
      if (width === 390) {
        check(await page.locator('#compact-summary').isVisible(), 'Mobile calculation strip is visible');
        check(await page.locator('#compact-summary [data-style-state]').count() === 5, 'Mobile uses the same five style range indicators');
      }
    }
    await page.locator('[data-action="navigate:system"]').click();
    for (const width of [390, 1440]) {
      await page.setViewportSize({ width, height: 950 });
      check(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `No showcase overflow at ${width}px`);
    }
    await page.locator('[data-action="theme"]').first().click();
    check(await page.locator('html').getAttribute('data-theme') === 'dark', 'Dark tokens apply');
    await page.locator('[data-action="demo-dialog"]').click();
    check(await page.locator('dialog[open]').count() === 1 && await page.evaluate(() => document.querySelector('dialog[open]').contains(document.activeElement)), 'Dialog contains keyboard focus');
    await page.keyboard.press('Escape');
    check(await page.locator('dialog[open]').count() === 0, 'Escape closes dialog');
    check(errors.length === 0, `No JavaScript errors: ${errors.join(', ')}`);
    console.log(`${checks} prototype browser checks passed.`);
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
