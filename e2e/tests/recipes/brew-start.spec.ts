import { test, expect } from '@playwright/test';
import {
  DEFAULT_TEST_USER,
  createEquipmentProfile,
  getDefaultUserId,
  loginUser,
  seedWorkspace,
  sqliteExec,
  walCheckpoint,
} from '../../helpers/test-helpers';

const VISUAL_BREW_ID = process.env.KRAUSEN_VISUAL_BREW_ID;


// The legacy `switch_workspace` server function updates only the database; the
// signed access token keeps its workspace claim until it is refreshed, so the
// guard's post-switch verification cannot observe it (the account feature owns
// that gap). Establish the new context the way editor.spec.ts does: change the
// stored workspace, re-sign the session, announce it to other tabs, reload.
async function switchTabContext(tab: import('@playwright/test').Page, workspaceId: string) {
  sqliteExec(
    `UPDATE users SET last_workspace_id = '${workspaceId}' WHERE user_id = '${getDefaultUserId()}'`,
  );
  walCheckpoint();
  const refreshed = await tab.evaluate(
    async () => (await fetch('/api/v1/auth/refresh', { method: 'POST' })).status,
  );
  expect(refreshed).toBe(200);
  await tab.evaluate(() => localStorage.setItem('krausen-context-changed', String(Date.now())));
  await tab.reload();
}

function commandFrom(request: import('@playwright/test').Request): any {
  const payload = new URLSearchParams(request.postData() ?? '').get('request_json');
  return payload ? JSON.parse(payload) : null;
}

async function callServerFn(
  page: import('@playwright/test').Page,
  path: string,
  payload: unknown,
): Promise<{ status: number; body: any }> {
  return page.evaluate(
    async ({ path, payload }) => {
      const response = await fetch(path, {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: new URLSearchParams({ request_json: JSON.stringify(payload) }).toString(),
      });
      const text = await response.text();
      let body: unknown = text;
      try {
        body = JSON.parse(text);
      } catch {}
      return { status: response.status, body };
    },
    { path, payload },
  );
}

function outcomeOf(body: any): any {
  return body?.Ok ?? body;
}

// Brews started from a recipe; accepts the recipe id or its editor URL.
async function brewCount(recipe: string): Promise<number> {
  // The database file is read from the host; flush the WAL so committed rows are visible.
  walCheckpoint();
  const recipeId = recipe.split('/').pop()!.split('?')[0];
  return Number(sqliteExec(`SELECT COUNT(*) FROM brews WHERE recipe_id = '${recipeId}'`));
}

// A real response envelope (`{Ok: …}` or bare) from a side-effect-free server call,
// reused so a controlled typed outcome has the shape the browser really receives.
function withEnvelope(real: any, outcome: unknown): unknown {
  return real && typeof real === 'object' && Object.prototype.hasOwnProperty.call(real, 'Ok') ? { Ok: outcome } : outcome;
}

// Saves a new head (the next revision) with a changed recipe name.
async function saveHeadThree(page: import('@playwright/test').Page, title: string) {
  await page.getByLabel('Recipe name', { exact: true }).fill(`${title} head three`);
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 3' })).toBeVisible();
}

// The editor learns of another tab's save through its focus refetch.
async function refreshHead(page: import('@playwright/test').Page) {
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByRole('status').filter({ hasText: 'Updated to the current saved revision.' })).toBeVisible();
}

test.beforeEach(async ({ page }) => {
  await loginUser(page, DEFAULT_TEST_USER.email, DEFAULT_TEST_USER.password);
});

async function importRecipe(page: import('@playwright/test').Page, title: string) {
  await page.goto('/beerxml');
  await page.getByLabel('Paste BeerXML').fill(
    `<RECIPES><RECIPE><NAME>${title}</NAME><BATCH_SIZE>20</BATCH_SIZE><BOIL_TIME>60</BOIL_TIME><FERMENTABLES><FERMENTABLE><NAME>Pilsner malt</NAME><AMOUNT>4.2</AMOUNT><TYPE>Grain</TYPE><YIELD>80</YIELD><COLOR>3</COLOR></FERMENTABLE></FERMENTABLES><HOPS><HOP><NAME>Hallertauer</NAME><AMOUNT>0.03</AMOUNT><USE>Boil</USE><TIME>60</TIME><ALPHA>5</ALPHA></HOP></HOPS></RECIPE></RECIPES>`,
  );
  await page.getByLabel('Source colour').selectOption('srm');
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Import preview' })).toBeVisible();
  await page.getByRole('button', { name: 'Import recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Recipe imported' })).toBeVisible();
  await page.getByRole('link', { name: 'Open imported recipe' }).click();
}

async function makeSavedRevisionReady(
  page: import('@playwright/test').Page,
  profileName = 'E2E kettle',
): Promise<string> {
  const recipeUrl = page.url();
  const profileId = await createEquipmentProfile(page, profileName, {
    'profile-mash-efficiency': '75',
    'profile-boil-off': '3',
    'profile-transfer-loss': '2',
    'profile-deadspace': '0',
    'profile-absorption': '0.8',
    'profile-thickness': '2.5',
    'profile-capacity': '30',
    'profile-diameter': '35',
    'profile-opening': '30',
    'profile-boil-temperature': '100',
  }, 'natural');
  await page.goto(recipeUrl);
  await page.getByText('Equipment settings', { exact: true }).click();
  await page.locator('#equipment-profile-select').selectOption(profileId);
  await page.getByText('Addition details', { exact: true }).first().click();
  await page.locator('#fermentables-fermentable-1-stage').selectOption('mash');
  await page.locator('#recipe-attenuation').fill('78');
  await page.locator('#recipe-mash-method').selectOption('manual');
  await page.locator('#recipe-mash-water').fill('15');
  await page.locator('#recipe-whirlpool').fill('15');
  await page.locator('#recipe-top-up').fill('0');
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
  return profileId;
}

// INT-005: Start opens a bounded review; preparation is automatic and the one
// final Start brew follows the source, equipment and targets.
async function reviewCurrentStart(page: import('@playwright/test').Page) {
  await page.getByRole('group', { name: 'Recipe commands' }).getByRole('button', { name: 'Start brew', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Start brew' });
  await expect(dialog.getByTestId('start-source')).toBeVisible();
  return dialog;
}

async function reviewHistoricalStart(page: import('@playwright/test').Page, revision: number) {
  await page.getByRole('tab', { name: 'Saved versions', exact: true }).click();
  await page.getByRole('button', { name: `Start brew from revision ${revision}` }).click();
  const dialog = page.getByRole('dialog', { name: 'Start brew' });
  await expect(dialog.getByTestId('start-source')).toContainText(`Revision ${revision}`);
  return dialog;
}

async function confirmStart(dialog: import('@playwright/test').Locator) {
  await dialog.getByRole('button', { name: 'Start brew', exact: true }).click();
  await expect(dialog.getByText('Brew started', { exact: true }).first()).toBeVisible();
}

async function startCurrent(page: import('@playwright/test').Page) {
  await confirmStart(await reviewCurrentStart(page));
}


async function startBrewAndOpenEvidence(
  page: import('@playwright/test').Page,
  title: string,
): Promise<string> {
  await importRecipe(page, title);
  await makeSavedRevisionReady(page);
  await startCurrent(page);
  await page.getByRole('link', { name: 'Open brew' }).click();
  await expect(page.getByRole('heading', { name: 'Brew evidence' })).toBeVisible();
  return page.url().split('/').pop() ?? '';
}

function lifecycleItems(page: import('@playwright/test').Page) {
  return page
    .locator('h3', { hasText: 'Lifecycle evidence' })
    .locator('xpath=following-sibling::ul[1]/li');
}

test('header promotes Start only for a clean saved Ready recipe and swaps back after an edit', async ({ page }) => {
  await importRecipe(page, `Header ready ${Date.now()}`);
  await makeSavedRevisionReady(page, `Header kettle ${Date.now()}`);
  const commands = page.getByRole('group', { name: 'Recipe commands' });
  const save = commands.getByRole('button', { name: /^Save (recipe|changes)$/ });
  const start = commands.getByRole('button', { name: 'Start brew', exact: true });
  await expect(start).toHaveClass(/bg-primary/);
  await expect(save).not.toHaveClass(/bg-primary/);
  await expect(start).not.toHaveAccessibleDescription(/Save|complete|estimate/);
  await page.getByLabel('Batch volume (L)').fill('21');
  await expect(save).toHaveClass(/bg-primary/);
  await expect(start).not.toHaveClass(/bg-primary/);
  await expect(start).toHaveAccessibleDescription('Save your changes before starting a brew.');
  await start.focus();
  await page.getByRole('button', { name: 'Undo' }).first().click();
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue('20');
  await expect(save).not.toHaveClass(/bg-primary/);
  await expect(start).toHaveClass(/bg-primary/);
});

test('starts the prepared saved revision with frozen targets', async ({ page }) => {
  const title = `Brew start ${Date.now()}`;
  await importRecipe(page, title);
  await makeSavedRevisionReady(page);
  await startCurrent(page);
  await expect(page.getByText(/Started from revision 2/)).toBeVisible();
});

test('editing a shared profile leaves a started brew frozen and changes only later previews', async ({ page }) => {
  await importRecipe(page, `Brew frozen ${Date.now()}`);
  const profileId = await makeSavedRevisionReady(page, `Frozen kettle ${Date.now()}`);
  const recipeUrl = page.url();
  await startCurrent(page);
  await page.getByRole('link', { name: 'Open brew' }).click();
  await expect(page.getByRole('heading', { name: 'Brew evidence' })).toBeVisible();
  const brewUrl = page.url();
  await expect(page.getByRole('heading', { name: 'Lifecycle evidence' })).toBeVisible();
  const before = await page.getByRole('main').innerText();
  await page.goto(`/equipment?profile=${profileId}`);
  await expect(page.locator('#profile-boil-off')).toHaveValue('3');
  await page.locator('#profile-boil-off').fill('6');
  await page.getByRole('button', { name: 'Save profile' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Profile saved.' })).toBeVisible();
  await page.goto(brewUrl);
  await expect(page.getByRole('heading', { name: 'Lifecycle evidence' })).toBeVisible();
  expect(await page.getByRole('main').innerText()).toBe(before);
  // A later start review resolves the edited current profile instead.
  await page.goto(recipeUrl);
  await page.getByText('Equipment settings', { exact: true }).click();
  await expect(page.getByLabel('Equipment profile', { exact: true })).toHaveValue(profileId);
  await expect(page.getByText(/Inherited: effective 6 L\/h from Frozen kettle/)).toBeVisible();
});

test('starts an explicitly selected historical revision without restoring it', async ({
  page,
}) => {
  const title = `Brew historical ${Date.now()}`;
  await importRecipe(page, title);
  await makeSavedRevisionReady(page);
  await saveHeadThree(page, title);
  const dialog = await reviewHistoricalStart(page, 2);
  await expect(dialog.getByTestId('start-equipment')).toContainText('E2E kettle');
  await confirmStart(dialog);
  await expect(dialog.getByText(/Started from revision 2/)).toBeVisible();
  await expect(dialog.getByRole('link', { name: 'Open brew' })).toBeFocused();
  // The head is untouched: nothing was restored or silently saved.
  await dialog.getByRole('button', { name: 'Done' }).click();
  await page.getByRole('tab', { name: 'Recipe', exact: true }).click();
  await expect(page.getByLabel('Recipe name', { exact: true })).toHaveValue(`${title} head three`);
});

test('a dirty draft offers explicit choices; Use saved revision keeps the draft and never saves it', async ({ page }) => {
  await importRecipe(page, `Brew dirty ${Date.now()}`);
  await makeSavedRevisionReady(page);
  const sends: string[] = [];
  const saves: string[] = [];
  await page.route('**/leptos-api/**', async (route) => {
    const url = route.request().url();
    if (/start_brew/.test(url)) sends.push(url);
    if (/save_recipe/.test(url)) saves.push(url);
    await route.continue();
  });
  await page.getByLabel('Batch volume (L)').fill('21');
  await page.getByRole('group', { name: 'Recipe commands' }).getByRole('button', { name: 'Start brew', exact: true }).click();
  const choose = page.getByRole('dialog', { name: 'Start brew' });
  await expect(choose.getByRole('heading', { name: 'Choose what to start' })).toBeVisible();
  await expect(choose.getByRole('button', { name: 'Save changes and continue' })).toBeVisible();
  await expect(choose.getByRole('button', { name: 'Cancel' })).toBeVisible();
  await choose.getByRole('button', { name: 'Use saved revision 2' }).click();
  const review = page.getByRole('dialog', { name: 'Start brew' });
  await expect(review.getByTestId('start-source')).toContainText('revision 2');
  await expect(review.getByText('Your unsaved changes are not part of this brew.')).toBeVisible();
  // Review precedes the single final command in DOM order.
  const order = await review.evaluate((dialog) => {
    const source = dialog.querySelector('[data-testid="start-source"]')!;
    const final = [...dialog.querySelectorAll('button')].find((b) => b.textContent === 'Start brew')!;
    return source.compareDocumentPosition(final) & Node.DOCUMENT_POSITION_FOLLOWING;
  });
  expect(order).toBeTruthy();
  await review.getByRole('button', { name: 'Cancel' }).click();
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue('21');
  expect(saves).toHaveLength(0);
  expect(sends).toHaveLength(0);
  // Save changes and continue: confirmed save, then a fresh read-only review.
  await page.getByRole('group', { name: 'Recipe commands' }).getByRole('button', { name: 'Start brew', exact: true }).click();
  await choose.getByRole('button', { name: 'Save changes and continue' }).click();
  await expect(review.getByTestId('start-source')).toContainText('revision 3');
  expect(saves.length).toBeGreaterThan(0);
  expect(sends).toHaveLength(0);
});

test('a lost start response keeps the original request through dismissal and reload and creates one brew', async ({ page }) => {
  await importRecipe(page, `Brew lost start ${Date.now()}`);
  await makeSavedRevisionReady(page);
  const recipeUrl = page.url();
  const sent: any[] = [];
  let first = true;
  await page.route('**/leptos-api/start_brew*', async (route) => {
    const command = commandFrom(route.request());
    sent.push(command);
    if (first) {
      first = false;
      const response = await route.fetch();
      expect(response.ok()).toBeTruthy();
      await route.abort('failed');
      return;
    }
    await route.continue();
  });
  const dialog = await reviewCurrentStart(page);
  await dialog.getByRole('button', { name: 'Start brew', exact: true }).click();
  await expect(dialog.getByText('Result unconfirmed')).toBeVisible();
  await expect(dialog).not.toContainText(/nothing was (started|created|sent)|no brew was (created|started)|was not (created|started)/i);
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  await page.reload();
  const recovered = page.getByRole('dialog', { name: 'Start brew' });
  await expect(recovered.getByRole('heading', { name: 'Brew start unconfirmed' })).toBeVisible();
  await expect(recovered.getByText('Result unconfirmed')).toBeVisible();
  await recovered.getByRole('button', { name: 'Check previous result' }).click();
  await expect(recovered.getByText('Brew started', { exact: true }).first()).toBeVisible();
  expect(sent).toHaveLength(2);
  expect(sent[1].request_id).toBe(sent[0].request_id);
  expect(sent[1]).toEqual(sent[0]);
  await expect.poll(() => brewCount(sent[0].recipe_id)).toBe(1);
  await page.goto(recipeUrl);
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('a second tab holding the start lock delays the start until it is released', async ({ page }) => {
  await importRecipe(page, `Brew locked ${Date.now()}`);
  await makeSavedRevisionReady(page);
  const recipeId = page.url().split('/').pop()!.split('?')[0];
  // The recipe's own workspace is the one the app scopes its lock and records to.
  const workspace = sqliteExec(`SELECT workspace_id FROM recipes WHERE recipe_id = '${recipeId}'`);
  const sent: any[] = [];
  await page.route('**/leptos-api/start_brew*', async (route) => { sent.push(commandFrom(route.request())); await route.continue(); });
  const dialog = await reviewCurrentStart(page);
  const holder = await page.context().newPage();
  await holder.goto('/recipes');
  await holder.evaluate(([account, ws, recipe]) => new Promise<void>((granted) => {
    navigator.locks.request(`krausen-start:${account}:${ws}:${recipe}`, () => new Promise<void>((release) => {
      (window as any).__releaseStartLock = release;
      granted();
    }));
  }), [getDefaultUserId(), workspace, recipeId]);
  const lockName = `krausen-start:${getDefaultUserId()}:${workspace}:${recipeId}`;
  await dialog.getByRole('button', { name: 'Start brew', exact: true }).click();
  // Serialised behind the other tab: the app's request is queued on the lock,
  // the review is still showing and nothing was dispatched.
  await expect
    .poll(() => holder.evaluate(async (name) => (await navigator.locks.query()).pending?.some((p) => p.name === name) ?? false, lockName))
    .toBe(true);
  await expect(dialog.getByRole('heading', { name: 'Review before starting' })).toBeVisible();
  await expect(dialog.getByRole('heading', { name: 'Starting brew' })).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: 'Start brew', exact: true })).toBeDisabled();
  expect(sent).toHaveLength(0);
  await holder.evaluate(() => (window as any).__releaseStartLock());
  await expect(dialog.getByText('Brew started', { exact: true }).first()).toBeVisible();
  expect(sent).toHaveLength(1);
  expect(await brewCount(recipeId)).toBe(1);
  await holder.waitForLoadState('networkidle');
  await holder.close();
});

test('without navigator.locks Start degrades to retain-then-verify and still starts once', async ({ page }) => {
  await page.addInitScript(() => { delete (Navigator.prototype as any).locks; });
  await importRecipe(page, `Brew no locks ${Date.now()}`);
  await makeSavedRevisionReady(page);
  const sent: any[] = [];
  await page.route('**/leptos-api/start_brew*', async (route) => { sent.push(commandFrom(route.request())); await route.continue(); });
  expect(await page.evaluate(() => 'locks' in navigator)).toBe(false);
  const dialog = await reviewCurrentStart(page);
  await confirmStart(dialog);
  expect(sent).toHaveLength(1);
  expect(await brewCount(sent[0].recipe_id)).toBe(1);
  // The record was retained before dispatch and released once the result was known.
  expect(await page.evaluate(() => Object.keys(localStorage).filter((k) => k.startsWith('krausen-start-v1:')))).toHaveLength(0);
});

test('a rejected locks.request degrades to retain-then-verify and still starts once', async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'locks', {
      configurable: true,
      value: {
        request: (name: string) => {
          ((window as any).__lockCalls ??= []).push(name);
          return Promise.reject(new DOMException('blocked', 'SecurityError'));
        },
      },
    });
  });
  await importRecipe(page, `Brew lock rejected ${Date.now()}`);
  await makeSavedRevisionReady(page);
  const sent: any[] = [];
  await page.route('**/leptos-api/start_brew*', async (route) => { sent.push(commandFrom(route.request())); await route.continue(); });
  const dialog = await reviewCurrentStart(page);
  await confirmStart(dialog);
  expect(sent).toHaveLength(1);
  expect(await brewCount(sent[0].recipe_id)).toBe(1);
  // The stub really was consulted (the page also asks for its session-refresh lock).
  const lockCalls: string[] = await page.evaluate(() => (window as any).__lockCalls ?? []);
  expect(lockCalls.filter((name) => name.startsWith('krausen-start:')).length).toBeGreaterThanOrEqual(1);
});

