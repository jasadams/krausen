// SPDX-License-Identifier: AGPL-3.0-or-later

//! Stage 5 of §4.2 — colour, by Morey.
//!
//! Morey is defined in imperial units over malt colour units, so the grist is
//! converted at the edge and converted back. The volume basis is post-boil,
//! matching OG — see §4.4.

use crate::{
    recipe::Recipe,
    units::{GAL_PER_L, LB_PER_KG},
};

/// EBC -> °Lovibond: `((EBC / 1.97) + 0.76) / 1.3546`.
fn ebc_to_lovibond(ebc: f64) -> f64 {
    ((ebc / 1.97) + 0.76) / 1.3546
}

/// Morey's fit: `SRM = 1.4922 * MCU^0.6859`.
const MOREY_COEFFICIENT: f64 = 1.4922;
const MOREY_EXPONENT: f64 = 0.6859;

/// SRM -> EBC.
const EBC_PER_SRM: f64 = 1.97;

/// Colour as `(SRM, EBC)`.
///
/// `volume_l` must be the post-boil volume. Using batch volume instead reads
/// about 7.27 EBC on the reference recipe where the sheet says 6.82 — the same
/// class of error as computing OG at the wrong volume, and for the same
/// reason.
pub(crate) fn morey(recipe: &Recipe, volume_l: f64) -> (f64, f64) {
    if volume_l <= 0.0 {
        return (0.0, 0.0);
    }

    let volume_gal = volume_l * GAL_PER_L;

    let malt_colour_units: f64 = recipe
        .fermentables
        .iter()
        .map(|f| (f.kg * LB_PER_KG) * ebc_to_lovibond(f.ebc))
        .sum::<f64>()
        / volume_gal;

    let srm = MOREY_COEFFICIENT * malt_colour_units.powf(MOREY_EXPONENT);
    (srm, srm * EBC_PER_SRM)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::{Fermentable, Yeast};

    fn recipe(fermentables: Vec<Fermentable>) -> Recipe {
        Recipe {
            name: "t".into(),
            batch_size_l: 20.5,
            boil_minutes: 60,
            mash_water_l: 15.26,
            fermentables,
            hops: vec![],
            yeast: Yeast {
                name: "y".into(),
                apparent_attenuation: 0.82,
            },
            whirlpool_minutes: 0,
            whirlpool_temp_c: None,
        }
    }

    fn malt(kg: f64, ebc: f64) -> Fermentable {
        Fermentable {
            name: "m".into(),
            kg,
            ppg: 37.0,
            ebc,
        }
    }

    #[test]
    fn volume_basis_changes_the_answer_materially() {
        // The reason §4.4 pins post-boil: the same grist over batch volume
        // reads noticeably darker.
        let r = recipe(vec![malt(3.9, 3.6), malt(0.3, 14.0)]);
        let (_, ebc_post_boil) = morey(&r, 22.5);
        let (_, ebc_batch) = morey(&r, 20.5);
        assert!((ebc_post_boil - 6.82).abs() < 0.01);
        assert!(ebc_batch > ebc_post_boil);
        assert!((ebc_batch - 7.27).abs() < 0.02);
    }

    #[test]
    fn darker_malt_darkens_the_beer() {
        let pale = recipe(vec![malt(4.2, 3.6)]);
        let dark = recipe(vec![malt(4.2, 300.0)]);
        assert!(morey(&dark, 22.5).1 > morey(&pale, 22.5).1);
    }

    #[test]
    fn more_water_lightens_the_beer() {
        let r = recipe(vec![malt(4.2, 3.6)]);
        assert!(morey(&r, 45.0).1 < morey(&r, 22.5).1);
    }

    #[test]
    fn srm_and_ebc_stay_in_step() {
        let r = recipe(vec![malt(4.2, 3.6)]);
        let (srm, ebc) = morey(&r, 22.5);
        assert!((ebc - srm * 1.97).abs() < 1e-9);
    }

    #[test]
    fn zero_volume_is_zero_rather_than_a_division_by_zero() {
        let r = recipe(vec![malt(4.2, 3.6)]);
        assert_eq!(morey(&r, 0.0), (0.0, 0.0));
    }
}
