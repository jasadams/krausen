// SPDX-License-Identifier: AGPL-3.0-or-later
//! Presentation state for a native recipe draft. Brewing policy remains shared.
use krausen_types::brewery::RecipeDocumentV1;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftContents {
    document: RecipeDocumentV1,
    raw_numbers: BTreeMap<String, String>,
    malformed: BTreeMap<String, String>,
    #[serde(default)]
    settled_numbers: BTreeSet<String>,
}

// Feedback timing is retained for recovery/Undo, but is not a content edit.
impl PartialEq for DraftContents {
    fn eq(&self, other: &Self) -> bool {
        self.document == other.document
            && self.raw_numbers == other.raw_numbers
            && self.malformed == other.malformed
    }
}

impl DraftContents {
    pub fn title(&self) -> &str {
        &self.document.title
    }
}

/// Raw entry state removed together with a row, retained for a scoped Undo.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RemovedInputs {
    raw_numbers: BTreeMap<String, String>,
    malformed: BTreeMap<String, String>,
    settled_numbers: Vec<String>,
}

fn take_prefixed(map: &mut BTreeMap<String, String>, prefix: &str) -> BTreeMap<String, String> {
    let keys: Vec<String> = map.keys().filter(|path| path.starts_with(prefix)).cloned().collect();
    keys.into_iter()
        .filter_map(|key| map.remove(&key).map(|value| (key, value)))
        .collect()
}

#[derive(Debug, Clone)]
pub struct RecipeDraft {
    document: RecipeDocumentV1,
    raw_numbers: BTreeMap<String, String>,
    malformed: BTreeMap<String, String>,
    settled_numbers: BTreeSet<String>,
    diagnostics: Vec<krausen_types::validation::Diagnostic>,
    /// The draft generation `diagnostics` were computed for.
    diagnostics_generation: u64,
    /// Server readiness for exactly one draft generation; never carried across edits.
    readiness: Option<(u64, krausen_types::validation::Readiness)>,
    baseline: DraftContents,
    undo: Vec<DraftContents>,
    pub dirty: bool,
    pub generation: u64,
    /// Advances only when the whole document is replaced (reload/remote
    /// revision), so a scoped Undo can never re-insert into a foreign document.
    epoch: u64,
}

