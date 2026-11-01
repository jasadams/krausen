// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared recipe brew history, frozen comparison and explicit feedback views.
use crate::{
    components::{BreweryPanel, Button, ButtonLink, ButtonVariant, Checkbox, PrintableRecipe},
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
    brewery::BrewStatusV1,
    request_context::OperationIntent,
};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use std::collections::BTreeSet;

fn status_label(status: BrewStatusV1) -> &'static str {
    match status {
        BrewStatusV1::Started => "Started",
        BrewStatusV1::Completed => "Completed",
        BrewStatusV1::Cancelled => "Cancelled",
    }
}

fn measure_text(value: Option<f64>, unit: &str) -> String {
    value
        .map(|value| format!("{value} {unit}"))
        .unwrap_or_else(|| "Missing".into())
}

#[component]
pub fn RecipeBrewHistoryPage() -> impl IntoView {
    let params = use_params_map();
    let recipe_id = Signal::derive(move || params.read().get("id").unwrap_or_default());
    let records = RwSignal::new(Vec::<RecipeBrewSummary>::new());
    let identity = RwSignal::new(None::<OperationIntent>);
    let next_cursor = RwSignal::new(None::<String>);
    let message = RwSignal::new("Loading brew history…".to_owned());
    let busy = RwSignal::new(false);

    let load_page = Callback::new(move |cursor: Option<String>| {
        let recipe = recipe_id.get_untracked();
        let Some(intent) = identity.get_untracked() else {
            return;
        };
        if recipe.is_empty() {
            message.set("Recipe not found.".into());
            return;
        }
        busy.set(true);
        message.set("Loading brew history…".into());
        leptos::task::spawn_local(async move {
            let outcome = brew_history_shared(RecipeBrewHistoryQuery {
                intent,
                recipe_id: recipe,
                include_archived: true,
                cursor: cursor.clone(),
                limit: 50,
            })
            .await;
            busy.set(false);
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
                Ok(RecipeBrewHistoryOutcome::NotFound) => message.set("Recipe not found.".into()),
                Ok(RecipeBrewHistoryOutcome::Forbidden) => {
                    message.set("You no longer have access to this workspace.".into())
                }
                Ok(RecipeBrewHistoryOutcome::ContextChanged) => {
                    message.set("Workspace changed; reload.".into())
                }
                Ok(RecipeBrewHistoryOutcome::InvalidRequest) => {
                    message.set("The history request was invalid.".into())
                }
                Ok(RecipeBrewHistoryOutcome::Unavailable) => {
                    message.set("Brew history is unavailable; try again.".into())
                }
                Err(_) => message.set("Brew history is unavailable; try again.".into()),
            }
        });
    });

    Effect::new(move |_| {
        let recipe = recipe_id.get();
        if recipe.is_empty() {
            message.set("Recipe not found.".into());
            return;
        }
        records.set(Vec::new());
        next_cursor.set(None);
        leptos::task::spawn_local(async move {
            let Ok(ctx) = crate::server_fns::context::get_user_context().await else {
                message.set("Sign in to view brew history.".into());
                return;
            };
            let Some(workspace_id) = ctx.workspace_id else {
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

    view! {
        <div class="recipe-editor">
            <header class="recipe-editor__header"><div><p class="kit-showcase__eyebrow">"Your workspace"</p><h1>"Brew history"</h1><p>"Stored plans and optional actual evidence remain separate from the current recipe."</p></div></header>
            <p role="status">{move || message.get()}</p>
            <BreweryPanel title="Brews">
                <ul>{move || records.get().into_iter().map(|record| {
                    let brew_id = record.brew_id.clone();
                    let open = brew_id.clone();
                    view! {
                        <li>
                            <p><strong>{status_label(record.status)}</strong>{format!(" · recipe revision {} · brew revision {}{}", record.selected_recipe_revision, record.revision, if record.archived { " · archived" } else { "" })}</p>
                            <p class="note-caption">{format!("Started {} by {}{}", record.started_at, record.started_by, record.actual_revision.map(|revision| format!(" · actual revision {revision}")).unwrap_or_default())}</p>
                            <ButtonLink href=format!("/brews/{brew_id}/analysis") variant=ButtonVariant::Secondary>"Compare and feedback"</ButtonLink>
                            <ButtonLink href=format!("/brews/{open}") variant=ButtonVariant::Ghost>"Open evidence"</ButtonLink>
                        </li>
                    }
                }).collect_view()}</ul>
                <Show when=move || next_cursor.get().is_some() fallback=|| view! { <p class="note-caption">"End of retained brew history."</p> }>
                    <Button variant=ButtonVariant::Secondary disabled=busy on:click=move |_| load_page.run(next_cursor.get_untracked())>"Load more"</Button>
                </Show>
            </BreweryPanel>
        </div>
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
            if !current_context_matches(&intent).await {
                message.set("Workspace changed; reload.".into());
                return;
            }
            let outcome = brew_comparison_shared(BrewComparisonRequest {
                intent,
                brew_id: brew.clone(),
            })
            .await;
            if generation.try_get_untracked() != Some(mine)
                || current_brew.try_get_untracked().as_deref() != Some(brew.as_str())
            {
                return;
            }
            busy.set(false);
            match outcome {
                Ok(BrewComparisonOutcome::Comparison(value)) => {
                    comparison.set(Some(value));
                    message.set(String::new());
                }
                Ok(BrewComparisonOutcome::NotFound) => message.set("Brew not found.".into()),
                Ok(BrewComparisonOutcome::Forbidden) => {
                    message.set("You no longer have access to this workspace.".into())
                }
                Ok(BrewComparisonOutcome::ContextChanged) => {
                    message.set("Workspace changed; reload.".into())
                }
                Ok(BrewComparisonOutcome::Unavailable) | Err(_) => {
                    message.set("Comparison is unavailable; try again.".into())
                }
            }
        });
    });

    Effect::new(move |_| {
        let brew = brew_id.get();
        current_brew.set(brew.clone());
        comparison.set(None);
        preview.set(None);
        print.set(None);
        selected.set(BTreeSet::new());
        retry.set(None);
        if brew.is_empty() {
            message.set("Brew not found.".into());
            return;
        }
        leptos::task::spawn_local(async move {
            let Ok(ctx) = crate::server_fns::context::get_user_context().await else {
                message.set("Sign in to view this brew.".into());
                return;
            };
            let Some(workspace_id) = ctx.workspace_id else {
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
        if retry.get_untracked().is_some() {
            message.set("Retry the previous request before starting another.".into());
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
        let brew = current_brew.get_untracked();
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
            if !current_context_matches(&intent).await {
                message.set("Workspace changed; reload.".into());
                return;
            }
            let outcome = brew_feedback_preview_shared(BrewFeedbackPreviewRequest {
                intent,
                brew_id: value.brew_id.clone(),
                expected_actual_revision: value.actual.as_ref().map(|actual| actual.revision),
                change_ids,
            })
            .await;
            if generation.try_get_untracked() != Some(mine)
                || current_brew.try_get_untracked() != Some(brew.clone())
            {
                return;
            }
            busy.set(false);
            match outcome {
                Ok(BrewFeedbackPreviewOutcome::Preview(value)) => {
                    preview.set(Some(value));
                    message.set(String::new());
                }
                Ok(BrewFeedbackPreviewOutcome::NoActual) => {
                    message.set("This brew has no actual formulation amendment.".into())
                }
                Ok(BrewFeedbackPreviewOutcome::NoChanges { diagnostics }) => {
                    message.set(if diagnostics.is_empty() {
                        "The selected changes would not alter the current recipe.".into()
                    } else {
                        format!("No recipe changes to apply: {}", diagnostics.join("; "))
                    })
                }
                Ok(BrewFeedbackPreviewOutcome::Conflict { expected, actual }) => message.set(format!(
                    "Actual revision changed from {expected:?} to {actual:?}; reload the comparison."
                )),
                Ok(BrewFeedbackPreviewOutcome::NotFound) => message.set("Brew not found.".into()),
                Ok(BrewFeedbackPreviewOutcome::Forbidden) => {
                    message.set("You no longer have access to this workspace.".into())
                }
                Ok(BrewFeedbackPreviewOutcome::ContextChanged) => {
                    message.set("Workspace changed; reload.".into())
                }
                Ok(BrewFeedbackPreviewOutcome::InvalidRequest) => {
                    message.set("The feedback selection is invalid.".into())
                }
                Ok(BrewFeedbackPreviewOutcome::UnresolvedProfile) => {
                    message.set("The selected equipment profile is unavailable.".into())
                }
                Ok(BrewFeedbackPreviewOutcome::UnresolvedStyle) => {
                    message.set("The selected style is unavailable.".into())
                }
                Ok(BrewFeedbackPreviewOutcome::Unavailable) | Err(_) => {
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
        let mine = generation.get_untracked().wrapping_add(1);
        generation.set(mine);
        busy.set(true);
        message.set("Applying selected feedback…".into());
        retry.set(None);
        leptos::task::spawn_local(async move {
            let outcome = brew_feedback_apply_shared(request.clone()).await;
            if generation.try_get_untracked() != Some(mine)
                || current_brew.try_get_untracked() != Some(brew.clone())
            {
                return;
            }
            busy.set(false);
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
                Ok(BrewFeedbackApplyOutcome::Unchanged { recipe_revision, .. }) => {
                    preview.set(None);
                    message.set(format!(
                        "The selected changes are already in recipe revision {recipe_revision}."
                    ));
                }
                Ok(BrewFeedbackApplyOutcome::HeadConflict { expected, actual }) => message.set(format!(
                    "The recipe head advanced from {expected} to {actual}; reload the comparison."
                )),
                Ok(BrewFeedbackApplyOutcome::ContextConflict { .. }) => message.set(
                    "The accepted calculation context changed; preview the current comparison again."
                        .into(),
                ),
                Ok(BrewFeedbackApplyOutcome::ActualConflict { expected, actual }) => message.set(format!(
                    "The actual revision changed from {expected:?} to {actual:?}; reload the comparison."
                )),
                Ok(BrewFeedbackApplyOutcome::NoActual) => {
                    message.set("This brew has no actual formulation amendment.".into())
                }
                Ok(BrewFeedbackApplyOutcome::NoChanges { diagnostics }) => message.set(
                    if diagnostics.is_empty() {
                        "No recipe revision was needed.".into()
                    } else {
                        format!("No recipe changes to apply: {}", diagnostics.join("; "))
                    },
                ),
                Ok(BrewFeedbackApplyOutcome::NotFound) => message.set("Brew not found.".into()),
                Ok(BrewFeedbackApplyOutcome::Forbidden) => {
                    message.set("You no longer have access to this workspace.".into())
                }
                Ok(BrewFeedbackApplyOutcome::ContextChanged) => {
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
                Ok(BrewFeedbackApplyOutcome::Invalid(validation)) => message.set(format!(
                    "The proposed recipe is invalid ({:?}).",
                    validation.state
                )),
                Ok(BrewFeedbackApplyOutcome::UnresolvedIngredientSource) => message.set(
                    "A copied ingredient source is no longer valid on the current recipe.".into(),
                ),
                Ok(BrewFeedbackApplyOutcome::RequestIdentityConflict) => message
                    .set("This request identity was already used for different feedback.".into()),
                Ok(BrewFeedbackApplyOutcome::ResultExpired { .. }) => {
                    message.set("This feedback result expired; preview again.".into())
                }
                Ok(BrewFeedbackApplyOutcome::Retryable)
                | Ok(BrewFeedbackApplyOutcome::Unavailable)
                | Err(_) => {
                    message.set("The response was lost; the same request can be retried.".into());
                    retry.set(Some(request.clone()));
                }
            }
        });
    });

    let apply = Callback::new(move |()| {
        if let Some(request) = retry.get_untracked() {
            submit.run(request);
            return;
        }
        let Some(preview_value) = preview.get_untracked() else {
            return;
        };
        if preview_value.brew_id != current_brew.get_untracked() {
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
            if !current_context_matches(&intent).await {
                message.set("Workspace changed; reload.".into());
                return;
            }
            let outcome = brew_print_shared(BrewPrintRequest {
                intent,
                brew_id: brew.clone(),
            })
            .await;
            if generation.try_get_untracked() != Some(mine)
                || current_brew.try_get_untracked() != Some(brew)
            {
                return;
            }
            busy.set(false);
            match outcome {
                Ok(BrewPrintOutcome::Print(value)) => {
                    print.set(Some(value));
                    message.set(String::new());
                }
                Ok(BrewPrintOutcome::NotFound) => message.set("Brew not found.".into()),
                Ok(BrewPrintOutcome::Forbidden) => {
                    message.set("You no longer have access to this workspace.".into())
                }
                Ok(BrewPrintOutcome::ContextChanged) => {
                    message.set("Workspace changed; reload.".into())
                }
                Ok(BrewPrintOutcome::InvalidRequest) => {
                    message.set("The print request was invalid.".into())
                }
                Ok(BrewPrintOutcome::Unavailable) | Err(_) => {
                    message.set("Historical print is unavailable; try again.".into())
                }
            }
        });
    });

    view! {
        <div class="recipe-editor">
            <header class="recipe-editor__header"><div><p class="kit-showcase__eyebrow">"Frozen brew record"</p><h1>"Plan versus actual"</h1><p>"Targets and formulation come from the immutable plan; actual evidence is shown separately."</p></div><div class="recipe-editor__actions"><Button variant=ButtonVariant::Secondary disabled=busy on:click=move |_| load_comparison.run(())>"Refresh"</Button><Button variant=ButtonVariant::Secondary disabled=busy on:click=move |_| load_print.run(())>"Print frozen plan"</Button></div></header>
            <p role="status">{move || message.get()}</p>
            {move || comparison.get().map(|brew| {
                let changes = brew.changes.clone();
                let has_changes = !changes.is_empty();
                view! {
                <BreweryPanel title=format!("Brew · {} · recipe revision {}", status_label(brew.status), brew.selected_recipe_revision)>
                    <p>{format!("Brew revision {} · plan {} · {}{}", brew.revision, brew.plan.context_id, if brew.archived { "archived" } else { "active" }, brew.actual.as_ref().map(|actual| format!(" · actual revision {}", actual.revision)).unwrap_or_default())}</p>
                    <h3>"Target versus selected actual"</h3>
                    <ul>{brew.measurements.iter().map(|row| view! {
                        <li>{format!("{}: target {} · selected {} · {}{}{}", row.label, measure_text(row.target, &row.unit), row.selected.as_ref().map(|entry| measure_text(entry.value, &row.unit)).unwrap_or_else(|| "missing".into()), if let (Some(target), Some(actual)) = (row.target, row.selected.as_ref().and_then(|entry| entry.value)) { format!("delta {:.4} {}", actual - target, row.unit) } else { "not comparable".into() }, row.warning.as_ref().map(|warning| format!(" · {warning}")).unwrap_or_default(), if row.comparable { "" } else { " · no compatible planned target" })}</li>
                    }).collect_view()}</ul>
                    <h3>"Formulation changes"</h3>
                    {if has_changes {
                        view! { <ul>{changes.iter().map(|change| { let id = change.id.clone(); view! {
                            <li><label><Checkbox checked=Signal::derive({ let id = id.clone(); move || selected.with(|set| set.contains(&id)) }) on_change=Callback::new({ let id = id.clone(); move |_| { let id = id.clone(); preview.set(None); generation.update(|value| *value = value.wrapping_add(1)); selected.update(|set| { if !set.remove(&id) { set.insert(id); } }); } })/> {format!("{:?} · {} · {:?} → {:?}", change.section, change.id, change.planned, change.actual)}</label></li>
                        } }).collect_view()}</ul> }.into_any()
                    } else {
                        view! { <p>"The plan is still the effective actual formulation; no amended rows to compare."</p> }.into_any()
                    }}
                    <div class="recipe-editor__actions"><Button disabled=Signal::derive(move || busy.get() || retry.get().is_some()) on:click=move |_| run_preview.run(())>"Preview selected feedback"</Button></div>
                    {move || preview.get().map(|preview_value| view! {
                        <BreweryPanel title="Feedback preview">
                            <ul>{preview_value.applied_change_ids.iter().map(|id| view! { <li>{id.clone()}</li> }).collect_view()}</ul>
                            <p>{format!("Proposed recipe head {} · validation {:?}", preview_value.base_recipe_head, preview_value.preview.validation.state)}</p>
                            <ul>{preview_value.diagnostics.iter().map(|diagnostic| view! { <li>{diagnostic.clone()}</li> }).collect_view()}</ul>
                            <div class="recipe-editor__actions"><Button disabled=busy on:click=move |_| apply.run(())>"Apply as new recipe revision"</Button></div>
                        </BreweryPanel>
                    })}
                    <Show when=move || retry.get().is_some()><Button variant=ButtonVariant::Secondary disabled=busy on:click=move |_| { if let Some(request) = retry.get_untracked() { submit.run(request); } }>"Retry previous request"</Button></Show>
                </BreweryPanel>
            }})}
            {move || print.get().map(|print| view! {
                <section class="print-preview" aria-label="Historical print preview">
                    <div class="print-preview__toolbar"><p class="kit-showcase__eyebrow">"Historical print preview"</p><Button on:click=move |_| { #[cfg(target_arch = "wasm32")] if let Some(window) = web_sys::window() { let _ = window.print(); } }>"Print / Save as PDF"</Button></div>
                    <div class="print-preview__sheet"><PrintableRecipe summary=print.summary/></div>
                    <BreweryPanel title="Actuals (separate from the frozen plan)">
                        <ul>{print.measurements.iter().map(|row| view! { <li>{format!("{}: target {} · selected {}", row.label, measure_text(row.target, &row.unit), row.current.as_ref().map(|entry| measure_text(entry.value, &row.unit)).unwrap_or_else(|| "missing".into()))}</li> }).collect_view()}</ul>
                        <p>{print.actual.as_ref().map(|actual| format!("Actual formulation revision {} recorded {} by {}", actual.revision, actual.recorded_at, actual.actor_id)).unwrap_or_else(|| "No actual formulation amendment.".into())}</p>
                    </BreweryPanel>
                </section>
            })}
        </div>
    }
}