test('a never-granted lock times out, Start degrades, and the stale grant does not block later starts', async ({ page }) => {
  await page.clock.install();
  await importRecipe(page, `Brew lock timeout ${Date.now()}`);
  await makeSavedRevisionReady(page);
  const recipeId = page.url().split('/').pop()!.split('?')[0];
  // The recipe's own workspace is the one the app scopes its lock and records to.
  const workspace = sqliteExec(`SELECT workspace_id FROM recipes WHERE recipe_id = '${recipeId}'`);
  const lockName = `krausen-start:${getDefaultUserId()}:${workspace}:${recipeId}`;
  const sent: any[] = [];
  await page.route('**/leptos-api/start_brew*', async (route) => { sent.push(commandFrom(route.request())); await route.continue(); });
  const dialog = await reviewCurrentStart(page);
  const holder = await page.context().newPage();
  await holder.goto('/recipes');
  await holder.evaluate((name) => new Promise<void>((granted) => {
    navigator.locks.request(name, () => new Promise<void>((release) => {
      (window as any).__releaseStartLock = release;
      granted();
    }));
  }), lockName);
  await dialog.getByRole('button', { name: 'Start brew', exact: true }).click();
  await expect.poll(() => holder.evaluate(async (name) => (await navigator.locks.query()).pending?.some((p) => p.name === name) ?? false, lockName)).toBe(true);
  expect(sent).toHaveLength(0);
  await page.clock.fastForward(11_000);
  await expect(dialog.getByText('Brew started', { exact: true }).first()).toBeVisible();
  expect(sent).toHaveLength(1);
  // The abandoned request must not keep the lock once the holder lets go.
  await holder.evaluate(() => (window as any).__releaseStartLock());
  await expect.poll(() => holder.evaluate(async (name) => {
    const state = await navigator.locks.query();
    return [...(state.held ?? []), ...(state.pending ?? [])].some((l) => l.name === name);
  }, lockName)).toBe(false);
  await holder.waitForLoadState('networkidle');
  await holder.close();
});

test('a failed access confirmation before sending retains nothing and sends nothing', async ({ page }) => {
  await importRecipe(page, `Brew access failure ${Date.now()}`);
  await makeSavedRevisionReady(page);
  const sent: string[] = [];
  await page.route('**/leptos-api/start_brew*', async (route) => { sent.push(route.request().url()); await route.continue(); });
  const dialog = await reviewCurrentStart(page);
  await page.route('**/leptos-api/get_user_context*', (route) => route.abort('failed'));
  await dialog.getByRole('button', { name: 'Start brew', exact: true }).click();
  // The editor itself takes over with its own access notice; nothing is dispatched or retained.
  await expect(page.getByText('Network unavailable', { exact: true })).toBeVisible();
  expect(sent).toHaveLength(0);
  expect(await page.evaluate(() => Object.keys(localStorage).filter((k) => k.startsWith('krausen-start-v1:')))).toHaveLength(0);
});

test('an unreadable start record store blocks starting with the unable-to-read copy', async ({ page }) => {
  await importRecipe(page, `Brew unreadable store ${Date.now()}`);
  await makeSavedRevisionReady(page);
  const recipeId = page.url().split('/').pop()!.split('?')[0];
  // The recipe's own workspace is the one the app scopes its lock and records to.
  const workspace = sqliteExec(`SELECT workspace_id FROM recipes WHERE recipe_id = '${recipeId}'`);
  await page.evaluate(({ recipeId, workspace, account }) => {
    localStorage.setItem('krausen-start-v1:' + JSON.stringify([location.origin, account, workspace, recipeId, 'unreadable']), '{}');
  }, { recipeId, workspace, account: getDefaultUserId() });
  await page.addInitScript(() => {
    const original = Storage.prototype.getItem;
    Storage.prototype.getItem = function (key: string) {
      if (String(key).startsWith('krausen-start-v1:')) throw new Error('storage read failed');
      return original.call(this, key);
    };
  });
  await page.reload();
  const dialog = page.getByRole('dialog', { name: 'Start brew' });
  await expect(dialog.getByRole('heading', { name: 'Brew start unconfirmed' })).toBeVisible();
  await expect(dialog).toContainText('Unable to read start recovery');
  await expect(dialog.getByRole('button', { name: 'Start brew', exact: true })).toHaveCount(0);
});

test('browser Back across a document with an unconfirmed Start shows the native leave warning and Stay keeps the page', async ({ page }) => {
  await importRecipe(page, `Brew back start ${Date.now()}`);
  await makeSavedRevisionReady(page);
  await page.route('**/leptos-api/start_brew*', async (route) => { await route.fetch(); await route.abort('failed'); });
  const dialog = await reviewCurrentStart(page);
  await dialog.getByRole('button', { name: 'Start brew', exact: true }).click();
  await expect(dialog.getByRole('heading', { name: 'Brew start unconfirmed' })).toBeVisible();
  await page.keyboard.press('Escape');
  const editorUrl = page.url();
  const kinds: string[] = [];
  page.on('dialog', (native) => { kinds.push(native.type()); void native.dismiss(); });
  await page.evaluate(() => history.back());
  await expect.poll(() => kinds).toEqual(['beforeunload']);
  await expect(page).toHaveURL(editorUrl);
  // Staying keeps the retained request and its state.
  await page.getByRole('group', { name: 'Recipe commands' }).getByRole('button', { name: 'Start brew', exact: true }).click();
  await expect(page.getByRole('dialog', { name: 'Start brew' }).getByRole('heading', { name: 'Brew start unconfirmed' })).toBeVisible();
});

