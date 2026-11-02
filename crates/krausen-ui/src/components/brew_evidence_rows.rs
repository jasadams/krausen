// SPDX-License-Identifier: AGPL-3.0-or-later
//! Stateless presentation for already-formatted historical evidence.
use leptos::prelude::*;

/// `comparison=None` renders three cells; `Some` renders a fourth cell and
/// requires the caller to provide a fourth table header.
#[component]
pub fn BrewMeasurementRow(
    label: String,
    #[prop(optional)] basis: Option<String>,
    planned: String,
    actual: String,
    #[prop(optional)] actual_context: Option<String>,
    #[prop(optional)] comparison: Option<String>,
) -> impl IntoView {
    view! { <tr>
        <td><span class="print-sheet__name">{label}</span>{basis.map(|text| view! { <span class="print-sheet__sub">{text}</span> })}</td>
        <td class="print-sheet__numeric">{planned}</td>
        <td class="print-sheet__numeric">{actual}{actual_context.map(|text| view! { <span class="print-sheet__sub">{text}</span> })}</td>
        {comparison.map(|text| view! { <td>{text}</td> })}
    </tr> }
}

/// A single fact inside a caller-owned definition list.
#[component]
pub fn BrewProvenanceFact(label: String, value: String, #[prop(optional)] mono: bool) -> impl IntoView {
    view! { <div><dt>{label}</dt><dd class:print-sheet__path=mono>{value}</dd></div> }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use leptos::tachys::view::RenderHtml;

    #[test]
    fn measurement_row_matches_header_count() {
        let three = view! { <BrewMeasurementRow label="Gravity".to_owned() planned="1.050 SG".to_owned() actual="No reading recorded".to_owned()/> }.to_html();
        let four = view! { <BrewMeasurementRow label="Gravity".to_owned() planned="1.050 SG".to_owned() actual="1.056 SG".to_owned() comparison="Difference +0.006 SG".to_owned()/> }.to_html();
        assert_eq!(three.matches("<td").count(), 3);
        assert_eq!(four.matches("<td").count(), 4);
        assert!(four.contains("Difference +0.006 SG"));
    }

    #[test]
    fn provenance_fact_keeps_definition_list_semantics() {
        let html = view! { <dl><BrewProvenanceFact label="Calculation context".to_owned() value="frozen-context".to_owned() mono=true/></dl> }.to_html();
        assert!(html.contains("<dt>"));
        assert!(html.contains("Calculation context"));
        assert!(html.contains("<dd"));
        assert!(html.contains("print-sheet__path"));
        assert!(html.contains("frozen-context"));
    }

    #[test]
    fn evidence_text_is_escaped() {
        let html = view! { <BrewMeasurementRow label="<gravity>".to_owned() planned="1 & 2".to_owned() actual="<script>".to_owned()/> }.to_html();
        assert!(html.contains("&lt;gravity&gt;"));
        assert!(html.contains("1 &amp; 2"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
    }
}
