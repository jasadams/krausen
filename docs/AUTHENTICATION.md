# Installation authentication

Browser and MCP access require an active account and a durable credential. On
upgrade, existing browser access and refresh tokens without a credential binding
are rejected; users must sign in again. Existing accounts, workspace ownership,
recipes and password or passkey methods are retained. Browser refresh tokens
rotate with the same grace-period behaviour as Kyomi: a token that was just
replaced is still accepted for `refresh_token_grace_period_seconds` (30 seconds)
and rotates again, so a lost refresh response or several tabs holding the same
cookie never sign the user out. A replaced token presented after the grace period
is treated as theft: its credential family is revoked and the user must sign in
again. A revoked, expired or unbound credential is rejected even inside the grace
period. MCP OAuth refresh tokens are unchanged and remain single use: reusing a
consumed one revokes its credential, so the MCP client must reconnect.

With SMTP configured, signup sends a link; opening the link changes nothing until the
user presses "Confirm Email", which creates the account, so mail scanners that
prefetch links cannot consume it. An address that already has an account receives a
notice email and no link, so a signup link can never change an existing account's
password or sign-in methods. Without SMTP on a self-hosted install, the first user
and invited users sign up directly; any other address is refused.

The installation publishes OAuth authorization and protected-resource metadata.
Public clients use authorization code with S256 PKCE; confidential clients use
`client_secret_basic` and S256 PKCE. Redirects must match registered values and
use HTTPS or HTTP loopback. Client metadata is registered locally; the server
does not fetch arbitrary client metadata URLs. Configure the public base URL to
match the externally visible issuer and MCP resource URL.

Production OAuth and MCP require an HTTPS external base URL. A trusted reverse
proxy may terminate TLS and forward to the server over a private connection;
configure the externally visible HTTPS base URL and HTTPS frontend URL explicitly,
with Secure and HttpOnly browser cookies enabled. OAuth refuses an HTTPS base
configuration when Secure cookies are disabled. Host and forwarding
headers do not determine the issuer or grant a loopback exception. HTTP is
accepted only for a configured loopback origin as a development convenience;
that mode is not evidence of HTTPS protocol conformance.

Browser refresh is coordinated across tabs with the Web Locks API. A browser
without cross-tab locks cannot silently refresh and must sign in again when its
access token expires; this can affect HTTP LAN origins where the browser does
not expose secure-context APIs. Configure HTTPS for normal browser use.

Public passkey enrollment is temporarily unavailable until account ownership
can be proven. Existing authenticated passkey management is unchanged.
