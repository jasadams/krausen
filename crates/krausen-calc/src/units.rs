// SPDX-License-Identifier: AGPL-3.0-or-later

//! Unit conversions and the gravity representation.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// PPG -> points·L/kg. See KRAUSEN.md §4.3.
///
/// PPG is an imperial unit (points per pound per US gallon) leaking into a
/// metric recipe, which is why Munich I reads 37.826 rather than a round
/// number. Converting once at the edge keeps the whole extract chain metric.
pub const PPG_TO_POINTS_L_PER_KG: f64 = 8.345404;

/// Pounds per kilogram — needed only by Morey, which is defined in imperial.
pub const LB_PER_KG: f64 = 2.2046226;

/// US gallons per litre — as above.
pub const GAL_PER_L: f64 = 0.26417205;

/// A specific gravity, stored as points.
///
/// "Points" are `(SG - 1) * 1000`, so 1.0450 is 45.0 points. The whole extract
/// chain works in points — `points / volume` needs no unit conversion — and
/// this newtype exists so a bare `1.045` can never be added to a bare `45.0`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Gravity(f64);

impl Gravity {
    /// Build from points, e.g. `Gravity::from_points(45.0)` for 1.0450.
    pub const fn from_points(points: f64) -> Self {
        Self(points)
    }

    /// Build from specific gravity, e.g. `Gravity::from_sg(1.045)`.
    pub fn from_sg(sg: f64) -> Self {
        Self((sg - 1.0) * 1000.0)
    }

    /// Gravity points — `(SG - 1) * 1000`.
    pub const fn points(self) -> f64 {
        self.0
    }

    /// Specific gravity, for display and interchange.
    pub fn sg(self) -> f64 {
        1.0 + self.0 / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gravity_round_trips_between_points_and_sg() {
        let g = Gravity::from_points(45.0);
        assert!((g.sg() - 1.045).abs() < 1e-12);
        assert!((Gravity::from_sg(1.045).points() - 45.0).abs() < 1e-9);
    }

    #[test]
    fn water_is_zero_points() {
        assert_eq!(Gravity::from_sg(1.0).points(), 0.0);
        assert_eq!(Gravity::from_points(0.0).sg(), 1.0);
    }

    #[test]
    fn documented_ppg_conversions_hold() {
        // §4.3 worked examples.
        assert!((37.000 * PPG_TO_POINTS_L_PER_KG - 308.78).abs() < 0.01);
        assert!((37.826 * PPG_TO_POINTS_L_PER_KG - 315.67).abs() < 0.01);
    }
}