test('an Incomplete saved recipe shows why and offers no Start brew', async ({ page }) => {
  await importRecipe(page, `Brew incomplete ${Date.now()}`);
  const sends: string[] = [];
  await page.route('**/leptos-api/start_brew*', async (route) => { sends.push(route.request().url()); await route.continue(); });
  await page.getByRole('group', { name: 'Recipe commands' }).getByRole('button', { name: 'Start brew', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Start brew' });
  await expect(dialog.getByRole('heading', { name: 'Not ready to start' })).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Start brew', exact: true })).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: 'Check again' })).toBeVisible();
  expect(sends).toHaveLength(0);
});

test('header Start defaults to the inspected saved version and the visible review names equipment', async ({ page }) => {
  const title = `Brew inspected ${Date.now()}`;
  await importRecipe(page, title);
  await makeSavedRevisionReady(page);
  await saveHeadThree(page, title);
  await page.getByRole('tab', { name: 'Saved versions', exact: true }).click();
  await page.getByRole('button', { name: 'Inspect revision 2' }).click();
  await page.getByRole('group', { name: 'Recipe commands' }).getByRole('button', { name: 'Start brew', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Start brew' });
  await expect(dialog.getByTestId('start-source')).toContainText('Revision 2');
  await expect(dialog.getByTestId('start-equipment')).toContainText('E2E kettle');
  await expect(dialog.getByTestId('start-fermenter')).toContainText('L into the fermenter');
  await expect(dialog.getByTestId('start-overrides')).toBeVisible();
  await expect(dialog).not.toContainText(/\d\.\d{6,}/);
  await expect(dialog.getByRole('heading', { name: 'Review before starting' })).toBeFocused();
});

test('Escape while Starting keeps the task; the late result is shown, never a fresh review', async ({ page }) => {
  await importRecipe(page, `Brew late ${Date.now()}`);
  await makeSavedRevisionReady(page);
  let release: () => void = () => {};
  const held = new Promise<void>((resolve) => { release = resolve; });
  const sent: any[] = [];
  await page.route('**/leptos-api/start_brew*', async (route) => {
    sent.push(commandFrom(route.request()));
    await held;
    await route.continue();
  });
  const dialog = await reviewCurrentStart(page);
  await dialog.getByRole('button', { name: 'Start brew', exact: true }).click();
  await expect(dialog.getByRole('heading', { name: 'Starting brew' })).toBeVisible();
  await expect(dialog.getByTestId('start-pinned-source')).toContainText('Revision 2');
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  release();
  await expect.poll(() => sent.length).toBe(1);
  await page.getByRole('group', { name: 'Recipe commands' }).getByRole('button', { name: 'Start brew', exact: true }).click();
  await expect(dialog.getByRole('heading', { name: 'Brew started' })).toBeVisible();
  await expect(dialog.getByRole('link', { name: 'Open brew' })).toBeFocused();
  expect(sent).toHaveLength(1);
});

test('leaving while a start is unconfirmed keeps the retained request and says so', async ({ page }) => {
  await importRecipe(page, `Brew leave ${Date.now()}`);
  await makeSavedRevisionReady(page);
  await page.route('**/leptos-api/start_brew*', async (route) => { await route.fetch(); await route.abort('failed'); });
  const dialog = await reviewCurrentStart(page);
  await dialog.getByRole('button', { name: 'Start brew', exact: true }).click();
  await expect(dialog.getByRole('heading', { name: 'Brew start unconfirmed' })).toBeVisible();
  await page.keyboard.press('Escape');
  // An in-app exit is guarded by the app's own dialog; browser Back is covered by the next two tests.
  const editorUrl = page.url();
  await page.getByRole('link', { name: 'Equipment' }).first().click();
  const guard = page.locator('#workspace-draft-warning');
  await expect(guard).toBeVisible();
  await expect(guard).toContainText('does not cancel it');
  await expect(guard).not.toContainText(/nothing was (started|created|sent)|no brew was (created|started)/i);
  await expect(guard.getByRole('button', { name: 'Leave and keep the retained request' })).toBeVisible();
  await guard.getByRole('button', { name: 'Stay' }).click();
  await expect(guard).toBeHidden();
  await expect(page).toHaveURL(editorUrl);
});

test('an unreadable retained record blocks starting until it is explicitly discarded', async ({ page }) => {
  await importRecipe(page, `Brew corrupt ${Date.now()}`);
  await makeSavedRevisionReady(page);
  const recipeId = page.url().split('/').pop()!.split('?')[0];
  // The recipe's own workspace is the one the app scopes its lock and records to.
  const workspace = sqliteExec(`SELECT workspace_id FROM recipes WHERE recipe_id = '${recipeId}'`);
  await page.evaluate(({ recipeId, workspace, account }) => {
    const key = 'krausen-start-v1:' + JSON.stringify([location.origin, account, workspace, recipeId, 'corrupt']);
    localStorage.setItem(key, '{not json');
  }, { recipeId, workspace, account: getDefaultUserId() });
  await page.reload();
  const dialog = page.getByRole('dialog', { name: 'Start brew' });
  await expect(dialog.getByRole('heading', { name: 'Brew start unconfirmed' })).toBeVisible();
  await expect(dialog).toContainText('Discarding removes only records that cannot be read');
  await expect(dialog.getByRole('button', { name: 'Start brew', exact: true })).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: 'Check again' })).toHaveCount(0);
  await dialog.getByRole('button', { name: 'I checked Brews; discard this record' }).click();
  await page.keyboard.press('Escape');
  await page.getByRole('group', { name: 'Recipe commands' }).getByRole('button', { name: 'Start brew', exact: true }).click();
  await expect(dialog.getByTestId('start-source')).toBeVisible();
});


test('a head that moved after the review is reported, creates no brew, and needs a fresh explicit start', async ({ page }) => {
  const title = `Brew moved head ${Date.now()}`;
  await importRecipe(page, title);
  await makeSavedRevisionReady(page);
  const recipeUrl = page.url();
  const dialog = await reviewCurrentStart(page);
  const other = await page.context().newPage();
  await other.goto(recipeUrl);
  await saveHeadThree(other, title);
  // Let background requests finish; closing a tab cancels in-flight server calls.
  await other.waitForLoadState('networkidle');
  await other.close();
  await dialog.getByRole('button', { name: 'Start brew', exact: true }).click();
  await expect(dialog.getByRole('heading', { name: 'Review again before starting' })).toBeVisible();
  await expect(dialog).toContainText('No brew was started');
  await refreshHead(page);
  expect(await brewCount(recipeUrl)).toBe(0);
  await dialog.getByRole('button', { name: 'Review again' }).click();
  await expect(dialog.getByTestId('start-source')).toContainText('revision 3');
  expect(await brewCount(recipeUrl)).toBe(0);
  await confirmStart(dialog);
  expect(await brewCount(recipeUrl)).toBe(1);
});

test('after a moved head, a start from an older revision is refused until resolved and Review again follows it', async ({ page }) => {
  const title = `Brew moved then older ${Date.now()}`;
  await importRecipe(page, title);
  await makeSavedRevisionReady(page);
  const recipeUrl = page.url();
  const dialog = await reviewCurrentStart(page);
  const other = await page.context().newPage();
  await other.goto(recipeUrl);
  await saveHeadThree(other, title);
  // Let background requests finish; closing a tab cancels in-flight server calls.
  await other.waitForLoadState('networkidle');
  await other.close();
  await dialog.getByRole('button', { name: 'Start brew', exact: true }).click();
  await expect(dialog.getByRole('heading', { name: 'Review again before starting' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  await refreshHead(page);
  await page.getByRole('tab', { name: 'Saved versions', exact: true }).click();
  await page.getByRole('button', { name: 'Start brew from revision 2' }).click();
  await expect(dialog).toContainText('Starting revision 2 is unavailable until the previous result is resolved.');
  await dialog.getByRole('button', { name: 'Review again' }).click();
  await expect(dialog.getByTestId('start-source')).toContainText('Revision 2');
  expect(await brewCount(recipeUrl)).toBe(0);
});

test('a changed equipment context is reported with refreshed targets and renewed acceptance', async ({ page }) => {
  await importRecipe(page, `Brew moved context ${Date.now()}`);
  const profileId = await makeSavedRevisionReady(page, `Context kettle ${Date.now()}`);
  const recipeUrl = page.url();
  const dialog = await reviewCurrentStart(page);
  const other = await page.context().newPage();
  await other.goto(`/equipment?profile=${profileId}`);
  // Wait for the saved profile to load before editing, or the form can reset the fill.
  await expect(other.locator('#profile-boil-off')).toHaveValue('3');
  await other.locator('#profile-boil-off').fill('6');
  await other.getByRole('button', { name: 'Save profile' }).click();
  await expect(other.getByRole('status').filter({ hasText: 'Profile saved.' })).toBeVisible();
  // Let background requests finish; closing a tab cancels in-flight server calls.
  await other.waitForLoadState('networkidle');
  await other.close();
  await dialog.getByRole('button', { name: 'Start brew', exact: true }).click();
  await expect(dialog.getByRole('heading', { name: 'Review again before starting' })).toBeVisible();
  await expect(dialog.getByText('Refreshed targets', { exact: true })).toBeVisible();
  expect(await brewCount(recipeUrl)).toBe(0);
  await dialog.getByRole('button', { name: 'Review again' }).click();
  await expect(dialog.getByTestId('start-source')).toBeVisible();
  expect(await brewCount(recipeUrl)).toBe(0);
  await confirmStart(dialog);
  expect(await brewCount(recipeUrl)).toBe(1);
});

test('a failed Save and continue returns to the choices and sends no start', async ({ page }) => {
  await importRecipe(page, `Brew save failure ${Date.now()}`);
  await makeSavedRevisionReady(page);
  const starts: string[] = [];
  await page.route('**/leptos-api/start_brew*', async (route) => { starts.push(route.request().url()); await route.continue(); });
  await page.route('**/leptos-api/save_recipe*', (route) => route.abort('failed'));
  await page.getByLabel('Batch volume (L)').fill('21');
  await page.getByRole('group', { name: 'Recipe commands' }).getByRole('button', { name: 'Start brew', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Start brew' });
  await dialog.getByRole('button', { name: 'Save changes and continue' }).click();
  await expect(dialog.getByRole('heading', { name: 'Choose what to start' })).toBeVisible();
  await expect(dialog).toContainText('not saved completely');
  await expect(dialog.getByRole('button', { name: 'Save changes and continue' })).toBeEnabled();
  expect(starts).toHaveLength(0);
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await expect(page.getByLabel('Batch volume (L)')).toHaveValue('21');
});

test('an invalid number is reported by Save and continue, then fixed and continued', async ({ page }) => {
  await importRecipe(page, `Brew invalid then fixed ${Date.now()}`);
  await makeSavedRevisionReady(page);
  const start = page.getByRole('group', { name: 'Recipe commands' }).getByRole('button', { name: 'Start brew', exact: true });
  await page.getByLabel('Batch volume (L)').fill('abc');
  await start.click();
  const dialog = page.getByRole('dialog', { name: 'Start brew' });
  await dialog.getByRole('button', { name: 'Save changes and continue' }).click();
  await expect(dialog).toContainText(/invalid number fields/i);
  await expect(dialog.getByRole('heading', { name: 'Choose what to start' })).toBeVisible();
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await page.getByLabel('Batch volume (L)').fill('21');
  await start.click();
  await dialog.getByRole('button', { name: 'Save changes and continue' }).click();
  await expect(dialog.getByTestId('start-source')).toContainText('revision 3');
});

test('a new recipe is saved by Save recipe and continue before its review', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  await page.getByLabel('Recipe name', { exact: true }).fill(`Brew new ${Date.now()}`);
  await page.getByRole('group', { name: 'Recipe commands' }).getByRole('button', { name: 'Start brew', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Start brew' });
  await dialog.getByRole('button', { name: 'Save recipe and continue' }).click();
  await expect(page).toHaveURL(/\/recipe-editor\/[0-9a-f-]+/);
  await expect(dialog.getByRole('heading', { name: 'Not ready to start' })).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Start brew', exact: true })).toHaveCount(0);
});

test('a recipe override is listed in the review with a trimmed value', async ({ page }) => {
  await importRecipe(page, `Brew override ${Date.now()}`);
  await makeSavedRevisionReady(page);
  // makeSavedRevisionReady leaves this disclosure open; only open it if it is closed.
  const equipmentSettings = page.locator('#recipe-equipment-disclosure');
  if (!(await equipmentSettings.evaluate((el) => (el as HTMLDetailsElement).open))) {
    await equipmentSettings.locator('summary').click();
  }
  await page.locator('#recipe-boil-off').fill('4.5');
  await page.getByRole('button', { name: /^Save (recipe|changes)$/ }).click();
  await expect(page.getByRole('status').filter({ hasText: /Saved revision 3/ })).toBeVisible();
  const dialog = await reviewCurrentStart(page);
  await expect(dialog.getByTestId('start-overrides')).toContainText('1 recipe override');
  await expect(dialog.getByTestId('start-override-list')).toContainText('Boil-off');
  await expect(dialog.getByTestId('start-override-list')).toContainText('4.5 L/h · recipe override');
  await expect(dialog).not.toContainText(/\d\.\d{6,}/);
});

for (const [name, body, text] of [
  ['an expired response without a brew id', { ResultExpired: { resource_id: null } }, /brew may already exist/],
  ['a request identity conflict', 'RequestIdentityConflict', /brew may already exist/],
] as const) {
  test(`${name} pins the start as unresolved and offers no fresh start`, async ({ page }) => {
    await importRecipe(page, `Brew stuck ${Date.now()}`);
    await makeSavedRevisionReady(page);
    // A real, side-effect-free response (the review's own read) gives the envelope.
    const realResponse = page.waitForResponse((r) => r.url().includes('/leptos-api/') && r.request().method() === 'POST');
    const dialog = await reviewCurrentStart(page);
    const real = await (await realResponse).json().catch(() => null);
    await page.route('**/leptos-api/start_brew*', (route) =>
      route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(withEnvelope(real, body)) }));
    await dialog.getByRole('button', { name: 'Start brew', exact: true }).click();
    await expect(dialog.getByRole('heading', { name: 'Brew start unconfirmed' })).toBeVisible();
    await expect(dialog).toContainText(text);
    await expect(dialog).toContainText("Discarding removes this request's record only.");
    await expect(dialog.getByRole('button', { name: 'Start brew', exact: true })).toHaveCount(0);
    await expect(dialog.getByRole('button', { name: 'Check again' })).toHaveCount(0);
    await dialog.getByRole('button', { name: 'I checked Brews; discard this record' }).click();
    await expect(dialog.getByTestId('start-source')).toBeVisible();
  });
}

test('a new recipe must be saved before a brew can start', async ({ page }) => {
  await page.goto('/recipe-editor/new');
  const sends: string[] = [];
  await page.route('**/leptos-api/start_brew*', async (route) => { sends.push(route.request().url()); await route.continue(); });
  await page.getByRole('group', { name: 'Recipe commands' }).getByRole('button', { name: 'Start brew', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Start brew' });
  await expect(dialog.getByRole('heading', { name: 'Save this recipe to start a brew' })).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Save recipe and continue' })).toBeVisible();
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await expect(dialog).toBeHidden();
  expect(sends).toHaveLength(0);
});

test('held Record stays locked while a real focus overview completes', async ({
  page,
  context,
}) => {
  await startBrewAndOpenEvidence(page, `Brew delayed write ${Date.now()}`);
  let releaseRecord!: () => void;
  const recordReleased = new Promise<void>((resolve) => {
    releaseRecord = resolve;
  });
  let recordEntered!: () => void;
  const recordHeld = new Promise<void>((resolve) => {
    recordEntered = resolve;
  });
  const recordCommands: any[] = [];
  await page.route('**/leptos-api/brew_measurement*', async (route) => {
    const command = commandFrom(route.request());
    if (command?.action === 'record') {
      recordCommands.push(command);
      recordEntered();
      await recordReleased;
      await route.continue();
      return;
    }
    await route.continue();
  });
  await page.locator('#measurement-gravity-original-v1').fill('1.050');
  await page.getByRole('button', { name: 'Record reading' }).first().click();
  await recordHeld;
  const other = await context.newPage();
  await other.goto(page.url());
  await expect(other.getByRole('heading', { name: 'Brew evidence' })).toBeVisible();
  await other.getByRole('button', { name: 'Complete' }).click();
  await expect(other.getByText('Brew is now completed at revision 2.')).toBeVisible();
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(
    page
      .locator('.k-metric')
      .filter({ hasText: 'Status' })
      .getByText('Completed', { exact: true }),
  ).toBeVisible();
  await expect(page.getByText('Outcome uncertain')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Retry same request' })).toBeDisabled();
  await expect(
    page.getByRole('button', { name: 'Keep original request' }),
  ).toBeDisabled();
  expect(recordCommands).toHaveLength(1);
  releaseRecord();
  await expect(page.getByText('Measurement recorded.')).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText('Outcome uncertain')).toHaveCount(0);
  // Let background requests finish; closing a tab cancels in-flight server calls.
  await other.waitForLoadState('networkidle');
  await other.close();
});

test('dirty evidence Back is guarded and retains the draft', async ({ page }) => {
  const brewId = await startBrewAndOpenEvidence(page, `Brew dirty back ${Date.now()}`);
  await page.locator('#brew-note').fill('draft note');
  await page.evaluate(() => history.back());
  await expect(page.locator('#workspace-draft-warning')).toBeVisible();
  await page.getByRole('button', { name: 'Stay' }).click();
  await expect(page).toHaveURL(new RegExp(`/brews/${brewId}`));
  await expect(page.locator('#brew-note')).toHaveValue('draft note');
});

test('lost Record response is proven committed and retries the same identity', async ({
  page,
}) => {
  const brewId = await startBrewAndOpenEvidence(page, `Brew lost response ${Date.now()}`);
  const workspace = sqliteExec(
    `SELECT workspace_id FROM brews WHERE brew_id = '${brewId}'`,
  );
  const recordCommands: any[] = [];
  let firstRecord = true;
  await page.route('**/leptos-api/brew_measurement*', async (route) => {
    const command = commandFrom(route.request());
    if (command?.action !== 'record') {
      await route.continue();
      return;
    }
    recordCommands.push(command);
    if (firstRecord) {
      firstRecord = false;
      const response = await route.fetch();
      expect(response.ok()).toBeTruthy();
      await expect
        .poll(() =>
          sqliteExec(
            `SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = '${workspace}' AND brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`,
          ),
        )
        .toBe('1');
      await route.abort('failed');
      return;
    }
    await route.continue();
  });
  await page.locator('#measurement-gravity-original-v1').fill('1.050');
  await page.getByRole('button', { name: 'Record reading' }).first().click();
  await expect(page.getByText('Outcome uncertain')).toBeVisible();
  await expect.poll(() => recordCommands.length).toBe(1);
  await expect(page.getByRole('status').filter({ hasText: 'Outcome uncertain' })).toBeVisible();
  await page.getByRole('button', { name: 'Retry same request' }).click();
  await expect(page.getByText('Measurement recorded.')).toBeVisible();
  await expect.poll(() => recordCommands.length).toBe(2);
  await expect(page.getByRole('status').filter({ hasText: 'Measurement recorded.' })).toBeVisible();
  expect(recordCommands[1].request_id).toBe(recordCommands[0].request_id);
  expect(recordCommands[1]).toEqual(recordCommands[0]);
  expect(
    sqliteExec(
      `SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = '${workspace}' AND brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`,
    ),
  ).toBe('1');
});

test('delayed lifecycle continuation ignores a second click', async ({ page }) => {
  const brewId = await startBrewAndOpenEvidence(page, `Brew continuation ${Date.now()}`);
  const workspace = sqliteExec(
    `SELECT workspace_id FROM brews WHERE brew_id = '${brewId}'`,
  );
  const actor = sqliteExec(`SELECT started_by FROM brews WHERE brew_id = '${brewId}'`);
  for (let revision = 100; revision <= 150; revision += 1) {
    sqliteExec(
      `INSERT INTO brew_lifecycle_events (workspace_id, brew_id, brew_revision, from_status, to_status, reason, actor_id, recorded_at) VALUES ('${workspace}', '${brewId}', ${revision}, 'started', 'completed', 'synthetic', '${actor}', 1)`,
    );
  }
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Brew evidence' })).toBeVisible();
  await expect(lifecycleItems(page)).toHaveCount(50);
  let continuationRequests = 0;
  page.on('request', (request) => {
    if (request.url().includes('brew_evidence_view')) {
      continuationRequests += 1;
    }
  });
  await page.route('**/brew_evidence_view*', async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 1200));
    await route.continue();
  });
  const loadOlder = page.getByRole('button', { name: 'Load older lifecycle' });
  await loadOlder.click();
  await expect(loadOlder).toBeDisabled();
  await loadOlder.evaluate((element) => (element as HTMLButtonElement).click());
  await expect(lifecycleItems(page)).toHaveCount(51);
  expect(continuationRequests).toBe(1);
});

test('hidden uncertain request can be discarded after another tab changes context', async ({
  page,
  context,
}) => {
  const brewId = await startBrewAndOpenEvidence(page, `Brew context loss ${Date.now()}`);
  const workspace = sqliteExec(
    `SELECT workspace_id FROM brews WHERE brew_id = '${brewId}'`,
  );
  const recordCommands: any[] = [];
  let firstRecord = true;
  await page.route('**/leptos-api/brew_measurement*', async (route) => {
    const command = commandFrom(route.request());
    if (command?.action !== 'record') {
      await route.continue();
      return;
    }
    recordCommands.push(command);
    if (firstRecord) {
      firstRecord = false;
      await route.fetch();
      await expect
        .poll(() =>
          sqliteExec(
            `SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = '${workspace}' AND brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`,
          ),
        )
        .toBe('1');
      await route.abort('failed');
      return;
    }
    await route.continue();
  });
  await page.locator('#measurement-gravity-original-v1').fill('1.050');
  await page.getByRole('button', { name: 'Record reading' }).first().click();
  await expect(page.getByText('Outcome uncertain')).toBeVisible();
  await page.locator('#brew-note').fill('hidden draft');
  const otherName = `Evidence context ${Date.now()}`;
  const otherWorkspace = seedWorkspace({ ownerId: getDefaultUserId(), name: otherName });
  sqliteExec(
    `UPDATE users SET last_workspace_id = '${workspace}' WHERE user_id = '${getDefaultUserId()}'`,
  );
  walCheckpoint();
  const switcher = await context.newPage();
  await switcher.goto(page.url());
  await expect(switcher.getByRole('heading', { name: 'Brew evidence' })).toBeVisible();
  await switchTabContext(switcher, otherWorkspace);
  await expect(switcher.getByText('Brew not found in this workspace.')).toBeVisible();
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByText('Account or workspace changed')).toBeVisible();
  await expect(page.getByText('Outcome uncertain')).toHaveCount(0);
  await expect(page.locator('#brew-note')).toHaveCount(0);
  await page.getByRole('link', { name: 'Recipes' }).first().click();
  const guard = page.locator('#workspace-draft-warning');
  await expect(guard.getByRole('button', { name: 'Discard local inputs and leave' })).toBeEnabled();
  await guard.getByRole('button', { name: 'Discard local inputs and leave' }).click();
  await expect(page).toHaveURL(/\/recipes/);
  expect(
    sqliteExec(
      `SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = '${workspace}' AND brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`,
    ),
  ).toBe('1');
  await switcher.close();
});

test('conflict refreshes current evidence and keeps the draft for reapply', async ({
  page,
  context,
}) => {
  await startBrewAndOpenEvidence(page, `Brew conflict ${Date.now()}`);
  await page.locator('#measurement-gravity-original-v1').fill('1.050');
  await page.getByRole('button', { name: 'Record reading' }).first().click();
  await expect(page.getByText('Measurement recorded.')).toBeVisible();
  const other = await context.newPage();
  await other.goto(page.url());
  await expect(other.getByRole('heading', { name: 'Brew evidence' })).toBeVisible();
  await other.locator('#measurement-gravity-original-v1').fill('1.052');
  await other.getByRole('button', { name: 'Correct selected' }).first().click();
  await expect(other.getByText('Measurement corrected.')).toBeVisible();
  await page.locator('#measurement-gravity-original-v1').fill('1.053');
  await page.getByRole('button', { name: 'Correct selected' }).first().click();
  await expect(
    page.getByText('Changed elsewhere; current evidence refreshed. Review and reapply.'),
  ).toBeVisible();
  await expect(page.locator('#measurement-gravity-original-v1')).toHaveValue('1.053');
  await expect(page.getByText(/selected actual 1\.052 SG/)).toBeVisible();
  await page.getByRole('button', { name: 'Correct selected' }).first().click();
  await expect(page.getByText('Measurement corrected.')).toBeVisible();
  // Let background requests finish; closing a tab cancels in-flight server calls.
  await other.waitForLoadState('networkidle');
  await other.close();
});

test('retained uncertain request recovers after a real workspace round trip', async ({
  page,
  context,
}) => {
  const brewId = await startBrewAndOpenEvidence(page, `Brew context recovery ${Date.now()}`);
  const originalWorkspace = sqliteExec(
    `SELECT workspace_id FROM brews WHERE brew_id = '${brewId}'`,
  );
  const originalName = sqliteExec(
    `SELECT name FROM workspaces WHERE workspace_id = '${originalWorkspace}'`,
  );
  const recordCommands: any[] = [];
  let firstRecord = true;
  await page.route('**/leptos-api/brew_measurement*', async (route) => {
    const command = commandFrom(route.request());
    if (command?.action !== 'record') {
      await route.continue();
      return;
    }
    recordCommands.push(command);
    if (firstRecord) {
      firstRecord = false;
      await route.fetch();
      await expect
        .poll(() =>
          sqliteExec(
            `SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = '${originalWorkspace}' AND brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`,
          ),
        )
        .toBe('1');
      await route.abort('failed');
      return;
    }
    await route.continue();
  });
  await page.locator('#measurement-gravity-original-v1').fill('1.050');
  await page.getByRole('button', { name: 'Record reading' }).first().click();
  await expect(page.getByText('Outcome uncertain')).toBeVisible();
  const otherName = `Evidence recovery ${Date.now()}`;
  const otherWorkspace = seedWorkspace({ ownerId: getDefaultUserId(), name: otherName });
  sqliteExec(
    `UPDATE users SET last_workspace_id = '${originalWorkspace}' WHERE user_id = '${getDefaultUserId()}'`,
  );
  walCheckpoint();
  const switcher = await context.newPage();
  await switcher.goto(page.url());
  await expect(switcher.getByRole('heading', { name: 'Brew evidence' })).toBeVisible();
  await switchTabContext(switcher, otherWorkspace);
  await expect(switcher.getByText('Brew not found in this workspace.')).toBeVisible();
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByText('Account or workspace changed')).toBeVisible();
  await expect(page.getByText('Outcome uncertain')).toHaveCount(0);
  await expect(page.locator('#measurement-gravity-original-v1')).toHaveCount(0);
  await switchTabContext(switcher, originalWorkspace);
  await expect(switcher.getByRole('heading', { name: 'Brew evidence' })).toBeVisible();
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByText('Outcome uncertain')).toBeVisible();
  await page.getByRole('button', { name: 'Retry same request' }).click();
  await expect(page.getByText('Measurement recorded.')).toBeVisible();
  await expect.poll(() => recordCommands.length).toBe(2);
  expect(recordCommands[1].request_id).toBe(recordCommands[0].request_id);
  expect(
    sqliteExec(
      `SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = '${originalWorkspace}' AND brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`,
    ),
  ).toBe('1');
  await switcher.close();
});

test('failed retry context preflight retains the same uncertain request', async ({
  page,
}) => {
  const brewId = await startBrewAndOpenEvidence(page, `Brew retry preflight ${Date.now()}`);
  const workspace = sqliteExec(
    `SELECT workspace_id FROM brews WHERE brew_id = '${brewId}'`,
  );
  const recordCommands: any[] = [];
  let firstRecord = true;
  await page.route('**/leptos-api/brew_measurement*', async (route) => {
    const command = commandFrom(route.request());
    if (command?.action !== 'record') {
      await route.continue();
      return;
    }
    recordCommands.push(command);
    if (firstRecord) {
      firstRecord = false;
      await route.fetch();
      await expect
        .poll(() =>
          sqliteExec(
            `SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = '${workspace}' AND brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`,
          ),
        )
        .toBe('1');
      await route.abort('failed');
      return;
    }
    await route.continue();
  });
  await page.locator('#measurement-gravity-original-v1').fill('1.050');
  await page.getByRole('button', { name: 'Record reading' }).first().click();
  await expect(page.getByText('Outcome uncertain')).toBeVisible();
  const otherName = `Retry preflight ${Date.now()}`;
  const otherWorkspace = seedWorkspace({ ownerId: getDefaultUserId(), name: otherName });
  const setWorkspace = (workspaceId: string) => {
    sqliteExec(
      `UPDATE users SET last_workspace_id = '${workspaceId}' WHERE user_id = '${getDefaultUserId()}'`,
    );
    walCheckpoint();
  };
  setWorkspace(workspace);
  let releasePreflight!: () => void;
  const preflightReleased = new Promise<void>((resolve) => {
    releasePreflight = resolve;
  });
  let preflightEntered!: () => void;
  const preflightHeld = new Promise<void>((resolve) => {
    preflightEntered = resolve;
  });
  let retryPreflight = false;
  let firstPreflight = true;
  let preflightRequests = 0;
  await page.route('**/leptos-api/get_user_context*', async (route) => {
    if (!retryPreflight || !firstPreflight) {
      await route.continue();
      return;
    }
    firstPreflight = false;
    preflightRequests += 1;
    preflightEntered();
    await preflightReleased;
    const cookies = await page.context().cookies();
    const cookieHeader = cookies
      .map((cookie) => `${cookie.name}=${cookie.value}`)
      .join('; ');
    const response = await route.fetch({
      headers: { ...route.request().headers(), cookie: cookieHeader },
    });
    const body = await response.json();
    expect(JSON.stringify(body)).toContain(otherWorkspace);
    await route.fulfill({ response });
  });
  // The uncertain notice appears when the first write starts; wait until that
  // write has fully settled so its own context check cannot consume the held
  // preflight meant for the retry.
  await expect(page.getByRole('button', { name: 'Retry same request' })).toBeEnabled();
  retryPreflight = true;
  await page.getByRole('button', { name: 'Retry same request' }).click();
  await preflightHeld;
  expect(preflightRequests).toBe(1);
  setWorkspace(otherWorkspace);
  const refreshed = await page.evaluate(
    async () => (await fetch('/api/v1/auth/refresh', { method: 'POST' })).status,
  );
  expect(refreshed).toBe(200);
  releasePreflight();
  await expect(page.getByText('Account or workspace changed')).toBeVisible();
  await expect(page.getByText('Outcome uncertain')).toHaveCount(0);
  expect(recordCommands).toHaveLength(1);
  await page.getByRole('link', { name: 'Recipes' }).first().click();
  const guard = page.locator('#workspace-draft-warning');
  await expect(guard.getByRole('button', { name: 'Discard local inputs and leave' })).toBeEnabled();
  await guard.getByRole('button', { name: 'Stay' }).click();
  await expect(page.getByText('Account or workspace changed')).toBeVisible();
  expect(recordCommands).toHaveLength(1);
  setWorkspace(workspace);
  const restored = await page.evaluate(
    async () => (await fetch('/api/v1/auth/refresh', { method: 'POST' })).status,
  );
  expect(restored).toBe(200);
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByText('Outcome uncertain')).toBeVisible();
  await page.getByRole('button', { name: 'Retry same request' }).click();
  await expect(page.getByText('Measurement recorded.')).toBeVisible();
  await expect.poll(() => recordCommands.length).toBe(2);
  expect(recordCommands[1].request_id).toBe(recordCommands[0].request_id);
  expect(
    sqliteExec(
      `SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = '${workspace}' AND brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`,
    ),
  ).toBe('1');
});

