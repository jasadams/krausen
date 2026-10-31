-- Recipes. See the SQLite migration of the same name for the reasoning behind
-- the document-plus-index shape.

CREATE TABLE IF NOT EXISTS public.recipes (
    recipe_id       TEXT NOT NULL PRIMARY KEY,
    workspace_id    TEXT NOT NULL REFERENCES public.workspaces(workspace_id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    style_name      TEXT,
    batch_size_l    DOUBLE PRECISION,
    boil_minutes    INTEGER,
    document        TEXT NOT NULL,
    source          TEXT NOT NULL DEFAULT 'krausen',
    created_by      TEXT REFERENCES public.users(user_id),
    created_at      TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    deleted_at      TIMESTAMP WITH TIME ZONE
);

CREATE INDEX IF NOT EXISTS idx_recipes_workspace
    ON public.recipes(workspace_id, deleted_at, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_recipes_name
    ON public.recipes(workspace_id, name);
