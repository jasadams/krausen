# Recipes, revisions and historical brews

Status: adopted KRA-5 implementation contract, 2026-10-31. The requirements
below replace the earlier KRA-2 sketches; independent changed-contract approval
is recorded in [ADOPTION.md](ADOPTION.md). Production persistence remains
implementation work.

Read with [architecture](ARCHITECTURE.md), [calculations](CALCULATIONS.md) and
[interaction design](../DESIGN.md). UI and MCP call the same application
operations and obey the same ownership, validation and history rules.

## Ownership and representation

`krausen-types::brewery` owns the versioned native document contracts:
`RecipeDocumentV1`, `EquipmentSettingsV1`, `BrewPlanV1`, measurement quantity
identifiers and shared command/query payloads. This is a serde-only data boundary;
it contains no persistence, authentication, transport or calculation policy.

`krausen-brewery` owns persistence and domain helpers. Its
`mapping::calculation` module projects a complete, resolved native document into
`krausen-calc` inputs. `krausen-beerxml::native` owns native-to/from-BeerXML
mapping. The shared application layer resolves references, authorizes and
orchestrates those mappings. `krausen-calc` remains pure and independent of
workspace crates. Engine inputs and interchange records are projections, never
alternative editable sources of truth.

The native document preserves formulation and process information beyond the
engine's current model. Do not flatten a recipe to calculated inputs or use
BeerXML as the native schema. Versioned documents are stored as JSON with a few
indexed metadata columns; individual ingredients need not become database rows.
Mapping and validation diagnostics remain structured, not silently discarded.

All persisted records use server-assigned IDs and UTC recorded timestamps.
Actor identity comes from the trusted principal, not an input author field;
source metadata records the authenticated interface/import origin separately.
Examples use
readable fixture IDs. Store original actor IDs even when a display name changes;
account deactivation does not erase authorship. A disconnected actor can be
represented by an unavailable display identity without deleting historical data.

## Workspace and parent invariants

Workspaces own recipes, equipment profiles, custom ingredient entries and brews.
Members can create and edit brewing content; shared administration policy governs
membership/settings. Personal collections are one-member workspaces.

Every lookup and relationship must match the operation's effective workspace
**and its expected parent**, even when the actor belongs to several workspaces:

- A selected recipe revision belongs to the requested recipe and workspace.
- Selected equipment/custom ingredients belong to that workspace. Public starter
  catalogue entries are the explicit read-only exception; recipes copy their
  resolved specifications and source edition/version.
- A brew's recipe, revision and snapshot provenance agree on workspace/parent.
- A correction, actual-formulation amendment, note and primary-actual selection
  refer to evidence belonging to that same brew. A correction also retains the
  same quantity and observation lineage.

Use scoped shared-service lookups and corresponding composite unique/foreign-key
constraints wherever the relational records permit them. JSON references require
explicit service validation. Do not infer authority or lineage from a globally
unique ID alone. Starter data cannot contain links into another workspace.

## Record boundaries and constraints

These are logical record and key contracts. Implementation may choose equivalent
SQL names, but cannot weaken uniqueness, ownership or atomicity on either backend.

| Record | Required content and constraints |
| --- | --- |
| Recipe | Workspace/stable ID, current revision, archive metadata. Unique `(workspace_id, recipe_id)`; mutable metadata has an expected revision. |
| RecipeRevision | Complete versioned native document, monotonically increasing recipe-local revision, author/time, optional summary and canonical content digest. Unique `(workspace_id, recipe_id, revision)`; append-only. |
| EquipmentProfile / EquipmentRevision | Profile identity/current revision/archive metadata plus append-only complete settings revisions and provenance. Referenced revisions cannot be pruned. |
| IngredientEntry / IngredientRevision | Workspace library identity, complete versioned specifications and provenance. Starter entries carry catalogue/edition identity instead of workspace ownership. |
| Brew | Workspace/recipe identity, pinned saved revision, immutable `BrewPlanV1`, lifecycle and mutable aggregate revision. Unique brew identity; recipe/revision links cannot cascade-delete its plan. |
| ActualFormulationRevision | Append-only complete actual formulation/process document, predecessor revision, actor/time and optional reason. Initial effective actual is the planned formulation until an explicit deviation exists. |
| MeasurementEntry | Immutable observed value or retraction, quantity identity, unit/basis, observation and recording times, actor/source, root observation ID and optional superseded entry ID. Unique successor per superseded entry; parent/brew/quantity consistent. |
| ActualSelection / ActualSelectionRevision | One nullable selected observation root per `(workspace_id, brew_id, quantity_id)`, with its own expected revision and append-only selection-change events. The display resolves that root's current unsuperseded entry. |
| ObservationRevision | Versioned qualitative note, actor/time and optional retraction; corrections preserve earlier text. Notes are separate from numeric readings. |
| ImportProvenance | Original immutable file bytes/hash, parsed source estimates, preserved extension data and mapping diagnostics, associated with the imported revision. Access follows its workspace. |
| BrewLifecycleEvent | Append-only from/to status, actor/time and reason, ordered with the brew's aggregate revision. |

