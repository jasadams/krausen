-- Keep the workspace and parent in every revision/provenance reference.
CREATE UNIQUE INDEX recipes_workspace_identity ON recipes(workspace_id, recipe_id);
ALTER TABLE recipes ADD COLUMN native_head_revision BIGINT NOT NULL DEFAULT 0;
ALTER TABLE recipes ADD COLUMN metadata_revision BIGINT NOT NULL DEFAULT 0;

CREATE TABLE recipe_revisions (
    workspace_id TEXT NOT NULL,
    recipe_id TEXT NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    schema_version INTEGER NOT NULL,
    document TEXT NOT NULL,
    content_digest TEXT NOT NULL,
    saved_by TEXT,
    saved_at TIMESTAMPTZ,
    migrated_at TIMESTAMPTZ,
    summary TEXT,
    PRIMARY KEY (workspace_id, recipe_id, revision),
    FOREIGN KEY (workspace_id, recipe_id) REFERENCES recipes(workspace_id, recipe_id)
);

CREATE TABLE recipe_import_provenance (
    workspace_id TEXT NOT NULL,
    recipe_id TEXT NOT NULL,
    revision BIGINT NOT NULL,
    format TEXT NOT NULL,
    original_bytes BYTEA,
    original_hash TEXT,
    source_record TEXT,
    mapping_version TEXT NOT NULL,
    diagnostics TEXT NOT NULL,
    PRIMARY KEY (workspace_id, recipe_id, revision),
    FOREIGN KEY (workspace_id, recipe_id, revision)
        REFERENCES recipe_revisions(workspace_id, recipe_id, revision)
);

CREATE FUNCTION reject_recipe_history_mutation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'recipe history is immutable';
END;
$$;
CREATE TRIGGER recipe_revisions_immutable BEFORE UPDATE OR DELETE ON recipe_revisions
    FOR EACH ROW EXECUTE FUNCTION reject_recipe_history_mutation();
CREATE TRIGGER recipe_provenance_immutable BEFORE UPDATE OR DELETE ON recipe_import_provenance
    FOR EACH ROW EXECUTE FUNCTION reject_recipe_history_mutation();
