// SPDX-License-Identifier: AGPL-3.0-or-later
//! Scaling and duplication dialogs over the shared application operations.
use crate::{
    components::{BreweryDraftField, BreweryPanel, Button, ButtonVariant, Modal, ModalSize},
    server_fns::scaling::{duplicate_recipe_shared, scale_apply_shared, scale_preview_shared},
};
use krausen_types::{
    brewery::RecipeDocumentV1,
    formulation::Targets,
    request_context::OperationIntent,
    scaling::{
        DuplicateOutcome, DuplicateRecipe, ScaleApplyOutcome, ScaleApplyRequest, ScaleChange,
        ScaleOutcome, ScalePreview, ScalePreviewRequest, ScaleWarning,
    },
};
use leptos::prelude::*;

fn warning_text(warning: &ScaleWarning) -> String {
    match warning {
        ScaleWarning::KettleCapacityExceeded {
            capacity_l,
            post_boil_l,
            pre_boil_l,
        } => {
            let post_exceeds = post_boil_l > capacity_l;
            let pre_exceeds = pre_boil_l.is_some_and(|pre| pre > *capacity_l);
            let fill = match (post_exceeds, pre_exceeds) {
                (true, true) => format!(
                    "pre-boil {:.2} L / post-boil {post_boil_l:.2} L",
                    pre_boil_l.unwrap_or_default()
                ),
                (true, false) => format!("post-boil {post_boil_l:.2} L"),
                (false, true) => format!(
                    "pre-boil {:.2} L (post-boil {post_boil_l:.2} L fits)",
                    pre_boil_l.unwrap_or_default()
                ),
                (false, false) => format!("post-boil {post_boil_l:.2} L"),
            };
            format!(
                "Kettle capacity {capacity_l:.2} L does not fit the scaled {fill}. Choose a larger kettle or change the process; geometry is not changed automatically."
            )
        }
        ScaleWarning::CapacityUnknown { post_boil_l } => format!(
            "Kettle capacity is not set; check that the scaled post-boil volume of {post_boil_l:.2} L fits. Geometry is not changed automatically."
        ),
        ScaleWarning::FixedLossesRetained {
            transfer_loss_l,
            boil_off_l_per_hour,
            pre_boil_deadspace_l,
            grain_absorption_l_per_kg,
        } => format!(
            "Fixed losses stay unchanged (transfer {}, boil-off {}, deadspace {}, absorption {}), so targets can change. This is ingredient scaling, not target matching.",
            option_value(*transfer_loss_l, "L"),
            option_value(*boil_off_l_per_hour, "L/h"),
            option_value(*pre_boil_deadspace_l, "L"),
            option_value(*grain_absorption_l_per_kg, "L/kg"),
        ),
        ScaleWarning::UnscaledIngredients {
            yeast_rows,
            other_ingredient_rows,
        } if *yeast_rows > 0 || *other_ingredient_rows > 0 => format!(
            "Yeast ({yeast_rows}) and other ingredients ({other_ingredient_rows}) are preserved, not multiplied, by linear-ingredients-v1."
        ),
        ScaleWarning::UnscaledIngredients { .. } => String::new(),
        ScaleWarning::DerivedMashWater { thickness_l_per_kg } => format!(
            "Mash water is derived from the scaled grain at {thickness_l_per_kg:.3} L/kg rather than multiplied."
        ),
    }
}

fn option_value(value: Option<f64>, unit: &str) -> String {
    value
        .map(|value| format!("{value:.3} {unit}"))
        .unwrap_or_else(|| "not set".to_owned())
}

fn target_rows(
    before: &Targets,
    after: &Targets,
) -> Vec<(&'static str, Option<f64>, Option<f64>, &'static str)> {
    vec![
        (
            "Total water",
            before.total_water_l,
            after.total_water_l,
            "L",
        ),
        ("Pre-boil", before.pre_boil_l, after.pre_boil_l, "L"),
        ("Post-boil", before.post_boil_l, after.post_boil_l, "L"),
        ("Mash water", before.mash_water_l, after.mash_water_l, "L"),
        (
            "Sparge water",
            before.sparge_water_l,
            after.sparge_water_l,
            "L",
        ),
        ("Pre-boil SG", before.pre_boil_sg, after.pre_boil_sg, ""),
        ("OG", before.original_sg, after.original_sg, ""),
        ("FG", before.final_sg, after.final_sg, ""),
        ("ABV", before.abv_percent, after.abv_percent, "%"),
        ("IBU", before.ibu, after.ibu, ""),
        ("Colour", before.colour_ebc, after.colour_ebc, "EBC"),
    ]
}

