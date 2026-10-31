// SPDX-License-Identifier: AGPL-3.0-or-later

//! Plugging in new equipment must mean supplying data, not writing code.
//!
//! These tests exercise the path an application actually takes: read a preset
//! from a config file or a database row, put it in the catalogue, adopt it,
//! and derive with it.

use krausen_calc::{
    derive, Chilling, EquipmentField, EquipmentPreset, Fermentable, PresetCatalogue, ProfileSource,
    Recipe, Yeast,
};

/// A preset exactly as an application might ship it — a file on disk, a row in
/// a table, a response from an API. Nothing here is Rust.
const SHED_KETTLE_JSON: &str = r#"
{
  "id": "shed-kettle-30l",
  "name": "30 L shed kettle",
  "maker": "Anonymous Welder",
  "notes": "Dimensions measured with a tape. Everything else is a guess until there are brews to fit.",
  "typical_batch_l": 23.0,
  "equipment": {
    "boil_off_rate_l_per_hour": 2.6,
    "grain_absorption_l_per_kg": 0.85,
    "kettle_deadspace_l": 1.2,
    "losses_l": 1.8,
    "mash_efficiency": 0.72,
    "kettle_diameter_cm": 34.0,
    "kettle_opening_cm": 34.0,
    "boil_temp_c": 99.4,
    "chilling": { "Forced": { "rate_per_min": 0.12, "plateau_c": 22.0 } }
  },
  "measured": ["kettle_geometry", "kettle_deadspace", "boil_temperature"]
}
"#;

fn test_recipe() -> Recipe {
    Recipe {
        name: "plug-in smoke test".into(),
        batch_size_l: 23.0,
        boil_minutes: 60,
        mash_water_l: 16.0,
        fermentables: vec![Fermentable {
            name: "base".into(),
            kg: 5.0,
            ppg: 37.0,
            ebc: 3.5,
        }],
        hops: vec![],
        yeast: Yeast {
            name: "y".into(),
            apparent_attenuation: 0.75,
        },
        whirlpool_minutes: 0,
        whirlpool_temp_c: None,
    }
}

#[test]
fn a_preset_loads_from_config_and_derives() {
    let preset: EquipmentPreset =
        serde_json::from_str(SHED_KETTLE_JSON).expect("preset deserialises");

    assert_eq!(preset.id, "shed-kettle-30l");
    assert_eq!(preset.maker.as_deref(), Some("Anonymous Welder"));
    assert_eq!(preset.equipment.boil_off_rate_l_per_hour, 2.6);
    assert_eq!(preset.equipment.boil_temp_c, 99.4);
    assert_eq!(
        preset.equipment.chilling,
        Chilling::Forced {
            rate_per_min: 0.12,
            plateau_c: 22.0
        }
    );

    let mut catalogue = PresetCatalogue::with_builtins();
    catalogue.insert(preset);

    let (equipment, source) = catalogue
        .get("shed-kettle-30l")
        .expect("in the catalogue")
        .adopt();

    assert_eq!(
        source,
        ProfileSource::Preset {
            id: "shed-kettle-30l".into()
        }
    );

    let derived = derive(&test_recipe(), &equipment).expect("derives");
    assert!(derived.original_gravity.points() > 0.0);
    assert!((derived.volumes.deadspace_l - 1.2).abs() < 1e-9);
}

#[test]
fn a_preset_states_which_of_its_numbers_are_real() {
    let preset: EquipmentPreset = serde_json::from_str(SHED_KETTLE_JSON).expect("deserialises");

    // Measured with a tape — a brew disagreeing with these means something
    // different from a brew disagreeing with a guess (§3.3).
    assert!(preset.is_measured(EquipmentField::KettleGeometry));
    assert!(preset.is_measured(EquipmentField::KettleDeadspace));
    assert!(preset.is_measured(EquipmentField::BoilTemperature));

    // Guesses, and the ones Phase 3 should fit and replace.
    let estimated = preset.estimated();
    assert!(estimated.contains(&EquipmentField::MashEfficiency));
    assert!(estimated.contains(&EquipmentField::BoilOffRate));
    assert!(estimated.contains(&EquipmentField::GrainAbsorption));

    // The useful property: what this preset guesses is mostly what a
    // regression over logged brews can recover, so the guesses are temporary.
    let fittable_guesses = estimated.iter().filter(|f| f.is_fittable()).count();
    assert_eq!(
        fittable_guesses, 4,
        "boil-off, absorption, losses and efficiency are all recoverable"
    );
}

#[test]
fn a_profile_survives_the_round_trip_a_brew_snapshot_needs() {
    // §4.6: a brew embeds a full copy of the profile it was brewed with. That
    // copy has to serialise and come back identical, or old bench sheets stop
    // reproducing.
    let preset: EquipmentPreset = serde_json::from_str(SHED_KETTLE_JSON).expect("deserialises");
    let (equipment, source) = preset.adopt();

    let encoded = serde_json::to_string(&equipment).expect("serialises");
    let decoded: krausen_calc::Equipment = serde_json::from_str(&encoded).expect("round trips");
    assert_eq!(decoded, equipment);

    let encoded_source = serde_json::to_string(&source).expect("serialises");
    let decoded_source: ProfileSource =
        serde_json::from_str(&encoded_source).expect("round trips");
    assert_eq!(decoded_source, source);

    // And the snapshot must still produce the same numbers.
    let before = derive(&test_recipe(), &equipment).expect("derives");
    let after = derive(&test_recipe(), &decoded).expect("derives");
    assert_eq!(before, after);
}

#[test]
fn an_application_can_override_a_builtin_without_forking() {
    let mut catalogue = PresetCatalogue::with_builtins();
    let builtin_count = catalogue.len();

    let mut corrected = catalogue
        .get("generic-homebrew-23l")
        .expect("built in")
        .clone();
    corrected.equipment.boil_off_rate_l_per_hour = 2.2;
    catalogue.insert(corrected);

    assert_eq!(catalogue.len(), builtin_count, "replaced, not added");
    assert_eq!(
        catalogue
            .get("generic-homebrew-23l")
            .expect("still there")
            .equipment
            .boil_off_rate_l_per_hour,
        2.2
    );
}

#[test]
fn editing_an_adopted_profile_is_recorded_as_such() {
    let preset: EquipmentPreset = serde_json::from_str(SHED_KETTLE_JSON).expect("deserialises");
    let (adopted, _) = preset.adopt();

    let mut edited = adopted.clone();
    edited.mash_efficiency = 0.80;
    let source = preset.adopt_modified();

    assert_eq!(
        source,
        ProfileSource::PresetModified {
            id: "shed-kettle-30l".into()
        },
        "a bench sheet should be able to say the profile was changed"
    );

    // The edit has to actually reach the numbers, and the preset it came from
    // must be left alone — a profile is adopted, not borrowed.
    let recipe = test_recipe();
    let before = derive(&recipe, &adopted).expect("derives");
    let after = derive(&recipe, &edited).expect("derives");
    assert!(
        after.original_gravity.points() > before.original_gravity.points(),
        "raising efficiency must raise OG"
    );
    assert_eq!(
        preset.equipment.mash_efficiency, 0.72,
        "the preset itself is unchanged"
    );
}
