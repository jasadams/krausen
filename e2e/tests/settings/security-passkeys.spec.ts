import { test, expect, type Page, type CDPSession } from '@playwright/test';
import {
  db,
  getDefaultUserId,
  gotoAuthenticated,
  isPersonalMode,
  walCheckpoint,
} from '../../helpers/test-helpers';

const SECURITY_URL = '/settings/security';

async function navigateToSecurity(page: Page) {
  await gotoAuthenticated(page, SECURITY_URL);
}

async function setupVirtualAuthenticator(page: Page): Promise<{
  client: CDPSession;
  authenticatorId: string;
}> {
  const client = await page.context().newCDPSession(page);
  await client.send('WebAuthn.enable');
  const authenticatorId = await addVirtualAuthenticator(client);
  return { client, authenticatorId };
}

async function addVirtualAuthenticator(client: CDPSession): Promise<string> {
  const { authenticatorId } = await client.send('WebAuthn.addVirtualAuthenticator', {
    options: {
      protocol: 'ctap2',
      transport: 'internal',
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
    },
  });
  return authenticatorId;
}

async function addPasskeyViaModal(page: Page, name: string): Promise<void> {
  const addBtn = page.getByRole('button', { name: /Add.*Passkey/ }).first();
  await addBtn.click();

  const modalTitle = page.locator('h2', { hasText: 'Add Passkey' });
  await expect(modalTitle).toBeVisible({ timeout: 5000 });

  const deviceNameInput = page.locator('input[placeholder*="MacBook"]');
  await deviceNameInput.fill(name);

  const modalSubmit = page.getByRole('button', { name: 'Add Passkey' }).last();
  await modalSubmit.click();

  await expect(page.getByText(/added successfully/i)).toBeVisible({ timeout: 15000 });
  await page.waitForTimeout(1000);
}