fn change_text(change: &ScaleChange) -> String {
    let name = change
        .row_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or(&change.field);
    format!(
        "{name} · {}: {:.4} to {:.4}",
        change.field, change.before, change.after
    )
}

#[component]
fn ScalePreviewView(value: ScalePreview) -> impl IntoView {
    let has_changes = !value.changes.is_empty();
    let changes = value
        .changes
        .iter()
        .map(|change| view! { <li>{change_text(change)}</li> })
        .collect_view();
    let target_rows = target_rows(&value.source_targets, &value.targets)
        .into_iter()
        .filter_map(|(label, before, after, unit)| {
            (before != after).then(|| {
                view! {
                    <li>{format!(
                        "{label}: {} to {}",
                        option_value(before, unit),
                        option_value(after, unit),
                    )}</li>
                }
            })
        })
        .collect_view();
    let warnings = value
        .warnings
        .iter()
        .map(|warning| view! { <li>{warning_text(warning)}</li> })
        .collect_view();
    let factor = value.factor;
    let source_batch = value.source_batch_size_l;
    let target_batch = value.target_batch_size_l;
    let validation = value.validation.state;
    let changes_view = if has_changes {
        view! { <ul>{changes}</ul> }.into_any()
    } else {
        view! { <p>"No stored amount changes at this factor."</p> }.into_any()
    };
    view! {
        <div class="space-y-4">
            <BreweryPanel title="Scaled preview">
                <p>{format!("{source_batch} L to {target_batch} L · factor {factor:.6}")}</p>
                {changes_view}
            </BreweryPanel>
            <BreweryPanel title="Recalculated targets">
                <ul>{target_rows}</ul>
                <p>{format!("Validation: {validation:?}")}</p>
            </BreweryPanel>
            <BreweryPanel title="Effects and warnings">
                <ul>{warnings}</ul>
            </BreweryPanel>
        </div>
    }
}

