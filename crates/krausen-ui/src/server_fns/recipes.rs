// SPDX-License-Identifier: AGPL-3.0-or-later

//! Server functions for recipes.
//!
//! Thin wrappers: extract the caller's workspace, call the service, return.
//! All the storage reasoning lives in [`krausen_brewery::recipe_service`].
//!
//! Recipes cross the wire as BeerXML rather than as a serialised internal
//! model. That keeps one representation instead of two, means the browser can
//! hand a file straight to the server and get one straight back, and stops the
//! wire format quietly becoming a second schema that has to be kept in step.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use super::AuthenticatedContext;

/// A recipe in a list, with the numbers Krausen derives for it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecipeListItem {
    pub recipe_id: String,
    pub name: String,
    pub style_name: Option<String>,
    pub batch_size_l: Option<f64>,
    pub boil_minutes: Option<i64>,
    /// `"krausen"` or `"beerxml"`.
    pub source: String,
    pub updated_at: String,
}

/// One recipe, as BeerXML, together with what the engine makes of it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecipeDetail {
    pub recipe_id: String,
    pub name: String,
    pub source: String,
    /// The complete recipe. Everything the file had, including what Krausen
    /// does not model.
    pub beerxml: String,
    /// What the engine derives, or the reason it could not.
    pub derived: Option<DerivedSummary>,
    /// What conversion could not do faithfully — §3.5 wants these visible.
    pub warnings: Vec<String>,
}

/// The headline figures, flattened for display.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DerivedSummary {
    pub original_gravity: f64,
    pub final_gravity: f64,
    pub pre_boil_gravity: f64,
    pub abv_percent: f64,
    pub colour_ebc: f64,
    pub ibu_total: f64,
    /// What boil-only Tinseth would have said, for comparison with software
    /// that stops at flameout.
    pub ibu_boil_only: f64,
    pub pre_boil_l: f64,
    pub post_boil_l: f64,
    pub mash_water_l: f64,
    pub sparge_water_l: f64,
}

#[cfg(feature = "ssr")]
fn describe(warning: &krausen_beerxml::convert::Warning) -> String {
    use krausen_beerxml::convert::Warning;
    match warning {
        Warning::UnmodelledHopUse { hop, use_ } => {
            format!("{hop}: “{use_}” additions are not modelled and were treated as boil additions")
        }
        Warning::MissingField { field, assumed } => {
            format!("{field} was missing; assumed {assumed}")
        }
        Warning::UnreadableValue { field, raw } => {
            format!("{field} could not be read as a number: “{raw}”")
        }
        Warning::NoWhirlpoolData => {
            "BeerXML carries no whirlpool time or temperature, so no hop stand is modelled".into()
        }
        Warning::EquipmentIncomplete { missing } => {
            format!("the equipment profile has no {}", missing.join(", "))
        }
        Warning::ColourUnitAssumed => "malt colour was assumed to be in Lovibond".into(),
    }
}

/// Derive a stored recipe, reporting rather than hiding what went wrong.
#[cfg(feature = "ssr")]
fn summarise(document: &krausen_beerxml::Recipe) -> (Option<DerivedSummary>, Vec<String>) {
    let converted = krausen_beerxml::convert::to_calc(document);
    let mut warnings: Vec<String> = converted.warnings.iter().map(describe).collect();

    let equipment = converted.equipment.clone().unwrap_or_default();
    match krausen_calc::derive(&converted.recipe, &equipment) {
        Ok(d) => (
            Some(DerivedSummary {
                original_gravity: d.original_gravity.sg(),
                final_gravity: d.final_gravity.sg(),
                pre_boil_gravity: d.pre_boil_gravity.sg(),
                abv_percent: d.abv_percent,
                colour_ebc: d.colour_ebc,
                ibu_total: d.ibu_total,
                ibu_boil_only: d.ibu_boil_only,
                pre_boil_l: d.volumes.pre_boil_l,
                post_boil_l: d.volumes.post_boil_l,
                mash_water_l: d.volumes.mash_water_l,
                sparge_water_l: d.volumes.sparge_water_l,
            }),
            warnings,
        ),
        Err(e) => {
            // A recipe that cannot be derived is still a recipe worth keeping
            // and showing. Say why rather than showing nothing.
            warnings.push(format!("could not derive: {e}"));
            (None, warnings)
        }
    }
}

#[cfg(feature = "ssr")]
fn parse_one(beerxml: &str) -> Result<krausen_beerxml::Recipe, ServerFnError> {
    let mut doc = krausen_beerxml::parse(beerxml)
        .map_err(|e| ServerFnError::new(format!("could not read that BeerXML: {e}")))?;
    if doc.recipes.is_empty() {
        return Err(ServerFnError::new("that BeerXML contains no recipes"));
    }
    Ok(doc.recipes.remove(0))
}

#[cfg(feature = "ssr")]
fn detail(stored: krausen_brewery::Recipe) -> RecipeDetail {
    let (derived, mut warnings) = summarise(&stored.document);
    if let Some(error) = &stored.native_migration_error {
        warnings.push(format!("Native migration failed: {error}. This legacy record is read-only."));
    }
    RecipeDetail {
        recipe_id: stored.recipe_id,
        name: stored.name,
        source: stored.source.as_str().to_string(),
        beerxml: krausen_beerxml::write_recipe_document(&stored.document),
        derived,
        warnings,
    }
}


