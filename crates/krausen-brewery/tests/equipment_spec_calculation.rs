// SPDX-License-Identifier: AGPL-3.0-or-later
//! KRA-70: the spec's water chain, capacity warnings and cooling methods
//! (docs/CALCULATIONS.md, "Equipment profile V2").
use krausen_brewery::{mapping::calculation::targets_with_defaults, validation::validate_draft};
use krausen_types::{
    brewery::*,
    formulation::{AppliedDefault, Targets, Vessel},
    validation::{Readiness, Target},
};

fn recipe() -> RecipeDocumentV1 {
    let mut d = RecipeDocumentV1::default();
    d.batch_size_l = Some(20.0);
    d.boil_minutes = Some(60.0);
    d.process.mash_water_method = Some("manual".into());
    d.process.mash_water_l = Some(15.0);
    d.process.apparent_attenuation_fraction = Some(0.8);
    d.fermentables.push(FermentableAddition {
        row_id: "g".into(),
        amount_kg: Some(4.2),
        addition: AdditionStage {
            stage: "mash".into(),
            time_minutes: None,
        },
        specification: FermentableSpecification {
            name: None,
            producer: None,
            kind: Some("grain".into()),
            yield_fraction: Some(0.8),
            colour_ebc: Some(5.0),
        },
        source: None,
    });
    d
}
fn hopped() -> RecipeDocumentV1 {
    let mut d = recipe();
    d.hops.push(HopAddition {
        row_id: "h".into(),
        amount_g: Some(30.0),
        stage: "whirlpool".into(),
        contact_minutes: Some(0.0),
        temperature_c: None,
        specification: HopSpecification {
            name: None,
            producer: None,
            alpha_fraction: Some(0.12),
        },
        source: None,
    });
    d.process.whirlpool_minutes = Some(20.0);
    d
}
/// Today's fields only (the 20 L worked case of CALCULATIONS.md).
fn base() -> EquipmentSettingsV1 {
    EquipmentSettingsV1 {
        mash_efficiency_fraction: Some(0.75),
        boil_loss_l_per_hour: Some(3.0),
        trub_and_chiller_loss_l: Some(2.0),
        mash_tun_deadspace_l: Some(0.0),
        mash_grain_absorption_l_per_kg: Some(0.8),
        kettle_diameter_cm: Some(30.0),
        cooling_method: Some(CoolingMethodV1::Natural),
        ..Default::default()
    }
}
fn calc(d: &RecipeDocumentV1, e: &EquipmentSettingsV1) -> (Targets, Vec<AppliedDefault>, Readiness) {
    let mut v = validate_draft(d, e);
    let mut defaults = vec![];
    let t = targets_with_defaults(d, e, &mut v, &mut defaults);
    (t, defaults, v.state)
}
fn close(a: Option<f64>, b: f64) {
    assert!((a.unwrap() - b).abs() < 1e-9, "{a:?} vs {b}");
}

#[test]
fn the_spec_losses_absent_give_the_earlier_water_chain_and_are_recorded_as_defaults() {
    let (t, defaults, state) = calc(&recipe(), &base());
    assert_eq!(state, Readiness::Ready);
    close(t.post_boil_l, 22.0);
    close(t.pre_boil_l, 25.0);
    close(t.total_water_l, 28.36);
    close(t.sparge_water_l, 13.36);
    assert!(t.vessel_warnings.is_empty());
    let paths: Vec<_> = defaults.iter().map(|d| d.path.as_str()).collect();
    for path in [
        "equipment.wort_shrinkage_fraction",
        "equipment.mash_tun_loss_l",
        "equipment.sparge_deadspace_l",
        "equipment.sparge_grain_absorption_l_per_kg",
    ] {
        assert!(paths.contains(&path), "{path} recorded");
    }
}

#[test]
fn each_water_stage_adds_its_own_term() {
    let mut e = base();
    e.mash_tun_deadspace_l = Some(3.5);
    e.mash_tun_loss_l = Some(0.5);
    e.sparge_deadspace_l = Some(1.0);
    e.sparge_grain_absorption_l_per_kg = Some(0.1);
    let (t, _, _) = calc(&recipe(), &e);
    // 25 + 4.2 × (0.8 + 0.1) + 3.5 + 0.5 + 1
    close(t.total_water_l, 33.78);
    close(t.grain_absorption_l, 3.78);
}

