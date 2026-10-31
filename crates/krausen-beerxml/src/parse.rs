// SPDX-License-Identifier: AGPL-3.0-or-later

//! Reading a BeerXML document.
//!
//! Deliberately permissive about structure and strict about data. Real
//! exports vary — unexpected elements, unexpected nesting, elements in a
//! different order — and none of that is a reason to refuse a brewer's
//! recipe. What is never done is discarding something: anything unrecognised
//! becomes a [`RawElement`] and comes back out on export.

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::{
    document::{BeerXml, Mash, Recipe},
    error::ParseError,
    raw::RawElement,
    record::Record,
    records::{Equipment, Fermentable, Hop, MashStep, Misc, Style, Water, Yeast},
};

/// Parse a BeerXML document.
///
/// # Errors
///
/// Returns [`ParseError`] only for XML that cannot be read at all. A document
/// whose *contents* are surprising parses successfully — the surprises are
/// preserved rather than rejected.
pub fn parse(xml: &str) -> Result<BeerXml, ParseError> {
    let tree = read_tree(xml)?;

    // The root is normally RECIPES, but a single-recipe document with RECIPE
    // at the root is common enough in the wild to accept.
    let mut doc = BeerXml::default();
    match tree.name.as_str() {
        "RECIPE" => doc.recipes.push(recipe_from(&tree)),
        _ => {
            for child in &tree.children {
                if child.name == "RECIPE" {
                    doc.recipes.push(recipe_from(child));
                } else {
                    doc.extensions.push(child.clone());
                }
            }
        }
    }
    Ok(doc)
}

/// Read the whole document into a generic element tree.
///
/// Mapping onto typed records happens afterwards, which keeps the XML handling
/// in one place and makes preserving unknown elements a matter of not
/// consuming them rather than a special case at every level.
fn read_tree(xml: &str) -> Result<RawElement, ParseError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut stack: Vec<RawElement> = Vec::new();
    let mut root: Option<RawElement> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => {
                return Err(ParseError::Xml {
                    position: reader.buffer_position(),
                    message: e.to_string(),
                });
            }
            Ok(Event::Eof) => break,

            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                stack.push(RawElement {
                    name,
                    text: None,
                    children: Vec::new(),
                });
            }

            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let element = RawElement {
                    name,
                    text: Some(String::new()),
                    children: Vec::new(),
                };
                match stack.last_mut() {
                    Some(parent) => parent.children.push(element),
                    None => root = Some(element),
                }
            }

            Ok(Event::Text(e)) => {
                if let Some(current) = stack.last_mut() {
                    let text = e.unescape().map(|t| t.into_owned()).unwrap_or_default();
                    if !text.is_empty() {
                        current.text = Some(match current.text.take() {
                            Some(existing) => existing + &text,
                            None => text,
                        });
                    }
                }
            }

            Ok(Event::CData(e)) => {
                if let Some(current) = stack.last_mut() {
                    let text = String::from_utf8_lossy(&e).into_owned();
                    current.text = Some(match current.text.take() {
                        Some(existing) => existing + &text,
                        None => text,
                    });
                }
            }

            Ok(Event::End(_)) => {
                if let Some(finished) = stack.pop() {
                    match stack.last_mut() {
                        Some(parent) => parent.children.push(finished),
                        None => root = Some(finished),
                    }
                }
            }

            _ => {}
        }
        buf.clear();
    }

    root.ok_or(ParseError::NoRootElement)
}

/// Fill a record's scalar fields from an element's children, preserving any
/// child it does not recognise.
fn fill<R: Record>(element: &RawElement) -> R {
    let mut record = R::default();
    for child in &element.children {
        let handled = child.children.is_empty()
            && record.set_scalar(&child.name, child.text.clone().unwrap_or_default());
        if !handled {
            record.push_extension(child.clone());
        }
    }
    record
}

