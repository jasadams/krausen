// SPDX-License-Identifier: AGPL-3.0-or-later

//! Equipment profile — what the brewery contributes, independent of the beer.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// How the wort is chilled once the stand is over.
///
/// This matters because isomerisation does not stop at flameout — it stops
/// when the wort falls out of the reaction band, around 44 °C. How quickly
/// that happens is a property of the brewery, not the beer.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Chilling {
    /// No active chilling — the wort is left to cool in the kettle.
    None,

    /// Exponential decay toward a plateau, at `rate_per_min`.
    ///
    /// Both an immersion chiller and an ice bath have this shape; they differ
    /// in how fast they pull heat out and how cold they can get. Use
    /// [`Chilling::immersion`] or [`Chilling::ice_bath`] for the published
    /// defaults, or set the two numbers directly once you have measured your
    /// own — which is exactly what Phase 3 should fit from logged brews.
    Forced { rate_per_min: f64, plateau_c: f64 },

    /// Wort is chilled as it leaves the kettle, so what remains behind stays
    /// hot. The transfer rate sets how long that lasts.
    Counterflow { litres_per_minute: f64 },
}

impl Chilling {
    /// Immersion chiller defaults from the mIBU reference implementation.
    ///
    /// The rate falls with volume — a bigger kettle takes longer:
    /// `0.6075 · e^(-0.0704 · volume)`.
    ///
    /// Note on the plateau: the reference implementation's own constants have
    /// the immersion chiller settling at 20 °C and the ice bath at 40.85 °C,
    /// with the immersion decaying *faster*. Both are the wrong way round
    /// against its published documentation (immersion 41 °C, ice bath 20 °C)
    /// and against physics — tap water cannot chill below itself, ice can.
    /// The names look transposed at the source, so the assignment here follows
    /// the documentation. In practice the plateau barely matters: isomerisation
    /// has already ceased by ~44 °C, so the descent through the 80-100 °C band
    /// is what moves the number.
    pub fn immersion(post_boil_l: f64) -> Self {
        Self::Forced {
            rate_per_min: 0.4071 * (-0.0754 * post_boil_l).exp(),
            plateau_c: 41.0,
        }
    }

    /// Ice bath defaults from the mIBU reference implementation:
    /// `0.6075 · e^(-0.0704 · volume)`, settling at 20 °C. See the note on
    /// [`Chilling::immersion`] about which constant belongs to which method.
    pub fn ice_bath(post_boil_l: f64) -> Self {
        Self::Forced {
            rate_per_min: 0.6075 * (-0.0704 * post_boil_l).exp(),
            plateau_c: 20.0,
        }
    }

    /// Counterflow default transfer rate, in litres per minute.
    pub const fn counterflow() -> Self {
        Self::Counterflow {
            litres_per_minute: 2.082,
        }
    }

    /// Fill in any volume-dependent defaults. Explicit settings pass through.
    pub(crate) fn resolved(self, post_boil_l: f64) -> Self {
        match self {
            // A zero transfer rate would mean the wort never leaves; treat it
            // as "not configured" and use the published default.
            Self::Counterflow { litres_per_minute } if litres_per_minute <= 0.0 => {
                Self::counterflow()
            }
            Self::Forced { rate_per_min, .. } if rate_per_min <= 0.0 => Self::immersion(post_boil_l),
            other => other,
        }
    }
}

