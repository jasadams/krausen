# Architecture adoption and implementation contract

Status: adopted on 2026-10-31 after three independent changed-contract reviews.
Baseline before this resolution: `2c12479`. No application code is implemented
by this documentation change. KRA-6 starts the implementation sequence.

## Scope and authority

The product scope is recipes, formulation calculations, polished editing and
historical brew evidence. Hosted and self-hosted installations use authenticated
accounts and the same workspace policy. UI and MCP expose the same shared
operations, including unsaved-draft previews. No separate agent permissions,
live collaboration or automatic recipe/equipment learning is introduced.

The canonical documents are [architecture](ARCHITECTURE.md),
[data model](DATA_MODEL.md), [calculations](CALCULATIONS.md),
[design system](../DESIGN.md) and [coding standards](CODING_STANDARDS.md).
This record binds their decisions and tracks implementation ownership. Historical
KRAUSEN/starter text cannot override them. KRA-1–4 design delivery stays complete;
only materially changed contracts need fresh review.

## Resolution of independent review findings

The [original review](reviews/2026-10-31-boundaries.md) remains an immutable account
of its baseline. The contracts below address its findings; implementation tests
remain required before the respective feature tickets are accepted.

| Finding | Resolved contract | Implementation owner |
| --- | --- | --- |
| B1 transaction ownership | App command owns begin/commit/rollback; all validating reads and writes share one `DbTx`; no nested pool acquisition; authority/source locks and guarded writes; notification after commit | KRA-7, integration acceptance KRA-20/24 |
| B2 native model ownership | `krausen-types::brewery` owns versioned native data; brewery maps native→engine; BeerXML `native` maps interchange; calc is standalone pure code | KRA-6/7/9/17 |
| B3 relationship tenancy | Every protected linked ID is workspace- and parent-scoped; shared starter entries are the explicit public exception | KRA-6/7/14/20/21 |
| B4 historical start | Immutable selected revision is distinct from optional expected current head; explicit historical starts need not restore or match head | KRA-20/22 |
| B5 calculation context | Shared canonical digest covers complete resolved inputs and engine/schema/policy/style context; recompute/check at start | KRA-7/9/20 |
| B6 quantity semantics | Stable versioned quantity IDs carry physical stage/basis and units; corrections retain evidence; explicit guarded primary selection | KRA-6/21/22 |
| B7 transport dependencies | Shared mutations accept no WebSocket/SSE manager; auth's legacy transport modules are adapter/infrastructure migration work | KRA-7/10 |
| B8 migration ownership | Core owns connections/transactions; composition root owns migration execution; initial schema work may use existing runner before extraction | KRA-7 |

Auth/pending proof, sensitive-read visibility, retry retention and membership races
are settled in architecture. Archive/retention, lifecycle, measurement lineage
and schema evolution are settled in the data model. Incomplete saves, fermentable
stages, efficiency, water/bitterness assumptions, import preservation and scaling
are settled in calculations. Supporting import/scale/print/conflict/actual-entry
interactions are specified in DESIGN.md using the approved kit.

## Delivery order and acceptance ownership

This is a dependency map, not an instruction to execute all tickets at once.
Implementation descriptions must link these canonical contracts. Tickets can
choose SQL names and Rust lifetimes without changing their semantics.

