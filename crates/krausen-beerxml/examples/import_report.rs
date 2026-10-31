//! Run the synthetic BeerXML fixture through the calculation pipeline.

use krausen_beerxml::{convert::to_calc, parse};

const EXAMPLES: &str = include_str!("../tests/fixtures/synthetic.xml");

fn main() {
    let doc = parse(EXAMPLES).expect("parses");
    println!("{} recipes\n", doc.len());

    for recipe in &doc.recipes {
        let c = to_calc(recipe);
        let equipment = c.equipment.clone().unwrap_or_default();

        print!("{:<22}", c.recipe.name);
        match krausen_calc::derive(&c.recipe, &equipment) {
            Ok(d) => println!(
                "OG {:.4}  FG {:.4}  {:.1}% ABV  {:>5.1} EBC  {:>4.1} IBU",
                d.original_gravity.sg(),
                d.final_gravity.sg(),
                d.abv_percent,
                d.colour_ebc,
                d.ibu_total,
            ),
            Err(e) => println!("did not derive: {e}"),
        }

        // What the file said, for comparison — never used as an input.
        if let (Some(og), Some(ibu)) = (recipe.header.og(), recipe.header.ibu()) {
            println!(
                "{:<22}file says: OG {:.4}{:>26.1} IBU",
                "", og, ibu
            );
        }

        if !c.warnings.is_empty() {
            let mut seen = std::collections::BTreeSet::new();
            for w in &c.warnings {
                let line = match w {
                    krausen_beerxml::convert::Warning::UnmodelledHopUse { hop, use_ } => {
                        format!("hop use not modelled: {use_} ({hop})")
                    }
                    krausen_beerxml::convert::Warning::MissingField { field, assumed } => {
                        format!("missing {field}, assumed {assumed}")
                    }
                    krausen_beerxml::convert::Warning::UnreadableValue { field, raw } => {
                        format!("unreadable {field}: {raw:?}")
                    }
                    krausen_beerxml::convert::Warning::NoWhirlpoolData => {
                        "no whirlpool data in BeerXML 1.0".to_string()
                    }
                    krausen_beerxml::convert::Warning::EquipmentIncomplete { missing } => {
                        format!("equipment lacks: {}", missing.join(", "))
                    }
                    krausen_beerxml::convert::Warning::ColourUnitAssumed => {
                        "COLOR assumed to be Lovibond".to_string()
                    }
                };
                if seen.insert(line.clone()) {
                    println!("{:<22}  · {line}", "");
                }
            }
        }
        println!();
    }
}
