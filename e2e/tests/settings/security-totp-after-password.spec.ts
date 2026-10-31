import { test, expect, type Page } from '@playwright/test';
import { authenticator } from 'otplib';
import {
  gotoAuthenticated,
  getDefaultUserId,
  db,
  walCheckpoint,
} from '../../helpers/test-helpers';

const SECURITY_URL = '/settings/security';
const PASSWORD = 'SamePageTotpPassword1!';

function clearPasswordAndTotp(userId: string) {
  db.exec(
    `DELETE FROM user_auth_methods WHERE user_id = '${userId}' AND auth_type IN ('password', 'totp')`
  );
  walCheckpoint();
}

async function restoreDefaultPassword(userId: string) {
  const argon2 = require('argon2');
  const hash = await argon2.hash('TestPassword1234', { type: argon2.argon2id });
  const authData = JSON.stringify({ hash }).replace(/'/g, "''");
  db.exec(
    `DELETE FROM user_auth_methods WHERE user_id = '${userId}' AND auth_type IN ('password', 'totp')`
  );
  db.exec(
    `INSERT INTO user_auth_methods (user_id, auth_type, auth_data, active) VALUES ('${userId}', 'password', '${authData}', 1)`
  );
  walCheckpoint();
}

async function navigateToSecurity(page: Page) {
  await gotoAuthenticated(page, SECURITY_URL);
}

test.describe('TC-012a: TOTP after password change on the same page', () => {
  test.beforeAll(() => {
    clearPasswordAndTotp(getDefaultUserId());
  });

  test.afterAll(async () => {
    try {
      await restoreDefaultPassword(getDefaultUserId());
    } catch {}
  });

  test('set-password then TOTP setup/enable without reloading', async ({ page }) => {
    await navigateToSecurity(page);

    // The bootstrap owner starts without a password for this test.
    const setPassword = page.getByRole('button', { name: 'Set Password' });
    await expect(setPassword).toBeVisible({ timeout: 10_000 });
    await setPassword.click();

    await page.locator('#newPassword').fill(PASSWORD);
    await page.locator('#confirmPassword').fill(PASSWORD);
    await page.getByRole('button', { name: 'Set Password' }).click();
    await expect(page.getByText('Password set successfully')).toBeVisible({
      timeout: 10_000,
    });

    // Deliberately no reload: the TOTP card still holds the pre-password
    // security revision. A new setup command must re-read current authority
    // rather than submit the stale revision or silently retry a conflict.
    await page.locator('button:has-text("Setup 2FA")').click();
    await expect(page.getByText('Setup Two-Factor Authentication')).toBeVisible({
      timeout: 10_000,
    });
    await expect(page.locator('img[alt="2FA QR Code"]')).toBeVisible();

    const secretInput = page.locator('input[readonly]');
    await expect(secretInput).toBeVisible({ timeout: 10_000 });
    const secret = await secretInput.inputValue();
    expect(secret.length).toBeGreaterThanOrEqual(16);

    const code = authenticator.generate(secret);
    await page.locator('#verification-code').fill(code);
    await page.getByRole('button', { name: 'Enable 2FA' }).click();

    await expect(page.getByText('2FA has been successfully enabled')).toBeVisible({
      timeout: 10_000,
    });
    await expect(page.getByRole('button', { name: 'Disable 2FA' })).toBeVisible();
  });
});
