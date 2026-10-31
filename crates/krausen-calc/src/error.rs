// SPDX-License-Identifier: AGPL-3.0-or-later

//! Errors from [`crate::derive`].

use std::fmt;

/// Why a recipe could not be derived.
///
/// Each variant carries the offending value, because "efficiency out of range"
/// without the number is a worse message than it needs to be.
#[derive(Debug, Clone, PartialEq)]
pub enum CalcError {
    /// No fermentables — there is nothing to derive a gravity from.
    EmptyGrist,
    NonPositiveBatchSize(f64),
    NonPositiveGrainWeight(f64),
    /// Efficiency must be a fraction in `(0, 1]`.
    EfficiencyOutOfRange(f64),
    /// Apparent attenuation must be a fraction in `[0, 1]`.
    AttenuationOutOfRange(f64),
    NegativeBoilOffRate(f64),
    /// The boil would remove at least as much wort as goes into it.
    BoilOffExceedsPreBoil {
        pre_boil_l: f64,
        boil_off_l: f64,
    },
    /// Mash water exceeds the total water the recipe calls for, which would
    /// make the sparge volume negative.
    MashWaterExceedsTotal {
        mash_water_l: f64,
        total_water_l: f64,
    },
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyGrist => write!(f, "recipe has no fermentables"),
            Self::NonPositiveBatchSize(v) => {
                write!(f, "batch size must be greater than zero, got {v} L")
            }
            Self::NonPositiveGrainWeight(v) => {
                write!(f, "total grain weight must be greater than zero, got {v} kg")
            }
            Self::EfficiencyOutOfRange(v) => write!(
                f,
                "efficiency must be a fraction between 0 and 1, got {v} \
                 (78% is 0.78, not 78)"
            ),
            Self::AttenuationOutOfRange(v) => write!(
                f,
                "apparent attenuation must be a fraction between 0 and 1, got {v} \
                 (82% is 0.82, not 82)"
            ),
            Self::NegativeBoilOffRate(v) => {
                write!(f, "boil-off rate cannot be negative, got {v} L/hr")
            }
            Self::BoilOffExceedsPreBoil {
                pre_boil_l,
                boil_off_l,
            } => write!(
                f,
                "boil would remove {boil_off_l} L from a {pre_boil_l} L pre-boil volume — \
                 check the boil time and boil-off rate"
            ),
            Self::MashWaterExceedsTotal {
                mash_water_l,
                total_water_l,
            } => write!(
                f,
                "mash water ({mash_water_l} L) exceeds total water ({total_water_l} L), \
                 which would make the sparge negative"
            ),
        }
    }
}

impl std::error::Error for CalcError {}
