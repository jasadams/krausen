-- Archive metadata is retained separately from immutable recipe content.
CREATE TABLE recipe_archive_events (
    workspace_id TEXT NOT NULL,
    recipe_id TEXT NOT NULL,
    metadata_revision INTEGER NOT NULL,
    archived INTEGER NOT NULL CHECK (archived IN (0, 1)),
    actor_id TEXT NOT NULL,
    recorded_at INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, recipe_id, metadata_revision),
    FOREIGN KEY (workspace_id, recipe_id) REFERENCES recipes(workspace_id, recipe_id)
);
CREATE TRIGGER recipe_archive_events_no_update BEFORE UPDATE ON recipe_archive_events BEGIN SELECT RAISE(ABORT, 'recipe archive history is immutable'); END;
CREATE TRIGGER recipe_archive_events_no_delete BEFORE DELETE ON recipe_archive_events BEGIN SELECT RAISE(ABORT, 'recipe archive history is immutable'); END;
