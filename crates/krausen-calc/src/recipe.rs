// SPDX-License-Identifier: AGPL-3.0-or-later

//! Recipe inputs — what the brewer specifies.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A fermentable in the grist.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Fermentable {
    pub name: String,
    pub kg: f64,
    /// Points per pound per US gallon, as printed on the maltster's spec sheet.
    ///
    /// Retained in PPG only because that is how suppliers publish it and how
    /// BeerXML carries it. It is converted to metric once, on the way in.
    pub ppg: f64,
    /// Colour in EBC.
    pub ebc: f64,
}

/// When a hop addition goes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum HopUse {
    /// In the boil for `minutes`, contributing full Tinseth utilisation.
    Boil,
    /// Added at flameout and held hot during the whirlpool.
    ///
    /// Isomerisation continues below the boil, which is why this is a
    /// first-class case rather than a zero-minute boil addition — see §4.5.
    Whirlpool,
    /// Dry hop. No isomerisation, so no IBU contribution.
    DryHop,
}

/// A single hop addition.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Hop {
    pub name: String,
    pub grams: f64,
    /// Alpha acid as a fraction, so 3.8% is `0.038`.
    pub alpha_acid: f64,
    /// Boil time for [`HopUse::Boil`]; ignored for whirlpool and dry hop,
    /// which take their contact time from the recipe's whirlpool fields.
    pub minutes: u32,
    pub use_: HopUse,
}

/// The yeast, and the only thing the engine needs from it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Yeast {
    pub name: String,
    /// Apparent attenuation as a fraction, so 82% is `0.82`.
    pub apparent_attenuation: f64,
}

/// A recipe: everything about the beer, nothing about the brewery.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Recipe {
    pub name: String,
    /// Volume into the fermenter.
    pub batch_size_l: f64,
    pub boil_minutes: u32,
    /// Water in the mash. Sparge is the remainder of total water.
    ///
    /// An explicit input for now. KRAUSEN.md §7 leaves open whether this
    /// should instead be derived from the grain bill and a thickness, or from
    /// Grainfather's formula; the reference sheet's 15.26 L for 4.2 kg
    /// (3.633 L/kg) matches neither.
    pub mash_water_l: f64,
    pub fermentables: Vec<Fermentable>,
    pub hops: Vec<Hop>,
    pub yeast: Yeast,
    /// Whirlpool stand time. Feeds the same Tinseth integral as boil
    /// additions — see §4.5.
    pub whirlpool_minutes: u32,
    /// Whirlpool temperature in °C. Recorded for the bench sheet; the
    /// utilisation model does not yet use it.
    pub whirlpool_temp_c: Option<f64>,
}

impl Recipe {
    /// Total grain weight, which drives absorption.
    pub fn total_grain_kg(&self) -> f64 {
        self.fermentables.iter().map(|f| f.kg).sum()
    }

    /// Boil time in hours, which is what the boil-off rate is expressed in.
    pub fn boil_hours(&self) -> f64 {
        f64::from(self.boil_minutes) / 60.0
    }
}
