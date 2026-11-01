// SPDX-License-Identifier: AGPL-3.0-or-later
//! Minimal brew evidence surface over the shared KRA-21 operations.
use crate::{
    components::{
        BreweryDraftArea, BreweryDraftField, BreweryMetric, BreweryNotice, BreweryPanel, Button,
        ButtonVariant,
    },
    server_fns::brew::{
        brew_archive_shared, brew_evidence_view_shared, brew_measurement_shared, brew_note_shared,
    },
};
use krausen_types::{
    brew::{
        ArchiveBrewRequest, BrewArchiveOutcome, BrewEvidenceView, BrewEvidenceViewOutcome,
        BrewEvidenceViewRequest, BrewLifecycleOutcome, BrewTransitionV1, CorrectMeasurementRequest,
        CorrectNoteRequest, MeasurementCommand, MeasurementListRequest, MeasurementOutcome,
        NoteCommand, NoteOutcome, RecordMeasurementRequest, RecordNoteRequest,
        RetractMeasurementRequest, RetractNoteRequest, SelectMeasurementRequest,
        TransitionBrewRequest,
    },
    request_context::OperationIntent,
};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use std::collections::BTreeMap;

fn measurement_message(outcome: &MeasurementOutcome) -> (bool, String) {
    match outcome {
        MeasurementOutcome::Recorded(_) => (true, "Measurement recorded.".into()),
        MeasurementOutcome::Corrected(_) => (true, "Measurement corrected.".into()),
        MeasurementOutcome::Retracted(_) => (true, "Measurement retracted.".into()),
        MeasurementOutcome::Selected(_) => (true, "Primary reading updated.".into()),
        MeasurementOutcome::Page(_) => (true, String::new()),
        MeasurementOutcome::NotFound => (false, "Brew or reading not found.".into()),
        MeasurementOutcome::Archived => (
            false,
            "This brew is archived. Unarchive it before editing evidence.".into(),
        ),
        MeasurementOutcome::Conflict { expected, actual } => (
            false,
            format!(
                "Changed elsewhere: expected revision {expected}, current {actual}. Reload and retry."
            ),
        ),
        MeasurementOutcome::EntryConflict { .. } => (
            false,
            "That reading was superseded. Reload and retry from the current entry.".into(),
        ),
        MeasurementOutcome::RequestIdentityConflict => (
            false,
            "That request identity was already used for different content.".into(),
        ),
        MeasurementOutcome::ResultExpired { .. } => (
            false,
            "The previous result expired. Start a new request.".into(),
        ),
        MeasurementOutcome::ContextChanged => (
            false,
            "Account or workspace changed. Reopen the brew.".into(),
        ),
        MeasurementOutcome::Forbidden => (false, "You no longer have access to this brew.".into()),
        MeasurementOutcome::Invalid => (false, "That measurement is not valid.".into()),
        MeasurementOutcome::Retryable => (
            false,
            "The reading operation is busy or conflicted; retry the same request.".into(),
        ),
        MeasurementOutcome::Unavailable => {
            (false, "The reading could not be saved. Try again.".into())
        }
    }
}

fn note_message(outcome: &NoteOutcome) -> (bool, String) {
    match outcome {
        NoteOutcome::Recorded(_) => (true, "Note recorded.".into()),
        NoteOutcome::Corrected(_) => (true, "Note corrected; earlier text is retained.".into()),
        NoteOutcome::Retracted(_) => (true, "Note retracted; earlier text is retained.".into()),
        NoteOutcome::Page(_) | NoteOutcome::History(_) => (true, String::new()),
        NoteOutcome::NotFound => (false, "Note not found.".into()),
        NoteOutcome::Archived => (
            false,
            "This brew is archived. Unarchive it before editing notes.".into(),
        ),
        NoteOutcome::Conflict { expected, actual } => (
            false,
            format!(
                "Note changed elsewhere: expected revision {expected}, current {actual}. Reload and retry."
            ),
        ),
        NoteOutcome::RequestIdentityConflict => (
            false,
            "That request identity was already used for different content.".into(),
        ),
        NoteOutcome::ResultExpired { .. } => (
            false,
            "The previous result expired. Start a new request.".into(),
        ),
        NoteOutcome::ContextChanged => (
            false,
            "Account or workspace changed. Reopen the brew.".into(),
        ),
        NoteOutcome::Forbidden => (false, "You no longer have access to this brew.".into()),
        NoteOutcome::Invalid => (false, "That note is not valid.".into()),
        NoteOutcome::Retryable => (
            false,
            "The note operation is busy or conflicted; retry the same request.".into(),
        ),
        NoteOutcome::Unavailable => (false, "The note could not be saved. Try again.".into()),
    }
}

