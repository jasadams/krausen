import { test, expect, type Browser } from '@playwright/test';
import { db, gotoAuthenticated, isPersonalMode, isSelfHostedMode, loginUser, waitForWasm } from '../../helpers/test-helpers';

async function createSession(browser: Browser, userAgent: string) {
  const context = await browser.newContext({ userAgent });
  try {
    const page = await context.newPage();
    if (isSelfHostedMode()) await loginUser(page, 'local@localhost', 'TestPassword1234');
    else { await page.goto('/'); await waitForWasm(page); }
  } finally { await context.close(); }
}

test('uncertain revocation retains its request identity when another row completes', async ({ page, browser }) => {
  test.skip(isPersonalMode(), 'personal mode has no browser sessions');
  const marker = `KRA11-${Date.now()}`;
  const agentA = `${marker}-A`;
  const agentB = `${marker}-B`;
  await createSession(browser, agentA);
  await createSession(browser, agentB);
  await gotoAuthenticated(page, '/settings/security');
  const rowA = page.locator('tr').filter({ hasText: agentA });
  const rowB = page.locator('tr').filter({ hasText: agentB });
  await expect(rowA).toBeVisible();
  await expect(rowB).toBeVisible();
  type Session = { token_id: string; [key: string]: unknown };
  let initialSessions: Session[] = [];
  let aToken: string | undefined;
  await page.route('**/leptos-api/get_sessions*', async route => {
    const response = await route.fetch();
    const sessions = await response.json() as Session[];
    if (!aToken) initialSessions = sessions;
    // Keep A visible after B's success-triggered refresh, simulating a stale
    // screen where an uncertain committed response can be retried.
    if (aToken && !sessions.some(session => session.token_id === aToken)) {
      const saved = initialSessions.find(session => session.token_id === aToken);
      if (saved) sessions.push(saved);
    }
    await route.fulfill({ response, json: sessions });
  });
  await page.locator('button[title="Refresh sessions"]').click();
  await expect.poll(() => initialSessions.length).toBeGreaterThanOrEqual(3);
  const requests: { token_id: string; request_id: string }[] = [];
  await page.route('**/leptos-api/revoke_session*', async route => {
    const payload = route.request().postDataJSON() as { token_id: string; request_id: string };
    requests.push(payload);
    if (requests.length === 1) {
      aToken = payload.token_id;
      expect(aToken).toMatch(/^[A-Za-z0-9_-]+$/);
      const response = await route.fetch();
      expect(response.ok()).toBeTruthy();
      expect((await response.json()).outcome.status).toBe('revoked');
      expect(db.exec(`SELECT COUNT(*) FROM account_session_revocations WHERE expected_token_id='${aToken}'`)).toBe('1');
      expect(db.exec(`SELECT COUNT(*) FROM refresh_tokens WHERE token_id='${aToken}' AND is_active=0`)).toBe('1');
      expect(db.exec(`SELECT COUNT(*) FROM auth_credentials c JOIN refresh_tokens t ON t.family_id=c.credential_id WHERE t.token_id='${aToken}' AND c.revoked_at IS NOT NULL`)).toBe('1');
      await route.abort('failed'); // The committed response is lost to the browser.
    } else if (requests.length === 3) {
      const response = await route.fetch();
      expect(response.ok()).toBeTruthy();
      expect((await response.json()).outcome.status).toBe('revoked');
      await route.fulfill({ response });
    } else await route.continue();
  });
  const disconnect = async (row: typeof rowA) => {
    await row.locator('button[title="Disconnect this session"]').click();
    await page.getByRole('button', { name: 'Disconnect', exact: true }).click();
  };
  await disconnect(rowA);
  await expect(page.getByText(/Failed to disconnect session/)).toBeVisible();
  await disconnect(rowB);
  await expect(rowB).toHaveCount(0);
  await expect(rowA).toBeVisible();
  await disconnect(rowA);
  await expect.poll(() => requests.length).toBe(3);
  expect(requests[0].token_id).toBe(requests[2].token_id);
  expect(requests[0].token_id).not.toBe(requests[1].token_id);
  expect(requests[2].request_id).toBe(requests[0].request_id);
  expect(requests[2]).toEqual(requests[0]);
  expect(db.exec(`SELECT COUNT(*) FROM account_session_revocations WHERE expected_token_id='${aToken}'`)).toBe('1');
  await expect(page.getByText('Active Sessions', { exact: true })).toBeVisible();
});
