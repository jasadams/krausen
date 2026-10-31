# Krausen

A brewing recipe workspace with formulation calculations, recipe revision history
and historical brew records. Released as AGPL-3.0-or-later, with hosted and
self-hosted deployment targets.

UI and MCP are two interfaces to the same application: the same operations,
permissions, calculation results and historical-integrity rules. Read the
[documentation index](docs/README.md) and [agent guidance](AGENTS.md) before
implementation.

## Project state

The repository contains the Rust/Leptos foundation, existing account/workspace
services, pure calculation and BeerXML crates, and an approved interactive
[recipe/component design reference](docs/design/prototype/README.md).
The shared application foundation and native recipe revision persistence are
under development; authenticated MCP operations, native recipe authoring and
immutable production brew records remain implementation targets. The prototype
uses browser-local fixtures and WASM.

On startup, legacy recipe records are converted to native revisions. If conversion
fails, the original stored record remains available for authorized recovery and
the failure is reported. The old editor cannot open or modify migrated recipes,
and legacy create/import requests are unavailable until native authoring arrives.
The recovery download contains the stored legacy JSON record; it is not a claim
that original uploaded file bytes were retained by earlier versions.

[Architecture](docs/ARCHITECTURE.md), [data model](docs/DATA_MODEL.md),
[calculations](docs/CALCULATIONS.md) and [design system](DESIGN.md) define the
implementation contracts. [ADOPTION.md](docs/ADOPTION.md) records boundary decisions,
acceptance scenarios and delivery ownership. Historical `KRAUSEN.md` and the old
starter plan are context, not current feature or coding instructions.

## Code boundaries

| Boundary | Role |
| --- | --- |
| `krausen-app` | Shared commands, queries, policy and transaction ownership |
| `krausen-types` | Versioned native data and transport-neutral DTOs |
| `krausen-brewery` | Brewing persistence helpers and native-to-engine projection |
| `krausen-calc` | Pure deterministic calculation library |
| `krausen-beerxml` | Format parsing, serialization and native mappings |
| `krausen-auth` | Account/session/security primitives |
| `krausen-core` | Database/transaction/configuration infrastructure |
| `krausen-ui`, server adapters | Presentation and protocol envelopes; invoke app |

The diagram describes the target division of responsibility; the existing starter
still has logic to migrate. Do not add brewing policy to auth or either adapter.

## Deployment contract

Hosted uses PostgreSQL; self-hosted supports SQLite. Both use authenticated
installation-local accounts and workspace ownership. A personal collection is a
one-member workspace, not a login bypass. The starter's `personal` mode remains
legacy code to retire in KRA-10 and must not be used to claim protected-operation
acceptance. Shared database state owns request results, credentials and history;
process-local caches and notifications are not sources of authority.

Initial acceptance is one application process with either supported database.
Additional application instances are future deployment work requiring shared
credential/retry state and transport checks; no horizontal scalability has been
benchmarked or promised by the design review.

## Running in Development

### Prerequisites

- Rust (stable)
- [Trunk](https://trunkrs.dev/) (`cargo install trunk`)
- [tailwindcss CLI](https://tailwindcss.com/blog/standalone-cli) (v4)
- PostgreSQL (for SaaS mode) or nothing (SQLite mode)

### Environment Variables

Create a `.env` file or export these:

```bash
# Required
DATABASE_URL=postgres://user:pass@localhost:5432/krausen   # or sqlite://./data/krausen.db
JWT_SECRET_KEY=your-secret-key-here
ENCRYPTION_KEY=base64-encoded-32-byte-key

# Optional
KRAUSEN_MODE=self_hosted                    # saas | self_hosted (personal is legacy)
PORT=8003                                # default: 8003
FRONTEND_URL=http://localhost:5173       # Trunk dev server
BASE_URL=http://localhost:8003           # Backend URL

# SMTP (required for email verification in saas mode)
SMTP_HOST=smtp.example.com
SMTP_USER=you@example.com
SMTP_PASSWORD=your-smtp-password
SMTP_FROM=noreply@example.com

# Google OAuth (optional)
GOOGLE_OAUTH_CLIENT_ID=...
GOOGLE_OAUTH_CLIENT_SECRET=...

# WebAuthn (optional, derived from FRONTEND_URL by default)
WEBAUTHN_RP_ID=localhost
WEBAUTHN_RP_NAME=Krausen
```

### Build and Run

```bash
# Terminal 1: Build the WASM frontend (watches for changes)
cd crates/krausen-ui
trunk build --watch

# Terminal 2: Run the server
cargo run --package krausen-server
```

The server starts at `http://localhost:8003`. In development with Trunk, the frontend is served from `http://localhost:5173` with proxy to the backend.

## Repository

```sh
git clone https://github.com/jasadams/krausen.git
cd krausen
```

## License

AGPL-3.0-or-later.
