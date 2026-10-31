// SPDX-License-Identifier: AGPL-3.0-or-later

//! Turning an imported recipe into the engine's inputs.
//!
//! This is the only place units change. BeerXML speaks kilograms, percentages
//! and degrees Lovibond; [`krausen_calc`] speaks grams, fractions and EBC.
//! Keeping the translation here means the interchange model never drifts from
//! what the file actually said.
//!
//! Conversion reports what it could not do. A recipe that loses its first-wort
//! hops to an approximation, or whose equipment profile has no grain
//! absorption to import, should say so rather than hand back a confident
//! number — §3.5 wants the gaps visible, not filled in silently.

use krausen_calc::{Equipment, Fermentable, Hop, HopUse, Recipe, Yeast};

use crate::{document::Recipe as XmlRecipe, record::Scalar};

/// Points per pound per US gallon contributed by pure sucrose.
///
/// BeerXML carries extract as `YIELD`, a percentage of this, rather than as
/// PPG. See KRAUSEN.md §4.3 on why PPG is itself only an interchange unit.
const SUCROSE_PPG: f64 = 46.214;

/// Something the conversion could not do faithfully.
///
/// Not errors — the conversion still produces a usable recipe. These are the
/// places a brewer should look before trusting the numbers.
#[derive(Debug, Clone, PartialEq)]
pub enum Warning {
    /// A hop use BeerXML has and the engine does not model.
    ///
    /// First-wort and mash hops isomerise differently from a boil addition of
    /// the same length. Treating them as boil additions would overstate or
    /// understate the bitterness rather than admit the gap.
    UnmodelledHopUse { hop: String, use_: String },

    /// A required value was missing and a default was substituted.
    MissingField {
        field: &'static str,
        assumed: String,
    },

