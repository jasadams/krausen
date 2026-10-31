-- Public starter catalogue (explicit global-read exception with documented
-- provenance) and workspace-owned custom ingredient entries with immutable
-- versioned revisions and archive events.
CREATE TABLE starter_ingredient_entries (
    entry_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    producer TEXT NOT NULL DEFAULT '',
    category TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version > 0),
    archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0,1)),
    document_json TEXT NOT NULL,
    provenance_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE ingredient_entries (
    workspace_id TEXT NOT NULL,
    entry_id TEXT NOT NULL,
    name TEXT NOT NULL,
    producer TEXT NOT NULL DEFAULT '',
    category TEXT NOT NULL,
    dup_key TEXT NOT NULL,
    current_revision INTEGER NOT NULL CHECK (current_revision > 0),
    metadata_revision INTEGER NOT NULL CHECK (metadata_revision > 0),
    archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0,1)),
    created_at INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, entry_id),
    FOREIGN KEY (workspace_id) REFERENCES workspaces(workspace_id)
);
CREATE UNIQUE INDEX ingredient_entry_global_id ON ingredient_entries(entry_id);
CREATE UNIQUE INDEX ingredient_entry_duplicate ON ingredient_entries(workspace_id, dup_key);
CREATE TABLE ingredient_revisions (
    workspace_id TEXT NOT NULL,
    entry_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    schema_version INTEGER NOT NULL,
    name TEXT NOT NULL,
    producer TEXT NOT NULL DEFAULT '',
    category TEXT NOT NULL,
    document_json TEXT NOT NULL,
    provenance_json TEXT NOT NULL,
    content_digest TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    recorded_at INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, entry_id, revision),
    FOREIGN KEY (workspace_id, entry_id) REFERENCES ingredient_entries(workspace_id, entry_id)
);
CREATE TABLE ingredient_archive_events (
    workspace_id TEXT NOT NULL,
    entry_id TEXT NOT NULL,
    metadata_revision INTEGER NOT NULL,
    archived INTEGER NOT NULL CHECK (archived IN (0,1)),
    actor_id TEXT NOT NULL,
    recorded_at INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, entry_id, metadata_revision),
    FOREIGN KEY (workspace_id, entry_id) REFERENCES ingredient_entries(workspace_id, entry_id)
);
CREATE TRIGGER ingredient_revisions_no_update BEFORE UPDATE ON ingredient_revisions BEGIN SELECT RAISE(ABORT,'ingredient revisions are immutable'); END;
CREATE TRIGGER ingredient_revisions_no_delete BEFORE DELETE ON ingredient_revisions BEGIN SELECT RAISE(ABORT,'ingredient revisions are immutable'); END;
CREATE TRIGGER ingredient_archive_events_no_update BEFORE UPDATE ON ingredient_archive_events BEGIN SELECT RAISE(ABORT,'ingredient archive events are immutable'); END;
CREATE TRIGGER ingredient_archive_events_no_delete BEFORE DELETE ON ingredient_archive_events BEGIN SELECT RAISE(ABORT,'ingredient archive events are immutable'); END;

-- Starter seed v1. Source/licensing/provenance are explicit: derived from the
-- approved reference fixtures in docs/design/prototype/catalogue.js using the
-- documented PPG boundary conversion (yield = PPG / 46.214). Illustrative
-- specifications, not independently verified manufacturer data.
INSERT INTO starter_ingredient_entries (entry_id, name, producer, category, version, archived, document_json, provenance_json, created_at) VALUES
('starter-vienna', 'Weyermann Vienna', 'Weyermann', 'Base malt', 1, 0,
 '{"schema_version":1,"kind":"fermentable","category":"Base malt","specification":{"name":"Weyermann Vienna","producer":"Weyermann","type":"grain","yield_fraction":0.7789847232440387,"colour_ebc":7},"extensions":{}}',
 '{"source":"krausen-starter-catalogue-v1","license":"AGPL-3.0-or-later (bundled with Krausen); factual ingredient specifications","notes":["Derived from docs/design/prototype/catalogue.js reference fixtures","Illustrative; not independently verified manufacturer specifications","yield_fraction = 36 PPG / 46.214 documented boundary conversion"]}',
 1793404800),
('starter-carahell', 'Weyermann Carahell', 'Weyermann', 'Specialty malt', 1, 0,
 '{"schema_version":1,"kind":"fermentable","category":"Specialty malt","specification":{"name":"Weyermann Carahell","producer":"Weyermann","type":"grain","yield_fraction":0.757346258709482,"colour_ebc":25},"extensions":{}}',
 '{"source":"krausen-starter-catalogue-v1","license":"AGPL-3.0-or-later (bundled with Krausen); factual ingredient specifications","notes":["Derived from docs/design/prototype/catalogue.js reference fixtures","Illustrative; not independently verified manufacturer specifications","yield_fraction = 35 PPG / 46.214 documented boundary conversion"]}',
 1793404800),
('starter-signature', 'Joe White Signature Lager', 'Joe White', 'Base malt', 1, 0,
 '{"schema_version":1,"kind":"fermentable","category":"Base malt","specification":{"name":"Joe White Signature Lager","producer":"Joe White","type":"grain","yield_fraction":0.8006231877785952,"colour_ebc":3.6},"extensions":{}}',
 '{"source":"krausen-starter-catalogue-v1","license":"AGPL-3.0-or-later (bundled with Krausen); factual ingredient specifications","notes":["Derived from docs/design/prototype/catalogue.js reference fixtures","Illustrative; not independently verified manufacturer specifications","yield_fraction = 37 PPG / 46.214 documented boundary conversion"]}',
 1793404800),
('starter-munich', 'Weyermann Munich I', 'Weyermann', 'Specialty malt', 1, 0,
 '{"schema_version":1,"kind":"fermentable","category":"Specialty malt","specification":{"name":"Weyermann Munich I","producer":"Weyermann","type":"grain","yield_fraction":0.818496559484139,"colour_ebc":14},"extensions":{}}',
 '{"source":"krausen-starter-catalogue-v1","license":"AGPL-3.0-or-later (bundled with Krausen); factual ingredient specifications","notes":["Derived from docs/design/prototype/catalogue.js reference fixtures","Illustrative; not independently verified manufacturer specifications","yield_fraction = 37.826 PPG / 46.214 documented boundary conversion"]}',
 1793404800);
