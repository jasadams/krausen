import type { Page } from '@playwright/test';
import {
  db,
  generateUserId,
  generateWorkspaceId,
  loginUser,
  nowISO,
  sqlText,
} from './test-helpers';

export interface DisposableUser {
  userId: string;
  workspaceId: string;
  email: string;
  password: string;
}

export interface DisposableUserOptions {
  email?: string;
  password?: string;
  name?: string;
  workspaceName?: string;
}

/**
 * Provision a disposable account/workspace in the already-disposable test DB.
 *
 * This seeds only fixture records: user, workspace, membership and a password
 * verifier produced with the application's argon2 hashing helper. All sessions
 * and tokens are still minted by the supported login path; no cookie/token is
 * injected and no auth check is bypassed.
 */
export async function createDisposableUser(
  options: DisposableUserOptions = {}
): Promise<DisposableUser> {
  const argon2 = require('argon2');
  const suffix = Math.random().toString(36).slice(2, 10);
  const userId = generateUserId();
  const workspaceId = generateWorkspaceId();
  const email = options.email ?? `krausen-e2e-${suffix}@example.test`;
  const password = options.password ?? 'DisposablePassword1234!';
  const name = options.name ?? 'Disposable E2E User';
  const workspaceName = options.workspaceName ?? `Disposable Workspace ${suffix}`;
  const now = nowISO();

  db.exec(
    `INSERT INTO users (user_id, email, name, verified, active, created_at, updated_at) VALUES ('${userId}', ${sqlText(email)}, ${sqlText(name)}, 1, 1, '${now}', '${now}')`
  );
  db.exec(
    `INSERT INTO workspaces (workspace_id, name, status, owner_user_id, created_at, updated_at) VALUES ('${workspaceId}', ${sqlText(workspaceName)}, 'active', '${userId}', '${now}', '${now}')`
  );
  db.exec(
    `INSERT INTO workspace_users (workspace_id, user_id, role, active, created_at) VALUES ('${workspaceId}', '${userId}', 'workspace_admin', 1, '${now}')`
  );

  const hash = await argon2.hash(password, { type: argon2.argon2id });
  const authData = JSON.stringify({ hash }).replace(/'/g, "''");
  db.exec(
    `INSERT INTO user_auth_methods (user_id, auth_type, auth_data, active) VALUES ('${userId}', 'password', '${authData}', 1)`
  );

  return { userId, workspaceId, email, password };
}

/** Log in a disposable user through the supported login path. */
export async function loginDisposableUser(page: Page, user: DisposableUser): Promise<void> {
  await loginUser(page, user.email, user.password);
}

/**
 * Best-effort cleanup of fixture-owned rows. The browser suite always runs on
 * a disposable database, so failure to remove a row is not a product result.
 */
export function deleteDisposableUser(user: DisposableUser): void {
  try {
    db.exec(`DELETE FROM refresh_tokens WHERE user_id = '${user.userId}'`);
    db.exec(`DELETE FROM auth_credentials WHERE user_id = '${user.userId}'`);
    db.exec(`DELETE FROM user_auth_methods WHERE user_id = '${user.userId}'`);
    db.exec(`DELETE FROM workspace_users WHERE workspace_id = '${user.workspaceId}'`);
    db.exec(`DELETE FROM workspaces WHERE workspace_id = '${user.workspaceId}'`);
    db.exec(`DELETE FROM users WHERE user_id = '${user.userId}'`);
  } catch {
    // Disposable fixture: the runner removes the entire database afterwards.
  }
}
