// SPDX-License-Identifier: AGPL-3.0-or-later

//! Equipment presets — a starting profile for a known brewing setup.
//!
//! A preset exists so a brewer does not have to answer questions the software
//! could answer for them (§3.1). It is a *starting point*, never an assertion:
//! the numbers it offers are exactly the ones Phase 3 is meant to fit from
//! logged brews and replace.
//!
//! # Why data and not a trait
//!
//! §4.6 requires a brew to embed a full copy of the equipment profile it was
//! brewed with, so that editing a profile later cannot retroactively change
//! what a past brew claims to have been. A trait object cannot be snapshotted;
//! a value can. Presets are therefore plain serialisable data, and plugging in
//! new equipment means supplying more data — from a built-in constant, a
//! config file, or a database row — not implementing anything.
//!
//! # What a preset does and does not know
//!
//! A vessel knows its own dimensions. It does not know your efficiency, which
//! depends on your mill gap, your water and your process. [`EquipmentPreset`]
//! keeps these apart via [`EquipmentPreset::measured`], because §3.3 turns on
//! the difference: a measured value is not evidence of drift when a brew
//! disagrees with it, and an estimate is.

use std::borrow::Cow;

use crate::equipment::Equipment;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A tunable constant in an [`Equipment`] profile.
///
/// This is the vocabulary the rest of the application uses to talk about
/// individual constants — which one a preset actually knows, which one a
/// regression has fitted, and which one has drifted. §3.4's "your system has
/// changed" alert has to name a field, and this is that name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum EquipmentField {
    BoilOffRate,
    GrainAbsorption,
    KettleDeadspace,
    Losses,
    MashEfficiency,
    KettleGeometry,
    BoilTemperature,
    Chilling,
}

impl EquipmentField {
    /// Every field, for iteration.
    pub const ALL: [Self; 8] = [
        Self::BoilOffRate,
        Self::GrainAbsorption,
        Self::KettleDeadspace,
        Self::Losses,
        Self::MashEfficiency,
        Self::KettleGeometry,
        Self::BoilTemperature,
        Self::Chilling,
    ];

    /// Short label for display.
    pub const fn label(self) -> &'static str {
        match self {
            Self::BoilOffRate => "boil-off rate",
            Self::GrainAbsorption => "grain absorption",
            Self::KettleDeadspace => "kettle deadspace",
            Self::Losses => "losses to the fermenter",
            Self::MashEfficiency => "efficiency",
            Self::KettleGeometry => "kettle dimensions",
            Self::BoilTemperature => "boiling point",
            Self::Chilling => "chilling method",
        }
    }

    /// Unit for display, where the field has one.
    pub const fn unit(self) -> Option<&'static str> {
        match self {
            Self::BoilOffRate => Some("L/hr"),
            Self::GrainAbsorption => Some("L/kg"),
            Self::KettleDeadspace | Self::Losses => Some("L"),
            Self::MashEfficiency => Some("%"),
            Self::KettleGeometry => Some("cm"),
            Self::BoilTemperature => Some("°C"),
            Self::Chilling => None,
        }
    }

    /// Whether a regression over logged brews can recover this field.
    ///
    /// The recoverable ones are why §3.1 can delete most of the equipment
    /// form. The rest are either physical measurements or a choice of process,
    /// and no amount of brew history will infer them.
    pub const fn is_fittable(self) -> bool {
        match self {
            Self::BoilOffRate
            | Self::GrainAbsorption
            | Self::KettleDeadspace
            | Self::Losses
            | Self::MashEfficiency => true,
            Self::KettleGeometry | Self::BoilTemperature | Self::Chilling => false,
        }
    }
}

/// Where an equipment profile came from.
///
/// Recorded alongside the profile in a brew snapshot so that a bench sheet
/// from two years ago can still say what it was based on.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ProfileSource {
    /// Taken from a preset, unchanged.
    Preset { id: String },
    /// Started from a preset and then changed.
    PresetModified { id: String },
    /// Entered by the brewer from scratch.
    Custom,
    /// Fitted from logged brews (Phase 3), over `sample_size` of them.
    Fitted { sample_size: usize },
}

/// A named starting profile for a particular brewing setup.
///
/// Built-ins are declared as constants and cost nothing; a preset loaded from
/// a file or a database row is the same type with owned strings. Adding
/// equipment is adding one of these — see [`PresetCatalogue`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct EquipmentPreset {
    /// Stable identifier, e.g. `"grainfather-g30"`.
    ///
    /// Written into brew snapshots, so it must not change once published.
    pub id: Cow<'static, str>,

    /// Display name, e.g. `"Grainfather G30"`.
    pub name: Cow<'static, str>,

    /// Who makes it, where that is meaningful.
    pub maker: Option<Cow<'static, str>>,

    /// What a brewer should know before choosing it — in particular, which of
    /// its numbers are real and which are placeholders.
    pub notes: Cow<'static, str>,

    /// Batch size the preset is sized around, in litres.
    ///
    /// Presets are not scale-free: a boil-off rate belongs to a vessel at a
    /// working volume.
    pub typical_batch_l: f64,

    /// The profile this preset starts a brewer at.
    pub equipment: Equipment,

    /// Fields this preset states as fact about the hardware.
    ///
    /// Everything not listed is a starting estimate. The distinction is not
    /// cosmetic: §3.3 says the deviation is the information, and a brew
    /// disagreeing with a *measured* value means something different from one
    /// disagreeing with a guess.
    pub measured: Vec<EquipmentField>,
}

