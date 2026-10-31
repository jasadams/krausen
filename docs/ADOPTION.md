# Architecture adoption and implementation contract

Status: adopted on 2026-10-31 after three independent changed-contract reviews.
Baseline before this resolution: `2c12479`. No application code is implemented
by this documentation change. The post-design delivery audit below preserves
the original ticket set; KRA-7 establishes the transaction boundary while KRA-6
prepares native types/migrations and KRA-8 prepares validation against those types.

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

The delivery lead and three developers reviewed the original tickets after
adoption. Keep the original tickets; no new ticket or sub-ticket may be created
without Jason's explicit approval. Milestones are exit criteria, not a rule that
only one developer or module can be active at a time.

| Ticket | Cohesive delivery scope | Prerequisites after KRA-5 |
| --- | --- | --- |
| KRA-6 | Native recipe/equipment input types, recipe revision persistence, initial native/XML migration mapping and versioned backfill/readers on both databases; future profile/catalogue/brew/evidence storage belongs to its existing feature owner | KRA-7 |
| KRA-7 | Shared app/DbTx/authority/retry/change/outbox foundation, representative existing workspace operation, helper/transport migration and dual-backend PR CI/test harness; no native recipe DTO prerequisite | None |
| KRA-8 | Pure/shared Ready/Incomplete/Invalid validation and diagnostics; canonical native mapping when KRA-6 lands; never invent duplicate types to bypass it | KRA-6 |
| KRA-9 | Native→engine mapping, typed fermentables/process policy, complete shared preview context and independent references | KRA-7, KRA-8 |
| KRA-10 | Authenticated official-SDK MCP and installation browser authorization, durable revocation/proof and dual-version adapter acceptance | KRA-7 |
| KRA-11 | Remaining account/workspace/team/security shared operation extraction and complete MCP/UI parity | KRA-7, KRA-10 |
| KRA-12 | Production token/component/showcase migration followed by the one editable recipe page over shared APIs; this ticket owns the kit, not a new ticket | KRA-6, KRA-7, KRA-9, KRA-10 |
| KRA-13 | Explicit Save/undo/recovery/navigation/conflict integration, account-isolated drafts | KRA-12 |
| KRA-14 | Catalogue storage/shared operations and picker integration; guarded archive/unarchive, resolved copied specs/provenance | KRA-6, KRA-7, KRA-9, KRA-10, KRA-12 |
| KRA-15 | Equipment storage/resolution/shared operations and control integration; guarded archive/unarchive and immutable historical settings | KRA-6, KRA-7, KRA-9, KRA-10, KRA-12 |
| KRA-16 | Bounded library query and approved navigation/list states, reusing production kit; history entry integration with KRA-22 | KRA-6, KRA-7, KRA-10, KRA-12 |
| KRA-17 | Extend the single XML/native mapping into complete source-preserving authorized preview/import/export and screens | KRA-6, KRA-7, KRA-9, KRA-10, KRA-12 |
| KRA-18 | Shared scaling/duplicate preview and guarded apply plus controls; library is an entry point, not a prerequisite | KRA-7, KRA-9, KRA-10, KRA-12, KRA-15 |
| KRA-19 | Current/draft structured process summary and printable recipe, reusable by historical view; no dependency on library screen | KRA-6, KRA-7, KRA-9, KRA-10, KRA-12, KRA-15 |
| KRA-20 | Brew-plan types/storage plus atomic shared start, adapters and start presentation; complete frozen inputs/targets/marker and historical/head/context checks | KRA-6, KRA-7, KRA-9, KRA-10, KRA-12, KRA-15 |
| KRA-21 | Evidence/quantity/selection/lifecycle schema and operations plus optional actual-entry UI; append-only amendment/note/correction/retraction and guarded archive/reopen | KRA-20 |
| KRA-22 | Shared history/comparison queries and views, explicit guarded feedback to recipe and frozen historical print/export reusing KRA-19 | KRA-16, KRA-19, KRA-21 |
| KRA-23 | Aggregate registry-wide UI/MCP parity verification, supplementing per-ticket adapter and backend checks | KRA-11, KRA-13, KRA-14, KRA-17, KRA-18, KRA-19, KRA-22 |
| KRA-24 | Integrated product acceptance, both backends, responsive/a11y/print and packaging evidence | KRA-23 |

KRA-7's infrastructure and representative existing-workspace operation can land
without native recipe models; KRA-6 helpers then use its reviewed DbTx. KRA-6 may
prepare types/migrations while that prerequisite is being implemented, but cannot
merge incomplete helpers or duplicate transaction machinery. KRA-8 may prepare
pure engine validation/references, then consumes the single native contract.

Each developer sees one assigned ticket through tests, review fixes and an open
PR. The lead independently reviews, requires relevant green CI/evidence, merges,
records Done and assigns the next ticket. Preparation on a blocked ticket is
explicit on the board; completion remains gated by its real prerequisites.
No extra tickets are needed to track review, fixtures, components or CI.

Backend work inside catalogue/profile/import/brew tickets may be prepared before
screen integration. Their production acceptance still includes the shared API,
UI/MCP adapters and required presentation; no feature is marked Done for only
half its scope. Accepted module ownership, snapshot and permission contracts are
unchanged. Cancelled audit-created KRA-25–32 are not prerequisites or deliverables.

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
| Correction links another brew/quantity | Rejected by shared parent checks and applicable constraints | KRA-20/21 |
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
