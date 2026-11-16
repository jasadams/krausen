import { test, expect, type Page } from '@playwright/test';
import { DEFAULT_TEST_USER, createSecondUser, db, getDefaultWorkspaceId, loginUser, sqlText, walCheckpoint } from '../../helpers/test-helpers';

test.beforeEach(async ({ page }) => {
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
});

// KRA-40 preparation: these assertions must first run on the accepted kit and
// unchanged importer. Captures separate intrinsic padding from layout gaps;
// neither source inspection nor test discovery is a geometry RED/GREEN result.
// Soft checks are still fatal to the test at completion. They let every real
// state/raw-source/retry/native-count assertion run before geometry is reported,
// rather than letting the first idle spacing defect hide the later outcomes.
const importGeometryExpect = expect.configure({ soft: true, timeout: 1000 });
async function measureImportActions(page: Page, state: string, primaryName: string, pending = false, quietName = 'Cancel') {
  const expect = importGeometryExpect;
  const panel = page.locator('.k-panel').filter({ has: page.getByRole('heading', { name: 'Import BeerXML', exact: true }) });
  const snapshot = await panel.evaluate((element, names) => {
    const root = getComputedStyle(document.documentElement);
    const rootSize = parseFloat(root.fontSize);
    const tokenPixels = (name: string) => {
      const value = root.getPropertyValue(name).trim();
      return value.endsWith('rem') ? parseFloat(value) * rootSize : parseFloat(value);
    };
    const bounds = (node: Element) => {
      const rect = node.getBoundingClientRect();
      return { x: rect.x, y: rect.y, width: rect.width, height: rect.height, right: rect.right, bottom: rect.bottom };
    };
    const actions = [...element.querySelectorAll<HTMLButtonElement | HTMLAnchorElement>('button, a')]
      .filter(node => names.includes(node.textContent?.trim() ?? ''))
      .filter(node => node.getClientRects().length > 0)
      .map(node => {
        const style = getComputedStyle(node);
        return {
          name: node.textContent!.trim(), rect: bounds(node),
          padding: { top: parseFloat(style.paddingTop), right: parseFloat(style.paddingRight), bottom: parseFloat(style.paddingBottom), left: parseFloat(style.paddingLeft) },
          background: style.backgroundColor, foreground: style.color,
          textClipped: node.scrollWidth > node.clientWidth + 1 || node.scrollHeight > node.clientHeight + 1,
          disabled: node.matches(':disabled'), busy: node.getAttribute('aria-busy'),
          inFooter: !!node.closest('.k-panel__footer'),
        };
      });
    const group = element.querySelector<HTMLElement>('[role="group"][aria-label="Import actions"]');
    const groupStyle = group ? getComputedStyle(group) : null;
    const fields = [...element.querySelectorAll<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>('input, select, textarea')]
      .filter(node => node.getClientRects().length > 0)
      .map(node => ({ id: node.id, rect: bounds(node), disabled: node.matches(':disabled'),
        labels: [...(node.labels ?? [])].map(label => label.textContent?.trim()),
        describedBy: node.getAttribute('aria-describedby'), invalid: node.getAttribute('aria-invalid'),
      }));
    return {
      viewport: { width: innerWidth, height: innerHeight, documentWidth: document.documentElement.scrollWidth },
      rootSize, actionGapToken: tokenPixels('--k-space-3'), intrinsicPaddingToken: tokenPixels('--k-space-4'),
      panel: bounds(element), fields, actions,
      group: group ? { rect: bounds(group), rowGap: parseFloat(groupStyle!.rowGap), columnGap: parseFloat(groupStyle!.columnGap) } : null,
      // Actual field/control separation remains inspectable independently of
      // the actions' padding and the footer's action-to-action layout.
      fieldGaps: fields.slice(1).map((field, index) => field.rect.y - fields[index].rect.bottom),
      actionGaps: actions.slice(1).map((action, index) => ({
        horizontal: action.rect.x - actions[index].rect.right,
        vertical: action.rect.y - actions[index].rect.bottom,
      })),
    };
  }, ['Preview', 'Import recipe', 'Check previous result', 'Acknowledge and clear retained request', 'Open imported recipe', quietName]);
  await test.info().attach(`import-${state}-geometry`, { body: JSON.stringify(snapshot, null, 2), contentType: 'application/json' });
  await test.info().attach(`import-${state}`, { body: await panel.screenshot(), contentType: 'image/png' });
  expect(snapshot.viewport.documentWidth, `${state}: document overflow`).toBeLessThanOrEqual(snapshot.viewport.width + 1);
  expect(Number.isFinite(snapshot.actionGapToken)).toBe(true);
  expect(Number.isFinite(snapshot.intrinsicPaddingToken)).toBe(true);
  const primary = snapshot.actions.find(action => action.name === primaryName)!;
  const cancel = snapshot.actions.find(action => action.name === quietName)!;
  expect(primary, `${state}: named stage action`).toBeDefined();
  expect(cancel, `${state}: retained Cancel`).toBeDefined();
  expect(new Set(snapshot.fields.map(field => field.id)).size).toBe(snapshot.fields.length);
  for (const field of snapshot.fields) {
    expect(field.labels.length, `${state}: ${field.id} persistent label`).toBeGreaterThan(0);
    expect(field.rect.x, `${state}: ${field.id} left edge`).toBeGreaterThanOrEqual(snapshot.panel.x - 1);
    expect(field.rect.right, `${state}: ${field.id} right edge`).toBeLessThanOrEqual(snapshot.panel.right + 1);
  }
  for (const action of snapshot.actions) {
    expect(action.rect.x, `${state}: ${action.name} left edge`).toBeGreaterThanOrEqual(snapshot.panel.x - 1);
    expect(action.rect.right, `${state}: ${action.name} right edge`).toBeLessThanOrEqual(snapshot.panel.right + 1);
    expect(action.textClipped, `${state}: ${action.name} label clipped`).toBe(false);
    expect(action.padding.left, `${state}: intrinsic padding differs from shared button`).toBeCloseTo(snapshot.intrinsicPaddingToken, 1);
    expect(action.padding.right).toBeCloseTo(snapshot.intrinsicPaddingToken, 1);
    if (snapshot.viewport.width <= 760) expect(action.rect.height, `${state}: touch target`).toBeGreaterThanOrEqual(43.5);
  }
  for (let index = 1; index < snapshot.actions.length; index++) {
    const before = snapshot.actions[index - 1].rect;
    const after = snapshot.actions[index].rect;
    const sameRow = Math.abs(before.y - after.y) < 1;
    const gap = sameRow ? after.x - before.right : after.y - before.bottom;
    expect(gap, `${state}: real action gap ${index}`).toBeGreaterThanOrEqual(snapshot.actionGapToken - 1);
  }
  // Hierarchy and actual kit ownership supplement, never replace, bounds and
  // intrinsic-padding measurements above.
  expect(cancel.background, `${state}: Cancel must not compete with primary`).not.toBe(primary.background);
  for (const action of snapshot.actions) {
    if (!pending && action.name !== primaryName && !action.disabled) {
      expect(action.background, `${state}: only ${primaryName} is primary`).not.toBe(primary.background);
    }
    expect(action.inFooter, `${state}: action belongs to shared task footer`).toBe(true);
  }
  const group = panel.getByRole('group', { name: 'Import actions', exact: true });
  await expect(group).toBeVisible();
  if (snapshot.group) {
    expect(snapshot.group.rowGap).toBeCloseTo(snapshot.actionGapToken, 1);
    expect(snapshot.group.columnGap).toBeCloseTo(snapshot.actionGapToken, 1);
  }
  const active = primaryName === 'Open imported recipe'
    ? panel.getByRole('link', { name: primaryName, exact: true })
    : panel.getByRole('button', { name: primaryName, exact: true });
  if (pending) {
    await expect(active).toBeDisabled();
    await expect(active).toHaveAttribute('aria-busy', 'true');
    await expect(active).toHaveAccessibleName(primaryName);
  } else {
    await expect(active).toBeEnabled();
    await active.scrollIntoViewIfNeeded();
    await active.focus();
    await expect(active).toBeFocused();
    await expect(active).toBeInViewport();
  }
  const quietCancel = panel.getByRole('link', { name: quietName, exact: true });
  await quietCancel.scrollIntoViewIfNeeded();
  await quietCancel.focus();
  await expect(quietCancel).toBeFocused();
  await expect(quietCancel).toBeInViewport();
}

