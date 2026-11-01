ALTER TABLE users ADD COLUMN settings_revision INTEGER NOT NULL DEFAULT 0;
CREATE TABLE account_settings_changes (
    change_id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES users(user_id),
    actor_id TEXT NOT NULL REFERENCES users(user_id),
    operation TEXT NOT NULL,
    revision INTEGER NOT NULL,
    recorded_at INTEGER NOT NULL,
    UNIQUE(account_id, revision)
);
