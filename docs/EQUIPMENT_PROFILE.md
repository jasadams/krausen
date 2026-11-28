# Equipment profile specification

Status: Jason's equipment spec, recorded 2026-11-28 under KRA-70. It is the
approved target for the Equipment manager (UX-005, IA-009, VIS-002/003). It was
given when the design started but never written down, so the earlier design was
built without it. Formulas are in [CALCULATIONS.md](CALCULATIONS.md#equipment-profile-water-chain-and-capacity-checks-policy-v3);
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

Decision (Jason, 2026-11-28, final): the profile is **exactly** this spec. Fields
the spec does not list are removed entirely: no separate pre-boil deadspace, no
kettle opening, no boil temperature, no forced-cooling rate/plateau and no
Brewhouse efficiency; there are no hidden stored fields and no extra UI.
Calculations use only the spec fields plus the fixed assumptions below. Existing
profile data is not migrated: it may be reset to the new shape. Saved recipe
revisions and the immutable snapshots of historical brews are never rewritten and
stay readable.

The profile page presents exactly these sections and fields in this order, using
the shared kit. The section order is About, Mash, Sparge, Boil, Cooling, Notes.

## Which fields do what

| Spec field | Stored as | Role |
| --- | --- | --- |
| Default equipment | workspace default pointer (not a revisioned setting) | New recipes select it |
| Equipment name | profile name | Identity |
| Equipment brand, Description, Controller type, Recirculation system, Notes | `about` block | Stored and shown |
| Measurement | `about.measurement` (only `metric`) | Stored and shown |
| Equipment type | `about.equipment_type` (`all_grain`) | Stored and shown |
| Number of vessels | `about.vessel_count` | Stored and shown |
| Batch size (L) | `about.batch_size_l` | Stored and shown; does not seed new recipes (a per-user default batch size is KRA-74) |
| Sparge water alert, Sparge progress counter, Boil power control | `about` toggles | Stored and shown |
| Mash tun volume | `mash_tun_volume_l` | Capacity check |
| Mash efficiency | `mash_efficiency_fraction` | Calculation |
| Mash tun deadspace | `mash_tun_deadspace_l` | Calculation |
| Mash thickness | `mash_thickness_l_per_kg` | Calculation |
| Mash tun loss | `mash_tun_loss_l` | Calculation |
| Grain absorption (Mash) | `mash_grain_absorption_l_per_kg` | Calculation |
| Sparge vessel volume | `sparge_vessel_volume_l` | Capacity check |
| Grain absorption (Sparge) | `sparge_grain_absorption_l_per_kg` | Calculation |
| Sparge deadspace volume | `sparge_deadspace_l` | Calculation |
| Boil volume | `boil_volume_l` | Capacity check |
| Wort shrinkage | `wort_shrinkage_fraction` | Calculation |
| Boil loss | `boil_loss_l_per_hour` | Calculation |
| Trub and chiller loss | `trub_and_chiller_loss_l` | Calculation |
| Kettle diameter | `kettle_diameter_cm` | Calculation (post-flameout model) |
| Cooling method | `cooling_method` (`natural`, `immersion_chiller`, `ice_bath`, `counterflow_chiller`) | Calculation |
| Flow rate | `cooling_flow_rate_l_per_minute` | Calculation, counterflow only |

The calculation fields live in `EquipmentSettingsV1`, which recipes also use for
per-recipe overrides and brews freeze; `about` is profile-only. The recipe-level
"Brewhouse efficiency override" (`efficiency_override_fraction`) and BeerXML's
efficiency choice are unchanged and are not profile fields.

## Fixed assumptions (not fields)

Shown with every estimate that uses them and recorded in the calculation context:

- The kettle is uncovered: opening equals kettle diameter.
- The wort boils at 100 °C.
- Cooling methods that need a curve use the published defaults of the mIBU
  reference implementation in `krausen-calc`: natural is no active chilling;
  immersion chiller is rate `0.4071·e^(-0.0754·V)` per minute (V = post-boil
  litres) settling at 41 °C; ice bath is `0.6075·e^(-0.0704·V)` settling at 20 °C;
  counterflow chiller uses the profile's Flow rate.
- Grain displacement is 0.67 L/kg, used only by the mash tun overfill warning.

## Renames from the earlier profile

| Earlier field | Spec field |
| --- | --- |
| `boil_off_l_per_hour` | `boil_loss_l_per_hour` |
| `transfer_loss_l` | `trub_and_chiller_loss_l` |
| `pre_boil_deadspace_l` | `mash_tun_deadspace_l` (the one deadspace term; no separate pre-boil field) |
| `grain_absorption_l_per_kg` | `mash_grain_absorption_l_per_kg` |
| `kettle_capacity_l` | `boil_volume_l` |

The old names are accepted as read aliases so older recipe and brew snapshots
still load. `kettle_opening_cm`, `boil_temperature_c`, `chilling` and a profile's
`efficiency_override_fraction` are removed; old documents that carry them still
load and the values are ignored.
