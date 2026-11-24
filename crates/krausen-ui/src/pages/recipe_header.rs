// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recipe header: one directly editable identity, formulation context and stable
//! commands (UX-001, IA-006, IA-009, VIS-003/004).
//!
//! Presentation only. Save, Start, Scale, Duplicate, Print and Export workflows
//! stay with their existing owners; this component places their entry points and
//! exposes [`RecipeActionSlots`] so later source-bound tasks can be wired without
//! another header change. Nothing here saves or starts automatically.
use crate::{
    components::{
        BreweryActionGroup, BreweryDraftField, BreweryDraftSelect, BrewerySaveStatus,
        BreweryPopover, Button, ButtonVariant, SaveState,
    },
    pages::{
        recipe_draft::RecipeDraft,
        recipe_equipment::{EquipmentSource, EquipmentSummary},
    },
};
use crate::components::brewery_kit::BreweryBadge;
use krausen_types::validation::Readiness;
use leptos::prelude::*;

/// The one copper action for the current editor state (UX-001).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimaryAction {
    Save,
    Start,
    /// Clean, saved but not Ready: nothing to save and nothing to start yet.
    Neither,
}

/// Save is copper only while there is something to save; Start brew only for a
/// clean saved Ready recipe. Unknown readiness never promotes Start.
pub fn primary_action(saved: bool, dirty: bool, readiness: Option<Readiness>) -> PrimaryAction {
    if !saved || dirty {
        PrimaryAction::Save
    } else if readiness == Some(Readiness::Ready) {
        PrimaryAction::Start
    } else {
        PrimaryAction::Neither
    }
}

/// What kind of failure the editor is currently reporting. Stored with the text so
/// a later success only retires the class it actually supersedes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// Saving, validation or preview: retired by the next save outcome.
    Operation,
    /// Local draft retention/removal: retired when the retried storage step succeeds.
    Storage,
}

/// The editor's single user-facing status line with a typed outcome: `set`
/// reports progress or success, `fail` reports an operation failure and
/// `fail_storage` a local-storage failure.
#[derive(Clone, Copy)]
pub struct EditorStatus {
    pub text: RwSignal<String>,
    failed: RwSignal<Option<FailureKind>>,
}

impl EditorStatus {
    pub fn new() -> Self {
        Self { text: RwSignal::new(String::new()), failed: RwSignal::new(None) }
    }
    pub fn set(&self, message: String) {
        self.failed.set(None);
        self.text.set(message);
    }
    pub fn fail(&self, message: String) {
        self.failed.set(Some(FailureKind::Operation));
        self.text.set(message);
    }
    /// A local draft could not be retained or removed; only storage success clears it.
    pub fn fail_storage(&self, message: String) {
        self.failed.set(Some(FailureKind::Storage));
        self.text.set(message);
    }
    pub fn failed(&self) -> bool {
        self.failed.get().is_some()
    }
    /// Retire a local-storage failure that a later write/removal made obsolete.
    /// Operation failures keep their message until the next outcome.
    pub fn resolve_storage_failure(&self) {
        if self.failed.get_untracked() == Some(FailureKind::Storage) {
            self.failed.set(None);
            self.text.set(String::new());
        }
    }
}

impl Default for EditorStatus {
    fn default() -> Self {
        Self::new()
    }
}

/// Stable id of the editor page heading: the focus successor for several tasks and
/// the fallback of a diagnostic jump.
pub(crate) const EDITOR_HEADING_ID: &str = "recipe-editor-heading";

pub fn save_label(saved: bool) -> &'static str {
    if saved { "Save changes" } else { "Save recipe" }
}

/// Why Start brew cannot proceed yet; `None` when nothing blocks it.
pub fn start_reason(saved: bool, dirty: bool, readiness: Option<Readiness>) -> Option<&'static str> {
    if !saved {
        Some("Save this recipe before starting a brew.")
    } else if dirty {
        Some("Save your changes before starting a brew.")
    } else {
        match readiness {
            Some(Readiness::Ready) => None,
            Some(Readiness::Incomplete) => {
                Some("Starting needs a complete recipe. Review the missing inputs first.")
            }
            Some(Readiness::Invalid) => Some("Correct the highlighted inputs before starting."),
            None => Some("Starting waits for the current estimate."),
        }
    }
}

/// Stable entry points for recipe commands. Print and Export are intentionally
/// callbacks: their source-bound task workflows are owned elsewhere.
#[derive(Clone, Copy)]
pub struct RecipeActionSlots {
    /// Go to the first control that must be corrected before the recipe can be saved.
    pub review: Callback<()>,
    pub save: Callback<()>,
    pub undo: Callback<()>,
    pub start: Callback<()>,
    pub scale: Callback<()>,
    pub duplicate: Callback<()>,
    pub print: Callback<()>,
    pub export: Callback<()>,
    /// Reveal the equipment selector owned by the equipment section.
    pub equipment: Callback<()>,
}

