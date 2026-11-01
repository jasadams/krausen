-- Measurement, actual-formulation, note, selection, lifecycle and archive
-- evidence for started brews. The immutable plan remains owned by KRA-20.
ALTER TABLE brews ADD COLUMN actual_revision BIGINT CHECK (actual_revision IS NULL OR actual_revision > 0);
ALTER TABLE brews ADD COLUMN metadata_revision BIGINT NOT NULL DEFAULT 1 CHECK (metadata_revision > 0);
ALTER TABLE brews ADD COLUMN archived BOOLEAN NOT NULL DEFAULT FALSE;

CREATE INDEX brews_workspace_archive_started
    ON brews(workspace_id, archived, started_at, brew_id);

CREATE TABLE brew_actual_formulations (
    workspace_id TEXT NOT NULL,
    brew_id TEXT NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    predecessor_revision BIGINT,
    schema_version INTEGER NOT NULL,
    document_json TEXT NOT NULL,
    content_digest TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    recorded_at BIGINT NOT NULL,
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
    value DOUBLE PRECISION,
    observed_at BIGINT NOT NULL,
    time_source TEXT NOT NULL CHECK (time_source IN ('recorded_default', 'supplied')),
    recorded_at BIGINT NOT NULL,
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
    current_revision BIGINT NOT NULL CHECK (current_revision > 0),
    created_at BIGINT NOT NULL,
    created_by TEXT NOT NULL,
    PRIMARY KEY (workspace_id, brew_id, note_id),
    FOREIGN KEY (workspace_id, brew_id) REFERENCES brews(workspace_id, brew_id)
);

CREATE TABLE brew_note_revisions (
    workspace_id TEXT NOT NULL,
    brew_id TEXT NOT NULL,
    note_id TEXT NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    schema_version INTEGER NOT NULL,
    document_json TEXT NOT NULL,
    content_digest TEXT NOT NULL,
    supersedes_revision BIGINT,
    actor_id TEXT NOT NULL,
    recorded_at BIGINT NOT NULL,
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
    revision BIGINT NOT NULL CHECK (revision > 0),
    selected_root_observation_id TEXT,
    updated_at BIGINT NOT NULL,
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
    revision BIGINT NOT NULL CHECK (revision > 0),
    selected_root_observation_id TEXT,
    actor_id TEXT NOT NULL,
    recorded_at BIGINT NOT NULL,
    PRIMARY KEY (workspace_id, brew_id, quantity_id, revision),
    FOREIGN KEY (workspace_id, brew_id, quantity_id)
        REFERENCES brew_actual_selections(workspace_id, brew_id, quantity_id),
    FOREIGN KEY (workspace_id, brew_id, selected_root_observation_id)
        REFERENCES brew_measurement_entries(workspace_id, brew_id, entry_id)
);

CREATE TABLE brew_lifecycle_events (
    workspace_id TEXT NOT NULL,
    brew_id TEXT NOT NULL,
    brew_revision BIGINT NOT NULL CHECK (brew_revision > 0),
    from_status TEXT NOT NULL CHECK (from_status IN ('started', 'completed', 'cancelled')),
    to_status TEXT NOT NULL CHECK (to_status IN ('started', 'completed', 'cancelled')),
    reason TEXT,
    actor_id TEXT NOT NULL,
    recorded_at BIGINT NOT NULL,
    PRIMARY KEY (workspace_id, brew_id, brew_revision),
    FOREIGN KEY (workspace_id, brew_id) REFERENCES brews(workspace_id, brew_id)
);

CREATE TABLE brew_archive_events (
    workspace_id TEXT NOT NULL,
    brew_id TEXT NOT NULL,
    metadata_revision BIGINT NOT NULL CHECK (metadata_revision > 0),
    archived BOOLEAN NOT NULL,
    actor_id TEXT NOT NULL,
    recorded_at BIGINT NOT NULL,
    PRIMARY KEY (workspace_id, brew_id, metadata_revision),
    FOREIGN KEY (workspace_id, brew_id) REFERENCES brews(workspace_id, brew_id)
);

CREATE FUNCTION brew_evidence_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'brew evidence is immutable';
END;
$$;
CREATE TRIGGER brew_actual_formulations_immutable BEFORE UPDATE OR DELETE ON brew_actual_formulations
    FOR EACH ROW EXECUTE FUNCTION brew_evidence_immutable();
CREATE TRIGGER brew_measurement_entries_immutable BEFORE UPDATE OR DELETE ON brew_measurement_entries
    FOR EACH ROW EXECUTE FUNCTION brew_evidence_immutable();
CREATE TRIGGER brew_note_revisions_immutable BEFORE UPDATE OR DELETE ON brew_note_revisions
    FOR EACH ROW EXECUTE FUNCTION brew_evidence_immutable();
CREATE TRIGGER brew_actual_selection_events_immutable BEFORE UPDATE OR DELETE ON brew_actual_selection_events
    FOR EACH ROW EXECUTE FUNCTION brew_evidence_immutable();
CREATE TRIGGER brew_lifecycle_events_immutable BEFORE UPDATE OR DELETE ON brew_lifecycle_events
    FOR EACH ROW EXECUTE FUNCTION brew_evidence_immutable();
CREATE TRIGGER brew_archive_events_immutable BEFORE UPDATE OR DELETE ON brew_archive_events
    FOR EACH ROW EXECUTE FUNCTION brew_evidence_immutable();
