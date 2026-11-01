// SPDX-License-Identifier: AGPL-3.0-or-later
//! Brew start presentation over the shared atomic start operation.
use crate::{
    components::{
        BreweryDisclosure, BreweryDraftField, BreweryMetric, BreweryNotice, BreweryPanel, Button,
        ButtonLink,
    },
    pages::recipe_editor::current_context_matches,
    server_fns::{brew::start_brew_shared, process_summary::request_process_summary_shared},
};
use krausen_types::{
    brew::{StartBrewOutcome, StartBrewRequest},
    brewery::BrewStatusV1,
    formulation::{Preview, Targets},
    process_summary::{
        ProcessSummary, ProcessSummaryOutcome, ProcessSummaryRequest, ProcessSummarySelection,
        SummarySource,
    },
    request_context::OperationIntent,
    validation::{Diagnostic, Readiness},
};
use leptos::prelude::*;

#[derive(Clone, Debug, PartialEq)]
struct StartedView {
    brew_id: String,
    selected_revision: i64,
    revision: i64,
    status: BrewStatusV1,
    targets: Targets,
}

fn diagnostic_text(diagnostic: &Diagnostic) -> String {
    crate::components::printable_recipe::diagnostic_message(diagnostic)
}

fn metric(label: &'static str, value: Option<f64>, unit: &'static str, digits: usize) -> AnyView {
    let formatted = value
        .map(|value| format!("{value:.digits$}"))
        .unwrap_or_else(|| "—".into());
    view! { <BreweryMetric label=label value=formatted unit=unit>{""}</BreweryMetric> }.into_any()
}

fn target_metrics(targets: &Targets) -> AnyView {
    view! {
        <div class="recipe-editor__metrics">
            {metric("Original gravity", targets.original_sg, "SG", 3)}
            {metric("Final gravity", targets.final_sg, "SG", 3)}
            {metric("Alcohol", targets.abv_percent, "%", 1)}
            {metric("Bitterness", targets.ibu, "IBU", 1)}
            {metric("Colour", targets.colour_ebc, "EBC", 1)}
            {metric("Total water", targets.total_water_l, "L", 1)}
        </div>
    }
    .into_any()
}

fn refreshed_metrics(preview: &Preview) -> AnyView {
    target_metrics(&preview.targets)
}

fn may_prepare(pending: Option<&StartBrewRequest>) -> bool {
    pending.is_none()
}

fn invalidate_selection<T: Send + Sync + 'static>(
    prepared: RwSignal<Option<T>>,
    digest: RwSignal<Option<String>>,
    prepared_current: RwSignal<Option<i64>>,
    _pending: RwSignal<Option<StartBrewRequest>>,
) {
    prepared.set(None);
    digest.set(None);
    prepared_current.set(None);
}

fn retry_request(pending: RwSignal<Option<StartBrewRequest>>) -> Option<StartBrewRequest> {
    pending.get_untracked()
}

fn build_start_request(
    intent: OperationIntent,
    recipe_id: String,
    selected: i64,
    digest: Option<String>,
    prepared_current: Option<i64>,
    current: Option<i64>,
    request_id: String,
) -> Option<StartBrewRequest> {
    Some(StartBrewRequest {
        intent,
        recipe_id,
        selected_recipe_revision: selected,
        expected_head_revision: request_head_guard(selected, prepared_current, current),
        expected_calculation_context: digest?,
        request_id,
    })
}

fn prepared_head_guard(selected: i64, prepared_current: Option<i64>) -> Option<i64> {
    (prepared_current == Some(selected)).then_some(selected)
}

fn request_head_guard(
    selected: i64,
    prepared_current: Option<i64>,
    _current: Option<i64>,
) -> Option<i64> {
    prepared_head_guard(selected, prepared_current)
}

#[cfg(test)]
mod regression_tests {
    use super::*;

