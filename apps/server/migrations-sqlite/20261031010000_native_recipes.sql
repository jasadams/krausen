-- Native revisions retain legacy rows and their source while backfill proceeds.
CREATE UNIQUE INDEX recipes_workspace_identity ON recipes(workspace_id, recipe_id);
ALTER TABLE recipes ADD COLUMN native_head_revision INTEGER NOT NULL DEFAULT 0;
ALTER TABLE recipes ADD COLUMN metadata_revision INTEGER NOT NULL DEFAULT 0;

CREATE TABLE recipe_revisions (
    workspace_id TEXT NOT NULL,
    recipe_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    schema_version INTEGER NOT NULL,
    document TEXT NOT NULL,
    content_digest TEXT NOT NULL,
    saved_by TEXT,
    saved_at TEXT,
    migrated_at TEXT,
    summary TEXT,
    PRIMARY KEY (workspace_id, recipe_id, revision),
    FOREIGN KEY (workspace_id, recipe_id) REFERENCES recipes(workspace_id, recipe_id)
);

CREATE TABLE recipe_import_provenance (
    workspace_id TEXT NOT NULL,
    recipe_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    format TEXT NOT NULL,
    original_bytes BLOB,
    original_hash TEXT,
    source_record TEXT,
    mapping_version TEXT NOT NULL,
    diagnostics TEXT NOT NULL,
    PRIMARY KEY (workspace_id, recipe_id, revision),
    FOREIGN KEY (workspace_id, recipe_id, revision)
        REFERENCES recipe_revisions(workspace_id, recipe_id, revision)
);

CREATE TRIGGER recipe_revisions_no_update BEFORE UPDATE ON recipe_revisions
BEGIN SELECT RAISE(ABORT, 'recipe revisions are immutable'); END;
CREATE TRIGGER recipe_revisions_no_delete BEFORE DELETE ON recipe_revisions
BEGIN SELECT RAISE(ABORT, 'recipe revisions are immutable'); END;
CREATE TRIGGER recipe_provenance_no_update BEFORE UPDATE ON recipe_import_provenance
BEGIN SELECT RAISE(ABORT, 'import provenance is immutable'); END;
CREATE TRIGGER recipe_provenance_no_delete BEFORE DELETE ON recipe_import_provenance
BEGIN SELECT RAISE(ABORT, 'import provenance is immutable'); END;
