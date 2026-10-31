// SPDX-License-Identifier: AGPL-3.0-or-later

//! What the engine produces.

use crate::units::Gravity;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Every volume in the water chain, in the order §4.2 computes them.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Volumes {
    /// Into the fermenter — the recipe's batch size, restated for the sheet.
    pub batch_l: f64,
    /// In the kettle at flameout: batch + losses.
    pub post_boil_l: f64,
    /// In the kettle at the start of the boil.
    pub pre_boil_l: f64,
    /// Retained by the grain.
    pub grain_absorption_l: f64,
    /// Left in the kettle.
    pub deadspace_l: f64,
    /// Everything that goes in: pre-boil + absorption + deadspace.
    pub total_water_l: f64,
    /// Mash water, as specified.
    pub mash_water_l: f64,
    /// The remainder: total - mash.
    pub sparge_water_l: f64,
    /// Boiled off over the whole boil.
    pub boil_off_l: f64,
}

/// Per-fermentable extract, kept so a bench sheet can show its working.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FermentablePoints {
    pub name: String,
    pub kg: f64,
    /// The metric extract constant — PPG converted once. See §4.3.
    pub points_per_kg_per_l: f64,
    /// Theoretical contribution before efficiency: kg × points_per_kg_per_l.
    pub potential_points_l: f64,
    /// Share of the grist's total potential, as a fraction.
    pub fraction_of_grist: f64,
}

/// One hop addition's bitterness contribution.
///
/// The boil and post-flameout halves are reported separately because they are
/// the interesting part: the gap between them is what plain Tinseth throws
/// away, and a bench sheet should be able to show its working.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct IbuContribution {
    pub name: String,
    pub grams: f64,
    pub minutes: u32,
    /// Tinseth's time factor for the boil itself.
    pub boil_utilisation: f64,
    /// Additional time factor earned after flameout, while the wort cools
    /// through the isomerisation band.
    pub post_flameout_utilisation: f64,
    /// Bitterness from the boil alone — what plain Tinseth would report.
    pub boil_ibu: f64,
    /// Bitterness added after flameout.
    pub post_flameout_ibu: f64,
    /// The total for this addition.
    pub ibu: f64,
}

/// Mid-boil corrections, both of which scale with pre-boil volume.
///
/// Derived, never stored — that is precisely why shortening the boil changed
/// them between the two reference variants.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Corrections {
    /// Dry malt extract needed to raise pre-boil gravity by one point.
    pub dme_grams_per_point: f64,
    /// Water needed to drop pre-boil gravity by one point.
    pub dilution_litres_per_point: f64,
}

/// The complete derived state of a recipe on a given brewery.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Derived {
    pub volumes: Volumes,

    pub fermentable_points: Vec<FermentablePoints>,
    /// Grist potential before efficiency.
    pub potential_points_l: f64,
    /// Extract actually landing in the kettle: potential × efficiency.
    pub kettle_points_l: f64,

    /// The adjustment window — the one point where volume and gravity are
    /// both still free to move. §3.2: set volume first, gravity second.
    pub pre_boil_gravity: Gravity,
    pub original_gravity: Gravity,
    pub final_gravity: Gravity,
    pub abv_percent: f64,

    /// Mean of pre-boil and original gravity — Tinseth's bigness input.
    pub mean_boil_gravity: Gravity,
    pub ibu_contributions: Vec<IbuContribution>,
    /// Total bitterness, boil plus post-flameout.
    pub ibu_total: f64,
    /// What boil-only Tinseth would report. Kept for comparison against
    /// software that stops at flameout — not a second opinion to choose
    /// between (§3.1), just the working shown.
    pub ibu_boil_only: f64,

    pub colour_ebc: f64,
    pub colour_srm: f64,

    pub corrections: Corrections,
}
