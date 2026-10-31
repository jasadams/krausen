// SPDX-License-Identifier: AGPL-3.0-or-later

//! Stage 1 of §4.2 — the water chain.
//!
//! Everything else depends on these, which is why boil time (through boil-off)
//! reaches so far downstream.

use crate::{derived::Volumes, equipment::Equipment, error::CalcError, recipe::Recipe};

pub(crate) fn compute(recipe: &Recipe, equipment: &Equipment) -> Result<Volumes, CalcError> {
    let batch_l = recipe.batch_size_l;

    // Into the kettle at flameout: what reaches the fermenter, plus what does
    // not survive the transfer.
    let post_boil_l = batch_l + equipment.losses_l;

    // The boil removes wort at a constant rate, so a longer boil needs a
    // larger pre-boil volume to land on the same post-boil volume.
    let boil_off_l = equipment.boil_off_rate_l_per_hour * recipe.boil_hours();
    let pre_boil_l = post_boil_l + boil_off_l;

    let grain_absorption_l = equipment.grain_absorption_l_per_kg * recipe.total_grain_kg();
    let deadspace_l = equipment.kettle_deadspace_l;

    // Everything poured in has to account for what the boil takes, what the
    // grain keeps, and what the kettle keeps.
    let total_water_l = pre_boil_l + grain_absorption_l + deadspace_l;

    let mash_water_l = recipe.mash_water_l;
    if mash_water_l > total_water_l {
        return Err(CalcError::MashWaterExceedsTotal {
            mash_water_l,
            total_water_l,
        });
    }
    let sparge_water_l = total_water_l - mash_water_l;

    Ok(Volumes {
        batch_l,
        post_boil_l,
        pre_boil_l,
        grain_absorption_l,
        deadspace_l,
        total_water_l,
        mash_water_l,
        sparge_water_l,
        boil_off_l,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::{Fermentable, Yeast};

    fn recipe(boil_minutes: u32, mash_water_l: f64) -> Recipe {
        Recipe {
            name: "t".into(),
            batch_size_l: 20.5,
            boil_minutes,
            mash_water_l,
            fermentables: vec![Fermentable {
                name: "base".into(),
                kg: 4.2,
                ppg: 37.0,
                ebc: 3.6,
            }],
            hops: vec![],
            yeast: Yeast {
                name: "y".into(),
                apparent_attenuation: 0.82,
            },
            whirlpool_minutes: 0,
            whirlpool_temp_c: None,
        }
    }

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
    fn post_boil_is_batch_plus_losses_and_ignores_boil_time() {
        let eq = equipment();
        let a = compute(&recipe(90, 15.26), &eq).expect("computes");
        let b = compute(&recipe(60, 15.26), &eq).expect("computes");
        assert!((a.post_boil_l - 22.5).abs() < 1e-9);
        assert!((b.post_boil_l - 22.5).abs() < 1e-9);
    }

    #[test]
    fn boil_off_scales_with_boil_time() {
        let eq = equipment();
        let ninety = compute(&recipe(90, 15.26), &eq).expect("computes");
        let sixty = compute(&recipe(60, 15.26), &eq).expect("computes");
        assert!((ninety.boil_off_l - 4.5).abs() < 1e-9, "3 L/hr for 1.5 h");
        assert!((sixty.boil_off_l - 3.0).abs() < 1e-9, "3 L/hr for 1 h");
        assert!((ninety.pre_boil_l - sixty.pre_boil_l - 1.5).abs() < 1e-9);
    }

    #[test]
    fn total_water_accounts_for_absorption_and_deadspace() {
        let mut eq = equipment();
        eq.kettle_deadspace_l = 1.0;
        let v = compute(&recipe(60, 15.26), &eq).expect("computes");
        // 25.5 pre-boil + 3.36 absorbed + 1.0 deadspace
        assert!((v.total_water_l - 29.86).abs() < 1e-9);
    }

    #[test]
    fn sparge_is_the_remainder_after_mash_water() {
        let v = compute(&recipe(60, 15.26), &equipment()).expect("computes");
        assert!((v.mash_water_l + v.sparge_water_l - v.total_water_l).abs() < 1e-9);
    }

    #[test]
    fn mash_water_larger_than_total_is_rejected_not_clamped() {
        // A negative sparge is not a number to paper over — it means the
        // water schedule is wrong and the brewer needs to know.
        let err = compute(&recipe(60, 999.0), &equipment()).expect_err("rejects");
        assert!(matches!(err, CalcError::MashWaterExceedsTotal { .. }));
    }

    #[test]
    fn absorption_scales_with_grain_not_volume() {
        let mut r = recipe(60, 15.26);
        r.fermentables[0].kg = 8.4; // double the grist
        let v = compute(&r, &equipment()).expect("computes");
        assert!((v.grain_absorption_l - 6.72).abs() < 1e-9);
    }
}
