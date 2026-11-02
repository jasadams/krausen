// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared recipe brew history, frozen comparison and explicit feedback views.
use super::brew_history_presentation::{
    actor_text, change_actual_text, change_kind_label, change_planned_text, change_section_text,
    diagnostic_text, format_timestamp, humanize_words, item_label, measurement_status_text,
    measurement_value_or, revision_text, selected_reading_context, status_label, validation_label,
};
use crate::{
    components::{
        BreweryDisclosure, BreweryMetric, BreweryPanel, Button, ButtonLink, ButtonVariant,
        Checkbox, PrintableRecipe,
        brew_evidence_rows::{BrewMeasurementRow, BrewProvenanceFact},
    },
    pages::recipe_editor::current_context_matches,
    server_fns::brew_history::{
        brew_comparison_shared, brew_feedback_apply_shared, brew_feedback_preview_shared,
        brew_history_shared, brew_print_shared,
    },
};
use krausen_types::{
    brew_history::{
        BrewComparison, BrewComparisonOutcome, BrewComparisonRequest, BrewFeedbackApplyOutcome,
        BrewFeedbackApplyRequest, BrewFeedbackPreview, BrewFeedbackPreviewOutcome,
        BrewFeedbackPreviewRequest, BrewPrint, BrewPrintOutcome, BrewPrintRequest,
        RecipeBrewHistoryOutcome, RecipeBrewHistoryQuery, RecipeBrewSummary,
    },
    request_context::OperationIntent,
};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use std::collections::BTreeSet;

#[component]
pub fn RecipeBrewHistoryPage() -> impl IntoView {
    view! { <BrewHistoryView/> }
}

