// SPDX-License-Identifier: AGPL-3.0-or-later

//! The composite records — the ones that contain other records.

use crate::{
    raw::RawElement,
    records::{
        Equipment, Fermentable, Hop, MashHeader, MashStep, Misc, RecipeHeader, Style, Water, Yeast,
    },
};

/// A mash profile: its own fields, plus an ordered list of steps.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Mash {
    pub header: MashHeader,
    pub steps: Vec<MashStep>,
}

/// A complete recipe.
///
/// The scalar fields are on [`Self::header`]; everything structured hangs off
/// this type. Collections keep their file order, because a hop schedule read
/// back in a different order is a different bench sheet.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Recipe {
    pub header: RecipeHeader,
    pub style: Option<Style>,
    pub equipment: Option<Equipment>,
    pub mash: Option<Mash>,
    pub hops: Vec<Hop>,
    pub fermentables: Vec<Fermentable>,
    pub yeasts: Vec<Yeast>,
    pub miscs: Vec<Misc>,
    pub waters: Vec<Water>,
}

impl Recipe {
    /// The recipe's name, or an empty string.
    pub fn name(&self) -> &str {
        self.header.name().map_or("", String::as_str)
    }
}

/// A BeerXML document: the `<RECIPES>` element and everything in it.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BeerXml {
    pub recipes: Vec<Recipe>,
    /// Anything at the top level that is not a `RECIPE`.
    pub extensions: Vec<RawElement>,
}

impl BeerXml {
    /// A document holding a single recipe.
    pub fn from_recipe(recipe: Recipe) -> Self {
        Self {
            recipes: vec![recipe],
            extensions: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }
}