test('out-of-order context success cannot restore hidden evidence', async ({
  page,
  context,
}) => {
  const brewId = await startBrewAndOpenEvidence(page, `Brew out of order ${Date.now()}`);
  const originalWorkspace = sqliteExec(
    `SELECT workspace_id FROM brews WHERE brew_id = '${brewId}'`,
  );
  const otherName = `Out of order ${Date.now()}`;
  const otherWorkspace = seedWorkspace({ ownerId: getDefaultUserId(), name: otherName });
  sqliteExec(
    `UPDATE users SET last_workspace_id = '${originalWorkspace}' WHERE user_id = '${getDefaultUserId()}'`,
  );
  walCheckpoint();
  const switcher = await context.newPage();
  await switcher.goto(page.url());
  await expect(switcher.getByRole('heading', { name: 'Brew evidence' })).toBeVisible();
  await page.waitForTimeout(500);
  await expect(page.getByRole('heading', { name: 'Brew evidence' })).toBeVisible();
  await expect(page.locator('#measurement-gravity-original-v1')).toBeVisible();
  let releaseOlder!: () => void;
  const olderReleased = new Promise<void>((resolve) => {
    releaseOlder = resolve;
  });
  let olderEntered!: () => void;
  const olderHeld = new Promise<void>((resolve) => {
    olderEntered = resolve;
  });
  let releaseNewer!: () => void;
  const newerReleased = new Promise<void>((resolve) => {
    releaseNewer = resolve;
  });
  let newerEntered!: () => void;
  const newerHeld = new Promise<void>((resolve) => {
    newerEntered = resolve;
  });
  let olderFulfilled!: () => void;
  const olderDone = new Promise<void>((resolve) => {
    olderFulfilled = resolve;
  });
  let newerFulfilled!: () => void;
  const newerDone = new Promise<void>((resolve) => {
    newerFulfilled = resolve;
  });
  let contextChecks = 0;
  await page.route('**/leptos-api/get_user_context*', async (route) => {
    contextChecks += 1;
    if (contextChecks === 1) {
      const response = await route.fetch();
      const body = await response.json();
      expect(JSON.stringify(body)).toContain(originalWorkspace);
      olderEntered();
      await olderReleased;
      await route.fulfill({ response });
      olderFulfilled();
      return;
    }
    if (contextChecks === 2) {
      newerEntered();
      await newerReleased;
      const response = await route.fetch();
      const body = await response.json();
      expect(JSON.stringify(body)).toContain(otherWorkspace);
      await route.fulfill({ response });
      newerFulfilled();
      return;
    }
    await route.continue();
  });
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await olderHeld;
  await switchTabContext(switcher, otherWorkspace);
  await expect(switcher.getByText('Brew not found in this workspace.')).toBeVisible();
  await newerHeld;
  await expect
    .poll(() =>
      sqliteExec(
        `SELECT last_workspace_id FROM users WHERE user_id = '${getDefaultUserId()}'`,
      ),
    )
    .toBe(otherWorkspace);
  releaseNewer();
  await newerDone;
  await expect(page.getByText('Account or workspace changed')).toBeVisible();
  await page.evaluate(() => {
    (window as any).__oldEvidenceSeen = false;
    const observer = new MutationObserver(() => {
      if (document.querySelector('#measurement-gravity-original-v1')) {
        (window as any).__oldEvidenceSeen = true;
      }
    });
    observer.observe(document.body, { childList: true, subtree: true });
    (window as any).__oldEvidenceObserver = observer;
  });
  releaseOlder();
  await olderDone;
  await page.evaluate(
    () =>
      new Promise<void>((resolve) => {
        let frames = 0;
        const tick = () => {
          frames += 1;
          if (frames >= 5) {
            resolve();
          } else {
            requestAnimationFrame(tick);
          }
        };
        requestAnimationFrame(tick);
      }),
  );
  const oldEvidenceSeen = await page.evaluate(() => {
    const observer = (window as any).__oldEvidenceObserver as MutationObserver | undefined;
    const records = observer?.takeRecords() ?? [];
    const seenInRecords = records.some((record) =>
      Array.from(record.addedNodes).some(
        (node) =>
          node instanceof Element &&
          (node.matches('#measurement-gravity-original-v1') ||
            node.querySelector('#measurement-gravity-original-v1') !== null),
      ),
    );
    observer?.disconnect();
    return Boolean((window as any).__oldEvidenceSeen) || seenInRecords;
  });
  expect(oldEvidenceSeen).toBe(false);
  await expect(page.getByText('Account or workspace changed')).toBeVisible();
  await expect(page.getByText('Outcome uncertain')).toHaveCount(0);
  await expect(page.getByText('Brew evidence')).toHaveCount(0);
  await switcher.close();
});

