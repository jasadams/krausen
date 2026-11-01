import { test, expect, type APIRequestContext } from '@playwright/test';
import { createHash, randomBytes } from 'node:crypto';
import { db, getDefaultUserId, getDefaultWorkspaceId, gotoAuthenticated, isPersonalMode, seedUser, seedWorkspace, seedWorkspaceMember } from '../../helpers/test-helpers';

function rpcBody(text: string): any {
  const data = text.split('\n').find(line => line.startsWith('data:') && line.slice(5).trim().startsWith('{'));
  return JSON.parse(data ? data.slice(5).trim() : text);
}

test('browser switches immediately without retargeting an existing MCP connection', async ({ page }) => {
  test.skip(isPersonalMode(), 'workspace switching requires a normal authenticated account');
  await gotoAuthenticated(page, '/settings/workspace');
  const base = new URL(page.url()).origin;
  const user = getDefaultUserId();
  const first = getDefaultWorkspaceId();
  const firstName = db.query(`SELECT name FROM workspaces WHERE workspace_id='${first}'`)[0];
  const secondName = `Switch evidence ${Date.now()}`;
  const second = seedWorkspace({ ownerId: user, name: secondName });
  // Seeding writes the default preference; restore the setup state before the
  // actual switch so the browser and MCP begin in the first workspace.
  db.exec(`UPDATE users SET last_workspace_id='${first}' WHERE user_id='${user}'`);
  const extra = seedUser({ name: 'Second workspace only' });
  seedWorkspaceMember({ workspaceId: second, userId: extra.userId });

  const request: APIRequestContext = page.request;
  const redirect = 'http://127.0.0.1:34567/callback';
  const registration = await request.post('/api/v1/oauth/register', { data: { client_name: 'Switch isolation test', redirect_uris: [redirect], token_endpoint_auth_method: 'none' } });
  expect(registration.status()).toBe(201);
  const client = await registration.json();
  const verifier = randomBytes(32).toString('base64url');
  const challenge = createHash('sha256').update(verifier).digest('base64url');
  const resource = `${base}/mcp`;
  const authorize = await request.get('/api/v1/oauth/authorize', { params: { client_id: client.client_id, redirect_uri: redirect, resource, state: 'switch-test', code_challenge: challenge, code_challenge_method: 'S256', response_type: 'code' } });
  expect(authorize.status()).toBe(200);
  const pending = (await authorize.text()).match(/name="state" value="([^"]+)"/)?.[1];
  expect(pending).toBeTruthy();
  const approved = await request.post('/api/v1/oauth/authorize/approve', { form: { state: pending! }, headers: { origin: base }, maxRedirects: 0 });
  expect(approved.status()).toBe(303);
  const code = new URL(approved.headers().location).searchParams.get('code');
  expect(code).toBeTruthy();
  const exchanged = await request.post('/api/v1/oauth/token', { form: { grant_type: 'authorization_code', client_id: client.client_id, resource, code: code!, redirect_uri: redirect, code_verifier: verifier } });
  expect(exchanged.status()).toBe(200);
  const token = (await exchanged.json()).access_token;
  const headers: Record<string, string> = { authorization: `Bearer ${token}`, accept: 'application/json, text/event-stream', 'content-type': 'application/json' };
  const init = await request.post('/mcp', { headers, data: { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'switch-test', version: '1' } } } });
  expect(init.ok()).toBeTruthy();
  const sessionId = init.headers()['mcp-session-id'];
  if (sessionId) headers['mcp-session-id'] = sessionId;
  headers['mcp-protocol-version'] = '2025-11-25';
  await request.post('/mcp', { headers, data: { jsonrpc: '2.0', method: 'notifications/initialized' } });
  async function members(id: number, workspaceId: string): Promise<string[]> {
    const response = await request.post('/mcp', { headers, data: { jsonrpc: '2.0', id, method: 'tools/call', params: { name: 'team_members', arguments: { workspace_id: workspaceId } } } });
    expect(response.ok()).toBeTruthy();
    const outcome = rpcBody(await response.text()).result.structuredContent.outcome;
    expect(outcome.status).toBe('available');
    return outcome.page.items.map((item: { user_id: string }) => item.user_id).sort();
  }
  const before = await members(2, first);
  expect(before).not.toContain(extra.userId);
  expect(await members(3, second)).toContain(extra.userId);
  await page.getByRole('button').filter({ hasText: firstName }).click();
  await page.getByRole('button', { name: secondName, exact: true }).click();
  await expect(page.getByRole('button').filter({ hasText: secondName })).toBeVisible();
  await expect(page.getByPlaceholder('My Workspace')).toHaveValue(secondName);
  expect(await members(4, first)).toEqual(before);
  expect(await members(5, second)).toContain(extra.userId);
});