| Ticket | Deliverable and exit evidence | Prerequisites after KRA-5 |
| --- | --- | --- |
| KRA-6 | Native V1 documents and equivalent migrations; immutable revisions/plans/evidence, composite tenancy constraints, preserved import/backfill diagnostics and versioned readers; save/reload/archive/recovery on both databases | None |
| KRA-7 | App commands/queries and trusted context, transaction-capable helpers, authority/source guards, scoped durable retries, typed outcomes, post-commit delivery; remove transport policy and extract migration runner | KRA-6 |
| KRA-8 | Ready/Incomplete/Invalid diagnostics and shared field validation; structurally valid incomplete saves, malformed rejection, no stale targets | KRA-6 |
| KRA-9 | Versioned calculation/mapping policy, grain/extract/late-boil handling and reference arithmetic/model tests; complete preview context | KRA-8 |
| KRA-10 | Installation-local browser OAuth, durable credential/revocation checks, authenticated official-SDK MCP adapters; retire protected anonymous/personal bypass | KRA-7 |
| KRA-11 | Existing profile/workspace/team/security operations call app and are exposed equivalently by MCP; proof, role and sensitive-read cases | KRA-7, KRA-10 |
| KRA-12 | Approved editable recipe components use shared preview/save operations; missing/invalid states and head/style context remain honest | KRA-7, KRA-9, KRA-10 |
| KRA-13 | Explicit Save, undo/recovery and dirty navigation; conflict keeps draft; isolate recovery by account/workspace/recipe | KRA-12 |
| KRA-14 | Durable starter/custom ingredient library, copied specs and provenance, scoped shared operations and picker | KRA-12 |
| KRA-15 | Versioned equipment profiles, live future-preview resolution and explicit overrides; history unchanged | KRA-7, KRA-9, KRA-10, KRA-12 |
| KRA-16 | Library/search/history navigation with approved states/components, shared bounded queries | KRA-12 |
| KRA-17 | Source-preserving native BeerXML preview/import/export with explicit ambiguity/loss handling and source download | KRA-7, KRA-9, KRA-10, KRA-12 |
| KRA-18 | Linear ingredient scaling/duplication preview, recalculated context/capacity and explicit guarded apply; no target-match promise | KRA-12, KRA-15, KRA-16 |
| KRA-19 | Printable current/saved/historical process summary with context and assumptions; equivalent structured export | KRA-15, KRA-16 |
| KRA-20 | Atomic brew start and immutable plan/targets/marker; historical/head/context distinction, scoped retry and source checks | KRA-7, KRA-9, KRA-10, KRA-12, KRA-15 |
| KRA-21 | Optional actuals/notes, append-only amendments/corrections, guarded selections/retractions and lifecycle events | KRA-20 |
| KRA-22 | Brew history and planned-versus-actual comparison using stored plans and selected readings, no silent recalculation | KRA-16, KRA-21 |
| KRA-23 | Cross-interface operation registry/authorization/result parity across all delivered capability families | KRA-11, KRA-13, KRA-14, KRA-17, KRA-18, KRA-19, KRA-22 |
| KRA-24 | Integrated recipe-to-brew/historical acceptance, both databases, packaging and deployment checks | KRA-23 |

KRA-6 accepts persistence/helper tests without claiming the public application API
is finished. KRA-7 accepts representative shared operations and reusable boundaries;
later feature tickets extend its operation registry, never bypass it. KRA-8/9 may
develop pure validation/calculation work alongside the app migration after the
canonical types exist. Resolve a prerequisite through its reviewed implementation,
not by duplicating its functionality in a caller.

## Acceptance scenarios

Tests belong to the implementation owner, using real shared operations and both
SQLite/PostgreSQL where persistence is involved. These are required future tests,
not checks claimed to have run during architecture adoption.

