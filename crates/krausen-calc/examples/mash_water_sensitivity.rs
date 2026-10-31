//! What does the §7 mash-water question actually move?

use krausen_calc::{derive, Equipment, Fermentable, Hop, HopUse, Recipe, Yeast};

fn main() {
    let eq = Equipment {
        boil_off_rate_l_per_hour: 3.0,
        grain_absorption_l_per_kg: 0.8,
        kettle_deadspace_l: 0.0,
        losses_l: 2.0,
        mash_efficiency: 0.78,
        ..Equipment::default()
    };

    let base = |mash_water_l: f64| Recipe {
        name: "Munich Helles v4".into(),
        batch_size_l: 20.5,
        boil_minutes: 90,
        mash_water_l,
        fermentables: vec![
            Fermentable { name: "Signature Lager".into(), kg: 3.9, ppg: 37.0, ebc: 3.6 },
            Fermentable { name: "Munich I".into(), kg: 0.3, ppg: 37.826, ebc: 14.0 },
        ],
        hops: vec![
            Hop { name: "Hallertau".into(), grams: 30.0, alpha_acid: 0.038, minutes: 90, use_: HopUse::Boil },
            Hop { name: "Hallertau".into(), grams: 20.0, alpha_acid: 0.038, minutes: 15, use_: HopUse::Boil },
        ],
        yeast: Yeast { name: "S-23".into(), apparent_attenuation: 0.82 },
        whirlpool_minutes: 0,
        whirlpool_temp_c: None,
    };

    let grain = 4.2_f64;
    let candidates = [
        ("reference sheet", 15.26),
        ("Grainfather 2.7kg+3.5", 2.7 * grain + 3.5),
        ("thickness 2.7 L/kg", 2.7 * grain),
        ("thickness 3.0 L/kg", 3.0 * grain),
    ];

    println!(
        "{:<24}{:>8}{:>10}{:>10}{:>9}{:>8}{:>8}",
        "mash water basis", "mash L", "sparge L", "pre-boil", "OG", "EBC", "IBU"
    );
    for (label, mw) in candidates {
        let d = derive(&base(mw), &eq).expect("derives");
        println!(
            "{:<24}{:>8.2}{:>10.2}{:>10.2}{:>9.4}{:>8.2}{:>8.1}",
            label,
            d.volumes.mash_water_l,
            d.volumes.sparge_water_l,
            d.volumes.pre_boil_l,
            d.original_gravity.sg(),
            d.colour_ebc,
            d.ibu_total
        );
    }
    println!("\nthickness implied by the reference sheet: {:.3} L/kg", 15.26 / grain);
    println!("Grainfather formula for 4.2 kg:          {:.2} L", 2.7 * grain + 3.5);
    println!("unexplained difference:                  {:.2} L", 15.26 - (2.7 * grain + 3.5));
}
