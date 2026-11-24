import { test, expect } from '@playwright/test';
import { DEFAULT_TEST_USER, loginUser, sqliteExec } from '../../helpers/test-helpers';
import { expectTabContainment } from '../../helpers/focus-boundary';

test.beforeEach(async ({ page }) => {
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
});

async function createScalableRecipe(page: import('@playwright/test').Page, title: string) {
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(title);
  await page.getByLabel('Batch volume (L)').fill('20');
  await page.getByLabel('Boil time (min)').fill('60');
  await page.getByRole('button', { name: 'Add fermentable' }).click();
  await expect(page.getByRole('heading', { name: 'Build your grain bill' })).toBeVisible();
  await page.getByLabel('Search fermentables').fill('Joe White Signature');
  await page.getByRole('button', { name: /Joe White Signature Lager/ }).click();
  await page.getByLabel('Amount in recipe (kg)').fill('4');
  await page.getByLabel('Build your grain bill').getByRole('button', { name: 'Add to recipe' }).click();
  await expect(page.getByLabel('Amount (kg)', { exact: true })).toHaveValue(/^4(\.0+)?$/);
  await page.locator('#recipe-attenuation').fill('80');
  await page.locator('#recipe-mash-method').selectOption('manual');
  await page.locator('#recipe-mash-water').fill('15');
  await page.getByText('Equipment settings', { exact: true }).click();
  await page.locator('#recipe-efficiency').fill('75');
  await page.locator('#recipe-boil-off').fill('3');
  await page.locator('#recipe-loss').fill('2');
  await page.locator('#recipe-deadspace').fill('0');
  await page.locator('#recipe-absorption').fill('0.8');
  await page.locator('#recipe-capacity').fill('30');
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
}