for (const theme of ['light', 'dark'] as const) {
  for (const viewport of [
    { width: 1440, height: 900, enlarged: false },
    { width: 320, height: 480, enlarged: false },
    { width: 390, height: 600, enlarged: true },
  ]) {
    test(`import action geometry preserves all states ${theme} ${viewport.width}${viewport.enlarged ? ' enlarged text' : ''}`, async ({ page }) => {
      await page.setViewportSize({ width: viewport.width, height: viewport.height });
      await page.addInitScript(value => localStorage.setItem('krausen-theme', value), theme);
      await page.goto('/beerxml');
      if (theme === 'dark') await expect(page.locator('html')).toHaveClass(/dark/);
      else await expect(page.locator('html')).not.toHaveClass(/dark/);
      // Controlled text enlargement is not native browser/OS zoom evidence.
      if (viewport.enlarged) await page.evaluate(() => { document.documentElement.style.fontSize = '30px'; });
      const errors: string[] = [];
      page.on('pageerror', error => errors.push(error.message));
      const title = `KRA40 spacing ${theme} ${viewport.width} ${Date.now()}`;
      const xml = `<RECIPES vendor="spacing"><RECIPE><NAME>${title}</NAME><EFFICIENCY>75</EFFICIENCY><FERMENTABLES><FERMENTABLE><NAME>Malt</NAME><COLOR>3</COLOR></FERMENTABLE></FERMENTABLES></RECIPE></RECIPES>`;
      // Read-only observations happen BEFORE the geometry helper can refocus.
      // They do not move focus or infer a remount from source/view structure.
      const observeFocus = async (transition: string) => {
        const value = await page.evaluate(() => {
          const active = document.activeElement as HTMLElement | null;
          const rect = active?.getBoundingClientRect();
          const style = active ? getComputedStyle(active) : null;
          return {
            tag: active?.tagName, id: active?.id,
            name: (active?.getAttribute('aria-label') ?? active?.textContent ?? '').trim().slice(0, 160),
            connected: active?.isConnected ?? false,
            inImport: !!active?.closest('.k-panel')?.querySelector('#beerxml-source'),
            enabled: !!active && !active.matches(':disabled'),
            visible: !!rect && rect.width > 0 && rect.height > 0 && style?.visibility !== 'hidden',
            actions: [...document.querySelectorAll<HTMLButtonElement | HTMLAnchorElement>('.k-panel button, .k-panel a')]
              .filter(node => ['Preview', 'Import recipe', 'Open imported recipe', 'Cancel'].includes(node.textContent?.trim() ?? ''))
              .map(node => ({ name: node.textContent?.trim(), disabled: node.matches(':disabled'), busy: node.getAttribute('aria-busy'), focused: node === active })),
          };
        });
        await test.info().attach(`focus-${transition}-before-helper`, { body: JSON.stringify(value, null, 2), contentType: 'application/json' });
        return value;
      };
      const source = page.getByLabel('Paste BeerXML', { exact: true });
      await measureImportActions(page, 'idle', 'Preview');
      await page.getByLabel('Choose a UTF-8 BeerXML file').setInputFiles({ name: 'source-with-preserved-bytes.xml', mimeType: 'application/xml', buffer: Buffer.from(xml) });
      await expect(source).toHaveValue(xml);
      await measureImportActions(page, 'file', 'Preview');
      await source.fill(`\n${xml}\n`);
      await page.getByLabel('Recipe number in file').fill('1');
      await page.getByLabel('Source colour', { exact: true }).selectOption('srm');
      await page.getByLabel('Source efficiency', { exact: true }).selectOption('override');
      await measureImportActions(page, 'source', 'Preview');
      let releasePreview!: () => void;
      const previewGate = new Promise<void>(resolve => { releasePreview = resolve; });
      let previewStarted!: () => void;
      const previewRequest = new Promise<void>(resolve => { previewStarted = resolve; });
      await page.route('**/leptos-api/preview_beerxml*', async route => {
        const response = await route.fetch();
        previewStarted();
        await previewGate;
        await route.fulfill({ response });
      });
      try {
        await page.getByRole('button', { name: 'Preview', exact: true }).focus();
        await page.keyboard.press('Enter');
        await previewRequest;
        await observeFocus('preview-native-disabled');
        await measureImportActions(page, 'preview-loading', 'Preview', true);
        await observeFocus('preview-release-held-cancel');
      } finally { releasePreview(); }
      await expect(page.getByRole('region', { name: 'Import preview' })).toContainText(title);
      await page.unroute('**/leptos-api/preview_beerxml*');
      await observeFocus('preview-completed');
      await importGeometryExpect(page.getByRole('link', { name: 'Cancel', exact: true })).toBeFocused();
      await measureImportActions(page, 'preview', 'Import recipe');
      await source.fill('not valid BeerXML');
      await page.getByRole('button', { name: 'Preview', exact: true }).click();
      await expect(source).toHaveAttribute('aria-invalid', 'true');
      await expect(source).toHaveAccessibleDescription(/not a BeerXML recipe file/);
      await measureImportActions(page, 'error', 'Preview');
      await source.fill(`\n${xml}\n`);
      await page.getByRole('button', { name: 'Preview', exact: true }).click();
      await expect(page.getByRole('region', { name: 'Import preview' })).toContainText(title);
      let releaseImport!: () => void;
      const importGate = new Promise<void>(resolve => { releaseImport = resolve; });
      let importStarted!: () => void;
      const importRequest = new Promise<void>(resolve => { importStarted = resolve; });
      const bodies: string[] = [];
      await page.route('**/leptos-api/commit_beerxml*', async route => {
        bodies.push(route.request().postData() ?? '');
        const response = await route.fetch();
        if (bodies.length === 1) {
          importStarted();
          await importGate;
          await route.fulfill({ status: 503, body: 'response lost after real import' });
        } else await route.fulfill({ response });
      });
      try {
        await page.getByRole('button', { name: 'Import recipe', exact: true }).focus();
        await page.keyboard.press('Enter');
        await importRequest;
        await expect(source).toBeDisabled();
        await observeFocus('import-native-disabled');
        await measureImportActions(page, 'import-loading', 'Import recipe', true);
        await observeFocus('import-release-held-cancel');
      } finally { releaseImport(); }
      await expect(page.getByText('A recipe may already have been created')).toBeVisible();
      // The sent source stays pinned and unchangeable; no fresh Import is offered.
      await expect(source).toHaveValue(`\n${xml}\n`);
      await expect(source).toBeDisabled();
      await expect(page.getByRole('button', { name: 'Import recipe', exact: true })).toHaveCount(0);
      await observeFocus('import-unknown-reenabled');
      await importGeometryExpect(page.getByRole('link', { name: 'Back to recipes', exact: true })).toBeFocused();
      await measureImportActions(page, 'uncertain-retry', 'Check previous result', false, 'Back to recipes');
      await page.getByRole('button', { name: 'Check previous result', exact: true }).focus();
      await observeFocus('retry-import-before-dispatch');
      await page.keyboard.press('Enter');
      await expect(page.getByRole('status').filter({ hasText: 'Recipe imported.' })).toBeVisible();
      expect(bodies).toHaveLength(2);
      expect(bodies[0]).not.toBe('');
      expect(bodies[1]).toBe(bodies[0]);
      expect(db.exec(`SELECT COUNT(*) FROM recipes WHERE name = ${sqlText(title)}`)).toBe('1');
      const successor = await observeFocus('import-success-action-disappeared');
      importGeometryExpect(successor.connected && successor.inImport && successor.enabled && successor.visible).toBe(true);
      importGeometryExpect(['Preview', 'Open imported recipe', 'Cancel']).toContain(successor.name);
      await measureImportActions(page, 'confirmation', 'Open imported recipe');
      await page.getByRole('link', { name: 'Open imported recipe', exact: true }).focus();
      await page.keyboard.press('Enter');
      await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(title);
      expect(errors).toEqual([]);
    });
  }
}