impl EquipmentPreset {
    /// Whether this preset claims to know `field` as a fact.
    pub fn is_measured(&self, field: EquipmentField) -> bool {
        self.measured.contains(&field)
    }

    /// Fields this preset is only estimating, which Phase 3 should fit.
    pub fn estimated(&self) -> Vec<EquipmentField> {
        EquipmentField::ALL
            .into_iter()
            .filter(|f| !self.is_measured(*f))
            .collect()
    }

    /// Take this preset's profile as a starting point.
    ///
    /// Returns the profile and the provenance to record with it. Callers that
    /// then change a value should switch the source to
    /// [`ProfileSource::PresetModified`] — see [`Self::adopt_modified`].
    pub fn adopt(&self) -> (Equipment, ProfileSource) {
        (
            self.equipment.clone(),
            ProfileSource::Preset {
                id: self.id.to_string(),
            },
        )
    }

    /// Provenance for a profile that started here and was then edited.
    pub fn adopt_modified(&self) -> ProfileSource {
        ProfileSource::PresetModified {
            id: self.id.to_string(),
        }
    }
}

/// The presets available to choose from.
///
/// Deliberately a value rather than a global: §4.1 keeps this crate free of
/// state, so the application owns its catalogue and decides what goes in it.
/// Build one from [`Self::with_builtins`] and add whatever else you have —
/// user-defined profiles, rows from a database, a shipped TOML file.
#[derive(Debug, Clone, Default)]
pub struct PresetCatalogue {
    presets: Vec<EquipmentPreset>,
}

impl PresetCatalogue {
    /// An empty catalogue.
    pub fn new() -> Self {
        Self::default()
    }

    /// A catalogue seeded with the presets built into Krausen.
    pub fn with_builtins() -> Self {
        Self {
            presets: builtin_presets(),
        }
    }

    /// Add a preset. A later preset with the same id replaces an earlier one,
    /// so an application can override a built-in without forking it.
    pub fn insert(&mut self, preset: EquipmentPreset) -> &mut Self {
        if let Some(existing) = self.presets.iter_mut().find(|p| p.id == preset.id) {
            *existing = preset;
        } else {
            self.presets.push(preset);
        }
        self
    }

    /// Add several presets.
    pub fn extend<I: IntoIterator<Item = EquipmentPreset>>(&mut self, presets: I) -> &mut Self {
        for preset in presets {
            self.insert(preset);
        }
        self
    }

    /// Look a preset up by id.
    pub fn get(&self, id: &str) -> Option<&EquipmentPreset> {
        self.presets.iter().find(|p| p.id == id)
    }

    /// Every preset, in insertion order.
    pub fn all(&self) -> &[EquipmentPreset] {
        &self.presets
    }

    pub fn len(&self) -> usize {
        self.presets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.presets.is_empty()
    }
}

