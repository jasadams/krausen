# Calculation and interchange contracts

Status: KRA-3 design record. Confirmed product requirements are identified below;
remaining formula, validation and storage decisions require KRA-5 adoption.
Existing Rust behavior and prototype output are evidence, not acceptance of all
current assumptions or proof of general recipe support.

Read with [architecture](ARCHITECTURE.md), [data model](DATA_MODEL.md) and
[design system](../DESIGN.md). UI and MCP share input resolution, validation,
calculation, import/export and historical rules.

## Confirmed product requirements

- First-release workflows are metric: litres, kilograms/grams and degrees
  Celsius, with explicit unit metadata. Convert supported interchange units at
  the boundary. Imperial display/input is deferred.
- Batch size means wort volume **into the fermenter**. A 20 L recipe targets
  20 L there; kettle, water and packaged volumes are different quantities.
- Editable equipment profiles drive derived volumes and other targets. Current
  profile updates automatically affect future previews/new brews without
  creating a recipe-content revision solely for that change.
- Starting a brew checks the recipe/profile calculation context and freezes all
  effective inputs, assumptions, original outputs and schema/engine versions.
  Later edits or engine upgrades do not change its original targets.
- BeerXML is the first-release interchange format. Preserve information outside
  the calculation model and expose missing-input/assumption/conversion warnings.
  Source-application estimates remain reference data; current Krausen targets
  come from its shared deterministic engine and selected equipment context.
- Live style ranges cover OG, FG, ABV, IBU and colour. They are guidance and
  never prevent saving an otherwise valid recipe.

## Shared calculation boundary

Keep `krausen-calc` pure and deterministic. UI and MCP both invoke the same
shared application calculation query for unsaved recipe previews. It resolves
the authorized equipment/catalogue context, validates inputs and returns targets,
assumptions, diagnostics, style comparisons and context/engine versions. Calling
the query does not save a recipe revision. UI edits refresh this query; agents
can evaluate the same draft through MCP.

Save/start/import operations use the same resolution, validation and calculation
pipeline, with current-context checks at persistence boundaries. Client-supplied
outputs are not authoritative saved targets. There is no UI-exclusive brewing
calculation capability or separate JavaScript calculator.

The browser-local design prototype runs the Rust engine through WASM because it
has no application backend. This demonstrates the engine and visual feedback;
it does not prescribe a separate production preview path.

Results need resolved inputs/context identifiers, derived values, assumptions,
diagnostics and engine/schema versions. Draft previews must distinguish missing
or invalid inputs from zero and must clear stale estimates when they cannot be
calculated. Proposed outcome shapes are ready/incomplete/invalid with field
diagnostics; exact types and the saveability of incomplete recipes remain
adoption decisions. Starting a brew must not silently invent required inputs.

Store full-precision canonical values and round for presentation. Current Rust
uses gravity points internally and SG at display/interchange boundaries;
percentages such as attenuation/alpha become fractions in engine inputs.
Exact native representations, rounding digits/tolerances and alternate gravity
scale support are still to be adopted.

## Equipment and volume example

The current engine's water chain is in
[`volumes.rs`](../crates/krausen-calc/src/volumes.rs). For an illustrative 20 L
batch, 2 L transfer loss, 3 L/h boil-off, one-hour boil, 4.2 kg grain at
0.8 L/kg absorption and zero deadspace:

| Quantity | Calculation | Result |
| --- | --- | --- |
| Post-boil kettle volume | 20 + 2 | 22 L |
| Pre-boil volume | 22 + 3 × 1 | 25 L |
| Grain absorption | 4.2 × 0.8 | 3.36 L |
| Total water | 25 + 3.36 + 0 | 28.36 L |
| Sparge water, if mash water is 15 L | 28.36 − 15 | 13.36 L |

These calculations explain the current all-grain engine, not a validated preset
for every brewery. Thermal shrinkage, deadspace/loss boundaries and mash-water
derivation need explicit decisions. Current `mash_water_l` is an input; the
product requirement for equipment-driven water planning does not settle a mash
thickness or equipment-specific formula.

Changing profile boil-off from 3 to 4 L/h changes this preview's pre-boil volume
to 26 L and total water to 29.36 L. A brew already started using the previous
profile keeps 25 L and 28.36 L as its original targets. A new start based on the
old displayed profile revision must refresh or conflict before committing.

Resolve and snapshot all effective equipment values, including efficiencies,
losses, dimensions, boil temperature and chilling parameters. Defaults must be
identified as assumptions. Source profile identity/revision alone cannot recover
historical inputs after its deletion or an engine-default change.

## Existing implementation gaps