test('cancelling after preview creates no recipe', async ({ page }) => {
  const title = `Cancel import ${Date.now()}`;
  await page.goto('/beerxml');
  await page.getByLabel('Paste BeerXML').fill(`<RECIPES><RECIPE><NAME>${title}</NAME></RECIPE></RECIPES>`);
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Import preview' })).toContainText(title);
  // Edited uncommitted source is guarded (INT-003); the explicit choice leaves.
  await page.getByRole('link', { name: 'Cancel' }).click();
  await page.locator('#workspace-draft-warning').getByRole('button', { name: 'Discard and leave' }).click();
  await expect(page).toHaveURL(/\/recipes$/);
  expect(db.exec(`SELECT COUNT(*) FROM recipes WHERE name = '${title}'`)).toBe('0');
});

const lostXml = (title: string) => `<RECIPES><RECIPE><NAME>${title}</NAME></RECIPE></RECIPES>`;
async function previewAndLoseImportResponse(page: Page, title: string, checkOutcome?: unknown) {
  await page.goto('/beerxml');
  await page.getByLabel('Paste BeerXML').fill(lostXml(title));
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Import preview' })).toContainText(title);
  const bodies: string[] = [];
  await page.route('**/leptos-api/commit_beerxml*', async route => {
    bodies.push(route.request().postData() ?? '');
    const response = await route.fetch();
    if (bodies.length === 1) await route.fulfill({ status: 503, body: 'response lost after real import' });
    else if (checkOutcome !== undefined) await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(checkOutcome) });
    else await route.fulfill({ response });
  });
  await page.getByRole('button', { name: 'Import recipe', exact: true }).click();
  await expect(page.getByText('A recipe may already have been created')).toBeVisible();
  return bodies;
}
const retainedKeys = (page: Page) => page.evaluate(() => Object.keys(localStorage).filter(key => key.startsWith('krausen-import-v1:')));

