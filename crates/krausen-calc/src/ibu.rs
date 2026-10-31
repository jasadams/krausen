// SPDX-License-Identifier: AGPL-3.0-or-later

//! Stage 4 of §4.2 — bitterness, by Tinseth.
//!
//! One IBU model, consistently applied. §3.1: users do not need to choose
//! between Tinseth, Rager and Garetz; they need their numbers comparable to
//! each other.

use crate::{
    derived::IbuContribution,
    equipment::Equipment,
    recipe::{Hop, HopUse, Recipe},
    units::Gravity,
};

/// Tinseth's bigness coefficient: `1.65 * 0.000125^(Gb - 1)`.
const BIGNESS_COEFFICIENT: f64 = 1.65;
const BIGNESS_BASE: f64 = 0.000125;

/// Tinseth's time factor: `(1 - e^(-0.04t)) / 4.15`.
const TIME_RATE: f64 = 0.04;
const TIME_DIVISOR: f64 = 4.15;

/// Alpha acid decays at Tinseth's rate whether or not the burner is on, which
/// is what lets the boil and post-flameout phases share one model.
const AA_DECAY_PER_MIN: f64 = TIME_RATE;

/// The gravity the wort spends the boil at — the mean of its start and end.
pub(crate) fn mean_boil_gravity(pre_boil: Gravity, original: Gravity) -> Gravity {
    Gravity::from_points((pre_boil.points() + original.points()) / 2.0)
}

/// Tinseth's gravity term. Sugar suppresses isomerisation, so a heavier wort
/// extracts less bitterness from the same hops.
fn bigness_factor(mean_boil_gravity: Gravity) -> f64 {
    BIGNESS_COEFFICIENT * BIGNESS_BASE.powf(mean_boil_gravity.sg() - 1.0)
}

/// Tinseth's time term — utilisation rising towards an asymptote.
fn time_factor(minutes: f64) -> f64 {
    (1.0 - (-TIME_RATE * minutes).exp()) / TIME_DIVISOR
}

