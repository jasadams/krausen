# Recipes, revisions and historical brews

Status: KRA-2 design record. Product contracts below are confirmed. Storage
layout, example field names and unresolved choices are proposals for KRA-5;
this document does not claim that production persistence implements them.

Read with [architecture](ARCHITECTURE.md), [calculations](CALCULATIONS.md) and
the [interaction design](../DESIGN.md). UI and MCP use the same application
operations, ownership, authorization and historical integrity rules.

## Confirmed contracts

Workspaces own recipes and brews. Members can create and edit shared brewing
content; retain creator/editor provenance. Administration follows the shared
workspace policy. Interface choice never changes a user's permissions.

Recipe history is a linear sequence of immutable saved revisions. Explicit
Save publishes a revision; an automatically recovered local draft does not.
Independent edits use expected-revision checks, with conflicts preserving the
caller's draft. Restoring an old revision creates a new current revision.
There are no history branches or live collaborative editing.

Starting a brew permanently pins a selected saved recipe revision, like a tag
in its history. Multiple brews may pin the same revision. The brew also keeps
its complete effective recipe, ingredient, equipment and process inputs,
original calculated targets, assumptions and schema/engine versions. A link to
mutable source records is insufficient. Physical deduplication is permitted
only if the historical data remains independently recoverable.

Equipment profiles remain editable live references for future recipe previews
and new brews. A profile edit alone does not create a recipe-content revision.
Recipe ingredient specifications are resolved copies with source/version
provenance: catalogue edits do not silently change a saved formulation.
Neither profile/catalogue edits nor calculator upgrades change an old brew.

Actual substitutions, quantities and process deviations belong to the brew as
structured data separate from its original plan. Applying selected changes
back to a recipe is explicit, checks its current revision, and creates a new
revision. Measurements do not overwrite planned targets or ingredient inputs.

The ordinary measurement UI is optional actual values beside targets and notes.
Missing stays absent, never zero or an invented target. Capture authorship,
source and timestamps automatically. Support repeated readings underneath;
correcting a reading preserves prior evidence and is distinct from observing
another reading. No stage questionnaire or sensor workflow is required.

## Proposed record boundaries

Names here express responsibilities, not final table or API schemas.

| Record | Responsibility |
| --- | --- |
| Recipe | Stable workspace identity, current revision pointer, visibility/deletion metadata |
| RecipeRevision | Immutable complete native formulation, process metadata, resolved ingredient specifications, profile selection, author/date and optional summary |
| EquipmentProfile | Workspace-owned versioned settings with provenance; future calculations resolve its current revision |
| IngredientEntry | Starter or workspace library identity and versioned specifications; recipe rows retain copies |
| Brew | Stable identity, pinned revision, immutable planned input/target bundle and separately mutable lifecycle metadata |
| BrewActualFormulation | Structured actual ingredients/process changes and their provenance; never replaces the planned bundle |
| Measurement | Observed quantity, unit, observed/recorded times, actor/source and correction linkage |
| Observation | Qualitative notes associated with a brew, with authorship and retained correction provenance |
| ImportProvenance | Source estimates, unsupported information and preservation references accompanying the native recipe |

A saved recipe revision may include its selected profile identity, but historical
targets belong to the brew's frozen resolved bundle. A current recipe's targets
can change as its selected profile changes. Calculation results identify both
recipe and profile revisions so callers can distinguish the contexts.

## Illustrative documents

These abbreviated examples are design sketches. Identifiers, version labels,
timestamps and quantities are fixtures, not production records. Omitted fields
are explicitly not a license to store an incomplete brew snapshot.

```json
{
  "recipe_id": "recipe-example",
  "workspace_id": "workspace-example",
  "revision": 7,
  "title": "Pale ale",
  "batch_size_l": 20,
  "equipment_profile_id": "equipment-example",
  "fermentables": [{
    "row_id": "grain-1",
    "amount_kg": 4.2,
    "specification": {"name": "Pale malt", "type": "grain", "yield_percent": 80, "colour_ebc": 5},
    "source": {"entry_id": "ingredient-example", "revision": 2}
  }],
  "saved_by": "user-example",
  "saved_at": "2026-01-01T09:00:00Z"
}
```

