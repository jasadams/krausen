// SPDX-License-Identifier: AGPL-3.0-or-later

//! BeerXML import and export.
//!
//! Recipes come in from other brewing software and go back out to it, so this
//! crate reads and writes BeerXML 1.0 in full.
//!
//! # Nothing is thrown away
//!
//! The whole record set is modelled — mash schedules, water profiles, styles,
//! fermentation stages — whether or not Krausen currently uses any of it, and
//! anything the standard does not cover is preserved verbatim as a
//! [`RawElement`]. Importing a recipe and exporting it again is therefore not
//! a way to lose data a brewer already had.
//!
//! That matters beyond tidiness. §4.6 requires a brew to embed the recipe it
//! was brewed from, so an old brew can still render its own bench sheet. A
//! lossy import would make those snapshots quietly incomplete.
//!
//! # This layer stays faithful to the file
//!
//! Records here hold BeerXML's units and conventions, not Krausen's — weights
//! in kilograms, colour in SRM, extract as a percentage yield rather than PPG.
//! Conversion into the engine's input types happens in [`convert`], at the
//! boundary, so the interchange model never drifts from what the file said.
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let doc = krausen_beerxml::parse(&std::fs::read_to_string("recipes.xml")?)?;
//! for recipe in &doc.recipes {
//!     println!("{}", recipe.name());
//! }
//! std::fs::write("out.xml", krausen_beerxml::write(&doc))?;
//! # Ok(())
//! # }
//! ```

pub mod convert;
mod document;
mod error;
pub mod native;
mod parse;
pub mod raw;
pub mod record;
pub mod records;
mod write;

pub use document::{BeerXml, Mash, Recipe};
pub use error::ParseError;
pub use parse::parse;
pub use raw::RawElement;
pub use records::{
    Equipment, Fermentable, Hop, MashHeader, MashStep, Misc, RecipeHeader, Style, Water, Yeast,
};
pub use write::{write, write_recipe_document};
