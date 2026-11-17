# Installation authentication

Browser access requires an active account and a valid signed access token; there is no
per-session credential record. MCP access requires an active account and an MCP access
token (below). On upgrade, existing browser access tokens (which carry credential
claims) are rejected and users refresh or sign in again; MCP OAuth grants made before
the upgrade are removed and MCP clients must reconnect. Existing accounts, workspace
ownership, recipes and password or passkey methods are retained. Browser refresh tokens
rotate with a grace period: a token that was just
replaced is still accepted for `refresh_token_grace_period_seconds` (30 seconds)
and rotates again, so a lost refresh response or several tabs holding the same
cookie never sign the user out. A replaced token presented after the grace period
is treated as theft: its refresh-token family is revoked and the user must sign in
again. A revoked or expired refresh token is rejected even inside the grace period.
Revoking a refresh family or signing out one session does not end an access token
already issued; it ends at its 15-minute expiry. Only signing out everywhere or recovery
(below) ends access tokens at once.

Token lifetimes, cookie settings, rate-limit buckets, key-value key prefixes and TTLs come
from `shared/constants.toml`, which is also embedded in the binary. Browser cookies are
always `SameSite=Strict` and `HttpOnly`; `Secure` is dropped only when `FRONTEND_URL` is
not `https`. Rate limits are fixed windows per client address: sign-in 10 per 900
seconds, sign-up 5 per 3600 seconds and refresh 30 per 300 seconds. Per-account values
(5, 1 and 20) and the API-call bucket (1000 per address and 500 per account in 60
seconds) are configured but not enforced, because no call site passes a user id or
uses that bucket (Kyomi's call sites are the same). Behind a reverse proxy, set
`X-Real-IP`: the refresh limit takes the address from that header, and without it all
clients share one bucket.

Switching workspace (`switch_workspace`) requires an active membership in the target
workspace, saves it as the account's last workspace and re-mints the browser session
(new access and refresh cookies) for it. A protected page that finds its access token
rejected tries one silent refresh and retries (at most 3 attempts in 30 seconds), then
sends the browser to `/login?redirect=<path>` with a full navigation. A browser that is
already signed in is sent from the sign-in page by full navigation, and a sign-in that
carries an MCP `oauth_continue` token also leaves by full navigation, so the server
endpoint that resumes the authorization is reached; an ordinary sign-in uses
client-side navigation.

Signing out everywhere and completing account recovery (password) revoke browser
sessions immediately: each account has a revocation cutoff (`users.sessions_valid_from`,
Unix microseconds). A browser access token carries its signed microsecond issuance and is
rejected on the next request, WebSocket connection or refresh once its issuance is at or
before the cutoff, and the account's refresh tokens are revoked in the same transaction.
A WebSocket is checked only when it connects: an already-open socket is not closed by
signing out everywhere or by its token expiring, and ends only when it closes.
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
cutoff and the active account); a
single-session logout does not stop a copied access cookie from approving a consent
for the remaining access-token lifetime. `/mcp` also accepts a browser session. An
unauthenticated request receives 401 with a `WWW-Authenticate` header pointing at the
protected-resource metadata.

Browser sessions refresh transparently. A request with a missing or expired access cookie
and a valid refresh cookie is refreshed on the server (a new access token, a rotated
refresh token and `Set-Cookie` headers on the response) before the handler runs; if the
refresh fails the request continues unchanged and receives the usual 401. Logout requests
are never refreshed. The page also posts to `/api/v1/auth/refresh` before the application
starts. There is no cross-tab lock: tabs that refresh the same cookie at once all
succeed because a replaced refresh token stays valid for the grace period above, so
sessions also refresh on origins without `navigator.locks` (plain-HTTP LAN addresses).
The client address recorded for a session and used for rate limits comes from `X-Real-IP`,
then the first `X-Forwarded-For` entry, and is `unknown` when neither is a valid address.

Public passkey enrollment is temporarily unavailable until account ownership
can be proven. Existing authenticated passkey management is unchanged.
