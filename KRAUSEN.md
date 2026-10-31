# Krausen

A brewing application that learns your equipment from your own brew history and
tells you what to measure, when to adjust, and where things are drifting.

This document is the project's orientation. Read it before writing code. It
records decisions that have already been made and the reasoning behind them, so
that they don't get relitigated or quietly undone.

---

## 1. The thesis

Existing brewing software is either too complex or too shallow, and none of it
closes the loop.

Brewfather, BeerSmith and Brewer's Friend are recipe databases with calculators
attached. They will happily store the fact that your boil-off is 3 L/hr, because
they made you type it in. They will not notice that your last five batches say
it's actually 2.6. They present pre-boil gravity as a field to record, but never
tell you that pre-boil is the only point in the process where volume and gravity
are both still free to move, so you set volume first and gravity second or you
chase your own tail.

Krausen is built around that second kind of knowledge. The recipe data is table
stakes. The product is the derived layer: a calibrated model of *this* brewery,
and a process document that encodes decisions rather than storing numbers.

The deliverable a user actually holds is a **bench sheet** — a printable,
write-on-able process document generated from a recipe plus an equipment
profile, containing the targets, the order of operations, the correction rules
scaled to this specific batch, and the reasoning behind the gates.

## 2. What Krausen is not

- **Not a Brewfather clone.** Feature parity is not a goal. If a feature exists
  only because competitors have it, it doesn't ship.
- **Not a simplified brewing app.** Hiding complexity means *more* calculation
  underneath, not fewer capabilities. Step mashes, whirlpool additions and water
  chemistry all stay. What goes is the interrogation.
- **Not a chatbot.** See §3.6.
- **Not a business.** No monetisation, no growth target, no pressure to serve
  users who don't exist yet. It may be shared publicly at some point. That is a
  consequence of it being good, not a requirement placed on it.

---

## 3. Principles

These are the load-bearing commitments. Code that violates one of these is
wrong even if it works.

### 3.1 Hide questions, not capabilities

There is a difference between removing a control and removing a question the
software should have been able to answer itself.

Most of the exposed complexity in competing products lives in the equipment
profile: boil-off rate, grain absorption, kettle deadspace, trub loss, mash
efficiency. That's five numbers a new brewer cannot possibly know and an
experienced one is estimating. Every one of them is recoverable by regression
from a handful of logged brews. Deleting that form is the single largest
simplification available to this product.

Adjacent cases, same shape:

- Pick one IBU model and be consistent. Users do not need to choose between
  Tinseth, Rager and Garetz; they need their numbers to be comparable to each
  other. **Tinseth.**
- Water chemistry outputs grams of two or three salts. Not an eight-ion spider
  chart.
- One efficiency number, clearly defined. Not mash vs. brewhouse vs. conversion
  efficiency, which mostly exist to confuse people about which one moved.

### 3.2 The software has a point of view

An audience of one, or of a few hundred hobbyists, means Krausen can be
opinionated where a commercial product must stay neutral. Hard-code the
judgements. Pre-boil is the adjustment window. Volume is set before gravity.
Gravity stability decides fermentation timing, not the calendar and never the
airlock. Where a default is a judgement call, state the judgement in the UI
rather than hiding it behind a setting.

### 3.3 Learn the environment, never silently change it

The app continuously fits an equipment model from logged brews. It does **not**
apply that model without the user saying so.

Keep two separate things:

- **Fitted model** — always updating, always visible, derived from history.
- **Active profile** — what bench sheets actually use. Changes only on explicit
  user action.

An alert is the gap between them, presented with its evidence and its sample
size: *"Your last five brews average 2.6 L/hr boil-off. Your profile says 3.0.
Apply?"*

This is a trust decision, but it's also a technical one. Silent auto-tuning
destroys the signal you most want. If every batch is folded into the estimate,
then the day the lid is half on, or the element starts scaling, the model
absorbs the anomaly instead of flagging it. **The deviation is the information.**
Averaging it away is the one thing that can't be undone.

### 3.4 Flag runs, not misses

Deciding what counts as "off" is harder than it looks. Batch-to-batch scatter is
real, so a fixed percentage threshold either fires constantly or misses genuine
drift.

Track each residual against the *spread of past residuals*, not against the
profile value. Then:

- One batch deviating in either direction → "check something." That's Tuesday.
- Three or four consecutive batches deviating in the same direction → "your
  system has changed, consider updating the profile."

Those are genuinely different messages and the user should never see the second
one when the first is true.

### 3.5 Say "I don't know yet"

Brew one has no history. The app uses defaults, says visibly that they are
defaults, and shows the estimate tightening as batches accumulate. Confidence is
part of the display, not an afterthought.

### 3.6 AI never produces a number

Subtle AI enhancement is in scope. A chat interface is not.

**Every figure in the application comes from the deterministic engine.** The
model reasons *about* figures it is given; it never generates one. This is the
line that keeps the feature trustworthy, and it's the difference between an
evaluator and something that confidently hallucinates an IBU.

