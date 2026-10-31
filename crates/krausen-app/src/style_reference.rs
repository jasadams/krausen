// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reviewed numeric BJCP 2021 facts for the initial supported styles.
use krausen_types::{
    brewery::StyleReference,
    formulation::{Range, StyleRanges},
};
pub fn resolve(style: &StyleReference) -> Option<StyleRanges> {
    if style.edition.as_deref() != Some("BJCP 2021") {
        return None;
    }
    let id = style.style_id.as_deref()?;
    let (name, og, fg, ibu, abv, srm) = match id {
        "4A" => (
            "Munich Helles",
            (1.044, 1.048),
            (1.006, 1.012),
            (16.0, 22.0),
            (4.7, 5.4),
            (3.0, 5.0),
        ),
        "5D" => (
            "German Pils",
            (1.044, 1.050),
            (1.008, 1.013),
            (22.0, 40.0),
            (4.4, 5.2),
            (2.0, 4.0),
        ),
        "6A" => (
            "Märzen",
            (1.054, 1.060),
            (1.010, 1.014),
            (18.0, 24.0),
            (5.6, 6.3),
            (8.0, 17.0),
        ),
        _ => return None,
    };
    if style.name != name {
        return None;
    }
    let range = |(min, max)| Range { min, max };
    Some(StyleRanges {
        style_id: id.into(),
        name: name.into(),
        edition: "BJCP 2021".into(),
        reference_version: "bjcp-2021-initial-v1".into(),
        original_sg: range(og),
        final_sg: range(fg),
        ibu: range(ibu),
        abv_percent: range(abv),
        colour_ebc: range((srm.0 * 1.97, srm.1 * 1.97)),
    })
}

/// Initial bounded identities for selection.
pub fn identities() -> Vec<StyleReference> {
    [
        ("4A", "Munich Helles"),
        ("5D", "German Pils"),
        ("6A", "Märzen"),
    ]
    .into_iter()
    .map(|(id, name)| StyleReference {
        name: name.into(),
        edition: Some("BJCP 2021".into()),
        style_id: Some(id.into()),
    })
    .collect()
}

/// Compare full precision shared targets; unavailable targets remain unavailable.
pub fn compare(
    style: &StyleRanges,
    targets: &krausen_types::formulation::Targets,
) -> krausen_types::formulation::StyleComparison {
    use krausen_types::formulation::{RangeStatus, StyleComparison};
    fn check(value: Option<f64>, range: &Range) -> Option<RangeStatus> {
        value.map(|v| {
            if v < range.min {
                RangeStatus::Below
            } else if v > range.max {
                RangeStatus::Above
            } else {
                RangeStatus::Within
            }
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
    #[test]
    fn identities_resolve_and_bounds_are_inclusive_at_full_precision() {
        let styles = identities();
        assert_eq!(styles.len(), 3);
        for reference in styles {
            assert_eq!(
                resolve(&reference).unwrap().reference_version,
                "bjcp-2021-initial-v1"
            );
        }
        let style = resolve(&StyleReference {
            name: "Munich Helles".into(),
            edition: Some("BJCP 2021".into()),
            style_id: Some("4A".into()),
        })
        .unwrap();
        let mut targets = Targets::default();
        targets.original_sg = Some(style.original_sg.min);
        assert_eq!(
            compare(&style, &targets).original_sg,
            Some(RangeStatus::Within)
        );
        targets.original_sg = Some(style.original_sg.min - 0.00000001);
        assert_eq!(
            compare(&style, &targets).original_sg,
            Some(RangeStatus::Below)
        );
        assert_eq!(compare(&style, &targets).final_sg, None);
    }
}
