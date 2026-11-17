-- Browser sessions and MCP OAuth no longer use durable credential rows: a valid
-- signed access token is authority (revocation is `users.sessions_valid_from`),
-- and MCP OAuth grants live in `oauth_clients` and `refresh_tokens`.
-- Existing MCP OAuth grants are not carried over; MCP clients must reconnect.
DROP TABLE IF EXISTS mcp_oauth_access_tokens;
DROP TABLE IF EXISTS mcp_oauth_refresh_tokens;
DROP TABLE IF EXISTS mcp_oauth_authorization_codes;
DROP TABLE IF EXISTS mcp_oauth_pending;
DROP TABLE IF EXISTS mcp_oauth_clients;
DROP TABLE IF EXISTS auth_credentials;
