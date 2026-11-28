-- Equipment spec (KRA-70): descriptive about-block per immutable revision, and
-- one optional default profile per workspace. Existing revisions get an empty
-- about-block; they are never rewritten.
ALTER TABLE equipment_revisions ADD COLUMN about_json TEXT NOT NULL DEFAULT '{}';
CREATE TABLE workspace_default_equipment (
    workspace_id TEXT PRIMARY KEY,
    profile_id TEXT NOT NULL,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(workspace_id),
    FOREIGN KEY (workspace_id,profile_id) REFERENCES equipment_profiles(workspace_id,profile_id)
);
