//! Does the post-flameout model close the §4.5 gap against Brewfather?
//!
//! Appendix A records Tinseth boil-only at 19.4 / 17.1 IBU where Brewfather
//! reports 21.0 / 19.2. The chilling process for those brews is not recorded,
//! so this sweeps plausible ones rather than fitting to a single guess.

use krausen_calc::{derive, Chilling, Equipment, Fermentable, Hop, HopUse, Recipe, Yeast};

fn recipe(boil_minutes: u32, hops: Vec<(f64, u32)>) -> Recipe {
    Recipe {
        name: "Munich Helles".into(),
        batch_size_l: 20.5,
        boil_minutes,
        mash_water_l: 15.26,
        fermentables: vec![
            Fermentable { name: "Signature Lager".into(), kg: 3.9, ppg: 37.0, ebc: 3.6 },
            Fermentable { name: "Munich I".into(), kg: 0.3, ppg: 37.826, ebc: 14.0 },
        ],
        hops: hops
            .into_iter()
            .map(|(grams, minutes)| Hop {
                name: "Hallertau".into(),
                grams,
                alpha_acid: 0.038,
                minutes,
                use_: HopUse::Boil,
            })
            .collect(),
        yeast: Yeast { name: "S-23".into(), apparent_attenuation: 0.82 },
        whirlpool_minutes: 0,
        whirlpool_temp_c: None,
    }
}

fn equipment(diameter_cm: f64, opening_cm: f64, chilling: Chilling) -> Equipment {
    Equipment {
        boil_off_rate_l_per_hour: 3.0,
        grain_absorption_l_per_kg: 0.8,
        kettle_deadspace_l: 0.0,
        losses_l: 2.0,
        mash_efficiency: 0.78,
        kettle_diameter_cm: diameter_cm,
        kettle_opening_cm: opening_cm,
        ..Equipment::default()
    }
    .with_chilling(chilling)
}

trait WithChilling {
    fn with_chilling(self, c: Chilling) -> Self;
}
impl WithChilling for Equipment {
    fn with_chilling(mut self, c: Chilling) -> Self {
        self.chilling = c;
        self
    }
}

fn main() {
    let v4 = recipe(90, vec![(30.0, 90), (20.0, 15)]);
    let v5 = recipe(60, vec![(25.0, 60), (25.0, 15)]);

    println!("Appendix A: Tinseth boil-only 19.4 / 17.1   Brewfather 21.0 / 19.2\n");
    println!(
        "{:<34}{:>12}{:>12}{:>12}{:>12}",
        "chilling scenario", "v4 total", "v4 target", "v5 total", "v5 target"
    );

    let scenarios: Vec<(&str, Chilling)> = vec![
        ("counterflow 2.08 L/min (default)", Chilling::counterflow()),
        ("counterflow 4 L/min", Chilling::Counterflow { litres_per_minute: 4.0 }),
        ("counterflow 8 L/min", Chilling::Counterflow { litres_per_minute: 8.0 }),
        ("immersion chiller", Chilling::immersion(22.5)),
        ("ice bath", Chilling::ice_bath(22.5)),
        ("none (left to cool)", Chilling::None),
    ];

    for (label, chilling) in scenarios {
        let eq = equipment(31.0, 31.0, chilling);
        let a = derive(&v4, &eq).expect("derives");
        let b = derive(&v5, &eq).expect("derives");
        println!(
            "{:<34}{:>12.1}{:>12.1}{:>12.1}{:>12.1}",
            label, a.ibu_total, 21.0, b.ibu_total, 19.2
        );
    }

    println!("\nboil-only component (must still match Appendix A):");
    let eq = equipment(31.0, 31.0, Chilling::counterflow());
    let a = derive(&v4, &eq).expect("derives");
    let b = derive(&v5, &eq).expect("derives");
    println!("  v4 {:.2} (sheet 19.43)   v5 {:.2} (sheet 17.12)", a.ibu_boil_only, b.ibu_boil_only);

    println!("\nsensitivity to the kettle geometry guess (counterflow default):");
    println!("{:<28}{:>12}{:>12}", "kettle diameter / opening", "v4 total", "v5 total");
    for (d, o) in [(25.0, 25.0), (31.0, 31.0), (38.0, 38.0), (31.0, 15.0), (31.0, 8.0)] {
        let eq = equipment(d, o, Chilling::counterflow());
        let a = derive(&v4, &eq).expect("derives");
        let b = derive(&v5, &eq).expect("derives");
        let label = if (d - o).abs() < 1e-9 {
            format!("{d:.0} cm, open")
        } else {
            format!("{d:.0} cm, opening {o:.0} cm")
        };
        println!("{label:<28}{:>12.1}{:>12.1}", a.ibu_total, b.ibu_total);
    }

    println!("\nper-addition split, counterflow @ 2.08 L/min:");
    println!("{:<18}{:>10}{:>16}{:>10}", "addition", "boil", "post-flameout", "total");
    for c in &a.ibu_contributions {
        println!(
            "  v4 {:>3.0} g @ {:>2}{:>10.2}{:>16.2}{:>10.2}",
            c.grams, c.minutes, c.boil_ibu, c.post_flameout_ibu, c.ibu
        );
    }
    for c in &b.ibu_contributions {
        println!(
            "  v5 {:>3.0} g @ {:>2}{:>10.2}{:>16.2}{:>10.2}",
            c.grams, c.minutes, c.boil_ibu, c.post_flameout_ibu, c.ibu
        );
    }
}
