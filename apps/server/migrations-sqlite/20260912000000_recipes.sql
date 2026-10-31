-- Recipes.
--
-- A recipe is always read and written as a whole document — nothing queries an
-- individual hop row — so the complete model lives in `document` as JSON and
-- the columns beside it exist only for listing and searching. That also keeps
-- "store everything" honest: mash schedules, water profiles and vendor
-- extensions survive a round trip through the database untouched, and the brew
-- snapshot KRAUSEN.md §4.6 requires becomes a copy of one column.
--
-- No derived values are stored. OG, IBU and colour are a pure function of the
-- recipe and an equipment profile, and a cached copy of a derived number is
-- just a copy that can go stale.

CREATE TABLE IF NOT EXISTS recipes (
    recipe_id       TEXT NOT NULL PRIMARY KEY,
    workspace_id    TEXT NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
    -- Denormalised from the document so the list view can sort and search
    -- without parsing every recipe.
    name            TEXT NOT NULL,
    style_name      TEXT,
    batch_size_l    REAL,
    boil_minutes    INTEGER,
    -- The complete recipe, losslessly.
    document        TEXT NOT NULL,
    -- Where it came from: 'krausen' or 'beerxml'.
    source          TEXT NOT NULL DEFAULT 'krausen',
    created_by      TEXT REFERENCES users(user_id),
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    -- Soft delete. A recipe is the record of something a person made; a
    -- mis-click should not be the end of it.
    deleted_at      TEXT
);

CREATE INDEX IF NOT EXISTS idx_recipes_workspace
    ON recipes(workspace_id, deleted_at, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_recipes_name
    ON recipes(workspace_id, name);
