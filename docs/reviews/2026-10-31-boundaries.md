# Independent architecture and documentation reviews

Date: 2026-10-31. Reviewed baseline:
[`d4afe9a`](https://github.com/jasadams/krausen/commit/d4afe9ab63968ac300c6f612a3620bfb053e8180).

Three independent reviewers read all 14 Markdown files at that baseline. They
received separate scopes, did not share findings while reviewing, and made no
edits. The coordinator checked their evidence and consolidated overlaps below.
This report records its baseline findings. Their subsequent resolutions and
independent changed-contract approval are in [ADOPTION.md](../ADOPTION.md);
recommendations below are historical evidence, not unresolved current status.

## Verdicts

| Review angle | Written verdict |
| --- | --- |
| Modular architecture and future scaling | The modular monolith supports product and team growth. Retain the KRA-5 implementation gate until transaction composition and canonical model ownership are explicit. |
| Interface parity, security and tenancy | The shared application boundary is sound. No remaining UI-exclusive production calculation path or separate MCP permission model was found. Clarify relationship ownership and complete the already tracked trust/transaction contracts. |
| History, calculations and domain evolution | The confirmed domain boundaries support future growth. Clarify selected revisions, preview context and measurement semantics before the affected persistence/operation contracts are implemented. |

The reviews support the architecture's direction, rather than unconditional
production adoption. KRA-1–4 remain completed design-delivery tickets. KRA-5 owns
the targeted refinements and reconciliation; the reviews do not require another
visual approval or a restart of design discovery.

## Prioritized findings

P1 means resolve before architecture adoption and transactional implementation.
P2 means resolve before implementing the affected contract, or explicitly assign
the migration work. P3 is a later improvement. These are documentation and
boundary findings, not claims of demonstrated production vulnerabilities.
Line references below refer to the reviewed baseline.

### B1 — P1: specify one transaction owner and a composable database context

Evidence: `docs/ARCHITECTURE.md:105`, `:107`, `:198`, `:205`;
`docs/CODING_STANDARDS.md:8`, `:14`, `:16`, `:53`.

The architecture requires entity, history, retry-result and change-record writes
to commit atomically. However, application transaction orchestration and brewery
workflow ownership do not yet specify how helpers share that transaction. Older
mandatory pool-first/separate-log conventions obstruct that composition.

Failure scenario: starting a brew opens a transaction, but a called helper
acquires another pool connection to read equipment or write history. This can
produce inconsistent or partially committed data. With the current single
SQLite connection, a nested acquisition can also wait behind the caller's own
transaction (`crates/krausen-core/src/db.rs:37`).

Smallest resolution: the shared application operation owns begin/commit/rollback;
mutating helpers receive the same transaction-capable context and cannot
independently commit or reacquire a pool connection. Place revision and relevant
permission checks inside that boundary. Replace the conflicting coding rules.
Validate a representative start-brew rollback/race/retry case on both SQLite and
PostgreSQL before multiplying operations. A generic repository framework is not
needed. This sharpens an already recorded KRA-5 adoption requirement.

### B2 — P2: name the canonical native model and conversion owners

Evidence: `docs/ARCHITECTURE.md:107`, `:109`, `:110`;
`docs/DATA_MODEL.md:54`, `:170`; `docs/CALCULATIONS.md:119`.

The documents require one editable native formulation but do not assign its
versioned types or native-to-engine/native-to-BeerXML mappings to a named owner.
Current persistence embeds BeerXML (`crates/krausen-brewery/src/recipe_service.rs:17`,
`:57`), while calculation inputs are narrower (`crates/krausen-calc/src/recipe.rs:74`)
and the existing converter targets them directly (`crates/krausen-beerxml/src/convert.rs:15`).

Evolution scenario: independent editor, snapshot and import work each grows a
different recipe representation; new process fields require coordinated changes
throughout the application and cannot reliably round-trip.

Smallest resolution: name one module/crate owner for the canonical native model
and each mapping. State that engine inputs and interchange records are projections.
An existing pure boundary may suffice; another crate is not inherently required.
This makes the acknowledged schema/interchange adoption work specific.

### B3 — P2: scope relationships to both workspace and parent

Evidence: `docs/ARCHITECTURE.md:120`, `:125`;
`docs/DATA_MODEL.md:53`, `:143`.

Membership and authorized reference resolution are specified, but linked-entity
ownership should be an explicit invariant. Access to two workspaces does not
permit combining their records in one operation.

Failure scenario: a user who belongs to workspaces A and B starts a brew in A
using B's equipment settings, or supersedes a reading belonging to another brew.
The first creates a permanent cross-workspace copy; the second corrupts lineage.
The existing recipe CRUD already scopes queries, so this is not a demonstrated
exploit in that code.

Smallest resolution: scope lookups and linked IDs to the operation's effective
workspace and expected parent. Define the explicit exception for shared starter
catalogue entries. Enforce this in shared operations and applicable constraints;
include a negative test where the actor legitimately belongs to both workspaces.

### B4 — P2: distinguish selected historical revision from expected current head

Evidence: `docs/DATA_MODEL.md:23`, `:143`, `:153`;
`docs/ARCHITECTURE.md:238`.

Brews can pin a selected saved revision, while the generic source-change/conflict
examples can be read as requiring that revision to remain the current head.

Failure scenario: explicitly selecting revision 4 while revision 7 exists is
rejected as stale, even though revision 4 is immutable and was intentionally
chosen. Conversely, starting the current recipe after an unseen edit must not
silently substitute the new formulation.

Smallest resolution: distinguish selected saved revision from an optional
expected current head/context. Add acceptance cases for both historical selection
and stale current-selection previews. This needs no branching history.

### B5 — P2: check the complete calculation context at brew start

Evidence: `docs/CALCULATIONS.md:22`, `:37`, `:50`;
`docs/DATA_MODEL.md:143`; `docs/ARCHITECTURE.md:238`.

The general invariant already requires changed-context detection, but the
illustrative start request exposes only recipe and equipment versions.

Failure scenario: a deployment changes formulas or defaults between preview and
start. Recipe/profile revisions are unchanged, but the committed original targets
differ from those the caller accepted.

Smallest resolution: define context identity covering the canonical draft or
selected revision, resolved profile, engine/schema and relevant assumption/policy
versions. A changed context conflicts or requires explicitly accepting refreshed
targets. This makes an existing requirement executable, rather than inventing a
new versioning system.

### B6 — P2: give measurements stable quantity semantics

Evidence: `docs/DATA_MODEL.md:59`, `:110`, `:170`;
`docs/CALCULATIONS.md:17`.

Measurement vocabulary is an acknowledged open adoption choice. Units alone
cannot encode its minimum semantics: hot kettle volume, fermenter volume and
packaged volume are different observations even when all use litres.

Evolution scenario: future feedback compares unlike volumes or gravity readings
and derives an incorrect equipment adjustment from otherwise preserved evidence.

Smallest resolution: define stable quantity identifiers with stage/physical basis,
explicit unit semantics and schema-evolution rules. Preserve optional observation
context when supplied. Sensor integration, calibration workflows and automatic
learning remain deferred.

### B7 — P2: keep transport delivery out of brewing workflow dependencies

Evidence: `docs/ARCHITECTURE.md:104`, `:106`, `:113`;
`docs/CODING_STANDARDS.md:53`; `crates/krausen-auth/src/lib.rs:11`, `:20`, `:26`;
`crates/krausen-auth/src/mcp_session_manager.rs:46`.

The proposed responsibilities are sound, but the current auth crate also owns
MCP session delivery, WebSockets and sync logging. An app-to-auth dependency alone
does not make all of those responsibilities domain primitives.

Evolution scenario: a brewing write gains a WebSocket-manager argument under the
old coding rules, so changing MCP/SSE delivery affects modules below shared policy.

Smallest resolution: explicitly classify retained transport modules as adapter or
infrastructure reuse. Domain mutations record durable changes and return committed
outcomes; optional delivery happens outside their transaction. Move existing
modules when the migration needs it, rather than starting a broad extraction.
Reconcile `README.md:175` and the old copy-without-rewrite plan at the same time.

### B8 — P3: move executable migration ownership to the composition root

Evidence: `docs/ARCHITECTURE.md:111`; `crates/krausen-core/src/db.rs:30`, `:49`.

Core embeds migrations using paths into the server application. This reverse
ownership is hidden outside Cargo dependencies.

Evolution scenario: a maintenance command or migration runner needs reusable
database primitives but must carry the server's source layout and migrations.

Smallest resolution: core owns database connection primitives; the composition
root owns application migration execution through a small explicit entry point.
This is a later refinement, not grounds for another service or deployment.

## Existing adoption checks confirmed by the reviews

These were already disclosed in KRA-5; they are not newly discovered bugs.

- Reconcile historical login-free personal-mode guidance (`README.md:83`,
  `docs/E2E_TEST_PLAN.md:519`) with installation-local authentication. The
  documentation index already gives current architecture precedence.
- Bind pending proof interactions to account, initiating credential/session,
  action, expiry and single consumption while implementing the existing
  credential lifecycle contract (`docs/ARCHITECTURE.md:130`).
- Extend starter acceptance beyond anonymous MCP initialization/hello
  (`docs/E2E_TEST_PLAN.md:470`): shared authorization, revocation, cross-workspace
  references, retries and transaction failure cases need implementation coverage.
- Choose primary-actual selection and correction concurrency together; concurrent
  corrections must not silently create competing current readings. Specify
  retention of actual-formulation amendment evidence (`docs/DATA_MODEL.md:136`,
  `:170`).
- Distinguish preserving and displaying original inputs/results from re-executing
  an old engine. A stored version label alone does not promise the latter
  (`docs/DATA_MODEL.md:99`, `:163`; `docs/CALCULATIONS.md:171`).
- Finalize native/BeerXML mappings and preservation diagnostics without a second
  independently editable XML truth (`docs/CALCULATIONS.md:119`).
- Retire or clearly supersede historical scope and executable instructions in
  `KRAUSEN.md:6`, `:368`, coding standards and the starter plan. Do not reintroduce
  learning/sensor work or mandatory delivery infrastructure through old guidance.

## Boundaries the reviewers supported

- One shared operation and authorization layer for UI and MCP, including draft
  calculations and style comparisons.
- A pure deterministic calculation library and framework-free interchange code.
- Complete immutable brew plans and original targets, separate from editable
  equipment/catalogues, actual formulation, readings and corrections.
- Explicit recipe feedback and revision-checked saves, without live collaboration.
- Shared component/token ownership, with page composition and application policy
  kept separate.
- A modular monolith; no review finding requires microservices, a command bus,
  a generic repository framework or a distributed collaboration system.

## Coverage and limits

Each reviewer read:

- `AGENTS.md`
- `DESIGN.md`
- `README.md`
- `KRAUSEN.md`
- `docs/README.md`
- `docs/ARCHITECTURE.md`
- `docs/DATA_MODEL.md`
- `docs/CALCULATIONS.md`
- `docs/CODING_STANDARDS.md`
- `docs/E2E_TEST_PLAN.md`
- `docs/plans/2026-05-02-app-starter-skeleton.md`
- `docs/design/prototype/AGENTS.md`
- `docs/design/prototype/README.md`
- `docs/design/prototype/fonts/README.md`

Selected manifests, persistence, conversion, database and transport modules were
inspected as boundary evidence. This was not exhaustive source review, protocol
conformance testing, a security test, formula validation or a load benchmark.
The reviews assess extensibility and composition; horizontal deployment scaling
has not been demonstrated. No application tests were rerun for the review-only
report. The prior prototype checks do not establish production acceptance.

KRA-5 should address B1–B6 before adopting the affected foundational contracts,
assign B7 to the application/service migration and retain B8 as a later task.
Existing accepted decisions and reviewed material should be reused. Review the
resulting changes where material; do not repeat unchanged design work.
