-- Measurement, actual-formulation, note, selection, lifecycle and archive
-- evidence for started brews. The immutable plan remains owned by KRA-20.
ALTER TABLE brews ADD COLUMN actual_revision INTEGER CHECK (actual_revision IS NULL OR actual_revision > 0);
ALTER TABLE brews ADD COLUMN metadata_revision INTEGER NOT NULL DEFAULT 1 CHECK (metadata_revision > 0);
ALTER TABLE brews ADD COLUMN archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0,1));

CREATE INDEX brews_workspace_archive_started
    ON brews(workspace_id, archived, started_at, brew_id);

CREATE TABLE brew_actual_formulations (
    workspace_id TEXT NOT NULL,
    brew_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    predecessor_revision INTEGER,
    schema_version INTEGER NOT NULL,
    document_json TEXT NOT NULL,
    content_digest TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    recorded_at INTEGER NOT NULL,
    reason TEXT,
    PRIMARY KEY (workspace_id, brew_id, revision),
    FOREIGN KEY (workspace_id, brew_id) REFERENCES brews(workspace_id, brew_id),
    FOREIGN KEY (workspace_id, brew_id, predecessor_revision)
        REFERENCES brew_actual_formulations(workspace_id, brew_id, revision)
);
CREATE UNIQUE INDEX brew_actual_one_successor
    ON brew_actual_formulations(workspace_id, brew_id, predecessor_revision)
    WHERE predecessor_revision IS NOT NULL;

CREATE TABLE brew_measurement_entries (
    workspace_id TEXT NOT NULL,
    brew_id TEXT NOT NULL,
    entry_id TEXT NOT NULL,
    quantity_id TEXT NOT NULL,
    unit TEXT NOT NULL,
    basis TEXT NOT NULL,
    stage TEXT NOT NULL,
    value REAL,
    observed_at INTEGER NOT NULL,
    time_source TEXT NOT NULL CHECK (time_source IN ('recorded_default', 'supplied')),
    recorded_at INTEGER NOT NULL,
    actor_id TEXT NOT NULL,
    source TEXT NOT NULL,
    root_observation_id TEXT NOT NULL,
    supersedes_entry_id TEXT,
    schema_version INTEGER NOT NULL,
    metadata_json TEXT NOT NULL,
    extensions_json TEXT NOT NULL,
    PRIMARY KEY (workspace_id, brew_id, entry_id),
    FOREIGN KEY (workspace_id, brew_id) REFERENCES brews(workspace_id, brew_id),
    FOREIGN KEY (workspace_id, brew_id, root_observation_id)
        REFERENCES brew_measurement_entries(workspace_id, brew_id, entry_id),
    FOREIGN KEY (workspace_id, brew_id, supersedes_entry_id)
        REFERENCES brew_measurement_entries(workspace_id, brew_id, entry_id)
);
CREATE UNIQUE INDEX brew_measurement_one_successor
    ON brew_measurement_entries(workspace_id, brew_id, supersedes_entry_id)
    WHERE supersedes_entry_id IS NOT NULL;
CREATE INDEX brew_measurement_quantity_history
    ON brew_measurement_entries(workspace_id, brew_id, quantity_id, recorded_at, entry_id);

CREATE TABLE brew_notes (
    workspace_id TEXT NOT NULL,
    brew_id TEXT NOT NULL,
    note_id TEXT NOT NULL,
    current_revision INTEGER NOT NULL CHECK (current_revision > 0),
    created_at INTEGER NOT NULL,
    created_by TEXT NOT NULL,
    PRIMARY KEY (workspace_id, brew_id, note_id),
    FOREIGN KEY (workspace_id, brew_id) REFERENCES brews(workspace_id, brew_id)
);

CREATE TABLE brew_note_revisions (
    workspace_id TEXT NOT NULL,
    brew_id TEXT NOT NULL,
    note_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    schema_version INTEGER NOT NULL,
    document_json TEXT NOT NULL,
    content_digest TEXT NOT NULL,
    supersedes_revision INTEGER,
    actor_id TEXT NOT NULL,
    recorded_at INTEGER NOT NULL,
    reason TEXT,
    PRIMARY KEY (workspace_id, brew_id, note_id, revision),
    FOREIGN KEY (workspace_id, brew_id, note_id)
        REFERENCES brew_notes(workspace_id, brew_id, note_id),
    FOREIGN KEY (workspace_id, brew_id, note_id, supersedes_revision)
        REFERENCES brew_note_revisions(workspace_id, brew_id, note_id, revision)
);
CREATE UNIQUE INDEX brew_note_one_successor
    ON brew_note_revisions(workspace_id, brew_id, note_id, supersedes_revision)
    WHERE supersedes_revision IS NOT NULL;