/// Bitterness contributed by each addition, boil and post-flameout.
///
/// Dry hops contribute nothing — no isomerisation without heat. Whirlpool
/// additions spend no time in the boil; their entire contribution comes from
/// the stand, which is why [`HopUse::Whirlpool`] cannot be modelled as a
/// zero-minute boil addition (§4.5).
pub(crate) fn contributions(
    recipe: &Recipe,
    equipment: &Equipment,
    post_boil_l: f64,
    mean_boil_gravity: Gravity,
) -> Vec<IbuContribution> {
    let bigness = bigness_factor(mean_boil_gravity);

    // Tinseth is defined over the volume the bitterness ends up in, which
    // Appendix A takes as the batch volume.
    let volume_l = recipe.batch_size_l;

    let bittering: Vec<&Hop> = recipe
        .hops
        .iter()
        .filter(|hop| hop.use_ != HopUse::DryHop)
        .collect();

    // Boil time per addition: a whirlpool charge goes in at flameout, so it
    // spends none of the boil isomerising.
    let boil_minutes: Vec<f64> = bittering
        .iter()
        .map(|hop| match hop.use_ {
            HopUse::Boil => f64::from(hop.minutes),
            HopUse::Whirlpool | HopUse::DryHop => 0.0,
        })
        .collect();

    // Alpha acid still un-isomerised when the burner goes out. That is the
    // state the post-flameout model starts from.
    let remaining_alpha: Vec<f64> = boil_minutes
        .iter()
        .map(|m| (-AA_DECAY_PER_MIN * m).exp())
        .collect();

    let post_flameout = crate::mibu::post_flameout_utilisation(
        equipment,
        post_boil_l,
        f64::from(recipe.whirlpool_minutes),
        recipe.whirlpool_temp_c,
        &remaining_alpha,
    );

    bittering
        .iter()
        .zip(boil_minutes.iter())
        .zip(post_flameout.iter())
        .map(|((hop, &minutes), &post_utilisation)| {
            let boil_utilisation = time_factor(minutes);

            // Grams of alpha acid per litre, scaled to IBU's mg/L.
            let concentration = if volume_l > 0.0 {
                hop.grams * hop.alpha_acid * 1000.0 / volume_l
            } else {
                0.0
            };

            let boil_ibu = concentration * bigness * boil_utilisation;
            let post_flameout_ibu = concentration * bigness * post_utilisation;

            IbuContribution {
                name: hop.name.clone(),
                grams: hop.grams,
                minutes: match hop.use_ {
                    HopUse::Whirlpool => recipe.whirlpool_minutes,
                    _ => hop.minutes,
                },
                boil_utilisation,
                post_flameout_utilisation: post_utilisation,
                boil_ibu,
                post_flameout_ibu,
                ibu: boil_ibu + post_flameout_ibu,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equipment::Chilling;
    use crate::recipe::{Fermentable, Yeast};

    fn recipe_with(hops: Vec<Hop>, whirlpool_minutes: u32) -> Recipe {
        Recipe {
            name: "t".into(),
            batch_size_l: 20.5,
            boil_minutes: 60,
            mash_water_l: 15.26,
            fermentables: vec![Fermentable {
                name: "base".into(),
                kg: 4.2,
                ppg: 37.0,
                ebc: 3.6,
            }],
            hops,
            yeast: Yeast {
                name: "y".into(),
                apparent_attenuation: 0.82,
            },
            whirlpool_minutes,
            whirlpool_temp_c: None,
        }
    }

    /// Chilled the instant the burner goes out, so the post-flameout term is
    /// negligible and these tests isolate the boil phase.
    fn no_post_flameout() -> Equipment {
        Equipment {
            chilling: Chilling::Forced {
                rate_per_min: 100.0,
                plateau_c: 20.0,
            },
            ..Equipment::default()
        }
    }

    fn hop(grams: f64, minutes: u32, use_: HopUse) -> Hop {
        Hop {
            name: "Hallertau".into(),
            grams,
            alpha_acid: 0.038,
            minutes,
            use_,
        }
    }

    #[test]
    fn mean_boil_gravity_sits_between_the_two_readings() {
        let m = mean_boil_gravity(Gravity::from_points(37.5), Gravity::from_points(45.0));
        assert!((m.points() - 41.25).abs() < 1e-9);
    }

    #[test]
    fn time_factor_rises_and_flattens() {
        assert!(time_factor(0.0).abs() < 1e-12, "no time, no isomerisation");
        assert!(time_factor(15.0) < time_factor(60.0));
        assert!(time_factor(60.0) < time_factor(90.0));
        // Rises towards 1/4.15 asymptotically. At brew-day times it is still
        // clearly short of the limit; far beyond them the exponential term
        // underflows and f64 lands exactly on it.
        assert!(time_factor(90.0) < 1.0 / TIME_DIVISOR);
        assert!(time_factor(1000.0) <= 1.0 / TIME_DIVISOR);
    }

    #[test]
    fn heavier_wort_extracts_less_bitterness() {
        let light = bigness_factor(Gravity::from_points(30.0));
        let heavy = bigness_factor(Gravity::from_points(60.0));
        assert!(heavy < light, "bigness must fall as gravity rises");
    }

    #[test]
    fn dry_hops_contribute_no_bitterness_and_no_row() {
        let r = recipe_with(vec![hop(50.0, 0, HopUse::DryHop)], 0);
        let c = contributions(&r, &no_post_flameout(), 22.5, Gravity::from_points(41.3));
        assert!(c.is_empty(), "a dry hop is not a bitterness contribution");
    }

    #[test]
    fn whirlpool_additions_use_the_stand_time_not_their_own_minutes() {
        // §4.5: a whirlpool charge's contact time is the stand, which is why
        // it cannot be modelled as a zero-minute boil addition.
        let r = recipe_with(vec![hop(25.0, 0, HopUse::Whirlpool)], 20);
        let c = contributions(&r, &no_post_flameout(), 22.5, Gravity::from_points(41.3));
        assert_eq!(c[0].minutes, 20);
        assert!(c[0].ibu > 0.0, "a 20 minute stand must contribute bitterness");
    }

    #[test]
    fn a_whirlpool_with_no_stand_still_isomerises_while_the_wort_chills() {
        // Plain Tinseth would say zero here. It is not zero: the wort is at
        // boiling the instant the burner goes out and takes time to leave the
        // reaction band. That is the whole point of the post-flameout model.
        let r = recipe_with(vec![hop(25.0, 0, HopUse::Whirlpool)], 0);
        let eq = Equipment {
            chilling: Chilling::ice_bath(22.5),
            ..Equipment::default()
        };
        let c = contributions(&r, &eq, 22.5, Gravity::from_points(41.3));
        assert_eq!(c[0].boil_ibu, 0.0, "it spent no time in the boil");
        assert!(c[0].post_flameout_ibu > 0.0);

        // A stand holds it hot for longer, so it must extract more.
        let stood = recipe_with(vec![hop(25.0, 0, HopUse::Whirlpool)], 20);
        let c2 = contributions(&stood, &eq, 22.5, Gravity::from_points(41.3));
        assert!(c2[0].post_flameout_ibu > c[0].post_flameout_ibu);
    }

    #[test]
    fn bitterness_scales_linearly_with_hop_weight() {
        let single = recipe_with(vec![hop(25.0, 60, HopUse::Boil)], 0);
        let double = recipe_with(vec![hop(50.0, 60, HopUse::Boil)], 0);
        let g = Gravity::from_points(41.3);
        let a = contributions(&single, &no_post_flameout(), 22.5, g)[0].ibu;
        let b = contributions(&double, &no_post_flameout(), 22.5, g)[0].ibu;
        assert!((b - 2.0 * a).abs() < 1e-9);
    }
}
