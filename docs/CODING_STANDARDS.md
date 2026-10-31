# Krausen Coding Standards

Implementation conventions for the contracts in [ARCHITECTURE.md](ARCHITECTURE.md),
[DATA_MODEL.md](DATA_MODEL.md) and [CALCULATIONS.md](CALCULATIONS.md). These
replace conflicting starter conventions; the implementation migration is tracked
separately from adoption.

## Rust Patterns

### Database Queries
- Use the backend-aware query helpers when they accept the required executor.
  Mutating workflows and their source/authorization reads use the same `DbTx`;
  extend helpers or match explicitly on its PostgreSQL/SQLite variants. Never
  acquire a pool connection from inside a transaction-participating helper.
- Support both PostgreSQL and SQLite in pool and transaction execution; never
  assume one backend. Enable and test foreign keys on SQLite.
- Use `sql_compat` helpers (`now()`, `bool_true()`, `ilike()`, etc.) for dialect-dependent SQL fragments.
- Generate UUIDs as `Uuid::new_v4().to_string()` for all entity IDs.

### Service Layer
- `krausen-app` owns public commands/queries, trusted context, policy and transaction
  orchestration. Public results/errors are typed, transport-neutral shared types.
- The application command alone begins, commits or rolls back a write transaction.
  Mutating auth/brewery helpers take `&mut DbTx`; standalone reads may take a pool.
  Helpers cannot independently commit, nest transactions or obtain a new connection.
- Commit entity/revision/evidence/retry/change records atomically. Append required
  change records before commit on that same transaction, never as a separate write.
- Dispatch optional notifications only after commit. Required email/external effects
  use a committed outbox; delivery errors do not change a successful command result.
- No SSE/WebSocket/MCP manager types in application/domain helper signatures.
- Low-level infrastructure errors may use `krausen_core::Result`; public operations
  map them to the shared application outcomes. No domain helper returns `ServerFnError`.

### Server Functions (Leptos)
- Server functions are thin wrappers: extract auth, extract context, call service, return.
- No business logic in UI or MCP adapters: invoke `krausen-app`. Auth provides
  identity/security primitives, not a home for brewing/application orchestration.
- Use `#[server(prefix = "/leptos-api")]` for all server functions.
- Return typed Rust enums (not HTTP status codes). UI pattern-matches on variants.

### Error Handling
- Never silently discard errors with `let _ =`. At minimum, log with `tracing::warn!`.
- Preserve typed domain/application outcomes through both adapters; map infrastructure
  failures at the application boundary without exposing database/secrets to clients.
- Server function errors convert via `IntoServerFnError` trait.

### No Banned Patterns
- No `#[allow(dead_code)]`, `#[allow(unused_variables)]`, `#[allow(unused_imports)]`
- No `closure.forget()` on persistent listeners
- No hardcoded `"unknown"` IP addresses
- No duplicated helper functions across modules

## Leptos / Frontend

### Component Patterns
- Use Leptos components (`<Button>`, `<Modal>`), never raw HTML elements for styled components.
- Styles live in the component definition, not in the caller.
- Pass `variant`, `size`, and optional `class` props — don't inline Tailwind in callers.

### SSR / Hydration
- Never use `Resource::new()` inside `#[cfg(target_arch = "wasm32")]` blocks (desyncs hydration IDs).
- Gate server-only code with `#[cfg(feature = "ssr")]`.
- Gate client-only code with `#[cfg(target_arch = "wasm32")]`.

### Dynamic Query Builders
- When binding values dynamically in Postgres, CAST integer/bigint parameters explicitly: `CAST($N AS INTEGER)` or `CAST($N AS BIGINT)`. Postgres rejects TEXT binds for integer columns.
- Inline LIMIT/OFFSET values directly (they are sanitized i64, not user input). Postgres rejects TEXT binds for LIMIT/OFFSET.
- If an existing Leptos encoding cannot carry a shared DTO directly, adapt the
  transport envelope. Do not weaken typed app inputs into comma-delimited strings
  or add UI-only validation inside shared services.

### Change delivery and client state
- Durable change records and domain history are distinct responsibilities. Delivery
  managers belong to server adapters/infrastructure, never brewing policy.
- Refetch after writes/navigation/focus; optional invalidation is an acceleration.
  Missed notifications cannot affect revision/conflict or retry correctness.
- Client-side IndexedDB operations must be wrapped in `spawn_local` — they are async but not Send.
- Wrap non-Send types (like IDB handles) in `SendWrapper` for use with Leptos signals.

## Design System Adherence
- Root font size is 15px (`html { font-size: 15px }` in `style/main.css`) — all rem calculations must account for this.
- Design tokens live in `crates/krausen-ui/style/main.css`. That file is the source of truth for colour; do not hardcode hex values in components.
- All interactive elements need `transition-colors` with `duration-200`.
- All interactive elements need `focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring`.

## Canonical models and projection boundaries

- Versioned native documents live in `krausen-types::brewery` and contain data,
  not policy or transport dependencies. Do not persist a BeerXML record as the
  editable native recipe or use reduced engine inputs as a full recipe.
- `krausen-brewery::mapping::calculation` maps resolved native values to the pure
  engine. `krausen-beerxml::native` owns interchange mapping and warnings.
- `krausen-app::formulation` resolves/authorizes context and owns calculation
  preview assembly for both UI and MCP. No production browser calculation bypass.
- Validate every linked ID against workspace and expected parent, including
  measurement corrections. Two memberships do not permit a cross-workspace link.
- Follow versioned calculation-context and correction-slot concurrency contracts;
  never compare rounded display values or choose a primary actual by accident.

## Acceptance

Use [ADOPTION.md](ADOPTION.md) for required boundary scenarios and the ticket map.
Tests must exercise public shared operations and real supported database paths;
mocking only an adapter is not transaction or authorization acceptance. Document
remaining legacy code explicitly until its implementation ticket replaces it.