fn lifecycle_message(outcome: &BrewLifecycleOutcome) -> (bool, String) {
    match outcome {
        BrewLifecycleOutcome::Transitioned {
            status, revision, ..
        } => (
            true,
            format!("Brew is now {} at revision {revision}.", status.as_str()),
        ),
        BrewLifecycleOutcome::InvalidTransition { from, to } => (
            false,
            format!(
                "Cannot move from {} to {} in that direction.",
                from.as_str(),
                to.as_str()
            ),
        ),
        BrewLifecycleOutcome::Conflict { expected, actual } => (
            false,
            format!("Brew changed elsewhere: expected revision {expected}, current {actual}."),
        ),
        BrewLifecycleOutcome::NotFound => (false, "Brew not found.".into()),
        BrewLifecycleOutcome::Archived => (
            false,
            "Unarchive the brew before changing lifecycle.".into(),
        ),
        BrewLifecycleOutcome::RequestIdentityConflict => (
            false,
            "That request identity was already used for different content.".into(),
        ),
        BrewLifecycleOutcome::ResultExpired { .. } => {
            (false, "The previous result expired.".into())
        }
        BrewLifecycleOutcome::ContextChanged => (
            false,
            "Account or workspace changed. Reopen the brew.".into(),
        ),
        BrewLifecycleOutcome::Forbidden => {
            (false, "You no longer have access to this brew.".into())
        }
        BrewLifecycleOutcome::Invalid => {
            (false, "A reason is required for this transition.".into())
        }
        BrewLifecycleOutcome::Retryable => (
            false,
            "The lifecycle operation is busy or conflicted; retry the same request.".into(),
        ),
        BrewLifecycleOutcome::Unavailable => (false, "Lifecycle change failed. Try again.".into()),
    }
}

fn archive_message(outcome: &BrewArchiveOutcome) -> (bool, String) {
    match outcome {
        BrewArchiveOutcome::Archived {
            archived,
            metadata_revision,
            ..
        } => (
            true,
            format!(
                "Brew {} at metadata revision {metadata_revision}.",
                if *archived { "archived" } else { "unarchived" }
            ),
        ),
        BrewArchiveOutcome::Conflict { expected, actual } => (
            false,
            format!("Archive state changed elsewhere: expected {expected}, current {actual}."),
        ),
        BrewArchiveOutcome::NotFound => (false, "Brew not found.".into()),
        BrewArchiveOutcome::RequestIdentityConflict => (
            false,
            "That request identity was already used for different content.".into(),
        ),
        BrewArchiveOutcome::ResultExpired { .. } => (false, "The previous result expired.".into()),
        BrewArchiveOutcome::ContextChanged => (
            false,
            "Account or workspace changed. Reopen the brew.".into(),
        ),
        BrewArchiveOutcome::Forbidden => (false, "You no longer have access to this brew.".into()),
        BrewArchiveOutcome::Invalid => (false, "Archive request is not valid.".into()),
        BrewArchiveOutcome::Retryable => (
            false,
            "The archive operation is busy or conflicted; retry the same request.".into(),
        ),
        BrewArchiveOutcome::Unavailable => (false, "Archive change failed. Try again.".into()),
    }
}

