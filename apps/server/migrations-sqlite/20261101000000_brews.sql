-- Immutable brew plan/source identity; lifecycle and evidence remain later work.
CREATE TABLE brews (
    workspace_id TEXT NOT NULL,
    brew_id TEXT NOT NULL,
    recipe_id TEXT NOT NULL,
    selected_recipe_revision INTEGER NOT NULL CHECK (selected_recipe_revision > 0),
    status TEXT NOT NULL CHECK (status IN ('started', 'completed', 'cancelled')),
    revision INTEGER NOT NULL CHECK (revision > 0),
    started_at INTEGER NOT NULL,
    started_by TEXT NOT NULL,
    plan_schema_version INTEGER NOT NULL,
    plan_json TEXT NOT NULL,
    plan_content_digest TEXT NOT NULL,
    PRIMARY KEY (workspace_id, brew_id),
    FOREIGN KEY (workspace_id, recipe_id)
        REFERENCES recipes(workspace_id, recipe_id),
    FOREIGN KEY (workspace_id, recipe_id, selected_recipe_revision)
        REFERENCES recipe_revisions(workspace_id, recipe_id, revision)
);

CREATE INDEX brews_workspace_recipe_started
    ON brews(workspace_id, recipe_id, started_at, brew_id);

CREATE TRIGGER brews_plan_immutable
BEFORE UPDATE ON brews
WHEN NEW.workspace_id IS NOT OLD.workspace_id
  OR NEW.brew_id IS NOT OLD.brew_id
  OR NEW.recipe_id IS NOT OLD.recipe_id
  OR NEW.selected_recipe_revision IS NOT OLD.selected_recipe_revision
  OR NEW.started_at IS NOT OLD.started_at
  OR NEW.started_by IS NOT OLD.started_by
  OR NEW.plan_schema_version IS NOT OLD.plan_schema_version
  OR NEW.plan_json IS NOT OLD.plan_json
  OR NEW.plan_content_digest IS NOT OLD.plan_content_digest
BEGIN
    SELECT RAISE(ABORT, 'brew plan and source identity are immutable');
END;

CREATE TRIGGER brews_no_delete
BEFORE DELETE ON brews
BEGIN
    SELECT RAISE(ABORT, 'brew history cannot be deleted');
END;