That leaves the model the work it's actually suited to — judgement over
unstructured context:

- Tasting notes → suggested recipe changes.
- Reading across brew logs and free-text notes for patterns a regression can't
  see (three low-efficiency batches sharing a grain supplier, a crush change).
- Explaining why a given deviation matters *for this beer*: 2 EBC off is nothing
  in a stout and visible in a Helles.

---

## 4. Architecture

### 4.1 The calculation core is a pure function

```
derive(&Recipe, &Equipment) -> Derived
```

No I/O, no database access, no state. Everything else in the application is CRUD
around this function. It must be testable in isolation against batches that have
already been brewed (see Appendix A).

### 4.2 Evaluation order is a DAG, and it matters

These are not independent formulas. Most homebrew calculators produce subtly
wrong results by evaluating them in the wrong order. Boil time sits at the root
of the chain, which is why changing it moves five other numbers.

```
1. Volumes        post_boil   = batch + losses
                  pre_boil    = post_boil + boil_off_rate * boil_hours
                  absorption  = absorption_coeff * grain_kg
                  total_water = pre_boil + absorption + deadspace
                  sparge      = total_water - mash_water

2. Extract        potential   = Σ(kg * points_per_kg_per_L)
                  kettle_pts  = potential * efficiency
                  pre_boil_SG = kettle_pts / pre_boil      <- needs (1)
                  OG          = kettle_pts / post_boil     <- needs (1)

3. Attenuation    FG  = OG - (OG - 1) * apparent_attenuation
                  ABV = (76.08 * (OG-FG) / (1.775-OG)) * (FG / 0.794)

4. IBU            Tinseth, bigness factor from mean boil gravity  <- needs (2)

5. Colour         Morey, over post-boil volume                    <- needs (1)
```

### 4.3 Metric-native extract

Drop PPG from the internal model. It is an imperial unit leaking into a metric
recipe, which is why Munich I shows as 37.826 rather than a round number.

```
points_per_kg_per_L = PPG * 8.345404
```

Signature Lager → 308.78. Munich I → 315.66. Gravity is then `points / volume`
with no unit conversion anywhere in the chain. Retain PPG only for display and
for BeerXML interchange.

### 4.4 Colour uses post-boil volume

Morey over post-boil (22.5 L) gives 6.82 EBC, which matches the reference sheet.
Over batch volume (20.5 L) it gives 7.27, which does not. Same volume basis as
OG.

### 4.5 Whirlpool is an explicit recipe field

`whirlpool_minutes` and `whirlpool_temp`, feeding the same Tinseth integral as
boil additions.

This is not optional. Brewfather reports 7.4 IBU for a 25 g / 15 min charge in
the reference recipe; boil-only Tinseth gives 5.68. The gap is only reachable if
post-flameout isomerisation is being counted. Without these fields, Krausen's
late-hop numbers will always read low against the software users are migrating
from, and neither they nor we will be able to tell whether that's a bug or a
modelling difference.

### 4.6 Brews snapshot recipes, they do not reference them

A brew record embeds a **full copy** of the recipe and equipment profile as they
were at the moment the brew was created.

If a brew holds a foreign key and the recipe is later edited, that brew
retroactively claims to have been a different beer — and every number you'd want
to calibrate from becomes untrustworthy. This is not hypothetical: the reference
recipe in Appendix A changed boil time from 90 to 60 minutes, which moved five
derived values.

Snapshots also mean an old brew can still render its own original bench sheet.
Storage is free. Correct history is not.

### 4.7 Equipment profile is first-class from day one

Even in Phase 1, where it's just a form. It holds boil-off rate, absorption
coefficient, deadspace, efficiency and thermal shrinkage.

The bench sheet is a pure function of (recipe snapshot, equipment profile). That
is what makes Phase 2 small, and what makes Phase 3 a change of *source* for
those fields rather than a change of schema.

### 4.8 Import before UI

Brewfather exports BeerXML and BeerJSON. Write the importer first. It loads the
back catalogue on day one and reveals which fields the model actually needs,
which is more reliable than designing them in the abstract.

---

## 5. Phases

Each phase produces the input the next one requires. Nothing here is out of
order, and each layer must be independently useful — solo projects die at the
integration layer, and if stalling at any boundary costs nothing, stalling is
survivable.

**Phase 1 — Recipes and bench sheets.**
Recipe CRUD, equipment profiles, the full calculation engine, BeerXML/BeerJSON
import, bench sheet generation and print layout. The engine is the whole phase;
storage is incidental.

**Phase 2 — Brews.**
Record a brew day against a recipe snapshot. Actual vs. target for every
measurable: volumes, gravities, temperatures, timings, notes. This is what
generates the data Phase 3 regresses.