test('scaling previews fixed-loss target changes and applies one revision', async ({ page }) => {
  const title = `Scaled recipe ${Date.now()}`;
  await createScalableRecipe(page, title);

  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await expect(page.getByRole('heading', { name: 'Scale batch' })).toBeVisible();
  await page.getByLabel('Target fermenter volume (L)').fill('40');
  await page.getByRole('button', { name: 'Preview scaling' }).click();
  await expect(page.getByText(/20 L to 40 L/)).toBeVisible();
  await expect(page.getByText(/Kettle capacity 30\.00 L does not fit the scaled/)).toBeVisible();
  await expect(page.getByText(/Fixed losses stay unchanged/)).toBeVisible();

  await page.getByRole('button', { name: 'Apply scaling' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Scaled recipe saved as revision 2' })).toBeVisible();
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue(/^40(\.0+)?$/);
  await expect(page.getByLabel('Amount (kg)', { exact: true })).toHaveValue(/^8(\.0+)?$/);
});

test('scaling cancel writes nothing and duplicate opens an independent recipe', async ({ page }) => {
  const title = `Duplicate recipe ${Date.now()}`;
  await createScalableRecipe(page, title);

  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await page.getByLabel('Target fermenter volume (L)').fill('30');
  await page.getByRole('button', { name: 'Preview scaling' }).click();
  await expect(page.getByText(/20 L to 30 L/)).toBeVisible();
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(page.getByRole('heading', { name: 'Scale batch' })).toHaveCount(0);
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue('20');
  await expect(page.getByRole('button', { name: 'Recipe actions' }), 'focus returns to the visible opener after Cancel').toBeFocused();

  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Duplicate…' }).click();
  await expect(page.getByRole('heading', { name: 'Duplicate recipe', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(page.getByRole('heading', { name: 'Duplicate recipe', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Recipe actions' }), 'focus returns after cancelling Duplicate').toBeFocused();
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Duplicate…' }).click();
  await page.getByLabel('New recipe name (optional)').fill(`${title} copy`);
  await page.getByRole('button', { name: 'Duplicate', exact: true }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(`${title} copy`);
  await expect(page).toHaveURL(/\/recipe-editor\/[0-9a-f-]+$/);
  await expect(page.getByLabel('Amount (kg)', { exact: true })).toHaveValue('4');
  // A committed Duplicate navigation leaves focus to the new page; the closed
  // popover's opener must not reclaim it (KRA-48 owns arrival focus itself).
  await expect(page.getByRole('button', { name: 'Recipe actions' })).not.toBeFocused();
});

test('same-context focus during apply settles without losing the request', async ({ page }) => {
  const title = `Focus apply ${Date.now()}`;
  await createScalableRecipe(page, title);
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await page.getByLabel('Target fermenter volume (L)').fill('40');
  await page.getByRole('button', { name: 'Preview scaling' }).click();
  await expect(page.getByText(/20 L to 40 L/)).toBeVisible();
  await page.route('**/execute_scale_apply*', async route => {
    await new Promise(resolve => setTimeout(resolve, 1000));
    await route.continue();
  });
  await page.getByRole('button', { name: 'Apply scaling' }).click();
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByRole('status').filter({ hasText: 'Scaled recipe saved as revision 2' })).toBeVisible();
  await expect(page.getByRole('button', { name: /^Save (recipe|changes)$/ })).toBeEnabled();
});

test('uncertain apply keeps its identity through dismissal and blocks navigation', async ({ page }) => {
  const title = `Uncertain apply ${Date.now()}`;
  await createScalableRecipe(page, title);
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await page.getByLabel('Target fermenter volume (L)').fill('40');
  await page.getByRole('button', { name: 'Preview scaling' }).click();
  await expect(page.getByText(/20 L to 40 L/)).toBeVisible();
  await page.route('**/execute_scale_apply*', route => route.abort());
  await page.getByRole('button', { name: 'Apply scaling' }).click();
  await expect(page.getByRole('button', { name: 'Retry previous request' })).toBeVisible();
  await page.getByRole('button', { name: 'Cancel' }).click();
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await expect(page.getByRole('button', { name: 'Retry previous request' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Apply scaling' })).toBeDisabled();
  await page.getByRole('button', { name: 'Cancel' }).click();
  const recipes = page.getByRole('link', { name: 'Recipes', exact: true }).first();
  await recipes.click();
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Stay' }).click();
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await expect(page.getByRole('button', { name: 'Retry previous request' })).toBeVisible();
  await page.unroute('**/execute_scale_apply*');
  await page.getByRole('button', { name: 'Retry previous request' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Scaled recipe saved as revision 2' })).toBeVisible();
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue(/^40(\.0+)?$/);
  await expect(page.getByLabel('Amount (kg)', { exact: true })).toHaveValue(/^8(\.0+)?$/);
  // The retry must synchronize the committed revision, not leave the editor at rev1.
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
});

test('uncertain duplicate keeps its identity through dismissal', async ({ page }) => {
  const title = `Uncertain duplicate ${Date.now()}`;
  await createScalableRecipe(page, title);
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Duplicate…' }).click();
  await page.getByLabel('New recipe name (optional)').fill(`${title} copy`);
  await page.route('**/execute_duplicate_recipe*', route => route.abort());
  await page.getByRole('button', { name: 'Duplicate', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Retry previous request' })).toBeVisible();
  await page.getByRole('button', { name: 'Cancel' }).click();
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Duplicate…' }).click();
  await expect(page.getByRole('button', { name: 'Retry previous request' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Duplicate', exact: true })).toBeDisabled();
  await page.unroute('**/execute_duplicate_recipe*');
  await page.getByRole('button', { name: 'Retry previous request' }).click();
  await expect(page).toHaveURL(/\/recipe-editor\/[0-9a-f-]+$/);
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(`${title} copy`);
});

test('Scale and Duplicate explain the clean-saved prerequisite at invocation and never save', async ({ page }) => {
  await createScalableRecipe(page, `Prerequisite ${Date.now()}`);
  const name = page.getByLabel('Recipe name', { exact: true });
  await name.fill('Unsaved edit that must survive');
  const revisionsBefore = sqliteExec('SELECT COUNT(*) FROM recipe_revisions');
  for (const [action, dialogName] of [['Scale…', 'Scale batch'], ['Duplicate…', 'Duplicate recipe']]) {
    await page.getByRole('button', { name: 'Recipe actions' }).click();
    const item = page.getByRole('button', { name: action });
    await expect(item, 'the action is not an inert mystery button').toBeEnabled();
    await item.click();
    const dialog = page.getByRole('dialog', { name: dialogName });
    await expect(dialog).toContainText('Save your changes first; nothing is saved for you');
    await expect(dialog.getByRole('button', { name: /Preview scaling|^Duplicate$|Apply scaling/ })).toHaveCount(0);
    await expect(dialog.getByRole('button', { name: 'Close' })).toBeVisible();
    await dialog.getByRole('button', { name: 'Close' }).click();
    await expect(dialog).toBeHidden();
    await expect(page.getByRole('button', { name: 'Recipe actions' })).toBeFocused();
  }
  await expect(name).toHaveValue('Unsaved edit that must survive');
  expect(sqliteExec('SELECT COUNT(*) FROM recipe_revisions')).toBe(revisionsBefore);
});

test('a new unsaved recipe is told to save first when Scale is chosen', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await expect(page.getByRole('dialog', { name: 'Scale batch' })).toContainText('Save this recipe first; nothing is saved for you');
});

test('Scale is a real modal: focus is contained and the page behind is inert', async ({ page }) => {
  await createScalableRecipe(page, `Modal ${Date.now()}`);
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  const dialog = page.getByRole('dialog', { name: 'Scale batch' });
  await expect(dialog).toBeVisible();
  expect(await dialog.evaluate((element) => element.matches(':modal'))).toBe(true);
  expect(await page.evaluate(() => !!document.activeElement?.closest('dialog'))).toBe(true);
  await expectTabContainment(page, 'dialog#scale-dialog', 12);
  await dialog.getByLabel('Target fermenter volume (L)').focus();
  await expectTabContainment(page, 'dialog#scale-dialog', 12, { shift: true });
  // The trace may end with focus handed to the browser UI; Escape is the user's
  // action from inside the dialog.
  await dialog.getByRole('button', { name: 'Cancel' }).focus();
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  await expect(page.getByRole('button', { name: 'Recipe actions' })).toBeFocused();
});

test('a lost Scale response survives Escape and reload and replays the same request once', async ({ page }) => {
  await createScalableRecipe(page, `Reload scale ${Date.now()}`);
  const id = page.url().match(/recipe-editor\/([0-9a-f-]+)/)![1];
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await page.getByLabel('Target fermenter volume (L)').fill('40');
  await page.getByRole('button', { name: 'Preview scaling' }).click();
  await expect(page.getByText(/20 L to 40 L/)).toBeVisible();
  // The server commits, the response is lost.
  const requestIds: string[] = [];
  await page.route('**/execute_scale_apply*', async (route) => {
    requestIds.push(route.request().postData() ?? '');
    await route.fetch();
    await route.abort();
  });
  await page.getByRole('button', { name: 'Apply scaling' }).click();
  await expect(page.getByRole('button', { name: 'Retry previous request' })).toBeVisible();
  expect(sqliteExec(`SELECT COUNT(*) FROM recipe_revisions WHERE recipe_id = '${id}'`)).toBe('2');
  await page.keyboard.press('Escape');
  await page.unroute('**/execute_scale_apply*');
  page.once('dialog', (d) => d.accept());
  await page.reload();
  await expect(page.getByLabel('Recipe name', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await expect(page.getByRole('button', { name: 'Retry previous request' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Apply scaling' })).toBeDisabled();
  const replayed: string[] = [];
  await page.route('**/execute_scale_apply*', async (route) => {
    replayed.push(route.request().postData() ?? '');
    await route.continue();
  });
  await page.getByRole('button', { name: 'Retry previous request' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Scaled recipe saved as revision 2' })).toBeVisible();
  expect(replayed).toHaveLength(1);
  expect(replayed[0], 'the replay is the original request, not a fresh one').toBe(requestIds[0]);
  expect(sqliteExec(`SELECT COUNT(*) FROM recipe_revisions WHERE recipe_id = '${id}'`)).toBe('2');
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue(/^40(\.0+)?$/);
});

test('a lost Duplicate response survives reload and replays to exactly one new recipe', async ({ page }) => {
  const title = `Reload duplicate ${Date.now()}`;
  await createScalableRecipe(page, title);
  const before = Number(sqliteExec('SELECT COUNT(*) FROM recipes'));
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Duplicate…' }).click();
  await page.getByLabel('New recipe name (optional)').fill(`${title} copy`);
  const sent: string[] = [];
  await page.route('**/execute_duplicate_recipe*', async (route) => {
    sent.push(route.request().postData() ?? '');
    await route.fetch();
    await route.abort();
  });
  await page.getByRole('button', { name: 'Duplicate', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Retry previous request' })).toBeVisible();
  expect(Number(sqliteExec('SELECT COUNT(*) FROM recipes'))).toBe(before + 1);
  await page.keyboard.press('Escape');
  await page.unroute('**/execute_duplicate_recipe*');
  page.once('dialog', (d) => d.accept());
  await page.reload();
  await expect(page.getByLabel('Recipe name', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Duplicate…' }).click();
  await expect(page.getByRole('button', { name: 'Retry previous request' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Duplicate', exact: true })).toBeDisabled();
  const replayed: string[] = [];
  await page.route('**/execute_duplicate_recipe*', async (route) => {
    replayed.push(route.request().postData() ?? '');
    await route.continue();
  });
  await page.getByRole('button', { name: 'Retry previous request' }).click();
  await expect(page).toHaveURL(/\/recipe-editor\/[0-9a-f-]+$/);
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(`${title} copy`);
  expect(replayed).toHaveLength(1);
  expect(replayed[0]).toBe(sent[0]);
  expect(Number(sqliteExec('SELECT COUNT(*) FROM recipes'))).toBe(before + 1);
});

for (const [width, height] of [[320, 568], [390, 844], [844, 390]]) {
  test(`recipe task dialogs stay reachable without document overflow at ${width}x${height}`, async ({ page }) => {
    await page.setViewportSize({ width, height });
    await createScalableRecipe(page, `Narrow ${width} ${Date.now()}`);
    for (const [action, dialogName, primary] of [
      ['Scale…', 'Scale batch', 'Preview scaling'],
      ['Duplicate…', 'Duplicate recipe', 'Duplicate'],
      ['Export…', 'Export BeerXML', 'Close'],
    ]) {
      await page.getByRole('button', { name: 'Recipe actions' }).click();
      await page.getByRole('button', { name: action }).click();
      const dialog = page.getByRole('dialog', { name: dialogName });
      await expect(dialog).toBeVisible();
      const box = await dialog.boundingBox();
      expect(box!.x).toBeGreaterThanOrEqual(0);
      expect(box!.x + box!.width).toBeLessThanOrEqual(width + 0.5);
      expect(box!.y).toBeGreaterThanOrEqual(0);
      expect(box!.y + box!.height).toBeLessThanOrEqual(height + 0.5);
      const button = dialog.getByRole('button', { name: primary, exact: true });
      await button.scrollIntoViewIfNeeded();
      const action_box = await button.boundingBox();
      expect(action_box!.y + action_box!.height).toBeLessThanOrEqual(height + 0.5);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
      await dialog.getByRole('button', { name: /^(Cancel|Close)$/ }).click();
      await expect(dialog).toBeHidden();
    }
  });
}

test('an unreadable retained record blocks Scale and Duplicate until it is explicitly discarded', async ({ page }) => {
  await createScalableRecipe(page, `Broken record ${Date.now()}`);
  await page.evaluate(() => localStorage.setItem('krausen-transform-v1:garbage', 'not json'));
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  const dialog = page.getByRole('dialog', { name: 'Scale batch' });
  await expect(dialog.getByRole('alert')).toContainText('cannot be identified');
  await expect(dialog.getByRole('button', { name: 'Preview scaling' })).toBeDisabled();
  await dialog.getByRole('button', { name: 'Discard unreadable retained request' }).click();
  await expect(dialog.getByRole('alert')).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: 'Preview scaling' })).toBeEnabled();
  expect(await page.evaluate(() => localStorage.getItem('krausen-transform-v1:garbage'))).toBeNull();
});

test('a block caused by the other command is lifted once that command is resolved', async ({ page }) => {
  await createScalableRecipe(page, `Cross block ${Date.now()}`);
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await page.getByLabel('Target fermenter volume (L)').fill('40');
  await page.getByRole('button', { name: 'Preview scaling' }).click();
  await expect(page.getByText(/20 L to 40 L/)).toBeVisible();
  await page.route('**/execute_scale_apply*', (route) => route.abort());
  await page.getByRole('button', { name: 'Apply scaling' }).click();
  await expect(page.getByRole('button', { name: 'Retry previous request' })).toBeVisible();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Duplicate…' }).click();
  const duplicate = page.getByRole('dialog', { name: 'Duplicate recipe' });
  await expect(duplicate.getByRole('alert')).toContainText('A Scale request for this recipe is unresolved');
  await expect(duplicate.getByRole('button', { name: 'Duplicate', exact: true })).toBeDisabled();
  await duplicate.getByRole('button', { name: /^(Cancel|Close)$/ }).click();
  await page.unroute('**/execute_scale_apply*');
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await page.getByRole('button', { name: 'Retry previous request' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Scaled recipe saved as revision 2' })).toBeVisible();
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Duplicate…' }).click();
  await expect(duplicate.getByRole('alert')).toHaveCount(0);
  await expect(duplicate.getByRole('button', { name: 'Duplicate', exact: true })).toBeEnabled();
});

test('a Scale preview that finishes after Escape is not shown when reopened', async ({ page }) => {
  await createScalableRecipe(page, `Late preview ${Date.now()}`);
  await page.route('**/leptos-api/execute_scale_preview*', async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 1200));
    await route.continue();
  });
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await page.getByLabel('Target fermenter volume (L)').fill('40');
  await page.getByRole('button', { name: 'Preview scaling' }).click();
  await page.keyboard.press('Escape');
  await page.waitForTimeout(2000);
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await expect(page.getByRole('dialog', { name: 'Scale batch' })).toBeVisible();
  await expect(page.getByText(/20 L to 40 L/)).toHaveCount(0);
});

test('an open Scale preview survives the editor revalidating on window focus', async ({ page }) => {
  await createScalableRecipe(page, `Scale revalidate ${Date.now()}`);
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await page.getByLabel('Target fermenter volume (L)').fill('40');
  await page.getByRole('button', { name: 'Preview scaling' }).click();
  await expect(page.getByText(/20 L to 40 L/)).toBeVisible();
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByRole('dialog', { name: 'Scale batch' })).toBeVisible();
  await expect(page.getByText(/20 L to 40 L/)).toBeVisible();
  await expect(page.getByRole('button', { name: 'Apply scaling' })).toBeEnabled();
});

test('retrying a pending Scale never silently replaces an unsaved draft', async ({ page }) => {
  await createScalableRecipe(page, `Retry draft ${Date.now()}`);
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await page.getByLabel('Target fermenter volume (L)').fill('40');
  await page.getByRole('button', { name: 'Preview scaling' }).click();
  await expect(page.getByText(/20 L to 40 L/)).toBeVisible();
  await page.route('**/execute_scale_apply*', (route) => route.abort());
  await page.getByRole('button', { name: 'Apply scaling' }).click();
  await expect(page.getByRole('button', { name: 'Retry previous request' })).toBeVisible();
  await page.keyboard.press('Escape');
  await page.unroute('**/execute_scale_apply*');
  // The user keeps editing while the original request is still unresolved.
  const name = page.getByLabel('Recipe name', { exact: true });
  await name.fill('My unsaved edit');
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  const dialog = page.getByRole('dialog', { name: 'Scale batch' });
  await expect(dialog).toContainText('Save your changes first');
  const retry = dialog.getByRole('button', { name: 'Retry previous request' });
  await expect(retry, 'the retained request is explained and cannot overwrite the draft').toBeDisabled();
  await dialog.getByRole('button', { name: 'Close' }).click();
  await expect(name).toHaveValue('My unsaved edit');
  // Undo the edit, and the retained original request becomes retryable.
  await page.getByRole('button', { name: 'Undo' }).click();
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await expect(dialog.getByRole('button', { name: 'Retry previous request' })).toBeEnabled();
});

test('a changed-context apply keeps its fresh preview and message for review', async ({ page }) => {
  await createScalableRecipe(page, `Context conflict ${Date.now()}`);
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await page.getByLabel('Target fermenter volume (L)').fill('40');
  const previewResponse = page.waitForResponse('**/execute_scale_preview*');
  await page.getByRole('button', { name: 'Preview scaling' }).click();
  const outcome = await (await previewResponse).json();
  expect(outcome.Preview, 'the preview response carries the preview').toBeTruthy();
  // The apply reports that the accepted context changed and returns a fresh preview.
  await page.route('**/execute_scale_apply*', (route) =>
    route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ContextConflict: { fresh: outcome.Preview } }) }));
  await page.getByRole('button', { name: 'Apply scaling' }).click();
  const dialog = page.getByRole('dialog', { name: 'Scale batch' });
  await expect(dialog.getByRole('status')).toContainText('Review the fresh preview and apply again');
  await expect(dialog.getByText(/20 L to 40 L/)).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Apply scaling' })).toBeEnabled();
});

test('retrying a pending Duplicate never navigates away from an unsaved draft', async ({ page }) => {
  const title = `Duplicate retry draft ${Date.now()}`;
  await createScalableRecipe(page, title);
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Duplicate…' }).click();
  await page.getByLabel('New recipe name (optional)').fill(`${title} copy`);
  await page.route('**/execute_duplicate_recipe*', (route) => route.abort());
  await page.getByRole('button', { name: 'Duplicate', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Retry previous request' })).toBeVisible();
  await page.keyboard.press('Escape');
  await page.unroute('**/execute_duplicate_recipe*');
  const name = page.getByLabel('Recipe name', { exact: true });
  await name.fill('My unsaved edit');
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Duplicate…' }).click();
  const dialog = page.getByRole('dialog', { name: 'Duplicate recipe' });
  await expect(dialog).toContainText('Save your changes first');
  await expect(dialog.getByRole('button', { name: 'Retry previous request' })).toBeDisabled();
  await dialog.getByRole('button', { name: 'Close' }).click();
  await expect(name).toHaveValue('My unsaved edit');
  await page.getByRole('button', { name: 'Undo' }).click();
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Duplicate…' }).click();
  await expect(dialog.getByRole('button', { name: 'Retry previous request' })).toBeEnabled();
});

test('a retained Scale replay never rolls the editor back past a newer saved revision', async ({ page }) => {
  const title = `Replay moved head ${Date.now()}`;
  await createScalableRecipe(page, title);
  const id = page.url().match(/recipe-editor\/([0-9a-f-]+)/)![1];
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await page.getByLabel('Target fermenter volume (L)').fill('40');
  await page.getByRole('button', { name: 'Preview scaling' }).click();
  await expect(page.getByText(/20 L to 40 L/)).toBeVisible();
  // The server commits revision 2, the response is lost.
  await page.route('**/execute_scale_apply*', async (route) => { await route.fetch(); await route.abort(); });
  await page.getByRole('button', { name: 'Apply scaling' }).click();
  await expect(page.getByRole('button', { name: 'Retry previous request' })).toBeVisible();
  await page.keyboard.press('Escape');
  await page.unroute('**/execute_scale_apply*');
  page.once('dialog', (d) => d.accept());
  await page.reload();
  const name = page.getByLabel('Recipe name', { exact: true });
  await expect(name).toHaveValue(title);
  // The user keeps working on the scaled head and saves revision 3.
  await name.fill(`${title} saved later`);
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 3' })).toBeVisible();
  expect(sqliteExec(`SELECT COUNT(*) FROM recipe_revisions WHERE recipe_id = '${id}'`)).toBe('3');
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  const dialog = page.getByRole('dialog', { name: 'Scale batch' });
  await dialog.getByRole('button', { name: 'Retry previous request' }).click();
  // The recorded outcome is reported truthfully; the editor keeps revision 3.
  await expect(dialog).toContainText('An earlier Scale was saved as revision 2');
  await expect(dialog).toContainText('has changed since');
  await expect(page.getByRole('status').filter({ hasText: 'Scaled recipe saved as revision 2' })).toHaveCount(0);
  await dialog.getByRole('button', { name: /^(Cancel|Close)$/ }).click();
  await expect(name, 'the newer saved edit is still shown').toHaveValue(`${title} saved later`);
  // The record is released: nothing is retained any more.
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Scale…' }).click();
  await expect(dialog.getByRole('button', { name: 'Retry previous request' })).toHaveCount(0);
  expect(sqliteExec(`SELECT COUNT(*) FROM recipe_revisions WHERE recipe_id = '${id}'`)).toBe('3');
});

test('a retained Duplicate replay over a newer saved revision reports the copy instead of leaving the page', async ({ page }) => {
  const title = `Duplicate moved head ${Date.now()}`;
  await createScalableRecipe(page, title);
  const before = Number(sqliteExec('SELECT COUNT(*) FROM recipes'));
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Duplicate…' }).click();
  await page.getByLabel('New recipe name (optional)').fill(`${title} copy`);
  await page.route('**/execute_duplicate_recipe*', async (route) => { await route.fetch(); await route.abort(); });
  await page.getByRole('button', { name: 'Duplicate', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Retry previous request' })).toBeVisible();
  await page.keyboard.press('Escape');
  await page.unroute('**/execute_duplicate_recipe*');
  page.once('dialog', (d) => d.accept());
  await page.reload();
  const name = page.getByLabel('Recipe name', { exact: true });
  await expect(name).toHaveValue(title);
  await name.fill(`${title} saved later`);
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
  const url = page.url();
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Duplicate…' }).click();
  const dialog = page.getByRole('dialog', { name: 'Duplicate recipe' });
  await dialog.getByRole('button', { name: 'Retry previous request' }).click();
  await expect(dialog).toContainText('An earlier Duplicate created a copy of revision 1');
  await expect(dialog).toContainText('Find the copy in Recipes');
  expect(page.url(), 'the page is not replaced by the copy').toBe(url);
  await expect(name).toHaveValue(`${title} saved later`);
  expect(Number(sqliteExec('SELECT COUNT(*) FROM recipes'))).toBe(before + 1);
  await dialog.getByRole('button', { name: /^(Cancel|Close)$/ }).click();
  await page.getByRole('button', { name: 'Recipe actions' }).click();
  await page.getByRole('button', { name: 'Duplicate…' }).click();
  await expect(dialog.getByRole('button', { name: 'Retry previous request' })).toHaveCount(0);
});