#[component]
pub fn ScaleDialog(
    open: RwSignal<bool>,
    identity: RwSignal<OperationIntent>,
    recipe_id: RwSignal<Option<String>>,
    revision: RwSignal<Option<i64>>,
    context_lost: RwSignal<bool>,
    uncertain: RwSignal<bool>,
    pending: RwSignal<bool>,
    status: RwSignal<String>,
    on_applied: Callback<(RecipeDocumentV1, i64)>,
) -> impl IntoView {
    let target = RwSignal::new(String::new());
    let preview = RwSignal::new(None::<ScalePreview>);
    let message = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let retry = RwSignal::new(None::<ScaleApplyRequest>);
    let retry_document = RwSignal::new(None::<RecipeDocumentV1>);
    let generation = RwSignal::new(0_u64);

    let reset = Callback::new(move |()| {
        preview.set(None);
        message.set(String::new());
        generation.update(|value| *value = value.wrapping_add(1));
    });
    let abandon = Callback::new(move |()| {
        open.set(false);
        reset.run(());
        busy.set(false);
        pending.set(false);
        retry.set(None);
        retry_document.set(None);
        uncertain.set(false);
    });
    let close = Callback::new(move |()| {
        if busy.get_untracked() {
            return;
        }
        open.set(false);
        reset.run(());
        message.set(String::new());
    });

    // A focus revalidation briefly hides the editor; that is not abandonment.
    // Only a completed context mismatch closes and clears the dialog.
    Effect::new(move |_| {
        if context_lost.get() {
            abandon.run(());
            message.set("Workspace changed; return to the original workspace.".into());
        }
    });
    Effect::new(move |_| {
        if open.get() {
            if retry.get().is_some() {
                message.set(
                    "An earlier apply may have committed. Retry the previous request before starting another."
                        .into(),
                );
            } else {
                message.set(String::new());
            }
        }
    });

    let run_preview = Callback::new(move |()| {
        if retry.get_untracked().is_some() {
            message.set("Retry the previous apply before starting another.".into());
            return;
        }
        let Some(id) = recipe_id.get_untracked() else {
            message.set("Save the recipe before scaling it.".into());
            return;
        };
        let value = match target.get_untracked().trim().parse::<f64>() {
            Ok(value) if value.is_finite() && value > 0.0 => value,
            _ => {
                message.set("Enter a positive target volume.".into());
                return;
            }
        };
        let expected = identity.get_untracked();
        let next_generation = generation.get_untracked().wrapping_add(1);
        generation.set(next_generation);
        busy.set(true);
        pending.set(true);
        message.set("Calculating scaled amounts and targets…".into());
        leptos::task::spawn_local(async move {
            let outcome = scale_preview_shared(ScalePreviewRequest {
                intent: expected.clone(),
                recipe_id: id,
                target_batch_size_l: value,
            })
            .await;
            if generation.try_get_untracked() != Some(next_generation) {
                return;
            }
            busy.set(false);
            pending.set(false);
            match outcome {
                Ok(ScaleOutcome::Preview(preview_value)) => {
                    if revision.try_get_untracked().flatten() != Some(preview_value.source_revision)
                    {
                        preview.set(None);
                        message
                            .set("The saved recipe has changed. Reload it before scaling.".into());
                    } else {
                        preview.set(Some(preview_value));
                        message.set(String::new());
                    }
                }
                Ok(ScaleOutcome::Unscalable(problem)) => message.set(format!(
                    "This recipe cannot be scaled: {}.",
                    problem_text(problem)
                )),
                Ok(ScaleOutcome::Archived) => {
                    message.set("Archived recipes cannot be scaled.".into())
                }
                Ok(ScaleOutcome::NotFound) => message.set("The recipe was not found.".into()),
                Ok(ScaleOutcome::Forbidden) => {
                    message.set("You no longer have access to this workspace.".into())
                }
                Ok(ScaleOutcome::ContextChanged) => {
                    context_lost.set(true);
                    message.set("Workspace changed; reload the recipe.".into());
                }
                Ok(ScaleOutcome::UnresolvedProfile) => {
                    message.set("The selected equipment profile is unavailable or archived.".into())
                }
                Ok(ScaleOutcome::UnresolvedStyle) => {
                    message.set("The selected style is no longer supported.".into())
                }
                Ok(ScaleOutcome::Invalid(validation)) => message.set(format!(
                    "The scaled recipe is invalid ({:?}).",
                    validation.state
                )),
                Ok(ScaleOutcome::Unavailable) => {
                    message.set("Scaling is unavailable; try again.".into())
                }
                Err(_) => message.set("Unable to preview scaling; try again.".into()),
            }
        });
    });

    let submit = Callback::new(move |request: ScaleApplyRequest| {
        if let Some(preview_value) = preview.get_untracked() {
            retry_document.set(Some(preview_value.scaled_document));
        }
        let next_generation = generation.get_untracked().wrapping_add(1);
        generation.set(next_generation);
        busy.set(true);
        pending.set(true);
        message.set("Applying scaling…".into());
        leptos::task::spawn_local(async move {
            let outcome = scale_apply_shared(request.clone()).await;
            if generation.try_get_untracked() != Some(next_generation) {
                return;
            }
            busy.set(false);
            pending.set(false);
            let uncertain_outcome = matches!(
                outcome,
                Ok(ScaleApplyOutcome::Unavailable) | Ok(ScaleApplyOutcome::Retryable) | Err(_)
            );
            if uncertain_outcome {
                retry.set(Some(request.clone()));
                uncertain.set(true);
            } else {
                retry.set(None);
                uncertain.set(false);
                retry_document.set(None);
            }
            match outcome {
                Ok(ScaleApplyOutcome::Applied {
                    revision: applied, ..
                }) => {
                    let document = preview
                        .get_untracked()
                        .map(|preview_value| preview_value.scaled_document)
                        .or_else(|| retry_document.get_untracked());
                    if let Some(document) = document {
                        on_applied.run((document, applied));
                        status.set(format!("Scaled recipe saved as revision {applied}."));
                        open.set(false);
                        reset.run(());
                    } else {
                        message.set(
                            "Scaling committed but the editor could not be synchronized. Reload the recipe."
                                .into(),
                        );
                        uncertain.set(false);
                        retry.set(None);
                        retry_document.set(None);
                    }
                }
                Ok(ScaleApplyOutcome::Unchanged {
                    revision: current, ..
                }) => {
                    status.set(format!(
                        "Scale target matches revision {current}; nothing changed."
                    ));
                    open.set(false);
                    reset.run(());
                }
                Ok(ScaleApplyOutcome::ContextConflict { fresh }) => {
                    preview.set(Some(*fresh));
                    message.set("The saved recipe or calculation context changed. Review the fresh preview and apply again.".into());
                    generation.update(|value| *value = value.wrapping_add(1));
                }
                Ok(ScaleApplyOutcome::Conflict { expected, actual }) => {
                    message.set(format!(
                        "The recipe advanced from revision {expected} to {actual}. Reload it and scale again."
                    ));
                }
                Ok(ScaleApplyOutcome::Unscalable(problem)) => message.set(format!(
                    "This recipe cannot be scaled: {}.",
                    problem_text(problem)
                )),
                Ok(ScaleApplyOutcome::Archived) => {
                    message.set("Archived recipes cannot be scaled.".into())
                }
                Ok(ScaleApplyOutcome::NotFound) => message.set("The recipe was not found.".into()),
                Ok(ScaleApplyOutcome::Forbidden) => {
                    message.set("You no longer have access to this workspace.".into())
                }
                Ok(ScaleApplyOutcome::ContextChanged) => {
                    context_lost.set(true);
                    message.set("Workspace changed; reload the recipe.".into());
                }
                Ok(ScaleApplyOutcome::UnresolvedProfile) => {
                    message.set("The selected equipment profile is unavailable or archived.".into())
                }
                Ok(ScaleApplyOutcome::UnresolvedStyle) => {
                    message.set("The selected style is no longer supported.".into())
                }
                Ok(ScaleApplyOutcome::Invalid(validation)) => message.set(format!(
                    "The scaled recipe is invalid ({:?}); correct the source first.",
                    validation.state
                )),
                Ok(ScaleApplyOutcome::InvalidRequest) => {
                    message.set("The scaling request was invalid.".into())
                }
                Ok(ScaleApplyOutcome::RequestIdentityConflict) => message.set(
                    "This request identity was already used for a different scale request.".into(),
                ),
                Ok(ScaleApplyOutcome::ResultExpired { .. }) => {
                    message.set("This scaling result expired; preview and apply again.".into())
                }
                Ok(ScaleApplyOutcome::Retryable) => {
                    message.set("Scaling is retryable; try again.".into())
                }
                Ok(ScaleApplyOutcome::Unavailable) => {
                    message
                        .set("Scaling is unavailable; the request identity was retained.".into());
                }
                Err(_) => {
                    message.set("The response was lost; the same request can be retried.".into());
                }
            }
        });
    });

    let apply = Callback::new(move |()| {
        let Some(preview_value) = preview.get_untracked() else {
            return;
        };
        let Some(recipe) = recipe_id.get_untracked() else {
            return;
        };
        let Some(request_id) = crate::utils::request_id::new_request_id() else {
            message.set("Unable to create a request identity.".into());
            return;
        };
        submit.run(ScaleApplyRequest {
            intent: identity.get_untracked(),
            recipe_id: recipe,
            expected_revision: preview_value.source_revision,
            target_batch_size_l: preview_value.target_batch_size_l,
            accepted: preview_value.accepted.clone(),
            request_id,
        });
    });

    view! {
        <Modal
            show=Signal::derive(move || open.get())
            on_close=close
            title="Scale batch"
            size=ModalSize::Lg
        >
            <div class="space-y-4">
                <p>"Scale ingredient amounts and specified water allocations with linear-ingredients-v1. Timings, specifications, equipment and fixed losses stay unchanged; targets are recalculated."</p>
                <BreweryDraftField
                    id="scale-target-volume"
                    label="Target fermenter volume"
                    unit="L"
                    numeric=true
                    value=Signal::derive(move || target.get())
                    on_change=Callback::new(move |value: String| target.set(value))
                />
                <div class="recipe-editor__actions">
                    <Button disabled=Signal::derive(move || busy.get() || retry.get().is_some()) on:click=move |_| run_preview.run(())>"Preview scaling"</Button>
                    <Button variant=ButtonVariant::Secondary disabled=Signal::derive(move || busy.get() || retry.get().is_some() || preview.get().is_none()) on:click=move |_| apply.run(())>"Apply scaling"</Button>
                    <Button variant=ButtonVariant::Ghost disabled=busy on:click=move |_| close.run(())>"Cancel"</Button>
                </div>
                <p role="status">{move || message.get()}</p>
                <Show when=move || retry.get().is_some()>
                    <Button variant=ButtonVariant::Secondary disabled=busy on:click=move |_| {
                        if let Some(request) = retry.get_untracked() { submit.run(request); }
                    }>"Retry previous request"</Button>
                </Show>
                <Show when=move || preview.get().is_some()>
                    {move || preview.get().map(|value| view! { <ScalePreviewView value=value/> })}
                </Show>
            </div>
        </Modal>
    }
}

