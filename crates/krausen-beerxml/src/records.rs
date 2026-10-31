// SPDX-License-Identifier: AGPL-3.0-or-later

//! The BeerXML 1.0 record set, complete.
//!
//! Every field is modelled whether or not Krausen currently uses it. A recipe
//! with a step mash, a water profile and a fermentation schedule can carry
//! those fields in this structural representation. Native import retains the
//! original file separately and edited export reports omitted data.
//!
//! Units follow the standard: volumes in litres, weights in kilograms,
//! temperatures in Celsius, times in minutes, colour in SRM, and percentages
//! as numbers out of 100. Conversion into Krausen's own units happens in
//! [`crate::convert`], not here — this layer stays faithful to the file.
//!
//! Note the estimate fields (`EST_OG`, `IBU`, `CALORIES` and friends). Those
//! are *another program's* derived values. They are stored because they were
//! in the file, and because comparing them against Krausen's own numbers is
//! useful, but they are never treated as inputs. §3.6: every figure in this
//! application comes from the deterministic engine.

use crate::record;

record! {
    /// A hop addition, or a hop in inventory.
    pub struct Hop = "HOP" {
        name: String = "NAME",
        version: u32 = "VERSION",
        /// Alpha acid as a percentage, so 5.5 means 5.5%.
        alpha: f64 = "ALPHA",
        /// Weight in kilograms.
        amount: f64 = "AMOUNT",
        /// `Boil`, `Dry Hop`, `Mash`, `First Wort` or `Aroma`.
        use_: String = "USE",
        /// Minutes. Its meaning depends on `USE` — boil time, or days for a
        /// dry hop.
        time: f64 = "TIME",
        notes: String = "NOTES",
        /// `Bittering`, `Aroma` or `Both`.
        type_: String = "TYPE",
        /// `Pellet`, `Plug` or `Leaf`.
        form: String = "FORM",
        beta: f64 = "BETA",
        /// Hop stability index — percent alpha lost in six months of storage.
        hsi: f64 = "HSI",
        origin: String = "ORIGIN",
        substitutes: String = "SUBSTITUTES",
        humulene: f64 = "HUMULENE",
        caryophyllene: f64 = "CARYOPHYLLENE",
        cohumulone: f64 = "COHUMULONE",
        myrcene: f64 = "MYRCENE",
        inventory: String = "INVENTORY",
        display_amount: String = "DISPLAY_AMOUNT",
        display_time: String = "DISPLAY_TIME",
    }
}

record! {
    /// Anything that contributes extract: grain, extract, sugar or adjunct.
    pub struct Fermentable = "FERMENTABLE" {
        name: String = "NAME",
        version: u32 = "VERSION",
        /// `Grain`, `Sugar`, `Extract`, `Dry Extract` or `Adjunct`.
        type_: String = "TYPE",
        /// Weight in kilograms.
        amount: f64 = "AMOUNT",
        /// Dry yield as a percentage of sucrose. This is the extract figure —
        /// BeerXML does not carry PPG.
        yield_: f64 = "YIELD",
        /// Colour in SRM (Lovibond for grains, per the standard's wording).
        color: f64 = "COLOR",
        add_after_boil: bool = "ADD_AFTER_BOIL",
        origin: String = "ORIGIN",
        supplier: String = "SUPPLIER",
        notes: String = "NOTES",
        coarse_fine_diff: f64 = "COARSE_FINE_DIFF",
        moisture: f64 = "MOISTURE",
        diastatic_power: f64 = "DIASTATIC_POWER",
        protein: f64 = "PROTEIN",
        max_in_batch: f64 = "MAX_IN_BATCH",
        recommend_mash: bool = "RECOMMEND_MASH",
        ibu_gal_per_lb: f64 = "IBU_GAL_PER_LB",
        inventory: String = "INVENTORY",
        potential: f64 = "POTENTIAL",
        display_amount: String = "DISPLAY_AMOUNT",
        display_color: String = "DISPLAY_COLOR",
    }
}

