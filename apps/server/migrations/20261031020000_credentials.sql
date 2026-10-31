-- Durable principal credentials and installation-local OAuth grants.
-- Times are Unix seconds on both supported backends.
CREATE TABLE auth_credentials (
    credential_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(user_id),
    kind TEXT NOT NULL CHECK (kind IN ('browser', 'mcp')),
    issuer TEXT NOT NULL,
    audience TEXT NOT NULL,
    expires_at BIGINT NOT NULL,
    revoked_at BIGINT
);
CREATE INDEX auth_credentials_user ON auth_credentials(user_id);
CREATE TABLE mcp_oauth_clients (
    client_id TEXT PRIMARY KEY,
    client_name TEXT NOT NULL,
    redirect_uris TEXT NOT NULL,
    auth_method TEXT NOT NULL CHECK (auth_method IN ('none', 'client_secret_basic')),
    secret_hash TEXT,
    created_at BIGINT NOT NULL
);
CREATE TABLE mcp_oauth_authorization_codes (
    code_hash TEXT PRIMARY KEY,
    client_id TEXT NOT NULL REFERENCES mcp_oauth_clients(client_id),
    user_id TEXT NOT NULL REFERENCES users(user_id),
    initiating_credential_id TEXT NOT NULL REFERENCES auth_credentials(credential_id),
    redirect_uri TEXT NOT NULL,
    resource TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    expires_at BIGINT NOT NULL,
    consumed_at BIGINT
);
CREATE TABLE mcp_oauth_refresh_tokens (
    token_hash TEXT PRIMARY KEY,
    credential_id TEXT NOT NULL REFERENCES auth_credentials(credential_id),
    client_id TEXT NOT NULL REFERENCES mcp_oauth_clients(client_id),
    expires_at BIGINT NOT NULL,
    consumed_at BIGINT
);
CREATE TABLE mcp_oauth_access_tokens (
    token_hash TEXT PRIMARY KEY,
    credential_id TEXT NOT NULL REFERENCES auth_credentials(credential_id),
    resource TEXT NOT NULL,
    expires_at BIGINT NOT NULL
);
CREATE TABLE mcp_oauth_pending (
    pending_hash TEXT PRIMARY KEY,
    client_id TEXT NOT NULL REFERENCES mcp_oauth_clients(client_id),
    redirect_uri TEXT NOT NULL,
    resource TEXT NOT NULL,
    state TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    user_id TEXT REFERENCES users(user_id),
    credential_id TEXT REFERENCES auth_credentials(credential_id),
    expires_at BIGINT NOT NULL,
    consumed_at BIGINT
);