/// Collect a list of records from a wrapper element such as `<HOPS>`.
fn collect<R: Record>(element: &RawElement) -> Vec<R> {
    element
        .children
        .iter()
        .filter(|c| c.name == R::TAG)
        .map(fill)
        .collect()
}

/// Children of a wrapper that are not the record type it should contain.
fn strays<R: Record>(element: &RawElement) -> impl Iterator<Item = &RawElement> {
    element.children.iter().filter(|c| c.name != R::TAG)
}

fn mash_from(element: &RawElement) -> Mash {
    let mut mash = Mash {
        header: crate::records::MashHeader::default(),
        steps: Vec::new(),
    };

    for child in &element.children {
        if child.name == "MASH_STEPS" {
            mash.steps = collect::<MashStep>(child);
            for stray in strays::<MashStep>(child) {
                mash.header.push_extension(stray.clone());
            }
        } else {
            let handled = child.children.is_empty()
                && mash
                    .header
                    .set_scalar(&child.name, child.text.clone().unwrap_or_default());
            if !handled {
                mash.header.push_extension(child.clone());
            }
        }
    }
    mash
}

fn recipe_from(element: &RawElement) -> Recipe {
    let mut recipe = Recipe::default();

    for child in &element.children {
        match child.name.as_str() {
            "STYLE" => recipe.style = Some(fill::<Style>(child)),
            "EQUIPMENT" => recipe.equipment = Some(fill::<Equipment>(child)),
            "MASH" => recipe.mash = Some(mash_from(child)),
            "HOPS" => {
                recipe.hops = collect::<Hop>(child);
                keep_strays::<Hop>(&mut recipe, child);
            }
            "FERMENTABLES" => {
                recipe.fermentables = collect::<Fermentable>(child);
                keep_strays::<Fermentable>(&mut recipe, child);
            }
            "YEASTS" => {
                recipe.yeasts = collect::<Yeast>(child);
                keep_strays::<Yeast>(&mut recipe, child);
            }
            "MISCS" => {
                recipe.miscs = collect::<Misc>(child);
                keep_strays::<Misc>(&mut recipe, child);
            }
            "WATERS" => {
                recipe.waters = collect::<Water>(child);
                keep_strays::<Water>(&mut recipe, child);
            }
            _ => {
                let handled = child.children.is_empty()
                    && recipe
                        .header
                        .set_scalar(&child.name, child.text.clone().unwrap_or_default());
                if !handled {
                    recipe.header.push_extension(child.clone());
                }
            }
        }
    }
    recipe
}