record! {
    /// A yeast culture.
    pub struct Yeast = "YEAST" {
        name: String = "NAME",
        version: u32 = "VERSION",
        /// `Ale`, `Lager`, `Wheat`, `Wine` or `Champagne`.
        type_: String = "TYPE",
        /// `Liquid`, `Dry`, `Slant` or `Culture`.
        form: String = "FORM",
        /// Litres if `AMOUNT_IS_WEIGHT` is false, otherwise kilograms.
        amount: f64 = "AMOUNT",
        amount_is_weight: bool = "AMOUNT_IS_WEIGHT",
        laboratory: String = "LABORATORY",
        product_id: String = "PRODUCT_ID",
        min_temperature: f64 = "MIN_TEMPERATURE",
        max_temperature: f64 = "MAX_TEMPERATURE",
        /// `Low`, `Medium`, `High` or `Very High`.
        flocculation: String = "FLOCCULATION",
        /// Apparent attenuation as a percentage.
        attenuation: f64 = "ATTENUATION",
        notes: String = "NOTES",
        best_for: String = "BEST_FOR",
        times_cultured: u32 = "TIMES_CULTURED",
        max_reuse: u32 = "MAX_REUSE",
        add_to_secondary: bool = "ADD_TO_SECONDARY",
        inventory: String = "INVENTORY",
        culture_date: String = "CULTURE_DATE",
        display_amount: String = "DISPLAY_AMOUNT",
        disp_min_temp: String = "DISP_MIN_TEMP",
        disp_max_temp: String = "DISP_MAX_TEMP",
    }
}

record! {
    /// Spices, finings, water agents and anything else that is not a
    /// fermentable, hop or yeast.
    pub struct Misc = "MISC" {
        name: String = "NAME",
        version: u32 = "VERSION",
        /// `Spice`, `Fining`, `Water Agent`, `Herb`, `Flavor` or `Other`.
        type_: String = "TYPE",
        /// `Boil`, `Mash`, `Primary`, `Secondary` or `Bottling`.
        use_: String = "USE",
        /// Minutes, or days for a fermentation-stage addition.
        time: f64 = "TIME",
        /// Litres if `AMOUNT_IS_WEIGHT` is false, otherwise kilograms.
        amount: f64 = "AMOUNT",
        amount_is_weight: bool = "AMOUNT_IS_WEIGHT",
        use_for: String = "USE_FOR",
        notes: String = "NOTES",
        /// Batch size the amount was scaled for, in litres.
        batch_size: f64 = "BATCH_SIZE",
        inventory: String = "INVENTORY",
        display_amount: String = "DISPLAY_AMOUNT",
        display_time: String = "DISPLAY_TIME",
    }
}

record! {
    /// A water profile. Ion concentrations are in ppm.
    pub struct Water = "WATER" {
        name: String = "NAME",
        version: u32 = "VERSION",
        /// Litres.
        amount: f64 = "AMOUNT",
        calcium: f64 = "CALCIUM",
        bicarbonate: f64 = "BICARBONATE",
        sulfate: f64 = "SULFATE",
        chloride: f64 = "CHLORIDE",
        sodium: f64 = "SODIUM",
        magnesium: f64 = "MAGNESIUM",
        ph: f64 = "PH",
        notes: String = "NOTES",
        display_amount: String = "DISPLAY_AMOUNT",
    }
}

record! {
    /// A style definition, for judging a recipe against a guideline.
    pub struct Style = "STYLE" {
        name: String = "NAME",
        version: u32 = "VERSION",
        category: String = "CATEGORY",
        category_number: String = "CATEGORY_NUMBER",
        style_letter: String = "STYLE_LETTER",
        style_guide: String = "STYLE_GUIDE",
        /// `Lager`, `Ale`, `Mead`, `Wheat`, `Mixed` or `Cider`.
        type_: String = "TYPE",
        og_min: f64 = "OG_MIN",
        og_max: f64 = "OG_MAX",
        fg_min: f64 = "FG_MIN",
        fg_max: f64 = "FG_MAX",
        ibu_min: f64 = "IBU_MIN",
        ibu_max: f64 = "IBU_MAX",
        color_min: f64 = "COLOR_MIN",
        color_max: f64 = "COLOR_MAX",
        carb_min: f64 = "CARB_MIN",
        carb_max: f64 = "CARB_MAX",
        abv_min: f64 = "ABV_MIN",
        abv_max: f64 = "ABV_MAX",
        notes: String = "NOTES",
        profile: String = "PROFILE",
        ingredients: String = "INGREDIENTS",
        examples: String = "EXAMPLES",
        display_og_min: String = "DISPLAY_OG_MIN",
        display_og_max: String = "DISPLAY_OG_MAX",
        display_fg_min: String = "DISPLAY_FG_MIN",
        display_fg_max: String = "DISPLAY_FG_MAX",
        display_color_min: String = "DISPLAY_COLOR_MIN",
        display_color_max: String = "DISPLAY_COLOR_MAX",
        og_range: String = "OG_RANGE",
        fg_range: String = "FG_RANGE",
        ibu_range: String = "IBU_RANGE",
        carb_range: String = "CARB_RANGE",
        color_range: String = "COLOR_RANGE",
        abv_range: String = "ABV_RANGE",
    }
}

