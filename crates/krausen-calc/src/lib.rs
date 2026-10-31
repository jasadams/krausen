// SPDX-License-Identifier: AGPL-3.0-or-later

//! Krausen's calculation core.
//!
//! One entry point: [`derive`], a pure function from a recipe and an equipment
//! profile to every number a bench sheet needs. No I/O, no database, no state,
//! and no dependency on any framework — everything else in Krausen is CRUD
//! around this function.
//!
//! [`PresetCatalogue`] supplies starting profiles for known equipment, so a
//! brewer need not answer questions the software could answer for them.
//!
//! # Evaluation order
//!
//! These are not independent formulas. Most homebrew calculators produce
//! subtly wrong results by evaluating them in the wrong order, so the order
//! below is load-bearing (KRAUSEN.md §4.2). Boil time sits at the root of the
//! chain, which is why changing it moves five other numbers:
//!
//! 1. **Volumes** — post-boil, pre-boil, absorption, total water, sparge
//! 2. **Extract** — potential, kettle points, pre-boil gravity, OG *(needs 1)*
//! 3. **Attenuation** — FG, ABV *(needs 2)*
//! 4. **IBU** — Tinseth through the boil, then mIBU's temperature-weighted
//!    integral for the post-flameout cooling *(needs 2)*
//! 5. **Colour** — Morey over post-boil volume *(needs 1)*

mod colour;
mod derived;
mod equipment;
mod error;
mod extract;
mod ibu;
mod mibu;
mod presets;
mod recipe;
mod units;
mod volumes;

pub use derived::{Corrections, Derived, FermentablePoints, IbuContribution, Volumes};
pub use equipment::{Chilling, Equipment};
pub use presets::{
    builtin_presets, EquipmentField, EquipmentPreset, PresetCatalogue, ProfileSource,
};
pub use error::CalcError;
pub use recipe::{Fermentable, Hop, HopUse, Recipe, Yeast};
pub use units::{Gravity, GAL_PER_L, LB_PER_KG, PPG_TO_POINTS_L_PER_KG};

/// Derive every bench-sheet figure for a recipe brewed on a given brewery.
///
/// Pure: the same inputs always produce the same output.
///
/// # Errors
///
/// Returns [`CalcError`] when the inputs cannot describe a real brew day — an
/// empty grist, a non-positive batch size, an efficiency outside `(0, 1]`, or
/// a water schedule that does not balance (mash water exceeding total water).
pub fn derive(recipe: &Recipe, equipment: &Equipment) -> Result<Derived, CalcError> {
    validate(recipe, equipment)?;

    // 1. Volumes — everything downstream needs these.
    let volumes = volumes::compute(recipe, equipment)?;

    // 2. Extract — needs pre-boil and post-boil volumes.
    let extract = extract::compute(recipe, equipment, &volumes);

    // 3. Attenuation — needs OG.
    let final_gravity = extract::final_gravity(extract.original_gravity, recipe.yeast.apparent_attenuation);
    let abv_percent = extract::abv(extract.original_gravity, final_gravity);

    // 4. IBU — needs both boil gravities.
    let mean_boil_gravity = ibu::mean_boil_gravity(extract.pre_boil_gravity, extract.original_gravity);
    let ibu_contributions =
        ibu::contributions(recipe, equipment, volumes.post_boil_l, mean_boil_gravity);
    let ibu_total = ibu_contributions.iter().map(|c| c.ibu).sum();
    let ibu_boil_only = ibu_contributions.iter().map(|c| c.boil_ibu).sum();

    // 5. Colour — needs post-boil volume.
    let (colour_srm, colour_ebc) = colour::morey(recipe, volumes.post_boil_l);

    let corrections = extract::corrections(&volumes, extract.pre_boil_gravity);

    Ok(Derived {
        volumes,
        fermentable_points: extract.fermentable_points,
        potential_points_l: extract.potential_points_l,
        kettle_points_l: extract.kettle_points_l,
        pre_boil_gravity: extract.pre_boil_gravity,
        original_gravity: extract.original_gravity,
        final_gravity,
        abv_percent,
        mean_boil_gravity,
        ibu_contributions,
        ibu_total,
        ibu_boil_only,
        colour_ebc,
        colour_srm,
        corrections,
    })
}

/// Reject inputs that cannot describe a real brew day.
///
/// Deliberately strict: a silently-wrong bench sheet is worse than a refused
/// one, and these are all conditions a brewer would want told about rather
/// than papered over.
fn validate(recipe: &Recipe, equipment: &Equipment) -> Result<(), CalcError> {
    if recipe.fermentables.is_empty() {
        return Err(CalcError::EmptyGrist);
    }
    if recipe.batch_size_l <= 0.0 {
        return Err(CalcError::NonPositiveBatchSize(recipe.batch_size_l));
    }
    if recipe.total_grain_kg() <= 0.0 {
        return Err(CalcError::NonPositiveGrainWeight(recipe.total_grain_kg()));
    }
    if equipment.mash_efficiency <= 0.0 || equipment.mash_efficiency > 1.0 {
        return Err(CalcError::EfficiencyOutOfRange(equipment.mash_efficiency));
    }
    let attenuation = recipe.yeast.apparent_attenuation;
    if !(0.0..=1.0).contains(&attenuation) {
        return Err(CalcError::AttenuationOutOfRange(attenuation));
    }
    if equipment.boil_off_rate_l_per_hour < 0.0 {
        return Err(CalcError::NegativeBoilOffRate(
            equipment.boil_off_rate_l_per_hour,
        ));
    }
    Ok(())
}
