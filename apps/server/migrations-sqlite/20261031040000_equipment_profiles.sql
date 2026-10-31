-- Workspace-owned equipment identity and immutable settings revisions.
CREATE TABLE equipment_profiles (
    workspace_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    name TEXT NOT NULL,
    current_revision INTEGER NOT NULL CHECK (current_revision > 0),
    metadata_revision INTEGER NOT NULL CHECK (metadata_revision > 0),
    archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0,1)),
    PRIMARY KEY (workspace_id, profile_id),
    FOREIGN KEY (workspace_id) REFERENCES workspaces(workspace_id)
);
CREATE UNIQUE INDEX equipment_profile_global_id ON equipment_profiles(profile_id);
CREATE TABLE equipment_revisions (
    workspace_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    name TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    settings_json TEXT NOT NULL,
    provenance_json TEXT NOT NULL,
    content_digest TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    recorded_at INTEGER NOT NULL,
    PRIMARY KEY (workspace_id,profile_id,revision),
    FOREIGN KEY (workspace_id,profile_id) REFERENCES equipment_profiles(workspace_id,profile_id)
);
CREATE TABLE equipment_archive_events (
    workspace_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    metadata_revision INTEGER NOT NULL,
    archived INTEGER NOT NULL CHECK (archived IN (0,1)),
    actor_id TEXT NOT NULL,
    recorded_at INTEGER NOT NULL,
    PRIMARY KEY (workspace_id,profile_id,metadata_revision),
    FOREIGN KEY (workspace_id,profile_id) REFERENCES equipment_profiles(workspace_id,profile_id)
);
CREATE TRIGGER equipment_revisions_no_update BEFORE UPDATE ON equipment_revisions BEGIN SELECT RAISE(ABORT,'equipment revisions are immutable'); END;
CREATE TRIGGER equipment_revisions_no_delete BEFORE DELETE ON equipment_revisions BEGIN SELECT RAISE(ABORT,'equipment revisions are immutable'); END;
CREATE TRIGGER equipment_archive_events_no_update BEFORE UPDATE ON equipment_archive_events BEGIN SELECT RAISE(ABORT,'equipment archive events are immutable'); END;
CREATE TRIGGER equipment_archive_events_no_delete BEFORE DELETE ON equipment_archive_events BEGIN SELECT RAISE(ABORT,'equipment archive events are immutable'); END;