record! {
    /// A brewery's equipment, as BeerXML describes it.
    ///
    /// Notably absent: grain absorption and any kettle dimensions. An
    /// equipment profile imported from BeerXML is therefore genuinely partial,
    /// which is what the measured-versus-estimated split in
    /// `krausen_calc::EquipmentPreset` exists to express.
    pub struct Equipment = "EQUIPMENT" {
        name: String = "NAME",
        version: u32 = "VERSION",
        /// Litres into the kettle at the start of the boil.
        boil_size: f64 = "BOIL_SIZE",
        /// Litres into the fermenter.
        batch_size: f64 = "BATCH_SIZE",
        tun_volume: f64 = "TUN_VOLUME",
        tun_weight: f64 = "TUN_WEIGHT",
        tun_specific_heat: f64 = "TUN_SPECIFIC_HEAT",
        top_up_water: f64 = "TOP_UP_WATER",
        trub_chiller_loss: f64 = "TRUB_CHILLER_LOSS",
        /// Percent of the boil volume lost per hour — not litres per hour.
        evap_rate: f64 = "EVAP_RATE",
        boil_time: f64 = "BOIL_TIME",
        calc_boil_volume: bool = "CALC_BOIL_VOLUME",
        lauter_deadspace: f64 = "LAUTER_DEADSPACE",
        top_up_kettle: f64 = "TOP_UP_KETTLE",
        /// An exporter's own bitterness fudge factor, as a percentage.
        hop_utilization: f64 = "HOP_UTILIZATION",
        notes: String = "NOTES",
        display_boil_size: String = "DISPLAY_BOIL_SIZE",
        display_batch_size: String = "DISPLAY_BATCH_SIZE",
        display_tun_volume: String = "DISPLAY_TUN_VOLUME",
        display_tun_weight: String = "DISPLAY_TUN_WEIGHT",
        display_top_up_water: String = "DISPLAY_TOP_UP_WATER",
        display_trub_chiller_loss: String = "DISPLAY_TRUB_CHILLER_LOSS",
        display_lauter_deadspace: String = "DISPLAY_LAUTER_DEADSPACE",
        display_top_up_kettle: String = "DISPLAY_TOP_UP_KETTLE",
    }
}

record! {
    /// One step in a mash schedule.
    pub struct MashStep = "MASH_STEP" {
        name: String = "NAME",
        version: u32 = "VERSION",
        /// `Infusion`, `Temperature` or `Decoction`.
        type_: String = "TYPE",
        /// Litres of water infused at this step.
        infuse_amount: f64 = "INFUSE_AMOUNT",
        /// Target temperature in Celsius.
        step_temp: f64 = "STEP_TEMP",
        /// Minutes held at `STEP_TEMP`.
        step_time: f64 = "STEP_TIME",
        /// Minutes taken to reach `STEP_TEMP`.
        ramp_time: f64 = "RAMP_TIME",
        end_temp: f64 = "END_TEMP",
        description: String = "DESCRIPTION",
        water_grain_ratio: String = "WATER_GRAIN_RATIO",
        decoction_amt: String = "DECOCTION_AMT",
        infuse_temp: String = "INFUSE_TEMP",
        display_step_temp: String = "DISPLAY_STEP_TEMP",
        display_infuse_amt: String = "DISPLAY_INFUSE_AMT",
    }
}

