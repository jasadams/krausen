// SPDX-License-Identifier: AGPL-3.0-or-later
//! KRA-70: documents saved before the equipment spec still load. The decision is that old profile
//! data is not migrated, but saved recipe revisions and frozen brew plans must stay readable.
use krausen_brewery::native_recipe::read_document;

#[test]
fn a_saved_recipe_revision_with_pre_spec_overrides_loads_and_drops_what_the_spec_removed() {
    let saved = r#"{
        "schema_version": 1, "title": "Old", "fermentables": [], "hops": [], "yeast": [], "other_ingredients": [],
        "process": {"mash_steps": [], "fermentation_steps": []},
        "equipment_overrides": {
            "schema_version": 1,
            "efficiency_override_fraction": 0.7,
            "boil_off_l_per_hour": 3.5,
            "transfer_loss_l": 1.5,
            "pre_boil_deadspace_l": 0.5,
            "grain_absorption_l_per_kg": 0.9,
            "kettle_capacity_l": 40.0,
            "kettle_opening_cm": 30.0,
            "boil_temperature_c": 99.0,
            "chilling": {"method": "forced", "rate_per_minute": 0.5, "plateau_c": 20.0},
            "extensions": {}
        },
        "extensions": {}
    }"#;
    let document = read_document(saved).expect("a pre-spec revision still loads");
    let overrides = document.equipment_overrides;
    // The recipe-level efficiency override is unchanged; renamed fields keep their values.
    assert_eq!(overrides.efficiency_override_fraction, Some(0.7));
    assert_eq!(overrides.boil_loss_l_per_hour, Some(3.5));
    assert_eq!(overrides.trub_and_chiller_loss_l, Some(1.5));
    assert_eq!(overrides.mash_tun_deadspace_l, Some(0.5));
    assert_eq!(overrides.mash_grain_absorption_l_per_kg, Some(0.9));
    assert_eq!(overrides.boil_volume_l, Some(40.0));
    // Removed fields (opening, boil temperature, the chilling object) are ignored: no cooling method.
    assert_eq!(overrides.cooling_method, None);
    let reserialized = serde_json::to_value(&overrides).unwrap();
    for removed in ["kettle_opening_cm", "boil_temperature_c", "chilling"] {
        assert!(reserialized.get(removed).is_none(), "{removed}");
    }
}
