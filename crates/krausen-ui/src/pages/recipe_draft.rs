// SPDX-License-Identifier: AGPL-3.0-or-later
//! Presentation state for a native recipe draft. Brewing policy remains shared.
use super::recipe_preview_gate::PreviewGate;
use krausen_types::brewery::RecipeDocumentV1;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct RecipeDraft {
    document: RecipeDocumentV1,
    raw_numbers: BTreeMap<String, String>,
    malformed: BTreeMap<String, String>,
    diagnostics: Vec<krausen_types::validation::Diagnostic>,
    pub preview: PreviewGate,
    pub dirty: bool,
    pub generation: u64,
}

impl RecipeDraft {
    pub fn new(document: RecipeDocumentV1) -> Self {
        Self {
            document,
            raw_numbers: BTreeMap::new(),
            malformed: BTreeMap::new(),
            diagnostics: Vec::new(),
            preview: PreviewGate::default(),
            dirty: false,
            generation: 0,
        }
    }
    pub fn document(&self) -> &RecipeDocumentV1 {
        &self.document
    }
    pub fn edit(&mut self, change: impl FnOnce(&mut RecipeDocumentV1)) {
        change(&mut self.document);
        self.diagnostics.clear();
        self.dirty = true;
        self.generation = self.preview.invalidate();
    }
    pub fn number_text(&self, path: &str, value: Option<f64>) -> String {
        self.raw_numbers
            .get(path)
            .cloned()
            .unwrap_or_else(|| value.map(|v| v.to_string()).unwrap_or_default())
    }
    pub fn number_error(&self, path: &str) -> Option<&str> {
        self.malformed.get(path).map(String::as_str).or_else(|| {
            self.diagnostics
                .iter()
                .find(|d| {
                    d.path == path && d.severity == krausen_types::validation::Readiness::Invalid
                })
                .map(|d| match d.code.as_str() {
                    "negative" => "Enter a non-negative value",
                    "out_of_range" => "Value is out of range",
                    "non_finite" => "Enter a finite number",
                    _ => "Check this value",
                })
        })
    }
    pub fn edit_number(
        &mut self,
        path: &str,
        text: String,
        assign: impl FnOnce(&mut RecipeDocumentV1, Option<f64>),
    ) {
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
        self.generation = self.preview.invalidate();
    }
    pub fn set_validation(&mut self, validation: krausen_types::validation::Validation) {
        self.diagnostics = validation.diagnostics;
    }
    pub fn diagnostics(&self) -> &[krausen_types::validation::Diagnostic] {
        &self.diagnostics
    }
    pub fn remove_number_fields(&mut self, prefix: &str) {
        self.raw_numbers.retain(|path, _| !path.starts_with(prefix));
        self.malformed.retain(|path, _| !path.starts_with(prefix));
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
        let prior = draft.preview.invalidate();
        draft.edit_number("batch_size_l", "10".into(), |doc, value| {
            doc.batch_size_l = value
        });
        assert!(draft.diagnostics().is_empty());
        assert!(!draft.preview.accept(prior, "old", None));
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
}