record! {
    /// The scalar half of a mash profile. Steps live alongside in
    /// [`crate::Mash`].
    pub struct MashHeader = "MASH" {
        name: String = "NAME",
        version: u32 = "VERSION",
        /// Temperature of the grain before mashing in, in Celsius.
        grain_temp: f64 = "GRAIN_TEMP",
        notes: String = "NOTES",
        tun_temp: f64 = "TUN_TEMP",
        sparge_temp: f64 = "SPARGE_TEMP",
        ph: f64 = "PH",
        tun_weight: f64 = "TUN_WEIGHT",
        tun_specific_heat: f64 = "TUN_SPECIFIC_HEAT",
        equip_adjust: bool = "EQUIP_ADJUST",
        display_grain_temp: String = "DISPLAY_GRAIN_TEMP",
        display_tun_temp: String = "DISPLAY_TUN_TEMP",
        display_sparge_temp: String = "DISPLAY_SPARGE_TEMP",
        display_tun_weight: String = "DISPLAY_TUN_WEIGHT",
    }
}

record! {
    /// The scalar half of a recipe. Its ingredients and sub-records live
    /// alongside in [`crate::Recipe`].
    pub struct RecipeHeader = "RECIPE" {
        name: String = "NAME",
        version: u32 = "VERSION",
        /// `Extract`, `Partial Mash` or `All Grain`.
        type_: String = "TYPE",
        brewer: String = "BREWER",
        asst_brewer: String = "ASST_BREWER",
        /// Litres into the fermenter.
        batch_size: f64 = "BATCH_SIZE",
        /// Litres into the kettle at the start of the boil.
        boil_size: f64 = "BOIL_SIZE",
        /// Minutes.
        boil_time: f64 = "BOIL_TIME",
        /// Percent.
        efficiency: f64 = "EFFICIENCY",
        notes: String = "NOTES",
        taste_notes: String = "TASTE_NOTES",
        taste_rating: f64 = "TASTE_RATING",
        og: f64 = "OG",
        fg: f64 = "FG",
        fermentation_stages: u32 = "FERMENTATION_STAGES",
        primary_age: f64 = "PRIMARY_AGE",
        primary_temp: f64 = "PRIMARY_TEMP",
        secondary_age: f64 = "SECONDARY_AGE",
        secondary_temp: f64 = "SECONDARY_TEMP",
        tertiary_age: f64 = "TERTIARY_AGE",
        tertiary_temp: f64 = "TERTIARY_TEMP",
        age: f64 = "AGE",
        age_temp: f64 = "AGE_TEMP",
        date: String = "DATE",
        carbonation: f64 = "CARBONATION",
        forced_carbonation: bool = "FORCED_CARBONATION",
        priming_sugar_name: String = "PRIMING_SUGAR_NAME",
        carbonation_temp: f64 = "CARBONATION_TEMP",
        priming_sugar_equiv: f64 = "PRIMING_SUGAR_EQUIV",
        keg_priming_factor: f64 = "KEG_PRIMING_FACTOR",
        // ── Another program's derived values ──────────────────────────
        // Stored because they were in the file and because comparing them
        // against Krausen's own output is useful. Never used as inputs.
        est_og: String = "EST_OG",
        est_fg: String = "EST_FG",
        est_color: String = "EST_COLOR",
        ibu: f64 = "IBU",
        ibu_method: String = "IBU_METHOD",
        est_abv: f64 = "EST_ABV",
        abv: f64 = "ABV",
        actual_efficiency: f64 = "ACTUAL_EFFICIENCY",
        calories: String = "CALORIES",
        carbonation_used: String = "CARBONATION_USED",
        display_batch_size: String = "DISPLAY_BATCH_SIZE",
        display_boil_size: String = "DISPLAY_BOIL_SIZE",
        display_og: String = "DISPLAY_OG",
        display_fg: String = "DISPLAY_FG",
        display_primary_temp: String = "DISPLAY_PRIMARY_TEMP",
        display_secondary_temp: String = "DISPLAY_SECONDARY_TEMP",
        display_tertiary_temp: String = "DISPLAY_TERTIARY_TEMP",
        display_age_temp: String = "DISPLAY_AGE_TEMP",
    }
}
