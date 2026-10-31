// SPDX-License-Identifier: AGPL-3.0-or-later

//! Post-flameout isomerisation — the mIBU technique.
//!
//! Plain Tinseth predicts zero bitterness after flameout. That is wrong: wort
//! takes many minutes to fall out of the isomerisation band, and everything in
//! the kettle keeps reacting on the way down. KRAUSEN.md §4.5 identified the
//! gap against Brewfather as "the signature of unmodelled post-flameout
//! isomerisation"; this module models it rather than fudging it.
//!
//! Method: John-Paul Hosom's mIBU (alchemyoverlord). Alpha acid decays at
//! Tinseth's rate, and each increment of isomerised product is weighted by a
//! temperature-dependent relative utilisation. Integrate that from flameout
//! until the wort is too cold to react.
//!
//! We implement the mIBU *technique* — the temperature-weighted integral. The
//! reference calculator layers further modelling on top (non-IAA compounds,
//! krausen loss, pellet-versus-cone factors, top-off volume). That is Hosom's
//! later SMPH work, it carries parameters we have no data to fit, and §3.1
//! says one model consistently applied beats a pile of knobs.

use crate::equipment::{Chilling, Equipment};

/// Amplitude of the natural-cooling curve, in °C. `T(t) = A·e^(-bt) + C`,
/// with `C` the asymptote the wort would settle at (boil temperature − A).
const NATURAL_COOLING_AMPLITUDE_C: f64 = 53.7;

/// Natural cooling rate from kettle geometry:
/// `b = 0.0002925 · (effective area / volume) + 0.0053834`, effective area in
/// cm² and volume in litres. A wide, open kettle sheds heat faster than a
/// narrow or lidded one.
const NATURAL_COOLING_AREA_TERM: f64 = 0.000_292_5;
const NATURAL_COOLING_BASE_TERM: f64 = 0.005_383_4;

/// Relative utilisation against temperature: `2.39e11 · e^(-9773/T)`, T in
/// Kelvin. Normalised so boiling is 1.0.
const UREL_COEFFICIENT: f64 = 2.39e11;
const UREL_EXPONENT: f64 = -9773.0;

/// Below this the reaction is negligible and integration stops. At this
/// threshold the wort is around 44 °C.
const UREL_NEGLIGIBLE: f64 = 0.01;

/// Tinseth's alpha-acid decay rate, per minute.
const AA_DECAY_PER_MIN: f64 = 0.04;

/// Tinseth's normalising divisor.
const TINSETH_DIVISOR: f64 = 4.15;

/// Integration step, in minutes. Matches the reference implementation.
const STEP_MINUTES: f64 = 0.01;

/// Hard stop, in minutes. Only reached if the wort somehow never cools;
/// integration normally ends when utilisation becomes negligible.
const MAX_POST_FLAMEOUT_MINUTES: f64 = 180.0;

const KELVIN_OFFSET: f64 = 273.15;

/// Relative utilisation at a given wort temperature, capped at boiling.
pub(crate) fn relative_utilisation(temp_c: f64) -> f64 {
    let temp_k = temp_c + KELVIN_OFFSET;
    if temp_k <= 0.0 {
        return 0.0;
    }
    (UREL_COEFFICIENT * (UREL_EXPONENT / temp_k).exp()).min(1.0)
}

/// Natural cooling rate constant for this kettle and volume.
pub(crate) fn natural_cooling_rate(equipment: &Equipment, post_boil_l: f64) -> f64 {
    if post_boil_l <= 0.0 {
        return NATURAL_COOLING_BASE_TERM;
    }
    let radius = equipment.kettle_diameter_cm / 2.0;
    let surface_area = std::f64::consts::PI * radius * radius;

    let opening_radius = equipment.kettle_opening_cm / 2.0;
    let opening_area = std::f64::consts::PI * opening_radius * opening_radius;

    let effective_area = (surface_area * opening_area).sqrt();

    NATURAL_COOLING_AREA_TERM * (effective_area / post_boil_l) + NATURAL_COOLING_BASE_TERM
}

