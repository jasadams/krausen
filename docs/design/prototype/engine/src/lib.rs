// SPDX-License-Identifier: AGPL-3.0-or-later
//! Browser bridge for the design prototype. All brewing figures come from the
//! repository's existing pure engine, rather than a second JavaScript calculator.
use krausen_calc::{Equipment, Recipe, derive};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

#[derive(Deserialize)]
struct Input {
    recipe: Recipe,
    equipment: Equipment,
}

#[wasm_bindgen]
pub fn calculate(input: &str) -> Result<String, JsValue> {
    let input: Input = serde_json::from_str(input)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let result = derive(&input.recipe, &input.equipment)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    serde_json::to_string(&serde_json::json!({
        "og": result.original_gravity.sg(),
        "fg": result.final_gravity.sg(),
        "abv": result.abv_percent,
        "ibu": result.ibu_total,
        "ebc": result.colour_ebc,
        "volumes": result.volumes,
        "hops": result.ibu_contributions,
    })).map_err(|error| JsValue::from_str(&error.to_string()))
}
