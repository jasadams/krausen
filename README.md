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

## Local isolated run (development and Playwright)

`scripts/dev/krausen-dev` runs a disposable Krausen in Docker, published on `127.0.0.1`
only, with a throwaway database on a Docker volume and test-only secrets:

```bash
scripts/dev/krausen-dev build      # image; BuildKit caches reuse crates, compiled cache is per checkout
scripts/dev/krausen-dev up         # fresh empty database and server; the e2e global setup signs up local@localhost
scripts/dev/krausen-dev check      # fails unless 127.0.0.1 answers 200 and the LAN address does not
eval "$(scripts/dev/krausen-dev env)"   # this checkout's PATH shim, BASE_URL and database path
scripts/dev/krausen-dev browsers   # Playwright headless-shell Chromium
scripts/dev/krausen-dev down       # remove this checkout's containers, volume and sqlite3 shim
```

The e2e helpers call `sqlite3 <path>`; with the exported `PATH` that goes to a helper
container next to the server. Do not open the database file from the host: a SQLite file
shared between the host and Docker's VM loses writes. `down` deletes the `sqlite3` shim, so a
shell that already ran the `env` exports needs `up` again (or a new shell) afterwards. For a native run set
`BIND_ADDR=127.0.0.1` so the server listens on loopback only (default `0.0.0.0`).

Every checkout gets one id: `KRAUSEN_BUILD_CACHE_ID` when set, otherwise a
16-character hash of the checkout's absolute path. The default image tag is
`krausen-dev:<id>`, the containers are `krausen-dev-server-<id>` and
`krausen-dev-sqlite-<id>`, the database volume is `krausen-dev-data-<id>`, and
the `sqlite3` shim lives under `~/.cache/krausen-dev/<id>/shim`. `KRAUSEN_DEV_IMAGE`
and `KRAUSEN_DEV_STATE` still override the image tag and state directory. Set
`KRAUSEN_BUILD_CACHE_ID` in the shell that runs both `build` and `up`, so the run
uses the image it just built.

The loopback port is `41465 + (cksum(id) % 1000)` unless `KRAUSEN_DEV_PORT` chooses
one. It is stable for a checkout across `up`, `check` and `env`; when choosing a
port explicitly, export `KRAUSEN_DEV_PORT` for `env` as well as `up`. If anything
already holds the port, `up` fails and says so rather than silently choosing
another one. Because the image, containers, volume, shim and port all belong to
one checkout, two checkouts can `build` and `up` side by side, each server running
its own image against its own database, and `down` in one leaves the other running.

The image build keeps its cargo registry, git and compiled `/build/target` caches
between builds, but every cache-mount id includes the same `KRAUSEN_BUILD_CACHE_ID`.
The target directory must be per checkout so two images cannot link each other's
code; the download caches are also per checkout because concurrent cold builds
otherwise race while cargo unpacks the same crate. A raw `docker build` must pass
`--build-arg KRAUSEN_BUILD_CACHE_ID=<unique value>`; without it the build fails
with a clear error instead of silently sharing a cache.

Cargo decides whether a crate is up to date from file modification times, and the image
build copies source files with their original times. A file edited earlier than the last
cached build (an edit made while a build ran, a `git checkout` of older files, `touch -t`)
would therefore be skipped and the image would keep the old code. Before compiling,
`scripts/refresh-changed-mtimes.sh` compares the SHA-256 of every source file in the build
context with a manifest stored in the target cache (once the previous build fully succeeded) and gives each new or changed file a
current time, so every workspace crate whose content changed is compiled whatever its
timestamps. Unchanged crates and all registry dependencies stay cached.

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

### Test cadence

Run targeted checks while developing, then the PR checks once when the PR is ready,
either locally or in CI, not both. On failure, fix the cause and rerun only the
failed test until it passes. CI has two tiers, both manual so pushes, readiness
changes and merges cannot duplicate a local run:

- **PR checks** (`foundation.yml`): the Rust tests and checks plus only the browser
  groups that `e2e/browser-groups.json` maps to the paths the PR changes (the smoke
  group when nothing maps). Dispatch it on the ready PR's branch:

  ```sh
  gh workflow run foundation.yml --ref <pr-branch>
  ```

- **Full browser suite** (`browser-full.yml`): every selected browser test, split
  into parallel shards with one server and database each. Run it on the release
  commit; the release workflow will not build a version tag without a passing run
  on that commit:

  ```sh
  gh workflow run browser-full.yml --ref main
  ```

Locally, `e2e/scripts/profile-browser.sh` runs the whole selection against a
disposable server (`profile-browser.sh GROUPS I/N` runs named groups or one shard).
Wait for a requested run before merging. A new request cancels an in-progress run
for the same branch. If the PR checks already passed locally, record that evidence
instead of dispatching CI. Retain the tested commit and scope with the PR; do not
claim old results cover changed code. Retries are off: fix an intermittent failure.

## Repository

```sh
git clone https://github.com/jasadams/krausen.git
cd krausen
```

## License

AGPL-3.0-or-later.
