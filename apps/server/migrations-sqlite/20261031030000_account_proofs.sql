CREATE TABLE account_action_proofs (
 proof_id TEXT PRIMARY KEY,
 account_id TEXT NOT NULL REFERENCES users(user_id),
 credential_id TEXT NOT NULL REFERENCES auth_credentials(credential_id),
 action TEXT NOT NULL,
 target_id TEXT NOT NULL,
 workspace_id TEXT REFERENCES workspaces(workspace_id),
 expected_revision BIGINT,
 recipient_email TEXT NOT NULL,
 outbox_id TEXT,
 expires_at BIGINT NOT NULL,
 consumed_at BIGINT,
 completion_credential_id TEXT,
 result TEXT
);
ALTER TABLE users ADD COLUMN account_revision BIGINT NOT NULL DEFAULT 0;
ALTER TABLE auth_credentials ADD COLUMN revision BIGINT NOT NULL DEFAULT 0;
ALTER TABLE app_outbox ADD COLUMN delivery_attempts BIGINT NOT NULL DEFAULT 0;
ALTER TABLE app_outbox ADD COLUMN last_delivery_failed BIGINT NOT NULL DEFAULT 0;
CREATE TABLE account_password_request_bindings (
 actor_id TEXT NOT NULL REFERENCES users(user_id),
 operation TEXT NOT NULL,
 request_id TEXT NOT NULL,
 credential_id TEXT NOT NULL REFERENCES auth_credentials(credential_id),
 PRIMARY KEY (actor_id, operation, request_id)
);
