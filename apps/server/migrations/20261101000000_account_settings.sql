ALTER TABLE users ADD COLUMN settings_revision BIGINT NOT NULL DEFAULT 0;
CREATE TABLE account_settings_changes (
    change_id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES users(user_id),
    actor_id TEXT NOT NULL REFERENCES users(user_id),
    operation TEXT NOT NULL,
    revision BIGINT NOT NULL,
    recorded_at BIGINT NOT NULL,
    UNIQUE(account_id, revision)
);