fn equipment_context(summary: &EquipmentSummary) -> String {
    match &summary.source {
        EquipmentSource::Loading { .. } => "Selected equipment profile · loading".to_owned(),
        EquipmentSource::Unavailable { .. } => "Selected profile unavailable".to_owned(),
        _ => format!("{} · {}", summary.identity(), summary.overrides()),
    }
}

#[component]
pub fn RecipeHeader(
    draft: RwSignal<RecipeDraft>,
    revision: RwSignal<Option<i64>>,
    status: EditorStatus,
    retained: RwSignal<bool>,
    pending: RwSignal<bool>,
    conflict: Signal<bool>,
    equipment: RwSignal<EquipmentSummary>,
    title: Signal<String>,
    on_title: Callback<String>,
    style_value: Signal<String>,
    style_options: Signal<Vec<(String, String)>>,
    on_style: Callback<String>,
    actions: RecipeActionSlots,
    actions_open: RwSignal<bool>,
    /// Batch-into-fermenter field; raw-input behavior stays with the editor. Boil
    /// time belongs to the Boil & whirlpool process section.
    children: Children,
) -> impl IntoView {
    let saved = Signal::derive(move || revision.get().is_some());
    let dirty = Signal::derive(move || draft.with(|d| d.dirty));
    let readiness = Signal::derive(move || draft.with(|d| d.readiness()));
    let primary = Signal::derive(move || primary_action(saved.get(), dirty.get(), readiness.get()));
    let save_variant = Signal::derive(move || {
        if primary.get() == PrimaryAction::Save { ButtonVariant::Default } else { ButtonVariant::Secondary }
    });
    let start_variant = Signal::derive(move || {
        if primary.get() == PrimaryAction::Start { ButtonVariant::Default } else { ButtonVariant::Secondary }
    });
    let reason = Signal::derive(move || start_reason(saved.get(), dirty.get(), readiness.get()));
    let state = Signal::derive(move || {
        if pending.get() {
            SaveState::Saving
        } else if conflict.get() {
            SaveState::Conflict
        } else if status.failed() {
            SaveState::Error
        } else if dirty.get() || !saved.get() {
            SaveState::Unsaved
        } else {
            SaveState::Saved
        }
    });
    let message = Signal::derive(move || {
        let message = status.text.get();
        if dirty.get() && (message.is_empty() || message.starts_with("Saved revision")) {
            let local = if retained.get() { "Unsaved changes · retained locally" } else { "Unsaved changes" };
            if message.is_empty() { local.to_owned() } else { format!("{message} {local}") }
        } else if message.is_empty() && !saved.get() {
            "Not saved yet".to_owned()
        } else {
            message
        }
    });
    let can_transform = Signal::derive(move || !pending.get() && saved.get() && !dirty.get());
    let crumb = move || {
        let name = title.get();
        if name.is_empty() { "Recipe editor".to_owned() } else { name }
    };
    let close_then = move |command: Callback<()>| {
        move |_| {
            actions_open.set(false);
            command.run(());
        }
    };
    let reason_id = "recipe-start-reason";
    view! {
        <header class="recipe-header">
            <div class="recipe-header__topbar">
                <crate::components::wayfinding::Breadcrumb items=Signal::derive(move || vec![
                    crate::components::wayfinding::Crumb::link("Recipes", "/recipes"),
                    crate::components::wayfinding::Crumb::current(crumb()),
                ])/>
                <BreweryActionGroup end=true label="Recipe commands">
                    <span class="recipe-header__status"><BrewerySaveStatus state=state message=message single_line=true/></span>
                    <Button variant=ButtonVariant::Secondary disabled=Signal::derive(move || pending.get() || !draft.with(|d| d.can_undo())) on:click=move |_| actions.undo.run(())>"Undo"</Button>
                    <Button variant=save_variant class="min-w-[9rem]" disabled=pending on:click=move |_| actions.save.run(())>{move || save_label(saved.get())}</Button>
                    <Button id="recipe-start-command" variant=start_variant aria_describedby=Signal::derive(move || reason.get().map(|_| reason_id.to_owned())) on:click=move |_| actions.start.run(())>"Start brew"</Button>
                    <BreweryPopover id="recipe-actions" trigger_id="recipe-actions-trigger" label="Recipe actions" open=actions_open>
                        <Button variant=ButtonVariant::Ghost disabled=Signal::derive(move || !can_transform.get()) on:click=close_then(actions.scale)>"Scale…"</Button>
                        <Button variant=ButtonVariant::Ghost disabled=Signal::derive(move || !can_transform.get()) on:click=close_then(actions.duplicate)>"Duplicate…"</Button>
                        <Button variant=ButtonVariant::Ghost on:click=close_then(actions.print)>"Print…"</Button>
                        <Button variant=ButtonVariant::Ghost on:click=close_then(actions.export)>"Export…"</Button>
                        <Show when=move || !can_transform.get()>
                            <p class="note-caption">"Scale and Duplicate use the saved recipe. Save your changes first."</p>
                        </Show>
                    </BreweryPopover>
                </BreweryActionGroup>
            </div>
            <h1 id=EDITOR_HEADING_ID class="recipe-header__heading" tabindex="-1">{crumb}</h1>
            <BreweryDraftField id="recipe-title" label="Recipe name" value=title on_change=on_title title_variant=true/>
            <div class="recipe-header__context">
                <div class="recipe-header__batch">{children()}<span id="recipe-batch-note" class="k-field__hint recipe-header__hint">"Batch is the volume into the fermenter"</span></div>
                {move || view! { <BreweryDraftSelect id="recipe-style" label="Beer style" value=style_value on_change=on_style options=style_options.get()/> }}
                <div class="recipe-header__equipment">
                    <span class="k-field__label">"Equipment"</span>
                    <div class="recipe-header__equipment-body">
                        <BreweryBadge tone="accent">{move || equipment_context(&equipment.get())}</BreweryBadge>
                        <Button variant=ButtonVariant::Ghost size=crate::components::ButtonSize::Sm on:click=move |_| actions.equipment.run(())>"Change equipment"</Button>
                    </div>
                </div>
            </div>
            <Show when=move || draft.with(|d| crate::recipe_diagnostics::first_invalid(d).is_some())>
                <div class="recipe-header__review">
                    <Button variant=ButtonVariant::Secondary on:click=move |_| actions.review.run(())>"Review highlighted fields"</Button>
                </div>
            </Show>
            <Show when=move || reason.get().is_some()>
                <p id=reason_id class="recipe-header__reason">{move || reason.get()}</p>
            </Show>
        </header>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_follows_state_and_unknown_readiness_never_promotes_start() {
        use Readiness::*;
        assert_eq!(primary_action(false, false, Some(Ready)), PrimaryAction::Save);
        assert_eq!(primary_action(true, true, Some(Ready)), PrimaryAction::Save);
        assert_eq!(primary_action(true, false, Some(Incomplete)), PrimaryAction::Neither);
        assert_eq!(primary_action(true, false, Some(Invalid)), PrimaryAction::Neither);
        assert_eq!(primary_action(true, false, None), PrimaryAction::Neither);
        assert_eq!(primary_action(false, false, None), PrimaryAction::Save);
        assert_eq!(primary_action(true, false, Some(Ready)), PrimaryAction::Start);
    }

    #[test]
    fn labels_and_reasons_explain_each_blocked_start() {
        use Readiness::*;
        assert_eq!(save_label(false), "Save recipe");
        assert_eq!(save_label(true), "Save changes");
        assert!(start_reason(false, false, None).unwrap().contains("Save this recipe"));
        assert!(start_reason(true, true, Some(Ready)).unwrap().contains("Save your changes"));
        assert!(start_reason(true, false, Some(Incomplete)).unwrap().contains("complete"));
        assert!(start_reason(true, false, Some(Invalid)).unwrap().contains("highlighted"));
        assert!(start_reason(true, false, None).unwrap().contains("estimate"));
        assert_eq!(start_reason(true, false, Some(Ready)), None);
    }

    #[test]
    fn typed_status_outcome_never_outlives_its_message() {
        let owner = leptos::reactive::owner::Owner::new();
        owner.with(|| {
            let status = EditorStatus::new();
            assert!(!status.failed());
            status.fail("Could not save.".into());
            assert!(status.failed());
            status.set("Saved revision 3".into());
            assert!(!status.failed());
            assert_eq!(status.text.get_untracked(), "Saved revision 3");
            status.fail("Complete invalid number fields before saving.".into());
            status.resolve_storage_failure();
            assert!(status.failed(), "a save failure is not a storage failure");
            for storage in [
                "Unable to retain this draft locally; keep this page open.",
                "An older retained draft could not be removed from this browser.",
                "Local draft recovery is unavailable in this browser.",
            ] {
                status.fail_storage(storage.into());
                status.resolve_storage_failure();
                assert!(!status.failed(), "{storage}");
                assert_eq!(status.text.get_untracked(), "");
            }
        });
    }

    #[test]
    fn equipment_context_names_source_and_overrides_without_inventing_unavailable_counts() {
        let recipe_only = EquipmentSummary::default();
        assert_eq!(equipment_context(&recipe_only), "Recipe-only equipment · No recipe overrides");
        let loading = EquipmentSummary { source: EquipmentSource::Loading { id: "p".into() }, override_count: 0 };
        assert_eq!(equipment_context(&loading), "Selected equipment profile · loading");
        let missing = EquipmentSummary { source: EquipmentSource::Unavailable { id: "p".into() }, override_count: 3 };
        assert_eq!(equipment_context(&missing), "Selected profile unavailable");
    }
}