    #[test]
    fn unresolved_request_survives_selection_and_prepare() {
        let owner = Owner::new();
        owner.with(|| {
            let request = StartBrewRequest {
                intent: OperationIntent {
                    account_id: "account".into(),
                    workspace_id: "workspace".into(),
                },
                recipe_id: "recipe".into(),
                selected_recipe_revision: 2,
                expected_head_revision: Some(2),
                expected_calculation_context: "context".into(),
                request_id: "original-request".into(),
            };
            let pending = RwSignal::new(Some(request.clone()));
            let prepared = RwSignal::new(Some(()));
            let digest = RwSignal::new(Some("context".to_owned()));
            let current = RwSignal::new(Some(2));
            invalidate_selection(prepared, digest, current, pending);
            assert!(!may_prepare(pending.get_untracked().as_ref()));
            assert_eq!(retry_request(pending), Some(request));
            assert!(prepared.get_untracked().is_none());
        });
    }

    #[test]
    fn prepared_current_selection_keeps_original_head_guard() {
        let owner = Owner::new();
        owner.with(|| {
            let prepared = RwSignal::new(Some(()));
            let digest = RwSignal::new(Some("context".to_owned()));
            let current = RwSignal::new(Some(2));
            let pending = RwSignal::new(None);
            let intent = OperationIntent {
                account_id: "account".into(),
                workspace_id: "workspace".into(),
            };
            let guarded = build_start_request(
                intent.clone(),
                "recipe".into(),
                2,
                digest.get_untracked(),
                current.get_untracked(),
                Some(3),
                "id".into(),
            )
            .unwrap();
            assert_eq!(guarded.expected_head_revision, Some(2));
            invalidate_selection(prepared, digest, current, pending);
            assert!(
                build_start_request(
                    intent,
                    "recipe".into(),
                    2,
                    digest.get_untracked(),
                    current.get_untracked(),
                    Some(3),
                    "id".into()
                )
                .is_none()
            );
        });
    }
}

