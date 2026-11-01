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

test('held Record stays locked while a real focus overview completes', async ({ page }) => {
  await startBrewAndOpenEvidence(page, `Brew delayed write ${Date.now()}`);
  await page.waitForTimeout(500);
  let releaseRecord!: () => void;
  const recordReleased = new Promise<void>((resolve) => {
    releaseRecord = resolve;
  });
  let recordEntered!: () => void;
  const recordHeld = new Promise<void>((resolve) => {
    recordEntered = resolve;
  });
  let overviewCompleted!: () => void;
  const overviewDone = new Promise<void>((resolve) => {
    overviewCompleted = resolve;
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
  let awaitingOverview = false;
  page.on('response', (response) => {
    if (
      awaitingOverview &&
      response.url().includes('/brew_evidence_view') &&
      response.ok()
    ) {
      overviewCompleted();
    }
  });
  await page.locator('#measurement-gravity-original-v1').fill('1.050');
  await page.getByRole('button', { name: 'Record reading' }).first().click();
  await recordHeld;
  awaitingOverview = true;
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await overviewDone;
  await expect(page.getByText('Outcome uncertain')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Retry same request' })).toBeDisabled();
  await expect(
    page.getByRole('button', { name: 'Abandon uncertain request' }),
  ).toBeDisabled();
  expect(recordCommands).toHaveLength(1);
  releaseRecord();
  await expect(page.getByText('Measurement recorded.')).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText('Outcome uncertain')).toHaveCount(0);
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

test('uncertain evidence request is discarded on workspace change', async ({ page }) => {
  await startBrewAndOpenEvidence(page, `Brew context loss ${Date.now()}`);
  await page.route('**/brew_measurement*', (route) => route.abort('failed'));
  await page.locator('#measurement-gravity-original-v1').fill('1.050');
  await page.getByRole('button', { name: 'Record reading' }).first().click();
  await expect(page.getByText('Outcome uncertain')).toBeVisible();
  const workspaceName = `Evidence context ${Date.now()}`;
  seedWorkspace({ ownerId: getDefaultUserId(), name: workspaceName });
  walCheckpoint();
  await page.getByRole('button', { name: new RegExp(DEFAULT_TEST_USER.email) }).click();
  await page.getByRole('button', { name: workspaceName, exact: true }).click();
  const guard = page.getByRole('dialog', { name: 'Unsaved recipe changes' });
  await expect(guard.getByRole('button', { name: 'Discard and leave' })).toBeEnabled();
  await guard.getByRole('button', { name: 'Discard and leave' }).click();
  await expect(page.getByText('Brew not found in this workspace.')).toBeVisible();
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