| Area | Current behavior/evidence | Required design work |
| --- | --- | --- |
| Fermentables | `Recipe::total_grain_kg` sums all fermentables; no extract/sugar addition semantics | Define supported types/stages and separate absorption and efficiency treatment |
| Efficiency | Equipment `mash_efficiency` means fraction of theoretical grist extract reaching the kettle; imported recipe efficiency is not resolved into it | Adopt explicit efficiency boundary and source/override precedence |
| Validation | Some imported row specifications default to zero; duration conversions truncate/clamp | Add finite/per-row/physical validation and structured diagnostics without silent data loss |
| Mash water | Explicit engine input; BeerXML conversion sums infusion amounts | Decide manual/derived modes, incompatible equipment handling and negative-sparge behavior |
| Whirlpool | Temperature is recorded but not used by current utilisation model | Define temperature/chilling/stand assumptions before claiming accurate general late-hop estimates |
| Chilling | Engine has natural, forced and counterflow models with defaults | Expose assumptions and validate settings; prototype near-instant fixture is illustrative |
| Native settings | BeerXML adapter cannot express all Krausen process/equipment inputs | Define native storage and warning/preservation behavior |

Sources: [`recipe.rs`](../crates/krausen-calc/src/recipe.rs),
[`equipment.rs`](../crates/krausen-calc/src/equipment.rs) and
[`convert.rs`](../crates/krausen-beerxml/src/convert.rs).
Until these are resolved and implemented, the malt/grain prototype must not be
presented as validated support for extract or mixed recipes.

## BeerXML

Use one editable native recipe representation. Import preview explains parsed
values, unsupported fields, assumptions, selected equipment and current derived
targets before committing. Import/export are shared application operations in
both interfaces. BeerJSON and additional formats are deferred.

Preserving source information and exporting an edited recipe are distinct
guarantees. The current parser/serializer must not be called literally lossless.
The retained original-file/extension mechanism, unknown fields/attributes and
envelope handling, source-estimate storage and native-to-BeerXML mapping remain
technical design choices. Export must disclose native information the target
format cannot express rather than silently dropping it. Re-exporting the
original file is not an export of subsequent native edits.

Existing conversion examples: BeerXML hop amount 0.02835 kg becomes 28.35 g;
alpha 5.5% becomes 0.055; yield 80% becomes 36.9712 PPG using the adapter's
46.214 sucrose reference. These are current boundary conversions, not a mandate
to expose imperial PPG as the native/UI specification. BeerXML colour ambiguity,
unsupported hop uses, missing equipment and first-wort/mash-hop approximations
need visible diagnostics; successful parsing alone is not calculation readiness.

## Style comparisons

Compare full-precision results with inclusive reference bounds. Return
below/within/above status with values, units and style identity/edition/source
through shared UI/MCP results. Preserve relevant reference context for brew
history. No known style means no invented band; missing estimates have no
marker. Batch volume has no style band. Reveal additional digits if ordinary
rounding would make an outside value appear equal to a boundary.

The prototype's limited reference data lives in
[`styles.js`](design/prototype/styles.js), with BJCP 2021 identities/source URLs
and SRM bounds converted to EBC using the existing 1.97 factor. It is not a
complete style catalogue. The shared `StyleRange` renders both visible text and
accessible descriptions; colour alone must not communicate status.

## Scaling and reference cases before adoption

Scaling must provide a preview and explicitly apply a new recipe revision via
the shared operation, with expected-revision checks. Exact policy is unresolved:
linear ingredient scaling alone cannot promise identical bitterness or water
requirements when fixed losses, utilisation, equipment capacity and chilling
change. Do not silently scale geometry, timing or equipment constants. Freeze
the adopted policy/version with calculated context where relevant.

| Reference case | Evidence needed before general release |
| --- | --- |
| All-grain example | Independently checked volume/extract/gravity/colour and boil/late-hop results with explicit equipment |
| Extract and sugar | No mash-efficiency or grain-absorption penalty for non-mashed additions; supported stages documented |
| Mixed fermentables | Type-specific extract and water contributions; explicit process assumptions |
| Incomplete/non-finite/negative inputs | Field diagnostics, no stale or misleading outputs, defined save/start behavior |
| Equipment changed/deleted | Updated future preview, revision conflict on stale start, immutable historical targets |
| Import missing/unknown fields | Preservation evidence and warnings; no manufactured certainty |
| Export edited native recipe | Correct supported mappings plus declared unsupported native data |
| 20 L to 40 L scaling | Adopted policy accounts for fixed losses/utilisation; preview changes traceable |
| Style boundary and no style | Inclusive full-precision comparison, honest rounding and missing-data states |
| UI versus MCP | Same canonical input/context produces identical output, diagnostics and authorization |

KRA-5 must settle the unresolved semantics above, final canonical fields/units,
validation ranges, authoritative preview/save/start contracts, version migration,
worked reference values and numerical tolerances. These documents preserve
confirmed decisions while keeping unreviewed formula choices explicit.