**Phase 3 — Fermentation tracking and the fitted model.**
Tilt (BLE iBeacon broadcast, or TiltPi pushing to a webhook we host) and RAPT
Pill (cloud API, polled). Normalise both to a plain stream of readings attached
to a brew; keep a per-device calibration offset, because Tilts drift.

Then the derived layer: fit attenuation curves, fit the equipment constants,
surface drift per §3.3 and §3.4. Gravity gates become automatic rather than
eyeballed.

**Phase 4 — Grainfather integration.**
BLE control and telemetry for the G30 Connect. The protocol has been documented
by third parties; confirm which firmware revisions that covers before relying on
it. `btleplug` for the Rust side.

---

## 6. Stack

- **Rust** throughout. Leptos for the front end (SSR + hydrate via
  `cargo-leptos`).
- Existing app starter template handles auth and account management. Don't
  rebuild it.
- The calculation core is a plain library crate with no framework dependencies,
  so it can be tested, and eventually reused, on its own.

---

## 7. Open questions

- Mash water volume: fixed by grain bill and thickness, or by the Grainfather
  formula? The reference sheet uses 15.26 L for 4.2 kg (3.633 L/kg), which does
  not match Grainfather's published `2.7 × kg + 3.5`. Resolve before Phase 1
  ships.
- Thermal shrinkage: currently folded into the boil-off constant. Separating it
  would be more correct and changes pre-boil targets by roughly 0.3 L. Decide
  whether that precision is worth an extra profile field.
- Deadspace is currently subsumed into absorption (0.8 L/kg reproduces the
  reference sheet exactly). Splitting them is more honest but needs data to fit
  against.

---

## Appendix A — Reference recipe and expected output

Munich Helles, brewed on a Grainfather G30. **Every figure below has been
verified to reproduce the existing bench sheets from the constants given.** Use
both variants as the engine's regression test: they differ only in boil time and
hop schedule, which exercises the whole dependency chain in §4.2.

### Inputs (common)

| | |
|---|---|
| Batch size (into fermenter) | 20.5 L |
| Losses | 2.0 L |
| Mash efficiency | 78% |
| Mash water | 15.26 L |
| Boil-off rate | 3.0 L/hr |
| Grain absorption | 0.8 L/kg |
| Yeast | SafLager S-23, 82% apparent attenuation |

Grist:

| Malt | Amount | PPG | EBC |
|---|---|---|---|
| Joe White Signature Lager | 3.900 kg | 37.000 | 3.6 |
| Weyermann Munich I | 0.300 kg | 37.826 | 14.0 |

### Expected output

| | v4 — 90 min boil | v5 — 60 min boil |
|---|---|---|
| Hops (Hallertau 3.8%) | 30 g @ 90, 20 g @ 15 | 25 g @ 60, 25 g @ 15 |
| Grain absorption | 3.36 L | 3.36 L |
| Sparge water | 15.10 L | 13.60 L |
| Total water | 30.36 L | 28.86 L |
| Pre-boil volume | 27.0 L | 25.5 L |
| Pre-boil gravity | 1.0375 | 1.0397 |
| Post-boil volume | 22.5 L | 22.5 L |
| OG | 1.0450 | 1.0450 |
| FG | 1.0081 | 1.0081 |
| ABV | 4.89% | 4.89% |
| EBC | 6.82 | 6.82 |
| Mean boil gravity | 1.0413 | 1.0424 |
| IBU — Tinseth, boil only | 14.84 + 4.59 = 19.4 | 11.45 + 5.68 = 17.1 |
| IBU — Brewfather reports | 15.0 + 6.0 = 21.0 | 11.8 + 7.4 = 19.2 |
| DME correction | 71.9 g per point | 67.9 g per point |
| Dilution correction | 0.720 L per point | 0.642 L per point |

The IBU discrepancy is the open modelling question in §4.5, not an arithmetic
error. Tinseth agrees closely on the long additions (14.84 vs 15.0; 11.45 vs
11.8) and diverges on the 15-minute charge in both variants, which is the
signature of unmodelled post-flameout isomerisation.

### Correction formulas

```
DME g per gravity point   = pre_boil_L * 0.264172 * 453.592 / 45
                          = pre_boil_L * 2.6626

Dilution L per point      = pre_boil_L / pre_boil_points
```

Both scale with pre-boil volume, which is why shortening the boil changed them.
They must be derived, never stored.

### Constants

```
PPG -> points.L/kg     x 8.345404
lb per kg                2.2046226
gal per L                0.26417205

Tinseth   bigness = 1.65 * 0.000125^(Gb - 1)
          time    = (1 - e^(-0.04t)) / 4.15
          IBU     = (g * AA * 1000 / V) * bigness * time
          where V = batch volume (20.5 L) and Gb = mean boil gravity

Morey     Lovibond = ((EBC / 1.97) + 0.76) / 1.3546
          MCU      = Σ(lb * Lovibond) / post_boil_gal
          SRM      = 1.4922 * MCU^0.6859
          EBC      = SRM * 1.97
```