/// The wort temperature trajectory after flameout, sampled at [`STEP_MINUTES`].
///
/// Three phases, in order:
/// 1. the stand — natural cooling, or held at a target if one is set
/// 2. chilling — whatever the brewery uses
/// 3. done — once the wort is too cold to isomerise
struct Trajectory {
    natural_rate: f64,
    /// Rate and asymptote of the active chilling method, if it is a decay.
    chilling: Chilling,
    stand_minutes: f64,
    hold_temp_c: Option<f64>,
    boil_temp_c: f64,
    post_boil_l: f64,
}

impl Trajectory {
    /// Advance the wort temperature by one step.
    fn step(&self, temp_c: f64, elapsed: f64) -> f64 {
        let natural_asymptote = self.boil_temp_c - NATURAL_COOLING_AMPLITUDE_C;

        // During the stand the wort sits in the kettle. If the brewer holds a
        // hop-stand temperature, it is maintained rather than drifting down.
        if elapsed < self.stand_minutes {
            if let Some(hold) = self.hold_temp_c
                && temp_c <= hold
            {
                return hold;
            }
            return decay_toward(temp_c, natural_asymptote, self.natural_rate);
        }

        match self.chilling {
            // Wort keeps sitting in the kettle, losing heat to the room only.
            Chilling::None => decay_toward(temp_c, natural_asymptote, self.natural_rate),

            Chilling::Forced {
                rate_per_min,
                plateau_c,
            } => decay_toward(temp_c, plateau_c, rate_per_min),

            // The kettle stays hot; wort is chilled as it leaves. Whatever is
            // still in the kettle carries on cooling naturally, and the
            // transfer sets how long that lasts.
            Chilling::Counterflow { .. } => {
                decay_toward(temp_c, natural_asymptote, self.natural_rate)
            }
        }
    }

    /// When the wort has all left the kettle, nothing further can isomerise.
    fn finished_at(&self) -> f64 {
        match self.chilling {
            Chilling::Counterflow { litres_per_minute } if litres_per_minute > 0.0 => {
                self.stand_minutes + self.post_boil_l / litres_per_minute
            }
            _ => MAX_POST_FLAMEOUT_MINUTES,
        }
    }
}

/// One step of exponential decay toward an asymptote.
///
/// Explicit Euler, matching the reference implementation. A large enough
/// `rate_per_min * STEP_MINUTES` would overshoot the asymptote and oscillate
/// — physically impossible, since wort cannot cool past what is cooling it —
/// so the step is clamped to land on the asymptote instead.
fn decay_toward(temp_c: f64, asymptote_c: f64, rate_per_min: f64) -> f64 {
    let above = temp_c - asymptote_c;
    let fraction = (rate_per_min * STEP_MINUTES).clamp(0.0, 1.0);
    asymptote_c + above * (1.0 - fraction)
}