An imported revision uses the same editable native formulation. Additional
provenance may contain `format: "BeerXML"`, retained source-file/extension
references, source-application estimates and conversion diagnostics. There must
not be two independently editable native/XML truths. Exact preservation storage
is unresolved; see [the interchange contract](CALCULATIONS.md#beerxml).

For the recipe above, a brew pins revision 7 and profile revision 3. Its
`planned_inputs` contains the complete recipe and all resolved equipment/process
values, including chilling parameters. Its `original_targets` contains the
actual calculation result, diagnostics and style-reference context. Store
`schema_version` and `engine_version` with that bundle. These names are proposed;
the invariant is a complete immutable historical record, not any specific JSON
nesting. A substituted malt belongs to a separate actual-formulation record.

```json
[
  {
    "measurement_id": "reading-1",
    "brew_id": "brew-example",
    "quantity": "original_gravity",
    "value": 1.05,
    "unit": "sg",
    "observed_at": "2026-01-02T12:00:00Z",
    "recorded_at": "2026-01-02T12:01:00Z",
    "actor_id": "user-example",
    "source": "manual",
    "supersedes": null
  },
  {
    "measurement_id": "reading-1-correction",
    "brew_id": "brew-example",
    "quantity": "original_gravity",
    "value": 1.048,
    "unit": "sg",
    "observed_at": "2026-01-02T12:00:00Z",
    "recorded_at": "2026-01-02T12:05:00Z",
    "actor_id": "user-example",
    "source": "manual",
    "supersedes": "reading-1"
  }
]
```

The correction keeps the original observation time and retains the first entry.
A genuinely later observation gets its own reading identity and observed time;
it does not supersede a previous reading merely because it is newer. Exact
primary-actual selection and correction concurrency remain adoption decisions.

## Atomic start and historical scenarios

Start takes a saved recipe identity/revision, expected profile revision and a
scoped retry identity. The shared operation checks membership and source
versions, resolves inputs, validates/calculates authoritatively, and atomically
persists the frozen bundle, brew marker and required change/retry records.
It must detect source changes between preview and commit. An unsaved draft
cannot silently become a different historical plan; save or explicitly resolve
it before starting. Final request fields and transaction strategy are pending.

| Scenario | Required result |
| --- | --- |
| Recipe/profile changes after preview | Conflict or explicit refreshed calculation; never silently start with different targets |
| Same start request retried | Return the original committed brew; no duplicate |
| Recipe restored from revision 4 while brew pins 7 | New current recipe revision; brew still pins 7 |
| Catalogue/profile/source recipe edited or ordinarily deleted | Referenced historical inputs and original targets remain available |
| Engine upgraded | New previews may change; old brew inputs/targets/version remain unchanged |
| Actual OG corrected | Display corrected actual, retain evidence, leave target unchanged |
| Brew changes applied to a newer recipe | Check current recipe revision and create an explicit new revision or conflict |

## Migration and adoption work

Proposed migration sequence: introduce native revisioned recipe storage and
provenance; convert existing imported records with explicit diagnostics; add
atomic brew snapshot/marker storage; add actual-formulation, readings and
correction records; validate historical recovery and both database backends.
Do not fabricate missing historical inputs or call recalculated estimates the
original targets. Exact DDL/backfill/rollback plans require KRA-5 review.

Before adoption, settle the full canonical schema and version migrations,
supported measurement vocabulary/custom fields, primary-actual selection,
correction/deletion concurrency, brew lifecycle/transitions and corrections,
ordinary archive/delete/restore semantics, workspace/account erasure policy,
retention/export guarantees and treatment of deleted ingredient/profile choices.
Ordinary deletion must preserve brew-referenced evidence; this does not by
itself define permanent-erasure policy. No automatic recipe/profile feedback or
sensor integration is included in the first release.
