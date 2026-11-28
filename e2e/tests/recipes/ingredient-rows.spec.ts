import { test, expect, type Locator, type Page } from '@playwright/test';
import { DEFAULT_TEST_USER, db, loginUser, sqlText } from '../../helpers/test-helpers';

// KRA-75: every ingredient row keeps one top line for its fields, keeps the
// "Addition details" toggle where it is when details open, and shows numbers
// at a readable precision. The row is the shared kit's BreweryCompactIngredientRow,
// reached through the recipe editor and the design-system showcase.

const WIDTHS = [
  { name: 'desktop', width: 1440, height: 1000 },
  { name: 'mobile', width: 390, height: 900 },
] as const;

const SIGNATURE_YIELD_FRACTION = '0.8006231877785952';

type Box = { x: number; y: number; width: number; height: number };
type FieldGeometry = { id: string; field: Box; label: Box; control: Box };

test.beforeEach(async ({ page }) => {
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
});

async function addFermentable(page: Page) {
  await page.getByRole('button', { name: 'Add fermentable', exact: true }).click();
  const picker = page.getByRole('dialog', { name: 'Build your grain bill' });
  await expect(picker).toBeVisible();
  await page.getByLabel('Search fermentables').fill('signature');
  await picker.getByRole('button', { name: /Joe White Signature Lager/ }).click();
  await page.getByLabel('Amount in recipe (kg)').fill('4.5');
  await picker.getByRole('button', { name: 'Add to recipe' }).click();
  await expect(picker).toBeHidden();
}

async function addManual(page: Page, opener: string, name: string, extra: (dialog: Locator) => Promise<void>) {
  await page.getByRole('button', { name: opener, exact: true }).click();
  const dialog = page.getByRole('dialog', { name: opener, exact: true });
  await dialog.getByLabel('Name', { exact: true }).fill(name);
  await extra(dialog);
  await dialog.getByRole('button', { name: 'Add to recipe', exact: true }).click();
  await expect(dialog).toBeHidden();
}

/** Every field of the row that is outside its details, with its label and control boxes. */
async function geometry(row: Locator): Promise<FieldGeometry[]> {
  return row.evaluate((element) => {
    const box = (node: Element): Box => {
      const r = node.getBoundingClientRect();
      return { x: r.x, y: r.y, width: r.width, height: r.height };
    };
    return [...element.querySelectorAll('.k-field')]
      .filter((field) => !field.closest('details'))
      .map((field) => ({
        id: (field.querySelector('input,select') as HTMLElement).id,
        field: box(field),
        label: box(field.querySelector('.k-field__label')!),
        control: box(field.querySelector('.k-field__control')!),
      }));
  });
}

/** Fields that share a visual line: their boxes overlap vertically. */
function lines(fields: FieldGeometry[]): FieldGeometry[][] {
  const sorted = [...fields].sort((a, b) => a.field.y - b.field.y);
  const out: FieldGeometry[][] = [];
  for (const field of sorted) {
    const line = out.find((members) => members.some((m) => field.field.y < m.field.y + m.field.height && m.field.y < field.field.y + field.field.height));
    if (line) line.push(field);
    else out.push([field]);
  }
  return out;
}

const box = (locator: Locator) => locator.evaluate((node) => {
  const r = node.getBoundingClientRect();
  return { x: r.x, y: r.y, width: r.width, height: r.height };
});

async function assertRowAligned(row: Locator, desktop: boolean, what: string) {
  const fields = await geometry(row);
  expect(fields.length, `${what}: fields found`).toBeGreaterThanOrEqual(3);
  const grouped = lines(fields);
  if (desktop) expect(grouped.length, `${what}: one line of fields on desktop: ${JSON.stringify(grouped.map((l) => l.map((f) => f.id)))}`).toBe(1);
  for (const line of grouped) {
    const labelTops = line.map((f) => f.label.y);
    const controlTops = line.map((f) => f.control.y);
    expect(Math.max(...labelTops) - Math.min(...labelTops), `${what}: label tops ${labelTops}`).toBeLessThanOrEqual(1);
    expect(Math.max(...controlTops) - Math.min(...controlTops), `${what}: control tops ${controlTops}`).toBeLessThanOrEqual(1);
  }
}

async function assertToggleStable(row: Locator, what: string) {
  const summary = row.locator('summary', { hasText: 'Addition details' });
  const description = row.locator('.k-compact-ingredient__description');
  const hasDescription = (await description.count()) > 0;
  const describe = async () => (hasDescription ? box(description) : null);
  const before = { fields: await geometry(row), summary: await box(summary), description: await describe() };
  await summary.click();
  const body = row.locator('.k-disclosure__body');
  await expect(body).toBeVisible();
  const after = { fields: await geometry(row), summary: await box(summary), description: await describe() };
  expect(after.summary, `${what}: toggle position`).toEqual(before.summary);
  expect(after.description, `${what}: description position`).toEqual(before.description);
  expect(after.fields, `${what}: fields stay put`).toEqual(before.fields);
  const opened = await box(body);
  const fieldsBottom = Math.max(...after.fields.map((f) => f.field.y + f.field.height));
  expect(opened.y, `${what}: details open below the fields`).toBeGreaterThanOrEqual(fieldsBottom - 0.5);
  expect(opened.y, `${what}: details open below the toggle`).toBeGreaterThanOrEqual(after.summary.y + after.summary.height - 0.5);
  await summary.click();
  await expect(body).toBeHidden();
  expect(await box(summary), `${what}: toggle position after closing`).toEqual(before.summary);
  expect(await geometry(row), `${what}: fields after closing`).toEqual(before.fields);
}