impl RecipeDraft {
    pub fn new(document: RecipeDocumentV1) -> Self {
        let baseline = DraftContents {
            document: document.clone(),
            raw_numbers: BTreeMap::new(),
            malformed: BTreeMap::new(),
            settled_numbers: BTreeSet::new(),
        };
        Self {
            document,
            raw_numbers: BTreeMap::new(),
            malformed: BTreeMap::new(),
            settled_numbers: BTreeSet::new(),
            diagnostics: Vec::new(),
            diagnostics_generation: 0,
            readiness: None,
            baseline,
            undo: Vec::new(),
            dirty: false,
            generation: 0,
            epoch: 0,
        }
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    /// Keeps the exact text a brewer typed for values committed with a new row
    /// (INT-001): native storage is numeric, the entry text is not lost on Add.
    pub fn retain_raw_numbers(&mut self, entries: Vec<(String, String)>) {
        for (path, text) in entries {
            self.raw_numbers.insert(path, text);
        }
    }
    pub fn document(&self) -> &RecipeDocumentV1 {
        &self.document
    }
    pub fn edit(&mut self, change: impl FnOnce(&mut RecipeDocumentV1)) {
        self.remember();
        change(&mut self.document);
        self.dirty = true;
        self.generation = self
            .generation
            .checked_add(1)
            .expect("draft generation exhausted");
    }
    pub fn edit_clearing(&mut self, prefix: &str, change: impl FnOnce(&mut RecipeDocumentV1)) {
        self.remember();
        // The cleared field stops reporting its problem, under every path the validator uses for it.
        let reported = crate::recipe_diagnostics::validator_paths(prefix);
        self.diagnostics.retain(|d| !reported.iter().any(|path| d.path.starts_with(path.as_str())));
        self.raw_numbers.retain(|path, _| !path.starts_with(prefix));
        self.malformed.retain(|path, _| !path.starts_with(prefix));
        self.settled_numbers.retain(|path| !path.starts_with(prefix));
        change(&mut self.document);
        self.dirty = true;
        self.generation = self
            .generation
            .checked_add(1)
            .expect("draft generation exhausted");
    }
    /// Like `edit_clearing`, but hands back the exact transient strings of the
    /// removed row so a scoped Undo can restore them byte for byte.
    pub fn edit_removing(
        &mut self,
        prefix: &str,
        change: impl FnOnce(&mut RecipeDocumentV1),
    ) -> RemovedInputs {
        self.remember();
        let removed = RemovedInputs {
            raw_numbers: take_prefixed(&mut self.raw_numbers, prefix),
            malformed: take_prefixed(&mut self.malformed, prefix),
            settled_numbers: {
                let paths: Vec<String> = self
                    .settled_numbers
                    .iter()
                    .filter(|path| path.starts_with(prefix))
                    .cloned()
                    .collect();
                for path in &paths {
                    self.settled_numbers.remove(path);
                }
                paths
            },
        };
        change(&mut self.document);
        self.dirty = true;
        self.generation = self
            .generation
            .checked_add(1)
            .expect("draft generation exhausted");
        removed
    }
    /// Reinstates a row together with the transient strings captured by
    /// `edit_removing`; the draft's ordinary Undo history remains untouched.
    pub fn edit_restoring(
        &mut self,
        inputs: RemovedInputs,
        change: impl FnOnce(&mut RecipeDocumentV1),
    ) {
        self.remember();
        self.raw_numbers.extend(inputs.raw_numbers);
        self.malformed.extend(inputs.malformed);
        self.settled_numbers.extend(inputs.settled_numbers);
        change(&mut self.document);
        self.dirty = true;
        self.generation = self
            .generation
            .checked_add(1)
            .expect("draft generation exhausted");
    }
    pub fn number_text(&self, path: &str, value: Option<f64>) -> String {
        self.raw_numbers
            .get(path)
            .cloned()
            .unwrap_or_else(|| value.map(|v| v.to_string()).unwrap_or_default())
    }
    /// The server diagnostic of this severity for `path`, only while the diagnostics
    /// describe exactly the current draft generation.
    fn current_diagnostic(&self, path: &str, severity: krausen_types::validation::Readiness) -> Option<&krausen_types::validation::Diagnostic> {
        // Beside a control only what is true of the draft as it is now; the summary
        // is the place that keeps an earlier list, marked out of date.
        if !self.diagnostics_current() {
            return None;
        }
        self.diagnostics
            .iter()
            .find(|d| d.path == path && d.severity == severity)
    }
    pub fn validation_error(&self, path: &str) -> Option<&'static str> {
        self.current_diagnostic(path, krausen_types::validation::Readiness::Invalid)
            .map(|d| crate::recipe_diagnostics::inline_text(&d.code, d.severity))
    }
    /// What a field still needs before a brew can start; never a save blocker.
    pub fn validation_hint(&self, path: &str) -> Option<&'static str> {
        self.current_diagnostic(path, krausen_types::validation::Readiness::Incomplete)
            .map(|d| crate::recipe_diagnostics::inline_text(&d.code, d.severity))
    }
    pub fn number_error(&self, path: &str) -> Option<&str> {
        self.malformed
            .get(path)
            .filter(|_| self.settled_numbers.contains(path))
            .map(String::as_str)
            .or_else(|| self.validation_error(path))
    }
    /// Blur reveals formatting feedback without editing or normalizing content.
    pub fn settle_number(&mut self, path: &str) {
        self.settled_numbers.insert(path.to_owned());
    }
    /// Explicit submission settles all fields already touched by this draft.
    pub fn settle_numbers(&mut self) {
        self.settled_numbers.extend(self.raw_numbers.keys().cloned());
    }
    pub fn edit_number(
        &mut self,
        path: &str,
        text: String,
        assign: impl FnOnce(&mut RecipeDocumentV1, Option<f64>),
    ) {
        self.remember();
        // Only the field being corrected stops reporting its problem now; every other
        // distinct diagnostic stays until the next settled preview replaces them.
        let reported = crate::recipe_diagnostics::validator_paths(path);
        self.diagnostics.retain(|d| !reported.contains(&d.path));
        self.raw_numbers.insert(path.to_owned(), text.clone());
        let value = if text.trim().is_empty() {
            Some(None)
        } else {
            text.trim()
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .map(Some)
        };
        match value {
            Some(value) => {
                self.malformed.remove(path);
                assign(&mut self.document, value);
            }
            None => {
                self.malformed
                    .insert(path.to_owned(), "Enter a number".into());
            }
        }
        self.dirty = true;
        self.generation = self
            .generation
            .checked_add(1)
            .expect("draft generation exhausted");
    }
    /// Accepts a server validation only if it was computed for the draft's current
    /// generation; a late response for an older one changes nothing. Returns whether
    /// it was applied. The whole list is replaced, so any diagnostic that depended
    /// on another field is recomputed rather than carried.
    pub fn apply_validation(&mut self, generation: u64, validation: krausen_types::validation::Validation) -> bool {
        if generation != self.generation {
            return false;
        }
        self.readiness = Some((generation, validation.state));
        self.diagnostics = validation.diagnostics;
        self.diagnostics_generation = generation;
        true
    }
    /// Forget readiness when the preview that reported it is no longer trusted
    /// (context loss or retry at the same generation).
    pub fn clear_readiness(&mut self) {
        self.readiness = None;
    }
    /// Readiness the shared preview reported for this exact draft generation.
    pub fn readiness(&self) -> Option<krausen_types::validation::Readiness> {
        self.readiness.filter(|(generation, _)| *generation == self.generation).map(|(_, state)| state)
    }
    /// Whether the retained diagnostics describe the draft as it is now. False once
    /// the draft has moved on and no check has run yet, and also while the check that
    /// produced them is no longer trusted (`clear_readiness`: context loss or retry at
    /// the same generation), for as long as the estimate stays cleared.
    pub fn diagnostics_current(&self) -> bool {
        self.diagnostics_generation == self.generation && self.readiness().is_some()
    }
    /// The last settled server diagnostics, retained across edits so the summary
    /// keeps its shape while the brewer types; readiness stays generation-scoped.
    pub fn diagnostics(&self) -> &[krausen_types::validation::Diagnostic] {
        &self.diagnostics
    }
    /// Malformed entries whose format feedback is already showing; a first
    /// character such as "-" is transitional and is not one of these.
    pub fn settled_malformed(&self) -> Vec<String> {
        self.malformed.keys().filter(|path| self.settled_numbers.contains(*path)).cloned().collect()
    }
    pub fn unfinished_inputs(&self) -> Vec<(String, String)> {
        self.malformed.keys().filter_map(|path| self.raw_numbers.get(path).map(|text| (path.clone(), text.clone()))).collect()
    }
    pub fn raw_numbers(&self) -> &BTreeMap<String, String> {
        &self.raw_numbers
    }
    fn contents(&self) -> DraftContents {
        DraftContents {
            document: self.document.clone(),
            raw_numbers: self.raw_numbers.clone(),
            malformed: self.malformed.clone(),
            settled_numbers: self.settled_numbers.clone(),
        }
    }
    fn remember(&mut self) {
        self.undo.push(self.contents());
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn undo(&mut self) {
        if let Some(previous) = self.undo.pop() {
            self.document = previous.document;
            self.raw_numbers = previous.raw_numbers;
            self.malformed = previous.malformed;
            self.settled_numbers = previous.settled_numbers;
            self.dirty = self.contents() != self.baseline;
            self.generation = self
                .generation
                .checked_add(1)
                .expect("draft generation exhausted");
        }
    }
    pub fn recovery(&self) -> DraftContents {
        self.contents()
    }
    pub fn restore(&mut self, contents: DraftContents) {
        self.remember();
        self.epoch = self.epoch.checked_add(1).expect("draft epoch exhausted");
        self.document = contents.document;
        self.raw_numbers = contents.raw_numbers;
        self.malformed = contents.malformed;
        self.settled_numbers = contents.settled_numbers;
        self.dirty = self.contents() != self.baseline;
        self.generation = self
            .generation
            .checked_add(1)
            .expect("draft generation exhausted");
    }
    pub fn mark_saved(&mut self) {
        self.baseline = self.contents();
        self.dirty = false;
    }
    pub fn replace_saved(&mut self, document: RecipeDocumentV1) {
        let next = self.generation.checked_add(1).expect("draft generation exhausted");
        let epoch = self.epoch.checked_add(1).expect("draft epoch exhausted");
        *self = Self::new(document);
        self.generation = next;
        self.epoch = epoch;
    }
    pub fn mark_earlier_document_saved(&mut self, document: RecipeDocumentV1) {
        self.baseline = DraftContents {
            document,
            raw_numbers: BTreeMap::new(),
            malformed: BTreeMap::new(),
            settled_numbers: BTreeSet::new(),
        };
        self.dirty = self.contents() != self.baseline;
    }
    /// Do not convert an unfinished number into an absent value or zero.
    pub fn snapshot(&self) -> Result<RecipeDocumentV1, Vec<String>> {
        if self.malformed.is_empty() {
            Ok(self.document.clone())
        } else {
            Err(self.malformed.keys().cloned().collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transitional_number_is_retained_and_blocks_snapshot_without_premature_feedback() {
        let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
        for text in ["-", "1e", "1e-"] {
            draft.edit_number("batch_size_l", text.into(), |document, value| {
                document.batch_size_l = value;
            });
            assert_eq!(draft.number_text("batch_size_l", None), text);
            assert!(draft.snapshot().is_err(), "transient input must block submission");
            assert_eq!(draft.number_error("batch_size_l"), None,
                "formatting feedback must wait for blur or explicit submission");
        }
    }

    #[test]
    fn settled_feedback_tracks_correction_without_content_or_generation_changes_on_blur() {
        let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
        draft.edit_number("batch_size_l", "-".into(), |d, n| d.batch_size_l = n);
        let generation = draft.generation;
        let contents = draft.recovery();
        draft.settle_number("batch_size_l");
        assert_eq!(draft.generation, generation);
        assert_eq!(draft.recovery(), contents);
        assert_eq!(draft.number_error("batch_size_l"), Some("Enter a number"));
        draft.edit_number("batch_size_l", "1e".into(), |d, n| d.batch_size_l = n);
        assert_eq!(draft.number_error("batch_size_l"), Some("Enter a number"));
        draft.edit_number("batch_size_l", "2.000000000000000e1".into(), |d, n| d.batch_size_l = n);
        assert_eq!(draft.number_error("batch_size_l"), None);
        assert_eq!(draft.number_text("batch_size_l", None), "2.000000000000000e1");
        assert_eq!(draft.snapshot().unwrap().batch_size_l, Some(20.0));
        draft.edit_number("batch_size_l", "-".into(), |d, n| d.batch_size_l = n);
        let record = serde_json::to_string(&draft.recovery()).unwrap();
        let mut recovered = RecipeDraft::new(RecipeDocumentV1::default());
        recovered.restore(serde_json::from_str(&record).unwrap());
        assert_eq!(recovered.number_text("batch_size_l", None), "-");
        assert_eq!(recovered.number_error("batch_size_l"), Some("Enter a number"));
        assert!(recovered.snapshot().is_err());
        draft.undo();
        assert_eq!(draft.number_text("batch_size_l", None), "2.000000000000000e1");
        assert!(draft.snapshot().is_ok());
    }

    #[test]
    fn explicit_submission_settles_raw_fields_and_old_recovery_remains_readable() {
        let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
        draft.edit_number("batch_size_l", "1e-".into(), |d, n| d.batch_size_l = n);
        draft.settle_numbers();
        assert_eq!(draft.number_error("batch_size_l"), Some("Enter a number"));
        let mut legacy = serde_json::to_value(draft.recovery()).unwrap();
        legacy.as_object_mut().unwrap().remove("settled_numbers");
        let mut recovered = RecipeDraft::new(RecipeDocumentV1::default());
        recovered.restore(serde_json::from_value(legacy).unwrap());
        assert_eq!(recovered.number_text("batch_size_l", None), "1e-");
        assert!(recovered.snapshot().is_err());
        assert_eq!(recovered.number_error("batch_size_l"), None);
    }

    #[test]
    fn editing_clears_stale_server_diagnostics_and_preview() {
        use krausen_types::validation::{Diagnostic, Readiness, Validation};
        let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
        draft.apply_validation(draft.generation, Validation {
            state: Readiness::Invalid,
            diagnostics: vec![Diagnostic {
                code: "negative".into(),
                path: "batch_size_l".into(),
                severity: Readiness::Invalid,
                affected_targets: vec![],
            }],
        });
        assert_eq!(
            draft.number_error("batch_size_l"),
            Some("Enter a non-negative value")
        );
        let prior = draft.generation;
        draft.edit_number("batch_size_l", "10".into(), |doc, value| {
            doc.batch_size_l = value
        });
        assert!(draft.diagnostics().is_empty());
        assert_ne!(draft.generation, prior);
    }

    fn diagnostic(path: &str, code: &str, severity: krausen_types::validation::Readiness) -> krausen_types::validation::Diagnostic {
        krausen_types::validation::Diagnostic { code: code.into(), path: path.into(), severity, affected_targets: vec![] }
    }

    #[test]
    fn fixing_one_field_keeps_every_other_distinct_diagnostic() {
        use krausen_types::validation::{Readiness::*, Validation};
        let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
        draft.apply_validation(draft.generation, Validation {
            state: Incomplete,
            diagnostics: vec![
                diagnostic("hops[a].specification.alpha_fraction", "missing", Incomplete),
                diagnostic("hops[b].specification.alpha_fraction", "missing", Incomplete),
            ],
        });
        draft.edit_number("hops[a].specification.alpha_fraction", "5".into(), |_, _| {});
        let left: Vec<_> = draft.diagnostics().iter().map(|d| d.path.as_str()).collect();
        assert_eq!(left, ["hops[b].specification.alpha_fraction"]);
        draft.edit(|doc| doc.title = "renamed".into());
        assert_eq!(draft.diagnostics().len(), 1, "a structural edit must not empty the summary either");
        assert_eq!(draft.readiness(), None, "readiness is still only for the generation that was checked");
    }

    /// Correcting a raw-input field retires the validator's entry for it, even
    /// where the validator names the field differently from the input.
    fn edited_retires(raw: &str, reported: &str, other: &str) {
        use krausen_types::validation::{Readiness::*, Validation};
        let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
        draft.apply_validation(draft.generation, Validation {
            state: Invalid,
            diagnostics: vec![diagnostic(reported, "negative", Invalid), diagnostic(other, "negative", Invalid)],
        });
        draft.edit_number(raw, "5".into(), |_, _| {});
        let left: Vec<_> = draft.diagnostics().iter().map(|d| d.path.as_str()).collect();
        assert_eq!(left, [other], "editing {raw} must retire {reported} and keep {other}");
    }

    #[test]
    fn editing_a_yeast_amount_retires_the_ingredient_path_the_validator_reports() {
        edited_retires("yeast[y1].amount", "ingredients[y1].amount", "ingredients[y2].amount");
    }

    #[test]
    fn editing_an_other_ingredient_amount_retires_the_ingredient_path() {
        edited_retires("other_ingredients[o1].amount", "ingredients[o1].amount", "ingredients[o2].amount");
    }

    #[test]
    fn editing_a_yeast_attenuation_retires_the_specification_path() {
        edited_retires("yeast[y1].attenuation", "yeast[y1].specification.attenuation_fraction", "yeast[y2].specification.attenuation_fraction");
    }

    #[test]
    fn editing_an_equipment_override_retires_the_effective_equipment_path() {
        edited_retires("equipment_overrides.kettle_diameter_cm", "equipment.kettle_diameter_cm", "equipment.boil_loss_l_per_h");
    }

    #[test]
    fn editing_an_equipment_override_retires_the_override_path_too() {
        edited_retires("equipment_overrides.kettle_diameter_cm", "equipment_overrides.kettle_diameter_cm", "equipment_overrides.boil_loss_l_per_h");
    }

    #[test]
    fn clearing_an_override_retires_its_retained_entries_so_review_cannot_stay_on_it() {
        use krausen_types::validation::{Readiness::*, Validation};
        for (prefix, reported) in [
            ("equipment_overrides.kettle_diameter_cm", ["equipment_overrides.kettle_diameter_cm", "equipment.kettle_diameter_cm"]),
            ("equipment_overrides.cooling_flow_rate_l_per_minute", ["equipment_overrides.cooling_flow_rate_l_per_minute", "equipment.cooling_flow_rate_l_per_minute"]),
        ] {
            let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
            draft.apply_validation(draft.generation, Validation {
                state: Invalid,
                diagnostics: vec![diagnostic(reported[0], "negative", Invalid), diagnostic(reported[1], "negative", Invalid), diagnostic("batch_size_l", "negative", Invalid)],
            });
            draft.edit_clearing(prefix, |_| {});
            let left: Vec<_> = draft.diagnostics().iter().map(|d| d.path.as_str()).collect();
            assert_eq!(left, ["batch_size_l"], "clearing {prefix} retires its entries under both validator paths");
        }
    }

    #[test]
    fn inline_feedback_separates_save_blockers_from_start_requirements() {
        use krausen_types::validation::{Readiness::*, Validation};
        let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
        draft.apply_validation(draft.generation, Validation {
            state: Invalid,
            diagnostics: vec![
                diagnostic("batch_size_l", "negative", Invalid),
                diagnostic("boil_minutes", "missing", Incomplete),
                diagnostic("hops[a].stage", "unsupported_hop_stage", Incomplete),
            ],
        });
        assert_eq!(draft.validation_error("batch_size_l"), Some("Enter a non-negative value"));
        assert_eq!(draft.validation_hint("batch_size_l"), None);
        assert_eq!(draft.validation_error("boil_minutes"), None, "a start requirement is not an error");
        assert_eq!(draft.validation_hint("boil_minutes"), Some("Needed to start a brew"));
        assert_eq!(draft.validation_hint("hops[a].stage"), Some("Choose a supported stage to start a brew"));
    }

    #[test]
    fn a_new_error_in_the_edited_field_appears_when_the_next_validation_settles() {
        use krausen_types::validation::{Readiness::*, Validation};
        let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
        draft.apply_validation(draft.generation, Validation { state: Incomplete, diagnostics: vec![diagnostic("batch_size_l", "missing", Incomplete)] });
        draft.edit_number("batch_size_l", "-3".into(), |doc, n| doc.batch_size_l = n);
        assert_eq!((draft.validation_error("batch_size_l"), draft.validation_hint("batch_size_l")), (None, None), "no feedback from the old state while typing");
        let generation = draft.generation;
        assert!(draft.apply_validation(generation, Validation { state: Invalid, diagnostics: vec![diagnostic("batch_size_l", "negative", Invalid)] }));
        assert_eq!(draft.validation_error("batch_size_l"), Some("Enter a non-negative value"));
    }

    #[test]
    fn a_diagnostic_that_depends_on_another_field_is_recomputed_not_carried() {
        use krausen_types::validation::{Readiness::*, Validation};
        let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
        let exposure = diagnostic("hops[a].contact_minutes", "exposure_exceeds_boil", Invalid);
        draft.apply_validation(draft.generation, Validation { state: Invalid, diagnostics: vec![exposure] });
        // Raising the boil time fixes the hop exposure, but the hop's own entry is only
        // retained until the next settled validation replaces the whole list.
        draft.edit_number("boil_minutes", "90".into(), |doc, n| doc.boil_minutes = n);
        assert_eq!(draft.diagnostics().len(), 1);
        let generation = draft.generation;
        draft.apply_validation(generation, Validation { state: Incomplete, diagnostics: vec![diagnostic("hops[a].amount_g", "missing", Incomplete)] });
        let paths: Vec<_> = draft.diagnostics().iter().map(|d| d.path.as_str()).collect();
        assert_eq!(paths, ["hops[a].amount_g"], "the settled result replaces, never merges with, the old list");
    }

    #[test]
    fn a_response_for_an_older_generation_cannot_overwrite_newer_diagnostics() {
        use krausen_types::validation::{Readiness::*, Validation};
        let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
        let old = draft.generation;
        draft.edit_number("batch_size_l", "20".into(), |doc, n| doc.batch_size_l = n);
        let current = draft.generation;
        assert!(draft.apply_validation(current, Validation { state: Incomplete, diagnostics: vec![diagnostic("boil_minutes", "missing", Incomplete)] }));
        assert!(!draft.apply_validation(old, Validation { state: Invalid, diagnostics: vec![diagnostic("batch_size_l", "negative", Invalid)] }));
        assert_eq!(draft.diagnostics().len(), 1);
        assert_eq!(draft.diagnostics()[0].path, "boil_minutes");
        assert_eq!(draft.readiness(), Some(Incomplete));
    }

    #[test]
    fn inline_messages_describe_only_the_current_draft_while_the_summary_keeps_the_earlier_list() {
        use krausen_types::validation::{Readiness::*, Validation};
        let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
        draft.apply_validation(draft.generation, Validation { state: Incomplete, diagnostics: vec![diagnostic("hops[a].stage", "unsupported_hop_stage", Incomplete)] });
        assert!(draft.validation_hint("hops[a].stage").is_some());
        // A select is edited with the ordinary structural edit; its old message must go quiet at once.
        draft.edit(|doc| doc.title = "changed".into());
        assert_eq!(draft.validation_hint("hops[a].stage"), None);
        assert_eq!(draft.diagnostics().len(), 1, "the summary still has it, marked out of date");
        let generation = draft.generation;
        draft.apply_validation(generation, Validation { state: Incomplete, diagnostics: vec![diagnostic("hops[a].stage", "unsupported_hop_stage", Incomplete)] });
        assert!(draft.validation_hint("hops[a].stage").is_some(), "back once a check settles");
        draft.undo();
        assert_eq!(draft.validation_hint("hops[a].stage"), None, "Undo moves the draft on as well");
    }

    #[test]
    fn raw_numbers_preserve_empty_invalid_and_real_zero() {
        let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
        draft.edit_number("batch_size_l", "-".into(), |d, n| d.batch_size_l = n);
        assert_eq!(draft.number_text("batch_size_l", None), "-");
        assert!(draft.snapshot().is_err());
        draft.edit_number("batch_size_l", "0".into(), |d, n| d.batch_size_l = n);
        assert_eq!(draft.snapshot().unwrap().batch_size_l, Some(0.0));
        draft.edit_number("batch_size_l", "".into(), |d, n| d.batch_size_l = n);
        assert_eq!(draft.snapshot().unwrap().batch_size_l, None);
    }

    #[test]
    fn undo_and_recovery_keep_unfinished_numbers() {
        let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
        draft.edit(|doc| doc.title = "First".into());
        draft.edit_number("batch_size_l", "-".into(), |doc, n| doc.batch_size_l = n);
        let encoded = serde_json::to_string(&draft.recovery()).unwrap();
        let mut restored = RecipeDraft::new(RecipeDocumentV1::default());
        restored.restore(serde_json::from_str(&encoded).unwrap());
        assert_eq!(restored.number_text("batch_size_l", None), "-");
        assert!(restored.snapshot().is_err());
        draft.undo();
        assert_eq!(draft.document().title, "First");
        assert!(draft.snapshot().is_ok());
        draft.undo();
        assert!(!draft.dirty);
        draft.edit_number("row.amount", "-".into(), |_, _| {});
        draft.edit_clearing("row.", |_| {});
        assert!(draft.snapshot().is_ok());
        draft.undo();
        assert_eq!(draft.number_text("row.amount", None), "-");
        assert!(draft.snapshot().is_err());
    }

    #[test]
    fn replacing_clean_saved_document_invalidates_existing_preview_generation() {
        let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
        let before = draft.generation;
        let replacement = RecipeDocumentV1 { title: "Remote revision".into(), ..Default::default() };
        draft.replace_saved(replacement.clone());
        assert_ne!(draft.generation, before);
        assert_eq!(draft.document(), &replacement);
        assert!(!draft.dirty);
    }
}
