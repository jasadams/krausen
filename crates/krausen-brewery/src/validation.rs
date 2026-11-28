// SPDX-License-Identifier: AGPL-3.0-or-later

//! Validation of canonical drafts and resolved equipment, independent of adapters.

use krausen_types::brewery::{CoolingMethodV1, EquipmentSettingsV1, RecipeDocumentV1};
pub use krausen_types::validation::{Diagnostic, Readiness, Target, Validation};

trait ValidationOps {
    fn add(
        &mut self,
        code: &'static str,
        path: impl Into<String>,
        severity: Readiness,
        targets: &[Target],
    );
    fn number(
        &mut self,
        path: impl Into<String>,
        value: Option<f64>,
        required: bool,
        rule: Rule,
        targets: &[Target],
    );
}
impl ValidationOps for Validation {
    fn add(
        &mut self,
        code: &'static str,
        path: impl Into<String>,
        severity: Readiness,
        targets: &[Target],
    ) {
        if severity == Readiness::Invalid || self.state == Readiness::Ready {
            self.state = severity;
        }
        self.diagnostics.push(Diagnostic {
            code: code.into(),
            path: path.into(),
            severity,
            affected_targets: targets.to_vec(),
        });
    }
    fn number(
        &mut self,
        path: impl Into<String>,
        value: Option<f64>,
        required: bool,
        rule: Rule,
        targets: &[Target],
    ) {
        let path = path.into();
        match value {
            None if required => self.add("missing", path, Readiness::Incomplete, targets),
            None => (),
            Some(v) if !v.is_finite() => self.add("non_finite", path, Readiness::Invalid, targets),
            Some(v) if v < 0.0 && !matches!(rule, Rule::Finite) => {
                self.add("negative", path, Readiness::Invalid, targets)
            }
            Some(0.0) if matches!(rule, Rule::Addition) => {
                self.add("unfinished_quantity", path, Readiness::Incomplete, targets)
            }
            Some(v) if !rule.accepts(v) => {
                self.add("out_of_range", path, Readiness::Invalid, targets)
            }
            Some(_) => (),
        }
    }
}

#[derive(Clone, Copy)]
enum Rule {
    Finite,
    Nonnegative,
    Positive,
    Addition,
    Fraction,
    PositiveFraction,
    /// `[0, 1)`: a fraction that cannot reach one.
    FractionBelowOne,
}
impl Rule {
    fn accepts(self, v: f64) -> bool {
        match self {
            Self::Finite => true,
            Self::Nonnegative => v >= 0.0,
            Self::Positive | Self::Addition => v > 0.0,
            Self::Fraction => (0.0..=1.0).contains(&v),
            Self::PositiveFraction => v > 0.0 && v <= 1.0,
            Self::FractionBelowOne => (0.0..1.0).contains(&v),
        }
    }
}

const ALL: &[Target] = &[
    Target::Water,
    Target::OriginalGravity,
    Target::FinalGravity,
    Target::Alcohol,
    Target::Colour,
    Target::Bitterness,
];
const EXTRACT: &[Target] = &[
    Target::OriginalGravity,
    Target::FinalGravity,
    Target::Alcohol,
    Target::Colour,
    Target::Bitterness,
];
const IBU: &[Target] = &[Target::Bitterness];