    /// A value was present but could not be read as a number. Different
    /// from absent: the brewer put something there, and it did not survive.
    UnreadableValue { field: &'static str, raw: String },

    /// BeerXML 1.0 carries no whirlpool time or temperature, so an imported
    /// recipe cannot describe a hop stand. Post-flameout bitterness will be
    /// modelled from chilling alone until the brewer supplies one.
    NoWhirlpoolData,

    /// The equipment record has no grain absorption or kettle dimensions —
    /// BeerXML simply has no field for them.
    EquipmentIncomplete { missing: Vec<&'static str> },

    /// `COLOR` is Lovibond by the standard's wording but SRM in much
    /// real-world output, and the two differ enough to matter for pale beer.
    ColourUnitAssumed,
}

/// What a conversion produced, together with everything it could not do.
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    pub recipe: Recipe,
    /// The equipment profile, where the file carried one worth using.
    pub equipment: Option<Equipment>,
    pub warnings: Vec<Warning>,
}

/// Convert an imported recipe into the engine's inputs.
pub fn to_calc(xml: &XmlRecipe) -> Converted {
    let mut warnings = Vec::new();

    let batch_size_l = number(
        xml.header.batch_size.as_ref(),
        "BATCH_SIZE",
        20.0,
        &mut warnings,
    );
    let boil_minutes = number(
        xml.header.boil_time.as_ref(),
        "BOIL_TIME",
        60.0,
        &mut warnings,
    ) as u32;

    let fermentables = convert_fermentables(xml, &mut warnings);
    let hops = convert_hops(xml, &mut warnings);
    let yeast = convert_yeast(xml, &mut warnings);
    let mash_water_l = mash_water(xml);

    if !hops.is_empty() {
        // Worth saying once, not per hop: nothing in BeerXML 1.0 can express
        // a stand, and the post-flameout model is the better for having one.
        warnings.push(Warning::NoWhirlpoolData);
    }

    let recipe = Recipe {
        name: xml.name().to_string(),
        batch_size_l,
        boil_minutes,
        mash_water_l,
        fermentables,
        hops,
        yeast,
        whirlpool_minutes: 0,
        whirlpool_temp_c: None,
    };

    let equipment = xml
        .equipment
        .as_ref()
        .map(|eq| convert_equipment(eq, &recipe, &mut warnings));

    Converted {
        recipe,
        equipment,
        warnings,
    }
}

/// Read a numeric field, distinguishing "absent" from "there but unreadable".
fn number(
    scalar: Option<&Scalar<f64>>,
    field: &'static str,
    default: f64,
    warnings: &mut Vec<Warning>,
) -> f64 {
    match scalar {
        Some(s) => match s.value() {
            Some(v) => *v,
            None => {
                warnings.push(Warning::UnreadableValue {
                    field,
                    raw: s.raw().to_string(),
                });
                default
            }
        },
        None => {
            warnings.push(Warning::MissingField {
                field,
                assumed: default.to_string(),
            });
            default
        }
    }
}

fn convert_fermentables(xml: &XmlRecipe, warnings: &mut Vec<Warning>) -> Vec<Fermentable> {
    if !xml.fermentables.is_empty() {
        warnings.push(Warning::ColourUnitAssumed);
    }

    xml.fermentables
        .iter()
        .map(|f| Fermentable {
            name: f.name().cloned().unwrap_or_default(),
            kg: f.amount().copied().unwrap_or(0.0),
            // YIELD is a percentage of sucrose's extract, not PPG.
            ppg: f.yield_().copied().unwrap_or(0.0) / 100.0 * SUCROSE_PPG,
            ebc: lovibond_to_ebc(f.color().copied().unwrap_or(0.0)),
        })
        .collect()
}

/// Degrees Lovibond to EBC — the inverse of the conversion Morey uses.
fn lovibond_to_ebc(lovibond: f64) -> f64 {
    ((lovibond * 1.3546) - 0.76) * 1.97
}

fn convert_hops(xml: &XmlRecipe, warnings: &mut Vec<Warning>) -> Vec<Hop> {
    let mut hops = Vec::new();

    for h in &xml.hops {
        let name = h.name().cloned().unwrap_or_default();
        let raw_use = h.use_().map_or("Boil", String::as_str);

        let use_ = match raw_use.trim().to_ascii_lowercase().as_str() {
            "boil" => HopUse::Boil,
            "dry hop" => HopUse::DryHop,
            // An aroma addition goes in at flameout, which is what the
            // whirlpool case models.
            "aroma" => HopUse::Whirlpool,
            other => {
                // First wort and mash hops are real uses with real, different
                // utilisation. Silently calling them boil additions would put
                // a wrong number on a bench sheet.
                warnings.push(Warning::UnmodelledHopUse {
                    hop: name.clone(),
                    use_: other.to_string(),
                });
                HopUse::Boil
            }
        };

        hops.push(Hop {
            name,
            // BeerXML weighs hops in kilograms.
            grams: h.amount().copied().unwrap_or(0.0) * 1000.0,
            // ALPHA is a percentage.
            alpha_acid: h.alpha().copied().unwrap_or(0.0) / 100.0,
            minutes: h.time().copied().unwrap_or(0.0).max(0.0) as u32,
            use_,
        });
    }
    hops
}

fn convert_yeast(xml: &XmlRecipe, warnings: &mut Vec<Warning>) -> Yeast {
    match xml.yeasts.first() {
        Some(y) => Yeast {
            name: y.name().cloned().unwrap_or_default(),
            // ATTENUATION is a percentage.
            apparent_attenuation: y.attenuation().copied().unwrap_or(75.0) / 100.0,
        },
        None => {
            warnings.push(Warning::MissingField {
                field: "YEAST",
                assumed: "75% apparent attenuation".to_string(),
            });
            Yeast {
                name: String::new(),
                apparent_attenuation: 0.75,
            }
        }
    }
}

/// Mash water, summed from the infusion steps.
///
/// A pleasant consequence of modelling the mash schedule: BeerXML records how
/// much water each infusion adds, so mash water is recoverable rather than
/// guessed. §7 leaves open how Krausen should derive it in general; where a
/// file states it, the file wins.
fn mash_water(xml: &XmlRecipe) -> f64 {
    xml.mash
        .as_ref()
        .map(|m| {
            m.steps
                .iter()
                .filter_map(|s| s.infuse_amount().copied())
                .sum()
        })
        .unwrap_or(0.0)
}

fn convert_equipment(
    eq: &crate::records::Equipment,
    recipe: &Recipe,
    warnings: &mut Vec<Warning>,
) -> Equipment {
    let defaults = Equipment::default();

    // EVAP_RATE is a percentage of the boil volume per hour, so it only
    // becomes litres per hour once a volume is known.
    let boil_size = eq
        .boil_size()
        .copied()
        .unwrap_or(recipe.batch_size_l + defaults.losses_l);
    let boil_off_rate_l_per_hour = match eq.evap_rate() {
        Some(percent) => boil_size * percent / 100.0,
        None => defaults.boil_off_rate_l_per_hour,
    };

    // BeerXML has no field for either of these, so they stay as defaults and
    // the caller is told.
    warnings.push(Warning::EquipmentIncomplete {
        missing: vec!["grain absorption", "kettle dimensions", "chilling method"],
    });

    Equipment {
        boil_off_rate_l_per_hour,
        grain_absorption_l_per_kg: defaults.grain_absorption_l_per_kg,
        kettle_deadspace_l: eq
            .lauter_deadspace()
            .copied()
            .unwrap_or(defaults.kettle_deadspace_l),
        losses_l: eq
            .trub_chiller_loss()
            .copied()
            .unwrap_or(defaults.losses_l),
        ..defaults
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn convert(xml: &str) -> Converted {
        let doc = parse(xml).expect("parses");
        to_calc(&doc.recipes[0])
    }

    #[test]
    fn hop_weight_becomes_grams_and_alpha_becomes_a_fraction() {
        let c = convert(
            r#"<RECIPE><HOPS><HOP>
                 <NAME>Goldings</NAME><AMOUNT>0.0283500</AMOUNT>
                 <ALPHA>5.50</ALPHA><TIME>60.0</TIME><USE>Boil</USE>
               </HOP></HOPS></RECIPE>"#,
        );
        let hop = &c.recipe.hops[0];
        assert!((hop.grams - 28.35).abs() < 1e-9);
        assert!((hop.alpha_acid - 0.055).abs() < 1e-12);
        assert_eq!(hop.minutes, 60);
        assert_eq!(hop.use_, HopUse::Boil);
    }

    #[test]
    fn yield_becomes_ppg_against_sucrose() {
        let c = convert(
            r#"<RECIPE><FERMENTABLES><FERMENTABLE>
                 <NAME>Pale Malt</NAME><AMOUNT>5.0</AMOUNT>
                 <YIELD>80.0</YIELD><COLOR>3.0</COLOR>
               </FERMENTABLE></FERMENTABLES></RECIPE>"#,
        );
        let f = &c.recipe.fermentables[0];
        // 80% of sucrose's 46.214 ppg.
        assert!((f.ppg - 36.9712).abs() < 1e-4, "got {}", f.ppg);
        assert!((f.kg - 5.0).abs() < 1e-12);
    }

    #[test]
    fn attenuation_becomes_a_fraction() {
        let c = convert(
            r#"<RECIPE><YEASTS><YEAST>
                 <NAME>S-23</NAME><ATTENUATION>82.0</ATTENUATION>
               </YEAST></YEASTS></RECIPE>"#,
        );
        assert!((c.recipe.yeast.apparent_attenuation - 0.82).abs() < 1e-12);
    }

    #[test]
    fn a_hop_use_the_engine_cannot_model_is_reported_not_hidden() {
        let c = convert(
            r#"<RECIPE><HOPS><HOP>
                 <NAME>Magnum</NAME><AMOUNT>0.02</AMOUNT>
                 <ALPHA>12.0</ALPHA><TIME>60.0</TIME><USE>First Wort</USE>
               </HOP></HOPS></RECIPE>"#,
        );
        assert!(
            c.warnings.iter().any(|w| matches!(
                w,
                Warning::UnmodelledHopUse { use_, .. } if use_ == "first wort"
            )),
            "first-wort hops must be flagged, not quietly treated as boil"
        );
    }

    #[test]
    fn an_aroma_addition_becomes_a_whirlpool_addition() {
        let c = convert(
            r#"<RECIPE><HOPS><HOP>
                 <NAME>Saaz</NAME><AMOUNT>0.02</AMOUNT>
                 <ALPHA>3.0</ALPHA><TIME>0.0</TIME><USE>Aroma</USE>
               </HOP></HOPS></RECIPE>"#,
        );
        assert_eq!(c.recipe.hops[0].use_, HopUse::Whirlpool);
    }

    #[test]
    fn mash_water_comes_from_the_infusion_steps() {
        let c = convert(
            r#"<RECIPE><MASH><MASH_STEPS>
                 <MASH_STEP><NAME>Mash In</NAME><INFUSE_AMOUNT>15.0</INFUSE_AMOUNT></MASH_STEP>
                 <MASH_STEP><NAME>Mash Out</NAME><INFUSE_AMOUNT>5.5</INFUSE_AMOUNT></MASH_STEP>
               </MASH_STEPS></MASH></RECIPE>"#,
        );
        assert!((c.recipe.mash_water_l - 20.5).abs() < 1e-12);
    }

    #[test]
    fn evaporation_rate_becomes_litres_per_hour_using_the_boil_volume() {
        let c = convert(
            r#"<RECIPE><BATCH_SIZE>20.0</BATCH_SIZE><EQUIPMENT>
                 <NAME>Rig</NAME><BOIL_SIZE>25.0</BOIL_SIZE><EVAP_RATE>12.0</EVAP_RATE>
               </EQUIPMENT></RECIPE>"#,
        );
        let eq = c.equipment.expect("has equipment");
        // 12% of 25 L per hour.
        assert!((eq.boil_off_rate_l_per_hour - 3.0).abs() < 1e-12);
    }

    #[test]
    fn an_imported_equipment_profile_says_what_beerxml_could_not_supply() {
        let c = convert(
            r#"<RECIPE><EQUIPMENT><NAME>Rig</NAME><BOIL_SIZE>25.0</BOIL_SIZE></EQUIPMENT></RECIPE>"#,
        );
        assert!(
            c.warnings
                .iter()
                .any(|w| matches!(w, Warning::EquipmentIncomplete { .. })),
            "BeerXML has no grain absorption or kettle dimensions"
        );
    }

    #[test]
    fn a_value_that_is_present_but_unreadable_is_reported_as_such() {
        // Not the same as missing — the brewer wrote something there.
        let c = convert("<RECIPE><NAME>x</NAME><BATCH_SIZE>about twenty</BATCH_SIZE></RECIPE>");
        assert!(
            c.warnings.iter().any(|w| matches!(
                w,
                Warning::UnreadableValue { field, raw } if *field == "BATCH_SIZE" && raw == "about twenty"
            )),
            "got {:?}",
            c.warnings
        );
        assert!(
            !c.warnings.iter().any(
                |w| matches!(w, Warning::MissingField { field, .. } if *field == "BATCH_SIZE")
            ),
            "it was not missing"
        );
    }

    #[test]
    fn a_missing_value_is_defaulted_and_declared() {
        let c = convert("<RECIPE><NAME>Bare</NAME></RECIPE>");
        assert!(c.warnings.iter().any(
            |w| matches!(w, Warning::MissingField { field, .. } if *field == "BATCH_SIZE")
        ));
        assert!(
            c.warnings
                .iter()
                .any(|w| matches!(w, Warning::MissingField { field, .. } if *field == "YEAST"))
        );
    }

    #[test]
    fn lovibond_and_ebc_convert_back_and_forth() {
        // 3 Lovibond is a pale malt; the round trip should return it.
        let ebc = lovibond_to_ebc(3.0);
        let back = ((ebc / 1.97) + 0.76) / 1.3546;
        assert!((back - 3.0).abs() < 1e-9, "got {back}");
    }

    #[test]
    fn a_converted_recipe_derives() {
        let c = convert(
            r#"<RECIPE>
                 <NAME>Importable</NAME><BATCH_SIZE>20.0</BATCH_SIZE><BOIL_TIME>60.0</BOIL_TIME>
                 <FERMENTABLES><FERMENTABLE><NAME>Pale</NAME><AMOUNT>5.0</AMOUNT><YIELD>80.0</YIELD><COLOR>3.0</COLOR></FERMENTABLE></FERMENTABLES>
                 <HOPS><HOP><NAME>EKG</NAME><AMOUNT>0.03</AMOUNT><ALPHA>5.0</ALPHA><TIME>60.0</TIME><USE>Boil</USE></HOP></HOPS>
                 <YEASTS><YEAST><NAME>US-05</NAME><ATTENUATION>81.0</ATTENUATION></YEAST></YEASTS>
                 <MASH><MASH_STEPS><MASH_STEP><NAME>In</NAME><INFUSE_AMOUNT>16.0</INFUSE_AMOUNT></MASH_STEP></MASH_STEPS></MASH>
               </RECIPE>"#,
        );

        let equipment = c.equipment.unwrap_or_default();
        let derived = krausen_calc::derive(&c.recipe, &equipment).expect("derives");
        assert!(derived.original_gravity.points() > 0.0);
        assert!(derived.ibu_total > 0.0);
    }
}
