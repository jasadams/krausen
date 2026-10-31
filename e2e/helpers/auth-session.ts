import * as fs from 'fs';
import * as path from 'path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { DEFAULT_TEST_USER, loginUser } from './test-helpers';

/**
 * Harness auth mode.
 *
 * - `static`: the legacy shared `.auth-state.json` context. Kept only to
 *   reproduce the pre-fix harness regression.
 * - `fresh`: each context logs in through the supported login path and gets
 *   its own credential/token family. This is the acceptance harness mode.
 */
export type AuthMode = 'fresh' | 'static';

const AUTH_STATE_PATH = path.resolve(__dirname, '../.auth-state.json');

export function getAuthMode(): AuthMode {
  return process.env.E2E_AUTH_MODE === 'static' ? 'static' : 'fresh';
}

export interface OpenAuthenticatedOptions {
  mode?: AuthMode;
  email?: string;
  password?: string;
}

export interface OpenedSession {
  context: BrowserContext;
  page: Page;
}

/**
 * Open an authenticated browser context for one test.
 *
 * `fresh` performs a real login (supported password path) in a new context,
 * minting a new credential/token family. `static` intentionally reuses the
 * shared storageState file so the regression can show the old failure.
 */
export async function openAuthenticatedContext(
  browser: Browser,
  options: OpenAuthenticatedOptions = {}
): Promise<OpenedSession> {
  const mode = options.mode ?? getAuthMode();
  const baseURL = process.env.BASE_URL ?? 'http://localhost:8099';

  if (mode === 'static' && !fs.existsSync(AUTH_STATE_PATH)) {
    throw new Error(
      `Static auth mode requires ${AUTH_STATE_PATH}; run the legacy global setup first or use E2E_AUTH_MODE=fresh.`
    );
  }

  const context = await browser.newContext({
    baseURL,
    ...(mode === 'static' ? { storageState: AUTH_STATE_PATH } : {}),
  });
  const page = await context.newPage();

  if (mode === 'fresh') {
    await loginUser(
      page,
      options.email ?? DEFAULT_TEST_USER.email,
      options.password ?? DEFAULT_TEST_USER.password
    );
  }

  return { context, page };
}