test('leaving edited uncommitted source asks Discard or Stay and never imports', async ({ page }) => {
  const title = `Guarded import ${Date.now()}`;
  await page.goto('/beerxml');
  const source = page.getByLabel('Paste BeerXML');
  await source.fill(lostXml(title));
  const guard = page.locator('#workspace-draft-warning');
  await page.getByRole('link', { name: 'Cancel' }).click();
  await expect(guard).toBeVisible();
  await expect(guard.getByRole('button', { name: 'Save and leave' })).toHaveCount(0);
  await guard.getByRole('button', { name: 'Stay' }).click();
  await expect(page).toHaveURL(/\/beerxml$/);
  await expect(source).toHaveValue(lostXml(title));
  await page.getByRole('link', { name: 'Recipes', exact: true }).first().click();
  await guard.getByRole('button', { name: 'Discard and leave' }).click();
  await expect(page).toHaveURL(/\/recipes$/);
  expect(db.exec(`SELECT COUNT(*) FROM recipes WHERE name = ${sqlText(title)}`)).toBe('0');
});

test('an untouched import page leaves without a guard', async ({ page }) => {
  await page.goto('/beerxml');
  await page.getByRole('link', { name: 'Cancel' }).click();
  await expect(page).toHaveURL(/\/recipes$/);
});