#[component]
pub fn BrewEvidencePage() -> impl IntoView {
    let params = use_params_map();
    let load = LocalResource::new(move || {
        let brew_id = params.read().get("id").unwrap_or_default();
        async move {
            let context = crate::server_fns::context::get_user_context().await;
            (brew_id, context)
        }
    });
    view! {
        <Suspense fallback=|| view! { <BreweryNotice title="Loading brew" body="Please wait."/> }>
            {move || Suspend::new(async move {
                match load.await {
                    (brew_id, Ok(ctx)) => match ctx.workspace_id {
                        Some(workspace_id) => view! {
                            <BrewEvidencePanel
                                intent=OperationIntent { account_id: ctx.user_id, workspace_id }
                                brew_id=brew_id
                            />
                        }.into_any(),
                        None => view! {
                            <BreweryNotice title="Workspace required" body="Choose a workspace to view this brew." warning=true/>
                        }.into_any(),
                    },
                    (_, Err(_)) => view! {
                        <BreweryNotice title="Unable to load brew" body="Sign in and try again." warning=true/>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn BrewEvidencePanel(intent: OperationIntent, brew_id: String) -> impl IntoView {
    let intent = StoredValue::new(intent);
    let brew_id = StoredValue::new(brew_id);
    let view = RwSignal::new(None::<BrewEvidenceView>);
    let readings = RwSignal::new(Vec::new());
    let message = RwSignal::new(String::new());
    let warning = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let generation = RwSignal::new(0_u64);
    let inputs = RwSignal::new(BTreeMap::<String, String>::new());
    let note_text = RwSignal::new(String::new());
    let reason = RwSignal::new(String::new());
    let lifecycle_extra = RwSignal::new(Vec::new());
    let lifecycle_cursor = RwSignal::new(None::<String>);

    let load = Callback::new(move |_| {
        let sequence = generation.get_untracked() + 1;
        generation.set(sequence);
        let expected = intent.get_value();
        let id = brew_id.get_value();
        busy.set(true);
        leptos::task::spawn_local(async move {
            let view_outcome = brew_evidence_view_shared(BrewEvidenceViewRequest {
                intent: expected.clone(),
                brew_id: id.clone(),
                lifecycle_cursor: None,
            })
            .await;
            let readings_outcome =
                brew_measurement_shared(MeasurementCommand::List(MeasurementListRequest {
                    intent: expected,
                    brew_id: id,
                    root_observation_id: None,
                    cursor: None,
                    limit: 100,
                }))
                .await;
            if generation.try_get_untracked() != Some(sequence) {
                return;
            }
            busy.set(false);
            match view_outcome {
                Ok(BrewEvidenceViewOutcome::View(data)) => {
                    lifecycle_extra.set(Vec::new());
                    lifecycle_cursor.set(data.lifecycle_next_cursor.clone());
                    view.set(Some(data));
                    if let Ok(MeasurementOutcome::Page(page)) = readings_outcome {
                        readings.set(page.items);
                    }
                }
                Ok(BrewEvidenceViewOutcome::NotFound) => {
                    view.set(None);
                    warning.set(true);
                    message.set("Brew not found in this workspace.".into());
                }
                Ok(BrewEvidenceViewOutcome::Forbidden) => {
                    view.set(None);
                    warning.set(true);
                    message.set("You no longer have access to this brew.".into());
                }
                Ok(BrewEvidenceViewOutcome::ContextChanged) => {
                    view.set(None);
                    warning.set(true);
                    message.set("Account or workspace changed. Reopen the brew.".into());
                }
                Ok(BrewEvidenceViewOutcome::Invalid) => {
                    warning.set(true);
                    message.set("That brew reference is not valid.".into());
                }
                Ok(BrewEvidenceViewOutcome::Retryable) => {
                    warning.set(true);
                    message.set("The brew read is busy; retry the same request.".into());
                }
                Ok(BrewEvidenceViewOutcome::Unavailable) | Err(_) => {
                    warning.set(true);
                    message.set("The brew could not be loaded. Try again.".into());
                }
            }
        });
    });
    on_mount(move || load.run(()));

    let load_lifecycle_more = Callback::new(move |_| {
        let Some(cursor) = lifecycle_cursor.get_untracked() else {
            return;
        };
        let expected = intent.get_value();
        let id = brew_id.get_value();
        busy.set(true);
        leptos::task::spawn_local(async move {
            let outcome = brew_evidence_view_shared(BrewEvidenceViewRequest {
                intent: expected,
                brew_id: id,
                lifecycle_cursor: Some(cursor),
            })
            .await;
            busy.set(false);
            match outcome {
                Ok(BrewEvidenceViewOutcome::View(data)) => {
                    lifecycle_extra.update(|events| events.extend(data.lifecycle));
                    lifecycle_cursor.set(data.lifecycle_next_cursor);
                }
                Ok(BrewEvidenceViewOutcome::Invalid) => {
                    warning.set(true);
                    message.set("Lifecycle continuation expired; reload the brew.".into());
                }
                _ => {
                    warning.set(true);
                    message.set("Older lifecycle events could not be loaded.".into());
                }
            }
        });
    });

    let run_measurement = Callback::new(move |command: MeasurementCommand| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        leptos::task::spawn_local(async move {
            let outcome = brew_measurement_shared(command).await;
            busy.set(false);
            match outcome {
                Ok(outcome) => {
                    let (ok, text) = measurement_message(&outcome);
                    warning.set(!ok);
                    if !text.is_empty() {
                        message.set(text);
                    }
                    if ok {
                        load.run(());
                    }
                }
                Err(error) => {
                    warning.set(true);
                    message.set(format!("Reading request failed: {error}"));
                }
            }
        });
    });

    let run_note = Callback::new(move |command: NoteCommand| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        leptos::task::spawn_local(async move {
            let outcome = brew_note_shared(command).await;
            busy.set(false);
            match outcome {
                Ok(outcome) => {
                    let (ok, text) = note_message(&outcome);
                    warning.set(!ok);
                    if !text.is_empty() {
                        message.set(text);
                    }
                    if ok {
                        load.run(());
                    }
                }
                Err(error) => {
                    warning.set(true);
                    message.set(format!("Note request failed: {error}"));
                }
            }
        });
    });

    let run_transition = Callback::new(move |transition: BrewTransitionV1| {
        let Some(data) = view.get_untracked() else {
            return;
        };
        let reason_text = reason.get_untracked().trim().to_owned();
        let needs_reason = matches!(transition, BrewTransitionV1::Reopen)
            || matches!(
                data.status,
                krausen_types::brewery::BrewStatusV1::Completed
                    | krausen_types::brewery::BrewStatusV1::Cancelled
            );
        if needs_reason && reason_text.is_empty() {
            warning.set(true);
            message.set("Enter a reason before reopening or correcting a terminal brew.".into());
            return;
        }
        let Some(request_id) = crate::utils::request_id::new_request_id() else {
            warning.set(true);
            message.set("Unable to create a secure request identity.".into());
            return;
        };
        busy.set(true);
        leptos::task::spawn_local(async move {
            let outcome = crate::server_fns::brew::brew_lifecycle_shared(TransitionBrewRequest {
                intent: intent.get_value(),
                brew_id: brew_id.get_value(),
                expected_revision: data.revision,
                transition,
                reason: (!reason_text.is_empty()).then_some(reason_text),
                request_id,
            })
            .await;
            busy.set(false);
            match outcome {
                Ok(outcome) => {
                    let (ok, text) = lifecycle_message(&outcome);
                    warning.set(!ok);
                    message.set(text);
                    if ok {
                        load.run(());
                    }
                }
                Err(error) => {
                    warning.set(true);
                    message.set(format!("Lifecycle request failed: {error}"));
                }
            }
        });
    });

    let run_archive = Callback::new(move |archived: bool| {
        let Some(data) = view.get_untracked() else {
            return;
        };
        let Some(request_id) = crate::utils::request_id::new_request_id() else {
            warning.set(true);
            message.set("Unable to create a secure request identity.".into());
            return;
        };
        busy.set(true);
        leptos::task::spawn_local(async move {
            let outcome = brew_archive_shared(ArchiveBrewRequest {
                intent: intent.get_value(),
                brew_id: brew_id.get_value(),
                archived,
                expected_metadata_revision: data.metadata_revision,
                request_id,
            })
            .await;
            busy.set(false);
            match outcome {
                Ok(outcome) => {
                    let (ok, text) = archive_message(&outcome);
                    warning.set(!ok);
                    message.set(text);
                    if ok {
                        load.run(());
                    }
                }
                Err(error) => {
                    warning.set(true);
                    message.set(format!("Archive request failed: {error}"));
                }
            }
        });
    });

    view! {
        <div class="recipe-editor">
            <BreweryPanel
                title="Brew evidence"
                description="Readings, notes, lifecycle and archive all use the same shared operations as MCP."
            >
                {move || (!message.get().is_empty()).then(|| view! {
                    <BreweryNotice title="Brew evidence" body=message.get() warning=warning.get()/>
                })}
                {move || view.get().map(|data| {
                    let status = data.status.as_str().to_owned();
                    let actual_source = if data.actual.is_some() { "actual amendment" } else { "frozen plan" };
                    let boil_minutes = data.actual.as_ref()
                        .map(|actual| actual.document.boil_minutes)
                        .unwrap_or(data.plan.recipe.boil_minutes);
                    let archive_label = if data.archived { "Unarchive brew" } else { "Archive brew" };
                    let archived = data.archived;
                    view! {
                        <div class="recipe-editor__metrics">
                            <BreweryMetric label="Status" value=status.clone() unit="">{""}</BreweryMetric>
                            <BreweryMetric label="Revision" value=data.revision.to_string() unit="">{""}</BreweryMetric>
                            <BreweryMetric label="Metadata revision" value=data.metadata_revision.to_string() unit="">{""}</BreweryMetric>
                            <BreweryMetric label="Actual source" value=actual_source.to_string() unit="">{""}</BreweryMetric>
                            <BreweryMetric
                                label="Boil duration"
                                value=boil_minutes.map(|value| format!("{value:.0}")).unwrap_or_else(|| "—".into())
                                unit="min"
                            >{""}</BreweryMetric>
                        </div>
                        <p class="note-caption">
                            {format!(
                                "Started {} by {}. Recipe revision {} (frozen). Archived: {}.",
                                data.started_at,
                                data.started_by,
                                data.selected_recipe_revision,
                                if data.archived { "yes" } else { "no" }
                            )}
                        </p>
                        <div class="recipe-editor__actions">
                            <Button
                                disabled=busy
                                on:click=move |_| run_transition.run(BrewTransitionV1::Complete)
                            >"Complete"</Button>
                            <Button
                                variant=ButtonVariant::Secondary
                                disabled=busy
                                on:click=move |_| run_transition.run(BrewTransitionV1::Cancel)
                            >"Cancel brew"</Button>
                            <Button
                                variant=ButtonVariant::Secondary
                                disabled=busy
                                on:click=move |_| run_transition.run(BrewTransitionV1::Reopen)
                            >"Reopen"</Button>
                            <Button
                                variant=ButtonVariant::Secondary
                                disabled=busy
                                on:click=move |_| run_archive.run(!archived)
                            >{archive_label.to_string()}</Button>
                        </div>
                        <BreweryDraftField
                            id="brew-transition-reason"
                            label="Reason for reopen or terminal correction"
                            value=Signal::from(reason)
                            on_change=Callback::new(move |text: String| reason.set(text))
                            hint="Required for reopen and for changing one terminal status to another."
                        />

                        <h3 class="recipe-editor__section-title">"Measured actuals"</h3>
                        <p class="note-caption">
                            "The first reading for a quantity is selected atomically. Extra readings never replace the primary implicitly; select one explicitly."
                        </p>
                        {data.measurements.into_iter().map(|slot| {
                            let quantity_id = slot.quantity_id.clone();
                            let field_id = format!("measurement-{}", slot.quantity_id.replace('.', "-"));
                            let input_value = Signal::derive({
                                let quantity_id = quantity_id.clone();
                                move || inputs.get().get(&quantity_id).cloned().unwrap_or_default()
                            });
                            let input_change = Callback::new({
                                let quantity_id = quantity_id.clone();
                                move |text: String| inputs.update(|values| {
                                    values.insert(quantity_id.clone(), text);
                                })
                            });
                            let record_quantity = quantity_id.clone();
                            let record = move |_| {
                                let raw = inputs
                                    .get_untracked()
                                    .get(&record_quantity)
                                    .cloned()
                                    .unwrap_or_default();
                                let Ok(value) = raw.trim().parse::<f64>() else {
                                    warning.set(true);
                                    message.set("Enter a numeric reading.".into());
                                    return;
                                };
                                let Some(request_id) = crate::utils::request_id::new_request_id() else {
                                    warning.set(true);
                                    message.set("Unable to create a secure request identity.".into());
                                    return;
                                };
                                run_measurement.run(MeasurementCommand::Record(RecordMeasurementRequest {
                                    intent: intent.get_value(),
                                    brew_id: brew_id.get_value(),
                                    quantity_id: record_quantity.clone(),
                                    value,
                                    observed_at: None,
                                    source: "manual".into(),
                                    metadata: Default::default(),
                                    request_id,
                                }));
                            };
                            let current = slot.current.clone();
                            let selection_revision = slot.selection_revision;
                            let correct_quantity = quantity_id.clone();
                            let correct = {
                                let current = current.clone();
                                move |_| {
                                    let Some(current) = current.clone() else { return; };
                                    let raw = inputs
                                        .get_untracked()
                                        .get(&correct_quantity)
                                        .cloned()
                                        .unwrap_or_default();
                                    let Ok(value) = raw.trim().parse::<f64>() else {
                                        warning.set(true);
                                        message.set("Enter a numeric reading.".into());
                                        return;
                                    };
                                    let Some(request_id) = crate::utils::request_id::new_request_id() else {
                                        warning.set(true);
                                        message.set("Unable to create a secure request identity.".into());
                                        return;
                                    };
                                    run_measurement.run(MeasurementCommand::Correct(CorrectMeasurementRequest {
                                        intent: intent.get_value(),
                                        brew_id: brew_id.get_value(),
                                        entry_id: current.entry_id,
                                        expected_selection_revision: Some(selection_revision),
                                        value,
                                        source: "manual".into(),
                                        metadata: None,
                                        request_id,
                                    }));
                                }
                            };
                            let retract = {
                                let current = current.clone();
                                move |_| {
                                    let Some(current) = current.clone() else { return; };
                                    let Some(request_id) = crate::utils::request_id::new_request_id() else {
                                        warning.set(true);
                                        message.set("Unable to create a secure request identity.".into());
                                        return;
                                    };
                                    run_measurement.run(MeasurementCommand::Retract(RetractMeasurementRequest {
                                        intent: intent.get_value(),
                                        brew_id: brew_id.get_value(),
                                        entry_id: current.entry_id,
                                        expected_selection_revision: Some(selection_revision),
                                        request_id,
                                    }));
                                }
                            };
                            let selected_root = slot.selected_root_observation_id.clone();
                            let candidates = readings.get().into_iter()
                                .filter(|entry| entry.quantity_id == quantity_id && entry.value.is_some())
                                .collect::<Vec<_>>();
                            let target = slot.target.map(|value| format!("{value:.3}")).unwrap_or_else(|| "—".into());
                            let current_value = slot.current.as_ref()
                                .and_then(|entry| entry.value)
                                .map(|value| format!("{value:.3}"))
                                .unwrap_or_else(|| "—".into());
                            view! {
                                <div class="recipe-editor__summary">
                                    <h4>{format!("{} ({})", slot.label, slot.unit)}</h4>
                                    <p class="note-caption">
                                        {format!("Target {} · selected {}", target, current_value)}
                                    </p>
                                    <BreweryDraftField
                                        id=field_id
                                        label=slot.label
                                        value=input_value
                                        on_change=input_change
                                        unit=slot.unit
                                        numeric=true
                                    />
                                    <div class="recipe-editor__actions">
                                        <Button disabled=busy on:click=record>"Record reading"</Button>
                                        {current.clone().map(|_| view! {
                                            <Button variant=ButtonVariant::Secondary disabled=busy on:click=correct>"Correct selected"</Button>
                                            <Button variant=ButtonVariant::Secondary disabled=busy on:click=retract>"Retract selected"</Button>
                                        })}
                                    </div>
                                    {(!candidates.is_empty()).then(|| view! {
                                        <ul>
                                            {candidates.into_iter().map(|entry| {
                                                let root = entry.root_observation_id.clone();
                                                let selected = selected_root.as_deref() == Some(root.as_str());
                                                let select_quantity = quantity_id.clone();
                                                let select = move |_| {
                                                    let Some(request_id) = crate::utils::request_id::new_request_id() else {
                                                        warning.set(true);
                                                        message.set("Unable to create a secure request identity.".into());
                                                        return;
                                                    };
                                                    run_measurement.run(MeasurementCommand::Select(SelectMeasurementRequest {
                                                        intent: intent.get_value(),
                                                        brew_id: brew_id.get_value(),
                                                        quantity_id: select_quantity.clone(),
                                                        root_observation_id: Some(root.clone()),
                                                        expected_selection_revision: selection_revision,
                                                        request_id,
                                                    }));
                                                };
                                                view! {
                                                    <li>
                                                        <span>{format!("{:?} at {}", entry.value, entry.observed_at)}</span>
                                                        {(!selected).then(|| view! {
                                                            <Button variant=ButtonVariant::Secondary disabled=busy on:click=select>"Select"</Button>
                                                        })}
                                                    </li>
                                                }
                                            }).collect_view()}
                                        </ul>
                                    })}
                                </div>
                            }
                        }).collect_view()}

                        <h3 class="recipe-editor__section-title">"Notes"</h3>
                        <BreweryDraftArea
                            id="brew-note"
                            label="New note"
                            value=Signal::from(note_text)
                            on_change=Callback::new(move |text: String| note_text.set(text))
                        />
                        <div class="recipe-editor__actions">
                            <Button
                                disabled=busy
                                on:click=move |_| {
                                    let text = note_text.get_untracked();
                                    if text.trim().is_empty() {
                                        warning.set(true);
                                        message.set("Enter note text.".into());
                                        return;
                                    }
                                    let Some(request_id) = crate::utils::request_id::new_request_id() else {
                                        warning.set(true);
                                        message.set("Unable to create a secure request identity.".into());
                                        return;
                                    };
                                    run_note.run(NoteCommand::Record(RecordNoteRequest {
                                        intent: intent.get_value(),
                                        brew_id: brew_id.get_value(),
                                        text,
                                        request_id,
                                    }));
                                }
                            >"Record note"</Button>
                        </div>
                        <p class="note-caption">
                            {if data.notes_truncated { "Showing the newest 50 notes; use the shared list operation for more." } else { "" }}
                        </p>
                        <ul>
                            {data.notes.into_iter().map(|note_view| {
                                let note_id = note_view.note.note_id.clone();
                                let revision = note_view.current.revision;
                                let text = note_view.current.document.text.clone().unwrap_or_else(|| "(retracted)".into());
                                let correct_note = {
                                    let note_id = note_id.clone();
                                    move |_| {
                                        let text = note_text.get_untracked();
                                        if text.trim().is_empty() {
                                            warning.set(true);
                                            message.set("Enter replacement note text.".into());
                                            return;
                                        }
                                        let Some(request_id) = crate::utils::request_id::new_request_id() else {
                                            warning.set(true);
                                            message.set("Unable to create a secure request identity.".into());
                                            return;
                                        };
                                        run_note.run(NoteCommand::Correct(CorrectNoteRequest {
                                            intent: intent.get_value(),
                                            brew_id: brew_id.get_value(),
                                            note_id: note_id.clone(),
                                            expected_revision: revision,
                                            text,
                                            reason: None,
                                            request_id,
                                        }));
                                    }
                                };
                                let retract_note = {
                                    let note_id = note_id.clone();
                                    move |_| {
                                        let Some(request_id) = crate::utils::request_id::new_request_id() else {
                                            warning.set(true);
                                            message.set("Unable to create a secure request identity.".into());
                                            return;
                                        };
                                        run_note.run(NoteCommand::Retract(RetractNoteRequest {
                                            intent: intent.get_value(),
                                            brew_id: brew_id.get_value(),
                                            note_id: note_id.clone(),
                                            expected_revision: revision,
                                            reason: None,
                                            request_id,
                                        }));
                                    }
                                };
                                view! {
                                    <li>
                                        <p>{text}</p>
                                        <p class="note-caption">{format!("Revision {revision}")}</p>
                                        <div class="recipe-editor__actions">
                                            <Button variant=ButtonVariant::Secondary disabled=busy on:click=correct_note>"Correct with draft"</Button>
                                            <Button variant=ButtonVariant::Secondary disabled=busy on:click=retract_note>"Retract"</Button>
                                        </div>
                                    </li>
                                }
                            }).collect_view()}
                        </ul>

                        <h3 class="recipe-editor__section-title">"Lifecycle evidence"</h3>
                        <ul>
                            {data.lifecycle.into_iter().chain(lifecycle_extra.get().into_iter()).map(|event| view! {
                                <li>
                                    {format!(
                                        "Revision {}: {} → {}{}",
                                        event.brew_revision,
                                        event.from_status.as_str(),
                                        event.to_status.as_str(),
                                        event.reason.as_ref().map(|reason| format!(" ({reason})")).unwrap_or_default()
                                    )}
                                </li>
                            }).collect_view()}
                        </ul>
                        {lifecycle_cursor.get().map(|_| view! {
                            <Button
                                variant=ButtonVariant::Secondary
                                disabled=busy
                                on:click=move |_| load_lifecycle_more.run(())
                            >"Load older lifecycle"</Button>
                        })}
                    }
                })}
            </BreweryPanel>
        </div>
    }
}