/// Presets shipped with Krausen.
///
/// Intentionally thin. A preset naming real equipment is a claim about that
/// equipment, and inventing plausible-looking dimensions for a vessel nobody
/// has measured would put fiction where §3.5 wants a visible "I don't know
/// yet". Adding one is adding a constant below; the interface is the part
/// that needs to be right now.
pub fn builtin_presets() -> Vec<EquipmentPreset> {
    vec![EquipmentPreset {
        id: Cow::Borrowed("generic-homebrew-23l"),
        name: Cow::Borrowed("Generic homebrew setup"),
        maker: None,
        notes: Cow::Borrowed(
            "A neutral starting point for a typical 23 L batch, with no claim \
             about any particular vessel. Every number here is an estimate and \
             should be replaced by fitted values once there are brews to fit.",
        ),
        typical_batch_l: 23.0,
        equipment: Equipment::default(),
        // Nothing is measured: this preset describes no actual vessel.
        measured: Vec::new(),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equipment::Chilling;

    fn preset(id: &'static str, measured: Vec<EquipmentField>) -> EquipmentPreset {
        EquipmentPreset {
            id: Cow::Borrowed(id),
            name: Cow::Borrowed("Test rig"),
            maker: None,
            notes: Cow::Borrowed(""),
            typical_batch_l: 23.0,
            equipment: Equipment::default(),
            measured,
        }
    }

    #[test]
    fn builtins_are_available_and_uniquely_identified() {
        let catalogue = PresetCatalogue::with_builtins();
        assert!(!catalogue.is_empty());

        let mut ids: Vec<&str> = catalogue.all().iter().map(|p| p.id.as_ref()).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "preset ids must be unique");
    }

    #[test]
    fn a_preset_can_be_plugged_in_at_runtime() {
        // The point of the interface: new equipment is new data, not new code.
        let mut catalogue = PresetCatalogue::new();
        catalogue.insert(EquipmentPreset {
            id: Cow::Owned("my-shed-kettle".to_string()),
            name: Cow::Owned("The one in the shed".to_string()),
            maker: None,
            notes: Cow::Owned(String::new()),
            typical_batch_l: 19.0,
            equipment: Equipment {
                boil_off_rate_l_per_hour: 2.4,
                ..Equipment::default()
            },
            measured: vec![EquipmentField::KettleGeometry],
        });

        let found = catalogue.get("my-shed-kettle").expect("registered");
        assert_eq!(found.equipment.boil_off_rate_l_per_hour, 2.4);
        assert!(found.is_measured(EquipmentField::KettleGeometry));
    }

    #[test]
    fn a_later_preset_overrides_an_earlier_one_with_the_same_id() {
        // Lets an application correct a built-in without forking the crate.
        let mut catalogue = PresetCatalogue::with_builtins();
        let before = catalogue.len();

        let mut replacement = preset("generic-homebrew-23l", vec![]);
        replacement.equipment.boil_off_rate_l_per_hour = 99.0;
        catalogue.insert(replacement);

        assert_eq!(catalogue.len(), before, "replaced, not appended");
        assert_eq!(
            catalogue
                .get("generic-homebrew-23l")
                .expect("present")
                .equipment
                .boil_off_rate_l_per_hour,
            99.0
        );
    }

    #[test]
    fn adopting_a_preset_records_where_the_profile_came_from() {
        // §4.6: a brew snapshot has to be able to say what it was based on.
        let p = preset("rig-1", vec![]);
        let (equipment, source) = p.adopt();
        assert_eq!(equipment, p.equipment);
        assert_eq!(
            source,
            ProfileSource::Preset {
                id: "rig-1".to_string()
            }
        );
        assert_eq!(
            p.adopt_modified(),
            ProfileSource::PresetModified {
                id: "rig-1".to_string()
            }
        );
    }

    #[test]
    fn measured_and_estimated_fields_partition_the_profile() {
        let p = preset(
            "rig-1",
            vec![EquipmentField::KettleGeometry, EquipmentField::Losses],
        );
        assert!(p.is_measured(EquipmentField::KettleGeometry));
        assert!(!p.is_measured(EquipmentField::MashEfficiency));

        let estimated = p.estimated();
        assert!(estimated.contains(&EquipmentField::MashEfficiency));
        assert!(!estimated.contains(&EquipmentField::KettleGeometry));
        assert_eq!(
            estimated.len() + p.measured.len(),
            EquipmentField::ALL.len(),
            "every field is either measured or estimated"
        );
    }

    #[test]
    fn the_fittable_fields_are_the_ones_regression_can_recover() {
        // §3.1: these five are why the equipment form can mostly be deleted.
        for field in [
            EquipmentField::BoilOffRate,
            EquipmentField::GrainAbsorption,
            EquipmentField::KettleDeadspace,
            EquipmentField::Losses,
            EquipmentField::MashEfficiency,
        ] {
            assert!(field.is_fittable(), "{} should be fittable", field.label());
        }

        // No amount of brew history tells you how wide your kettle is, or
        // which chiller you own.
        for field in [
            EquipmentField::KettleGeometry,
            EquipmentField::BoilTemperature,
            EquipmentField::Chilling,
        ] {
            assert!(!field.is_fittable(), "{} is not fittable", field.label());
        }
    }

    #[test]
    fn every_field_has_a_label() {
        for field in EquipmentField::ALL {
            assert!(!field.label().is_empty());
        }
        assert_eq!(EquipmentField::Chilling.unit(), None, "not a quantity");
        assert_eq!(EquipmentField::BoilOffRate.unit(), Some("L/hr"));
    }

    #[test]
    fn a_preset_profile_derives_a_recipe() {
        // The profile a preset hands over has to be usable as-is.
        use crate::{derive, Fermentable, Recipe, Yeast};

        let (equipment, _) = PresetCatalogue::with_builtins()
            .get("generic-homebrew-23l")
            .expect("built in")
            .adopt();

        let recipe = Recipe {
            name: "smoke test".into(),
            batch_size_l: 23.0,
            boil_minutes: 60,
            mash_water_l: 15.0,
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
        };

        let derived = derive(&recipe, &equipment).expect("a preset must derive");
        assert!(derived.original_gravity.points() > 0.0);
    }

    #[test]
    fn chilling_choice_survives_a_round_trip_through_a_preset() {
        let mut p = preset("rig-1", vec![EquipmentField::Chilling]);
        p.equipment.chilling = Chilling::counterflow();
        let (equipment, _) = p.adopt();
        assert_eq!(equipment.chilling, Chilling::counterflow());
    }
}
