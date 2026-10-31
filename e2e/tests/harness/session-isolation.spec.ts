import { test, expect } from '@playwright/test';
import { getAuthMode, openAuthenticatedContext } from '../../helpers/auth-session';
import { isPersonalMode } from '../../helpers/test-helpers';

/**
 * Harness regression for independent session/token families.
 *
 * Under the legacy static storageState harness, contexts share one family:
 * rotating in context A and replaying its old token after the 30s grace window
 * revokes the family, so context B fails. Under the fresh-session harness each
 * context logs in through the supported path and owns a separate family, so
 * revoking family A leaves family B usable.
 *
 * Run with E2E_AUTH_MODE=static for the expected red baseline and with
 * E2E_AUTH_MODE=fresh for the green acceptance run.
 */
test.describe('harness: isolated auth session families', () => {
  test.skip(isPersonalMode(), 'Requires self_hosted/SaaS password login');
  test.setTimeout(150_000);

  test('fresh family B survives replay revocation of family A', async ({ browser, request }) => {
    const mode = getAuthMode();
    const baseURL = process.env.BASE_URL ?? 'http://localhost:8099';

    const first = await openAuthenticatedContext(browser, { mode });
    try {
      // Capture the pre-rotation refresh token before the document load rotates it.
      const oldRefresh = (await first.context.cookies()).find(
        (cookie) => cookie.name === 'refresh_token'
      )?.value;
      expect(oldRefresh).toBeTruthy();

      await first.page.goto('/settings/security');
      await first.page.waitForLoadState('networkidle');
      await first.page.waitForTimeout(2_000);

      // The page-load refresh is intentional; force a second rotation if the
      // first has already completed so the 30s grace window starts here.
      await first.page
        .evaluate(() => (window as any).krausenRefresh?.() ?? Promise.resolve(false))
        .catch(() => false);
      await first.page.waitForTimeout(1_000);

      // One bounded wait: the product's documented rotation grace is 30s.
      // 35s is deliberately past the boundary, not an arbitrary retry delay.
      await new Promise((resolve) => setTimeout(resolve, 35_000));

      const replayClient = await request.newContext({ baseURL });
      try {
        const replay = await replayClient.post('/api/v1/auth/refresh', {
          headers: { cookie: `refresh_token=${oldRefresh}` },
        });
        expect(replay.status(), 'old token must be rejected after grace').toBe(401);
        expect(await replay.text()).toContain('Refresh token replayed');
      } finally {
        await replayClient.dispose();
      }

      // Family A is now revoked. A genuinely independent fresh login for
      // family B must still reach protected content and run server functions.
      const second = await openAuthenticatedContext(browser, { mode });
      try {
        await second.page.goto('/settings/security');
        await expect(
          second.page.getByText('Active Sessions', { exact: true })
        ).toBeVisible({ timeout: 15_000 });
      } finally {
        await second.context.close();
      }
    } finally {
      await first.context.close();
    }
  });
});