#[component]
pub fn StartBrewPanel(
    identity: RwSignal<OperationIntent>,
    context_visible: RwSignal<bool>,
    recipe_id: RwSignal<Option<String>>,
    revision: RwSignal<Option<i64>>,
) -> impl IntoView {
    let revision_text = RwSignal::new(
        revision
            .get_untracked()
            .map(|revision| revision.to_string())
            .unwrap_or_default(),
    );
    Effect::new(move |_| {
        let current = revision.get();
        if current.is_some() && revision_text.get_untracked().is_empty() {
            revision_text.set(current.unwrap().to_string());
        }
    });
    let prepared = RwSignal::new(None::<ProcessSummary>);
    let prepared_digest = RwSignal::new(None::<String>);
    let prepared_current = RwSignal::new(None::<i64>);
    let pending = RwSignal::new(None::<StartBrewRequest>);
    let started = RwSignal::new(None::<StartedView>);
    let conflict = RwSignal::new(None::<StartBrewOutcome>);
    let status = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let last_current_revision = RwSignal::new(revision.get_untracked());
    Effect::new(move |_| {
        let current = revision.get();
        if current != last_current_revision.get_untracked() {
            if let Some(current) = current {
                revision_text.set(current.to_string());
            }
            invalidate_selection(prepared, prepared_digest, prepared_current, pending);
            last_current_revision.set(current);
        }
    });

    let prepare = Callback::new(move |()| {
        if busy.get_untracked() || !context_visible.get_untracked() {
            return;
        }
        if !may_prepare(pending.get_untracked().as_ref()) {
            status.set(
                "Resolve the previous start by retrying the same request before preparing another."
                    .into(),
            );
            return;
        }
        let Some(recipe_id) = recipe_id.get_untracked() else {
            status.set("Save the recipe before preparing a brew start.".into());
            return;
        };
        let Ok(selected) = revision_text.get_untracked().trim().parse::<i64>() else {
            status.set("Enter a saved revision number starting at 1.".into());
            return;
        };
        if selected <= 0 {
            status.set("Enter a saved revision number starting at 1.".into());
            return;
        }
        let expected = identity.get_untracked();
        let current_at_prepare = revision.get_untracked();
        busy.set(true);
        status.set("Preparing the accepted calculation context…".into());
        started.set(None);
        conflict.set(None);
        leptos::task::spawn_local(async move {
            if !current_context_matches(&expected).await {
                if context_visible.try_get_untracked().is_none() {
                    return;
                }
                context_visible.set(false);
                busy.set(false);
                return;
            }
            if context_visible.try_get_untracked().is_none() {
                return;
            }
            let request = ProcessSummaryRequest {
                intent: expected.clone(),
                selection: ProcessSummarySelection::Saved {
                    recipe_id: recipe_id.clone(),
                    revision: selected,
                },
            };
            #[cfg(target_arch = "wasm32")]
            let retry_recipe_id = recipe_id;
            let result = request_process_summary_shared(request).await;
            #[cfg(target_arch = "wasm32")]
            let result = if result
                .as_ref()
                .err()
                .is_some_and(|e| crate::utils::auth_refresh::is_auth_error(&e.to_string()))
                && crate::utils::auth_refresh::try_refresh().await
                && current_context_matches(&expected).await
            {
                request_process_summary_shared(ProcessSummaryRequest {
                    intent: expected.clone(),
                    selection: ProcessSummarySelection::Saved {
                        recipe_id: retry_recipe_id,
                        revision: selected,
                    },
                })
                .await
            } else {
                result
            };
            if context_visible.try_get_untracked().is_none() {
                return;
            }
            busy.set(false);
            match result {
                Ok(ProcessSummaryOutcome::Summary(summary)) => {
                    if revision.get_untracked() != current_at_prepare
                        || revision_text.get_untracked().trim().parse::<i64>().ok()
                            != Some(selected)
                        || pending.get_untracked().is_some()
                    {
                        status
                            .set("The selection changed. Prepare the saved revision again.".into());
                        return;
                    }
                    let digest = summary.context.digest.clone();
                    prepared.set(Some(summary));
                    prepared_digest.set(digest);
                    prepared_current.set(current_at_prepare);
                    status.set(
                        if prepared
                            .get_untracked()
                            .as_ref()
                            .is_some_and(|summary| summary.readiness == Readiness::Ready)
                        {
                            "Ready to start the selected saved revision.".into()
                        } else {
                            "This revision is not Ready; add the missing inputs and save again."
                                .into()
                        },
                    );
                }
                Ok(ProcessSummaryOutcome::Forbidden | ProcessSummaryOutcome::ContextChanged) => {
                    context_visible.set(false)
                }
                Ok(ProcessSummaryOutcome::UnresolvedEquipmentProfile) => {
                    status.set("The selected equipment profile is unavailable.".into())
                }
                Ok(ProcessSummaryOutcome::UnresolvedStyle) => {
                    status.set("The selected style reference is unavailable.".into())
                }
                Ok(ProcessSummaryOutcome::UnsupportedRevision { schema_version }) => status.set(
                    format!("Revision uses unsupported recipe schema version {schema_version}."),
                ),
                Ok(ProcessSummaryOutcome::NotFound) => {
                    status.set("That saved revision is not available in this workspace.".into())
                }
                Ok(ProcessSummaryOutcome::Invalid(validation)) => status.set(
                    validation
                        .diagnostics
                        .first()
                        .map(diagnostic_text)
                        .unwrap_or_else(|| "Correct the recipe before starting.".into()),
                ),
                _ => status.set(
                    "Unable to prepare the start context. Your saved recipe is unchanged.".into(),
                ),
            }
        });
    });
    let start = Callback::new(move |()| {
        if busy.get_untracked() || !context_visible.get_untracked() {
            return;
        }
        let request = if let Some(pending) = retry_request(pending) {
            pending
        } else {
            let (Some(summary), Some(digest), Some(recipe_id)) = (
                prepared.get_untracked(),
                prepared_digest.get_untracked(),
                recipe_id.get_untracked(),
            ) else {
                status.set("Prepare the selected saved revision before starting.".into());
                return;
            };
            let SummarySource::Saved {
                revision: selected, ..
            } = summary.source
            else {
                status.set("Prepare a saved revision before starting.".into());
                return;
            };
            let Some(request_id) = crate::utils::request_id::new_request_id() else {
                status.set("Unable to create a secure start request.".into());
                return;
            };
            build_start_request(
                identity.get_untracked(),
                recipe_id,
                selected,
                Some(digest),
                prepared_current.get_untracked(),
                revision.get_untracked(),
                request_id,
            )
            .expect("prepared digest")
        };
        pending.set(Some(request.clone()));
        busy.set(true);
        status.set("Starting the brew…".into());
        let expected = request.intent.clone();
        leptos::task::spawn_local(async move {
            if !current_context_matches(&expected).await {
                if context_visible.try_get_untracked().is_none() {
                    return;
                }
                context_visible.set(false);
                busy.set(false);
                return;
            }
            if context_visible.try_get_untracked().is_none() {
                return;
            }
            let result = start_brew_shared(request.clone()).await;
            #[cfg(target_arch = "wasm32")]
            let result = if result
                .as_ref()
                .err()
                .is_some_and(|e| crate::utils::auth_refresh::is_auth_error(&e.to_string()))
                && crate::utils::auth_refresh::try_refresh().await
                && current_context_matches(&expected).await
            {
                start_brew_shared(request).await
            } else {
                result
            };
            if context_visible.try_get_untracked().is_none() {
                return;
            }
            busy.set(false);
            match result {
                Ok(StartBrewOutcome::Started {
                    brew_id,
                    status: brew_status,
                    revision,
                    selected_recipe_revision,
                    plan,
                    ..
                }) => {
                    pending.set(None);
                    prepared.set(None);
                    prepared_digest.set(None);
                    conflict.set(None);
                    started.set(Some(StartedView {
                        brew_id,
                        selected_revision: selected_recipe_revision,
                        revision,
                        status: brew_status,
                        targets: plan.original_targets,
                    }));
                    status.set("Brew started with the frozen plan and targets.".into());
                }
                Ok(StartBrewOutcome::HeadConflict {
                    expected_head,
                    actual_head,
                    refreshed,
                }) => {
                    pending.set(None);
                    prepared.set(None);
                    prepared_digest.set(None);
                    conflict.set(Some(StartBrewOutcome::HeadConflict {
                        expected_head,
                        actual_head,
                        refreshed,
                    }));
                    status.set(format!(
                        "The recipe head moved from {expected_head} to {actual_head}. Nothing was started."
                    ));
                }
                Ok(StartBrewOutcome::ContextConflict { refreshed }) => {
                    pending.set(None);
                    prepared.set(None);
                    prepared_digest.set(None);
                    conflict.set(Some(StartBrewOutcome::ContextConflict { refreshed }));
                    status.set(
                        "The accepted calculation context changed. Review the refreshed targets, prepare again, then start explicitly."
                            .into(),
                    );
                }
                Ok(StartBrewOutcome::NotReady(validation)) => {
                    pending.set(None);
                    prepared.set(None);
                    prepared_digest.set(None);
                    status.set(format!(
                        "Not started: {}",
                        validation
                            .diagnostics
                            .first()
                            .map(diagnostic_text)
                            .unwrap_or_else(|| "the recipe is not Ready.".into())
                    ));
                }
                Ok(StartBrewOutcome::RecipeArchived) => {
                    pending.set(None);
                    status.set("This recipe is archived and cannot start a new brew.".into())
                }
                Ok(StartBrewOutcome::EquipmentProfileArchived) => {
                    pending.set(None);
                    status.set("The selected equipment profile is archived.".into())
                }
                Ok(StartBrewOutcome::UnresolvedEquipmentProfile) => {
                    pending.set(None);
                    status.set("The selected equipment profile is unavailable.".into())
                }
                Ok(StartBrewOutcome::UnresolvedStyle) => {
                    pending.set(None);
                    status.set("The selected style reference is unavailable.".into())
                }
                Ok(StartBrewOutcome::Forbidden | StartBrewOutcome::ContextChanged) => {
                    context_visible.set(false)
                }
                Ok(StartBrewOutcome::InvalidRequest) => {
                    pending.set(None);
                    status.set("The start request was invalid.".into())
                }
                Ok(StartBrewOutcome::RequestIdentityConflict) => {
                    pending.set(None);
                    status.set("The prior start request could not be verified. Prepare again.".into())
                }
                Ok(StartBrewOutcome::ResultExpired { resource_id }) => {
                    pending.set(None);
                    status.set(match resource_id {
                        Some(id) => format!(
                            "The saved start response expired. Brew {id} was already created."
                        ),
                        None => "The saved start response expired.".into(),
                    });
                }
                Ok(StartBrewOutcome::Retryable) | Ok(StartBrewOutcome::Unavailable) => status.set(
                    "Start is temporarily unavailable. Retry; the same request will not create a second brew."
                        .into(),
                ),
                Ok(StartBrewOutcome::NotFound) => {
                    pending.set(None);
                    status.set("The selected recipe revision is not available.".into())
                }
                Err(_) => status.set(
                    "The start result is unknown. Retry; the same request will not create a second brew."
                        .into(),
                ),
            }
        });
    });
    let ready = Signal::derive(move || {
        prepared.with(|summary| {
            summary
                .as_ref()
                .is_some_and(|summary| summary.readiness == Readiness::Ready)
                && prepared_digest.get().is_some()
        })
    });
    let revision_change = Callback::new(move |value: String| {
        revision_text.set(value);
        invalidate_selection(prepared, prepared_digest, prepared_current, pending);
        conflict.set(None);
        started.set(None);
        status.set(String::new());
    });
    view! {
        <BreweryPanel
            title="Start brew"
            description="Freeze one saved revision, its resolved equipment and its original targets. Unsaved editor changes are never started implicitly."
        >
            <HowToStart/>
            <BreweryDraftField
                id="brew-start-revision"
                label="Brew start revision"
                numeric=true
                value=Signal::from(revision_text)
                on_change=revision_change
            />
            <div class="recipe-editor__actions">
                <Button on:click=move |_| prepare.run(()) disabled=busy>"Prepare start"</Button>
                <Button
                    disabled=Signal::derive(move || busy.get() || (pending.get().is_none() && !ready.get()))
                    on:click=move |_| start.run(())
                >
                    {move || if pending.get().is_some() { "Retry start" } else { "Start brew" }}
                </Button>
            </div>
            <p role="status">{move || status.get()}</p>
            {move || prepared.get().map(|summary| view! {
                <div class="recipe-editor__summary">
                    <p>{format!("Selected source: saved revision {}.", match &summary.source {
                        SummarySource::Saved { revision, .. } => revision.to_string(),
                        SummarySource::HistoricalBrew {
                            selected_recipe_revision,
                            ..
                        } => selected_recipe_revision.to_string(),
                        SummarySource::UnsavedDraft { .. } => "unsaved".into(),
                    })}</p>
                    <p>{format!("Readiness: {:?}.", summary.readiness)}</p>
                    {target_metrics(&summary.targets)}
                </div>
                <BreweryDisclosure title="Accepted context and assumptions">
                    <p class="note-caption">{format!("Context: {}", summary.context.digest.clone().unwrap_or_else(|| "unavailable".into()))}</p>
                    <ul>{summary.assumptions.into_iter().map(|assumption| view! { <li>{assumption.replace('_', " ")}</li> }).collect_view()}</ul>
                    <ul>{summary.diagnostics.into_iter().map(|diagnostic| view! { <li>{diagnostic_text(&diagnostic)}</li> }).collect_view()}</ul>
                </BreweryDisclosure>
            })}
            {move || started.get().map(|started| view! {
                <BreweryNotice
                    title="Brew started"
                    body=format!(
                        "Brew {} started from revision {} as aggregate revision {} ({}).",
                        started.brew_id,
                        started.selected_revision,
                        started.revision,
                        started.status.as_str()
                    )
                />
                {target_metrics(&started.targets)}
                <ButtonLink href=format!("/brews/{}", started.brew_id)>"Open brew evidence"</ButtonLink>
            })}
            {move || conflict.get().map(|conflict| view! {
                <BreweryNotice title="Start conflict — nothing was created" body="Review the current values and prepare an explicit start again." warning=true/>
                {match conflict {
                    StartBrewOutcome::HeadConflict { expected_head, actual_head, refreshed } => view! {
                        <p>{format!("Expected head revision {expected_head}; current head is {actual_head}.")}</p>
                        {refreshed.map(|preview| refreshed_metrics(&preview))}
                    }.into_any(),
                    StartBrewOutcome::ContextConflict { refreshed } => view! {
                        <p>"The accepted context no longer matches the current resolved context."</p>
                        {refreshed_metrics(&refreshed)}
                    }.into_any(),
                    _ => view! { <p>"Prepare the start again."</p> }.into_any(),
                }}
            })}
        </BreweryPanel>
    }
}

#[component]
fn HowToStart() -> impl IntoView {
    view! {
        <p class="note-caption">
            "This starts the saved revision shown above, not the editor draft. A dirty draft must be saved or left out of this start. Historical revisions can be entered explicitly and are started without restoring the recipe."
        </p>
    }
}