CREATE TABLE brew_actual_selections (
    workspace_id TEXT NOT NULL,
    brew_id TEXT NOT NULL,
    quantity_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    selected_root_observation_id TEXT,
    updated_at INTEGER NOT NULL,
    updated_by TEXT NOT NULL,
    PRIMARY KEY (workspace_id, brew_id, quantity_id),
    FOREIGN KEY (workspace_id, brew_id) REFERENCES brews(workspace_id, brew_id),
    FOREIGN KEY (workspace_id, brew_id, selected_root_observation_id)
        REFERENCES brew_measurement_entries(workspace_id, brew_id, entry_id)
);

CREATE TABLE brew_actual_selection_events (
    workspace_id TEXT NOT NULL,
    brew_id TEXT NOT NULL,
    quantity_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    selected_root_observation_id TEXT,
    actor_id TEXT NOT NULL,
    recorded_at INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, brew_id, quantity_id, revision),
    FOREIGN KEY (workspace_id, brew_id, quantity_id)
        REFERENCES brew_actual_selections(workspace_id, brew_id, quantity_id),
    FOREIGN KEY (workspace_id, brew_id, selected_root_observation_id)
        REFERENCES brew_measurement_entries(workspace_id, brew_id, entry_id)
);

CREATE TABLE brew_lifecycle_events (
    workspace_id TEXT NOT NULL,
    brew_id TEXT NOT NULL,
    brew_revision INTEGER NOT NULL CHECK (brew_revision > 0),
    from_status TEXT NOT NULL CHECK (from_status IN ('started', 'completed', 'cancelled')),
    to_status TEXT NOT NULL CHECK (to_status IN ('started', 'completed', 'cancelled')),
    reason TEXT,
    actor_id TEXT NOT NULL,
    recorded_at INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, brew_id, brew_revision),
    FOREIGN KEY (workspace_id, brew_id) REFERENCES brews(workspace_id, brew_id)
);

CREATE TABLE brew_archive_events (
    workspace_id TEXT NOT NULL,
    brew_id TEXT NOT NULL,
    metadata_revision INTEGER NOT NULL CHECK (metadata_revision > 0),
    archived INTEGER NOT NULL CHECK (archived IN (0,1)),
    actor_id TEXT NOT NULL,
    recorded_at INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, brew_id, metadata_revision),
    FOREIGN KEY (workspace_id, brew_id) REFERENCES brews(workspace_id, brew_id)
);

CREATE TRIGGER brew_actual_formulations_no_update
BEFORE UPDATE ON brew_actual_formulations
BEGIN
    SELECT RAISE(ABORT, 'brew evidence is immutable');
END;
CREATE TRIGGER brew_actual_formulations_no_delete
BEFORE DELETE ON brew_actual_formulations
BEGIN
    SELECT RAISE(ABORT, 'brew evidence is immutable');
END;

CREATE TRIGGER brew_measurement_entries_no_update
BEFORE UPDATE ON brew_measurement_entries
BEGIN
    SELECT RAISE(ABORT, 'brew evidence is immutable');
END;
CREATE TRIGGER brew_measurement_entries_no_delete
BEFORE DELETE ON brew_measurement_entries
BEGIN
    SELECT RAISE(ABORT, 'brew evidence is immutable');
END;

CREATE TRIGGER brew_note_revisions_no_update
BEFORE UPDATE ON brew_note_revisions
BEGIN
    SELECT RAISE(ABORT, 'brew evidence is immutable');
END;
CREATE TRIGGER brew_note_revisions_no_delete
BEFORE DELETE ON brew_note_revisions
BEGIN
    SELECT RAISE(ABORT, 'brew evidence is immutable');
END;

CREATE TRIGGER brew_actual_selection_events_no_update
BEFORE UPDATE ON brew_actual_selection_events
BEGIN
    SELECT RAISE(ABORT, 'brew evidence is immutable');
END;
CREATE TRIGGER brew_actual_selection_events_no_delete
BEFORE DELETE ON brew_actual_selection_events
BEGIN
    SELECT RAISE(ABORT, 'brew evidence is immutable');
END;

CREATE TRIGGER brew_lifecycle_events_no_update
BEFORE UPDATE ON brew_lifecycle_events
BEGIN
    SELECT RAISE(ABORT, 'brew evidence is immutable');
END;
CREATE TRIGGER brew_lifecycle_events_no_delete
BEFORE DELETE ON brew_lifecycle_events
BEGIN
    SELECT RAISE(ABORT, 'brew evidence is immutable');
END;

CREATE TRIGGER brew_archive_events_no_update
BEFORE UPDATE ON brew_archive_events
BEGIN
    SELECT RAISE(ABORT, 'brew evidence is immutable');
END;
CREATE TRIGGER brew_archive_events_no_delete
BEFORE DELETE ON brew_archive_events
BEGIN
    SELECT RAISE(ABORT, 'brew evidence is immutable');
END;