test('held overview response after same-document unmount is ignored', async ({ page }) => {
  await startBrewAndOpenEvidence(page, `Brew unmount ${Date.now()}`);
  const pageErrors: string[] = [];
  page.on('pageerror', (error) => pageErrors.push(error.message));
  let releaseOverview!: () => void;
  const held = new Promise<void>((resolve) => {
    releaseOverview = resolve;
  });
  let overviewEntered!: () => void;
  const entered = new Promise<void>((resolve) => {
    overviewEntered = resolve;
  });
  await page.route('**/leptos-api/brew_evidence_view*', async (route) => {
    overviewEntered();
    await held;
    await route.continue();
  });
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await entered;
  const responsePromise = page.waitForResponse(
    (response) => response.url().includes('/brew_evidence_view') && response.ok(),
  );
  await page.getByRole('link', { name: 'Recipes' }).first().click();
  await expect(page).toHaveURL(/\/recipes/);
  releaseOverview();
  await responsePromise;
  await page.waitForTimeout(300);
  expect(pageErrors).toEqual([]);
});

test('populated historical comparison is human-readable at desktop/mobile light/dark', async ({
  page,
}) => {
  test.skip(!VISUAL_BREW_ID, 'requires KRAUSEN_VISUAL_BREW_ID');
  const forbidden = [
    'Some(',
    'BrewComparisonValue',
    'measurement_reference_temperature_unknown',
    'no_planned_target_for_basis',
    'canonical_digest',
  ];
  for (const viewport of [
    { width: 1440, height: 1000 },
    { width: 390, height: 844 },
  ]) {
    await page.setViewportSize(viewport);
    for (const colorScheme of ['light', 'dark'] as const) {
      await page.emulateMedia({ colorScheme });
      await page.goto(`/brews/${VISUAL_BREW_ID}/analysis`);
      await expect(page.getByRole('heading', { name: 'Plan versus actual' })).toBeVisible();
      await expect(page.getByRole('columnheader', { name: 'Planned target' })).toBeVisible();
      await expect(page.getByRole('columnheader', { name: 'Selected actual' })).toBeVisible();
      if (await page.getByRole('columnheader', { name: 'Change' }).count()) {
        await expect(
          page.getByRole('cell', { name: /Changed|Added|Removed/ }).first(),
        ).toBeVisible();
      }
      for (const text of forbidden) {
        await expect(page.getByText(text, { exact: false })).toHaveCount(0);
      }
      expect(
        await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
      ).toBe(true);
      if (viewport.width === 390) {
        const region = page.getByRole('region', {
          name: 'Planned target versus selected actual table',
        });
        await expect(region).toHaveAttribute('tabindex', '0');
        await region.focus();
        await expect(region).toBeFocused();
        const sizes = await region.evaluate((element) => ({
          client: element.clientWidth,
          scroll: element.scrollWidth,
        }));
        expect(sizes.scroll).toBeGreaterThan(sizes.client);
        await page.keyboard.press('ArrowRight');
        // Native arrow scrolling is asynchronous in Chromium; require the
        // eventual scroll without accepting a non-scrolling region.
        await expect
          .poll(() => region.evaluate((element) => element.scrollLeft), { timeout: 2000 })
          .toBeGreaterThan(0);
        await region.evaluate((element) => {
          element.scrollLeft = element.scrollWidth;
        });
        const headerBox = await page
          .getByRole('columnheader', { name: 'Comparison' })
          .boundingBox();
        const regionBox = await region.boundingBox();
        expect(headerBox).not.toBeNull();
        expect(regionBox).not.toBeNull();
        expect(headerBox!.x + headerBox!.width).toBeLessThanOrEqual(
          regionBox!.x + regionBox!.width + 1,
        );
      }
      const print = page.getByRole('button', { name: 'Print frozen plan' });
      await print.focus();
      await expect(print).toBeFocused();
      await page.keyboard.press('Enter');
      await expect(page.getByText('Historical print preview')).toBeVisible();
      await expect(
        page.getByRole('heading', { name: 'Recorded actuals' }),
      ).toBeVisible();
    }
  }
});

test('recipe brew history and frozen historical print are reachable', async ({ page }) => {
  const title = `Brew history ${Date.now()}`;
  const brewId = await startBrewAndOpenEvidence(page, title);
  const recipeId = sqliteExec(
    `SELECT recipe_id FROM brews WHERE brew_id = '${brewId.replaceAll("'", "''")}';`,
  );
  await page.goto(`/recipes/${recipeId}/brews`);
  await expect(page.getByRole('heading', { name: 'Brew history' })).toBeVisible();
  await expect(page.getByText(/recipe revision 2/i)).toBeVisible();
  await page.getByRole('link', { name: 'Compare and feedback' }).click();
  await expect(page.getByRole('heading', { name: 'Plan versus actual' })).toBeVisible();
  await expect(page.getByText('Target versus selected actual')).toBeVisible();
  await page.getByRole('button', { name: 'Print frozen plan' }).click();
  await expect(page.getByText('Historical print preview')).toBeVisible();
  await expect(page.getByText(/Historical brew .* frozen recipe revision 2/)).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Recorded actuals' })).toBeVisible();
  // KRA-43: the frozen preview gets the same real boundary; its content is unchanged.
  const frozen = page.getByRole('dialog', { name: 'Historical print preview' });
  await expect(frozen.getByRole('heading', { name: 'Historical print preview' })).toBeFocused();
  await expect(frozen.getByRole('heading', { name: 'Recorded actuals' })).toBeVisible();
  await expect(page.locator('body')).not.toHaveClass(/krausen-printing/);
  await page.keyboard.press('Escape');
  await expect(frozen).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Print frozen plan' })).toBeFocused();
});

test('brew pages carry the authorized Recipes → recipe → Brews → brew trail and sibling links', async ({ page }) => {
  const title = `Brew trail ${Date.now()}`;
  const brewId = await startBrewAndOpenEvidence(page, title);
  const recipeId = sqliteExec(
    `SELECT recipe_id FROM brews WHERE brew_id = '${brewId.replaceAll("'", "''")}';`,
  );
  const trail = page.getByRole('navigation', { name: 'Breadcrumb' });

  await expect(page).toHaveURL(new RegExp(`/brews/${brewId}$`));
  await expect(trail.getByRole('link', { name: 'Recipes', exact: true })).toHaveAttribute('href', '/recipes');
  await expect(trail.getByRole('link', { name: title })).toHaveAttribute('href', `/recipe-editor/${recipeId}`);
  await expect(trail.getByRole('link', { name: 'Brews', exact: true })).toHaveAttribute('href', `/recipes/${recipeId}/brews`);
  await expect(trail.getByText('Measurements', { exact: true })).toHaveAttribute('aria-current', 'page');
  await expect(page.getByRole('heading', { name: 'Brew measurements', level: 1 })).toBeVisible();

  // Sibling links reach Compare, whose trail ends there and links back.
  const siblings = page.getByRole('navigation', { name: 'This brew' });
  await expect(siblings.getByRole('link', { name: 'Measurements' })).toHaveAttribute('aria-current', 'page');
  await siblings.getByRole('link', { name: 'Compare' }).click();
  await expect(page).toHaveURL(new RegExp(`/brews/${brewId}/analysis$`));
  await expect(page.getByRole('heading', { name: 'Plan versus actual', level: 1 })).toBeFocused();
  await expect(trail.getByText('Compare', { exact: true })).toHaveAttribute('aria-current', 'page');
  await expect(trail.getByRole('link', { name: title })).toBeVisible();
  await page.getByRole('navigation', { name: 'This brew' }).getByRole('link', { name: 'Measurements' }).click();
  await expect(page).toHaveURL(new RegExp(`/brews/${brewId}$`));

  // The recipe-local Brews page ends the trail at Brews, with the recipe linked.
  await trail.getByRole('link', { name: 'Brews', exact: true }).click();
  await expect(page).toHaveURL(new RegExp(`/recipes/${recipeId}/brews$`));
  await expect(trail.getByText('Brews', { exact: true })).toHaveAttribute('aria-current', 'page');
  await trail.getByRole('link', { name: title }).click();
  await expect(page).toHaveURL(new RegExp(`/recipe-editor/${recipeId}$`));
});

test('historical presentation uses human labels and hides machine identifiers', async ({
  page,
}) => {
  const serverFnPaths = new Map<string, string>();
  page.on('request', (request) => {
    const path = new URL(request.url()).pathname;
    const match = path.match(/^\/leptos-api\/([a-z_]+)\d+$/);
    if (match) {
      serverFnPaths.set(match[1], path);
    }
  });

  const title = `Brew presentation ${Date.now()}`;
  const brewId = await startBrewAndOpenEvidence(page, title);
  const recipeId = sqliteExec(
    `SELECT recipe_id FROM brews WHERE brew_id = '${brewId.replaceAll("'", "''")}';`,
  );
  const workspaceId = sqliteExec(
    `SELECT workspace_id FROM brews WHERE brew_id = '${brewId.replaceAll("'", "''")}';`,
  );
  const startedBy = sqliteExec(
    `SELECT started_by FROM brews WHERE brew_id = '${brewId.replaceAll("'", "''")}';`,
  );
  const intent = { account_id: getDefaultUserId(), workspace_id: workspaceId };
  const evidencePath = serverFnPaths.get('brew_evidence_view');
  expect(evidencePath).toBeTruthy();

  const comparisonResponse = page.waitForResponse((response) =>
    response.url().includes('/leptos-api/execute_brew_comparison'),
  );
  await page.goto(`/brews/${brewId}/analysis`);
  const comparison = await (await comparisonResponse).json();
  const plan = outcomeOf(comparison).Comparison;
  const contextId = plan.plan.context_id;
  const document = structuredClone(plan.plan.recipe);
  document.boil_minutes = 70;
  document.notes = 'Browser presentation actual';
  const actualPath = evidencePath!.replace('brew_evidence_view', 'brew_actual');
  const amended = await callServerFn(page, actualPath, {
    action: 'amend',
    intent,
    brew_id: brewId,
    expected_actual_revision: null,
    document,
    reason: 'presentation proof',
    request_id: crypto.randomUUID(),
  });
  expect(amended.status, JSON.stringify(amended.body)).toBe(200);
  await page.reload();

  await expect(page.getByRole('heading', { name: 'Plan versus actual' })).toBeVisible();
  await expect(page.getByRole('columnheader', { name: 'Planned target' })).toBeVisible();
  await expect(page.getByRole('columnheader', { name: 'Selected actual' })).toBeVisible();
  await expect(page.getByRole('columnheader', { name: 'Planned', exact: true })).toBeVisible();
  await expect(page.getByRole('row', { name: /Boil duration/ })).toBeVisible();
  await expect(page.getByRole('row', { name: /Brew notes/ })).toBeVisible();
  await expect(page.getByText('No reading recorded').first()).toBeVisible();
  for (const forbidden of [
    'BrewComparisonValue',
    'canonical_digest',
    'process:boil_minutes',
    'Some(',
    'measurement_reference_temperature_unknown',
  ]) {
    await expect(page.getByText(forbidden, { exact: false })).toHaveCount(0);
  }
  await expect(page.getByText(contextId)).not.toBeVisible();
  await expect(page.getByText(startedBy)).toHaveCount(0);
  await expect(page.getByText('Recorded actual').first()).toBeVisible();
  await expect(page.getByText('Revision 1').first()).toBeVisible();

  await page.getByRole('button', { name: 'Print frozen plan' }).click();
  await expect(page.getByText('Historical print preview')).toBeVisible();
  await expect(
    page.getByText('Historical brew · frozen recipe revision 2', { exact: false }),
  ).toBeVisible();
  await expect(page.locator('.print-preview').getByText(brewId)).not.toBeVisible();

  await page.goto(`/recipes/${recipeId}/brews`);
  await expect(page.getByRole('heading', { name: 'Brew history' })).toBeVisible();
  await expect(page.getByRole('columnheader', { name: 'Plan' })).toBeVisible();
  await expect(page.getByText(startedBy)).toHaveCount(0);
  const historyTable = page.getByRole('region', { name: 'Brew history table' });
  await expect(
    historyTable.getByText(/^by (you|another workspace member)$/).first(),
  ).toBeVisible();
  await expect(
    page.getByText(/\d{2} [A-Z][a-z]{2} \d{4}, \d{2}:\d{2} UTC/).first(),
  ).toBeVisible();

  await page.goto(`/brews/${brewId}`);
  await expect(page.getByRole('heading', { name: 'Brew evidence' })).toBeVisible();
  await expect(page.getByText(startedBy)).toHaveCount(0);
  await expect(page.getByText('Planned target').first()).toBeVisible();
  await expect(page.getByText('Recorded actual').first()).toBeVisible();
  await expect(
    page.getByText(/Started .* by (you|another workspace member)/),
  ).toBeVisible();
});