/// Preserve anything inside a collection wrapper that is not the expected
/// record type — an exporter's own element, say.
fn keep_strays<R: Record>(recipe: &mut Recipe, wrapper: &RawElement) {
    for stray in strays::<R>(wrapper) {
        recipe.header.push_extension(RawElement {
            name: wrapper.name.clone(),
            text: None,
            children: vec![stray.clone()],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_minimal_recipe() {
        let doc = parse(
            r#"<RECIPES><RECIPE>
                 <NAME>Test Ale</NAME>
                 <VERSION>1</VERSION>
                 <BATCH_SIZE>20.0</BATCH_SIZE>
               </RECIPE></RECIPES>"#,
        )
        .expect("parses");

        assert_eq!(doc.len(), 1);
        assert_eq!(doc.recipes[0].name(), "Test Ale");
        assert_eq!(doc.recipes[0].header.batch_size().copied(), Some(20.0));
    }

    #[test]
    fn accepts_a_bare_recipe_without_the_recipes_wrapper() {
        let doc = parse("<RECIPE><NAME>Solo</NAME></RECIPE>").expect("parses");
        assert_eq!(doc.len(), 1);
        assert_eq!(doc.recipes[0].name(), "Solo");
    }

    #[test]
    fn keeps_an_unknown_element_instead_of_dropping_it() {
        let doc = parse(
            r#"<RECIPES><RECIPE>
                 <NAME>Test</NAME>
                 <GRAINFATHER_RECIPE_ID>abc-123</GRAINFATHER_RECIPE_ID>
               </RECIPE></RECIPES>"#,
        )
        .expect("parses");

        let ext = &doc.recipes[0].header.extensions;
        assert_eq!(ext.len(), 1);
        assert_eq!(ext[0].name, "GRAINFATHER_RECIPE_ID");
        assert_eq!(ext[0].text.as_deref(), Some("abc-123"));
    }

    #[test]
    fn keeps_a_structured_unknown_element() {
        let doc = parse(
            r#"<RECIPES><RECIPE>
                 <NAME>Test</NAME>
                 <VENDOR_BLOCK><A>1</A><B>2</B></VENDOR_BLOCK>
               </RECIPE></RECIPES>"#,
        )
        .expect("parses");

        let ext = &doc.recipes[0].header.extensions;
        assert_eq!(ext[0].name, "VENDOR_BLOCK");
        assert_eq!(ext[0].children.len(), 2);
    }

    #[test]
    fn collections_keep_their_order() {
        // A hop schedule read back in a different order is a different sheet.
        let doc = parse(
            r#"<RECIPES><RECIPE><HOPS>
                 <HOP><NAME>First</NAME><TIME>60.0</TIME></HOP>
                 <HOP><NAME>Second</NAME><TIME>15.0</TIME></HOP>
                 <HOP><NAME>Third</NAME><TIME>0.0</TIME></HOP>
               </HOPS></RECIPE></RECIPES>"#,
        )
        .expect("parses");

        let names: Vec<&str> = doc.recipes[0]
            .hops
            .iter()
            .map(|h| h.name().map_or("", String::as_str))
            .collect();
        assert_eq!(names, vec!["First", "Second", "Third"]);
    }

    #[test]
    fn reads_a_mash_with_its_steps() {
        let doc = parse(
            r#"<RECIPES><RECIPE><MASH>
                 <NAME>Single Infusion</NAME>
                 <GRAIN_TEMP>22.0</GRAIN_TEMP>
                 <MASH_STEPS>
                   <MASH_STEP><NAME>Sacch</NAME><STEP_TEMP>67.0</STEP_TEMP><STEP_TIME>60.0</STEP_TIME></MASH_STEP>
                   <MASH_STEP><NAME>Mash Out</NAME><STEP_TEMP>75.0</STEP_TEMP><STEP_TIME>10.0</STEP_TIME></MASH_STEP>
                 </MASH_STEPS>
               </MASH></RECIPE></RECIPES>"#,
        )
        .expect("parses");

        let mash = doc.recipes[0].mash.as_ref().expect("has a mash");
        assert_eq!(mash.header.name().map(String::as_str), Some("Single Infusion"));
        assert_eq!(mash.steps.len(), 2);
        assert_eq!(mash.steps[1].step_temp().copied(), Some(75.0));
    }

    #[test]
    fn an_empty_element_is_recorded_as_empty_not_absent() {
        let doc = parse("<RECIPES><RECIPE><ASST_BREWER/></RECIPE></RECIPES>").expect("parses");
        assert_eq!(
            doc.recipes[0].header.asst_brewer().map(String::as_str),
            Some("")
        );
    }

    #[test]
    fn malformed_xml_is_an_error_rather_than_a_guess() {
        assert!(parse("<RECIPES><RECIPE></RECIPES>").is_err());
        assert!(parse("").is_err());
    }

    #[test]
    fn cdata_survives() {
        let doc = parse(
            "<RECIPES><RECIPE><NOTES><![CDATA[Mind the < and & here]]></NOTES></RECIPE></RECIPES>",
        )
        .expect("parses");
        assert_eq!(
            doc.recipes[0].header.notes().map(String::as_str),
            Some("Mind the < and & here")
        );
    }
}