/// Validate a document against already resolved effective equipment. No I/O,
/// defaults or calculation occurs here; incomplete values remain absent.
pub fn validate_draft(recipe: &RecipeDocumentV1, equipment: &EquipmentSettingsV1) -> Validation {
    let mut v = Validation {
        state: Readiness::Ready,
        diagnostics: vec![],
    };
    if recipe.schema_version != 1 || equipment.schema_version != 1 {
        v.add(
            "unsupported_schema",
            "schema_version",
            Readiness::Incomplete,
            ALL,
        );
    }
    if recipe.recipe_only_equipment && recipe.equipment_profile_id.is_some() {
        // A deliberate recipe-only choice and a named profile contradict each other.
        v.add(
            "recipe_only_with_profile",
            "recipe_only_equipment",
            Readiness::Invalid,
            ALL,
        );
    }
    let mut row_ids = std::collections::HashSet::new();
    for (group, id) in recipe
        .fermentables
        .iter()
        .map(|r| ("fermentables", &r.row_id))
        .chain(recipe.hops.iter().map(|r| ("hops", &r.row_id)))
        .chain(recipe.yeast.iter().map(|r| ("yeast", &r.row_id)))
        .chain(
            recipe
                .other_ingredients
                .iter()
                .map(|r| ("other_ingredients", &r.row_id)),
        )
        .chain(
            recipe
                .process
                .mash_steps
                .iter()
                .map(|r| ("process.mash_steps", &r.row_id)),
        )
        .chain(
            recipe
                .process
                .fermentation_steps
                .iter()
                .map(|r| ("process.fermentation_steps", &r.row_id)),
        )
    {
        if id.is_empty() || !row_ids.insert((group, id)) {
            v.add(
                "invalid_row_id",
                format!("{group}[{id}].row_id"),
                Readiness::Invalid,
                ALL,
            );
        }
    }
    let overrides = &recipe.equipment_overrides;
    for (field, value, rule) in [
        (
            "efficiency_override_fraction",
            overrides.efficiency_override_fraction,
            Rule::PositiveFraction,
        ),
        (
            "mash_tun_volume_l",
            overrides.mash_tun_volume_l,
            Rule::Positive,
        ),
        (
            "mash_efficiency_fraction",
            overrides.mash_efficiency_fraction,
            Rule::PositiveFraction,
        ),
        (
            "mash_tun_deadspace_l",
            overrides.mash_tun_deadspace_l,
            Rule::Nonnegative,
        ),
        (
            "mash_thickness_l_per_kg",
            overrides.mash_thickness_l_per_kg,
            Rule::Positive,
        ),
        (
            "mash_tun_loss_l",
            overrides.mash_tun_loss_l,
            Rule::Nonnegative,
        ),
        (
            "mash_grain_absorption_l_per_kg",
            overrides.mash_grain_absorption_l_per_kg,
            Rule::Nonnegative,
        ),
        (
            "sparge_vessel_volume_l",
            overrides.sparge_vessel_volume_l,
            Rule::Positive,
        ),
        (
            "sparge_grain_absorption_l_per_kg",
            overrides.sparge_grain_absorption_l_per_kg,
            Rule::Nonnegative,
        ),
        (
            "sparge_deadspace_l",
            overrides.sparge_deadspace_l,
            Rule::Nonnegative,
        ),
        ("boil_volume_l", overrides.boil_volume_l, Rule::Positive),
        (
            "wort_shrinkage_fraction",
            overrides.wort_shrinkage_fraction,
            Rule::FractionBelowOne,
        ),
        (
            "boil_loss_l_per_hour",
            overrides.boil_loss_l_per_hour,
            Rule::Nonnegative,
        ),
        (
            "trub_and_chiller_loss_l",
            overrides.trub_and_chiller_loss_l,
            Rule::Nonnegative,
        ),
        (
            "kettle_diameter_cm",
            overrides.kettle_diameter_cm,
            Rule::Positive,
        ),
        (
            "cooling_flow_rate_l_per_minute",
            overrides.cooling_flow_rate_l_per_minute,
            Rule::Positive,
        ),
    ] {
        v.number(
            format!("equipment_overrides.{field}"),
            value,
            false,
            rule,
            ALL,
        );
    }
    v.number(
        "batch_size_l",
        recipe.batch_size_l,
        true,
        Rule::Positive,
        ALL,
    );
    v.number(
        "boil_minutes",
        recipe.boil_minutes,
        true,
        Rule::Nonnegative,
        ALL,
    );
    if recipe.fermentables.is_empty() {
        v.add(
            "empty_fermentables",
            "fermentables",
            Readiness::Incomplete,
            EXTRACT,
        );
    }
    for row in &recipe.fermentables {
        let path = format!("fermentables[{}]", row.row_id);
        v.number(
            format!("{path}.amount_kg"),
            row.amount_kg,
            true,
            Rule::Addition,
            ALL,
        );
        v.number(
            format!("{path}.specification.yield_fraction"),
            row.specification.yield_fraction,
            true,
            Rule::PositiveFraction,
            &[
                Target::OriginalGravity,
                Target::FinalGravity,
                Target::Alcohol,
                Target::Bitterness,
            ],
        );
        v.number(
            format!("{path}.specification.colour_ebc"),
            row.specification.colour_ebc,
            true,
            Rule::Nonnegative,
            &[Target::Colour],
        );
        v.number(
            format!("{path}.addition.time_minutes"),
            row.addition.time_minutes,
            row.addition.stage == "boil",
            Rule::Nonnegative,
            EXTRACT,
        );
        if !matches!(row.addition.stage.as_str(), "mash" | "boil") {
            v.add(
                "unsupported_fermentable_stage",
                format!("{path}.addition.stage"),
                Readiness::Incomplete,
                EXTRACT,
            );
        }
        if !matches!(
            (
                row.specification.kind.as_deref(),
                row.addition.stage.as_str()
            ),
            (Some("grain"), "mash") | (Some("extract" | "sugar"), "boil")
        ) {
            v.add(
                "unsupported_fermentable_kind_or_stage",
                format!("{path}.specification.type"),
                Readiness::Incomplete,
                EXTRACT,
            );
        }
        if let (Some(t), Some(boil)) = (row.addition.time_minutes, recipe.boil_minutes) {
            if row.addition.stage == "boil" && t.is_finite() && boil.is_finite() && t > boil {
                v.add(
                    "exposure_exceeds_boil",
                    format!("{path}.addition.time_minutes"),
                    Readiness::Invalid,
                    EXTRACT,
                );
            }
        }
    }
    for row in &recipe.hops {
        let path = format!("hops[{}]", row.row_id);
        v.number(
            format!("{path}.amount_g"),
            row.amount_g,
            true,
            Rule::Addition,
            IBU,
        );
        v.number(
            format!("{path}.specification.alpha_fraction"),
            row.specification.alpha_fraction,
            row.stage != "dry_hop",
            Rule::Fraction,
            IBU,
        );
        v.number(
            format!("{path}.contact_minutes"),
            row.contact_minutes,
            row.stage == "boil",
            Rule::Nonnegative,
            IBU,
        );
        v.number(
            format!("{path}.temperature_c"),
            row.temperature_c,
            false,
            Rule::Finite,
            IBU,
        );
        if !matches!(row.stage.as_str(), "boil" | "whirlpool" | "dry_hop") {
            v.add(
                "unsupported_hop_stage",
                format!("{path}.stage"),
                Readiness::Incomplete,
                IBU,
            );
        }
        if row.stage == "whirlpool" && row.temperature_c.is_some() {
            v.add(
                "unsupported_cooled_hop_addition",
                format!("{path}.temperature_c"),
                Readiness::Incomplete,
                IBU,
            );
        }
        if let (Some(t), Some(boil)) = (row.contact_minutes, recipe.boil_minutes) {
            if row.stage == "boil" && t.is_finite() && boil.is_finite() && t > boil {
                v.add(
                    "exposure_exceeds_boil",
                    format!("{path}.contact_minutes"),
                    Readiness::Invalid,
                    IBU,
                );
            }
        }
    }
    v.number(
        "process.apparent_attenuation_fraction",
        recipe.process.apparent_attenuation_fraction,
        true,
        Rule::Fraction,
        &[Target::FinalGravity, Target::Alcohol],
    );
    for row in &recipe.yeast {
        v.number(
            format!("yeast[{}].specification.attenuation_fraction", row.row_id),
            row.specification.attenuation_fraction,
            false,
            Rule::Fraction,
            &[Target::FinalGravity, Target::Alcohol],
        );
    }
    for row in recipe.yeast.iter().chain(&recipe.other_ingredients) {
        v.number(
            format!("ingredients[{}].amount", row.row_id),
            row.amount,
            false,
            Rule::Addition,
            &[],
        );
    }
    for (name, steps) in [
        ("mash_steps", &recipe.process.mash_steps),
        ("fermentation_steps", &recipe.process.fermentation_steps),
    ] {
        for row in steps {
            let path = format!("process.{name}[{}]", row.row_id);
            v.number(
                format!("{path}.time_minutes"),
                row.time_minutes,
                false,
                Rule::Nonnegative,
                &[],
            );
            v.number(
                format!("{path}.temperature_c"),
                row.temperature_c,
                false,
                Rule::Finite,
                &[],
            );
            v.number(
                format!("{path}.volume_l"),
                row.volume_l,
                false,
                Rule::Nonnegative,
                &[],
            );
        }
    }
    v.number(
        "process.whirlpool_minutes",
        recipe.process.whirlpool_minutes,
        recipe
            .hops
            .iter()
            .any(|row| matches!(row.stage.as_str(), "boil" | "whirlpool")),
        Rule::Nonnegative,
        IBU,
    );
    v.number(
        "process.whirlpool_temperature_c",
        recipe.process.whirlpool_temperature_c,
        false,
        Rule::Finite,
        IBU,
    );
    let has_mashed_grain = recipe
        .fermentables
        .iter()
        .any(|r| r.specification.kind.as_deref() == Some("grain") && r.addition.stage == "mash");
    v.number(
        "process.mash_water_l",
        recipe.process.mash_water_l,
        has_mashed_grain && recipe.process.mash_water_method.as_deref() == Some("manual"),
        Rule::Nonnegative,
        &[Target::Water],
    );
    v.number(
        "process.kettle_top_up_l",
        recipe.process.kettle_top_up_l,
        false,
        Rule::Nonnegative,
        &[Target::Water],
    );
    if has_mashed_grain
        && !matches!(
            recipe.process.mash_water_method.as_deref(),
            Some("manual" | "profile_thickness")
        )
    {
        v.add(
            "missing_or_unsupported_mash_water_method",
            "process.mash_water_method",
            Readiness::Incomplete,
            &[Target::Water],
        );
    }
    if !recipe.process.unsupported.is_empty() {
        v.add(
            "unsupported_process",
            "process.unsupported",
            Readiness::Incomplete,
            ALL,
        );
    }
    for (path, value, rule, targets) in [
        (
            "equipment.mash_efficiency_fraction",
            equipment.mash_efficiency_fraction,
            Rule::PositiveFraction,
            EXTRACT,
        ),
        (
            "equipment.efficiency_override_fraction",
            equipment.efficiency_override_fraction,
            Rule::PositiveFraction,
            EXTRACT,
        ),
        (
            "equipment.boil_loss_l_per_hour",
            equipment.boil_loss_l_per_hour,
            Rule::Nonnegative,
            ALL,
        ),
        (
            "equipment.trub_and_chiller_loss_l",
            equipment.trub_and_chiller_loss_l,
            Rule::Nonnegative,
            ALL,
        ),
        (
            "equipment.mash_tun_deadspace_l",
            equipment.mash_tun_deadspace_l,
            Rule::Nonnegative,
            &[Target::Water][..],
        ),
        (
            "equipment.mash_tun_loss_l",
            equipment.mash_tun_loss_l,
            Rule::Nonnegative,
            &[Target::Water][..],
        ),
        (
            "equipment.mash_grain_absorption_l_per_kg",
            equipment.mash_grain_absorption_l_per_kg,
            Rule::Nonnegative,
            &[Target::Water][..],
        ),
        (
            "equipment.sparge_grain_absorption_l_per_kg",
            equipment.sparge_grain_absorption_l_per_kg,
            Rule::Nonnegative,
            &[Target::Water][..],
        ),
        (
            "equipment.sparge_deadspace_l",
            equipment.sparge_deadspace_l,
            Rule::Nonnegative,
            &[Target::Water][..],
        ),
        (
            "equipment.wort_shrinkage_fraction",
            equipment.wort_shrinkage_fraction,
            Rule::FractionBelowOne,
            ALL,
        ),
        (
            "equipment.mash_thickness_l_per_kg",
            equipment.mash_thickness_l_per_kg,
            Rule::Positive,
            &[Target::Water][..],
        ),
        (
            "equipment.mash_tun_volume_l",
            equipment.mash_tun_volume_l,
            Rule::Positive,
            &[][..],
        ),
        (
            "equipment.sparge_vessel_volume_l",
            equipment.sparge_vessel_volume_l,
            Rule::Positive,
            &[][..],
        ),
        (
            "equipment.boil_volume_l",
            equipment.boil_volume_l,
            Rule::Positive,
            &[][..],
        ),
        (
            "equipment.kettle_diameter_cm",
            equipment.kettle_diameter_cm,
            Rule::Positive,
            IBU,
        ),
        (
            "equipment.cooling_flow_rate_l_per_minute",
            equipment.cooling_flow_rate_l_per_minute,
            Rule::Positive,
            IBU,
        ),
    ] {
        v.number(path, value, false, rule, targets);
    }
    for (path, value, targets) in [
        (
            "equipment.boil_loss_l_per_hour",
            equipment.boil_loss_l_per_hour,
            ALL,
        ),
        (
            "equipment.trub_and_chiller_loss_l",
            equipment.trub_and_chiller_loss_l,
            ALL,
        ),
        (
            "equipment.mash_tun_deadspace_l",
            equipment.mash_tun_deadspace_l,
            &[Target::Water][..],
        ),
    ] {
        if value.is_none() {
            v.add("missing", path, Readiness::Incomplete, targets);
        }
    }
    if has_mashed_grain && equipment.mash_grain_absorption_l_per_kg.is_none() {
        v.add(
            "missing",
            "equipment.mash_grain_absorption_l_per_kg",
            Readiness::Incomplete,
            &[Target::Water],
        );
    }
    if has_mashed_grain
        && equipment
            .efficiency_override_fraction
            .or(equipment.mash_efficiency_fraction)
            .is_none()
    {
        v.add(
            "missing",
            "equipment.mash_efficiency_fraction",
            Readiness::Incomplete,
            EXTRACT,
        );
    }
    if has_mashed_grain
        && recipe.process.mash_water_method.as_deref() == Some("profile_thickness")
        && equipment.mash_thickness_l_per_kg.is_none()
    {
        v.add(
            "missing",
            "equipment.mash_thickness_l_per_kg",
            Readiness::Incomplete,
            &[Target::Water],
        );
    }
    let grain: Option<f64> = recipe
        .fermentables
        .iter()
        .filter(|r| r.specification.kind.as_deref() == Some("grain") && r.addition.stage == "mash")
        .map(|r| r.amount_kg)
        .sum();
    if let (Some(batch), Some(loss), Some(rate), Some(minutes), Some(deadspace), Some(grain)) = (
        recipe.batch_size_l,
        equipment.trub_and_chiller_loss_l,
        equipment.boil_loss_l_per_hour,
        recipe.boil_minutes,
        equipment.mash_tun_deadspace_l,
        grain,
    ) {
        let absorption = if has_mashed_grain {
            equipment.mash_grain_absorption_l_per_kg.map(|mash| {
                mash + equipment.sparge_grain_absorption_l_per_kg.unwrap_or(0.0)
            })
        } else {
            Some(0.0)
        };
        let mash = if !has_mashed_grain {
            Some(0.0)
        } else {
            match recipe.process.mash_water_method.as_deref() {
                Some("manual") => recipe.process.mash_water_l,
                Some("profile_thickness") => equipment
                    .mash_thickness_l_per_kg
                    .map(|thickness| thickness * grain),
                _ => None,
            }
        };
        if let (Some(absorption), Some(mash)) = (absorption, mash) {
            let shrinkage = equipment.wort_shrinkage_fraction.unwrap_or(0.0);
            let total = batch
                + loss
                + rate * minutes / 60.0 * (1.0 - shrinkage)
                + deadspace
                + equipment.mash_tun_loss_l.unwrap_or(0.0)
                + equipment.sparge_deadspace_l.unwrap_or(0.0)
                + absorption * grain;
            let top_up = recipe.process.kettle_top_up_l.unwrap_or(0.0);
            if total.is_finite() && mash.is_finite() && top_up.is_finite() && mash + top_up > total
            {
                v.add(
                    "negative_sparge",
                    "process.mash_water_l",
                    Readiness::Invalid,
                    &[Target::Water],
                );
            }
        }
    }
    let has_hot_hops = recipe.hops.iter().any(|row| row.stage != "dry_hop");
    if has_hot_hops && equipment.kettle_diameter_cm.is_none() {
        v.add(
            "missing",
            "equipment.kettle_diameter_cm",
            Readiness::Incomplete,
            IBU,
        );
    }
    match equipment.cooling_method {
        None if has_hot_hops => v.add(
            "missing",
            "equipment.cooling_method",
            Readiness::Incomplete,
            IBU,
        ),
        Some(CoolingMethodV1::CounterflowChiller)
            if equipment.cooling_flow_rate_l_per_minute.is_none() =>
        {
            v.add(
                "missing",
                "equipment.cooling_flow_rate_l_per_minute",
                Readiness::Incomplete,
                IBU,
            )
        }
        _ => (),
    }
    v
}
