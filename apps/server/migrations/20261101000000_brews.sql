-- Immutable brew plan/source identity; lifecycle and evidence remain later work.
CREATE TABLE brews (
    workspace_id TEXT NOT NULL,
    brew_id TEXT NOT NULL,
    recipe_id TEXT NOT NULL,
    selected_recipe_revision BIGINT NOT NULL CHECK (selected_recipe_revision > 0),
    status TEXT NOT NULL CHECK (status IN ('started', 'completed', 'cancelled')),
    revision BIGINT NOT NULL CHECK (revision > 0),
    started_at BIGINT NOT NULL,
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

CREATE FUNCTION guard_brew_plan_mutation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'brew history cannot be deleted';
    END IF;
    IF NEW.workspace_id IS DISTINCT FROM OLD.workspace_id
       OR NEW.brew_id IS DISTINCT FROM OLD.brew_id
       OR NEW.recipe_id IS DISTINCT FROM OLD.recipe_id
       OR NEW.selected_recipe_revision IS DISTINCT FROM OLD.selected_recipe_revision
       OR NEW.started_at IS DISTINCT FROM OLD.started_at
       OR NEW.started_by IS DISTINCT FROM OLD.started_by
       OR NEW.plan_schema_version IS DISTINCT FROM OLD.plan_schema_version
       OR NEW.plan_json IS DISTINCT FROM OLD.plan_json
       OR NEW.plan_content_digest IS DISTINCT FROM OLD.plan_content_digest THEN
        RAISE EXCEPTION 'brew plan and source identity are immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER brews_plan_immutable
    BEFORE UPDATE OR DELETE ON brews
    FOR EACH ROW EXECUTE FUNCTION guard_brew_plan_mutation();
