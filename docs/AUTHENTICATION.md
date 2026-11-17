# Installation authentication

Browser access requires an active account and a durable credential; MCP access
requires an active account and an MCP access token (below). On
upgrade, existing browser access and refresh tokens without a credential binding
are rejected; users must sign in again. Existing accounts, workspace ownership,
recipes and password or passkey methods are retained. Browser refresh tokens
rotate with a grace period: a token that was just
replaced is still accepted for `refresh_token_grace_period_seconds` (30 seconds)
and rotates again, so a lost refresh response or several tabs holding the same
cookie never sign the user out. A replaced token presented after the grace period
is treated as theft: its credential family is revoked and the user must sign in
again. A revoked, expired or unbound credential is rejected even inside the grace
period.

Signing out everywhere and completing account recovery (password) revoke browser
sessions immediately: each account has a revocation cutoff (`users.sessions_valid_from`,
Unix microseconds). A browser access token carries its signed microsecond issuance and is
rejected on the next request, WebSocket connection or refresh once its issuance is at or
before the cutoff, and the account's refresh tokens are revoked in the same transaction.
The cutoff only moves forward, including under concurrent events and clock rollback.
Failures to load the account, workspace or membership during authentication reject the
request rather than continuing with a default context.

With SMTP configured, signup sends a link; opening the link changes nothing until the
user presses "Confirm Email", which creates the account, so mail scanners that
prefetch links cannot consume it. An address that already has an account receives a
notice email and no link, so a signup link can never change an existing account's
password or sign-in methods. Without SMTP on a self-hosted install, the first user
and invited users sign up directly; any other address is refused.

## MCP OAuth

The installation is an OAuth authorization server for MCP clients.
Discovery is published at `/.well-known/oauth-authorization-server`,
`/.well-known/oauth-protected-resource` (also with a path), and
`/.well-known/openid-configuration` (also `/mcp/.well-known/openid-configuration`);
the URLs in them (and in the `WWW-Authenticate` challenge) are built from the request's
`X-Forwarded-Proto` and `Host` headers and are trusted as sent, so a reverse proxy must
forward both and a deployment without a trusted proxy must not expose these routes to
untrusted `Host` values. Clients register dynamically (`/api/v1/oauth/register`)
as public clients (no secret, up to 10 redirect URIs: HTTPS, HTTP loopback or a native
app scheme). Authorization uses the authorization code flow with S256 PKCE: a signed-out
visitor is sent to `/login` and returned to a consent page that shows the client, the
account, the workspace and the callback origin, with Allow and Deny. The consent form is
bound to the browser session by a CSRF token and a per-transaction cookie, and the
pending authorization and the one-time code live in the key-value store for five minutes.
Without `REDIS_URL` the key-value store is in memory, so pending authorizations and
codes do not survive a restart and are not shared between server instances.

The token endpoint issues a signed JWT access token (`purpose = mcp`, 15 minutes) and an
opaque refresh token (7 days) bound to the client. Refresh does not rotate the token: it
returns the same refresh token with a new access token and extends the 7-day window while
the client keeps using it. An MCP access token authorizes only the `/mcp` endpoint (every
other route refuses it) and carries no browser session authority, so signing out of the
browser does not end it; it ends at expiry, when the refresh token is revoked or expires,
or when the account is deactivated. A token that names a workspace is refused (401) once its
user is removed from that workspace, and every tool call is also authorized against the
workspace it names.
Consent is approved by the signed-in browser session (its access cookie, the session
cutoff and the active account); until the browser credential binding is removed, a
single-session logout does not stop a copied access cookie from approving a consent
for the remaining access-token lifetime. `/mcp` also accepts a browser session. An
unauthenticated request receives 401 with a `WWW-Authenticate` header pointing at the
protected-resource metadata.

Browser refresh is coordinated across tabs with the Web Locks API. A browser
without cross-tab locks cannot silently refresh and must sign in again when its
access token expires; this can affect HTTP LAN origins where the browser does
not expose secure-context APIs. Configure HTTPS for normal browser use.

Public passkey enrollment is temporarily unavailable until account ownership
can be proven. Existing authenticated passkey management is unchanged.