/// Reusable history presentation. `embedded` omits the page header/panel for an
/// editor tab; `recipe_id` supplies the trusted current recipe after a first
/// save, because route params may still name a draft placeholder.
#[component]
pub fn BrewHistoryView(
    #[prop(optional)] embedded: bool,
    #[prop(optional, into)] recipe_id: Option<String>,
    #[prop(optional)] intent: Option<OperationIntent>,
    /// Embedded callers use this to redact their still-mounted parent draft on
    /// authority/context loss. `try_run` makes a disposed parent a no-op.
    #[prop(optional)]
    on_authority_failure: Option<Callback<()>>,
) -> impl IntoView {
    let params = use_params_map();
    let route_recipe = Signal::derive(move || params.read().get("id").unwrap_or_default());
    let explicit_recipe = recipe_id;
    let recipe_id = Signal::derive(move || {
        explicit_recipe
            .clone()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| route_recipe.get())
    });
    let captured_intent = intent;
    let records = RwSignal::new(Vec::<RecipeBrewSummary>::new());
    let identity = RwSignal::new(None::<OperationIntent>);
    let next_cursor = RwSignal::new(None::<String>);
    let message = RwSignal::new("Loading brew history…".to_owned());
    let busy = RwSignal::new(false);
    let generation = RwSignal::new(0_u64);

    let clear_page = move || {
        records.set(Vec::new());
        next_cursor.set(None);
    };

    let load_page = Callback::new(move |cursor: Option<String>| {
        let recipe = recipe_id.get_untracked();
        let Some(intent) = identity.get_untracked() else {
            return;
        };
        if recipe.is_empty() {
            message.set("Recipe not found.".into());
            return;
        }
        let mine = generation.get_untracked().wrapping_add(1);
        generation.set(mine);
        busy.set(true);
        message.set("Loading brew history…".into());
        leptos::task::spawn_local(async move {
            let outcome = brew_history_shared(RecipeBrewHistoryQuery {
                intent: intent.clone(),
                recipe_id: recipe.clone(),
                include_archived: true,
                cursor: cursor.clone(),
                limit: 50,
            })
            .await;
            let context_ok = current_context_matches(&intent).await;
            let denied = !context_ok
                || matches!(
                    &outcome,
                    Ok(RecipeBrewHistoryOutcome::Forbidden)
                        | Ok(RecipeBrewHistoryOutcome::ContextChanged)
                );
            if denied {
                if let Some(callback) = on_authority_failure.clone() {
                    let _ = callback.try_run(());
                }
            }
            if generation.try_get_untracked() != Some(mine)
                || recipe_id.try_get_untracked() != Some(recipe)
            {
                return;
            }
            busy.set(false);
            if !context_ok {
                clear_page();
                message.set("Workspace changed; reload.".into());
                return;
            }
            match outcome {
                Ok(RecipeBrewHistoryOutcome::History(page)) => {
                    if cursor.is_some() {
                        records.update(|items| items.extend(page.items));
                    } else {
                        records.set(page.items);
                    }
                    next_cursor.set(page.next_cursor);
                    if records.with(|items| items.is_empty()) {
                        message.set("No brews have been started for this recipe.".into());
                    } else {
                        message.set(String::new());
                    }
                }
                Ok(RecipeBrewHistoryOutcome::NotFound) => {
                    clear_page();
                    message.set("Recipe not found.".into())
                }
                Ok(RecipeBrewHistoryOutcome::Forbidden) => {
                    clear_page();
                    message.set("You no longer have access to this workspace.".into())
                }
                Ok(RecipeBrewHistoryOutcome::ContextChanged) => {
                    clear_page();
                    message.set("Workspace changed; reload.".into())
                }
                Ok(RecipeBrewHistoryOutcome::InvalidRequest) => {
                    clear_page();
                    message.set("The history request was invalid.".into())
                }
                Ok(RecipeBrewHistoryOutcome::Unavailable) => {
                    clear_page();
                    message.set("Brew history is unavailable; try again.".into())
                }
                Err(_) => {
                    clear_page();
                    message.set("Brew history is unavailable; try again.".into())
                }
            }
        });
    });

    Effect::new(move |_| {
        let recipe = recipe_id.get();
        clear_page();
        if recipe.is_empty() {
            busy.set(false);
            message.set("Recipe not found.".into());
            return;
        }
        if embedded && recipe == "new" {
            busy.set(false);
            message.set("Save this recipe before viewing brew history.".into());
            return;
        }
        if embedded && captured_intent.is_none() {
            busy.set(false);
            message.set("Open brew history from the saved recipe editor.".into());
            return;
        }
        busy.set(true);
        message.set("Loading brew history…".into());
        let mine = generation.get_untracked().wrapping_add(1);
        generation.set(mine);
        if let Some(expected) = captured_intent.clone() {
            // Embedded callers supply the editor's captured account/workspace.
            // Verify it against the current authenticated context instead of
            // adopting a newly selected workspace.
            identity.set(Some(expected.clone()));
            leptos::task::spawn_local(async move {
                let context_ok = current_context_matches(&expected).await;
                if !context_ok {
                    if let Some(callback) = on_authority_failure.clone() {
                        let _ = callback.try_run(());
                    }
                }
                if generation.try_get_untracked() != Some(mine)
                    || recipe_id.try_get_untracked() != Some(recipe)
                {
                    return;
                }
                if !context_ok {
                    busy.set(false);
                    message.set("Workspace changed; reload.".into());
                    return;
                }
                load_page.run(None);
            });
            return;
        }
        leptos::task::spawn_local(async move {
            let context = crate::server_fns::context::get_user_context().await;
            if context.is_err() {
                if let Some(callback) = on_authority_failure.clone() {
                    let _ = callback.try_run(());
                }
            }
            if generation.try_get_untracked() != Some(mine)
                || recipe_id.try_get_untracked() != Some(recipe)
            {
                return;
            }
            let Ok(ctx) = context else {
                busy.set(false);
                message.set("Sign in to view brew history.".into());
                return;
            };
            let Some(workspace_id) = ctx.workspace_id else {
                busy.set(false);
                message.set("Choose a workspace.".into());
                return;
            };
            identity.set(Some(OperationIntent {
                account_id: ctx.user_id,
                workspace_id,
            }));
            load_page.run(None);
        });
    });

    let current_actor = Signal::derive(move || identity.get().map(|value| value.account_id));
    let history_body = view! {
        <p role="status">{move || message.get()}</p>
        <div class="max-w-full overflow-x-auto" tabindex="0" role="region" aria-label="Brew history table">
        <table class="print-sheet__table">
            <thead>
                <tr>
                    <th scope="col">"Status"</th>
                    <th scope="col">"Started"</th>
                    <th scope="col">"Plan"</th>
                    <th scope="col">"Evidence"</th>
                    <th scope="col"><span class="sr-only">"Actions"</span></th>
                </tr>
            </thead>
            <tbody>
                {move || records.get().into_iter().map(|record| {
                    let brew_id = record.brew_id.clone();
                    let analysis = brew_id.clone();
                    let evidence = brew_id.clone();
                    let started = format_timestamp(record.started_at);
                    let actor = actor_text(&record.started_by, current_actor.get().as_deref());
                    let evidence_text = record
                        .actual_revision
                        .map(|revision| format!("Actual formulation revision {revision}"))
                        .unwrap_or_else(|| "Planned formulation only".to_owned());
                    view! {
                        <tr>
                            <td>
                                <span class="print-sheet__name">{status_label(record.status)}</span>
                                {record.archived.then(|| view! { <span class="print-sheet__sub">"Archived"</span> })}
                            </td>
                            <td>
                                <span class="print-sheet__name">{started}</span>
                                <span class="print-sheet__sub">{format!("by {actor}")}</span>
                            </td>
                            <td>
                                <span class="print-sheet__name">{format!("Recipe revision {}", record.selected_recipe_revision)}</span>
                                <span class="print-sheet__sub">{format!("Brew revision {}", record.revision)}</span>
                            </td>
                            <td><span class="print-sheet__sub">{evidence_text}</span></td>
                            <td>
                                <div class="recipe-editor__actions">
                                    <ButtonLink href=format!("/brews/{analysis}/analysis") variant=ButtonVariant::Secondary>"Compare and feedback"</ButtonLink>
                                    <ButtonLink href=format!("/brews/{evidence}") variant=ButtonVariant::Ghost>"Open evidence"</ButtonLink>
                                </div>
                            </td>
                        </tr>
                    }
                }).collect_view()}
            </tbody>
        </table>
        </div>
        <Show when=move || next_cursor.get().is_some()>
            <p class="note-caption">{move || format!("Showing {} brews; more are retained.", records.with(|items| items.len()))}</p>
            <Button variant=ButtonVariant::Secondary disabled=busy on:click=move |_| load_page.run(next_cursor.get_untracked())>"Load more"</Button>
        </Show>
        <Show when=move || !busy.get() && next_cursor.get().is_none() && !records.with(|items| items.is_empty())>
            <p class="note-caption">"End of retained brew history."</p>
        </Show>
        {embedded.then(|| view! {
            <div class="recipe-editor__actions">
                <ButtonLink href=format!("/recipes/{}/brews", recipe_id.get_untracked()) variant=ButtonVariant::Secondary>"View full brew history"</ButtonLink>
            </div>
        })}
    };
    if embedded {
        view! { <div class="recipe-editor__sections">{history_body}</div> }.into_any()
    } else {
        view! {
            <div class="recipe-editor">
                <header class="recipe-editor__header">
                    <div>
                        <p class="kit-showcase__eyebrow">"Your workspace"</p>
                        <h1>"Brew history"</h1>
                        <p>"Stored plans and optional actual evidence remain separate from the current recipe."</p>
                    </div>
                </header>
                <BreweryPanel title="Brews" description="Immutable plans and retained evidence for this recipe.">
                    {history_body}
                </BreweryPanel>
            </div>
        }
        .into_any()
    }
}

