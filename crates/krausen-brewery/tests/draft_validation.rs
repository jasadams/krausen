// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_brewery::validation::{Readiness, Target, validate_draft};
use krausen_types::brewery::*;

fn complete() -> (RecipeDocumentV1, EquipmentSettingsV1) {
    let mut r = RecipeDocumentV1::default();
    r.batch_size_l = Some(20.0);
    r.boil_minutes = Some(60.0);
    r.fermentables.push(FermentableAddition {
        row_id: "malt-1".into(),
        amount_kg: Some(4.0),
        addition: AdditionStage {
            stage: "mash".into(),
            time_minutes: None,
        },
        specification: FermentableSpecification {
            name: Some("Malt".into()),
            producer: None,
            kind: Some("grain".into()),
            yield_fraction: Some(0.8),
            colour_ebc: Some(5.0),
        },
        source: None,
    });
    r.yeast.push(IngredientAddition {
        row_id: "yeast-1".into(),
        amount: None,
        unit: None,
        specification: IngredientSpecification {
            name: None,
            producer: None,
            attenuation_fraction: Some(0.8),
            extensions: Default::default(),
        },
        source: None,
    });
    r.process.mash_water_method = Some("manual".into());
    r.process.mash_water_l = Some(12.0);
    r.process.apparent_attenuation_fraction = Some(0.8);
    let mut e = EquipmentSettingsV1::default();
    e.mash_efficiency_fraction = Some(0.75);
    e.boil_off_l_per_hour = Some(3.0);
    e.transfer_loss_l = Some(2.0);
    e.pre_boil_deadspace_l = Some(0.0);
    e.grain_absorption_l_per_kg = Some(0.8);
    (r, e)
}

#[test]
fn ready_and_incomplete_are_saveable_but_only_ready_starts() {
    let (mut r, e) = complete();
    let ready = validate_draft(&r, &e);
    assert_eq!(ready.state, Readiness::Ready);
    assert!(ready.saveable() && ready.startable());
    r.fermentables[0].amount_kg = Some(0.0);
    let incomplete = validate_draft(&r, &e);
    assert_eq!(incomplete.state, Readiness::Incomplete);
    assert!(incomplete.saveable() && !incomplete.startable());
    assert!(
        incomplete
            .diagnostics
            .iter()
            .any(|d| d.code == "unfinished_quantity" && d.path == "fermentables[malt-1].amount_kg")
    );
}

#[test]
fn nonfinite_and_conflicting_schedule_are_invalid() {
    let (mut r, e) = complete();
    r.fermentables[0].amount_kg = Some(f64::NAN);
    let result = validate_draft(&r, &e);
    assert_eq!(result.state, Readiness::Invalid);
    assert!(!result.saveable());
    r.fermentables[0].amount_kg = Some(4.0);
    r.process.mash_water_l = Some(100.0);
    assert!(
        validate_draft(&r, &e)
            .diagnostics
            .iter()
            .any(|d| d.code == "negative_sparge")
    );
}

#[test]
fn missing_hop_input_does_not_hide_water_and_no_hops_is_valid() {
    let (mut r, mut e) = complete();
    assert_eq!(validate_draft(&r, &e).state, Readiness::Ready);
    r.hops.push(HopAddition {
        row_id: "hop-2".into(),
        amount_g: Some(5.0),
        stage: "boil".into(),
        contact_minutes: Some(30.5),
        temperature_c: None,
        specification: HopSpecification {
            name: None,
            producer: None,
            alpha_fraction: None,
        },
        source: None,
    });
    e.chilling = Some(ChillingV1::Natural);
    e.kettle_diameter_cm = Some(30.0);
    e.kettle_opening_cm = Some(30.0);
    e.boil_temperature_c = Some(100.0);
    let result = validate_draft(&r, &e);
    assert_eq!(result.state, Readiness::Incomplete);
    assert!(result.target_available(Target::Water));
    assert!(!result.target_available(Target::Bitterness));
    r.hops[0].specification.alpha_fraction = Some(0.0);
    r.hops[0].contact_minutes = Some(61.0);
    assert!(
        validate_draft(&r, &e)
            .diagnostics
            .iter()
            .any(|d| d.code == "exposure_exceeds_boil")
    );
}

#[test]
fn unsupported_process_and_missing_equipment_never_become_ready() {
    let (mut r, mut e) = complete();
    r.process
        .unsupported
        .insert("vendor".into(), serde_json::json!({"step": "unknown"}));
    assert_eq!(validate_draft(&r, &e).state, Readiness::Incomplete);
    r.process.unsupported.clear();
    e.boil_off_l_per_hour = None;
    assert_eq!(validate_draft(&r, &e).state, Readiness::Incomplete);
}

#[test]
fn attenuation_is_explicit_and_never_inferred_from_yeast() {
    let (mut r, e) = complete();
    r.process.apparent_attenuation_fraction = None;
    let result = validate_draft(&r, &e);
    assert_eq!(result.state, Readiness::Incomplete);
    assert!(!result.target_available(Target::Alcohol));
    assert!(result.target_available(Target::Gravity));
    assert!(result.target_available(Target::Water));
    r.process.apparent_attenuation_fraction = Some(1.1);
    assert_eq!(validate_draft(&r, &e).state, Readiness::Invalid);
}

#[test]
fn rejects_invalid_overrides_and_preserves_invalid_over_incomplete() {
    let (mut r, e) = complete();
    r.process.apparent_attenuation_fraction = None;
    r.equipment_overrides.boil_off_l_per_hour = Some(-1.0);
    let result = validate_draft(&r, &e);
    assert_eq!(result.state, Readiness::Invalid);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.path == "equipment_overrides.boil_off_l_per_hour" && d.code == "negative")
    );
}

#[test]
fn stage_vocabulary_retains_unsupported_inputs_and_checks_boil_remaining_time() {
    let (mut r, e) = complete();
    r.fermentables[0].specification.kind = Some("extract".into());
    r.fermentables[0].addition.stage = "boil".into();
    r.fermentables[0].addition.time_minutes = Some(30.5);
    assert_eq!(validate_draft(&r, &e).state, Readiness::Ready);
    r.fermentables[0].addition.time_minutes = Some(61.0);
    assert_eq!(validate_draft(&r, &e).state, Readiness::Invalid);
    r.fermentables[0].addition.stage = "fermentation".into();
    let result = validate_draft(&r, &e);
    assert_eq!(result.state, Readiness::Incomplete);
    assert_eq!(r.fermentables[0].addition.stage, "fermentation");
}
