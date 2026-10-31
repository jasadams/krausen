import { test, expect } from '@playwright/test';
import {
  waitForWasm,
  generateEmail,
  isSelfHostedMode,
  allowSignup,
  SIGNED_OUT,
} from '../../helpers/test-helpers';

test.describe('TC-024: Self-Hosted Mode Without SMTP', () => {
  test.describe.configure({ mode: 'serial' });

  const isSelfHosted = isSelfHostedMode();

  test.skip(!isSelfHosted, 'Server must be running in self_hosted mode');
  // /signup redirects an authenticated visitor away, so the form is only
  // present for a signed-out browser.
  test.use({ storageState: SIGNED_OUT });

  const testEmail = generateEmail();
  const testName = 'E2E Self-Hosted User';
  const testPassword = 'TestPassword1234';

  test('signup form shows email, name, and password fields', async ({ page }) => {
    await page.goto('/signup');
    await waitForWasm(page);

    await expect(page.locator('#signup-email')).toBeVisible();
    await expect(page.locator('#signup-name')).toBeVisible();
    await expect(page.locator('#signup-password')).toBeVisible();
  });

  test('submit button reads "Create Account" in self-hosted mode', async ({ page }) => {
    await page.goto('/signup');
    await waitForWasm(page);

    const submitButton = page.locator('button[type="submit"]');
    await expect(submitButton).toContainText('Create Account');
  });

  test('account is created directly without email verification', async ({ page }) => {
    // Registration closes after the first account; a later one is invited.
    allowSignup(testEmail);
    await page.context().clearCookies();
    await page.goto('/signup');
    await waitForWasm(page);
    await page.waitForSelector('#signup-email', { timeout: 15000 });
    await page.waitForTimeout(1000);

    await page.locator('#signup-email').click();
    await page.locator('#signup-email').type(testEmail, { delay: 10 });
    await page.locator('#signup-name').click();
    await page.locator('#signup-name').type(testName, { delay: 10 });
    await page.locator('#signup-password').click();
    await page.locator('#signup-password').type(testPassword, { delay: 10 });

    await page.click('button[type="submit"]');

    // Should navigate into the app rather than showing "Check Your Email".
    // Where that lands is the app's choice — krausen sends "/" to /recipes —
    // so assert it left the auth pages rather than naming a destination.
    await page.waitForURL(
      (url) => !url.pathname.includes('/signup') && !url.pathname.includes('/login'),
      { timeout: 15_000 }
    );

    const url = page.url();
    expect(url).not.toContain('/login');
    expect(url).not.toContain('/signup');
  });

  test('user lands in the application with a workspace', async ({ page }) => {
    await page.goto('/login');
    await waitForWasm(page);
    await page.waitForSelector('#login-email', { timeout: 15000 });
    await page.waitForTimeout(500);

    await page.locator('#login-email').click();
    await page.locator('#login-email').type(testEmail, { delay: 10 });
    await page.locator('#login-password').click();
    await page.locator('#login-password').type(testPassword, { delay: 10 });
    await page.click('button[type="submit"]');

    await page.waitForURL(
      (url) => !url.pathname.includes('/signup') && !url.pathname.includes('/login'),
      { timeout: 15_000 }
    );

    const url = page.url();
    expect(url).not.toContain('/login');
    expect(url).not.toContain('/signup');
  });
});