test('held context check after navigation cannot install the previous brew', async ({
  page,
}) => {
  const firstBrew = await startBrewAndOpenEvidence(page, `Brew held A ${Date.now()}`);
  const secondBrew = await startBrewAndOpenEvidence(page, `Brew held B ${Date.now()}`);
  const workspace = sqliteExec(
    `SELECT workspace_id FROM brews WHERE brew_id = '${firstBrew.replaceAll("'", "''")}';`,
  );
  const intent = { account_id: getDefaultUserId(), workspace_id: workspace };

  const initialComparison = page.waitForResponse((response) =>
    response.url().includes('/leptos-api/execute_brew_comparison'),
  );
  await page.goto(`/brews/${firstBrew}/analysis`);
  await expect(page.getByRole('heading', { name: 'Plan versus actual' })).toBeVisible();
  const comparisonPath = new URL((await initialComparison).request().url()).pathname;
  const firstComparison = outcomeOf(
    (
      await callServerFn(page, comparisonPath!, {
        intent,
        brew_id: firstBrew,
      })
    ).body,
  ).Comparison;
  const secondComparison = outcomeOf(
    (
      await callServerFn(page, comparisonPath!, {
        intent,
        brew_id: secondBrew,
      })
    ).body,
  ).Comparison;
  expect(firstComparison.plan.context_id).not.toBe(secondComparison.plan.context_id);

  let holdNextContext = false;
  let releaseContext!: () => void;
  const contextReleased = new Promise<void>((resolve) => {
    releaseContext = resolve;
  });
  let contextEntered!: () => void;
  const contextHeld = new Promise<void>((resolve) => {
    contextEntered = resolve;
  });
  let contextDelivered!: () => void;
  const contextDeliveredLatch = new Promise<void>((resolve) => {
    contextDelivered = resolve;
  });
  await page.route('**/leptos-api/get_user_context*', async (route) => {
    if (!holdNextContext) {
      await route.continue();
      return;
    }
    holdNextContext = false;
    // Resolve the real context while the view still owns the first brew, but
    // deliver it only after the route has moved to the second brew.
    const response = await route.fetch();
    const body = await response.json();
    expect(body.user_id).toBe(intent.account_id);
    expect(body.workspace_id).toBe(intent.workspace_id);
    contextEntered();
    await contextReleased;
    await route.fulfill({ response });
    contextDelivered();
  });

  let releaseSecondComparison!: () => void;
  const secondComparisonReleased = new Promise<void>((resolve) => {
    releaseSecondComparison = resolve;
  });
  let secondComparisonEntered!: () => void;
  const secondComparisonHeld = new Promise<void>((resolve) => {
    secondComparisonEntered = resolve;
  });
  await page.route('**/leptos-api/execute_brew_comparison*', async (route) => {
    const request = commandFrom(route.request());
    if (request?.brew_id !== firstBrew) {
      // Hold the destination brew's read so the stale continuation's write, if
      // any, is observable before the correct data arrives.
      const response = await route.fetch();
      secondComparisonEntered();
      await secondComparisonReleased;
      await route.fulfill({ response });
      return;
    }
    const response = await route.fetch();
    holdNextContext = true;
    await route.fulfill({ response });
  });

  await page.getByRole('button', { name: 'Refresh' }).click();
  await contextHeld;
  await page.evaluate((href) => {
    history.pushState({}, '', href);
    window.dispatchEvent(new PopStateEvent('popstate'));
  }, `/brews/${secondBrew}/analysis`);
  await expect(page).toHaveURL(new RegExp(secondBrew));
  await secondComparisonHeld;
  releaseContext();
  // Wait for the matched workspace-A context response to complete and for the
  // owning wasm task to get a frame, while the destination comparison is still
  // held, instead of guessing with a fixed sleep.
  await contextDeliveredLatch;
  await page.evaluate(
    () =>
      new Promise<void>((resolve) => {
        requestAnimationFrame(() => setTimeout(resolve, 0));
      }),
  );
  await expect(page.getByText('Frozen plan provenance')).toHaveCount(0);
  releaseSecondComparison();
  await expect(page.getByText('Frozen plan provenance')).toBeVisible();
  await page.getByText('Frozen plan provenance').click();
  await expect(page.getByText(secondComparison.plan.context_id)).toBeVisible();
  await expect(page.getByText(firstComparison.plan.context_id)).toHaveCount(0);
});


test('feedback apply keeps one request through a lost response and ignores selection changes', async ({
  page,
}) => {
  // Leptos appends a per-module hash to server-fn paths. Capture the evidence
  // module hash from the page's own request and reuse it for brew_actual.
  const serverFnPaths = new Map<string, string>();
  page.on('request', (request) => {
    const path = new URL(request.url()).pathname;
    const match = path.match(/^\/leptos-api\/([a-z_]+)\d+$/);
    if (match) {
      serverFnPaths.set(match[1], path);
    }
  });
  const title = `Brew feedback ${Date.now()}`;
  const brewId = await startBrewAndOpenEvidence(page, title);
  const recipeId = sqliteExec(
    `SELECT recipe_id FROM brews WHERE brew_id = '${brewId.replaceAll("'", "''")}';`,
  );
  const workspaceId = sqliteExec(
    `SELECT workspace_id FROM brews WHERE brew_id = '${brewId.replaceAll("'", "''")}';`,
  );
  const intent = { account_id: getDefaultUserId(), workspace_id: workspaceId };
  const evidencePath = serverFnPaths.get('brew_evidence_view');
  expect(evidencePath).toBeTruthy();

  const comparisonResponse = page.waitForResponse((response) =>
    response.url().includes('/leptos-api/execute_brew_comparison'),
  );
  await page.goto(`/brews/${brewId}/analysis`);
  const comparison = await (await comparisonResponse).json();
  const plan = outcomeOf(comparison).Comparison;
  const document = structuredClone(plan.plan.recipe);
  document.boil_minutes = 70;
  document.notes = 'Browser proof actual';
  const actualPath = evidencePath!.replace('brew_evidence_view', 'brew_actual');
  const amended = await callServerFn(page, actualPath, {
    action: 'amend',
    intent,
    brew_id: brewId,
    expected_actual_revision: null,
    document,
    reason: 'browser proof',
    request_id: crypto.randomUUID(),
  });
  expect(amended.status, JSON.stringify(amended.body)).toBe(200);
  expect(outcomeOf(amended.body).Amended, JSON.stringify(amended.body)).toBeTruthy();
  expect(outcomeOf(amended.body).Amended.revision).toBe(1);

  await page.reload();
  await expect(page.getByRole('heading', { name: 'Plan versus actual' })).toBeVisible();
  await expect(page.getByText('Recorded actual').first()).toBeVisible();
  await expect(page.getByText('Revision 1').first()).toBeVisible();
  await page
    .getByRole('row', { name: /Boil duration/ })
    .getByRole('checkbox')
    .click();
  await page.getByRole('button', { name: 'Preview selected feedback' }).click();
  await expect(page.getByText(/Based on recipe revision 2/)).toBeVisible();

  const applyRequests: any[] = [];
  let allowApply = false;
  await page.route('**/leptos-api/execute_brew_feedback_apply*', async (route) => {
    const request = commandFrom(route.request());
    if (request) {
      applyRequests.push(request);
    }
    if (!allowApply && request) {
      // Deliver the real request so the server commits the typed result, then
      // drop the response so the browser sees an uncertain outcome.
      const response = await route.fetch();
      const body = await response.json();
      const applied = outcomeOf(body).Applied;
      expect(applied.recipe_revision).toBe(3);
      expect(
        sqliteExec(
          `SELECT COUNT(*) FROM app_requests WHERE request_id = '${request.request_id.replaceAll("'", "''")}' AND workspace_id = '${workspaceId.replaceAll("'", "''")}' AND operation = 'brew.feedback.apply.v1';`,
        ),
      ).toBe('1');
      expect(
        sqliteExec(
          `SELECT MAX(revision) FROM recipe_revisions WHERE recipe_id = '${recipeId.replaceAll("'", "''")}';`,
        ),
      ).toBe('3');
      await route.abort();
      return;
    }
    await route.continue();
  });
  await page.getByRole('button', { name: 'Apply as new recipe revision' }).click();
  await expect(page.getByText(/The response was lost/)).toBeVisible();
  await expect(page.getByRole('button', { name: 'Retry previous request' })).toBeVisible();
  expect(applyRequests).toHaveLength(1);

  // Changing another selection while the request is uncertain must not cancel
  // or replace the retained command.
  await page
    .getByRole('row', { name: /Brew notes/ })
    .getByRole('checkbox')
    .click();
  await expect(page.getByRole('button', { name: 'Retry previous request' })).toBeVisible();
  expect(applyRequests).toHaveLength(1);

  allowApply = true;
  await page.getByRole('button', { name: 'Retry previous request' }).click();
  await expect(page.getByText(/Feedback applied as recipe revision 3/)).toBeVisible();
  expect(applyRequests).toHaveLength(2);
  expect(applyRequests[1]).toEqual(applyRequests[0]);
  expect(applyRequests[1].request_id).toBe(applyRequests[0].request_id);
  expect(applyRequests[1].expected_recipe_head).toBe(2);
  expect(applyRequests[1].expected_actual_revision).toBe(1);
  expect(applyRequests[1].change_ids).toEqual(['process:boil_minutes']);
  expect(applyRequests[1].expected_calculation_context).not.toBe('');
  expect(applyRequests[1].intent).toEqual(intent);
  expect(applyRequests[1].brew_id).toBe(brewId);
  expect(
    sqliteExec(
      `SELECT COUNT(*) FROM app_requests WHERE request_id = '${applyRequests[0].request_id.replaceAll("'", "''")}' AND operation = 'brew.feedback.apply.v1';`,
    ),
  ).toBe('1');
  expect(
    sqliteExec(
      `SELECT MAX(revision) FROM recipe_revisions WHERE recipe_id = '${recipeId.replaceAll("'", "''")}';`,
    ),
  ).toBe('3');

  const after = await callServerFn(page, serverFnPaths.get('execute_brew_comparison')!, {
    intent,
    brew_id: brewId,
  });
  expect(outcomeOf(after.body).Comparison.plan.recipe.boil_minutes).toBe(60);

  // The applied feedback advances the recipe head, but the brew keeps its
  // frozen revision and historical print at the revision it was started from.
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(`/recipes/${recipeId}/brews`);
  await expect(page.getByRole('heading', { name: 'Brew history' })).toBeVisible();
  await expect(page.getByText(/recipe revision 2/i).first()).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
    true,
  );

  const compare = page.getByRole('link', { name: 'Compare and feedback' });
  await compare.focus();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('heading', { name: 'Plan versus actual' })).toBeVisible();
  await expect(page.getByText(/recipe revision 2/i).first()).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
    true,
  );

  const print = page.getByRole('button', { name: 'Print frozen plan' });
  await print.focus();
  await page.keyboard.press('Enter');
  await expect(page.getByText('Historical print preview')).toBeVisible();
  await expect(page.getByText(/Historical brew .* frozen recipe revision 2/)).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Recorded actuals' })).toBeVisible();
  const printable = page.getByRole('article', {
    name: 'Printable recipe and process summary',
  });
  await expect(printable).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
    true,
  );
  await page.emulateMedia({ media: 'print' });
  await expect(printable).toBeVisible();
  await expect(printable).toContainText(title);
  await expect(printable).toContainText('60 min boil');
  await expect(printable).toContainText(/Historical brew .* frozen recipe revision 2/);
  await expect(printable).not.toContainText('70 min boil');
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
  ).toBe(true);
});

test('evidence leave returns to explicit actions instead of offering an impossible batch save', async ({ page }) => {
  const brewId = await startBrewAndOpenEvidence(page, `Evidence explicit leave ${Date.now()}`);
  const writes: any[] = [];
  page.on('request', request => {
    if (!/\/leptos-api\/brew_(measurement|note)/.test(request.url())) return;
    const command = commandFrom(request);
    if (command && !['list', 'history'].includes(command.action)) writes.push(command);
  });
  const note = page.locator('#brew-note');
  await note.fill('Keep this unsent note for its explicit Record action');
  await page.getByRole('link', { name: 'Recipes', exact: true }).first().click();
  const guard = page.locator('#workspace-draft-warning');
  await expect(guard).toBeVisible();
  await expect(guard.getByRole('button', { name: 'Return to save evidence', exact: true })).toBeVisible();
  await expect(guard.getByRole('button', { name: 'Save and leave', exact: true })).toHaveCount(0);
  await guard.getByRole('button', { name: 'Return to save evidence', exact: true }).click();
  await expect(guard).not.toBeVisible();
  await expect(page).toHaveURL(new RegExp(`/brews/${brewId}$`));
  await expect(note).toHaveValue('Keep this unsent note for its explicit Record action');
  await expect(note).toBeFocused();
  expect(writes).toHaveLength(0);
});

for (const focusCase of ['newer successor', 'no-op target'] as const) {
  test(`evidence Return respects ${focusCase} before its owned focus frame`, async ({ page }) => {
    await startBrewAndOpenEvidence(page, `Evidence focus arbitration ${focusCase} ${Date.now()}`);
    const writes: any[] = [];
    page.on('request', request => {
      if (/\/leptos-api\/brew_(measurement|note|lifecycle|archive)/.test(request.url())) {
        const command = commandFrom(request);
        if (command && !['list', 'history'].includes(command.action)) writes.push(command);
      }
    });
    await page.locator('#brew-note').fill('Preserve this raw note and its native editing state');
    await page.getByRole('link', { name: 'Recipes', exact: true }).first().click();
    const guard = page.locator('#workspace-draft-warning');
    await expect(guard).toBeVisible();
    await page.evaluate(focusCase => {
      if (focusCase === 'no-op target') {
        (document.querySelector('#brew-note') as HTMLElement).focus = () => {};
        return;
      }
      const raf = window.requestAnimationFrame.bind(window);
      window.requestAnimationFrame = callback => raf(time => {
        if (!document.querySelector('#workspace-draft-warning[open]')) {
          window.requestAnimationFrame = raf;
          const successor = document.querySelector<HTMLButtonElement>('[aria-controls="account-popover"]');
          successor!.focus(); // newer real focus, no pointer/key epoch change
          (window as any).__evidenceSuccessorFocused = true;
        }
        callback(time);
      });
    }, focusCase);
    await guard.getByRole('button', { name: 'Return to save evidence', exact: true }).click();
    await expect(guard).not.toBeVisible();
    if (focusCase === 'newer successor') {
      await expect.poll(() => page.evaluate(() => (window as any).__evidenceSuccessorFocused === true)).toBe(true);
      await expect(page.getByRole('button', { name: /^Account menu/ })).toBeFocused();
    } else await expect(page.locator('#evidence-return-anchor')).toBeFocused();
    await expect(page.locator('#brew-note')).toHaveValue('Preserve this raw note and its native editing state');
    expect(writes).toHaveLength(0);
  });
}