#[test]
fn shrinkage_converts_the_hot_boil_loss_to_the_reference_basis_and_leaves_og_alone() {
    let (without, _, _) = calc(&recipe(), &base());
    let mut e = base();
    e.wort_shrinkage_fraction = Some(0.04);
    let (with, _, _) = calc(&recipe(), &e);
    // pre-boil = 22 + 3 × 0.96
    close(with.pre_boil_l, 24.88);
    close(with.total_water_l, 24.88 + 3.36);
    assert_eq!(with.original_sg, without.original_sg);
    assert!(with.pre_boil_sg > without.pre_boil_sg, "less volume, denser wort");
}

#[test]
fn overfilled_vessels_are_warnings_and_unset_volumes_are_not_checked() {
    let mut e = base();
    e.wort_shrinkage_fraction = Some(0.04);
    e.sparge_deadspace_l = Some(1.0);
    e.sparge_vessel_volume_l = Some(10.0);
    e.mash_tun_volume_l = Some(15.0);
    e.boil_volume_l = Some(25.0);
    let (t, _, state) = calc(&recipe(), &e);
    // Warnings never change readiness.
    assert_eq!(state, Readiness::Ready);
    let find = |vessel| t.vessel_warnings.iter().find(|w| w.vessel == vessel);
    // mash 15 + 4.2 × 0.67 = 17.814 > 15
    assert!((find(Vessel::MashTun).unwrap().required_l - 17.814).abs() < 1e-9);
    // sparge = 24.88 + 3.36 + 1 - 15 = 14.24 > 10
    assert!((find(Vessel::SpargeVessel).unwrap().required_l - 14.24).abs() < 1e-9);
    // kettle peak = pre-boil hot = 24.88 / 0.96 = 25.9167 > 25
    assert!((find(Vessel::Kettle).unwrap().required_l - 24.88 / 0.96).abs() < 1e-9);
    let (none, _, _) = calc(&recipe(), &base());
    assert!(none.vessel_warnings.is_empty());
    e.mash_tun_volume_l = Some(17.814);
    e.boil_volume_l = Some(24.88 / 0.96);
    let (exact, _, _) = calc(&recipe(), &e);
    assert!(
        exact
            .vessel_warnings
            .iter()
            .all(|w| w.vessel == Vessel::SpargeVessel),
        "a vessel filled exactly is not overfilled"
    );
}

#[test]
fn cooling_method_resolves_to_a_curve_and_counterflow_needs_its_flow_rate() {
    let ibu = |method: CoolingMethodV1, flow: Option<f64>| {
        let mut e = base();
        e.cooling_method = Some(method);
        e.cooling_flow_rate_l_per_minute = flow;
        calc(&hopped(), &e)
    };
    let (natural, _, _) = ibu(CoolingMethodV1::Natural, None);
    let (immersion, _, _) = ibu(CoolingMethodV1::ImmersionChiller, None);
    let (ice, _, _) = ibu(CoolingMethodV1::IceBath, None);
    let (counterflow, _, state) = ibu(CoolingMethodV1::CounterflowChiller, Some(1.0));
    assert_eq!(state, Readiness::Ready);
    let values: Vec<f64> = [&natural, &immersion, &ice, &counterflow]
        .iter()
        .map(|t| t.ibu.unwrap())
        .collect();
    assert!(natural.ibu.unwrap() > immersion.ibu.unwrap(), "{values:?}");
    assert!(immersion.ibu.unwrap() > ice.ibu.unwrap(), "{values:?}");
    assert!(values.iter().all(|v| v.is_finite() && *v > 0.0));
    let (missing, _, state) = ibu(CoolingMethodV1::CounterflowChiller, None);
    assert_eq!(state, Readiness::Incomplete);
    assert_eq!(missing.ibu, None);
    // Hops without any cooling method are incomplete; water is not affected.
    let mut e = base();
    e.cooling_method = None;
    let (no_method, _, state) = calc(&hopped(), &e);
    assert_eq!(state, Readiness::Incomplete);
    assert_eq!(no_method.ibu, None);
    assert!(no_method.total_water_l.is_some());
    assert!(!validate_draft(&hopped(), &e).target_available(Target::Bitterness));
}

#[test]
fn the_fixed_assumptions_replace_opening_and_boil_temperature() {
    // With an uncovered kettle and a 100 °C boil the profile needs only its
    // diameter: the same document is Ready without any opening/temperature.
    let (t, _, state) = calc(&hopped(), &base());
    assert_eq!(state, Readiness::Ready);
    assert!(t.ibu.is_some());
}