/// The calibrated constants for one brewery.
///
/// Every field here is recoverable by regression from a handful of logged
/// brews, which is the point: §3.1 says the largest simplification available
/// to this product is deleting the form that asks a brewer to guess them.
/// Phase 3 changes the *source* of these numbers, not this shape.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Equipment {
    /// Litres boiled off per hour.
    ///
    /// Thermal shrinkage is currently folded in here. §7 notes that separating
    /// the two would be more correct and moves pre-boil targets by roughly
    /// 0.3 L.
    pub boil_off_rate_l_per_hour: f64,

    /// Litres of wort retained per kilogram of grain.
    ///
    /// Deadspace is currently subsumed into this coefficient — 0.8 L/kg
    /// reproduces the reference sheet exactly. §7 notes that splitting them is
    /// more honest but needs data to fit against, which is why
    /// [`Self::kettle_deadspace_l`] exists and defaults to zero.
    pub grain_absorption_l_per_kg: f64,

    /// Wort left behind in the kettle. Zero while absorption absorbs it.
    pub kettle_deadspace_l: f64,

    /// Wort lost between the kettle and the fermenter — trub, transfer, and
    /// whatever else does not make it into the batch.
    pub losses_l: f64,

    /// Efficiency as a fraction, so 78% is `0.78`.
    ///
    /// One number, clearly defined: the fraction of the grist's theoretical
    /// extract that ends up as sugar in the kettle. §3.1 — not mash vs
    /// brewhouse vs conversion efficiency, which mostly exist to confuse
    /// people about which one moved.
    pub mash_efficiency: f64,

    /// Kettle diameter in centimetres.
    ///
    /// Only the post-flameout model uses this: a wide kettle sheds heat
    /// faster, so its wort spends less time isomerising after flameout.
    pub kettle_diameter_cm: f64,

    /// Diameter of the kettle's opening in centimetres.
    ///
    /// Equal to the kettle diameter when uncovered. A lid cuts heat loss
    /// sharply, which keeps the wort hot and raises late-hop bitterness.
    pub kettle_opening_cm: f64,

    /// Boiling point at this brewery's altitude, in °C.
    pub boil_temp_c: f64,

    /// How the wort is chilled once the stand is over.
    pub chilling: Chilling,
}

impl Default for Equipment {
    /// A typical homebrew setup, matching the mIBU reference defaults.
    ///
    /// §3.5: brew one has no history, so the app uses defaults and says
    /// visibly that they are defaults. These are that starting point, not a
    /// claim about anyone's kettle.
    fn default() -> Self {
        Self {
            boil_off_rate_l_per_hour: 3.0,
            grain_absorption_l_per_kg: 0.8,
            kettle_deadspace_l: 0.0,
            losses_l: 2.0,
            mash_efficiency: 0.75,
            kettle_diameter_cm: 36.83,
            kettle_opening_cm: 36.83,
            boil_temp_c: 100.0,
            chilling: Chilling::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immersion_and_ice_bath_differ_in_the_expected_direction() {
        let (Chilling::Forced {
            rate_per_min: immersion_rate,
            plateau_c: immersion_plateau,
        },
        Chilling::Forced {
            rate_per_min: ice_rate,
            plateau_c: ice_plateau,
        }) = (Chilling::immersion(22.5), Chilling::ice_bath(22.5))
        else {
            panic!("both are forced-decay methods");
        };

        assert!(ice_rate > immersion_rate, "ice pulls heat out faster");
        assert!(ice_plateau < immersion_plateau, "ice reaches a lower floor");
    }

    #[test]
    fn chilling_rates_fall_as_volume_rises() {
        let (Chilling::Forced { rate_per_min: small, .. },
             Chilling::Forced { rate_per_min: large, .. }) =
            (Chilling::ice_bath(10.0), Chilling::ice_bath(40.0))
        else {
            panic!("forced");
        };
        assert!(large < small, "a bigger volume takes longer to chill");
    }

    #[test]
    fn unset_rates_fall_back_to_published_defaults() {
        let resolved = Chilling::Forced {
            rate_per_min: 0.0,
            plateau_c: 41.0,
        }
        .resolved(22.5);
        assert!(matches!(resolved, Chilling::Forced { rate_per_min, .. } if rate_per_min > 0.0));

        let resolved = Chilling::Counterflow {
            litres_per_minute: 0.0,
        }
        .resolved(22.5);
        assert_eq!(resolved, Chilling::counterflow());
    }
}
