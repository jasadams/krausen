// SPDX-License-Identifier: AGPL-3.0-or-later
//! Projection from canonical native data into the pure formulation engine.
use krausen_calc::{
    Chilling, Equipment, HopUse,
    formulation::{self, FermentableInput, FermentableStage, HopInput, Inputs},
};
use krausen_types::{
    brewery::{CoolingMethodV1, EquipmentSettingsV1, RecipeDocumentV1},
    formulation::{AppliedDefault, Targets, Vessel, VesselWarning},
    validation::{Diagnostic, Readiness, Target, Validation},
};

/// Water displaced by one kilogram of mashed grain, for the mash tun overfill
/// warning only. A fixed, visible assumption of the equipment spec.
pub const GRAIN_DISPLACEMENT_L_PER_KG: f64 = 0.67;
/// Fixed boiling point: the spec has no boil temperature field.
const FIXED_BOIL_TEMPERATURE_C: f64 = 100.0;

/// The spec's water-chain terms with absent optional losses resolved to zero,
/// each recorded as an applied default.
struct WaterTerms {
    wort_shrinkage_fraction: f64,
    /// Boil loss converted to the 20 °C reference basis: `E × (1 - s)`.
    boil_loss_l_per_hour: f64,
    trub_and_chiller_loss_l: f64,
    /// Mash grain absorption plus sparge grain absorption.
    absorption_l_per_kg: f64,
    /// Mash tun deadspace plus mash tun loss plus sparge deadspace.
    deadspace_l: f64,
}

/// `stage_losses` includes absorption and deadspaces; without it only the boil
/// chain is resolved, which gravity needs even when water targets are unavailable.
fn water_terms(
    e: &EquipmentSettingsV1,
    grain: f64,
    stage_losses: bool,
    defaults: &mut Vec<AppliedDefault>,
) -> Option<WaterTerms> {
    let mut optional = |value: Option<f64>, path: &str, unit: &str| match value {
        Some(value) => value,
        None => {
            record_default(defaults, format!("equipment.{path}"), 0.0, unit);
            0.0
        }
    };
    let shrinkage = optional(e.wort_shrinkage_fraction, "wort_shrinkage_fraction", "fraction");
    let tun_loss = optional(e.mash_tun_loss_l, "mash_tun_loss_l", "L");
    let sparge_dead = optional(e.sparge_deadspace_l, "sparge_deadspace_l", "L");
    let sparge_abs = optional(
        e.sparge_grain_absorption_l_per_kg,
        "sparge_grain_absorption_l_per_kg",
        "L/kg",
    );
    Some(WaterTerms {
        wort_shrinkage_fraction: shrinkage,
        boil_loss_l_per_hour: e.boil_loss_l_per_hour? * (1.0 - shrinkage),
        trub_and_chiller_loss_l: e.trub_and_chiller_loss_l?,
        absorption_l_per_kg: if grain == 0.0 || !stage_losses {
            0.0
        } else {
            e.mash_grain_absorption_l_per_kg? + sparge_abs
        },
        deadspace_l: if stage_losses {
            e.mash_tun_deadspace_l? + tun_loss + sparge_dead
        } else {
            0.0
        },
    })
}

/// Overfilled vessels. Absent vessel volumes are not checked.
fn vessel_warnings(
    e: &EquipmentSettingsV1,
    grain: f64,
    mash_water_l: Option<f64>,
    sparge_water_l: Option<f64>,
    kettle_peak_l: f64,
) -> Vec<VesselWarning> {
    let mut out = Vec::new();
    let mut check = |vessel, required: Option<f64>, capacity: Option<f64>| {
        if let (Some(required_l), Some(capacity_l)) = (required, capacity) {
            if required_l.is_finite() && required_l > capacity_l {
                out.push(VesselWarning {
                    vessel,
                    required_l,
                    capacity_l,
                });
            }
        }
    };
    if grain > 0.0 {
        check(
            Vessel::MashTun,
            mash_water_l.map(|water| water + grain * GRAIN_DISPLACEMENT_L_PER_KG),
            e.mash_tun_volume_l,
        );
        check(Vessel::SpargeVessel, sparge_water_l.filter(|l| *l > 0.0), e.sparge_vessel_volume_l);
    }
    check(Vessel::Kettle, Some(kettle_peak_l), e.boil_volume_l);
    out
}

/// Compute available targets only. Validation and effective-settings resolution
/// precede this projection; unavailable values never escape as current targets.
pub fn targets(d: &RecipeDocumentV1, e: &EquipmentSettingsV1, v: &mut Validation) -> Targets {
    let mut defaults = Vec::new();
    targets_with_defaults(d, e, v, &mut defaults)
}

fn record_default(defaults: &mut Vec<AppliedDefault>, path: String, value: f64, unit: &str) {
    if !defaults.iter().any(|default| default.path == path) {
        defaults.push(AppliedDefault {
            path,
            value,
            unit: unit.into(),
        });
    }
}

fn merge_defaults(into: &mut Vec<AppliedDefault>, from: Vec<AppliedDefault>) {
    for default in from {
        if !into.iter().any(|existing| existing.path == default.path) {
            into.push(default);
        }
    }
}

