// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_calc::{Equipment, HopUse, formulation::*};
fn input() -> Inputs {
    Inputs {
        batch_l: 20.0,
        boil_minutes: 60.0,
        mash_water_l: 15.0,
        kettle_top_up_l: 0.0,
        attenuation: Some(0.8),
        fermentables: vec![FermentableInput {
            kg: 4.2,
            yield_fraction: 300.0 / POTENTIAL,
            colour_ebc: 5.0,
            stage: FermentableStage::Mash,
        }],
        hops: vec![],
        whirlpool_minutes: 0.0,
        whirlpool_temperature_c: None,
        equipment: Equipment {
            losses_l: 2.0,
            boil_off_rate_l_per_hour: 3.0,
            kettle_deadspace_l: 0.0,
            grain_absorption_l_per_kg: 0.8,
            mash_efficiency: 0.75,
            ..Equipment::default()
        },
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
#[test]
fn grain_and_flameout_sugar_reference() {
    let mut i = input();
    let r = calculate(&i).unwrap();
    close(r.total_water_l, 28.36);
    close(r.pre_boil_sg, 1.0378);
    close(r.original_sg, 1.0429545454545455);
    close(r.final_sg.unwrap(), 1.008590909090909);
    close(r.ibu, 0.0);
    i.fermentables.push(FermentableInput {
        kg: 0.5,
        yield_fraction: 1.0,
        colour_ebc: 0.0,
        stage: FermentableStage::Boil {
            remaining_minutes: 0.0,
        },
    });
    let mixed = calculate(&i).unwrap();
    close(mixed.pre_boil_sg, 1.0378);
    close(mixed.original_sg, 1.05171987501036);
    close(mixed.grain_absorption_l, 3.36);
}
#[test]
fn extract_only_ignores_efficiency_and_absorption() {
    let mut i = input();
    i.fermentables = vec![FermentableInput {
        kg: 3.0,
        yield_fraction: 300.0 / POTENTIAL,
        colour_ebc: 5.0,
        stage: FermentableStage::Boil {
            remaining_minutes: 60.0,
        },
    }];
    let a = calculate(&i).unwrap();
    i.equipment.mash_efficiency = 0.2;
    i.equipment.grain_absorption_l_per_kg = 10.0;
    let b = calculate(&i).unwrap();
    close(a.original_sg, 1.04090909090909);
    close(a.original_sg, b.original_sg);
    close(a.grain_absorption_l, 0.0);
    close(a.mash_water_l, 0.0);
    close(a.sparge_water_l, 0.0);
}
#[test]
fn event_partition_matches_independent_tinseth_segment_oracle() {
    let mut i = input();
    i.fermentables.push(FermentableInput {
        kg: 0.5,
        yield_fraction: 1.0,
        colour_ebc: 0.0,
        stage: FermentableStage::Boil {
            remaining_minutes: 30.0,
        },
    });
    i.hops.push(HopInput {
        grams: 30.0,
        alpha_fraction: 0.05,
        stage: HopUse::Boil,
        remaining_minutes: 60.0,
    });
    i.equipment.chilling = krausen_calc::Chilling::Forced {
        rate_per_min: 1000.0,
        plateau_c: 20.0,
    };
    let r = calculate(&i).unwrap();
    let first_sg = (1.0 + 945.0 / 25.0 / 1000.0 + 1.0 + 945.0 / 23.5 / 1000.0) / 2.0;
    let second_sg = (1.0
        + (945.0 + 0.5 * POTENTIAL) / 23.5 / 1000.0
        + 1.0
        + (945.0 + 0.5 * POTENTIAL) / 22.0 / 1000.0)
        / 2.0;
    let b = |sg: f64| 1.65 * 0.000125_f64.powf(sg - 1.0);
    let expected = (b(first_sg) * (1.0 - (-1.2_f64).exp())
        + b(second_sg) * ((-1.2_f64).exp() - (-2.4_f64).exp()))
        / 4.15
        * 30.0
        * 0.05
        * 1000.0
        / 20.0;
    assert!((r.ibu - expected).abs() < 0.05, "{} vs {}", r.ibu, expected);
}

#[test]
fn fractional_time_and_nonfinite_results_are_checked() {
    let mut i = input();
    i.boil_minutes = 60.5;
    let r = calculate(&i).unwrap();
    close(r.pre_boil_l, 25.025);
    i.fermentables[0].kg = f64::INFINITY;
    assert!(calculate(&i).is_err());
    i.fermentables[0].kg = 1e300;
    assert!(calculate(&i).is_err());
}
