# Calculation and interchange contracts

Status: adopted KRA-5 implementation contract, 2026-10-31. Independent
changed-contract approval is recorded in [ADOPTION.md](ADOPTION.md). Existing
Rust behavior and prototype output do not establish that the target is
implemented or validated for every process.

Read with [architecture](ARCHITECTURE.md), [data model](DATA_MODEL.md) and
[design system](../DESIGN.md). UI and MCP share resolution, validation,
calculation, import/export and historical rules.

## Ownership and shared execution

`krausen-types::brewery` owns the versioned native formulation and operation
contracts as serializable data. `krausen-brewery::mapping::calculation` projects
resolved native inputs into engine inputs. `krausen-beerxml::native` owns the
native/BeerXML mappings. Engine inputs and XML documents are projections, never
second editable recipes. `krausen-calc` remains pure and deterministic without
database, web, application or workspace-crate dependencies.

Both interfaces invoke the same application draft-calculation query. It resolves
authorized references in one workspace, validates, projects and returns targets,
assumptions, diagnostics, style comparisons and context versions. Preview does
not save a revision. Save/import/start use the same pipeline; client-supplied
targets are not authoritative persisted outputs. The backend-free prototype's
local WASM engine is a demonstration detail, not a production calculation bypass.

## Units and validity

Native inputs use litres, fermentable kilograms, hop grams, degrees Celsius,
minutes, EBC and SG. `yield_fraction`, `alpha_fraction`, `attenuation_fraction`
and mash efficiency are fractions; UI percentages convert at the boundary.
Durations retain fractional minutes. Engine gravity points are `(SG - 1) × 1000`.
Batch volume means wort into the fermenter, before fermentation; kettle and
packaged volumes are different quantities.

| State | Save | Preview/start |
| --- | --- | --- |
| Ready | Allowed with expected revision | Available targets; start checks accepted context |
| Incomplete | Allowed with explicit diagnostics and preserved absent/unmodelled data | Affected targets unavailable; start unavailable until Ready |
| Invalid | Rejected with field/row diagnostics | No stale or fabricated targets; start rejected |

Missing required values, an unfinished zero addition quantity, an empty fermentable
bill, or a preserved unsupported process are Incomplete. Malformed/non-finite
numbers, negative quantities, invalid fractions and contradictory schedules are
Invalid. No hops legitimately means zero IBU; no selected style is not an error.
Each diagnostic carries a stable code, field/row path, severity and affected
targets. Unavailable IBU does not hide independently calculable water, but the
overall state is not Ready.
Validation exposes original gravity separately from final gravity and alcohol:
missing wort attenuation affects the latter two, not original gravity or water.
Yield affects gravity and bitterness; missing colour affects colour alone.

Present batch volume and Ready addition quantities must be positive. Yield and
mash efficiency are in `(0, 1]`; attenuation and alpha are in `[0, 1]`; colour,
losses, absorption and durations are nonnegative. Hop exposure cannot exceed the
boil. Kettle diameter/opening are positive and opening cannot exceed diameter.
Forced chilling requires positive rate and plateau below boil temperature;
counterflow requires positive flow. Zero-rate fallback defaults in the current
engine must not silently replace invalid inputs. Explicitly selected defaults
carry assumption provenance and resolve to concrete values before calculation.

Retain finite binary64 values without display rounding. Defaults: SG 3 decimals,
ABV/IBU/EBC 1 decimal, volume 2 decimals; editable inputs retain entered precision.
Reveal additional precision near style boundaries. Rounded text never drives
comparison, persistence or context identity.

## Calculation policy V1

Policy `krausen-formulation-v2` is the current native implementation target. V2
extracts the same formulas, defaults and context envelope into one shared
transaction-aware resolver so preview and brew start use one producer; it does
not change calculation behaviour. Policy/source identity is versioned V2, so an
accepted pre-V2 digest is rejected with a context conflict and a refreshed
preview. Store policy, engine and native/result schema versions with contexts and
brew plans. Numerical rule changes require a new policy or engine version and
updated reference fixtures; old brew targets never change.

### Fermentables and efficiency

V1 supports mashed grain and dissolved malt extract/sugar. Mashed grain receives
mash efficiency and absorption. Extract/sugar receives factor 1 and no grain
absorption. Steeped grain is preserved as unmodelled in V1, not silently treated
as fully mashed grain.