test.describe('TC-013: Security - Passkey Management', () => {
  // Each scenario starts from a clean passkey store so accumulated virtual
  // authenticators from an earlier test cannot change the expected row count.
  test.beforeEach(() => {
    const userId = getDefaultUserId();
    db.exec(`DELETE FROM user_auth_methods WHERE user_id = '${userId}' AND auth_type = 'webauthn'`);
    walCheckpoint();
  });

  test.beforeEach(async ({ browserName }) => {
    test.skip(browserName !== 'chromium', 'CDP virtual authenticator requires Chromium');
    // Personal mode has no WebAuthn support — the server RP origin (http://localhost:5173 default)
    // does not match the actual browser origin (http://localhost:8099).
    test.skip(isPersonalMode(), 'Personal mode disables passkeys — server RP origin mismatch with embedded SPA');
  });

  test('should show empty state when no passkeys exist', async ({ page }) => {
    const { client, authenticatorId } = await setupVirtualAuthenticator(page);

    try {
      await navigateToSecurity(page);

      await expect(page.getByText('Passkeys', { exact: true }).first()).toBeVisible();
      await expect(page.getByText('No passkeys registered yet')).toBeVisible({ timeout: 10000 });
    } finally {
      await client.send('WebAuthn.removeVirtualAuthenticator', { authenticatorId });
    }
  });

  test('should add a passkey via virtual authenticator', async ({ page }) => {
    const { client, authenticatorId } = await setupVirtualAuthenticator(page);

    try {
      await navigateToSecurity(page);

      const addBtn = page.getByRole('button', { name: /Add.*Passkey/ });
      await expect(addBtn).toBeVisible({ timeout: 10000 });

      await addPasskeyViaModal(page, 'Test Virtual Authenticator');

      await expect(page.getByText('Test Virtual Authenticator', { exact: true })).toBeVisible({ timeout: 10000 });
    } finally {
      await client.send('WebAuthn.removeVirtualAuthenticator', { authenticatorId });
    }
  });

  test('should rename a passkey', async ({ page }) => {
    const { client, authenticatorId } = await setupVirtualAuthenticator(page);

    try {
      await navigateToSecurity(page);

      const emptyState = page.getByText('No passkeys registered yet');
      const isEmpty = await emptyState.isVisible({ timeout: 5000 }).catch(() => false);

      if (isEmpty) {
        await addPasskeyViaModal(page, 'Passkey To Rename');
      }

      const pencilBtn = page.locator('span[title="Rename passkey"]').first();
      await expect(pencilBtn).toBeVisible({ timeout: 10000 });
      await pencilBtn.click();

      await expect(page.locator('h2', { hasText: 'Rename Passkey' })).toBeVisible({ timeout: 5000 });

      const renameInput = page.locator('input[placeholder*="iPhone"]');
      await renameInput.clear();
      await renameInput.fill('Renamed Virtual Device');

      await page.getByRole('button', { name: 'Save' }).click();

      await expect(page.getByText('Passkey renamed successfully')).toBeVisible({ timeout: 10000 });
      await expect(page.getByText('Renamed Virtual Device')).toBeVisible({ timeout: 10000 });
    } finally {
      await client.send('WebAuthn.removeVirtualAuthenticator', { authenticatorId });
    }
  });

  test('should show tip when only one passkey exists', async ({ page }) => {
    const { client, authenticatorId } = await setupVirtualAuthenticator(page);

    try {
      await navigateToSecurity(page);

      const emptyState = page.getByText('No passkeys registered yet');
      const isEmpty = await emptyState.isVisible({ timeout: 5000 }).catch(() => false);

      if (isEmpty) {
        await addPasskeyViaModal(page, 'Single Passkey');
      }

      await expect(page.getByText(/Add a second passkey/i)).toBeVisible({ timeout: 10000 });
    } finally {
      await client.send('WebAuthn.removeVirtualAuthenticator', { authenticatorId });
    }
  });

  test('should delete a passkey when more than one exists', async ({ page }) => {
    const { client, authenticatorId } = await setupVirtualAuthenticator(page);
    let activeAuthenticator: string | undefined = authenticatorId;

    try {
      await navigateToSecurity(page);

      const passkeyRows = page.locator('tbody tr');
      let rowCount = await passkeyRows.count();

      if (rowCount === 0) {
        await addPasskeyViaModal(page, 'Delete Test Passkey 1');
        // The existing credential is excluded on the first device; register
        // the second passkey with a distinct authenticator instead.
        await client.send('WebAuthn.removeVirtualAuthenticator', { authenticatorId: activeAuthenticator });
        activeAuthenticator = undefined;
        activeAuthenticator = await addVirtualAuthenticator(client);
        await addPasskeyViaModal(page, 'Delete Test Passkey 2');
        await expect(page.getByText('Delete Test Passkey 1', { exact: true })).toBeVisible();
        await expect(page.getByText('Delete Test Passkey 2', { exact: true })).toBeVisible();
      } else if (rowCount === 1) {
        await addPasskeyViaModal(page, 'Delete Test Passkey Extra');
      }

      const deleteBtn = page.locator('button[title="Delete passkey"]').first();
      await expect(deleteBtn).toBeVisible({ timeout: 10000 });
      await deleteBtn.click();

      await expect(page.getByText('Delete Passkey?')).toBeVisible({ timeout: 5000 });
      await page.getByRole('button', { name: 'Delete' }).click();

      await expect(page.getByText('Passkey deleted successfully')).toBeVisible({ timeout: 10000 });
    } finally {
      if (activeAuthenticator) {
        await client.send('WebAuthn.removeVirtualAuthenticator', { authenticatorId: activeAuthenticator });
      }
    }
  });

  test('should not show delete button when only one passkey exists', async ({ page }) => {
    const { client, authenticatorId } = await setupVirtualAuthenticator(page);

    try {
      await navigateToSecurity(page);

      const emptyState = page.getByText('No passkeys registered yet');
      const isEmpty = await emptyState.isVisible({ timeout: 5000 }).catch(() => false);

      if (isEmpty) {
        await addPasskeyViaModal(page, 'Solo Passkey');
      }

      const rows = page.locator('tbody tr');
      const count = await rows.count();

      if (count === 1) {
        await expect(page.locator('button[title="Delete passkey"]')).not.toBeVisible();
      }
    } finally {
      await client.send('WebAuthn.removeVirtualAuthenticator', { authenticatorId });
    }
  });
});
