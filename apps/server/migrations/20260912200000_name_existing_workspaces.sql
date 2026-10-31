-- Give a name to workspaces that were created without one.
--
-- create_workspace_for_user used to insert name = NULL, so every workspace made
-- by signup was nameless. The product addresses a workspace by name in several
-- places — the switcher lists it, settings shows it, and transferring ownership
-- asks you to type it back to confirm, which cannot be satisfied by an empty
-- string.
--
-- Name them after the owner where there is a display name to work from, and use
-- the same default as personal mode otherwise. Owners can rename afterwards.
UPDATE workspaces
SET name = COALESCE(
    (
        SELECT u.name || '''s Workspace'
        FROM users u
        WHERE u.user_id = workspaces.owner_user_id
          AND u.name IS NOT NULL
          AND TRIM(u.name) != ''
    ),
    'My Workspace'
)
WHERE name IS NULL OR TRIM(name) = '';