/// Same projection while recording every engine fallback value actually used.
/// The caller freezes this list with the accepted calculation context.
pub fn targets_with_defaults(
    d: &RecipeDocumentV1,
    e: &EquipmentSettingsV1,
    v: &mut Validation,
    applied_defaults: &mut Vec<AppliedDefault>,
) -> Targets {
    let mut out = Targets::default();
    let mut water_defaults = Vec::new();
    let mut projection_defaults = Vec::new();
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
        if let (Some(batch), Some(boil), Some(grain)) = (d.batch_size_l, d.boil_minutes, grain) {
            if let Some(terms) = water_terms(e, grain, true, &mut water_defaults) {
                let post = batch + terms.trub_and_chiller_loss_l;
                let pre = post + terms.boil_loss_l_per_hour * boil / 60.0;
                let absorption = terms.absorption_l_per_kg * grain;
                let total = pre + absorption + terms.deadspace_l;
                let mash = if grain == 0.0 {
                    Some(0.0)
                } else {
                    match d.process.mash_water_method.as_deref() {
                        Some("manual") => d.process.mash_water_l,
                        Some("profile_thickness") => e.mash_thickness_l_per_kg.map(|v| v * grain),
                        _ => None,
                    }
                };
                let top_up = match d.process.kettle_top_up_l {
                    Some(value) => value,
                    None => {
                        record_default(
                            &mut water_defaults,
                            "process.kettle_top_up_l".into(),
                            0.0,
                            "L",
                        );
                        0.0
                    }
                };
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
                    out.vessel_warnings = vessel_warnings(
                        e,
                        grain,
                        mash,
                        sparge,
                        pre / (1.0 - terms.wort_shrinkage_fraction),
                    );
                    merge_defaults(applied_defaults, water_defaults);
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
            (d.batch_size_l, e.trub_and_chiller_loss_l, ingredients)
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
                        match f.specification.colour_ebc {
                            Some(value) => value,
                            None => {
                                record_default(
                                    &mut projection_defaults,
                                    format!("fermentables[{}].specification.colour_ebc", f.row_id),
                                    0.0,
                                    "EBC",
                                );
                                0.0
                            }
                        }
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
                        alpha_fraction: match h.specification.alpha_fraction {
                            Some(value) => value,
                            None => {
                                record_default(
                                    &mut projection_defaults,
                                    format!("hops[{}].specification.alpha_fraction", h.row_id),
                                    0.0,
                                    "fraction",
                                );
                                0.0
                            }
                        },
                        stage: match h.stage.as_str() {
                            "boil" => HopUse::Boil,
                            "whirlpool" => HopUse::Whirlpool,
                            "dry_hop" => HopUse::DryHop,
                            _ => return None,
                        },
                        remaining_minutes: match h.contact_minutes {
                            Some(value) => value,
                            None => {
                                record_default(
                                    &mut projection_defaults,
                                    format!("hops[{}].contact_minutes", h.row_id),
                                    0.0,
                                    "minutes",
                                );
                                0.0
                            }
                        },
                    })
                })
                .collect::<Option<Vec<_>>>()?
        } else {
            vec![]
        };
        let terms = water_terms(
            e,
            grain,
            v.target_available(Target::Water),
            &mut projection_defaults,
        )?;
        let chilling = if !v.target_available(Target::Bitterness) {
            Chilling::None
        } else {
            let post_boil_l = d.batch_size_l? + terms.trub_and_chiller_loss_l;
            match e.cooling_method {
                Some(CoolingMethodV1::Natural) => Chilling::None,
                Some(CoolingMethodV1::ImmersionChiller) => Chilling::immersion(post_boil_l),
                Some(CoolingMethodV1::IceBath) => Chilling::ice_bath(post_boil_l),
                Some(CoolingMethodV1::CounterflowChiller) => Chilling::Counterflow {
                    litres_per_minute: e.cooling_flow_rate_l_per_minute?,
                },
                None if hops.is_empty() => Chilling::None,
                None => return None,
            }
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
                match d.process.kettle_top_up_l {
                    Some(value) => value,
                    None => {
                        record_default(
                            &mut projection_defaults,
                            "process.kettle_top_up_l".into(),
                            0.0,
                            "L",
                        );
                        0.0
                    }
                }
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
                match d.process.whirlpool_minutes {
                    Some(value) => value,
                    None => {
                        record_default(
                            &mut projection_defaults,
                            "process.whirlpool_minutes".into(),
                            0.0,
                            "minutes",
                        );
                        0.0
                    }
                }
            } else {
                0.0
            },
            whirlpool_temperature_c: if v.target_available(Target::Bitterness) {
                d.process.whirlpool_temperature_c
            } else {
                None
            },
            equipment: {
                let kettle_diameter_cm = if v.target_available(Target::Bitterness) {
                    match e.kettle_diameter_cm {
                        Some(value) => value,
                        None => {
                            record_default(
                                &mut projection_defaults,
                                "equipment.kettle_diameter_cm".into(),
                                1.0,
                                "cm",
                            );
                            1.0
                        }
                    }
                } else {
                    1.0
                };
                Equipment {
                    boil_off_rate_l_per_hour: terms.boil_loss_l_per_hour,
                    grain_absorption_l_per_kg: terms.absorption_l_per_kg,
                    kettle_deadspace_l: terms.deadspace_l,
                    losses_l: terms.trub_and_chiller_loss_l,
                    mash_efficiency: if grain == 0.0 {
                        1.0
                    } else {
                        e.efficiency_override_fraction
                            .or(e.mash_efficiency_fraction)?
                    },
                    kettle_diameter_cm,
                    // Fixed assumptions of the equipment spec: an uncovered
                    // kettle and a 100 °C boil.
                    kettle_opening_cm: kettle_diameter_cm,
                    boil_temp_c: FIXED_BOIL_TEMPERATURE_C,
                    chilling,
                }
            },
        })
    })();
    if let Some(input) = projected {
        match formulation::calculate(&input) {
            Ok(r) => {
                merge_defaults(applied_defaults, projection_defaults);
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
