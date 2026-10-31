// SPDX-License-Identifier: AGPL-3.0-or-later

//! Stages 2 and 3 of §4.2 — extract, gravity, attenuation, and the
//! pre-boil corrections.

use crate::{
    derived::{Corrections, FermentablePoints, Volumes},
    equipment::Equipment,
    recipe::Recipe,
    units::{Gravity, PPG_TO_POINTS_L_PER_KG},
};

pub(crate) struct Extract {
    pub fermentable_points: Vec<FermentablePoints>,
    pub potential_points_l: f64,
    pub kettle_points_l: f64,
    pub pre_boil_gravity: Gravity,
    pub original_gravity: Gravity,
}

/// Grist potential, then gravity at the two volumes that matter.
///
/// Working in points throughout means gravity is just `points / volume`, with
/// no unit conversion anywhere in the chain (§4.3).
pub(crate) fn compute(recipe: &Recipe, equipment: &Equipment, volumes: &Volumes) -> Extract {
    let mut fermentable_points: Vec<FermentablePoints> = recipe
        .fermentables
        .iter()
        .map(|f| {
            let points_per_kg_per_l = f.ppg * PPG_TO_POINTS_L_PER_KG;
            FermentablePoints {
                name: f.name.clone(),
                kg: f.kg,
                points_per_kg_per_l,
                potential_points_l: f.kg * points_per_kg_per_l,
                fraction_of_grist: 0.0, // filled in below, once the total is known
            }
        })
        .collect();

    let potential_points_l: f64 = fermentable_points.iter().map(|f| f.potential_points_l).sum();

    if potential_points_l > 0.0 {
        for f in &mut fermentable_points {
            f.fraction_of_grist = f.potential_points_l / potential_points_l;
        }
    }

    // Efficiency is the one number standing between theoretical extract and
    // what actually makes it into the kettle.
    let kettle_points_l = potential_points_l * equipment.mash_efficiency;

    // The same sugar, measured at two different volumes. Pre-boil is the
    // adjustment window; OG is where it lands.
    let pre_boil_gravity = Gravity::from_points(kettle_points_l / volumes.pre_boil_l);
    let original_gravity = Gravity::from_points(kettle_points_l / volumes.post_boil_l);

    Extract {
        fermentable_points,
        potential_points_l,
        kettle_points_l,
        pre_boil_gravity,
        original_gravity,
    }
}

/// `FG = OG - (OG - 1) * apparent_attenuation`, in points.
pub(crate) fn final_gravity(og: Gravity, apparent_attenuation: f64) -> Gravity {
    Gravity::from_points(og.points() * (1.0 - apparent_attenuation))
}

/// ABV by the standard empirical formula.
///
/// `(76.08 * (OG-FG) / (1.775-OG)) * (FG / 0.794)`, which is defined over
/// specific gravity rather than points.
pub(crate) fn abv(og: Gravity, fg: Gravity) -> f64 {
    let og_sg = og.sg();
    let fg_sg = fg.sg();
    (76.08 * (og_sg - fg_sg) / (1.775 - og_sg)) * (fg_sg / 0.794)
}

/// Grams of DME to move pre-boil gravity one point.
///
/// `pre_boil_L * 0.264172 * 453.592 / 45` — DME contributes about 45 points
/// per pound per gallon, so this converts "one point across this volume" into
/// grams. The factors compose to 2.6628068; KRAUSEN.md Appendix A rounds this
/// to 2.6626, which is off in the fourth decimal but changes no sheet figure.
const DME_GRAMS_PER_POINT_PER_LITRE: f64 = 0.264172 * 453.592 / 45.0;

/// Both mid-boil corrections, scaled to this batch.
///
/// Derived on every call, never stored: they are a function of pre-boil
/// volume, which is a function of boil time.
pub(crate) fn corrections(volumes: &Volumes, pre_boil_gravity: Gravity) -> Corrections {
    let dme_grams_per_point = volumes.pre_boil_l * DME_GRAMS_PER_POINT_PER_LITRE;

    // Diluting spreads the same sugar over more water: to lose one point,
    // add volume in proportion to how many points are already there.
    let points = pre_boil_gravity.points();
    let dilution_litres_per_point = if points > 0.0 {
        volumes.pre_boil_l / points
    } else {
        0.0
    };

    Corrections {
        dme_grams_per_point,
        dilution_litres_per_point,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dme_constant_composes_to_the_documented_value() {
        // KRAUSEN.md Appendix A writes the composed constant as 2.6626; the
        // factors it gives actually compose to 2.662807. The difference is in
        // the fourth decimal place and changes no sheet figure (27.0 L still
        // gives 71.9 g/point, 25.5 L still gives 67.9), so the factors are
        // kept as the source of truth rather than the rounded constant.
        assert!((DME_GRAMS_PER_POINT_PER_LITRE - 2.6628068).abs() < 1e-6);

        // Both reference variants still round to the figure on the sheet.
        let to_1dp = |v: f64| (v * 10.0).round() / 10.0;
        assert!((to_1dp(27.0 * DME_GRAMS_PER_POINT_PER_LITRE) - 71.9).abs() < 1e-9);
        assert!((to_1dp(25.5 * DME_GRAMS_PER_POINT_PER_LITRE) - 67.9).abs() < 1e-9);
    }

    #[test]
    fn final_gravity_attenuates_the_points_not_the_sg() {
        // 82% attenuation of a 45-point wort leaves 18% of the points.
        let fg = final_gravity(Gravity::from_points(45.0), 0.82);
        assert!((fg.points() - 8.1).abs() < 1e-9);
    }

    #[test]
    fn zero_attenuation_leaves_gravity_untouched() {
        let fg = final_gravity(Gravity::from_points(45.0), 0.0);
        assert!((fg.points() - 45.0).abs() < 1e-9);
    }

    #[test]
    fn full_attenuation_reaches_water() {
        let fg = final_gravity(Gravity::from_points(45.0), 1.0);
        assert!(fg.points().abs() < 1e-9);
    }

    #[test]
    fn abv_is_zero_when_nothing_fermented() {
        let g = Gravity::from_points(45.0);
        assert!(abv(g, g).abs() < 1e-9);
    }

    #[test]
    fn dilution_is_zero_for_plain_water_rather_than_infinite() {
        let volumes = Volumes {
            batch_l: 20.5,
            post_boil_l: 22.5,
            pre_boil_l: 27.0,
            grain_absorption_l: 3.36,
            deadspace_l: 0.0,
            total_water_l: 30.36,
            mash_water_l: 15.26,
            sparge_water_l: 15.1,
            boil_off_l: 4.5,
        };
        let c = corrections(&volumes, Gravity::from_points(0.0));
        assert_eq!(c.dilution_litres_per_point, 0.0);
    }
}
