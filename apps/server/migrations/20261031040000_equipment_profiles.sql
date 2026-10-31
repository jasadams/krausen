-- Workspace-owned equipment identity and immutable settings revisions.
CREATE TABLE equipment_profiles (
    workspace_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    name TEXT NOT NULL,
    current_revision BIGINT NOT NULL CHECK (current_revision > 0),
    metadata_revision BIGINT NOT NULL CHECK (metadata_revision > 0),
    archived BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (workspace_id, profile_id),
    FOREIGN KEY (workspace_id) REFERENCES workspaces(workspace_id)
);
CREATE UNIQUE INDEX equipment_profile_global_id ON equipment_profiles(profile_id);
CREATE TABLE equipment_revisions (
    workspace_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    name TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    settings_json TEXT NOT NULL,
    provenance_json TEXT NOT NULL,
    content_digest TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    recorded_at BIGINT NOT NULL,
    PRIMARY KEY (workspace_id,profile_id,revision),
    FOREIGN KEY (workspace_id,profile_id) REFERENCES equipment_profiles(workspace_id,profile_id)
);
CREATE TABLE equipment_archive_events (
    workspace_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    metadata_revision BIGINT NOT NULL,
    archived BOOLEAN NOT NULL,
    actor_id TEXT NOT NULL,
    recorded_at BIGINT NOT NULL,
    PRIMARY KEY (workspace_id,profile_id,metadata_revision),
    FOREIGN KEY (workspace_id,profile_id) REFERENCES equipment_profiles(workspace_id,profile_id)
);
CREATE FUNCTION equipment_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'equipment history is immutable'; END $$;
CREATE TRIGGER equipment_revisions_no_change BEFORE UPDATE OR DELETE ON equipment_revisions FOR EACH ROW EXECUTE FUNCTION equipment_immutable();
CREATE TRIGGER equipment_archive_events_no_change BEFORE UPDATE OR DELETE ON equipment_archive_events FOR EACH ROW EXECUTE FUNCTION equipment_immutable();