Theoretical potential is `yield_fraction × 385.674500456` points·L/kg, preserving
the current boundary conversion `46.214 PPG × 8.345404`. PPG is not native storage.
The native V1 validation vocabulary uses `grain` with `mash`, and `extract` or
`sugar` with `boil`; boil `time_minutes` is remaining boil time. Hop stages are
`boil`, `whirlpool`, and `dry_hop`. Other source strings remain preserved and
incomplete until an explicit mapping is accepted; validation does not reinterpret
BeerXML stage or kind strings implicitly.
Grain extract reaching the kettle is `kg × potential × mash_efficiency`;
extract/sugar contributes `kg × potential`. Efficiency is the fraction of mashed
grain's theoretical extract reaching the kettle. Homogeneous transfer losses
remove wort and extract together, not a second efficiency penalty.

Mashed grain contributes at pre-boil. Extract/sugar supports boil-start and
late-boil events by remaining minutes, including zero at flameout. Late additions
contribute final OG but not earlier pre-boil or boil gravity. V1 assumes negligible
solid-addition volume displacement and reports that assumption. Post-boil and
fermentation additions remain preserved with `unsupported_fermentable_stage`;
affected targets are unavailable until explicitly resolved or a later policy
supports them. KRA-9 owns stage expansion; import cannot silently move additions.

The current selected profile supplies efficiency unless an explicit recipe
`efficiency_override_fraction` is present. An override has visible source provenance.
If BeerXML supplies efficiency, import preview requires an explicit choice:
retain it as a recipe override, or use the selected profile. Never silently drop
the source value or overwrite the reusable profile. Preserve source efficiency
regardless. Other explicit recipe equipment/process overrides follow the same
override-over-profile precedence and context capture.

### Water and volumes

V1 plans all volumes on a 20 °C reference basis, with no separate thermal-shrinkage
multiplier. Profile boil-off/loss values are entered or calibrated on that basis;
show the assumption. Hot observed volumes retain their physical/temperature
context and are not silently compared as reference volumes. Thermal correction
is a versioned later expansion.

For batch `B`, post-boil transfer loss `L`, boil-off `E` L/h, boil duration `T`
minutes, mashed-grain mass `G`, absorption `A` L/kg and pre-boil deadspace `D`:

```text
post_boil_kettle_l = B + L
boil_off_l = E × T / 60
pre_boil_l = post_boil_kettle_l + boil_off_l
grain_absorption_l = G × A
total_water_l = pre_boil_l + grain_absorption_l + D
```

`transfer_loss_l` is wort lost after boiling; `pre_boil_deadspace_l` is additional
unrecoverable water before boiling. Neither includes the other. Mapping legacy
`kettle_deadspace_l` requires an explicit assumption and retained source value,
not double-counting that loss as transfer loss.

Choose `manual` mash water or `profile_thickness`. The latter derives
`mash_water_l = mashed_grain_kg × profile_mash_thickness_l_per_kg`, with positive
thickness. With mashed grain, `sparge_water_l = total_water_l - mash_water_l -
kettle_top_up_l`; explicit kettle top-up defaults to zero. Negative sparge is
Invalid, never clamped; zero is no sparge. Without mashed grain, mash/sparge are
zero and all water is kettle water. Allocations must balance to total water.
Post-boil dilution is unsupported V1, not another meaning of kettle top-up.

### Gravity, attenuation and colour

Let `P(t)` be dissolved points·L and `V(t)` its wort volume:
`SG(t) = 1 + P(t) / V(t) / 1000`. OG uses total extract at post-boil kettle volume;
homogeneous transfer preserves that SG into the fermenter. The current engine's
all-fermentables-as-grain behavior must be corrected in KRA-9.

One explicit apparent attenuation applies to the complete wort; V1 does not
infer separate sugar/yeast fermentabilities. `FG = 1 + (OG - 1) × (1 - attenuation)`.
`ProcessV1.apparent_attenuation_fraction` owns this value independently of yeast
catalogue estimates. Missing is incomplete; legacy documents deserialize it as absent.
`ABV% = 76.08 × (OG - FG) / (1.775 - OG) × (FG / 0.794)`. V1 requires
`1 ≤ FG ≤ OG < 1.775`; formula-domain failures or non-finite outputs are Invalid.
These are estimates, not measured alcohol.

Morey uses post-boil kettle volume: MCU is summed `kg × 2.2046226 × lovibond`
divided by `litres × 0.26417205`; `SRM = 1.4922 × MCU^0.6859`; `EBC = SRM × 1.97`.
For positive ingredient colour, `lovibond = ((EBC / 1.97) + 0.76) / 1.3546`.
Explicit colourless ingredients contribute zero MCU. Preserve known extract
colour; unsupported colour processes receive diagnostics rather than certainty.

### Bitterness, timing and chilling