Application mutations own the transaction described in
[architecture](ARCHITECTURE.md#transaction-ownership). All persistence helpers
participate in that same context; they do not acquire another connection or
commit independently. Required mutation/revision/retry/change records commit
with their parent aggregate. Notification delivery is outside that transaction.

## Native recipe contract

`RecipeDocumentV1` has `schema_version: 1` and these field groups:

| Field group | Contract |
| --- | --- |
| Identity/content | Title, optional description, optional style identity/edition and brewer notes. Record identity/workspace/revision/author live in the revision envelope. |
| Recipe targets | Intended `batch_size_l` into fermenter; boil duration and process settings. Optional or incomplete values are represented as absent, never invented zeroes. |
| Fermentables | Stable row ID, amount in kg, addition type/stage, resolved specification (name/producer, type, yield and colour), optional catalogue identity/version. |
| Hops | Stable row ID, amount in g, use/stage/contact time and optional temperature, resolved specification including `alpha_fraction` and provenance. |
| Yeast/other ingredients | Stable row IDs, quantity/unit where supplied, resolved specifications including `attenuation_fraction` when known, and provenance. Preserve ingredients the engine does not model. |
| Process | Ordered mash/fermentation steps with stable IDs, explicit time/temperature/volume units, whirlpool/chilling-related recipe settings and retained unsupported process metadata. |
| Equipment selection | Selected live profile ID and explicit recipe-level settings allowed by the calculation contract, including a nullable `efficiency_override_fraction` when deliberately adopted. Profile revision is not pinned merely by saving recipe content. |
| Import/extensions | Immutable source provenance reference and preserved structured extensions. Native edits do not alter original bytes or create independently editable XML. |

Finite numeric validity and calculability are separate from document structural
validity; the shared [calculation contract](CALCULATIONS.md) defines diagnostics.
Field/unit semantic changes require a new schema version or explicit migration.
Do not reinterpret historical values in place. Additive fields need documented
absence behavior and do not authorize new implicit defaults in old brew plans.

### Saved history and equipment changes

Save checks `expected_head_revision` and appends one immutable revision when the
canonical formulation content differs. An unchanged save returns the existing
revision; changing solely presentation save-state does not create history.
Restoring an older formulation deliberately creates a new head revision with
restore provenance, even when that content previously existed. There are no
branches, history merges or live keystroke synchronization.

A recipe draft is not saved history. Recovery copies remain local presentation
state unless a later feature explicitly introduces shared persisted drafts.
Recipe and equipment revisions have independent histories: current profile
updates affect future previews/new brews without creating recipe-content history.
Saved ingredient specifications remain copies; catalogue edits never replace them.

## Brew start and calculation context

Starting a brew requires:

```text
StartBrew {
  recipe_id,
  selected_recipe_revision,
  expected_head_revision?,
  expected_calculation_context,
  request_id
}
```

`selected_recipe_revision` always identifies the immutable plan selected by the
caller. Explicitly starting historical revision 4 is permitted when head 7
exists. `expected_head_revision` is supplied by a start-current workflow and must
match both the selected revision and current head; a subsequent recipe save then
causes a conflict. Historical selection omits this head guard. The UI presents
the selected historical revision clearly; MCP has the same input semantics.

The preview context digest identifies the selected formulation, resolved profile
identity/revision and all effective values, engine/schema versions, assumption/
calculation policy versions and style-reference context. Its encoding is the
shared [calculation context contract](CALCULATIONS.md#calculation-context).
It is a comparison value, not an authorization token. All protected references
are freshly scoped and checked on start.

The application operation obtains its write transaction and locks/checks the
relevant membership and source revisions. It resolves and calculates through the
same pipeline as preview, compares the full expected context, then atomically
writes the brew, plan, history marker and required retry/change records. A changed
profile, engine, default policy or style reference cannot silently change accepted
targets. Return a typed context conflict with a fresh preview; acceptance requires
a new explicit request. Identical scoped request retries return the first
committed brew, subject to current access authorization.

A draft must be saved before starting; a start operation never implicitly saves
or substitutes a different revision. Archived recipe/profile selection is not
eligible for a new start. Existing historical brews remain readable after archive.
A user can restore/unarchive or explicitly choose a replacement profile and
accept a new preview before starting a new brew.

### Immutable `BrewPlanV1`

The plan contains the complete selected native recipe, resolved ingredient
specifications, resolved equipment/process settings (including chilling model
parameters and every applied default), source identities/revisions, calculated
original outputs, assumptions/diagnostics, style reference values/edition/source,
context digest and document/engine/policy versions. The brew owns this bundle;
links to mutable records are supplementary provenance. An implementation may
physically deduplicate immutable payloads only while preserving independent
recovery and referential retention.

Historical reads render stored original inputs and results. They do not rerun
current calculations and label the result original. Recording engine versions
supports interpretation; first-release compatibility promises preserving and
reading these records, not executing every old engine binary. A future
recalculation is a separately labelled comparison that cannot replace the plan.

## Actual formulation, readings and corrections

Actual ingredients/process deviations are structured, complete append-only
`ActualFormulationRevision` documents. The first explicit amendment derives from
the planned formulation; later amendments require its current expected revision
and record their predecessor. Measured gravity does not edit a formulation or
planned target. Applying selected actual changes to a recipe uses a separate
revision-checked operation and creates recipe history; it never mutates the brew
plan or silently tunes equipment.

Each distinct observation creates a new root reading with `observed_at`,
server-assigned `recorded_at`, actor and source. Manual observation time defaults
to entry time and is marked `time_source: recorded_default`; a supplied time is
retained as supplied. Capture automatic metadata without turning these into
required questionnaire steps. Optional instrument, sampling temperature and
method metadata are retained when provided; do not infer missing context.

A correction appends an entry with the same observation root and original
observation time, a new recording time and `supersedes` pointing to the current
entry. It requires `expected_entry_id` and must leave that entry with exactly one
successor. Two corrections racing from the same predecessor yield one success
and one typed conflict. Correcting quantity/stage to a different meaning requires
retracting the wrong observation and recording a new one, not rewriting lineage.
Retracting an observation appends a value-less retraction; missing is not zero.

### Primary actuals remain simple

For each brew/quantity, an `ActualSelection` identifies the reading shown beside
its target. Entering the first actual records and selects a root atomically.
Editing that field corrects the selected root, requiring both the expected
selected entry and selection revision. Recording an additional reading does not
silently change the displayed primary actual. Selecting another reading is an
explicit shared operation with `expected_selection_revision`.

Corrections to the selected root automatically display its current successor;
selection remains attached to the same observed event. Retracting it leaves the
slot missing until explicitly replaced. Clearing the actual field retracts its
selected reading and clears selection in one transaction. Earlier readings and
selection history remain retained. No last-observed-wins or concurrent last-write-
wins rule is permitted. This supports future analysis without a timeline UI now.

### Versioned quantity vocabulary

Quantity identity encodes physical basis and stage, not just a display label or
unit. Initial IDs use these meanings; a future semantic change gets a new ID.

| Quantity ID | Unit and physical basis |
| --- | --- |
| `gravity.original.v1` | Specific gravity of wort into the fermenter before fermentation, referenced to the instrument's stated reference temperature; correction status/measurement temperature may be supplied. |
| `gravity.final.v1` | Specific gravity after fermentation, with the same reference-temperature metadata convention. |
| `volume.pre_boil.v1` | Litres in the kettle immediately before boil; measured liquid temperature is optional metadata and must not be assumed equal to a cold-reference volume. |
| `volume.post_boil.v1` | Litres in the kettle at flameout, before transfer loss; temperature metadata is retained. |
| `volume.into_fermenter.v1` | Litres actually transferred into the fermenter; distinguish observed temperature from calculation assumptions. |
| `volume.packaged.v1` | Litres packaged after fermentation and packaging loss. |
| `temperature.mash.v1` | Degrees Celsius measured in the mash; repeated readings retain observation times. |
| `temperature.fermentation.v1` | Degrees Celsius of the fermenting beer; ambient temperature is a different future quantity. |

The unit/type is validated against the quantity registry. Missing temperature or
reference information remains unknown and comparison/feedback returns an
assumption warning where relevant. It cannot silently normalize measurements.
Custom quantities are deferred; unknown imported quantity definitions are
preserved as provenance/notes, not coerced into a supported quantity. Target
comparison uses a matching quantity/basis; unlike stages cannot be compared as
if they were interchangeable.

## Lifecycle, archive and retention

Starting creates status `started`. Normal terminal transitions are `completed`
or `cancelled`; optional measurements are never a completion prerequisite. A
member may explicitly reopen a terminal brew to `started`, with reason and
expected brew revision. Each transition appends a lifecycle event; reopening
never replaces the original start, plan or existing evidence. Corrections to
actuals/notes remain allowed after completion/cancellation with their own guards.

Ordinary recipe, profile, custom ingredient and brew deletion means archive,
with expected revision and archive event. Archive hides records from ordinary
lists; explicit history reads and export remain available to authorized members.
Unarchive restores visibility, not former content or moved brew markers.
Archiving cannot cascade-delete revisions, source bytes, plan bundles, readings
or corrections. There is no automatic history pruning in the first release.
Membership removal revokes access, not workspace evidence.

Permanent account/workspace erasure is not exposed by this release. It requires
a later explicit policy covering whole-workspace ownership, exports, snapshots
and backups; do not smuggle it into archive or a foreign-key cascade. Do not
promise legal erasure or indefinite external backup retention from this contract.

## Examples and acceptance scenarios

The following native saved revision is a structural fixture, not a validated
brewing reference. Its arrays and explicit nulls are intentional. `process`
retains data not modeled by the engine.

```json
{
  "workspace_id": "workspace-example",
  "recipe_id": "recipe-example",
  "revision": 7,
  "document": {
    "schema_version": 1,
    "title": "Example pale ale",
    "description": null,
    "style": null,
    "batch_size_l": 20,
    "boil_minutes": 60,
    "equipment_profile_id": "equipment-example",
    "fermentables": [{
      "row_id": "grain-1",
      "amount_kg": 4.2,
      "addition": {"stage": "mash"},
      "specification": {"name": "Pale malt", "producer": null, "type": "grain", "yield_fraction": 0.8, "colour_ebc": 5},
      "source": {"scope": "starter", "catalogue_id": "starter-example", "entry_id": "pale-example", "version": 2}
    }],
    "hops": [],
    "yeast": [],
    "other_ingredients": [],
    "process": {"mash_steps": [], "fermentation_steps": [], "whirlpool_minutes": 0, "whirlpool_temperature_c": null},
    "notes": "Yeast details have not been supplied; retain an incomplete formulation.",
    "import_provenance_id": null,
    "extensions": {}
  },
  "saved_by": "user-example",
  "saved_at": "2026-01-01T09:00:00Z"
}
```

An imported revision has this same native document plus an immutable provenance
record containing `format: "BeerXML"`, original bytes/hash, mapping version,
source-application estimates, retained extensions and warnings. Unsupported
information survives independently of a changed native projection. The shared
[interchange contract](CALCULATIONS.md#beerxml-preservation-and-export) governs edited export versus
retrieval of the original file.

For a complete calculable recipe, the brew envelope has the following required
shape. Names in angle brackets denote complete versioned objects, not omitted
production fields or references to mutable entities:

```text
Brew {
  workspace_id, brew_id, recipe_id, selected_recipe_revision,
  status: started, revision: 1, started_at, started_by,
  plan: BrewPlanV1 {
    schema_version: 1,
    recipe: <complete RecipeDocumentV1>,
    equipment: <complete resolved EquipmentSettingsV1>,
    recipe_revision, equipment_profile_id, equipment_revision,
    original_targets: <complete calculation result with quantity identities>,
    assumptions, diagnostics, style_reference: <resolved values or null>,
    engine_version, calculation_schema_version, policy_versions,
    calculation_context
  }
}
```

A reading `reading-1` of `gravity.original.v1` at `1.050 sg` observed at
`2026-01-02T12:00:00Z` is selected in slot revision 1. A correction appends
`reading-2`, root `reading-1`, value `1.048 sg`, the same `observed_at`, a later
`recorded_at`, and `supersedes: reading-1`. The original remains stored. A second
correction expecting `reading-1` conflicts. A later observed reading has its own
root and does not replace the selected actual unless explicitly selected.

| Scenario | Required result |
| --- | --- |
| Explicit revision 4 selected while head is 7 | Start revision 4 with accepted current calculation context; head alone is not a conflict. |
| Start-current preview of 7, then head becomes 8 | Expected-head conflict; preserve caller intent and show fresh context. |
| Profile, engine/default policy or style data changes after preview | Full context conflict; no silently changed original targets. |
| Same start request retried | Return original committed brew after current authorization; no duplicate. |
| Cross-workspace profile or cross-brew correction supplied by multi-workspace member | Reject the relationship; access elsewhere does not authorize mixing records. |
| Archive source recipe/profile or upgrade engine | Existing brew plan/results remain intact and readable. |
| Concurrent correction or primary-selection change | One guarded success; stale request conflicts without competing current values. |
| Apply actual formulation to newer recipe | New revision after current-head check, or conflict; no historical mutation. |
| Failure after any required snapshot/history/retry write | Roll back the entire operation on both database backends. |

## Migration and schema evolution

KRA-6 introduces native recipe/equipment input types and recipe revision
storage/backfill. KRA-14/15 own catalogue/profile storage and operations; KRA-20
owns complete brew-plan types/storage and atomic start; KRA-21 owns evidence,
selection and lifecycle types/storage/operations. These existing feature tickets
implement the constraints above without blocking every module on one migration. Derive list/search metadata from
the native document in the same write transaction; it is not independently
editable recipe content.

Existing BeerXML-backed recipes become an initial native revision with migration
provenance, retained original content and conversion diagnostics. Mark unknown
creator/observed times as unknown; record migration time separately. A conversion
that cannot preserve required native information must remain recoverable and
report diagnostics rather than silently discard or fabricate it. No prior brew
plan or original target is invented by rerunning today's engine over old data.

The earlier `native-v1` BeerXML mapping treated dry-hop `TIME` as days even
though BeerXML 1.0 specifies minutes. Native revisions produced by that mapping
may therefore contain an incorrect dry-hop duration. Existing immutable
revisions are not rewritten. For an affected recipe, compare the duration with
the separately downloadable unedited original when available, then deliberately
edit the duration in minutes and save a new revision using the ordinary
expected-revision check. Original file bytes were not available for legacy
database backfills: an authorized operator can instead inspect the scoped
`recipe_import_provenance.source_record` at revision 1 for mapping version
`native-v1`; its parsed `hops[].time.raw` retains the source value. Compare that
evidence with the current recipe before making any correction. New
`legacy-native-v2` and `beerxml-native-v2` mappings interpret HOP `TIME` in
minutes. No prior record is claimed to have been corrected automatically.

Keep original immutable payloads when adding read migrations or newer schemas.
Readers dispatch by stored schema version, rejecting unsupported versions with a
typed error rather than misreading them as current. New calculations project
from an explicit supported version; upgrades do not rewrite historical plan
semantics. Migration tests cover native/imported save/reload, source archive,
old-plan readability and both SQLite/PostgreSQL paths. Automatic equipment
learning, sensors, custom measurement schemas and old-engine execution remain
future work; the records above preserve the evidence needed for that expansion.