fn problem_text(problem: krausen_types::scaling::ScaleProblem) -> &'static str {
    use krausen_types::scaling::ScaleProblem;
    match problem {
        ScaleProblem::MissingSourceVolume => "the saved recipe has no batch volume",
        ScaleProblem::InvalidSourceVolume => "the saved batch volume is not positive",
        ScaleProblem::InvalidTargetVolume => "the target volume is not positive",
    }
}

#[component]
pub fn DuplicateDialog(
    open: RwSignal<bool>,
    identity: RwSignal<OperationIntent>,
    recipe_id: RwSignal<Option<String>>,
    revision: RwSignal<Option<i64>>,
    context_lost: RwSignal<bool>,
    uncertain: RwSignal<bool>,
    pending: RwSignal<bool>,
    status: RwSignal<String>,
) -> impl IntoView {
    let title = RwSignal::new(String::new());
    let message = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let retry = RwSignal::new(None::<DuplicateRecipe>);
    #[cfg(target_arch = "wasm32")]
    let navigate = StoredValue::new(leptos_router::hooks::use_navigate());

    let close = Callback::new(move |()| {
        if busy.get_untracked() {
            return;
        }
        open.set(false);
        message.set(String::new());
    });

    Effect::new(move |_| {
        if context_lost.get() {
            open.set(false);
            busy.set(false);
            pending.set(false);
            retry.set(None);
            uncertain.set(false);
            message.set("Workspace changed; return to the original workspace.".into());
        }
    });
    Effect::new(move |_| {
        if open.get() {
            title.set(String::new());
            if retry.get().is_some() {
                message.set(
                    "An earlier duplicate may have committed. Retry the previous request before duplicating again."
                        .into(),
                );
            } else {
                message.set(String::new());
            }
        }
    });

    let submit = Callback::new(move |request: DuplicateRecipe| {
        busy.set(true);
        pending.set(true);
        message.set("Duplicating recipe…".into());
        leptos::task::spawn_local(async move {
            let outcome = duplicate_recipe_shared(request.clone()).await;
            let Some(_) = open.try_get_untracked() else {
                return;
            };
            busy.set(false);
            pending.set(false);
            let uncertain_outcome = matches!(
                outcome,
                Ok(DuplicateOutcome::Unavailable) | Ok(DuplicateOutcome::Retryable) | Err(_)
            );
            if uncertain_outcome {
                retry.set(Some(request.clone()));
                uncertain.set(true);
            } else {
                retry.set(None);
                uncertain.set(false);
            }
            match outcome {
                Ok(DuplicateOutcome::Duplicated {
                    recipe_id: new_id,
                    revision: new_revision,
                    source_revision,
                }) => {
                    status.set(format!(
                        "Duplicated revision {source_revision} as revision {new_revision}."
                    ));
                    open.set(false);
                    #[cfg(target_arch = "wasm32")]
                    {
                        navigate.with_value(|navigate| {
                            navigate(
                                &format!("/recipe-editor/{new_id}"),
                                Default::default(),
                            )
                        });
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let _ = new_id;
                    }
                }
                Ok(DuplicateOutcome::Conflict { expected, actual }) => message.set(format!(
                    "The recipe advanced from revision {expected} to {actual}. Reload it and duplicate again."
                )),
                Ok(DuplicateOutcome::Archived) => {
                    message.set("Archived recipes cannot be duplicated.".into())
                }
                Ok(DuplicateOutcome::NotFound) => message.set("The recipe was not found.".into()),
                Ok(DuplicateOutcome::Forbidden) => {
                    message.set("You no longer have access to this workspace.".into())
                }
                Ok(DuplicateOutcome::ContextChanged) => {
                    message.set("Workspace changed; reload the recipe.".into())
                }
                Ok(DuplicateOutcome::InvalidRequest) => {
                    message.set("The duplicate request was invalid.".into())
                }
                Ok(DuplicateOutcome::RequestIdentityConflict) => message
                    .set("This request identity was already used for a different duplicate.".into()),
                Ok(DuplicateOutcome::ResultExpired { .. }) => {
                    message.set("This duplicate result expired; start again.".into())
                }
                Ok(DuplicateOutcome::Retryable) => {
                    message.set("Duplication is retryable; try again.".into())
                }
                Ok(DuplicateOutcome::Unavailable) => {
                    message.set("Duplication is unavailable; the request identity was retained.".into());
                }
                Err(_) => {
                    message.set("The response was lost; the same request can be retried.".into());
                }
            }
        });
    });

    let duplicate = Callback::new(move |()| {
        if retry.get_untracked().is_some() {
            message.set("Retry the previous duplicate request before starting another.".into());
            return;
        }
        let Some(source) = recipe_id.get_untracked() else {
            message.set("Save the recipe before duplicating it.".into());
            return;
        };
        let Some(expected_revision) = revision.get_untracked() else {
            message.set("Save the recipe before duplicating it.".into());
            return;
        };
        let Some(request_id) = crate::utils::request_id::new_request_id() else {
            message.set("Unable to create a request identity.".into());
            return;
        };
        let title = title.get_untracked().trim().to_owned();
        submit.run(DuplicateRecipe {
            intent: identity.get_untracked(),
            source_recipe_id: source,
            expected_revision,
            title: (!title.is_empty()).then_some(title),
            request_id,
        });
    });

    view! {
        <Modal
            show=Signal::derive(move || open.get())
            on_close=close
            title="Duplicate recipe"
            size=ModalSize::Md
        >
            <div class="space-y-4">
                <p>"Create an independent recipe from the current saved revision. Ingredient specifications and their source provenance are copied; brew history and imported-source claims are not."</p>
                <BreweryDraftField
                    id="duplicate-title"
                    label="New recipe name (optional)"
                    value=Signal::derive(move || title.get())
                    on_change=Callback::new(move |value: String| title.set(value))
                />
                <div class="recipe-editor__actions">
                    <Button disabled=Signal::derive(move || busy.get() || retry.get().is_some()) on:click=move |_| duplicate.run(())>"Duplicate"</Button>
                    <Button variant=ButtonVariant::Ghost disabled=busy on:click=move |_| close.run(())>"Cancel"</Button>
                </div>
                <p role="status">{move || message.get()}</p>
                <Show when=move || retry.get().is_some()>
                    <Button variant=ButtonVariant::Secondary disabled=busy on:click=move |_| {
                        if let Some(request) = retry.get_untracked() { submit.run(request); }
                    }>"Retry previous request"</Button>
                </Show>
            </div>
        </Modal>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_warning_names_values_and_explicit_geometry_policy() {
        let text = warning_text(&ScaleWarning::KettleCapacityExceeded {
            capacity_l: 30.0,
            post_boil_l: 42.0,
            pre_boil_l: Some(45.0),
        });
        assert!(text.contains("30.00 L"));
        assert!(text.contains("42.00 L"));
        assert!(text.contains("not changed automatically"));
    }

    #[test]
    fn pre_boil_only_capacity_overrun_is_named() {
        let text = warning_text(&ScaleWarning::KettleCapacityExceeded {
            capacity_l: 44.0,
            post_boil_l: 42.0,
            pre_boil_l: Some(45.0),
        });
        assert!(text.contains("pre-boil 45.00 L"));
        assert!(text.contains("post-boil 42.00 L fits"));
        assert!(text.contains("not changed automatically"));
    }

    #[test]
    fn fixed_loss_warning_discloses_retained_constants() {
        let text = warning_text(&ScaleWarning::FixedLossesRetained {
            transfer_loss_l: Some(2.0),
            boil_off_l_per_hour: Some(3.0),
            pre_boil_deadspace_l: Some(0.0),
            grain_absorption_l_per_kg: Some(0.8),
        });
        assert!(text.contains("2.000 L"));
        assert!(text.contains("3.000 L/h"));
        assert!(text.contains("not target matching"));
    }

    #[test]
    fn unscaled_and_derived_mash_warnings_are_honest() {
        assert!(
            warning_text(&ScaleWarning::UnscaledIngredients {
                yeast_rows: 1,
                other_ingredient_rows: 2,
            })
            .contains("preserved, not multiplied")
        );
        assert!(
            warning_text(&ScaleWarning::DerivedMashWater {
                thickness_l_per_kg: 3.0,
            })
            .contains("derived from the scaled grain")
        );
    }

    #[test]
    fn change_text_uses_row_and_field() {
        let change = ScaleChange {
            section: krausen_types::scaling::ScaleSection::Fermentable,
            row_id: Some("g1".into()),
            field: "amount_kg".into(),
            before: 4.0,
            after: 8.0,
        };
        let text = change_text(&change);
        assert!(text.contains("g1"));
        assert!(text.contains("4.0000"));
        assert!(text.contains("8.0000"));
    }
}