/// Post-flameout utilisation for each addition, in the order given.
///
/// `remaining_alpha` is the fraction of each addition's alpha acid still
/// un-isomerised at flameout — `e^(-0.04 · boil_minutes)` for a boil addition,
/// and 1.0 for anything added at flameout.
///
/// The returned values are Tinseth *time factors*, directly comparable to
/// `(1 - e^(-0.04t)) / 4.15`, so a caller multiplies by the bigness factor
/// exactly as it does for the boil phase.
pub(crate) fn post_flameout_utilisation(
    equipment: &Equipment,
    post_boil_l: f64,
    stand_minutes: f64,
    hold_temp_c: Option<f64>,
    remaining_alpha: &[f64],
) -> Vec<f64> {
    let mut accumulated = vec![0.0; remaining_alpha.len()];
    if remaining_alpha.is_empty() {
        return accumulated;
    }

    let trajectory = Trajectory {
        natural_rate: natural_cooling_rate(equipment, post_boil_l),
        chilling: equipment.chilling.resolved(post_boil_l),
        stand_minutes,
        hold_temp_c,
        boil_temp_c: equipment.boil_temp_c,
        post_boil_l,
    };

    let finish_at = trajectory.finished_at();
    let mut alpha = remaining_alpha.to_vec();
    let mut temp_c = equipment.boil_temp_c;
    let mut elapsed = 0.0_f64;

    while elapsed < finish_at && elapsed < MAX_POST_FLAMEOUT_MINUTES {
        let urel = relative_utilisation(temp_c);
        if urel < UREL_NEGLIGIBLE {
            break;
        }

        // Alpha acid isomerises at Tinseth's rate; only the share that does so
        // while the wort is hot enough counts toward bitterness.
        for (acc, aa) in accumulated.iter_mut().zip(alpha.iter_mut()) {
            let isomerised = *aa * AA_DECAY_PER_MIN * STEP_MINUTES;
            *acc += isomerised * urel / TINSETH_DIVISOR;
            *aa -= isomerised;
        }

        temp_c = trajectory.step(temp_c, elapsed);
        elapsed += STEP_MINUTES;
    }

    accumulated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equipment::Equipment;

    fn equipment() -> Equipment {
        Equipment {
            boil_off_rate_l_per_hour: 3.0,
            grain_absorption_l_per_kg: 0.8,
            kettle_deadspace_l: 0.0,
            losses_l: 2.0,
            mash_efficiency: 0.78,
            ..Equipment::default()
        }
    }

    #[test]
    fn relative_utilisation_is_one_at_boiling_and_falls_away() {
        assert!((relative_utilisation(100.0) - 1.0).abs() < 1e-9);
        assert!(relative_utilisation(90.0) < 1.0);
        assert!(relative_utilisation(80.0) < relative_utilisation(90.0));
        // Not zero, but three orders of magnitude below boiling.
        assert!(relative_utilisation(20.0) < 0.001);
        assert!(relative_utilisation(20.0) < relative_utilisation(100.0) / 1000.0);
    }

    #[test]
    fn isomerisation_becomes_negligible_around_forty_four_degrees() {
        // The documented cutoff: below this the integration stops.
        assert!(relative_utilisation(45.0) > UREL_NEGLIGIBLE);
        assert!(relative_utilisation(43.0) < UREL_NEGLIGIBLE);
    }

    #[test]
    fn a_wider_kettle_cools_faster() {
        let mut narrow = equipment();
        narrow.kettle_diameter_cm = 25.0;
        narrow.kettle_opening_cm = 25.0;

        let mut wide = equipment();
        wide.kettle_diameter_cm = 50.0;
        wide.kettle_opening_cm = 50.0;

        assert!(natural_cooling_rate(&wide, 22.5) > natural_cooling_rate(&narrow, 22.5));
    }

    #[test]
    fn a_lidded_kettle_cools_slower_than_an_open_one() {
        let mut open = equipment();
        open.kettle_opening_cm = open.kettle_diameter_cm;

        let mut lidded = equipment();
        lidded.kettle_opening_cm = lidded.kettle_diameter_cm / 4.0;

        assert!(natural_cooling_rate(&lidded, 22.5) < natural_cooling_rate(&open, 22.5));
    }

    #[test]
    fn more_wort_cools_slower() {
        let eq = equipment();
        assert!(natural_cooling_rate(&eq, 45.0) < natural_cooling_rate(&eq, 22.5));
    }

    #[test]
    fn a_flameout_addition_still_gains_bitterness() {
        let eq = equipment();
        // Added at flameout, so none of its alpha acid has isomerised yet.
        let u = post_flameout_utilisation(&eq, 22.5, 0.0, None, &[1.0]);
        assert!(
            u[0] > 0.0,
            "plain Tinseth would say zero; the wort is still hot"
        );
    }

    #[test]
    fn a_longer_stand_extracts_more_when_chilling_would_otherwise_have_started() {
        // The stand only matters if something would have cooled the wort
        // sooner. Left to cool on its own, stand length is irrelevant — and
        // the model says so.
        let mut chilled = equipment();
        chilled.chilling = Chilling::ice_bath(22.5);
        let short = post_flameout_utilisation(&chilled, 22.5, 0.0, None, &[1.0]);
        let long = post_flameout_utilisation(&chilled, 22.5, 30.0, None, &[1.0]);
        assert!(long[0] > short[0]);

        let mut unchilled = equipment();
        unchilled.chilling = Chilling::None;
        let a = post_flameout_utilisation(&unchilled, 22.5, 0.0, None, &[1.0]);
        let b = post_flameout_utilisation(&unchilled, 22.5, 30.0, None, &[1.0]);
        assert!(
            (a[0] - b[0]).abs() < 1e-12,
            "with no chilling the wort follows the same curve either way"
        );
    }

    #[test]
    fn decay_cannot_overshoot_its_asymptote() {
        // A chilling rate fast enough to overshoot in one Euler step would
        // otherwise drive the wort below the thing cooling it.
        assert!((decay_toward(100.0, 20.0, 1000.0) - 20.0).abs() < 1e-12);
        assert!(decay_toward(100.0, 20.0, 1000.0) >= 20.0);
    }

    #[test]
    fn a_held_hop_stand_extracts_less_than_a_boiling_one() {
        let eq = equipment();
        let hot = post_flameout_utilisation(&eq, 22.5, 20.0, Some(95.0), &[1.0]);
        let cool = post_flameout_utilisation(&eq, 22.5, 20.0, Some(70.0), &[1.0]);
        assert!(cool[0] < hot[0], "70 C isomerises far less than 95 C");
    }

    #[test]
    fn an_addition_already_spent_in_the_boil_gains_little() {
        let eq = equipment();
        // A 90-minute addition has ~2.7% of its alpha acid left at flameout.
        let long_boil = (-AA_DECAY_PER_MIN * 90.0).exp();
        let fresh = post_flameout_utilisation(&eq, 22.5, 0.0, None, &[1.0]);
        let spent = post_flameout_utilisation(&eq, 22.5, 0.0, None, &[long_boil]);
        assert!(
            spent[0] < fresh[0] * 0.1,
            "little alpha acid remains to isomerise"
        );
    }

    #[test]
    fn counterflow_transfer_time_bounds_the_contribution() {
        let mut slow = equipment();
        slow.chilling = Chilling::Counterflow {
            litres_per_minute: 1.0,
        };
        let mut fast = equipment();
        fast.chilling = Chilling::Counterflow {
            litres_per_minute: 10.0,
        };

        let slow_u = post_flameout_utilisation(&slow, 22.5, 0.0, None, &[1.0]);
        let fast_u = post_flameout_utilisation(&fast, 22.5, 0.0, None, &[1.0]);
        assert!(
            slow_u[0] > fast_u[0],
            "wort that lingers in a hot kettle keeps isomerising"
        );
    }

    #[test]
    fn chilling_promptly_beats_leaving_it_to_cool() {
        let mut chilled = equipment();
        chilled.chilling = Chilling::ice_bath(22.5);
        let mut left = equipment();
        left.chilling = Chilling::None;

        let a = post_flameout_utilisation(&chilled, 22.5, 0.0, None, &[1.0]);
        let b = post_flameout_utilisation(&left, 22.5, 0.0, None, &[1.0]);
        assert!(a[0] < b[0]);
    }

    #[test]
    fn additions_are_independent_of_each_other() {
        let eq = equipment();
        let together = post_flameout_utilisation(&eq, 22.5, 10.0, None, &[1.0, 0.5]);
        let first = post_flameout_utilisation(&eq, 22.5, 10.0, None, &[1.0]);
        let second = post_flameout_utilisation(&eq, 22.5, 10.0, None, &[0.5]);
        assert!((together[0] - first[0]).abs() < 1e-12);
        assert!((together[1] - second[0]).abs() < 1e-12);
    }

    #[test]
    fn no_additions_is_not_a_panic() {
        let eq = equipment();
        assert!(post_flameout_utilisation(&eq, 22.5, 0.0, None, &[]).is_empty());
    }
}
