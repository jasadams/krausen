import { test, expect } from '@playwright/test';
import { execFileSync } from 'child_process';
import * as path from 'path';
import {
  DEFAULT_TEST_USER,
  getDefaultUserId,
  loginUser,
  seedWorkspace,
  walCheckpoint,
} from '../../helpers/test-helpers';

const DB_PATH = process.env.KRAUSEN_E2E_DATABASE_PATH
  ? path.resolve(process.env.KRAUSEN_E2E_DATABASE_PATH)
  : path.resolve(__dirname, '../../../data/krausen.db');

function sqliteExec(sql: string): string {
  return execFileSync('sqlite3', [DB_PATH, '.timeout 5000', sql], {
    encoding: 'utf-8',
  }).trim();
}

function commandFrom(request: import('@playwright/test').Request): any {
  const payload = new URLSearchParams(request.postData() ?? '').get('request_json');
  return payload ? JSON.parse(payload) : null;
}

async function callServerFn(
  page: import('@playwright/test').Page,
  name: string,
  payload: unknown,
): Promise<{ status: number; body: any }> {
  return page.evaluate(
    async ({ name, payload }) => {
      const response = await fetch(`/leptos-api/${name}`, {
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
    { name, payload },
  );
}

function outcomeOf(body: any): any {
  return body?.Ok ?? body;
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

async function makeSavedRevisionReady(page: import('@playwright/test').Page) {
  await page.getByText('Equipment settings', { exact: true }).click();
  await page.locator('#equipment-profile-edit').selectOption('');
  await page.locator('#equipment-profile-name').fill('E2E kettle');
  await page.locator('#profile-mash-efficiency').fill('75');
  await page.locator('#profile-boil-off').fill('3');
  await page.locator('#profile-transfer-loss').fill('2');
  await page.locator('#profile-deadspace').fill('0');
  await page.locator('#profile-absorption').fill('0.8');
  await page.locator('#profile-thickness').fill('2.5');
  await page.locator('#profile-capacity').fill('30');
  await page.locator('#profile-diameter').fill('35');
  await page.locator('#profile-opening').fill('30');
  await page.locator('#profile-boil-temperature').fill('100');
  await page.locator('#profile-chilling').selectOption('natural');
  await page.getByRole('button', { name: 'Save profile' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Profile saved' })).toBeVisible();
  await page.locator('#equipment-profile-select').selectOption({ label: 'E2E kettle' });
  await page.getByText('Addition details', { exact: true }).first().click();
  await page.locator('#fermentables-fermentable-1-stage').selectOption('mash');
  await page.getByText('Recipe process and notes', { exact: true }).click();
  await page.locator('#recipe-attenuation').fill('78');
  await page.locator('#recipe-mash-method').selectOption('manual');
  await page.locator('#recipe-mash-water').fill('15');
  await page.locator('#recipe-whirlpool').fill('15');
  await page.locator('#recipe-top-up').fill('0');
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 2' })).toBeVisible();
}

async function prepareAndStart(page: import('@playwright/test').Page, revision: string) {
  await page.locator('#brew-start-revision').fill(revision);
  await page.getByRole('button', { name: 'Prepare start' }).click();
  await expect(
    page.getByText('Ready to start the selected saved revision.'),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Start brew' }).click();
  await expect(page.getByText('Brew started', { exact: true })).toBeVisible();
}


async function startBrewAndOpenEvidence(
  page: import('@playwright/test').Page,
  title: string,
): Promise<string> {
  await importRecipe(page, title);
  await makeSavedRevisionReady(page);
  await prepareAndStart(page, '2');
  await page.getByRole('link', { name: 'Open brew evidence' }).click();
  await expect(page.getByRole('heading', { name: 'Brew evidence' })).toBeVisible();
  return page.url().split('/').pop() ?? '';
}

function lifecycleItems(page: import('@playwright/test').Page) {
  return page
    .locator('h3', { hasText: 'Lifecycle evidence' })
    .locator('xpath=following-sibling::ul[1]/li');
}

test('starts the prepared saved revision with frozen targets', async ({ page }) => {
  const title = `Brew start ${Date.now()}`;
  await importRecipe(page, title);
  await makeSavedRevisionReady(page);
  await prepareAndStart(page, '2');
  await expect(page.getByText(`from revision 2`)).toBeVisible();
});

test('starts an explicitly selected historical revision without restoring it', async ({
  page,
}) => {
  const title = `Brew historical ${Date.now()}`;
  await importRecipe(page, title);
  await makeSavedRevisionReady(page);
  await page.getByLabel('Recipe name', { exact: true }).fill(`${title} head three`);
  await page.getByRole('button', { name: 'Save recipe' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved revision 3' })).toBeVisible();
  await prepareAndStart(page, '2');
  await expect(page.getByText('from revision 2')).toBeVisible();
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
      .getByText('completed', { exact: true }),
  ).toBeVisible();
  await expect(page.getByText('Outcome uncertain')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Retry same request' })).toBeDisabled();
  await expect(
    page.getByRole('button', { name: 'Abandon uncertain request' }),
  ).toBeDisabled();
  expect(recordCommands).toHaveLength(1);
  releaseRecord();
  await expect(page.getByText('Measurement recorded.')).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText('Outcome uncertain')).toHaveCount(0);
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
  await page.getByRole('button', { name: 'Retry same request' }).click();
  await expect(page.getByText('Measurement recorded.')).toBeVisible();
  await expect.poll(() => recordCommands.length).toBe(2);
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
  seedWorkspace({ ownerId: getDefaultUserId(), name: otherName });
  sqliteExec(
    `UPDATE users SET last_workspace_id = '${workspace}' WHERE user_id = '${getDefaultUserId()}'`,
  );
  walCheckpoint();
  const switcher = await context.newPage();
  await switcher.goto(page.url());
  await expect(switcher.getByRole('heading', { name: 'Brew evidence' })).toBeVisible();
  await switcher
    .getByRole('button', { name: new RegExp(DEFAULT_TEST_USER.email) })
    .click();
  await switcher.getByRole('button', { name: otherName, exact: true }).click();
  await expect(switcher.getByText('Brew not found in this workspace.')).toBeVisible();
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByText('Account or workspace changed')).toBeVisible();
  await expect(page.getByText('Outcome uncertain')).toHaveCount(0);
  await expect(page.locator('#brew-note')).toHaveCount(0);
  await page.getByRole('link', { name: 'Recipes' }).first().click();
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await expect(guard.getByRole('button', { name: 'Discard and leave' })).toBeEnabled();
  await guard.getByRole('button', { name: 'Discard and leave' }).click();
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
  await expect(page.getByText(/selected 1\.052/)).toBeVisible();
  await page.getByRole('button', { name: 'Correct selected' }).first().click();
  await expect(page.getByText('Measurement corrected.')).toBeVisible();
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
  seedWorkspace({ ownerId: getDefaultUserId(), name: otherName });
  sqliteExec(
    `UPDATE users SET last_workspace_id = '${originalWorkspace}' WHERE user_id = '${getDefaultUserId()}'`,
  );
  walCheckpoint();
  const switcher = await context.newPage();
  await switcher.goto(page.url());
  await expect(switcher.getByRole('heading', { name: 'Brew evidence' })).toBeVisible();
  await switcher
    .getByRole('button', { name: new RegExp(DEFAULT_TEST_USER.email) })
    .click();
  await switcher.getByRole('button', { name: otherName, exact: true }).click();
  await expect(switcher.getByText('Brew not found in this workspace.')).toBeVisible();
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByText('Account or workspace changed')).toBeVisible();
  await expect(page.getByText('Outcome uncertain')).toHaveCount(0);
  await expect(page.locator('#measurement-gravity-original-v1')).toHaveCount(0);
  await switcher
    .getByRole('button', { name: new RegExp(DEFAULT_TEST_USER.email) })
    .click();
  await switcher.getByRole('button', { name: originalName, exact: true }).click();
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
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await expect(guard.getByRole('button', { name: 'Discard and leave' })).toBeEnabled();
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
  await switcher
    .getByRole('button', { name: new RegExp(DEFAULT_TEST_USER.email) })
    .click();
  await switcher.getByRole('button', { name: otherName, exact: true }).click();
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

test('recipe brew history and frozen historical print are reachable', async ({ page }) => {
  const title = `Brew history ${Date.now()}`;
  const brewId = await startBrewAndOpenEvidence(page, title);
  const recipeId = sqliteExec(
    `SELECT recipe_id FROM brews WHERE brew_id = '${brewId.replaceAll("'", "''")}';`,
  );
  await page.goto(`/recipes/${recipeId}/brews`);
  await expect(page.getByRole('heading', { name: 'Brew history' })).toBeVisible();
  await expect(page.getByText(/recipe revision 2/)).toBeVisible();
  await page.getByRole('link', { name: 'Compare and feedback' }).click();
  await expect(page.getByRole('heading', { name: 'Plan versus actual' })).toBeVisible();
  await expect(page.getByText('Target versus selected actual')).toBeVisible();
  await page.getByRole('button', { name: 'Print frozen plan' }).click();
  await expect(page.getByText('Historical print preview')).toBeVisible();
  await expect(page.getByText(/Historical brew .* frozen recipe revision 2/)).toBeVisible();
  await expect(page.getByText('Actuals (separate from the frozen plan)')).toBeVisible();
});

test('feedback apply keeps one request through a lost response and ignores selection changes', async ({
  page,
}) => {
  const title = `Brew feedback ${Date.now()}`;
  const brewId = await startBrewAndOpenEvidence(page, title);
  const recipeId = sqliteExec(
    `SELECT recipe_id FROM brews WHERE brew_id = '${brewId.replaceAll("'", "''")}';`,
  );
  const workspaceId = sqliteExec(
    `SELECT workspace_id FROM brews WHERE brew_id = '${brewId.replaceAll("'", "''")}';`,
  );
  const intent = { account_id: getDefaultUserId(), workspace_id: workspaceId };

  const comparison = await callServerFn(page, 'execute_brew_comparison', {
    intent,
    brew_id: brewId,
  });
  expect(comparison.status).toBe(200);
  const plan = outcomeOf(comparison.body).Comparison;
  const document = structuredClone(plan.plan.recipe);
  document.boil_minutes = 70;
  document.notes = 'Browser proof actual';
  const amended = await callServerFn(page, 'brew_actual', {
    Amend: {
      intent,
      brew_id: brewId,
      expected_actual_revision: null,
      document,
      reason: 'browser proof',
      request_id: crypto.randomUUID(),
    },
  });
  expect(amended.status).toBe(200);
  expect(outcomeOf(amended.body).Amended.revision).toBe(1);

  await page.goto(`/brews/${brewId}/analysis`);
  await expect(page.getByRole('heading', { name: 'Plan versus actual' })).toBeVisible();
  await expect(page.getByText(/actual revision 1/)).toBeVisible();
  await page
    .locator('li', { hasText: 'process:boil_minutes' })
    .getByRole('checkbox')
    .click();
  await page.getByRole('button', { name: 'Preview selected feedback' }).click();
  await expect(page.getByText(/Proposed recipe head 2/)).toBeVisible();

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
    .locator('li', { hasText: 'Notes · notes' })
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

  const after = await callServerFn(page, 'execute_brew_comparison', {
    intent,
    brew_id: brewId,
  });
  expect(outcomeOf(after.body).Comparison.plan.recipe.boil_minutes).toBe(60);
});
