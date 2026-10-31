// SPDX-License-Identifier: AGPL-3.0-or-later
//! Versioned formulation policy. Inputs are resolved before entering this pure engine.
use crate::{Chilling, Equipment, HopUse};

/// The extract yield constant in points·litres per kg at yield 1.
pub const POTENTIAL: f64 = 385.674500456;
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FermentableStage {
    Mash,
    Boil { remaining_minutes: f64 },
}
#[derive(Debug, Clone, PartialEq)]
pub struct FermentableInput {
    pub kg: f64,
    pub yield_fraction: f64,
    pub colour_ebc: f64,
    pub stage: FermentableStage,
}
#[derive(Debug, Clone, PartialEq)]
pub struct HopInput {
    pub grams: f64,
    pub alpha_fraction: f64,
    pub stage: HopUse,
    pub remaining_minutes: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Inputs {
    pub batch_l: f64,
    pub boil_minutes: f64,
    pub mash_water_l: f64,
    pub kettle_top_up_l: f64,
    pub attenuation: Option<f64>,
    pub fermentables: Vec<FermentableInput>,
    pub hops: Vec<HopInput>,
    pub whirlpool_minutes: f64,
    pub whirlpool_temperature_c: Option<f64>,
    pub equipment: Equipment,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Results {
    pub post_boil_l: f64,
    pub pre_boil_l: f64,
    pub grain_absorption_l: f64,
    pub total_water_l: f64,
    pub mash_water_l: f64,
    pub sparge_water_l: f64,
    pub kettle_water_l: f64,
    pub pre_boil_sg: f64,
    pub original_sg: f64,
    pub final_sg: Option<f64>,
    pub abv_percent: Option<f64>,
    pub colour_ebc: f64,
    pub ibu: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Invalid(pub &'static str);

/// Morey colour, independent of attenuation, yield and bitterness inputs.
pub fn colour_ebc(post_boil_l: f64, ingredients: &[(f64, f64)]) -> Result<f64, Invalid> {
    if !post_boil_l.is_finite() || post_boil_l <= 0.0 {
        return Err(Invalid("colour volume"));
    }
    let mut mcu = 0.0;
    for &(kg, colour) in ingredients {
        if !kg.is_finite() || kg <= 0.0 || !colour.is_finite() || colour < 0.0 {
            return Err(Invalid("colour input"));
        }
        let lovibond = if colour == 0.0 {
            0.0
        } else {
            (colour / 1.97 + 0.76) / 1.3546
        };
        mcu += kg * 2.2046226 * lovibond / (post_boil_l * 0.26417205);
    }
    let result = 1.4922 * mcu.powf(0.6859) * 1.97;
    if result.is_finite() {
        Ok(result)
    } else {
        Err(Invalid("colour result"))
    }
}

/// Calculate fully resolved V1 inputs; incomplete or unsupported drafts must not be projected here.
pub fn calculate(i: &Inputs) -> Result<Results, Invalid> {
    let finite = |v: f64| v.is_finite();
    if ![
        i.batch_l,
        i.boil_minutes,
        i.mash_water_l,
        i.kettle_top_up_l,
        i.whirlpool_minutes,
    ]
    .into_iter()
    .all(finite)
        || i.batch_l <= 0.0
        || i.boil_minutes < 0.0
        || i.mash_water_l < 0.0
        || i.kettle_top_up_l < 0.0
        || i.attenuation
            .is_some_and(|a| !a.is_finite() || !(0.0..=1.0).contains(&a))
        || i.whirlpool_minutes < 0.0
        || i.fermentables.is_empty()
    {
        return Err(Invalid("recipe"));
    }
    let e = &i.equipment;
    if ![
        e.boil_off_rate_l_per_hour,
        e.losses_l,
        e.kettle_deadspace_l,
        e.grain_absorption_l_per_kg,
        e.mash_efficiency,
        e.kettle_diameter_cm,
        e.kettle_opening_cm,
        e.boil_temp_c,
    ]
    .into_iter()
    .all(finite)
        || e.boil_off_rate_l_per_hour < 0.0
        || e.losses_l < 0.0
        || e.kettle_deadspace_l < 0.0
        || e.grain_absorption_l_per_kg < 0.0
        || e.mash_efficiency <= 0.0
        || e.mash_efficiency > 1.0
        || e.kettle_diameter_cm <= 0.0
        || e.kettle_opening_cm <= 0.0
        || e.kettle_opening_cm > e.kettle_diameter_cm
    {
        return Err(Invalid("equipment"));
    }
    match e.chilling {
        Chilling::None => (),
        Chilling::Forced {
            rate_per_min,
            plateau_c,
        } if finite(rate_per_min)
            && rate_per_min > 0.0
            && finite(plateau_c)
            && plateau_c < e.boil_temp_c =>
        {
            ()
        }
        Chilling::Counterflow { litres_per_minute }
            if finite(litres_per_minute) && litres_per_minute > 0.0 =>
        {
            ()
        }
        _ => return Err(Invalid("chilling")),
    }
    if i.whirlpool_temperature_c
        .is_some_and(|t| !finite(t) || t > e.boil_temp_c)
    {
        return Err(Invalid("whirlpool temperature"));
    }
    for f in &i.fermentables {
        if ![f.kg, f.yield_fraction, f.colour_ebc]
            .into_iter()
            .all(finite)
            || f.kg <= 0.0
            || f.yield_fraction <= 0.0
            || f.yield_fraction > 1.0
            || f.colour_ebc < 0.0
        {
            return Err(Invalid("fermentable"));
        }
        if let FermentableStage::Boil {
            remaining_minutes: t,
        } = f.stage
        {
            if !finite(t) || t < 0.0 || t > i.boil_minutes {
                return Err(Invalid("fermentable timing"));
            }
        }
    }
    for h in &i.hops {
        if ![h.grams, h.alpha_fraction, h.remaining_minutes]
            .into_iter()
            .all(finite)
            || h.grams <= 0.0
            || !(0.0..=1.0).contains(&h.alpha_fraction)
            || h.remaining_minutes < 0.0
            || (h.stage == HopUse::Boil && h.remaining_minutes > i.boil_minutes)
        {
            return Err(Invalid("hop"));
        }
    }
    let grain: f64 = i
        .fermentables
        .iter()
        .filter(|f| f.stage == FermentableStage::Mash)
        .map(|f| f.kg)
        .sum();
    let post = i.batch_l + e.losses_l;
    let pre = post + e.boil_off_rate_l_per_hour * i.boil_minutes / 60.0;
    let absorption = grain * e.grain_absorption_l_per_kg;
    let total = pre + absorption + e.kettle_deadspace_l;
    let mash = if grain > 0.0 { i.mash_water_l } else { 0.0 };
    let sparge = if grain > 0.0 {
        total - mash - i.kettle_top_up_l
    } else {
        0.0
    };
    let kettle_water = if grain > 0.0 {
        i.kettle_top_up_l
    } else {
        total
    };
    if sparge < 0.0
        || ![post, pre, absorption, total, mash, sparge, kettle_water]
            .into_iter()
            .all(finite)
    {
        return Err(Invalid("water allocation"));
    }
    let points = |f: &FermentableInput| {
        f.kg * f.yield_fraction
            * POTENTIAL
            * if f.stage == FermentableStage::Mash {
                e.mash_efficiency
            } else {
                1.0
            }
    };
    let at = |remaining: f64, include_boundary: bool| -> f64 {
        i.fermentables
            .iter()
            .filter(|f| match f.stage {
                FermentableStage::Mash => true,
                FermentableStage::Boil { remaining_minutes } => {
                    remaining_minutes > remaining
                        || (include_boundary && remaining_minutes == remaining)
                }
            })
            .map(points)
            .sum()
    };
    let pre_points: f64 = i
        .fermentables
        .iter()
        .filter(|f| f.stage == FermentableStage::Mash)
        .map(points)
        .sum();
    let final_points: f64 = i.fermentables.iter().map(points).sum();
    let original_sg = 1.0 + final_points / post / 1000.0;
    if !original_sg.is_finite() || !(1.0..1.775).contains(&original_sg) {
        return Err(Invalid("gravity"));
    }
    let final_sg = i.attenuation.map(|a| 1.0 + (original_sg - 1.0) * (1.0 - a));
    let abv = final_sg.map(|fg| 76.08 * (original_sg - fg) / (1.775 - original_sg) * (fg / 0.794));
    let colour_inputs: Vec<_> = i
        .fermentables
        .iter()
        .map(|f| (f.kg, f.colour_ebc))
        .collect();
    let colour = colour_ebc(post, &colour_inputs)?;
    let mut ibu = 0.0;
    for h in &i.hops {
        if h.stage == HopUse::DryHop {
            continue;
        }
        let exposure = if h.stage == HopUse::Boil {
            h.remaining_minutes
        } else {
            0.0
        };
        let mut boundaries = vec![exposure, 0.0];
        for f in &i.fermentables {
            if let FermentableStage::Boil {
                remaining_minutes: t,
            } = f.stage
            {
                if t > 0.0 && t < exposure {
                    boundaries.push(t);
                }
            }
        }
        boundaries.sort_by(|a, b| b.total_cmp(a));
        boundaries.dedup();
        let mut utilisation = 0.0;
        for pair in boundaries.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            let volume = |remaining: f64| post + e.boil_off_rate_l_per_hour * remaining / 60.0;
            let present = at(start, true);
            let sg_start = 1.0 + present / volume(start) / 1000.0;
            let sg_end = 1.0 + present / volume(end) / 1000.0;
            let bigness = 1.65 * 0.000125_f64.powf((sg_start + sg_end) / 2.0 - 1.0);
            let (a0, a1) = (exposure - start, exposure - end);
            utilisation += bigness * ((-0.04 * a0).exp() - (-0.04 * a1).exp()) / 4.15;
        }
        let post_factor = crate::mibu::post_flameout_utilisation(
            e,
            post,
            i.whirlpool_minutes,
            i.whirlpool_temperature_c,
            &[(-0.04 * exposure).exp()],
        )[0];
        utilisation += 1.65 * 0.000125_f64.powf(original_sg - 1.0) * post_factor;
        ibu += utilisation * h.grams * h.alpha_fraction * 1000.0 / i.batch_l;
    }
    if ![colour, ibu].into_iter().all(finite) || abv.is_some_and(|v| !finite(v)) {
        return Err(Invalid("nonfinite result"));
    }
    Ok(Results {
        post_boil_l: post,
        pre_boil_l: pre,
        grain_absorption_l: absorption,
        total_water_l: total,
        mash_water_l: mash,
        sparge_water_l: sparge,
        kettle_water_l: kettle_water,
        pre_boil_sg: 1.0 + pre_points / pre / 1000.0,
        original_sg,
        final_sg,
        abv_percent: abv,
        colour_ebc: colour,
        ibu,
    })
}

/// Engine source bytes used to identify the actual policy implementation.
pub const ENGINE_SOURCE: &str = concat!(
    include_str!("formulation.rs"),
    include_str!("mibu.rs"),
    include_str!("equipment.rs"),
    include_str!("recipe.rs")
);
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Compiler, target and profile of the actual compiled engine build.
pub const ENGINE_BUILD: &str = concat!(
    env!("KRAUSEN_ENGINE_RUSTC"),
    "|",
    env!("KRAUSEN_ENGINE_TARGET"),
    "|",
    env!("KRAUSEN_ENGINE_PROFILE")
);
