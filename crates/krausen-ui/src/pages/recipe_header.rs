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
        BreweryDraftField, BreweryDraftSelect, BrewerySaveStatus,
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

/// Failure wording the editor reports through its one status string.
pub fn is_failure_message(message: &str) -> bool {
    let lower = message.to_lowercase();
    ["could not", "unable", "failed", "unavailable", "denied", "expired", "error"].iter().any(|word| lower.contains(word))
}

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
        EquipmentSource::Unavailable { .. } => summary.identity(),
        _ => format!("{} · {}", summary.identity(), summary.overrides()),
    }
}

#[component]
pub fn RecipeHeader(
    draft: RwSignal<RecipeDraft>,
    revision: RwSignal<Option<i64>>,
    status: RwSignal<String>,
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
    /// Batch-into-fermenter and boil-time fields; raw-input behavior stays with the editor.
    /// Boil time keeps this single owner until the process stages take it over.
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
        } else if is_failure_message(&status.get()) {
            SaveState::Error
        } else if dirty.get() || !saved.get() {
            SaveState::Unsaved
        } else {
            SaveState::Saved
        }
    });
    let message = Signal::derive(move || {
        let message = status.get();
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
                <nav class="recipe-header__breadcrumb" aria-label="Recipe breadcrumb">
                    <a href="/recipes">"Recipes"</a><span aria-hidden="true">"/"</span>
                    <span>{crumb}</span>
                </nav>
                <div class="recipe-header__commands" role="group" aria-label="Recipe commands">
                    <span class="recipe-header__status"><BrewerySaveStatus state=state message=message/></span>
                    <Button variant=ButtonVariant::Secondary disabled=Signal::derive(move || pending.get() || !draft.with(|d| d.can_undo())) on:click=move |_| actions.undo.run(())>"Undo"</Button>
                    <Button variant=save_variant class="min-w-[9rem]" disabled=pending on:click=move |_| actions.save.run(())>{move || save_label(saved.get())}</Button>
                    <Button variant=start_variant aria_describedby=Signal::derive(move || reason.get().map(|_| reason_id.to_owned())) on:click=move |_| actions.start.run(())>"Start brew"</Button>
                    <BreweryPopover id="recipe-actions" trigger_id="recipe-actions-trigger" label="Recipe actions" open=actions_open>
                        <Button variant=ButtonVariant::Ghost disabled=Signal::derive(move || !can_transform.get()) on:click=close_then(actions.scale)>"Scale…"</Button>
                        <Button variant=ButtonVariant::Ghost disabled=Signal::derive(move || !can_transform.get()) on:click=close_then(actions.duplicate)>"Duplicate…"</Button>
                        <Button variant=ButtonVariant::Ghost on:click=close_then(actions.print)>"Print…"</Button>
                        <Button variant=ButtonVariant::Ghost on:click=close_then(actions.export)>"Export…"</Button>
                        <Show when=move || !can_transform.get()>
                            <p class="note-caption">"Scale and Duplicate use the saved recipe. Save your changes first."</p>
                        </Show>
                    </BreweryPopover>
                </div>
            </div>
            <h1 id="recipe-editor-heading" class="recipe-header__heading">{crumb}</h1>
            <BreweryDraftField id="recipe-title" label="Recipe name" value=title on_change=on_title title_variant=true/>
            <div class="recipe-header__context">
                <div class="recipe-header__batch">{children()}</div>
                {move || view! { <BreweryDraftSelect id="recipe-style" label="Beer style" value=style_value on_change=on_style options=style_options.get()/> }}
                <div class="recipe-header__equipment">
                    <span class="k-field__label">"Equipment"</span>
                    <div class="recipe-header__equipment-body">
                        <BreweryBadge tone="accent">{move || equipment_context(&equipment.get())}</BreweryBadge>
                        <Button variant=ButtonVariant::Ghost size=crate::components::ButtonSize::Sm on:click=move |_| actions.equipment.run(())>"Change equipment"</Button>
                    </div>
                </div>
            </div>
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
    fn failure_wording_is_recognised_without_flagging_success() {
        assert!(is_failure_message("Could not discard the retained local draft."));
        assert!(is_failure_message("Preview unavailable."));
        assert!(!is_failure_message("Saved revision 3"));
        assert!(!is_failure_message("Scaled recipe saved as revision 4."));
        assert!(!is_failure_message("Draft retained against the current revision. Review it, then choose Save."));
    }

    #[test]
    fn equipment_context_names_source_and_overrides_without_inventing_unavailable_counts() {
        let recipe_only = EquipmentSummary::default();
        assert_eq!(equipment_context(&recipe_only), "Recipe-only equipment · No recipe overrides");
        let missing = EquipmentSummary { source: EquipmentSource::Unavailable { id: "p".into() }, override_count: 3 };
        assert_eq!(equipment_context(&missing), "Equipment profile unavailable");
    }
}
