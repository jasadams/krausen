// SPDX-License-Identifier: AGPL-3.0-or-later

//! Writing a BeerXML document.
//!
//! Known fields are written in declaration order, followed by structural
//! extensions. This is not byte-preserving; original source and export-loss
//! diagnostics are handled by the native interchange layer.

use std::fmt::Write as _;

use crate::{
    document::{BeerXml, Mash, Recipe},
    raw::RawElement,
    record::Record,
};

/// Two spaces per level, which is what most exporters produce.
const INDENT: &str = "  ";

/// Serialise a document to BeerXML.
pub fn write(doc: &BeerXml) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<RECIPES>\n");
    for recipe in &doc.recipes {
        write_recipe(&mut out, recipe, 1);
    }
    for extension in &doc.extensions {
        write_raw(&mut out, extension, 1);
    }
    out.push_str("</RECIPES>\n");
    out
}

/// Serialise a single recipe as a complete document.
pub fn write_recipe_document(recipe: &Recipe) -> String {
    write(&BeerXml::from_recipe(recipe.clone()))
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str(INDENT);
    }
}

/// Write a leaf element, escaping the text.
fn leaf(out: &mut String, depth: usize, tag: &str, text: &str) {
    indent(out, depth);
    let _ = writeln!(out, "<{tag}>{}</{tag}>", escape(text));
}

/// XML-escape text content.
///
/// Recipe notes routinely contain `&` and `<`, and a brewer's tasting note is
/// not worth producing a malformed document over.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

fn write_raw(out: &mut String, element: &RawElement, depth: usize) {
    if element.children.is_empty() {
        leaf(
            out,
            depth,
            &element.name,
            element.text.as_deref().unwrap_or(""),
        );
        return;
    }

    indent(out, depth);
    let _ = writeln!(out, "<{}>", element.name);
    if let Some(text) = &element.text
        && !text.is_empty()
    {
        indent(out, depth + 1);
        let _ = writeln!(out, "{}", escape(text));
    }
    for child in &element.children {
        write_raw(out, child, depth + 1);
    }
    indent(out, depth);
    let _ = writeln!(out, "</{}>", element.name);
}

/// Write a record's scalar fields followed by its preserved extensions.
fn write_record<R: Record>(out: &mut String, record: &R, depth: usize) {
    indent(out, depth);
    let _ = writeln!(out, "<{}>", R::TAG);
    write_record_body(out, record, depth + 1);
    indent(out, depth);
    let _ = writeln!(out, "</{}>", R::TAG);
}

fn write_record_body<R: Record>(out: &mut String, record: &R, depth: usize) {
    for (tag, value) in record.scalars() {
        leaf(out, depth, tag, &value);
    }
    for extension in record.extensions() {
        write_raw(out, extension, depth);
    }
}

/// Write a collection wrapper such as `<HOPS>`.
///
/// The wrapper is written even when empty, which is what real exporters do —
/// beerxml.com's own examples contain `<WATERS></WATERS>` — and costs nothing
/// beyond a line. Consumers that expect the element to be present get it.
fn write_collection<R: Record>(out: &mut String, wrapper: &str, items: &[R], depth: usize) {
    indent(out, depth);
    let _ = writeln!(out, "<{wrapper}>");
    for item in items {
        write_record(out, item, depth + 1);
    }
    indent(out, depth);
    let _ = writeln!(out, "</{wrapper}>");
}

fn write_mash(out: &mut String, mash: &Mash, depth: usize) {
    indent(out, depth);
    let _ = writeln!(out, "<MASH>");
    write_record_body(out, &mash.header, depth + 1);
    write_collection(out, "MASH_STEPS", &mash.steps, depth + 1);
    indent(out, depth);
    let _ = writeln!(out, "</MASH>");
}

fn write_recipe(out: &mut String, recipe: &Recipe, depth: usize) {
    indent(out, depth);
    let _ = writeln!(out, "<RECIPE>");

    write_record_body(out, &recipe.header, depth + 1);

    if let Some(style) = &recipe.style {
        write_record(out, style, depth + 1);
    }
    if let Some(equipment) = &recipe.equipment {
        write_record(out, equipment, depth + 1);
    }
    write_collection(out, "HOPS", &recipe.hops, depth + 1);
    write_collection(out, "FERMENTABLES", &recipe.fermentables, depth + 1);
    write_collection(out, "MISCS", &recipe.miscs, depth + 1);
    write_collection(out, "YEASTS", &recipe.yeasts, depth + 1);
    write_collection(out, "WATERS", &recipe.waters, depth + 1);
    if let Some(mash) = &recipe.mash {
        write_mash(out, mash, depth + 1);
    }

    indent(out, depth);
    let _ = writeln!(out, "</RECIPE>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    #[test]
    fn text_needing_escapes_round_trips() {
        // A tasting note with an ampersand is not a reason to emit broken XML.
        let doc = parse("<RECIPES><RECIPE><NOTES>Hops &amp; malt &lt;great&gt;</NOTES></RECIPE></RECIPES>")
            .expect("parses");
        let xml = write(&doc);
        assert!(xml.contains("&amp;"), "ampersand is escaped: {xml}");

        let again = parse(&xml).expect("re-parses");
        assert_eq!(
            again.recipes[0].header.notes().map(String::as_str),
            Some("Hops & malt <great>")
        );
    }

    #[test]
    fn empty_collections_are_still_written() {
        // Real exporters emit them, and a consumer may expect the element.
        let doc = parse("<RECIPES><RECIPE><NAME>x</NAME></RECIPE></RECIPES>").expect("parses");
        let xml = write(&doc);
        assert!(xml.contains("<HOPS>"), "{xml}");
        assert!(xml.contains("<WATERS>"), "{xml}");
        // And reading it back gives the same empty collections.
        assert_eq!(parse(&xml).expect("re-parses"), doc);
    }

    #[test]
    fn output_is_well_formed_enough_to_read_back() {
        let doc = parse(
            r#"<RECIPES><RECIPE>
                 <NAME>Test</NAME>
                 <HOPS><HOP><NAME>Saaz</NAME><AMOUNT>0.028</AMOUNT></HOP></HOPS>
                 <MASH><NAME>Infusion</NAME><MASH_STEPS>
                   <MASH_STEP><NAME>Sacch</NAME><STEP_TEMP>67.0</STEP_TEMP></MASH_STEP>
                 </MASH_STEPS></MASH>
               </RECIPE></RECIPES>"#,
        )
        .expect("parses");

        let again = parse(&write(&doc)).expect("re-parses");
        assert_eq!(again, doc);
    }
}
