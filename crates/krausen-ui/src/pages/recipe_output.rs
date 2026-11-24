// SPDX-License-Identifier: AGPL-3.0-or-later
//! Source-bound recipe output and transform tasks (INT-008, IA-009).
//!
//! Print and Export open on demand from Recipe actions. This module owns the
//! decisions that do not need a browser: which source an output task is bound
//! to, whether the user must choose, and why Scale/Duplicate cannot start yet.
//! The workflows themselves stay with their existing owners and shared
//! application operations.

/// The exact source an output is rendered from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputSource {
    /// An immutable saved revision of this recipe.
    Saved { revision: i64 },
    /// The current editable draft, explicitly labelled unsaved.
    Draft,
}

impl OutputSource {
    /// Text that names the source wherever it is shown or announced.
    pub fn label(self) -> String {
        match self {
            Self::Saved { revision } => format!("Saved revision {revision}"),
            Self::Draft => "Unsaved draft".to_owned(),
        }
    }
}

/// Whether the task knows its source or must ask for a short choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceChoice {
    /// One possible source: no choice is offered.
    Known(OutputSource),
    /// The saved head and the unsaved draft both exist and differ.
    Choose { saved: OutputSource, draft: OutputSource },
}

/// Resolve the source of Print/Export.
///
/// * `head` is the saved revision the editor is based on; `None` for a recipe
///   that has never been saved.
/// * `dirty` says the draft differs from that saved revision.
/// * `selected` is a saved version the user inspected in Saved versions. It
///   supplies its own known source, so no raw revision number is asked for and
///   the draft is neither used nor discarded.
pub fn resolve_source(head: Option<i64>, dirty: bool, selected: Option<i64>) -> SourceChoice {
    if let Some(revision) = selected {
        return SourceChoice::Known(OutputSource::Saved { revision });
    }
    match (head, dirty) {
        (None, _) => SourceChoice::Known(OutputSource::Draft),
        (Some(revision), false) => SourceChoice::Known(OutputSource::Saved { revision }),
        (Some(revision), true) => SourceChoice::Choose {
            saved: OutputSource::Saved { revision },
            draft: OutputSource::Draft,
        },
    }
}

/// Why Scale and Duplicate cannot start from this state; `None` when they can.
/// Both operate on the clean saved recipe and never save implicitly.
pub fn transform_prerequisite(saved: bool, dirty: bool) -> Option<&'static str> {
    if !saved {
        Some("Scale and Duplicate use a saved recipe. Save this recipe first; nothing is saved for you.")
    } else if dirty {
        Some("Scale and Duplicate use the saved revision, not your unsaved changes. Save your changes first; nothing is saved for you.")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_never_saved_recipe_has_only_the_draft_as_source() {
        assert_eq!(resolve_source(None, false, None), SourceChoice::Known(OutputSource::Draft));
        assert_eq!(resolve_source(None, true, None), SourceChoice::Known(OutputSource::Draft));
    }

    #[test]
    fn a_clean_saved_recipe_is_bound_to_its_revision_without_a_choice() {
        assert_eq!(resolve_source(Some(3), false, None), SourceChoice::Known(OutputSource::Saved { revision: 3 }));
    }

    #[test]
    fn only_a_dirty_saved_recipe_asks_for_a_source() {
        assert_eq!(
            resolve_source(Some(3), true, None),
            SourceChoice::Choose { saved: OutputSource::Saved { revision: 3 }, draft: OutputSource::Draft }
        );
    }

    #[test]
    fn a_selected_saved_version_supplies_its_own_source_even_with_a_dirty_draft() {
        for dirty in [false, true] {
            assert_eq!(
                resolve_source(Some(5), dirty, Some(2)),
                SourceChoice::Known(OutputSource::Saved { revision: 2 })
            );
        }
    }

    #[test]
    fn source_labels_name_revision_or_unsaved_draft() {
        assert_eq!(OutputSource::Saved { revision: 4 }.label(), "Saved revision 4");
        assert_eq!(OutputSource::Draft.label(), "Unsaved draft");
    }

    #[test]
    fn transforms_explain_every_unmet_prerequisite_and_never_offer_a_save() {
        assert!(transform_prerequisite(true, false).is_none());
        assert!(transform_prerequisite(false, false).unwrap().contains("Save this recipe first"));
        assert!(transform_prerequisite(true, true).unwrap().contains("unsaved changes"));
        for dirty in [false, true] {
            if let Some(text) = transform_prerequisite(false, dirty) {
                assert!(text.contains("nothing is saved for you"));
            }
        }
    }
}