#[component]
pub fn BrewAnalysisPage() -> impl IntoView {
    let params = use_params_map();
    let brew_id = Signal::derive(move || params.read().get("id").unwrap_or_default());
    let identity = RwSignal::new(None::<OperationIntent>);
    let comparison = RwSignal::new(None::<BrewComparison>);
    let preview = RwSignal::new(None::<BrewFeedbackPreview>);
    let print = RwSignal::new(None::<BrewPrint>);
    let selected = RwSignal::new(BTreeSet::<String>::new());
    let message = RwSignal::new("Loading comparison…".to_owned());
    let busy = RwSignal::new(false);
    let retry = RwSignal::new(None::<BrewFeedbackApplyRequest>);
    let generation = RwSignal::new(0_u64);
    let current_brew = RwSignal::new(String::new());

    let hide_brew_content = move || {
        comparison.set(None);
        preview.set(None);
        print.set(None);
        selected.set(BTreeSet::new());
    };

    let load_comparison = Callback::new(move |()| {
        let brew = current_brew.get_untracked();
        let Some(intent) = identity.get_untracked() else {
            return;
        };
        if brew.is_empty() {
            message.set("Brew not found.".into());
            return;
        }
        let mine = generation.get_untracked().wrapping_add(1);
        generation.set(mine);
        preview.set(None);
        print.set(None);
        selected.set(BTreeSet::new());
        busy.set(true);
        message.set("Refreshing comparison…".into());
        leptos::task::spawn_local(async move {
            let owned = || {
                generation.try_get_untracked() == Some(mine)
                    && current_brew.try_get_untracked().as_deref() == Some(brew.as_str())
            };
            let context_ok = current_context_matches(&intent).await;
            if !owned() {
                return;
            }
            if !context_ok {
                comparison.set(None);
                preview.set(None);
                print.set(None);
                selected.set(BTreeSet::new());
                busy.set(false);
                message.set("Workspace changed; reload.".into());
                return;
            }
            let outcome = brew_comparison_shared(BrewComparisonRequest {
                intent: intent.clone(),
                brew_id: brew.clone(),
            })
            .await;
            let context_ok = current_context_matches(&intent).await;
            if !owned() {
                return;
            }
            if !context_ok {
                comparison.set(None);
                preview.set(None);
                print.set(None);
                selected.set(BTreeSet::new());
                busy.set(false);
                message.set("Workspace changed; reload.".into());
                return;
            }
            busy.set(false);
            match outcome {
                Ok(BrewComparisonOutcome::Comparison(value)) => {
                    comparison.set(Some(value));
                    message.set(String::new());
                }
                Ok(BrewComparisonOutcome::NotFound) => {
                    comparison.set(None);
                    preview.set(None);
                    print.set(None);
                    selected.set(BTreeSet::new());
                    message.set("Brew not found.".into());
                }
                Ok(BrewComparisonOutcome::Forbidden) => {
                    comparison.set(None);
                    preview.set(None);
                    print.set(None);
                    selected.set(BTreeSet::new());
                    message.set("You no longer have access to this workspace.".into())
                }
                Ok(BrewComparisonOutcome::ContextChanged) => {
                    comparison.set(None);
                    preview.set(None);
                    print.set(None);
                    selected.set(BTreeSet::new());
                    message.set("Workspace changed; reload.".into())
                }
                Ok(BrewComparisonOutcome::Unavailable) | Err(_) => {
                    comparison.set(None);
                    preview.set(None);
                    print.set(None);
                    selected.set(BTreeSet::new());
                    message.set("Comparison is unavailable; try again.".into())
                }
            }
        });
    });

    Effect::new(move |_| {
        let brew = brew_id.get();
        current_brew.set(brew.clone());
        hide_brew_content();
        busy.set(false);
        if brew.is_empty() {
            message.set("Brew not found.".into());
            return;
        }
        message.set("Loading comparison…".into());
        let mine = generation.get_untracked().wrapping_add(1);
        generation.set(mine);
        leptos::task::spawn_local(async move {
            let context = crate::server_fns::context::get_user_context().await;
            if generation.try_get_untracked() != Some(mine)
                || current_brew.try_get_untracked().as_deref() != Some(brew.as_str())
            {
                return;
            }
            let Ok(ctx) = context else {
                busy.set(false);
                message.set("Sign in to view this brew.".into());
                return;
            };
            let Some(workspace_id) = ctx.workspace_id else {
                busy.set(false);
                message.set("Choose a workspace.".into());
                return;
            };
            identity.set(Some(OperationIntent {
                account_id: ctx.user_id,
                workspace_id,
            }));
            load_comparison.run(());
        });
    });

    let run_preview = Callback::new(move |()| {
        let brew = current_brew.get_untracked();
        if let Some(retained) = retry.get_untracked() {
            message.set(if retained.brew_id == brew {
                "Retry the previous request before starting another.".into()
            } else {
                "An earlier feedback request is unresolved; reopen its brew to retry it.".into()
            });
            return;
        }
        let Some(value) = comparison.get_untracked() else {
            return;
        };
        if selected.get_untracked().is_empty() {
            message.set("Select at least one changed row or field.".into());
            return;
        }
        let Some(intent) = identity.get_untracked() else {
            return;
        };
        if value.brew_id != brew {
            message.set("The comparison changed; refresh before previewing.".into());
            return;
        }
        let change_ids = selected.get_untracked().into_iter().collect::<Vec<_>>();
        let mine = generation.get_untracked().wrapping_add(1);
        generation.set(mine);
        busy.set(true);
        message.set("Preparing feedback preview…".into());
        leptos::task::spawn_local(async move {
            let owned = || {
                generation.try_get_untracked() == Some(mine)
                    && current_brew.try_get_untracked() == Some(brew.clone())
            };
            let context_ok = current_context_matches(&intent).await;
            if !owned() {
                return;
            }
            if !context_ok {
                hide_brew_content();
                busy.set(false);
                message.set("Workspace changed; reload.".into());
                return;
            }
            let outcome = brew_feedback_preview_shared(BrewFeedbackPreviewRequest {
                intent: intent.clone(),
                brew_id: value.brew_id.clone(),
                expected_actual_revision: value.actual.as_ref().map(|actual| actual.revision),
                change_ids,
            })
            .await;
            let context_ok = current_context_matches(&intent).await;
            if !owned() {
                return;
            }
            if !context_ok {
                hide_brew_content();
                busy.set(false);
                message.set("Workspace changed; reload.".into());
                return;
            }
            busy.set(false);
            match outcome {
                Ok(BrewFeedbackPreviewOutcome::Preview(value)) => {
                    preview.set(Some(value));
                    message.set(String::new());
                }
                Ok(BrewFeedbackPreviewOutcome::NoActual) => {
                    preview.set(None);
                    message.set("This brew has no actual formulation amendment.".into())
                }
                Ok(BrewFeedbackPreviewOutcome::NoChanges { diagnostics }) => {
                    preview.set(None);
                    message.set(if diagnostics.is_empty() {
                        "The selected changes would not alter the current recipe.".into()
                    } else {
                        format!("No recipe changes to apply: {}", diagnostics.join("; "))
                    })
                }
                Ok(BrewFeedbackPreviewOutcome::Conflict { expected, actual }) => {
                    preview.set(None);
                    message.set(format!(
                        "The recorded actual formulation changed from {} to {}; reload the comparison.",
                        revision_text(expected),
                        revision_text(actual)
                    ))
                }
                Ok(BrewFeedbackPreviewOutcome::NotFound) => {
                    preview.set(None);
                    message.set("Brew not found.".into())
                }
                Ok(BrewFeedbackPreviewOutcome::Forbidden) => {
                    hide_brew_content();
                    message.set("You no longer have access to this workspace.".into())
                }
                Ok(BrewFeedbackPreviewOutcome::ContextChanged) => {
                    hide_brew_content();
                    message.set("Workspace changed; reload.".into())
                }
                Ok(BrewFeedbackPreviewOutcome::InvalidRequest) => {
                    preview.set(None);
                    message.set("The feedback selection is invalid.".into())
                }
                Ok(BrewFeedbackPreviewOutcome::UnresolvedProfile) => {
                    preview.set(None);
                    message.set("The selected equipment profile is unavailable.".into())
                }
                Ok(BrewFeedbackPreviewOutcome::UnresolvedStyle) => {
                    preview.set(None);
                    message.set("The selected style is unavailable.".into())
                }
                Ok(BrewFeedbackPreviewOutcome::Unavailable) | Err(_) => {
                    preview.set(None);
                    message.set("Feedback preview is unavailable; try again.".into())
                }
            }
        });
    });

    let submit = Callback::new(move |request: BrewFeedbackApplyRequest| {
        let brew = current_brew.get_untracked();
        if request.brew_id != brew {
            message.set("The selected brew changed; reload before retrying.".into());
            return;
        }
        let owner = request.intent.clone();
        let mine = generation.get_untracked().wrapping_add(1);
        generation.set(mine);
        busy.set(true);
        message.set("Applying selected feedback…".into());
        // Retain the immutable command until a definitive outcome. A stale
        // view continuation must not drop a possibly committed request.
        retry.set(Some(request.clone()));
        leptos::task::spawn_local(async move {
            let outcome = brew_feedback_apply_shared(request.clone()).await;
            // Resolve the retained command by identity even if the view moved
            // away; only the owning view updates its content.
            let definitive = match &outcome {
                Ok(BrewFeedbackApplyOutcome::Retryable | BrewFeedbackApplyOutcome::Unavailable)
                | Err(_) => false,
                Ok(_) => true,
            };
            if definitive {
                retry.update(|slot| {
                    if slot
                        .as_ref()
                        .is_some_and(|retained| retained.request_id == request.request_id)
                    {
                        *slot = None;
                    }
                });
            }
            let context_ok = current_context_matches(&owner).await;
            if generation.try_get_untracked() != Some(mine)
                || current_brew.try_get_untracked() != Some(brew.clone())
            {
                return;
            }
            busy.set(false);
            if !context_ok {
                hide_brew_content();
                message.set(
                    "Workspace changed; the retained request can be retried from its brew.".into(),
                );
                return;
            }
            match outcome {
                Ok(BrewFeedbackApplyOutcome::Applied {
                    recipe_revision, ..
                }) => {
                    preview.set(None);
                    selected.set(BTreeSet::new());
                    message.set(format!(
                        "Feedback applied as recipe revision {recipe_revision}. The frozen plan is unchanged."
                    ));
                }
                Ok(BrewFeedbackApplyOutcome::Unchanged {
                    recipe_revision, ..
                }) => {
                    preview.set(None);
                    message.set(format!(
                        "The selected changes are already in recipe revision {recipe_revision}."
                    ));
                }
                Ok(BrewFeedbackApplyOutcome::HeadConflict { expected, actual }) => {
                    message.set(format!(
                        "The recipe head advanced from {expected} to {actual}; reload the comparison."
                    ))
                }
                Ok(BrewFeedbackApplyOutcome::ContextConflict { .. }) => {
                    message.set(
                        "The accepted calculation context changed; preview the current comparison again."
                            .into(),
                    )
                }
                Ok(BrewFeedbackApplyOutcome::ActualConflict { expected, actual }) => {
                    message.set(format!(
                        "The recorded actual formulation changed from {} to {}; reload the comparison.",
                        revision_text(expected),
                        revision_text(actual)
                    ))
                }
                Ok(BrewFeedbackApplyOutcome::NoActual) => {
                    message.set("This brew has no actual formulation amendment.".into())
                }
                Ok(BrewFeedbackApplyOutcome::NoChanges { diagnostics }) => {
                    message.set(if diagnostics.is_empty() {
                        "No recipe revision was needed.".into()
                    } else {
                        format!("No recipe changes to apply: {}", diagnostics.join("; "))
                    })
                }
                Ok(BrewFeedbackApplyOutcome::NotFound) => {
                    message.set("Brew not found.".into())
                }
                Ok(BrewFeedbackApplyOutcome::Forbidden) => {
                    hide_brew_content();
                    message.set("You no longer have access to this workspace.".into())
                }
                Ok(BrewFeedbackApplyOutcome::ContextChanged) => {
                    hide_brew_content();
                    message.set("Workspace changed; reload.".into())
                }
                Ok(BrewFeedbackApplyOutcome::InvalidRequest) => {
                    message.set("The feedback request was invalid.".into())
                }
                Ok(BrewFeedbackApplyOutcome::UnresolvedProfile) => {
                    message.set("The selected equipment profile is unavailable.".into())
                }
                Ok(BrewFeedbackApplyOutcome::UnresolvedStyle) => {
                    message.set("The selected style is unavailable.".into())
                }
                Ok(BrewFeedbackApplyOutcome::Invalid(validation)) => {
                    message.set(format!(
                        "The proposed recipe is {}; review the selected changes.",
                        validation_label(validation.state).to_lowercase()
                    ))
                }
                Ok(BrewFeedbackApplyOutcome::UnresolvedIngredientSource) => {
                    message.set(
                        "A copied ingredient source is no longer valid on the current recipe."
                            .into(),
                    )
                }
                Ok(BrewFeedbackApplyOutcome::RequestIdentityConflict) => {
                    message.set(
                        "This request identity was already used for different feedback.".into(),
                    )
                }
                Ok(BrewFeedbackApplyOutcome::ResultExpired { .. }) => {
                    message.set("This feedback result expired; preview again.".into())
                }
                Ok(BrewFeedbackApplyOutcome::Retryable)
                | Ok(BrewFeedbackApplyOutcome::Unavailable)
                | Err(_) => {
                    message.set("The response was lost; the same request can be retried.".into());
                }
            }
        });
    });

    let apply = Callback::new(move |()| {
        let brew = current_brew.get_untracked();
        if let Some(retained) = retry.get_untracked() {
            if retained.brew_id == brew {
                submit.run(retained);
            } else {
                message.set(
                    "An earlier feedback request is unresolved; reopen its brew to retry it."
                        .into(),
                );
            }
            return;
        }
        let Some(preview_value) = preview.get_untracked() else {
            return;
        };
        if preview_value.brew_id != brew {
            message.set("The selected brew changed; preview again.".into());
            return;
        }
        let Some(intent) = identity.get_untracked() else {
            return;
        };
        let Some(request_id) = crate::utils::request_id::new_request_id() else {
            message.set("Unable to create a request identity.".into());
            return;
        };
        submit.run(BrewFeedbackApplyRequest {
            intent,
            brew_id: preview_value.brew_id.clone(),
            expected_actual_revision: Some(preview_value.source_actual_revision),
            change_ids: preview_value.applied_change_ids.clone(),
            expected_recipe_head: preview_value.base_recipe_head,
            expected_calculation_context: preview_value
                .preview
                .context_digest
                .clone()
                .unwrap_or_default(),
            request_id,
        });
    });

    let load_print = Callback::new(move |()| {
        let Some(intent) = identity.get_untracked() else {
            return;
        };
        let brew = current_brew.get_untracked();
        let mine = generation.get_untracked().wrapping_add(1);
        generation.set(mine);
        busy.set(true);
        message.set("Preparing frozen historical print…".into());
        leptos::task::spawn_local(async move {
            let owned = || {
                generation.try_get_untracked() == Some(mine)
                    && current_brew.try_get_untracked() == Some(brew.clone())
            };
            let context_ok = current_context_matches(&intent).await;
            if !owned() {
                return;
            }
            if !context_ok {
                hide_brew_content();
                busy.set(false);
                message.set("Workspace changed; reload.".into());
                return;
            }
            let outcome = brew_print_shared(BrewPrintRequest {
                intent: intent.clone(),
                brew_id: brew.clone(),
            })
            .await;
            let context_ok = current_context_matches(&intent).await;
            if !owned() {
                return;
            }
            if !context_ok {
                hide_brew_content();
                busy.set(false);
                message.set("Workspace changed; reload.".into());
                return;
            }
            busy.set(false);
            match outcome {
                Ok(BrewPrintOutcome::Print(value)) => {
                    print.set(Some(value));
                    message.set(String::new());
                }
                Ok(BrewPrintOutcome::NotFound) => {
                    print.set(None);
                    message.set("Brew not found.".into())
                }
                Ok(BrewPrintOutcome::Forbidden) => {
                    hide_brew_content();
                    message.set("You no longer have access to this workspace.".into())
                }
                Ok(BrewPrintOutcome::ContextChanged) => {
                    hide_brew_content();
                    message.set("Workspace changed; reload.".into())
                }
                Ok(BrewPrintOutcome::InvalidRequest) => {
                    print.set(None);
                    message.set("The print request was invalid.".into())
                }
                Ok(BrewPrintOutcome::Unavailable) | Err(_) => {
                    print.set(None);
                    message.set("Historical print is unavailable; try again.".into())
                }
            }
        });
    });

    view! {
        <div class="recipe-editor">
            <header class="recipe-editor__header">
                <div>
                    <p class="kit-showcase__eyebrow">"Frozen brew record"</p>
                    <h1>"Plan versus actual"</h1>
                    <p>"Targets and planned formulation come from the immutable plan; recorded evidence is shown separately."</p>
                </div>
                <div class="recipe-editor__actions">
                    <Button variant=ButtonVariant::Secondary disabled=busy on:click=move |_| load_comparison.run(())>"Refresh"</Button>
                    <Button variant=ButtonVariant::Secondary disabled=busy on:click=move |_| load_print.run(())>"Print frozen plan"</Button>
                </div>
            </header>
            <p role="status">{move || message.get()}</p>
            {move || comparison.get().map(|brew| {
                let changes = brew.changes.clone();
                let changes_for_preview = changes.clone();
                let has_changes = !changes.is_empty();
                let status = status_label(brew.status);
                let actual = brew.actual.as_ref()
                    .map(|actual| format!("Revision {}", actual.revision))
                    .unwrap_or_else(|| "Planned only".to_owned());
                let archived = if brew.archived { "Archived" } else { "Active" };
                view! {
                    <BreweryPanel
                        title=format!("Brew · recipe revision {}", brew.selected_recipe_revision)
                        description="Stored targets and planned formulation do not change; selected actual readings and amended formulation are retained evidence."
                    >
                        <div class="recipe-editor__metrics">
                            <BreweryMetric label="Status" value=status.to_string() unit="">{""}</BreweryMetric>
                            <BreweryMetric label="Recipe revision" value=brew.selected_recipe_revision.to_string() unit="">{""}</BreweryMetric>
                            <BreweryMetric label="Brew revision" value=brew.revision.to_string() unit="">{""}</BreweryMetric>
                            <BreweryMetric label="Recorded actual" value=actual unit="">{""}</BreweryMetric>
                        </div>
                        <BreweryDisclosure title="Frozen plan provenance">
                            <dl class="print-sheet__facts">
                                <BrewProvenanceFact label="Brew reference".to_owned() value=brew.brew_id.clone()/>
                                <BrewProvenanceFact label="Frozen recipe revision".to_owned() value=format!("{}", brew.plan.recipe_revision)/>
                                <BrewProvenanceFact label="Record state".to_owned() value=archived.to_owned()/>
                                <BrewProvenanceFact label="Calculation context".to_owned() value=brew.plan.context_id.clone() mono=true/>
                            </dl>
                        </BreweryDisclosure>

                        <h3 class="recipe-editor__section-title">"Target versus selected actual"</h3>
                        <p class="note-caption">
                            "Planned targets come from the frozen plan. A missing reading is not zero, and unlike measurement bases are not converted."
                        </p>
                        <div class="max-w-full overflow-x-auto" tabindex="0" role="region" aria-label="Planned target versus selected actual table">
                        <table class="print-sheet__table">
                            <thead>
                                <tr>
                                    <th scope="col">"Measurement"</th>
                                    <th scope="col">"Planned target"</th>
                                    <th scope="col">"Selected actual"</th>
                                    <th scope="col">"Comparison"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {brew.measurements.iter().map(|row| {
                                    let target = measurement_value_or(row.target, &row.unit, "No planned target");
                                    let actual = row.selected.as_ref().and_then(|entry| entry.value);
                                    let actual_text = measurement_value_or(actual, &row.unit, "No reading recorded");
                                    let context = row.selected.as_ref()
                                        .map(|entry| selected_reading_context(entry, &row.basis))
                                        .unwrap_or_default();
                                    let status = if row.target.is_none() {
                                        "No planned target for this measurement.".to_owned()
                                    } else if row.selected.is_none() {
                                        "No reading recorded.".to_owned()
                                    } else {
                                        measurement_status_text(row.comparable, row.delta, &row.unit, row.warning.as_deref())
                                    };
                                    let basis = humanize_words(&row.basis);
                                    view! {
                                        <BrewMeasurementRow
                                            label=row.label.clone()
                                            basis=basis
                                            planned=target
                                            actual=actual_text
                                            actual_context=context
                                            comparison=status
                                        />
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                        </div>

                        <h3 class="recipe-editor__section-title">"Formulation changes"</h3>
                        {if has_changes {
                            view! {
                                <p class="note-caption">
                                    "Select the recorded differences to preview as one explicit recipe revision. Unselected evidence stays with the brew."
                                </p>
                                <div class="max-w-full overflow-x-auto" tabindex="0" role="region" aria-label="Formulation changes table">
                                <table class="print-sheet__table">
                                    <thead>
                                        <tr>
                                            <th scope="col"><span class="sr-only">"Include in feedback"</span></th>
                                            <th scope="col">"Section"</th>
                                            <th scope="col">"Item"</th>
                                            <th scope="col">"Planned"</th>
                                            <th scope="col">"Recorded actual"</th>
                                            <th scope="col">"Change"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {changes.iter().map(|change| {
                                            let id = change.id.clone();
                                            let item = item_label(change);
                                            let checked = Signal::derive({
                                                let id = id.clone();
                                                move || selected.with(|set| set.contains(&id))
                                            });
                                            view! {
                                                <tr>
                                                    <td>
                                                        <label>
                                                            <Checkbox
                                                                checked=checked
                                                                on_change=Callback::new({
                                                                    let id = id.clone();
                                                                    move |_| {
                                                                        if busy.get_untracked() || retry.get_untracked().is_some() {
                                                                            return;
                                                                        }
                                                                        let id = id.clone();
                                                                        preview.set(None);
                                                                        busy.set(false);
                                                                        generation.update(|value| *value = value.wrapping_add(1));
                                                                        selected.update(|set| { if !set.remove(&id) { set.insert(id); } });
                                                                    }
                                                                })
                                                            />
                                                            <span class="sr-only">{format!("Include {item} in feedback")}</span>
                                                        </label>
                                                    </td>
                                                    <td>{change_section_text(change)}</td>
                                                    <td>{item}</td>
                                                    <td>{change_planned_text(change)}</td>
                                                    <td>{change_actual_text(change)}</td>
                                                    <td>{change_kind_label(change.kind)}</td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <p class="note-caption">"The plan is still the effective actual formulation; no amended rows to compare."</p>
                            }.into_any()
                        }}
                        <div class="recipe-editor__actions">
                            <Button disabled=Signal::derive(move || busy.get() || retry.get().is_some()) on:click=move |_| run_preview.run(())>"Preview selected feedback"</Button>
                        </div>
                        {move || preview.get().map(|preview_value| {
                            let selected_labels = preview_value.applied_change_ids.iter().map(|id| {
                                changes_for_preview.iter()
                                    .find(|change| &change.id == id)
                                    .map(item_label)
                                    .unwrap_or_else(|| "Selected change".to_owned())
                            }).collect::<Vec<_>>();
                            view! {
                                <BreweryPanel
                                    title="Recipe feedback preview"
                                    description="Applying creates one new recipe revision. The frozen brew plan and recorded evidence stay unchanged."
                                >
                                    <ul>
                                        {selected_labels.into_iter().map(|label| view! { <li>{label}</li> }).collect_view()}
                                    </ul>
                                    <p class="note-caption">
                                        {format!(
                                            "Based on recipe revision {}. Proposed formulation is {}.",
                                            preview_value.base_recipe_head,
                                            validation_label(preview_value.preview.validation.state).to_lowercase()
                                        )}
                                    </p>
                                    {(!preview_value.diagnostics.is_empty()).then(|| view! {
                                        <ul>
                                            {preview_value.diagnostics.iter().map(|diagnostic| view! { <li>{diagnostic_text(diagnostic)}</li> }).collect_view()}
                                        </ul>
                                    })}
                                    <div class="recipe-editor__actions">
                                        <Button disabled=Signal::derive(move || busy.get() || retry.get().is_some()) on:click=move |_| apply.run(())>"Apply as new recipe revision"</Button>
                                    </div>
                                </BreweryPanel>
                            }
                        })}
                        <Show when=move || retry.get().is_some_and(|request| request.brew_id == current_brew.get())>
                            <Button variant=ButtonVariant::Secondary disabled=busy on:click=move |_| { if let Some(request) = retry.get_untracked() { submit.run(request); } }>"Retry previous request"</Button>
                        </Show>
                        <Show when=move || retry.get().is_some_and(|request| request.brew_id != current_brew.get())>
                            <p class="note-caption">"An earlier feedback request is unresolved; reopen its brew to retry it."</p>
                        </Show>
                    </BreweryPanel>
                }
            })}
            {move || print.get().map(|print| {
                let actual = print.actual.as_ref().map(|actual| {
                    format!(
                        "Actual formulation revision {} · recorded {}",
                        actual.revision,
                        format_timestamp(actual.recorded_at)
                    )
                }).unwrap_or_else(|| "No actual formulation amendment.".to_owned());
                view! {
                    <section class="print-preview" aria-label="Historical print preview">
                        <div class="print-preview__toolbar">
                            <div>
                                <p class="kit-showcase__eyebrow">"Historical print preview"</p>
                                <p class="note-caption">
                                    {format!("Frozen recipe revision {}. Planned targets are printed with recorded actuals listed separately.", print.selected_recipe_revision)}
                                </p>
                            </div>
                            <Button on:click=move |_| { #[cfg(target_arch = "wasm32")] if let Some(window) = web_sys::window() { let _ = window.print(); } }>"Print / Save as PDF"</Button>
                        </div>
                        <div class="print-preview__sheet"><PrintableRecipe summary=print.summary/></div>
                        <BreweryPanel
                            title="Recorded actuals"
                            description="Separate from the frozen plan; these readings and amendments never replace the printed targets."
                        >
                            <table class="print-sheet__table">
                                <thead>
                                    <tr>
                                        <th scope="col">"Measurement"</th>
                                        <th scope="col">"Planned target"</th>
                                        <th scope="col">"Recorded actual"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {print.measurements.iter().map(|row| {
                                        let target = measurement_value_or(row.target, &row.unit, "No planned target");
                                        let current = row.current.as_ref().and_then(|entry| entry.value);
                                        let actual_text = measurement_value_or(current, &row.unit, "No reading recorded");
                                        view! {
                                            <BrewMeasurementRow
                                                label=row.label.clone()
                                                planned=target
                                                actual=actual_text
                                            />
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                            <p class="note-caption">{actual}</p>
                            <BreweryDisclosure title="Frozen print provenance">
                                <dl class="print-sheet__facts">
                                    <BrewProvenanceFact label="Brew reference".to_owned() value=print.brew_id.clone()/>
                                    <BrewProvenanceFact label="Recipe revision".to_owned() value=format!("{}", print.selected_recipe_revision)/>
                                    {print.actual.as_ref().map(|actual| view! {
                                        <BrewProvenanceFact label="Actual recorded by".to_owned() value=actual.actor_id.clone()/>
                                    })}
                                </dl>
                            </BreweryDisclosure>
                        </BreweryPanel>
                    </section>
                }
            })}
        </div>
    }
}
