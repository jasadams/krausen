-- oauth_clients.id is bound as text by the OAuth client registration (as in Kyomi, which converted its
-- uuid columns to text); no foreign key references it.
ALTER TABLE oauth_clients
    ALTER COLUMN id TYPE TEXT USING id::text,
    ALTER COLUMN id DROP DEFAULT;
