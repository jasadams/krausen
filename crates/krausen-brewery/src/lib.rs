// SPDX-License-Identifier: AGPL-3.0-or-later

//! Krausen's domain services.
//!
//! Everything that stores and retrieves brewing data. Sits above the pure
//! crates — [`krausen_calc`] derives, [`krausen_beerxml`] interchanges — and
//! below the web layer, which only extracts a request and calls in here.

pub mod recipe_service;
pub mod native_recipe;
pub mod native_store;

pub use recipe_service::{Recipe, RecipeSource, RecipeSummary};

pub mod validation;

pub mod mapping;

pub mod equipment_store;
pub mod catalogue_store;
