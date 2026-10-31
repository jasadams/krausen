-- Native revisions retain legacy rows and their source while backfill proceeds.
CREATE UNIQUE INDEX recipes_workspace_identity ON recipes(workspace_id, recipe_id);
ALTER TABLE recipes ADD COLUMN native_head_revision INTEGER NOT NULL DEFAULT 0;
ALTER TABLE recipes ADD COLUMN metadata_revision INTEGER NOT NULL DEFAULT 0;
ALTER TABLE recipes ADD COLUMN native_migration_error TEXT;

CREATE TRIGGER recipes_guard_legacy_document BEFORE UPDATE OF document ON recipes
WHEN (OLD.native_head_revision > 0 OR OLD.native_migration_error IS NOT NULL) AND NEW.document <> OLD.document
BEGIN SELECT RAISE(ABORT, 'native recipe cannot be edited as legacy document'); END;
CREATE TRIGGER recipes_guard_legacy_archive BEFORE UPDATE OF deleted_at ON recipes
WHEN (OLD.native_head_revision > 0 OR OLD.native_migration_error IS NOT NULL) AND NEW.deleted_at IS NOT OLD.deleted_at
     AND NEW.metadata_revision = OLD.metadata_revision
BEGIN SELECT RAISE(ABORT, 'native recipe archive requires revision guard'); END;

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
    restored_from_revision INTEGER CHECK (restored_from_revision IS NULL OR restored_from_revision < revision),
    PRIMARY KEY (workspace_id, recipe_id, revision),
    FOREIGN KEY (workspace_id, recipe_id) REFERENCES recipes(workspace_id, recipe_id),
    FOREIGN KEY (workspace_id, recipe_id, restored_from_revision)
        REFERENCES recipe_revisions(workspace_id, recipe_id, revision)
);

CREATE TABLE recipe_import_provenance (
    workspace_id TEXT NOT NULL,
    provenance_id TEXT NOT NULL,
    recipe_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    format TEXT NOT NULL,
    original_bytes BLOB,
    original_hash TEXT,
    source_record TEXT,
    mapping_version TEXT NOT NULL,
    diagnostics TEXT NOT NULL,
    PRIMARY KEY (workspace_id, recipe_id, revision),
    UNIQUE (workspace_id, provenance_id),
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