const rowTypes = [
  {
    kind: 'fermentable',
    name: 'Joe White Signature Lager',
    row: /^Joe White Signature Lager, fermentable/,
    add: (page: Page) => addFermentable(page),
  },
  {
    kind: 'hop',
    name: 'Cascade',
    row: /^Cascade, hop/,
    add: (page: Page) => addManual(page, 'Add hop', 'Cascade', async (dialog) => {
      await dialog.getByLabel('Amount (g)', { exact: true }).fill('20');
      await dialog.getByLabel('Use', { exact: true }).selectOption('whirlpool');
      await dialog.getByLabel('Contact time (min)', { exact: true }).fill('15');
    }),
  },
  {
    kind: 'yeast',
    name: 'Safale US-05',
    row: /^Safale US-05, yeast/,
    add: (page: Page) => addManual(page, 'Add yeast', 'Safale US-05', async (dialog) => {
      await dialog.getByLabel('Amount', { exact: true }).fill('1');
    }),
  },
  {
    kind: 'other',
    name: 'Irish moss',
    row: /^Irish moss, other ingredient/,
    add: (page: Page) => addManual(page, 'Add other ingredient', 'Irish moss', async (dialog) => {
      await dialog.getByLabel('Amount', { exact: true }).fill('5');
    }),
  },
] as const;

for (const viewport of WIDTHS) {
  for (const type of rowTypes) {
    test(`KRA75 ${type.kind} row fields share one top line and the details toggle stays put at ${viewport.name} width`, async ({ page }) => {
      await page.setViewportSize({ width: viewport.width, height: viewport.height });
      await page.goto('/recipe-editor/new');
      await type.add(page);
      const row = page.getByRole('group', { name: type.row });
      await expect(row).toHaveCount(1);
      await assertRowAligned(row, viewport.name === 'desktop', `${type.kind} ${viewport.name}`);
      await assertToggleStable(row, `${type.kind} ${viewport.name}`);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    });
  }

  test(`KRA75 the design-system showcase rows align and keep their toggle at ${viewport.name} width`, async ({ page }) => {
    await page.setViewportSize({ width: viewport.width, height: viewport.height });
    await page.goto('/design-system');
    const rows = page.locator('.k-compact-ingredient:not(:has(details[open]))');
    await expect(rows.first()).toBeVisible();
    const count = await rows.count();
    expect(count).toBeGreaterThanOrEqual(2);
    for (let index = 0; index < count; index++) {
      const row = rows.nth(index);
      await assertRowAligned(row, viewport.name === 'desktop', `showcase row ${index} ${viewport.name}`);
      await assertToggleStable(row, `showcase row ${index} ${viewport.name}`);
    }
    // The showcase demonstrates a row with a stage control and its details already open.
    const open = page.locator('#showcase-open-details-row');
    await expect(open.locator('details[open] summary', { hasText: 'Addition details' })).toBeVisible();
    await expect(open.getByLabel('Stage', { exact: true })).toBeVisible();
    await assertRowAligned(open, viewport.name === 'desktop', `showcase open-details row ${viewport.name}`);
  });
}

test('KRA75 a fermentable shows yield and colour rounded while the saved recipe keeps full precision', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  const title = `Rounded rows ${Date.now()}`;
  await page.getByLabel('Recipe name', { exact: true }).fill(title);
  await page.getByLabel('Batch volume (L)').fill('20');
  await addFermentable(page);
  const row = page.getByRole('group', { name: /^Joe White Signature Lager, fermentable/ });
  await row.locator('summary', { hasText: 'Addition details' }).click();
  await expect(row.getByLabel('Yield (%)', { exact: true })).toHaveValue('80.1');
  await expect(row.getByLabel('Colour (EBC)', { exact: true })).toHaveValue('3.6');
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 1' })).toBeVisible();
  expect(db.exec(`SELECT COUNT(*) FROM recipe_revisions WHERE document LIKE ${sqlText(`%${title}%`)} AND document LIKE ${sqlText(`%${SIGNATURE_YIELD_FRACTION}%`)}`)).toBe('1');
  await page.reload();
  const reloaded = page.getByRole('group', { name: /^Joe White Signature Lager, fermentable/ });
  await reloaded.locator('summary', { hasText: 'Addition details' }).click();
  await expect(reloaded.getByLabel('Yield (%)', { exact: true })).toHaveValue('80.1');
  await expect(reloaded.getByText('80.06231877785952')).toHaveCount(0);
});

test('KRA75 a value the brewer types keeps their exact text and is stored unrounded', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await addFermentable(page);
  const row = page.getByRole('group', { name: /^Joe White Signature Lager, fermentable/ });
  await row.locator('summary', { hasText: 'Addition details' }).click();
  const yieldField = row.getByLabel('Yield (%)', { exact: true });
  await yieldField.fill('81.234');
  await yieldField.blur();
  await expect(yieldField).toHaveValue('81.234');
});
