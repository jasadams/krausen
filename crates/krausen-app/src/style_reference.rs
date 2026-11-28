// SPDX-License-Identifier: AGPL-3.0-or-later
//! BJCP 2021 beer style reference.
//!
//! The reviewed table lives in `reference/bjcp-2021-styles.json` together with its
//! citation; this module only loads, resolves and compares it. A style whose
//! guideline publishes no numeric ranges is still a selectable style; it
//! resolves with every range absent.
use krausen_types::{
    brewery::StyleReference,
    formulation::{Range, StyleRanges},
};
use serde::Deserialize;
use std::sync::LazyLock;

const EDITION: &str = "BJCP 2021";
/// EBC per SRM unit (the conversion this application has always applied).
const EBC_PER_SRM: f64 = 1.97;

#[derive(Debug, Deserialize)]
struct Table {
    reference_version: String,
    styles: Vec<Entry>,
}
#[derive(Debug, Deserialize)]
struct Entry {
    id: String,
    name: String,
    ranges: Option<EntryRanges>,
}
#[derive(Debug, Deserialize)]
struct EntryRanges {
    og: [f64; 2],
    fg: [f64; 2],
    ibu: [f64; 2],
    abv: [f64; 2],
    srm: [f64; 2],
}

static TABLE: LazyLock<Table> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../reference/bjcp-2021-styles.json"))
        .expect("embedded BJCP 2021 style table is valid (checked by the style_reference tests)")
});

pub fn resolve(style: &StyleReference) -> Option<StyleRanges> {
    if style.edition.as_deref() != Some(EDITION) {
        return None;
    }
    let id = style.style_id.as_deref()?;
    let entry = TABLE
        .styles
        .iter()
        .find(|entry| entry.id == id && entry.name == style.name)?;
    let range = |[min, max]: [f64; 2]| Range { min, max };
    let ebc = |[min, max]: [f64; 2]| Range {
        min: min * EBC_PER_SRM,
        max: max * EBC_PER_SRM,
    };
    let r = entry.ranges.as_ref();
    Some(StyleRanges {
        style_id: entry.id.clone(),
        name: entry.name.clone(),
        edition: EDITION.into(),
        reference_version: TABLE.reference_version.clone(),
        original_sg: r.map(|r| range(r.og)),
        final_sg: r.map(|r| range(r.fg)),
        ibu: r.map(|r| range(r.ibu)),
        abv_percent: r.map(|r| range(r.abv)),
        colour_ebc: r.map(|r| ebc(r.srm)),
    })
}

/// Every BJCP 2021 beer style, in guideline order.
pub fn identities() -> Vec<StyleReference> {
    TABLE
        .styles
        .iter()
        .map(|entry| StyleReference {
            name: entry.name.clone(),
            edition: Some(EDITION.into()),
            style_id: Some(entry.id.clone()),
        })
        .collect()
}

