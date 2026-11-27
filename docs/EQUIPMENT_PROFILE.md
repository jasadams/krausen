# Equipment profile specification

Status: Jason's equipment spec, recorded 2026-11-28 under KRA-70. It is the
approved target for the Equipment manager (UX-005, IA-009, VIS-002/003). It was
given when the design started but never written down, so the earlier design was
built without it. Formulas are in [CALCULATIONS.md](CALCULATIONS.md#equipment-profile-v2-water-chain-and-capacity-checks);
storage is in [DATA_MODEL.md](DATA_MODEL.md#equipment-profile-record).

**Why.** A brewer setting up their system (for example a Grainfather G30) expects
to describe it the way they think about it: what it is, then its mash, sparge,
boil and cooling stages, with the volumes and losses of each. They know it works
when they can enter their real equipment as below, save it as the default, and see
the estimates use the stages' volumes and losses.

## The spec (verbatim)

- **About:** Default equipment (toggle); Equipment name (e.g. "Grainfather (220V) - Bluetooth Controller"); Equipment brand (e.g. Grainfather); Description; Measurement (Metric); Equipment type (All Grain); Controller type (e.g. Grainfather Bluetooth Controller); Batch size (L); Recirculation system (e.g. RIMS); Number of vessels (e.g. 1).
- **Mash:** Mash tun volume (L, e.g. 30); Mash efficiency (%, e.g. 78); Mash tun deadspace (L, e.g. 3.5); Mash thickness (L/kg, e.g. 2.8); Mash tun loss (L, e.g. 0); Grain absorption (L/kg, e.g. 0.8).
- **Sparge:** Sparge water alert (toggle); Sparge progress counter (toggle); Sparge vessel volume (L, e.g. 19); Grain absorption (L/kg, e.g. 0); Sparge deadspace volume (L, e.g. 0).
- **Boil:** Boil power control (toggle); Boil volume (L, e.g. 30); Wort shrinkage (%, e.g. 4); Boil loss (L/hr, e.g. 3); Trub and chiller loss (L, e.g. 2); Kettle diameter (cm, e.g. 30).
- **Cooling:** Cooling method (e.g. Counterflow chiller); Flow rate (L/min, e.g. 1).
- **Notes.**

Clarification (Jason, 2026-11-28): today's "Transfer loss" (wort lost after the
boil) **is** "Trub and chiller loss", and "Boil-off" **is** "Boil loss (L/hr)". They
are one field each, not two.

The profile page presents exactly these sections and fields in this order, using
the shared kit. The section order is About, Mash, Sparge, Boil, Cooling, Notes.

## Which fields do what

| Spec field | Stored as | Role |
| --- | --- | --- |
| Default equipment | workspace default pointer (not a revisioned setting) | New recipes select it |
| Equipment name | profile name | Identity |
| Equipment brand, Description, Controller type, Recirculation system, Notes | `about` block | Stored and shown |
| Measurement | `about.measurement` (only `metric`) | Stored and shown |
| Equipment type | `about.equipment_type` (`all_grain`, …) | Stored and shown |
| Number of vessels | `about.vessel_count` | Stored and shown |
| Batch size (L) | `about.batch_size_l` | Seeds a new recipe's batch size |
| Sparge water alert, Sparge progress counter, Boil power control | `about` toggles | Stored and shown |
| Mash tun volume | `mash_tun_volume_l` | Capacity check |
| Mash efficiency | `mash_efficiency_fraction` | Calculation |
| Mash tun deadspace | `pre_boil_deadspace_l` | Calculation |
| Mash thickness | `mash_thickness_l_per_kg` | Calculation |
| Mash tun loss | `mash_tun_loss_l` | Calculation |
| Grain absorption (mash) | `grain_absorption_l_per_kg` | Calculation |
| Sparge vessel volume | `sparge_vessel_volume_l` | Capacity check |
| Grain absorption (sparge) | `sparge_grain_absorption_l_per_kg` | Calculation |
| Sparge deadspace volume | `sparge_deadspace_l` | Calculation |
| Boil volume | `kettle_capacity_l` | Capacity check |
| Wort shrinkage | `wort_shrinkage_fraction` | Calculation |
| Boil loss | `boil_off_l_per_hour` | Calculation |
| Trub and chiller loss | `transfer_loss_l` | Calculation |
| Kettle diameter | `kettle_diameter_cm` | Calculation (post-flameout model) |
| Cooling method, Flow rate | `chilling` (`counterflow.litres_per_minute`) | Calculation |

## Today's 14 fields mapped onto the spec

| Today's field (label) | Spec field | Treatment |
| --- | --- | --- |
| `efficiency_override_fraction` ("Brewhouse efficiency") | none | Removed from profiles. A profile that has it migrates: its value becomes Mash efficiency, because the effective efficiency today is `override.or(mash)`, so the estimate is unchanged. The recipe-level override and BeerXML's efficiency choice keep this field and are unchanged. |
| `mash_efficiency_fraction` | Mash efficiency | Same field |
| `boil_off_l_per_hour` ("Boil-off") | Boil loss (L/hr) | Same field (Jason, 2026-11-28) |
| `transfer_loss_l` ("Transfer loss") | Trub and chiller loss | Same field (Jason, 2026-11-28) |
| `pre_boil_deadspace_l` | Mash tun deadspace | Same water-chain term: unrecoverable water before the boil. Proposed mapping, see decision D1 |
| `grain_absorption_l_per_kg` | Grain absorption (Mash) | Same field |
| `mash_thickness_l_per_kg` | Mash thickness | Same field |
| `kettle_capacity_l` | Boil volume | Same field; now also a capacity check |
| `kettle_diameter_cm` | Kettle diameter | Same field |
| `chilling.counterflow.litres_per_minute` ("Counterflow rate") | Cooling → Flow rate | Same field |
| `chilling` method | Cooling method | Same field, see D3 |
| `kettle_opening_cm`, `boil_temperature_c`, `chilling.forced.rate_per_minute`, `chilling.forced.plateau_c` | none | Calculation-feeding with no spec equivalent: see decisions D2 and D3 |

## Decisions for Jason (calculation-feeding fields without a spec equivalent)

Until decided, these four fields stay stored and keep feeding the calculation
exactly as today, so no estimate moves; they are not shown on the profile page,
which presents only the spec. Removing them later is a separate migration that
needs the listed effect to be acceptable.

| ID | Field | What it affects | Proposal |
| --- | --- | --- | --- |
| D1 | `pre_boil_deadspace_l` | Total water (+litres), so sparge water | Mapped to Mash tun deadspace (above). Confirm the mapping. |
| D2a | `kettle_opening_cm` | Post-flameout cooling in the IBU model (lid effect); late hops/whirlpool IBU | Drop. Absent means uncovered (opening = diameter). A profile with a different opening moves whirlpool/flameout IBU slightly when dropped. |
| D2b | `boil_temperature_c` | Start temperature of the post-flameout IBU model (altitude boiling point) | Drop; assume 100 °C as the visible assumption. An altitude profile with a lower value moves late-hop IBU when dropped. |
| D3 | `chilling.forced.{rate_per_minute, plateau_c}` | Post-flameout cooling curve, late-hop IBU | Cooling method becomes Natural / Immersion chiller / Counterflow chiller. Immersion uses the engine's published volume-dependent rate and 41 °C plateau; explicit rate/plateau dropped. A profile with measured values moves late-hop IBU when dropped. |
| D4 | new constant: grain displacement 0.67 L/kg | Mash tun capacity check only (water + grain volume), no estimate | A visible assumption in every preview that has a mash tun volume. Confirm or give another value. |

## Migration and unchanged estimates

Stored equipment revisions and brew snapshots are immutable and are never
rewritten. New fields are additive and optional; reading an older revision
applies the single upgrade rule in the table above (Brewhouse efficiency folds
into Mash efficiency) and nothing else. Absent new loss fields mean "not
modelled" and are recorded as applied defaults of zero, so a legacy profile's
totals equal today's. Details and the test are in
[DATA_MODEL.md](DATA_MODEL.md#equipment-profile-record).
