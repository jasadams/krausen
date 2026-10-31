-- Archive metadata is retained separately from immutable recipe content.
CREATE TABLE recipe_archive_events (
    workspace_id TEXT NOT NULL,
    recipe_id TEXT NOT NULL,
    metadata_revision BIGINT NOT NULL,
    archived BOOLEAN NOT NULL,
    actor_id TEXT NOT NULL,
    recorded_at BIGINT NOT NULL,
    PRIMARY KEY (workspace_id, recipe_id, metadata_revision),
    FOREIGN KEY (workspace_id, recipe_id) REFERENCES recipes(workspace_id, recipe_id)
);
CREATE FUNCTION reject_recipe_archive_event_mutation() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'recipe archive history is immutable'; END $$;
CREATE TRIGGER recipe_archive_events_immutable BEFORE UPDATE OR DELETE ON recipe_archive_events FOR EACH ROW EXECUTE FUNCTION reject_recipe_archive_event_mutation();
