// SPDX-License-Identifier: AGPL-3.0-or-later
//! Presentation state for a native recipe draft. Brewing policy remains shared.
use krausen_types::brewery::RecipeDocumentV1;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DraftContents {
    document: RecipeDocumentV1,
    raw_numbers: BTreeMap<String, String>,
    malformed: BTreeMap<String, String>,
}

impl DraftContents {
    pub fn title(&self) -> &str {
        &self.document.title
    }
}

#[derive(Debug, Clone)]
pub struct RecipeDraft {
    document: RecipeDocumentV1,
    raw_numbers: BTreeMap<String, String>,
    malformed: BTreeMap<String, String>,
    diagnostics: Vec<krausen_types::validation::Diagnostic>,
    baseline: DraftContents,
    undo: Vec<DraftContents>,
    pub dirty: bool,
    pub generation: u64,
}

impl RecipeDraft {
    pub fn new(document: RecipeDocumentV1) -> Self {
        let baseline = DraftContents {
            document: document.clone(),
            raw_numbers: BTreeMap::new(),
            malformed: BTreeMap::new(),
        };
        Self {
            document,
            raw_numbers: BTreeMap::new(),
            malformed: BTreeMap::new(),
            diagnostics: Vec::new(),
            baseline,
            undo: Vec::new(),
            dirty: false,
            generation: 0,
        }
    }
    pub fn document(&self) -> &RecipeDocumentV1 {
        &self.document
    }
    pub fn edit(&mut self, change: impl FnOnce(&mut RecipeDocumentV1)) {
        self.remember();
        change(&mut self.document);
        self.diagnostics.clear();
        self.dirty = true;
        self.generation = self
            .generation
            .checked_add(1)
            .expect("draft generation exhausted");
    }
    pub fn edit_clearing(&mut self, prefix: &str, change: impl FnOnce(&mut RecipeDocumentV1)) {
        self.remember();
        self.raw_numbers.retain(|path, _| !path.starts_with(prefix));
        self.malformed.retain(|path, _| !path.starts_with(prefix));
        change(&mut self.document);
        self.diagnostics.clear();
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
    pub fn validation_error(&self, path: &str) -> Option<&str> {
        self.diagnostics
            .iter()
            .find(|d| d.path == path && d.severity == krausen_types::validation::Readiness::Invalid)
            .map(|d| match d.code.as_str() {
                "negative" => "Enter a non-negative value",
                "out_of_range" => "Value is out of range",
                "non_finite" => "Enter a finite number",
                _ => "Check this value",
            })
    }
    pub fn number_error(&self, path: &str) -> Option<&str> {
        self.malformed
            .get(path)
            .map(String::as_str)
            .or_else(|| self.validation_error(path))
    }
    pub fn edit_number(
        &mut self,
        path: &str,
        text: String,
        assign: impl FnOnce(&mut RecipeDocumentV1, Option<f64>),
    ) {
        self.remember();
        self.diagnostics.clear();
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
    pub fn set_validation(&mut self, validation: krausen_types::validation::Validation) {
        self.diagnostics = validation.diagnostics;
    }
    pub fn diagnostics(&self) -> &[krausen_types::validation::Diagnostic] {
        &self.diagnostics
    }
    pub fn unfinished_inputs(&self) -> Vec<(String, String)> {
        self.malformed.keys().filter_map(|path| self.raw_numbers.get(path).map(|text| (path.clone(), text.clone()))).collect()
    }
    fn contents(&self) -> DraftContents {
        DraftContents {
            document: self.document.clone(),
            raw_numbers: self.raw_numbers.clone(),
            malformed: self.malformed.clone(),
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
            self.diagnostics.clear();
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
        self.document = contents.document;
        self.raw_numbers = contents.raw_numbers;
        self.malformed = contents.malformed;
        self.diagnostics.clear();
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
    pub fn mark_earlier_document_saved(&mut self, document: RecipeDocumentV1) {
        self.baseline = DraftContents {
            document,
            raw_numbers: BTreeMap::new(),
            malformed: BTreeMap::new(),
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
    fn editing_clears_stale_server_diagnostics_and_preview() {
        use krausen_types::validation::{Diagnostic, Readiness, Validation};
        let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
        draft.set_validation(Validation {
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
}
