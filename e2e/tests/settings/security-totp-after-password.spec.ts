import { test, expect } from '@playwright/test';
import { authenticator } from 'otplib';
import { openAuthenticatedContext } from '../../helpers/auth-session';
import {
  createDisposableUser,
  deleteDisposableUser,
} from '../../helpers/disposable-user';
import { db, isPersonalMode, walCheckpoint } from '../../helpers/test-helpers';

const SECURITY_URL = '/settings/security';
const PASSWORD = 'SamePageTotpPassword1!';

test.describe('TC-012a: TOTP after password change on the same page', () => {
  test.skip(isPersonalMode(), 'Requires a password-authenticated self_hosted user');

  test('set-password then TOTP setup/enable without reloading', async ({ browser }) => {
    const user = await createDisposableUser({
      password: 'DisposableStart1234!',
      name: 'Same-page TOTP user',
    });
    const session = await openAuthenticatedContext(browser, {
      mode: 'fresh',
      email: user.email,
      password: user.password,
    });

    try {
      // Simulate an authenticated account that has no password yet. The
      // browser session remains valid; only the password method is removed.
      db.exec(
        `DELETE FROM user_auth_methods WHERE user_id = '${user.userId}' AND auth_type IN ('password', 'totp')`
      );
      walCheckpoint();

      await session.page.goto(SECURITY_URL);
      await session.page.waitForLoadState('networkidle');

      const setPassword = session.page.getByRole('button', { name: 'Set Password' });
      await expect(setPassword).toBeVisible({ timeout: 10_000 });
      await setPassword.click();

      await session.page.locator('#newPassword').fill(PASSWORD);
      await session.page.locator('#confirmPassword').fill(PASSWORD);
      await session.page.getByRole('button', { name: 'Set Password' }).click();
      await expect(session.page.getByText('Password set successfully')).toBeVisible({
        timeout: 10_000,
      });

      // Deliberately no reload: the TOTP card still holds the pre-password
      // security revision. A new setup command must re-read current authority
      // rather than submit the stale revision or silently retry a conflict.
      await session.page.locator('button:has-text("Setup 2FA")').click();
      await expect(
        session.page.getByText('Setup Two-Factor Authentication')
      ).toBeVisible({ timeout: 10_000 });
      await expect(session.page.locator('img[alt="2FA QR Code"]')).toBeVisible();

      const secretInput = session.page.locator('input[readonly]');
      await expect(secretInput).toBeVisible({ timeout: 10_000 });
      const secret = await secretInput.inputValue();
      expect(secret.length).toBeGreaterThanOrEqual(16);

      const code = authenticator.generate(secret);
      await session.page.locator('#verification-code').fill(code);
      await session.page.getByRole('button', { name: 'Enable 2FA' }).click();

      await expect(session.page.getByText('2FA has been successfully enabled')).toBeVisible({
        timeout: 10_000,
      });
      await expect(session.page.getByRole('button', { name: 'Disable 2FA' })).toBeVisible();
    } finally {
      await session.context.close();
      deleteDisposableUser(user);
    }
  });
});