V1 retains Tinseth boil utilisation and the temperature-weighted post-flameout
model in [`mibu.rs`](../crates/krausen-calc/src/mibu.rs). It is a documented
empirical approximation, not exact physical prediction. Dry hops contribute zero
modelled isomerisation IBU. First-wort/mash-hop uses are preserved with
`unsupported_hop_use` and unavailable IBU until explicitly remapped or supported.

For each hop, split its contact interval at fermentable events, beginning at
that hop's entry. Other hops' entry times do not change this partition. Each
segment uses the mean of its start/end SG with only extract present then. For hop contact ages
`a0` and `a1` minutes:

```text
bigness = 1.65 × 0.000125^(mean_segment_SG - 1)
segment_utilisation = bigness × (exp(-0.04 × a0) - exp(-0.04 × a1)) / 4.15
boil_IBU = sum(segment_utilisation) × hop_g × alpha_fraction × 1000 / batch_l
```

Events contribute after their boundary. Zero-time exposure adds no boil IBU.
One segment without late additions recovers the current mean-gravity/time-factor
rule. Fractional times are retained. Post-flameout uses final kettle SG for
bigness and each hop's remaining alpha after boil exposure in the existing
cooling integral. Numerical changes from the legacy engine are policy-versioned.

Whirlpool charges are at flameout in V1, with the declared stand duration. The
current model starts at profile boil temperature and naturally cools during the
stand; a supplied hold temperature is maintained once reached. It does not model
a hop first added after cooling to that temperature. Preserve that different
schedule with an unsupported-process diagnostic rather than silently treating it
as flameout. Snapshot geometry, boil point, stand/hold and chilling parameters.
Resolve natural, forced and counterflow settings before calling the pure engine.
Presets are visible assumptions, not certified equipment. `None` means explicit
natural cooling, not zero post-flameout utilisation. Prototype near-instant
chilling is only a fixture. The stale `recipe.rs` comment claiming ignored hold
temperature does not describe the current `mibu.rs` code.

## Calculation context

`context_id` is the shared application's lowercase SHA-256 digest of a
server-normalized RFC 8785 canonical JSON context envelope V1. Its sole producer
is `krausen-app::formulation`. The envelope includes effective workspace, selected
native content/revision, copied ingredient inputs, complete resolved profile and
revision, overrides/assumptions, native/result schema versions, engine/policy
versions, and style identity/version/reference values. No rounded display values
enter it. Normalize optional fields to the schema's explicit null/absence rule,
normalize negative zero to zero, use finite binary64 numbers and preserve array
order/stable row IDs. Reject integers outside interoperable exact range or encode
identifiers as strings. JCS serialization is shared code, not ordinary map-order
JSON. Engine version includes its released version and source/build identity;
changes to formulas/defaults cannot reuse that identity.

[Architecture](ARCHITECTURE.md) defines transactions. Canonical serialization
follows [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785), checked 2026-10-31;
SHA-256 is a content identity here, not a signature or authorization credential.

Start recomputes context under its application transaction; the digest is not
authorization proof. The shared transaction-aware resolver is the sole producer,
and start compares the complete accepted context before writing. Engine/default/
profile/source/style changes require a context conflict or explicit acceptance of
a refreshed preview. A selected historical revision and optional expected current
head are distinct; see [data model](DATA_MODEL.md). Freeze complete effective
inputs, assumptions, original outputs/diagnostics, style values, context ID and
versions on the brew.

V1 guarantees historical preservation/display, not indefinite execution of old
engine binaries. Recalculation is an explicit current-policy comparison and
cannot replace original targets. Stored correction/bench-sheet factors belong
to the original result, never silent regeneration under a newer engine.

## BeerXML preservation and export

BeerXML is the first-release format; others are deferred. Import preview displays
native values, unsupported/missing fields, unit assumptions, source estimates,
profile selection, efficiency choice and current targets. Parsing is not readiness;
an Incomplete native recipe can be imported while source information is retained.

Retain original file bytes as an immutable attachment with checksum, format/version
and provenance. Unmapped fields/attributes/envelope extensions are retained in a
sidecar linked to stable imported entity IDs and source paths. These are
preservation evidence, not editable XML truth alongside the native formulation.

Export renders current native values. Emit retained fields/fragments only where
their stable mapping remains valid; enumerate omitted/ambiguous fragments and
native-only data in export diagnostics. Require acknowledgement of lossy export
before download. Never restore original XML values over current native edits.
Original-file recovery is separate and labelled as unedited import. The current
parser/serializer is not literally lossless; KRA-17 implements these contracts.