| Scenario | Required outcome | Owner |
| --- | --- | --- |
| UI and MCP save from head 7 concurrently | One new head; stale writer receives typed conflict and retains draft | KRA-7/12/23 |
| Failure after brew/history/retry write but before commit | No partial brew, marker, evidence or success result | KRA-7/20/24 |
| SQLite helper called inside a write transaction | Uses the held connection, never waits for its own pool lease | KRA-7 |
| PostgreSQL concurrent authority/source mutation | Defined lock order and guarded checks; retryable rollback rather than partial success | KRA-7 |
| Same request retried after lost response | Original result/resource after current auth; no second write or post-self-write revision conflict | KRA-7/20 |
| Same request key with different semantic payload | `RequestIdentityConflict`, no new mutation | KRA-7 |
| Result expired but retry tombstone retained | `ResultExpired`, no re-execution | KRA-7 |
| Membership/credential revoked during other operation | Serialized authority boundary; calls ordered after revocation fail through both adapters | KRA-7/10/11 |
| User belongs to A and B, supplies B profile to A brew | Rejected despite both memberships; no copied foreign data | KRA-7/20 |
| Correction links another brew/quantity | Rejected by shared parent checks and applicable constraints | KRA-6/21 |
| Explicit historical revision 4 selected while head is 7 | Start 4 with accepted current equipment/context; no restore required | KRA-20 |
| Start-current preview at 7, then head becomes 8 | Head conflict; no silently substituted plan | KRA-20 |
| Profile/default/engine/style context changes between preview and start | Context conflict/refreshed acceptance; old brews remain unchanged | KRA-9/20 |
| Catalogue entry edited or any ordinary source archived | Copied recipe specs and frozen brew inputs/results remain available | KRA-6/14/15/20 |
| Two corrections/selectors race on same predecessor/slot | One success, stale caller conflict; no competing current actuals | KRA-21 |
| Extra reading, missing temperature or different volume stage | No implicit primary replacement or false basis normalization | KRA-21/22 |
| Completed brew reopened or actual edited | Guarded event/amendment retained; original plan unaffected | KRA-21 |
| Unfinished recipe saved, then Start requested | Save preserves missing data with diagnostics; Start requires Ready | KRA-8/12/20 |
| Late-boil sugar, extract-only and scaled fixed losses | Versioned type/timing rules and independently checked references | KRA-9/18 |
| Imported XML has unknown data or ambiguous colour/efficiency | Original recoverable; preview choice/diagnostics explicit; edited export never restores stale source values | KRA-17 |
| Notifications lost or connection drops after commit | Reconnect/refetch and durable request identity recover truth | KRA-7/10/13 |
| New account uses browser after another logs out | Cannot see the other account's recovery draft or credential state | KRA-10/13 |
| Schema/engine upgraded | Original versioned inputs/results readable; current recalculation separately labelled | KRA-6/9/22/24 |

## Deliberate deferrals

- Permanent account/workspace erasure and backup retention policy: no first-release
  destructive endpoint. Archive is reversible and cannot prune referenced evidence.
- Sensors, automatic fitting/learning, custom quantity schemas, Imperial UI and
  additional interchange formats: later features, not dependencies of recipe work.
- Additional fermentable stages and hop-at-cooled-temperature models: preserve
  unsupported input and disclose unavailable affected targets until a later
  versioned policy supports them. Do not fake Ready or silently approximate them.
- Historical engine binary replay: first release guarantees preservation and
  readable stored results, not indefinite execution of obsolete code.
- Multiple application processes: initial deployment acceptance is one process per
  installation, with either database. Future horizontal deployment must prove
  shared credential/retry/authority state and adapter routing; no process cache may
  become the source of correctness in the meantime.

These deferrals do not change the required shared application interface or justify
new services/frameworks. Migration extraction and transport cleanup have named
owners above and are not forgotten review findings.

## Changed-contract review and verification

Three independent reviewers assessed the revised contracts and guidance against
`2c12479`; none edited the material they reviewed. Written verdicts:

| Review | Final verdict and scope |
| --- | --- |
| Architecture/scaling | Approved KRA-5 documentation adoption; B1/B2/B7/B8 resolved, KRA-6→7 sequencing sensible, no blocking findings. |
| Interface/security | Approved changed contracts; same-workspace/parent constraints, shared operations, credential/proof/revocation and durable retry/transaction boundaries coherent; no blocking findings. |
| History/calculations | Approved changed domain/calculation contracts; B2–B6 resolved and arithmetic references independently recomputed; no blocking domain or arithmetic findings. |

The one nonblocking finding was a renamed BeerXML heading anchor; it is fixed.
The supporting dependency diagram was clarified to use consumer→dependency arrows
consistently, eliminating misleading reverse data-flow arrows to shared types.
Prior product/visual approvals stand. These verdicts approve implementation
contracts, not implemented behavior, calibrated model accuracy or protocol/database
conformance; those remain the acceptance work assigned above.

Documentation verification includes local link/heading integrity, public-file
checks and consistent ticket dependencies. No source changes or production test
results are implied by that verification. Implementation acceptance remains with
the mapped tickets.
