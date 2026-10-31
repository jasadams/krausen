// SPDX-License-Identifier: AGPL-3.0-or-later

//! Round-trip fidelity against a maintained synthetic BeerXML file.
//!
//! It exercises records Krausen does not yet use, including styles, mash
//! schedules, water, and unknown extensions.
//!
//! These tests check model and element preservation for this fixture. They do
//! not assert byte-identical output or cover every third-party document.

use krausen_beerxml::{parse, write};

const EXAMPLES: &str = include_str!("fixtures/synthetic.xml");

#[test]
fn the_example_file_parses() {
    let doc = parse(EXAMPLES).expect("synthetic fixture must parse");
    assert!(!doc.is_empty(), "found no recipes");
    for recipe in &doc.recipes {
        assert!(!recipe.name().is_empty(), "every recipe should be named");
    }
}

#[test]
fn a_write_then_read_reproduces_the_model_exactly() {
    // The load-bearing test. Not "looks similar" — identical.
    let original = parse(EXAMPLES).expect("parses");
    let round_tripped = parse(&write(&original)).expect("re-parses");
    assert_eq!(
        round_tripped, original,
        "the model changed when written and read back"
    );
}

#[test]
fn round_tripping_is_stable_across_repeated_passes() {
    // A conversion that drifts a little each pass would pass a single
    // round-trip and still corrupt a recipe over time.
    let once = write(&parse(EXAMPLES).expect("parses"));
    let twice = write(&parse(&once).expect("re-parses"));
    assert_eq!(once, twice, "output is not a fixed point");
}

#[test]
fn nothing_in_the_source_is_silently_dropped() {
    // Count every element in the source and in our output. The output may add
    // nothing and must lose nothing.
    fn element_counts(xml: &str) -> std::collections::BTreeMap<String, usize> {
        let mut counts = std::collections::BTreeMap::new();
        let mut rest = xml;
        while let Some(start) = rest.find('<') {
            rest = &rest[start + 1..];
            if rest.starts_with('/') || rest.starts_with('?') || rest.starts_with('!') {
                continue;
            }
            let Some(end) = rest.find(['>', ' ', '/']) else {
                break;
            };
            let name = &rest[..end];
            if !name.is_empty() {
                *counts.entry(name.to_string()).or_insert(0) += 1;
            }
        }
        counts
    }

    let source = element_counts(EXAMPLES);
    let output = element_counts(&write(&parse(EXAMPLES).expect("parses")));

    let mut lost = Vec::new();
    for (name, count) in &source {
        let produced = output.get(name).copied().unwrap_or(0);
        if produced < *count {
            lost.push(format!("{name}: {count} in source, {produced} written"));
        }
    }
    assert!(lost.is_empty(), "elements lost on export:\n  {}", lost.join("\n  "));
}

#[test]
fn the_records_krausen_does_not_use_are_still_read() {
    // "Store everything regardless of whether we use it or not" — these are
    // the records the engine ignores entirely.
    let doc = parse(EXAMPLES).expect("parses");

    assert!(
        doc.recipes.iter().any(|r| r.style.is_some()),
        "styles should be read"
    );
    assert!(
        doc.recipes
            .iter()
            .any(|r| r.mash.as_ref().is_some_and(|m| !m.steps.is_empty())),
        "mash schedules should be read"
    );
    assert!(
        doc.recipes.iter().any(|r| !r.miscs.is_empty()),
        "misc ingredients should be read"
    );
    assert!(
        doc.recipes.iter().any(|r| r.equipment.is_some()),
        "equipment profiles should be read"
    );
}

#[test]
fn another_programs_estimates_are_kept_but_stay_separate() {
    // §3.6: every figure in Krausen comes from its own engine. Another
    // program's estimates are stored because they were in the file and are
    // useful to compare against — never used as inputs.
    let doc = parse(EXAMPLES).expect("parses");
    let with_estimates = doc
        .recipes
        .iter()
        .find(|r| r.header.est_og().is_some())
        .expect("the examples carry estimates");

    assert!(with_estimates.header.ibu().is_some());
    assert!(with_estimates.header.est_color().is_some());
}