test('a lost import response is retained before sending and survives reload with the same request', async ({ page }) => {
  const title = `Reload import ${Date.now()}`;
  const bodies = await previewAndLoseImportResponse(page, title);
  expect(await retainedKeys(page)).toHaveLength(1);
  await page.reload();
  const recovery = page.locator('#beerxml-recovery');
  await expect(recovery).toContainText('may already have been created');
  await expect(recovery).toContainText('Pinned source');
  await expect(page.getByLabel('Paste BeerXML')).toHaveValue(lostXml(title));
  await expect(page.getByLabel('Paste BeerXML')).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Preview', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Import recipe', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Check previous result', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Recipe imported.' })).toBeVisible();
  expect(bodies).toHaveLength(2);
  expect(bodies[1]).toBe(bodies[0]);
  expect(JSON.parse(bodies[0]).request_id).toMatch(/^[0-9a-f-]{36}$/);
  expect(JSON.parse(bodies[1]).request_id).toBe(JSON.parse(bodies[0]).request_id);
  expect(db.exec(`SELECT COUNT(*) FROM recipes WHERE name = ${sqlText(title)}`)).toBe('1');
  expect(await retainedKeys(page)).toHaveLength(0);
  await expect(page.getByRole('link', { name: 'Open imported recipe' })).toBeFocused();
});

