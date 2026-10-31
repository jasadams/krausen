// SPDX-License-Identifier: AGPL-3.0-or-later
//! Projection from canonical native data into the pure formulation engine.
use krausen_calc::{
    Chilling, Equipment, HopUse,
    formulation::{self, FermentableInput, FermentableStage, HopInput, Inputs},
};
use krausen_types::{
    brewery::{ChillingV1, EquipmentSettingsV1, RecipeDocumentV1},
    formulation::Targets,
    validation::{Diagnostic, Readiness, Target, Validation},
};

/// Compute available targets only. Validation and effective-settings resolution
/// precede this projection; unavailable values never escape as current targets.
pub fn targets(d: &RecipeDocumentV1, e: &EquipmentSettingsV1, v: &mut Validation) -> Targets {
    let mut out = Targets::default();
    if d.hops.is_empty()
        && !v.diagnostics.iter().any(|d| {
            matches!(
                d.code.as_str(),
                "unsupported_process" | "unsupported_schema"
            )
        })
    {
        out.ibu = Some(0.0);
    }
    let grain: Option<f64> = d
        .fermentables
        .iter()
        .filter(|f| f.specification.kind.as_deref() == Some("grain") && f.addition.stage == "mash")
        .map(|f| f.amount_kg)
        .sum();
    if v.target_available(Target::Water) {
        if let (Some(batch), Some(boil), Some(loss), Some(rate), Some(dead), Some(grain)) = (
            d.batch_size_l,
            d.boil_minutes,
            e.transfer_loss_l,
            e.boil_off_l_per_hour,
            e.pre_boil_deadspace_l,
            grain,
        ) {
            let absorption = if grain == 0.0 {
                Some(0.0)
            } else {
                e.grain_absorption_l_per_kg.map(|a| a * grain)
            };
            if let Some(absorption) = absorption {
                let post = batch + loss;
                let pre = post + rate * boil / 60.0;
                let total = pre + absorption + dead;
                let mash = if grain == 0.0 {
                    Some(0.0)
                } else {
                    match d.process.mash_water_method.as_deref() {
                        Some("manual") => d.process.mash_water_l,
                        Some("profile_thickness") => e.mash_thickness_l_per_kg.map(|v| v * grain),
                        _ => None,
                    }
                };
                let top_up = d.process.kettle_top_up_l.unwrap_or(0.0);
                let sparge = mash.map(|m| {
                    if grain == 0.0 {
                        0.0
                    } else {
                        total - m - top_up
                    }
                });
                let kettle = if grain == 0.0 { total } else { top_up };
                if [
                    Some(post),
                    Some(pre),
                    Some(total),
                    Some(absorption),
                    mash,
                    sparge,
                    Some(kettle),
                ]
                .into_iter()
                .all(|v| v.is_some_and(f64::is_finite))
                {
                    out.post_boil_l = Some(post);
                    out.pre_boil_l = Some(pre);
                    out.total_water_l = Some(total);
                    out.grain_absorption_l = Some(absorption);
                    out.mash_water_l = mash;
                    out.sparge_water_l = sparge;
                    out.kettle_water_l = Some(kettle);
                } else {
                    v.state = Readiness::Invalid;
                    v.diagnostics.push(Diagnostic {
                        code: "nonfinite_result".into(),
                        path: "water".into(),
                        severity: Readiness::Invalid,
                        affected_targets: vec![
                            Target::Water,
                            Target::OriginalGravity,
                            Target::FinalGravity,
                            Target::Alcohol,
                            Target::Colour,
                            Target::Bitterness,
                        ],
                    });
                    out.ibu = None;
                }
            }
        }
    }
    if v.target_available(Target::Colour) {
        let ingredients: Option<Vec<_>> = d
            .fermentables
            .iter()
            .map(|f| Some((f.amount_kg?, f.specification.colour_ebc?)))
            .collect();
        if let (Some(batch), Some(loss), Some(ingredients)) =
            (d.batch_size_l, e.transfer_loss_l, ingredients)
        {
            match formulation::colour_ebc(batch + loss, &ingredients) {
                Ok(colour) => out.colour_ebc = Some(colour),
                Err(_) => {
                    v.state = Readiness::Invalid;
                    v.diagnostics.push(Diagnostic {
                        code: "nonfinite_colour".into(),
                        path: "colour".into(),
                        severity: Readiness::Invalid,
                        affected_targets: vec![Target::Colour],
                    });
                }
            }
        }
    }
    // A missing extract input makes gravity unavailable, but cannot erase a
    // separately calculated water target.
    if !v.target_available(Target::OriginalGravity) {
        return out;
    }
    let projected = (|| -> Option<Inputs> {
        let grain = grain?;
        let fermentables = d
            .fermentables
            .iter()
            .map(|f| {
                Some(FermentableInput {
                    kg: f.amount_kg?,
                    yield_fraction: f.specification.yield_fraction?,
                    colour_ebc: if v.target_available(Target::Colour) {
                        f.specification.colour_ebc.unwrap_or(0.0)
                    } else {
                        0.0
                    },
                    stage: match (f.specification.kind.as_deref(), f.addition.stage.as_str()) {
                        (Some("grain"), "mash") => FermentableStage::Mash,
                        (Some("extract" | "sugar"), "boil") => FermentableStage::Boil {
                            remaining_minutes: f.addition.time_minutes?,
                        },
                        _ => return None,
                    },
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let hops = if v.target_available(Target::Bitterness) {
            d.hops
                .iter()
                .map(|h| {
                    Some(HopInput {
                        grams: h.amount_g?,
                        alpha_fraction: h.specification.alpha_fraction.unwrap_or(0.0),
                        stage: match h.stage.as_str() {
                            "boil" => HopUse::Boil,
                            "whirlpool" => HopUse::Whirlpool,
                            "dry_hop" => HopUse::DryHop,
                            _ => return None,
                        },
                        remaining_minutes: h.contact_minutes.unwrap_or(0.0),
                    })
                })
                .collect::<Option<Vec<_>>>()?
        } else {
            vec![]
        };
        let chilling = match &e.chilling {
            _ if !v.target_available(Target::Bitterness) => Chilling::None,
            Some(ChillingV1::Natural) => Chilling::None,
            Some(ChillingV1::Forced {
                rate_per_minute,
                plateau_c,
            }) => Chilling::Forced {
                rate_per_min: (*rate_per_minute)?,
                plateau_c: (*plateau_c)?,
            },
            Some(ChillingV1::Counterflow { litres_per_minute }) => Chilling::Counterflow {
                litres_per_minute: (*litres_per_minute)?,
            },
            None if hops.is_empty() => Chilling::None,
            None => return None,
        };
        let mash = if grain == 0.0 || !v.target_available(Target::Water) {
            0.0
        } else {
            match d.process.mash_water_method.as_deref() {
                Some("manual") => d.process.mash_water_l?,
                Some("profile_thickness") => e.mash_thickness_l_per_kg? * grain,
                _ => return None,
            }
        };
        Some(Inputs {
            batch_l: d.batch_size_l?,
            boil_minutes: d.boil_minutes?,
            mash_water_l: mash,
            kettle_top_up_l: if v.target_available(Target::Water) {
                d.process.kettle_top_up_l.unwrap_or(0.0)
            } else {
                0.0
            },
            attenuation: if v.target_available(Target::FinalGravity) {
                d.process.apparent_attenuation_fraction
            } else {
                None
            },
            fermentables,
            hops,
            whirlpool_minutes: if v.target_available(Target::Bitterness) {
                d.process.whirlpool_minutes.unwrap_or(0.0)
            } else {
                0.0
            },
            whirlpool_temperature_c: if v.target_available(Target::Bitterness) {
                d.process.whirlpool_temperature_c
            } else {
                None
            },
            equipment: Equipment {
                boil_off_rate_l_per_hour: e.boil_off_l_per_hour?,
                grain_absorption_l_per_kg: if grain == 0.0 || !v.target_available(Target::Water) {
                    0.0
                } else {
                    e.grain_absorption_l_per_kg?
                },
                kettle_deadspace_l: if v.target_available(Target::Water) {
                    e.pre_boil_deadspace_l?
                } else {
                    0.0
                },
                losses_l: e.transfer_loss_l?,
                mash_efficiency: if grain == 0.0 {
                    1.0
                } else {
                    e.efficiency_override_fraction
                        .or(e.mash_efficiency_fraction)?
                },
                kettle_diameter_cm: if v.target_available(Target::Bitterness) {
                    e.kettle_diameter_cm.unwrap_or(1.0)
                } else {
                    1.0
                },
                kettle_opening_cm: if v.target_available(Target::Bitterness) {
                    e.kettle_opening_cm.unwrap_or(1.0)
                } else {
                    1.0
                },
                boil_temp_c: if v.target_available(Target::Bitterness) {
                    e.boil_temperature_c.unwrap_or(100.0)
                } else {
                    100.0
                },
                chilling,
            },
        })
    })();
    if let Some(input) = projected {
        match formulation::calculate(&input) {
            Ok(r) => {
                out.pre_boil_sg = Some(r.pre_boil_sg);
                out.original_sg = Some(r.original_sg);
                if v.target_available(Target::FinalGravity) {
                    out.final_sg = r.final_sg;
                }
                if v.target_available(Target::Alcohol) {
                    out.abv_percent = r.abv_percent;
                }
                if v.target_available(Target::Colour) {
                    out.colour_ebc = Some(r.colour_ebc);
                }
                if v.target_available(Target::Bitterness) {
                    out.ibu = Some(r.ibu);
                }
            }
            Err(_) => {
                v.state = Readiness::Invalid;
                v.diagnostics.push(Diagnostic {
                    code: "calculation_domain".into(),
                    path: "calculation".into(),
                    severity: Readiness::Invalid,
                    affected_targets: vec![
                        Target::OriginalGravity,
                        Target::FinalGravity,
                        Target::Alcohol,
                        Target::Bitterness,
                    ],
                });
                if !d.hops.is_empty() {
                    out.ibu = None;
                }
            }
        }
    }
    out
}
