# Installation authentication

Browser and MCP access require an active account and a durable credential. On
upgrade, existing browser access and refresh tokens without a credential binding
are rejected; users must sign in again. Existing accounts, workspace ownership,
recipes and password or passkey methods are retained. Refresh tokens rotate;
reusing an earlier token revokes its credential family, so the user must sign in
again or reconnect the MCP client.

The server independently excludes the MCP SDK's internal logging targets from
its output sink, even when `RUST_LOG` enables them. The SDK can emit complete
caller-controlled requests, notifications and responses at multiple levels;
application-owned transport logs record fixed operation identifiers instead.
The HTTP/SDK regression runs with `RUST_LOG=trace,rmcp=trace` and checks that
sentinel credentials and request/response payloads do not reach the sink.

Password changes use an installation-key-derived, purpose-separated HMAC
commitment for durable retry identity. Retry records contain neither passwords
nor password hashes. A separate binding ties each request to its initiating
credential, and the commitment includes a key identifier; after key
rotation, an older retained request reports that its result cannot be verified
instead of running again or treating a new payload as a match. A user can read
current account state and start a new request. Current credential authority is
checked before retry recovery; a first execution verifies the old password and
account revision inside the transaction.

For an empty installation, a local operator can initialize the first owner by
running the server binary with `bootstrap-owner EMAIL` and providing the new
password on standard input. This operation is one-time and refuses an existing
account. It is not exposed through an HTTP endpoint.

To retain the historical automatically provisioned personal workspace, an
operator can instead run `claim-legacy-owner EMAIL` with the new password on
standard input. The command succeeds only for an unambiguous, uncredentialed
legacy owner and workspace and refuses an established authentication method,
credential or competing claim. It retains the existing IDs, ownership and
records. Installations that do not meet these checks require operator-led
recovery; the command will not reset or take over an account.

Public email-only signup and email-only invitation acceptance are unavailable
when email verification is not configured. Possession of an email address alone
does not establish ownership.

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