Boundary conversions include hop kg→g, percentages→fractions and source colour→EBC.
BeerXML HOP `TIME` is in minutes for every use, including `Dry Hop`; a
display-formatted number of days is not an input duration.
Ambiguous BeerXML `COLOR` requires a visible preview choice of Lovibond or SRM,
retained with mapping version. Missing specifications stay absent, not zero.
Preserve unsupported timing/process information instead of truncating, clamping
or silently treating it as ordinary boil data. XML limitations never mutate
native inputs.

## Styles and scaling

Compare full-precision OG, FG, ABV, IBU and EBC against inclusive bounds. Return
below/within/above with identity, edition, source, values and units through both
interfaces. Missing estimates have no marker; no known style means no band.
Batch volume has no band. Style guidance does not block Save or Ready brew start.
The prototype's limited BJCP 2021 facts are in
[`styles.js`](design/prototype/styles.js), not a complete catalogue. Shared style
values/status enter context/history rather than being UI-owned calculations.

Policy `linear-ingredients-v1` uses `new_batch_l / old_batch_l`. Multiply fermentable
and hop amounts; retain specifications, timings, temperatures, attenuation,
profile selection and fixed equipment constants. Manual mash water and explicit
kettle top-up scale by the factor; profile-thickness mode derives mash water from
scaled grain. Recompute targets and capacity diagnostics. Fixed losses do not
scale, so targets can change: this is ingredient scaling, not target matching.
Apply checks expected recipe revision and accepted context, creating one new
revision. Duplicate creates an independent recipe with copied specs/provenance.
Neither changes source history or brews. KRA-18 owns implementation; capacity
problems require explicit profile/process changes, not automatic geometry changes.

## Worked cases and implementation acceptance

These are arithmetic references, not completed production tests. For a 20 L batch,
2 L transfer loss, 3 L/h boil-off, 60-minute boil, 4.2 kg mashed grain,
0.8 L/kg absorption and zero pre-boil deadspace:

| Quantity | Calculation | Expected |
| --- | --- | --- |
| Post-boil kettle | 20 + 2 | 22 L |
| Pre-boil | 22 + 3 | 25 L |
| Absorption | 4.2 × 0.8 | 3.36 L |
| Total water | 25 + 3.36 | 28.36 L |
| Sparge, manual mash 15 L, no top-up | 28.36 - 15 | 13.36 L |
| Grain extract, potential 300 points·L/kg, efficiency 0.75 | 4.2 × 300 × 0.75 | 945 points·L |
| Pre-boil SG | 1 + 945 / 25 / 1000 | 1.0378 |
| OG | 1 + 945 / 22 / 1000 | 1.042954545455 |
| FG, attenuation 0.80 | 1 + (OG - 1) × 0.20 | 1.008590909091 |

Boil-off 4 L/h gives pre-boil/total water 26/29.36 L; an existing brew retains
25/28.36 L. Adding 0.5 kg sugar at yield 1 at flameout adds 192.837250228 points·L:
pre-boil SG remains 1.0378, OG becomes 1.051719875010, earlier gravity excludes
the sugar. Extract-only 3 kg at 300 points·L/kg gives 900 points·L and OG
1.040909090909 at 22 L, independent of mash efficiency, with zero absorption.

| Acceptance | Required evidence / owner |
| --- | --- |
| Finite/negative/absent/per-row input | Invalid/Incomplete diagnostics, incomplete save and Ready-only start; KRA-8 |
| Grain/extract/mixed/late-boil additions | Arithmetic above, event-timing oracle, type-specific efficiency/absorption; KRA-9 |
| Hops/stand/chilling | Analytic segment oracle, dry-hop zero, held-temperature trajectory, unsupported schedules; KRA-9 |
| Changed engine/profile/default/style | Full-context start conflict and frozen old targets; KRA-15/20 |
| BeerXML unknown/missing/ambiguous data | Original-byte recovery, retained sidecar, explicit colour/efficiency and export-loss choices; KRA-17 |
| Scale 20→40 L | Ingredient factor 2, fixed losses/timing/geometry, recomputed targets and new revision; KRA-18 |
| Style boundary/no style | Full-precision inclusive status and unavailable states; KRA-12/23 |
| UI/MCP parity | Identical native/context yields identical outputs, diagnostics and policy; KRA-23 |

Reference arithmetic tolerance is absolute `1e-9` for litres, points·L and SG,
`1e-6` for ABV/IBU/EBC. Post-flameout integration fixtures must converge at half
the production timestep within 0.05 IBU; that bounds numerical, not physical,
error. Retain unrounded reference values and test display rounding separately.
KRA-9 supplies executable fixtures before claiming supported-process correctness.
