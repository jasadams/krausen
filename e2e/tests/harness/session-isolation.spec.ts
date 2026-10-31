import { createHash } from 'crypto';
import { test, expect } from '@playwright/test';
import { getAuthMode, openAuthenticatedContext } from '../../helpers/auth-session';
import { db, isPersonalMode } from '../../helpers/test-helpers';

function tokenHash(token: string): string {
  return createHash('sha256').update(token).digest('hex');
}

function familyOf(token: string | undefined): string | null {
  if (!token) return null;
  const rows = db.query(
    `SELECT family_id FROM refresh_tokens WHERE token_hash = '${tokenHash(token)}' LIMIT 1`
  );
  return rows[0] ?? null;
}

function credentialState(familyId: string | null): string | null {
  if (!familyId) return null;
  const rows = db.query(
    `SELECT CASE WHEN revoked_at IS NULL THEN 'active' ELSE 'revoked' END FROM auth_credentials WHERE credential_id = '${familyId}'`
  );
  return rows[0] ?? null;
}

function familyLabel(familyId: string | null): string {
  return familyId ? `${familyId.slice(0, 8)}…` : 'missing';
}

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

      const familyA = familyOf(oldRefresh);
      expect(familyA).toBeTruthy();

      await first.page.goto('/settings/security');
      await first.page.waitForLoadState('networkidle');
      await first.page.waitForTimeout(2_000);

      // The page-load refresh is intentional; force a second rotation if the
      // first has already completed so the 30s grace window starts here.
      await first.page
        .evaluate(() => (window as any).krausenRefresh?.() ?? Promise.resolve(false))
        .catch(() => false);
      await first.page.waitForTimeout(1_000);

      const rotatedRefresh = (await first.context.cookies()).find(
        (cookie) => cookie.name === 'refresh_token'
      )?.value;
      expect(rotatedRefresh).toBeTruthy();
      expect(rotatedRefresh).not.toBe(oldRefresh);
      test.info().annotations.push({
        type: 'auth-family-a',
        description: familyLabel(familyA),
      });

      // One bounded wait measured from the confirmed rotation: the product's
      // documented rotation grace is 30s. 35s is deliberately past the
      // boundary, not an arbitrary retry delay.
      await new Promise((resolve) => setTimeout(resolve, 35_000));

      const replayClient = await request.newContext({ baseURL });
      try {
        const replay = await replayClient.post('/api/v1/auth/refresh', {
          headers: { cookie: `refresh_token=${oldRefresh}` },
        });
        // Old token must be rejected after the documented grace window.
        expect(replay.status()).toBe(401);
        expect(await replay.text()).toContain('Refresh token replayed');
      } finally {
        await replayClient.dispose();
      }
      expect(credentialState(familyA)).toBe('revoked');

      // Family A is now revoked. The second context must be a genuinely
      // independent family in fresh mode; in the legacy static mode it loads
      // the exact same stale storageState family and is expected to fail.
      const second = await openAuthenticatedContext(browser, { mode });
      try {
        const secondRefresh = (await second.context.cookies()).find(
          (cookie) => cookie.name === 'refresh_token'
        )?.value;
        const familyB = familyOf(secondRefresh);
        test.info().annotations.push({
          type: 'auth-family-b',
          description: familyLabel(familyB),
        });
        if (mode === 'static') {
          // Static mode must reuse the exact stale storageState family.
          expect(familyB).toBe(familyA);
        } else {
          // Fresh mode must mint an independent family.
          expect(familyB).not.toBe(familyA);
        }
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