// ─────────────────────────────────────────────────────────────────────────
// Read
// ─────────────────────────────────────────────────────────────────────────

/// Every recipe in the caller's workspace.
#[server(prefix = "/leptos-api")]
pub async fn list_recipes() -> Result<Vec<RecipeListItem>, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;

    let rows = krausen_brewery::recipe_service::list_recipes(&ac.ctx.db, &ac.ws_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "list_recipes failed");
            ServerFnError::new("Could not list recipes")
        })?;

    Ok(rows
        .into_iter()
        .map(|r| RecipeListItem {
            recipe_id: r.recipe_id,
            name: r.name,
            style_name: r.style_name,
            batch_size_l: r.batch_size_l,
            boil_minutes: r.boil_minutes,
            source: r.source.as_str().to_string(),
            updated_at: r.updated_at.to_rfc3339(),
        })
        .collect())
}

/// One recipe, with its derived figures.
#[server(prefix = "/leptos-api")]
pub async fn get_recipe(recipe_id: String) -> Result<Option<RecipeDetail>, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;

    let stored = krausen_brewery::recipe_service::get_recipe(&ac.ctx.db, &ac.ws_id, &recipe_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "get_recipe failed");
            match e {
                krausen_core::Error::Conflict(message) => ServerFnError::new(message),
                _ => ServerFnError::new("Could not load that recipe"),
            }
        })?;

    Ok(stored.map(detail))
}

/// Search recipes by name.
#[server(prefix = "/leptos-api")]
pub async fn search_recipes(query: String) -> Result<Vec<RecipeListItem>, ServerFnError> {
    if query.trim().is_empty() {
        return list_recipes().await;
    }
    let ac = AuthenticatedContext::extract().await?;

    let rows = krausen_brewery::recipe_service::search_recipes(&ac.ctx.db, &ac.ws_id, query.trim())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "search_recipes failed");
            ServerFnError::new("Could not search recipes")
        })?;

    Ok(rows
        .into_iter()
        .map(|r| RecipeListItem {
            recipe_id: r.recipe_id,
            name: r.name,
            style_name: r.style_name,
            batch_size_l: r.batch_size_l,
            boil_minutes: r.boil_minutes,
            source: r.source.as_str().to_string(),
            updated_at: r.updated_at.to_rfc3339(),
        })
        .collect())
}

// ─────────────────────────────────────────────────────────────────────────
// Write
// ─────────────────────────────────────────────────────────────────────────

/// Store a recipe given as BeerXML.
#[server(prefix = "/leptos-api")]
pub async fn create_recipe(beerxml: String) -> Result<RecipeDetail, ServerFnError> {
    let _ = AuthenticatedContext::extract().await?;
    let _ = beerxml;
    Err(ServerFnError::new("Recipe creation is unavailable while native authoring is being introduced"))
}

/// Import a BeerXML file, which may hold several recipes.
///
/// Returns what was stored, so the caller can say how many arrived.
#[server(prefix = "/leptos-api")]
pub async fn import_beerxml(beerxml: String) -> Result<Vec<RecipeListItem>, ServerFnError> {
    let _ = AuthenticatedContext::extract().await?;
    let _ = beerxml;
    Err(ServerFnError::new("Recipe import is unavailable while native authoring is being introduced"))
}

/// Replace a recipe with the BeerXML given.
#[server(prefix = "/leptos-api")]
pub async fn update_recipe(
    recipe_id: String,
    beerxml: String,
) -> Result<Option<RecipeDetail>, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let document = parse_one(&beerxml)?;

    let stored =
        krausen_brewery::recipe_service::update_recipe(&ac.ctx.db, &ac.ws_id, &recipe_id, &document)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "update_recipe failed");
                match e {
                krausen_core::Error::Conflict(message) => ServerFnError::new(message),
                _ => ServerFnError::new("Could not save that recipe"),
            }
            })?;

    Ok(stored.map(detail))
}

/// Delete a recipe. Recoverable — see [`restore_recipe`].
#[server(prefix = "/leptos-api")]
pub async fn delete_recipe(recipe_id: String) -> Result<bool, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;

    krausen_brewery::recipe_service::delete_recipe(&ac.ctx.db, &ac.ws_id, &recipe_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "delete_recipe failed");
            match e {
                krausen_core::Error::Conflict(message) => ServerFnError::new(message),
                _ => ServerFnError::new("Could not delete that recipe"),
            }
        })
}

/// Undo a delete.
#[server(prefix = "/leptos-api")]
pub async fn restore_recipe(recipe_id: String) -> Result<bool, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;

    krausen_brewery::recipe_service::restore_recipe(&ac.ctx.db, &ac.ws_id, &recipe_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "restore_recipe failed");
            match e {
                krausen_core::Error::Conflict(message) => ServerFnError::new(message),
                _ => ServerFnError::new("Could not restore that recipe"),
            }
        })
}