for (const scenario of [
  { name: 'expired', outcome: { ResultExpired: { resource_id: null } }, copy: /no longer holds this import/ },
  { name: 'identity conflict', outcome: 'RequestIdentityConflict', copy: /bound to different content/ },
] as const) {
  test(`a ${scenario.name} check keeps the retained request until it is acknowledged by name`, async ({ page }) => {
    const title = `Retained ${scenario.name} ${Date.now()}`;
    const bodies = await previewAndLoseImportResponse(page, title, scenario.outcome);
    await page.getByRole('button', { name: 'Check previous result', exact: true }).click();
    const recovery = page.locator('#beerxml-recovery');
    await expect(recovery).toContainText(scenario.copy);
    expect(bodies).toHaveLength(2);
    expect(JSON.parse(bodies[1]).request_id).toBe(JSON.parse(bodies[0]).request_id);
    await expect(page.getByRole('button', { name: 'Preview', exact: true })).toHaveCount(0);
    // The status survives reload; no fresh import is offered until acknowledged.
    await page.reload();
    await expect(page.locator('#beerxml-recovery')).toContainText(scenario.copy);
    await expect(page.getByRole('button', { name: 'Import recipe', exact: true })).toHaveCount(0);
    await page.getByRole('button', { name: 'Acknowledge and clear retained request', exact: true }).click();
    await expect(page.locator('#beerxml-recovery')).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Preview', exact: true })).toBeEnabled();
    expect(await retainedKeys(page)).toHaveLength(0);
    await expect(page.getByLabel('Paste BeerXML')).toBeEnabled();
  });
}