for (const action of [
  { name: 'measurement', selector: '#measurement-gravity-original-v1', value: '1.050' },
  { name: 'reason', selector: '#brew-transition-reason', value: 'Keep this explicit lifecycle reason' },
]) {
  test(`evidence Return restores the authorized explicit ${action.name} field without writing`, async ({ page }) => {
    const brewId = await startBrewAndOpenEvidence(page, `Evidence Return ${action.name} ${Date.now()}`);
    const writes: any[] = [];
    page.on('request', request => {
      if (!/\/leptos-api\/brew_(measurement|note|lifecycle|archive)/.test(request.url())) return;
      const command = commandFrom(request);
      if (command && !['list', 'history'].includes(command.action)) writes.push(command);
    });
    const field = page.locator(action.selector);
    await field.fill(action.value);
    await page.getByRole('link', { name: 'Recipes', exact: true }).first().click();
    const guard = page.locator('#workspace-draft-warning');
    await expect(guard).toBeVisible();
    await expect(guard.getByRole('button', { name: 'Save and leave', exact: true })).toHaveCount(0);
    await guard.getByRole('button', { name: 'Return to save evidence', exact: true }).click();
    await expect(guard).not.toBeVisible();
    await expect(page).toHaveURL(new RegExp(`/brews/${brewId}$`));
    await expect(field).toHaveValue(action.value);
    await expect(field).toBeFocused();
    expect(writes).toHaveLength(0);
  });
}

test('evidence recovery storage failure refuses dispatch and preserves unsent inputs', async ({ page }) => {
  await startBrewAndOpenEvidence(page, `Evidence storage failure ${Date.now()}`);
  let writes = 0;
  page.on('request', request => {
    if (/\/leptos-api\/brew_measurement/.test(request.url()) && commandFrom(request)?.action === 'record') writes++;
  });
  await page.evaluate(() => {
    const setItem = Storage.prototype.setItem;
    Storage.prototype.setItem = function (key, value) {
      if (key.startsWith('krausen-evidence-v1:')) throw new DOMException('Controlled storage failure', 'QuotaExceededError');
      return setItem.call(this, key, value);
    };
  });
  const reading = page.locator('#measurement-gravity-original-v1');
  await reading.fill('1.050');
  await page.getByRole('button', { name: 'Record reading', exact: true }).first().click();
  await expect(page.getByText(/^Request not sent:/)).toBeVisible();
  await expect(reading).toHaveValue('1.050');
  await expect(page.getByRole('button', { name: 'Record reading', exact: true }).first()).toBeEnabled();
  expect(writes).toBe(0);
  await page.getByRole('link', { name: 'Recipes', exact: true }).first().click();
  const guard = page.locator('#workspace-draft-warning');
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Return to save evidence', exact: true }).click();
  await expect(reading).toBeFocused();
  await expect(reading).toHaveValue('1.050');
  expect(writes).toBe(0);
});

