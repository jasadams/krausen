// SPDX-License-Identifier: AGPL-3.0-or-later

use krausen_calc::{
    CalcError, Chilling, Equipment, Fermentable, Hop, HopUse, Recipe, Yeast, derive,
};

fn recipe() -> Recipe {
    Recipe {
        name: "Boundary".into(),
        batch_size_l: 20.0,
        boil_minutes: 60,
        mash_water_l: 12.0,
        fermentables: vec![Fermentable {
            name: "Malt".into(),
            kg: 4.0,
            ppg: 37.0,
            ebc: 4.0,
        }],
        hops: vec![],
        yeast: Yeast {
            name: "Yeast".into(),
            apparent_attenuation: 0.8,
        },
        whirlpool_minutes: 0,
        whirlpool_temp_c: None,
    }
}

fn invalid_field(recipe: &Recipe, equipment: &Equipment, expected: &str) {
    assert!(
        matches!(derive(recipe, equipment), Err(CalcError::InvalidInput { field, .. }) if field == expected)
    );
}

#[test]
fn rejects_nonfinite_values_before_they_can_poison_results() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut r = recipe();
        r.batch_size_l = value;
        invalid_field(&r, &Equipment::default(), "batch_size_l");
        r = recipe();
        r.fermentables[0].ebc = value;
        invalid_field(&r, &Equipment::default(), "fermentables[0].ebc");
        let mut eq = Equipment::default();
        eq.mash_efficiency = value;
        invalid_field(&recipe(), &eq, "mash_efficiency");
    }
}

#[test]
fn validates_each_row_and_hop_exposure() {
    let mut r = recipe();
    r.fermentables.push(Fermentable {
        name: "Second".into(),
        kg: -1.0,
        ppg: 37.0,
        ebc: 0.0,
    });
    invalid_field(&r, &Equipment::default(), "fermentables[1].kg");
    r = recipe();
    r.hops.push(Hop {
        name: "Hop".into(),
        grams: 10.0,
        alpha_acid: 1.01,
        minutes: 60,
        use_: HopUse::Boil,
    });
    invalid_field(&r, &Equipment::default(), "hops[0].alpha_acid");
    r.hops[0].alpha_acid = 0.0;
    r.hops[0].minutes = 61;
    invalid_field(&r, &Equipment::default(), "hops[0].minutes");
    r.hops[0].minutes = 60;
    assert!(derive(&r, &Equipment::default()).is_ok());
}

#[test]
fn invalid_chilling_and_geometry_are_not_replaced_by_defaults() {
    let r = recipe();
    let mut eq = Equipment::default();
    eq.chilling = Chilling::Counterflow {
        litres_per_minute: 0.0,
    };
    invalid_field(&r, &eq, "chilling.litres_per_minute");
    eq.chilling = Chilling::Forced {
        rate_per_min: 0.0,
        plateau_c: 20.0,
    };
    invalid_field(&r, &eq, "chilling.rate_per_min");
    eq.chilling = Chilling::Forced {
        rate_per_min: 1.0,
        plateau_c: eq.boil_temp_c,
    };
    invalid_field(&r, &eq, "chilling.plateau_c");
    eq = Equipment::default();
    eq.kettle_opening_cm = eq.kettle_diameter_cm + 1.0;
    invalid_field(&r, &eq, "kettle_opening_cm");
}

#[test]
fn no_hops_is_zero_ibu_and_zero_optional_losses_are_valid() {
    let mut eq = Equipment::default();
    eq.losses_l = 0.0;
    eq.kettle_deadspace_l = 0.0;
    let result = derive(&recipe(), &eq).expect("valid hop-free recipe");
    assert_eq!(result.ibu_total, 0.0);
}

#[test]
fn rejects_gravity_outside_abv_formula_domain() {
    let mut r = recipe();
    r.fermentables[0].ppg = 10_000.0;
    invalid_field(&r, &Equipment::default(), "gravity");
}
