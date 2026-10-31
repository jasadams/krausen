CREATE TABLE account_action_proofs (
 proof_id TEXT PRIMARY KEY,
 account_id TEXT NOT NULL REFERENCES users(user_id),
 credential_id TEXT NOT NULL REFERENCES auth_credentials(credential_id),
 action TEXT NOT NULL,
 target_id TEXT NOT NULL,
 workspace_id TEXT REFERENCES workspaces(workspace_id),
 recipient_email TEXT NOT NULL,
 outbox_id TEXT,
 expires_at BIGINT NOT NULL,
 consumed_at BIGINT,
 completion_credential_id TEXT,
 result TEXT
);