/// Compare full precision shared targets; unavailable targets remain unavailable.
pub fn compare(
    style: &StyleRanges,
    targets: &krausen_types::formulation::Targets,
) -> krausen_types::formulation::StyleComparison {
    use krausen_types::formulation::{RangeStatus, StyleComparison};
    fn check(value: Option<f64>, range: &Option<Range>) -> Option<RangeStatus> {
        let (v, range) = (value?, range.as_ref()?);
        Some(if v < range.min {
            RangeStatus::Below
        } else if v > range.max {
            RangeStatus::Above
        } else {
            RangeStatus::Within
        })
    }
    StyleComparison {
        original_sg: check(targets.original_sg, &style.original_sg),
        final_sg: check(targets.final_sg, &style.final_sg),
        ibu: check(targets.ibu, &style.ibu),
        abv_percent: check(targets.abv_percent, &style.abv_percent),
        colour_ebc: check(targets.colour_ebc, &style.colour_ebc),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krausen_types::formulation::{RangeStatus, Targets};

    fn reference(id: &str, name: &str) -> StyleReference {
        StyleReference {
            name: name.into(),
            edition: Some(EDITION.into()),
            style_id: Some(id.into()),
        }
    }

    #[test]
    fn bounds_are_inclusive_at_full_precision() {
        let style = resolve(&reference("4A", "Munich Helles")).unwrap();
        let original_sg = style.original_sg.clone().unwrap();
        let mut targets = Targets::default();
        targets.original_sg = Some(original_sg.min);
        assert_eq!(
            compare(&style, &targets).original_sg,
            Some(RangeStatus::Within)
        );
        targets.original_sg = Some(original_sg.min - 0.00000001);
        assert_eq!(
            compare(&style, &targets).original_sg,
            Some(RangeStatus::Below)
        );
        targets.original_sg = Some(original_sg.max + 0.00000001);
        assert_eq!(
            compare(&style, &targets).original_sg,
            Some(RangeStatus::Above)
        );
        assert_eq!(compare(&style, &targets).final_sg, None);
    }

    #[test]
    fn a_style_without_numeric_ranges_resolves_and_compares_as_unavailable() {
        let style = resolve(&reference("34C", "Experimental Beer")).unwrap();
        assert!(!style.has_numeric_ranges());
        assert_eq!(style.reference_version, TABLE.reference_version);
        let targets = Targets {
            original_sg: Some(1.05),
            final_sg: Some(1.01),
            ibu: Some(30.0),
            abv_percent: Some(5.0),
            colour_ebc: Some(10.0),
            ..Targets::default()
        };
        assert_eq!(
            compare(&style, &targets),
            krausen_types::formulation::StyleComparison::default()
        );
    }

    #[test]
    fn identity_requires_matching_id_name_and_edition() {
        assert!(resolve(&reference("4A", "German Pils")).is_none());
        assert!(resolve(&reference("99Z", "Munich Helles")).is_none());
        let mut old = reference("4A", "Munich Helles");
        old.edition = Some("BJCP 2015".into());
        assert!(resolve(&old).is_none());
        old.edition = None;
        assert!(resolve(&old).is_none());
    }

    #[test]
    fn every_listed_identity_resolves_and_is_unique() {
        let list = identities();
        assert_eq!(list.len(), 123);
        let mut seen = std::collections::HashSet::new();
        for style in &list {
            assert!(resolve(style).is_some(), "{} unresolved", style.name);
            assert!(
                seen.insert((style.style_id.clone(), style.name.clone())),
                "duplicate {style:?}"
            );
        }
        let names: std::collections::HashSet<_> = list.iter().map(|s| &s.name).collect();
        assert_eq!(names.len(), list.len(), "names are unique picker values");
    }

    #[test]
    fn the_data_file_cites_its_source() {
        let raw: serde_json::Value =
            serde_json::from_str(include_str!("../reference/bjcp-2021-styles.json")).unwrap();
        assert_eq!(raw["source"]["edition"], EDITION);
        assert!(
            raw["source"]["url"]
                .as_str()
                .unwrap()
                .starts_with("https://www.bjcp.org/")
        );
        assert_eq!(raw["reference_version"], TABLE.reference_version);
        assert_eq!(raw["categories"].as_array().unwrap().len(), 34);
        for style in raw["styles"].as_array().unwrap() {
            let id = style["id"].as_str().unwrap();
            let number: u64 = id
                .trim_end_matches(|c: char| c.is_ascii_alphabetic())
                .parse()
                .unwrap();
            assert_eq!(style["category"], number, "{id} is in its category");
        }
    }

    fn reviewed(cell: &str) -> Option<(f64, f64)> {
        let (min, max) = cell.split_once("..")?;
        Some((min.parse().unwrap(), max.parse().unwrap()))
    }

    /// The reviewed table is the published Vital Statistics text transcribed
    /// per style (with the one recorded Saison review note), kept apart from
    /// the data file so a wrong number in either is caught.
    #[test]
    fn every_entry_matches_the_reviewed_bjcp_table() {
        let table = include_str!("../tests/fixtures/bjcp-2021-reviewed.tsv");
        let rows: Vec<Vec<&str>> = table
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .map(|line| line.split('\t').collect())
            .collect();
        let list = identities();
        assert_eq!(rows.len(), list.len(), "same set of styles, same order");
        let mut without_ranges = 0;
        for (row, identity) in rows.iter().zip(&list) {
            let [id, name, url, _published, ibu, srm, og, fg, abv, _note] = row[..] else {
                panic!("malformed reviewed row {row:?}")
            };
            assert!(url.starts_with("https://www.bjcp.org/style/2021/"), "{id}");
            assert_eq!(identity.style_id.as_deref(), Some(id));
            assert_eq!(identity.name, name);
            let resolved = resolve(identity).unwrap();
            let expect = |cell: &str, actual: &Option<Range>, scale: f64, what: &str| {
                let actual = actual.as_ref().map(|r| (r.min, r.max));
                let expected = reviewed(cell).map(|(a, b)| (a * scale, b * scale));
                assert_eq!(actual, expected, "{id} {name} {what}");
            };
            expect(og, &resolved.original_sg, 1.0, "OG");
            expect(fg, &resolved.final_sg, 1.0, "FG");
            expect(ibu, &resolved.ibu, 1.0, "IBU");
            expect(abv, &resolved.abv_percent, 1.0, "ABV");
            expect(srm, &resolved.colour_ebc, EBC_PER_SRM, "colour");
            if !resolved.has_numeric_ranges() {
                without_ranges += 1;
            }
        }
        assert_eq!(without_ranges, 21);
    }

    /// A style range stored before ranges became optional (a saved brew's
    /// calculation context) deserialises and serialises byte-identically.
    #[test]
    fn a_stored_style_snapshot_round_trips_byte_identically() {
        use krausen_core::canonical_json;
        let raw: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/kra20_baseline_4fc8313/baseline_4fc8313_context.pretty.json"
        ))
        .unwrap();
        let stored = &raw["style"];
        assert_eq!(stored["reference_version"], "bjcp-2021-initial-v1");
        let parsed: StyleRanges = serde_json::from_value(stored.clone()).unwrap();
        assert!(parsed.has_numeric_ranges());
        let again = serde_json::to_value(&parsed).unwrap();
        assert_eq!(
            canonical_json::from_serializable(&again).unwrap(),
            canonical_json::from_serializable(stored).unwrap()
        );
        // No-range styles serialise their absent ranges as explicit nulls.
        let none =
            serde_json::to_value(resolve(&reference("34A", "Commercial Specialty Beer")).unwrap())
                .unwrap();
        assert!(none["original_sg"].is_null() && none["colour_ebc"].is_null());
        let back: StyleRanges = serde_json::from_value(none).unwrap();
        assert!(!back.has_numeric_ranges());
    }
}