for (const blocker of ['expired', 'identity conflict'] as const) {
  test(`controlled ${blocker} retains the original evidence envelope across reload and explicit review`, async ({ page }) => {
    const brewId = await startBrewAndOpenEvidence(page, `Evidence terminal ${blocker} ${Date.now()}`);
    const commands: any[] = [];
    let resourceId = '';
    await page.route('**/leptos-api/brew_measurement*', async route => {
      const command = commandFrom(route.request());
      if (command?.action !== 'record') return route.continue();
      commands.push(command);
      const response = await route.fetch();
      expect(response.ok()).toBeTruthy();
      const body = await response.json();
      const recorded = outcomeOf(body).Recorded;
      expect(recorded.brew_id).toBe(brewId);
      resourceId = recorded.entry_id;
      expect(resourceId).not.toBe('');
      // Exercise the production typed-outcome UI after a real native write.
      // This is controlled transport coverage, not a claim that the unchanged
      // backend ledger actually expired or conflicted during this test.
      const outcome = blocker === 'expired' ? { ResultExpired: { resource_id: resourceId } } : 'RequestIdentityConflict';
      const replacement = withEnvelope(body, outcome);
      await route.fulfill({ response, body: JSON.stringify(replacement) });
    });
    await page.locator('#measurement-gravity-original-v1').fill('1.050');
    await page.getByRole('button', { name: 'Record reading', exact: true }).first().click();
    await expect(page.getByText('Original request requires review', { exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Retry same request', exact: true })).toBeDisabled();
    expect(commands).toHaveLength(1);
    page.once('dialog', dialog => dialog.accept());
    await page.reload();
    await expect(page.getByText('Original request requires review', { exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Retry same request', exact: true })).toBeDisabled();
    if (blocker === 'expired') await expect(page.getByText(new RegExp(resourceId)).first()).toBeVisible();
    await page.getByRole('button', { name: 'Verify current evidence', exact: true }).click();
    const finish = page.getByRole('button', { name: 'Keep original identity and finish review', exact: true });
    await expect(finish).toBeEnabled();
    await finish.click();
    await expect(page.getByText('Original identity retained after review', { exact: true })).toBeVisible();
    const records = await page.evaluate(() => Object.keys(localStorage).filter(key => key.startsWith('krausen-evidence-v1:')).map(key => JSON.parse(localStorage.getItem(key)!)));
    expect(records).toHaveLength(1);
    expect(records[0].command.Measurement).toEqual(commands[0]);
    expect(records[0].receipt.Measurement).toEqual(blocker === 'expired' ? { ResultExpired: { resource_id: resourceId } } : 'RequestIdentityConflict');
    expect(commands).toHaveLength(1);
    expect(sqliteExec(`SELECT COUNT(*) FROM brew_measurement_entries WHERE brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`)).toBe('1');
  });
}

test('keyless held evidence preflight cannot replace or clear another discovered original', async ({ page, context }) => {
  const brewId = await startBrewAndOpenEvidence(page, `Keyless evidence ownership ${Date.now()}`);
  const other = await context.newPage();
  await other.goto(`/brews/${brewId}`);
  await expect(other.getByRole('button', { name: 'Record reading', exact: true }).first()).toBeEnabled();
  await page.bringToFront();
  await expect(page.getByRole('button', { name: 'Record reading', exact: true }).first()).toBeEnabled();
  let release!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  let reached!: () => void;
  const preflightHeld = new Promise<void>(resolve => { reached = resolve; });
  let firstContext = true;
  const originalB: any[] = [];
  const sentFromA: any[] = [];
  await page.route('**/leptos-api/get_user_context*', async route => {
    if (!firstContext) return route.continue();
    firstContext = false;
    const response = await route.fetch();
    expect(response.ok()).toBeTruthy();
    reached();
    await held;
    await route.fulfill({ response });
  });
  page.on('request', request => {
    if (/\/leptos-api\/brew_measurement/.test(request.url()) && commandFrom(request)?.action === 'record') sentFromA.push(commandFrom(request));
  });
  await other.route('**/leptos-api/brew_measurement*', async route => {
    const command = commandFrom(route.request());
    if (command?.action !== 'record') return route.continue();
    originalB.push(command);
    const response = await route.fetch();
    expect(response.ok()).toBeTruthy();
    await route.abort('failed');
  });
  try {
    await page.locator('#measurement-gravity-original-v1').fill('1.050');
    await page.getByRole('button', { name: 'Record reading', exact: true }).first().click();
    await preflightHeld;
    expect(sentFromA).toHaveLength(0);
    await other.locator('#measurement-gravity-original-v1').fill('1.052');
    await other.getByRole('button', { name: 'Record reading', exact: true }).first().click();
    await expect(other.getByText(/^Reading request uncertain:/)).toBeVisible();
    expect(originalB).toHaveLength(1);
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    const choiceB = page.getByRole('button', { name: `Check retained evidence request ${originalB[0].request_id}`, exact: true });
    await expect(choiceB).toBeVisible();
    await expect(choiceB).toBeDisabled(); // A still owns preflight, before retain
    release();
    await expect(choiceB).toBeEnabled(); // the old A continuation has settled
    await expect(page.getByRole('button', { name: 'Record reading', exact: true }).first()).toBeDisabled();
    expect(sentFromA).toHaveLength(0);
    await choiceB.click();
    await page.getByRole('button', { name: 'Retry same request', exact: true }).click();
    await expect(page.getByText('Measurement recorded.', { exact: true })).toBeVisible();
    expect(sentFromA).toHaveLength(1);
    expect(sentFromA[0]).toEqual(originalB[0]);
    expect(sqliteExec(`SELECT COUNT(*) FROM brew_measurement_entries WHERE brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`)).toBe('1');
  } finally { release(); await other.close(); }
});

test('mixed confirmed cleanup and unknown evidence retain the original retry lock', async ({ page, context }) => {
  const brewId = await startBrewAndOpenEvidence(page, `Mixed evidence recovery ${Date.now()}`);
  const workspace = sqliteExec(`SELECT workspace_id FROM brews WHERE brew_id = '${brewId}'`);
  // Both real owners finish their empty inventory before either write begins.
  const other = await context.newPage();
  await other.goto(`/brews/${brewId}`);
  await expect(other.getByRole('button', { name: 'Record reading', exact: true }).first()).toBeEnabled();
  let releaseA!: () => void;
  const heldA = new Promise<void>(resolve => { releaseA = resolve; });
  let committedA!: () => void;
  const aCommitted = new Promise<void>(resolve => { committedA = resolve; });
  const commandsA: any[] = [];
  const commandsB: any[] = [];
  await page.evaluate(() => {
    const removeItem = Storage.prototype.removeItem;
    Storage.prototype.removeItem = function (key) {
      if (key.startsWith('krausen-evidence-v1:')) throw new DOMException('Controlled cleanup fault', 'SecurityError');
      return removeItem.call(this, key);
    };
  });
  await page.route('**/leptos-api/brew_measurement*', async route => {
    const command = commandFrom(route.request());
    if (command?.action !== 'record') return route.continue();
    commandsA.push(command);
    const response = await route.fetch();
    expect(response.ok()).toBeTruthy();
    committedA();
    await heldA;
    await route.fulfill({ response });
  });
  await other.route('**/leptos-api/brew_measurement*', async route => {
    const command = commandFrom(route.request());
    if (command?.action !== 'record') return route.continue();
    commandsB.push(command);
    if (commandsB.length === 1) {
      const response = await route.fetch();
      expect(response.ok()).toBeTruthy();
      await route.abort('failed');
    } else await route.continue();
  });
  await page.locator('#measurement-gravity-original-v1').fill('1.050');
  await page.getByRole('button', { name: 'Record reading', exact: true }).first().click();
  await aCommitted;
  await other.locator('#measurement-gravity-original-v1').fill('1.052');
  await other.getByRole('button', { name: 'Record reading', exact: true }).first().click();
  await expect(other.getByText(/^Reading request uncertain:/)).toBeVisible();
  const inventoryOwner = await context.newPage();
  const inventoryWrites: any[] = [];
  inventoryOwner.on('request', request => {
    if (/\/leptos-api\/brew_measurement/.test(request.url()) && commandFrom(request)?.action === 'record') inventoryWrites.push(commandFrom(request));
  });
  await inventoryOwner.goto(`/brews/${brewId}`);
  await expect(inventoryOwner.getByRole('button', { name: /^Check retained evidence request / })).toHaveCount(2);
  await inventoryOwner.getByRole('button', { name: `Check retained evidence request ${commandsA[0].request_id}`, exact: true }).click();
  await expect(inventoryOwner.getByText('Outcome uncertain', { exact: true })).toBeVisible();
  releaseA();
  await expect(page.getByText('Measurement recorded.', { exact: true })).toBeVisible();
  await expect(page.getByText('Confirmed result — local cleanup pending', { exact: true })).toBeVisible();
  // This owner selected A while it was Unknown. A different owner then recorded
  // the known receipt; Retry must adopt that status, not send or repeatedly refuse.
  const staleRetry = inventoryOwner.getByRole('button', { name: 'Retry same request', exact: true });
  if (await staleRetry.isVisible()) await staleRetry.click();
  await expect(inventoryOwner.getByText('Confirmed result — local cleanup pending', { exact: true })).toBeVisible();
  expect(inventoryWrites).toHaveLength(0);
  // A real rescan goes from two Unknown choices to Confirmed A + Unknown B.
  // The old choice list must be replaced, not left behind after auto-adopting B.
  await inventoryOwner.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(inventoryOwner.getByText('Confirmed result — local cleanup pending', { exact: true })).toBeVisible();
  await expect(inventoryOwner.getByText('Outcome uncertain', { exact: true })).toBeVisible();
  await expect(inventoryOwner.getByRole('button', { name: /^Check retained evidence request / })).toHaveCount(0);
  page.once('dialog', dialog => dialog.accept());
  await page.reload();
  await expect(page.getByText('Outcome uncertain', { exact: true })).toBeVisible();
  await expect(page.getByText('Confirmed result — local cleanup pending', { exact: true })).toBeVisible();
  // Exercise the broken enabled path where present; corrected gating must also
  // keep B visible/locked rather than hiding the defect behind disabled markup.
  const retryB = page.getByRole('button', { name: 'Retry same request', exact: true });
  if (await retryB.isEnabled()) await retryB.click();
  await expect(page.getByText('Outcome uncertain', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Record reading', exact: true }).first()).toBeDisabled();
  expect(commandsA).toHaveLength(1);
  expect(commandsB).toHaveLength(1);
  await page.getByRole('button', { name: 'Retry local cleanup only', exact: true }).click();
  await expect(retryB).toBeEnabled();
  await expect(page.getByRole('button', { name: 'Record reading', exact: true }).first()).toBeDisabled();
  // The recovering owner is this page now; inspect its next actual request.
  await page.unroute('**/leptos-api/brew_measurement*');
  await page.route('**/leptos-api/brew_measurement*', async route => {
    const command = commandFrom(route.request());
    if (command?.action === 'record') commandsB.push(command);
    await route.continue();
  });
  await retryB.click();
  await expect(page.getByText('Measurement recorded.', { exact: true })).toBeVisible();
  expect(commandsB).toHaveLength(2);
  expect(commandsB[1]).toEqual(commandsB[0]);
  expect(sqliteExec(`SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = '${workspace}' AND brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`)).toBe('2');
  // Let background requests finish; closing a tab cancels in-flight server calls.
  await other.waitForLoadState('networkidle');
  await other.close();
  await inventoryOwner.close();
});

test('retained evidence fragment arrival cannot exempt a later independent native reload', async ({ page }) => {
  const brewId = await startBrewAndOpenEvidence(page, `Evidence native fragment ${Date.now()}`);
  const commands: any[] = [];
  await page.route('**/leptos-api/brew_measurement*', async route => {
    const command = commandFrom(route.request());
    if (command?.action !== 'record') return route.continue();
    commands.push(command);
    const response = await route.fetch();
    expect(response.ok()).toBeTruthy();
    await route.abort('failed');
  });
  await page.locator('#measurement-gravity-original-v1').fill('1.050');
  await page.getByRole('button', { name: 'Record reading', exact: true }).first().click();
  await expect(page.getByText(/^Reading request uncertain:/)).toBeVisible();
  const original = page.url();
  await page.evaluate(() => {
    // Only a native fragment-link fixture: production owns the actual click,
    // guard, permitted departure and native unload decisions.
    const link = document.createElement('a');
    link.href = `${location.href}#retained-evidence-fixture`;
    link.textContent = 'Native fragment recovery fixture';
    document.querySelector('.app-shell__main')!.append(link);
  });
  await page.getByRole('link', { name: 'Native fragment recovery fixture', exact: true }).click();
  const guard = page.locator('#workspace-draft-warning');
  await expect(guard).toBeVisible();
  await guard.getByRole('button', { name: 'Discard local inputs and leave', exact: true }).click();
  await expect(page).toHaveURL(`${original}#retained-evidence-fixture`);
  await expect(guard).not.toBeVisible();
  await expect(page.getByText('Outcome uncertain', { exact: true })).toBeVisible();
  const retainedEntry = await page.evaluate(() => ({
    document: performance.timeOrigin,
    key: (window as any).navigation.currentEntry.key,
    length: history.length,
  }));
  const warning = page.waitForEvent('dialog');
  // Trigger the real native reload, but do not wait for a load event that Stay
  // deliberately prevents. Unexpected document destruction remains an error.
  const reload = page.evaluate(() => window.location.reload());
  const dialog = await warning;
  expect(dialog.type()).toBe('beforeunload');
  await dialog.dismiss();
  await reload;
  await expect(page).toHaveURL(`${original}#retained-evidence-fixture`);
  await expect(page.getByText('Outcome uncertain', { exact: true })).toBeVisible();
  expect(await page.evaluate(() => ({ document: performance.timeOrigin, key: (window as any).navigation.currentEntry.key, length: history.length }))).toEqual(retainedEntry);
  // A different native listener can still require confirmation. Cancel the real
  // owned document departure and require the matching native abort disposition.
  await page.evaluate(() => window.addEventListener('beforeunload', event => {
    event.preventDefault(); event.returnValue = '';
  }, { once: true }));
  await page.getByRole('link', { name: 'Recipes', exact: true }).first().click();
  await expect(guard).toBeVisible();
  const ownedWarning = page.waitForEvent('dialog');
  const leave = guard.getByRole('button', { name: 'Discard local inputs and leave', exact: true }).click().catch(() => null);
  const ownedDialog = await ownedWarning;
  expect(ownedDialog.type()).toBe('beforeunload');
  await ownedDialog.dismiss();
  await leave;
  await expect(page.getByText(/^Document navigation was canceled\./)).toBeVisible();
  await expect(page).toHaveURL(`${original}#retained-evidence-fixture`);
  expect(await page.evaluate(() => ({ document: performance.timeOrigin, key: (window as any).navigation.currentEntry.key, length: history.length }))).toEqual(retainedEntry);
  const independentWarning = page.waitForEvent('dialog');
  const independentReload = page.evaluate(() => window.location.reload());
  const independentDialog = await independentWarning;
  expect(independentDialog.type()).toBe('beforeunload');
  await independentDialog.dismiss();
  await independentReload;
  await expect(page).toHaveURL(`${original}#retained-evidence-fixture`);
  await expect(page.getByText('Outcome uncertain', { exact: true })).toBeVisible();
  expect(await page.evaluate(() => ({ document: performance.timeOrigin, key: (window as any).navigation.currentEntry.key, length: history.length }))).toEqual(retainedEntry);
  expect(commands).toHaveLength(1);
  expect(sqliteExec(`SELECT COUNT(*) FROM brew_measurement_entries WHERE brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`)).toBe('1');
});

test('same-owner rescan cannot downgrade a known evidence receipt when marker persistence failed', async ({ page }) => {
  const brewId = await startBrewAndOpenEvidence(page, `Local known evidence ${Date.now()}`);
  const commands: any[] = [];
  await page.route('**/leptos-api/brew_measurement*', async route => {
    const command = commandFrom(route.request());
    if (command?.action !== 'record') return route.continue();
    commands.push(command);
    const response = await route.fetch();
    expect(response.ok()).toBeTruthy();
    // Initial Unknown capture already succeeded. Fault only subsequent marker/
    // receipt writes before releasing the real successful server response.
    await page.evaluate(() => {
      const setItem = Storage.prototype.setItem;
      (window as any).__restoreEvidenceMarkers = () => { Storage.prototype.setItem = setItem; };
      Storage.prototype.setItem = function (key, value) {
        if (key.startsWith('krausen-evidence-v1:')) throw new DOMException('Controlled marker failure after capture', 'QuotaExceededError');
        return setItem.call(this, key, value);
      };
    });
    await route.fulfill({ response });
  });
  await page.locator('#measurement-gravity-original-v1').fill('1.050');
  await page.getByRole('button', { name: 'Record reading', exact: true }).first().click();
  await expect(page.getByText('Measurement recorded.', { exact: true })).toBeVisible();
  await expect(page.getByText('Confirmed result — local cleanup pending', { exact: true })).toBeVisible();
  const durable = await page.evaluate(() => Object.keys(localStorage).filter(key => key.startsWith('krausen-evidence-v1:')).map(key => JSON.parse(localStorage.getItem(key)!)));
  expect(durable).toHaveLength(1);
  expect(durable[0].status).toBe('Unknown');
  expect(durable[0].command.Measurement).toEqual(commands[0]);
  let contextReads = 0;
  const finalContext = page.waitForResponse(response => response.url().includes('/leptos-api/get_user_context') && ++contextReads === 2);
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await (await finalContext).finished();
  // Drain the actual final authority response and the owner's settled frame,
  // not a wall-clock sleep or a copied recovery state machine.
  await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => resolve())));
  await expect(page.getByText('Confirmed result — local cleanup pending', { exact: true })).toBeVisible();
  await expect(page.getByText('Outcome uncertain', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Retry same request', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Record reading', exact: true }).first()).toBeDisabled();
  expect(commands).toHaveLength(1);
  await page.evaluate(() => (window as any).__restoreEvidenceMarkers());
  await page.getByRole('button', { name: 'Retry local cleanup only', exact: true }).click();
  await expect(page.getByText('Confirmed result — local cleanup pending', { exact: true })).toHaveCount(0);
  expect(commands).toHaveLength(1);
  expect(sqliteExec(`SELECT COUNT(*) FROM brew_measurement_entries WHERE brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`)).toBe('1');
});

test('confirmed evidence cleanup failure survives reload and retries removal without resending', async ({ page, context }) => {
  const brewId = await startBrewAndOpenEvidence(page, `Evidence cleanup debt ${Date.now()}`);
  const workspace = sqliteExec(`SELECT workspace_id FROM brews WHERE brew_id = '${brewId}'`);
  const commands: any[] = [];
  page.on('request', request => {
    if (!/\/leptos-api\/brew_measurement/.test(request.url())) return;
    const command = commandFrom(request);
    if (command?.action === 'record') commands.push(command);
  });
  await page.evaluate(() => {
    const removeItem = Storage.prototype.removeItem;
    Storage.prototype.removeItem = function (key) {
      if (key.startsWith('krausen-evidence-v1:')) throw new DOMException('Controlled cleanup failure', 'SecurityError');
      return removeItem.call(this, key);
    };
  });
  await page.locator('#measurement-gravity-original-v1').fill('1.050');
  await page.getByRole('button', { name: 'Record reading', exact: true }).first().click();
  await expect(page.getByText('Measurement recorded.', { exact: true })).toBeVisible();
  await expect(page.getByText('Confirmed result — local cleanup pending', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Record reading', exact: true }).first()).toBeDisabled();
  expect(commands).toHaveLength(1);
  expect(sqliteExec(`SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = '${workspace}' AND brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`)).toBe('1');
  page.once('dialog', dialog => dialog.accept());
  await page.reload();
  await expect(page.getByText('Confirmed result — local cleanup pending', { exact: true })).toBeVisible();
  await expect(page.getByText('Outcome uncertain', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Retry same request', exact: true })).toHaveCount(0);
  expect(commands).toHaveLength(1);
  // A second real owner may complete this exact cleanup first. The original
  // owner's known receipt/debt must reconcile idempotently, never resend.
  const cleanupOwner = await context.newPage();
  const cleanupWrites: any[] = [];
  cleanupOwner.on('request', request => {
    if (/\/leptos-api\/brew_measurement/.test(request.url()) && commandFrom(request)?.action === 'record') cleanupWrites.push(commandFrom(request));
  });
  await cleanupOwner.goto(`/brews/${brewId}`);
  await cleanupOwner.getByRole('button', { name: 'Retry local cleanup only', exact: true }).click();
  await expect.poll(() => cleanupOwner.evaluate(() => Object.keys(localStorage).filter(key => key.startsWith('krausen-evidence-v1:')).length)).toBe(0);
  await page.getByRole('button', { name: 'Retry local cleanup only', exact: true }).click();
  await expect(page.getByText('Confirmed result — local cleanup pending', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Record reading', exact: true }).first()).toBeEnabled();
  await expect.poll(() => page.evaluate(() => Object.keys(localStorage).filter(key => key.startsWith('krausen-evidence-v1:')).length)).toBe(0);
  expect(commands).toHaveLength(1);
  expect(cleanupWrites).toHaveLength(0);
  expect(sqliteExec(`SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = '${workspace}' AND brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`)).toBe('1');
  await cleanupOwner.close();
});

test('failed recovery discovery blocks fresh evidence until the exact original request is found', async ({ page }) => {
  const brewId = await startBrewAndOpenEvidence(page, `Evidence scan failure ${Date.now()}`);
  const workspace = sqliteExec(`SELECT workspace_id FROM brews WHERE brew_id = '${brewId}'`);
  const commands: any[] = [];
  await page.route('**/leptos-api/brew_measurement*', async route => {
    const command = commandFrom(route.request());
    if (command?.action !== 'record') return route.continue();
    commands.push(command);
    if (commands.length === 1) {
      const response = await route.fetch();
      expect(response.ok()).toBeTruthy();
      expect(sqliteExec(`SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = '${workspace}' AND brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`)).toBe('1');
      await route.abort('failed');
    } else await route.continue();
  });
  await page.locator('#measurement-gravity-original-v1').fill('1.050');
  await page.getByRole('button', { name: 'Record reading', exact: true }).first().click();
  await expect.poll(() => commands.length).toBe(1);
  await expect(page.getByText(/^Reading request uncertain:/)).toBeVisible();
  await expect.poll(() => page.evaluate(() => Object.keys(localStorage).filter(key => key.startsWith('krausen-evidence-v1:')).length)).toBe(1);
  await page.addInitScript(() => {
    const key = Storage.prototype.key;
    Storage.prototype.key = function (index) {
      if ((window as any).__restoreEvidenceDiscovery !== true) throw new DOMException('Controlled enumeration fault', 'SecurityError');
      return key.call(this, index);
    };
  });
  page.once('dialog', dialog => dialog.accept());
  await page.reload();
  await expect(page.getByText('Recovery discovery unavailable', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Record reading', exact: true }).first()).toBeDisabled();
  expect(commands).toHaveLength(1);
  await page.evaluate(() => { (window as any).__restoreEvidenceDiscovery = true; });
  await page.getByRole('button', { name: 'Retry recovery discovery', exact: true }).click();
  await expect(page.getByText('Outcome uncertain', { exact: true })).toBeVisible();
  expect(commands).toHaveLength(1);
  await page.getByRole('button', { name: 'Retry same request', exact: true }).click();
  await expect(page.getByText('Measurement recorded.', { exact: true })).toBeVisible();
  expect(commands).toHaveLength(2);
  expect(commands[1]).toEqual(commands[0]);
  expect(sqliteExec(`SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = '${workspace}' AND brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`)).toBe('1');
});

test('discarding unsent evidence preserves a committed uncertain request through navigation and reload', async ({ page }) => {
  const brewId = await startBrewAndOpenEvidence(page, `Evidence retained leave ${Date.now()}`);
  const workspace = sqliteExec(`SELECT workspace_id FROM brews WHERE brew_id = '${brewId}'`);
  const recordCommands: any[] = [];
  await page.route('**/leptos-api/brew_measurement*', async route => {
    const command = commandFrom(route.request());
    if (command?.action !== 'record') {
      await route.continue();
      return;
    }
    recordCommands.push(command);
    if (recordCommands.length === 1) {
      const response = await route.fetch();
      expect(response.ok()).toBeTruthy();
      expect(sqliteExec(`SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = '${workspace}' AND brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`)).toBe('1');
      await route.abort('failed');
      return;
    }
    await route.continue();
  });
  await page.locator('#measurement-gravity-original-v1').fill('1.050');
  await page.getByRole('button', { name: 'Record reading', exact: true }).first().click();
  await expect(page.getByText('Outcome uncertain', { exact: true })).toBeVisible();
  // Pending is rendered before preflight/transport finishes. Wait for the real
  // committed-and-lost response, not merely that early pending announcement.
  await expect.poll(() => recordCommands.length).toBe(1);
  await expect(page.getByText(/^Reading request uncertain:/)).toBeVisible();
  await page.locator('#brew-note').fill('Discard only this separate unsent note');
  await page.getByRole('link', { name: 'Recipes', exact: true }).first().click();
  const guard = page.locator('#workspace-draft-warning');
  await expect(guard).toBeVisible();
  // Reach the actual discard transition on both the legacy and corrected guard;
  // retention, not a renamed button alone, is the regression under test.
  await guard.getByRole('button', { name: /^(Discard local inputs and leave|Discard and leave)$/ }).click();
  await expect(page).toHaveURL(/\/recipes$/);
  expect(recordCommands).toHaveLength(1);
  // A full page load while a token refresh is still in flight can abort that refresh after the
  // server rotated the token; the browser then keeps the stale token, the next load presents it, and
  // the server revokes the session (reuse detection), landing on /login. Let the network settle first.
  // (Reproduced 1/5 on the unmodified base commit as well.)
  await page.waitForLoadState('networkidle');
  await page.goto(`/brews/${brewId}`);
  await expect(page.getByRole('heading', { name: 'Brew evidence', exact: true })).toBeVisible();
  await expect(page.getByText('Outcome uncertain', { exact: true })).toBeVisible();
  await expect(page.locator('#brew-note')).toHaveValue('');
  expect(recordCommands).toHaveLength(1);
  page.once('dialog', dialog => dialog.accept());
  await page.reload();
  await expect(page.getByText('Outcome uncertain', { exact: true })).toBeVisible();
  await expect(page.locator('#brew-note')).toHaveValue('');
  expect(recordCommands).toHaveLength(1);
  await page.getByRole('button', { name: 'Retry same request', exact: true }).click();
  await expect(page.getByText('Measurement recorded.', { exact: true })).toBeVisible();
  expect(recordCommands).toHaveLength(2);
  expect(recordCommands[1]).toEqual(recordCommands[0]);
  expect(recordCommands[1].request_id).toBe(recordCommands[0].request_id);
  expect(recordCommands[1].intent.workspace_id).toBe(workspace);
  expect(recordCommands[1].brew_id).toBe(brewId);
  expect(sqliteExec(`SELECT COUNT(*) FROM brew_measurement_entries WHERE workspace_id = '${workspace}' AND brew_id = '${brewId}' AND quantity_id = 'gravity.original.v1'`)).toBe('1');
});