test('leaving after a lost response keeps the retained request and reopening checks the same one', async ({ page }) => {
  const title = `Dismiss import ${Date.now()}`;
  const bodies = await previewAndLoseImportResponse(page, title);
  await page.getByRole('link', { name: 'Back to recipes' }).click();
  const guard = page.locator('#workspace-draft-warning');
  await expect(guard).toBeVisible();
  await expect(guard).toContainText('result is unconfirmed');
  await expect(guard).toContainText('leaving does not cancel it');
  await expect(guard).not.toContainText(/not created|cancelled by|nothing was/i);
  await guard.getByRole('button', { name: 'Leave and keep the retained request' }).click();
  await expect(page).toHaveURL(/\/recipes$/);
  expect(await retainedKeys(page)).toHaveLength(1);
  await page.goto('/beerxml');
  await expect(page.locator('#beerxml-recovery')).toContainText('may already have been created');
  await page.getByRole('button', { name: 'Check previous result', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Recipe imported.' })).toBeVisible();
  expect(bodies).toHaveLength(2);
  expect(bodies[1]).toBe(bodies[0]);
  expect(JSON.parse(bodies[1]).request_id).toBe(JSON.parse(bodies[0]).request_id);
  expect(db.exec(`SELECT COUNT(*) FROM recipes WHERE name = ${sqlText(title)}`)).toBe('1');
});

test('a retained import is neither shown nor sent under a different account', async ({ page }) => {
  const email = `beerxml-retained-${Date.now()}@example.test`;
  const password = 'BeerXmlRetained1234';
  await createSecondUser(null, { email, name: 'Other brewer', password, workspaceId: getDefaultWorkspaceId() });
  walCheckpoint();
  const title = `Scoped import ${Date.now()}`;
  await previewAndLoseImportResponse(page, title);
  const other = await page.context().newPage();
  await loginUser(other, email, password);
  const leaked: string[] = [];
  other.on('request', request => { if (request.postData()?.includes(title)) leaked.push(request.url()); });
  await other.goto('/beerxml');
  await expect(other.getByLabel('Paste BeerXML')).toHaveValue('');
  await expect(other.locator('#beerxml-recovery')).toHaveCount(0);
  expect(leaked).toEqual([]);
  await other.close();
});

test('source and interpretation errors are associated with their own controls', async ({ page }) => {
  await page.goto('/beerxml');
  let sent = 0;
  await page.route('**/leptos-api/*_beerxml*', route => { sent++; return route.continue(); });
  const source = page.getByLabel('Paste BeerXML', { exact: true });
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await expect(source).toHaveAttribute('aria-invalid', 'true');
  await expect(source).toHaveAccessibleDescription(/Paste BeerXML or choose a file/);
  await source.fill(lostXml('Number check'));
  await page.getByLabel('Recipe number in file').fill('0');
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await expect(page.getByLabel('Recipe number in file')).toHaveAccessibleDescription(/starting at 1/);
  expect(sent).toBe(0);
  await page.getByLabel('Recipe number in file').fill('1');
  await source.fill(`<RECIPES><RECIPE><NAME>Choices</NAME><EFFICIENCY>75</EFFICIENCY><FERMENTABLES><FERMENTABLE><NAME>Malt</NAME><COLOR>3</COLOR></FERMENTABLE></FERMENTABLES></RECIPE></RECIPES>`);
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Import preview' })).toContainText('Choices');
  await expect(page.getByLabel('Source colour', { exact: true })).toHaveAccessibleDescription(/source colour/);
  await expect(page.getByLabel('Source efficiency', { exact: true })).toHaveAccessibleDescription(/source efficiency/);
});

test('the preview leads with parsed content and keeps hashes in a disclosure', async ({ page }) => {
  await page.goto('/beerxml');
  await page.getByLabel('Paste BeerXML').fill(lostXml('Ordered preview'));
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  const region = page.getByRole('region', { name: 'Import preview' });
  await expect(region).toContainText('Ordered preview');
  await expect(region.getByRole('heading', { name: 'Missing or unsupported information' })).toBeVisible();
  await expect(region.getByText('Original source SHA-256')).toBeHidden();
  const order = await region.evaluate(node => {
    const text = node.textContent ?? '';
    return [text.indexOf('Readiness'), text.indexOf('Missing or unsupported'), text.indexOf('Source estimates'), text.indexOf('SHA-256')];
  });
  expect([...order].sort((a, b) => a - b)).toEqual(order);
  expect(order.every(index => index >= 0)).toBe(true);
  await region.getByText('Source details').click();
  await expect(region.getByText('Original source SHA-256')).toBeVisible();
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
  await expect(page.getByText('A recipe may already have been created')).toBeVisible();
  await page.getByRole('button', { name: 'Check previous result' }).click();
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
  await page.locator('summary').filter({ hasText: /^Export and print$/ }).click();
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
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
  await page.locator('summary').filter({ hasText: /^Export and print$/ }).click();
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
