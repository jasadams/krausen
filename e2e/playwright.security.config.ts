import { defineConfig, devices } from '@playwright/test';

const BASE_URL = process.env.BASE_URL ?? 'http://localhost:8099';

/**
 * Account/security/team acceptance runner.
 *
 * Deliberately has no shared storageState and no legacy global setup: every
 * authenticated context is created by the test through the supported login
 * path, so each context owns a fresh credential/token family. The existing
 * foundation recipe browser runner keeps its own config and coverage.
 */
export default defineConfig({
  testDir: './tests',
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: 0,
  workers: 1,
  reporter: [['line']],
  timeout: 120_000,
  expect: { timeout: 15_000 },
  use: {
    baseURL: BASE_URL,
    trace: 'off',
    screenshot: 'only-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
});
