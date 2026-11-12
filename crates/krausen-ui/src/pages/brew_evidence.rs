// SPDX-License-Identifier: AGPL-3.0-or-later
//! Minimal brew evidence surface over the shared KRA-21 operations.
use crate::{
    components::{
        BreweryDisclosure, BreweryDraftArea, BreweryDraftField, BreweryMetric, BreweryNotice,
        BreweryPanel, Button, ButtonVariant,
        brew_evidence_rows::BrewProvenanceFact,
        layout::{EditorGuard, GuardScope, GuardTaskCallbacks, GuardTaskKind, GuardTaskState},
    },
    pages::brew_history_presentation::{
        actor_text, format_measurement_value, format_timestamp, status_label,
    },
    pages::recipe_editor::current_context_matches,
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

#[path = "evidence_recovery.rs"]
mod evidence_recovery;
use evidence_recovery::{EvidenceReceipt, EvidenceRecovery, RecoveryStatus};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
enum PendingEvidence {
    Measurement(MeasurementCommand),
    Note(NoteCommand),
    Lifecycle(TransitionBrewRequest),
    Archive(ArchiveBrewRequest),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RecoveryInventory {
    Loading,
    Ready,
    Failed(String),
}

fn measurement_is_uncertain(outcome: &MeasurementOutcome) -> bool {
    matches!(
        outcome,
        MeasurementOutcome::Retryable | MeasurementOutcome::Unavailable
    )
}

fn measurement_needs_refresh(outcome: &MeasurementOutcome) -> bool {
    matches!(
        outcome,
        MeasurementOutcome::Conflict { .. } | MeasurementOutcome::EntryConflict { .. }
    )
}

fn note_is_uncertain(outcome: &NoteOutcome) -> bool {
    matches!(outcome, NoteOutcome::Retryable | NoteOutcome::Unavailable)
}

fn note_needs_refresh(outcome: &NoteOutcome) -> bool {
    matches!(outcome, NoteOutcome::Conflict { .. })
}

fn lifecycle_is_uncertain(outcome: &BrewLifecycleOutcome) -> bool {
    matches!(
        outcome,
        BrewLifecycleOutcome::Retryable | BrewLifecycleOutcome::Unavailable
    )
}

fn lifecycle_needs_refresh(outcome: &BrewLifecycleOutcome) -> bool {
    matches!(outcome, BrewLifecycleOutcome::Conflict { .. })
}

fn archive_is_uncertain(outcome: &BrewArchiveOutcome) -> bool {
    matches!(
        outcome,
        BrewArchiveOutcome::Retryable | BrewArchiveOutcome::Unavailable
    )
}

fn archive_needs_refresh(outcome: &BrewArchiveOutcome) -> bool {
    matches!(outcome, BrewArchiveOutcome::Conflict { .. })
}

fn measurement_recovery_status(outcome: &MeasurementOutcome) -> RecoveryStatus {
    match outcome {
        MeasurementOutcome::ResultExpired { resource_id } => RecoveryStatus::Expired {
            resource_id: resource_id.clone(),
        },
        MeasurementOutcome::RequestIdentityConflict => RecoveryStatus::IdentityConflict,
        MeasurementOutcome::Forbidden
        | MeasurementOutcome::ContextChanged
        | MeasurementOutcome::Retryable
        | MeasurementOutcome::Unavailable => RecoveryStatus::Unknown,
        _ => {
            let (accepted, message) = measurement_message(outcome);
            RecoveryStatus::Confirmed { accepted, message }
        }
    }
}
fn note_recovery_status(outcome: &NoteOutcome) -> RecoveryStatus {
    match outcome {
        NoteOutcome::ResultExpired { resource_id } => RecoveryStatus::Expired {
            resource_id: resource_id.clone(),
        },
        NoteOutcome::RequestIdentityConflict => RecoveryStatus::IdentityConflict,
        NoteOutcome::Forbidden
        | NoteOutcome::ContextChanged
        | NoteOutcome::Retryable
        | NoteOutcome::Unavailable => RecoveryStatus::Unknown,
        _ => {
            let (accepted, message) = note_message(outcome);
            RecoveryStatus::Confirmed { accepted, message }
        }
    }
}
fn lifecycle_recovery_status(outcome: &BrewLifecycleOutcome) -> RecoveryStatus {
    match outcome {
        BrewLifecycleOutcome::ResultExpired { resource_id } => RecoveryStatus::Expired {
            resource_id: resource_id.clone(),
        },
        BrewLifecycleOutcome::RequestIdentityConflict => RecoveryStatus::IdentityConflict,
        BrewLifecycleOutcome::Forbidden
        | BrewLifecycleOutcome::ContextChanged
        | BrewLifecycleOutcome::Retryable
        | BrewLifecycleOutcome::Unavailable => RecoveryStatus::Unknown,
        _ => {
            let (accepted, message) = lifecycle_message(outcome);
            RecoveryStatus::Confirmed { accepted, message }
        }
    }
}
fn archive_recovery_status(outcome: &BrewArchiveOutcome) -> RecoveryStatus {
    match outcome {
        BrewArchiveOutcome::ResultExpired { resource_id } => RecoveryStatus::Expired {
            resource_id: resource_id.clone(),
        },
        BrewArchiveOutcome::RequestIdentityConflict => RecoveryStatus::IdentityConflict,
        BrewArchiveOutcome::Forbidden
        | BrewArchiveOutcome::ContextChanged
        | BrewArchiveOutcome::Retryable
        | BrewArchiveOutcome::Unavailable => RecoveryStatus::Unknown,
        _ => {
            let (accepted, message) = archive_message(outcome);
            RecoveryStatus::Confirmed { accepted, message }
        }
    }
}

fn evidence_dirty(
    inputs: &BTreeMap<String, String>,
    note_text: &str,
    reason: &str,
    pending: bool,
) -> bool {
    inputs.values().any(|value| !value.trim().is_empty())
        || !note_text.trim().is_empty()
        || !reason.trim().is_empty()
        || pending
}

fn pending_after_preflight_failure(
    action: PendingEvidence,
    retained: bool,
) -> Option<PendingEvidence> {
    retained.then_some(action)
}

fn continuation_owned(
    attempt: Option<u64>,
    expected: u64,
    cursor: Option<&str>,
    requested: &str,
) -> bool {
    attempt == Some(expected) && cursor == Some(requested)
}

fn retry_action(pending: Option<&PendingEvidence>) -> Option<PendingEvidence> {
    pending.cloned()
}

fn same_original_evidence(left: &PendingEvidence, right: &PendingEvidence) -> bool {
    match (
        krausen_types::request_context::checked_json(left),
        krausen_types::request_context::checked_json(right),
    ) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn expired_result_message(resource_id: &Option<String>) -> String {
    format!(
        "The previous result expired{}. This does not prove the write was absent. Review the existing evidence; the original command remains retained and must not be re-executed or automatically replaced.",
        resource_id
            .as_ref()
            .map(|id| format!(" for resource {id}"))
            .unwrap_or_default()
    )
}

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
        MeasurementOutcome::ResultExpired { resource_id } => {
            (false, expired_result_message(resource_id))
        }
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
        NoteOutcome::ResultExpired { resource_id } => (false, expired_result_message(resource_id)),
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
            format!(
                "Brew is now {} at revision {revision}.",
                status_label(*status)
            ),
        ),
        BrewLifecycleOutcome::InvalidTransition { from, to } => (
            false,
            format!(
                "Cannot move from {} to {} in that direction.",
                status_label(*from),
                status_label(*to)
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
        BrewLifecycleOutcome::ResultExpired { resource_id } => {
            (false, expired_result_message(resource_id))
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
        BrewArchiveOutcome::ResultExpired { resource_id } => {
            (false, expired_result_message(resource_id))
        }
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
    let editor_guard = expect_context::<EditorGuard>();
    let view = RwSignal::new(None::<BrewEvidenceView>);
    let readings = RwSignal::new(Vec::new());
    let message = RwSignal::new(String::new());
    let warning = RwSignal::new(false);
    let read_busy = RwSignal::new(false);
    let write_busy = RwSignal::new(false);
    let write_attempt = RwSignal::new(0_u64);
    let context_generation = RwSignal::new(0_u64);
    let context_valid = RwSignal::new(true);
    let pending = RwSignal::new(None::<PendingEvidence>);
    let generation = RwSignal::new(0_u64);
    let inputs = RwSignal::new(BTreeMap::<String, String>::new());
    let input_generation = RwSignal::new(0_u64);
    let note_text = RwSignal::new(String::new());
    let reason = RwSignal::new(String::new());
    let last_field = StoredValue::new("brew-note".to_owned());
    let lifecycle_extra = RwSignal::new(Vec::new());
    let lifecycle_cursor = RwSignal::new(None::<String>);
    let continuation_busy = RwSignal::new(false);
    let continuation_attempt = RwSignal::new(0_u64);
    let original = intent.get_value();
    let scope = StoredValue::new(GuardScope {
        installation: GuardScope::installation(),
        account_id: original.account_id,
        workspace_id: original.workspace_id,
        resource_id: Some(brew_id.get_value()),
        base_revision: None,
    });
    let recovery_key = RwSignal::new(None::<String>);
    #[cfg(target_arch = "wasm32")]
    let task_session = StoredValue::new(crate::utils::request_id::new_request_id());
    #[cfg(not(target_arch = "wasm32"))]
    let task_session = StoredValue::new(None::<String>);
    // The session lock marks only a held, in-flight write. When the response
    // settles or is lost the lock is released, so the retained record becomes
    // an ordinary recoverable (uncertain) record visible from any tab.
    let session_lock = StoredValue::new(None::<evidence_recovery::SessionLock>);
    Effect::new(move |_| {
        if write_busy.get() {
            if session_lock.with_value(|lock| lock.is_none()) {
                session_lock.set_value(Some(evidence_recovery::SessionLock::acquire(
                    task_session.get_value().as_deref().unwrap_or_default(),
                )));
            }
        } else if let Some(mut lock) = session_lock.try_update_value(|lock| lock.take()).flatten() {
            lock.release();
        }
    });
    on_cleanup(move || {
        if let Some(mut lock) = session_lock.try_update_value(|lock| lock.take()).flatten() {
            lock.release();
        }
    });
    let recovery_blocked = RwSignal::new(false);
    let recovery_status = RwSignal::new(RecoveryStatus::Unknown);
    let reconciliation_ready = RwSignal::new(false);
    let reviewed_records = RwSignal::new(Vec::<(String, RecoveryStatus)>::new());
    let lock_recovery = Callback::new(move |status: RecoveryStatus| {
        recovery_blocked.set(true);
        reconciliation_ready.set(false);
        recovery_status.set(status.clone());
        if let Some(key) = recovery_key.get_untracked() {
            if let Err(error) = evidence_recovery::set_status(&key, status.clone()) {
                warning.set(true);
                message.set(format!("{} Local blocker retention failed: {error}. Keep the original request; do not create a replacement.", status.message()));
            }
        }
    });
    let recovery_inventory = RwSignal::new(RecoveryInventory::Loading);
    let recovery_cleanup_error = RwSignal::new(String::new());
    let recovery_records = RwSignal::new(Vec::<(String, EvidenceRecovery)>::new());
    // Records whose owning tab still holds its in-flight lock: visible and
    // checkable here, but another tab's held write must not block this task.
    let live_owned_keys = RwSignal::new(Vec::<String>::new());
    let blocking_records = Signal::derive(move || {
        let live = live_owned_keys.get();
        recovery_records.get().iter().any(|(key, _)| !live.contains(key))
    });
    let cleanup_debt = RwSignal::new(Vec::<(String, RecoveryStatus)>::new());
    let settled = Callback::new(
        move |(key, (accepted, summary)): (String, (bool, String))| {
            let status = RecoveryStatus::Confirmed {
                accepted,
                message: summary,
            };
            if let Err(error) =
                evidence_recovery::cleanup_confirmed(&scope.get_value(), &key, status.clone())
            {
                let notice = format!(
                    "Result confirmed, but local recovery cleanup failed: {error}. Do not send a new request to compensate."
                );
                recovery_cleanup_error.set(notice);
                cleanup_debt.update(|debts| {
                    debts.retain(|(existing, _)| existing != &key);
                    debts.push((key.clone(), status));
                });
            } else {
                cleanup_debt.update(|debts| debts.retain(|(candidate, _)| candidate != &key));
                recovery_records
                    .update(|records| records.retain(|(candidate, _)| candidate != &key));
            }
            if recovery_key.get_untracked().as_deref() == Some(&key) {
                recovery_key.set(None);
                pending.set(None);
            }
        },
    );

    let adopt_selected_status = Callback::new(move |(key, record): (String, EvidenceRecovery)| {
        if !context_valid.get_untracked()
            || !record.matches(&scope.get_value())
            || recovery_key.get_untracked().as_deref() != Some(&key)
            || !pending
                .get_untracked()
                .as_ref()
                .is_some_and(|original| same_original_evidence(original, &record.command))
            || record.status == RecoveryStatus::Unknown
        {
            return false;
        }
        recovery_records.update(|records| records.retain(|(candidate, _)| candidate != &key));
        reconciliation_ready.set(false);
        recovery_status.set(record.status.clone());
        message.set(record.status.message());
        match &record.status {
            RecoveryStatus::Confirmed { .. } => {
                cleanup_debt.update(|debts| {
                    debts.retain(|(candidate, _)| candidate != &key);
                    debts.push((key.clone(), record.status.clone()));
                });
                pending.set(None);
                recovery_key.set(None);
                recovery_blocked.set(false);
            }
            RecoveryStatus::ReviewedExpired { .. } | RecoveryStatus::ReviewedIdentityConflict => {
                reviewed_records.update(|records| {
                    records.retain(|(candidate, _)| candidate != &key);
                    records.push((key.clone(), record.status.clone()));
                });
                pending.set(None);
                recovery_key.set(None);
                recovery_blocked.set(false);
            }
            RecoveryStatus::Expired { .. } | RecoveryStatus::IdentityConflict => {
                recovery_blocked.set(true);
            }
            RecoveryStatus::Unknown => unreachable!(),
        }
        true
    });

    let load = Callback::new(move |_| {
        let sequence = generation.get_untracked() + 1;
        generation.set(sequence);
        let expected = intent.get_value();
        let expected_context = expected.clone();
        let id = brew_id.get_value();
        read_busy.set(true);
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
            let context_sequence = context_generation.get_untracked();
            if !current_context_matches(&expected_context).await {
                if generation.try_get_untracked() != Some(sequence)
                    || context_generation.try_get_untracked() != Some(context_sequence)
                {
                    return;
                }
                context_valid.set(false);
                view.set(None);
                readings.set(Vec::new());
                read_busy.set(false);
                return;
            }
            if generation.try_get_untracked() != Some(sequence)
                || context_generation.try_get_untracked() != Some(context_sequence)
            {
                return;
            }
            context_valid.set(true);
            if recovery_inventory.get_untracked() != RecoveryInventory::Ready {
                let loaded = match evidence_recovery::load(&scope.get_value()) {
                    Ok(records) => {
                        let own = task_session.get_value().unwrap_or_default();
                        let mut live = Vec::new();
                        let mut ordinary = Vec::new();
                        for (key, record) in records {
                            if evidence_recovery::owned_by_live_tab(&record, &own).await {
                                live.push((key, record));
                            } else {
                                ordinary.push((key, record));
                            }
                        }
                        Ok((ordinary, live))
                    }
                    Err(error) => Err(error),
                };
                match loaded {
                    Ok((records, live_records)) => {
                        let mut unresolved = Vec::new();
                        // A failed durable marker cannot erase this mounted
                        // owner's stronger exact-key receipt knowledge.
                        let mut debts = cleanup_debt.get_untracked();
                        let mut reviewed = Vec::new();
                        for (key, record) in records {
                            if debts.iter().any(|(known, _)| known == &key) {
                                continue;
                            }
                            if matches!(
                                &record.status,
                                RecoveryStatus::ReviewedExpired { .. }
                                    | RecoveryStatus::ReviewedIdentityConflict
                            ) {
                                reviewed.push((key, record.status));
                            } else if matches!(&record.status, RecoveryStatus::Confirmed { .. }) {
                                debts.push((key, record.status));
                            } else {
                                unresolved.push((key, record));
                            }
                        }
                        if !write_busy.get_untracked()
                            && recovery_key
                                .get_untracked()
                                .is_some_and(|current| debts.iter().any(|(key, _)| key == &current))
                        {
                            pending.set(None);
                            recovery_key.set(None);
                        }
                        cleanup_debt.set(debts);
                        reviewed_records.set(reviewed);
                        let mut records = unresolved;
                        recovery_records.set(Vec::new());
                        let preserves_other_original = write_busy.get_untracked()
                            || (pending.get_untracked().is_some()
                                && recovery_key.get_untracked().is_none_or(|current| {
                                    records.first().is_some_and(|(key, _)| key != &current)
                                }));
                        if records.len() == 1 && live_records.is_empty() && !preserves_other_original {
                            let (key, record) = records.remove(0);
                            recovery_blocked.set(record.status != RecoveryStatus::Unknown);
                            recovery_status.set(record.status.clone());
                            if record.status != RecoveryStatus::Unknown {
                                warning.set(true);
                                message.set(record.status.message());
                            }
                            recovery_key.set(Some(key));
                            pending.set(Some(record.command));
                        } else {
                            recovery_records.set(records);
                        }
                        live_owned_keys.set(live_records.iter().map(|(key, _)| key.clone()).collect());
                        recovery_records.update(|records| records.extend(live_records));
                        recovery_inventory.set(RecoveryInventory::Ready);
                    }
                    Err(error) => {
                        recovery_inventory.set(RecoveryInventory::Failed(error.clone()));
                        warning.set(true);
                        message.set(error);
                    }
                }
            }
            read_busy.set(false);
            match view_outcome {
                Ok(BrewEvidenceViewOutcome::View(data)) => {
                    reconciliation_ready.set(recovery_blocked.get_untracked());
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
                    context_valid.set(false);
                    view.set(None);
                    readings.set(Vec::new());
                    warning.set(true);
                    message.set("You no longer have access to this brew.".into());
                }
                Ok(BrewEvidenceViewOutcome::ContextChanged) => {
                    context_valid.set(false);
                    view.set(None);
                    readings.set(Vec::new());
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
    Effect::new(move |_| load.run(()));
    let return_owner = StoredValue::new(None::<crate::components::layout::GuardTaskHandle>);
    let task = editor_guard.register_task(
        GuardTaskKind::Evidence,
        scope.get_value(),
        GuardTaskCallbacks {
            save_and_leave: None,
            return_to_actions: Some(Callback::new(move |()| {
                #[cfg(target_arch = "wasm32")]
                {
                    let Some(owner) = return_owner.get_value() else {
                        return;
                    };
                    let Some((original_scope, original_state)) = owner.current() else {
                        return;
                    };
                    let field_id = last_field.get_value();
                    let interaction = editor_guard.interaction_epoch();
                    let navigation = editor_guard.navigation_epoch();
                    let origin = send_wrapper::SendWrapper::new(editor_guard.focus_origin());
                    request_animation_frame(move || {
                        use wasm_bindgen::JsCast;
                        let Some((current_scope, current_state)) = owner.current() else {
                            return;
                        };
                        if current_scope != original_scope
                            || current_state.generation != original_state.generation
                            || editor_guard.interaction_epoch() != interaction
                            || editor_guard.navigation_epoch() != navigation
                        {
                            return;
                        }
                        let Some(window) = web_sys::window() else {
                            return;
                        };
                        let Some(document) = window.document() else {
                            return;
                        };
                        if !document.has_focus().unwrap_or(false)
                            || document.query_selector("dialog[open]").ok().flatten().is_some()
                            || document.query_selector("[popover]:popover-open").ok().flatten().is_some() { return; }
                        let usable = |element: &web_sys::Element| {
                            element
                                .closest("[hidden], [inert], :disabled")
                                .ok()
                                .flatten()
                                .is_none()
                                && element.get_client_rects().length() > 0
                                && window
                                    .get_computed_style(element)
                                    .ok()
                                    .flatten()
                                    .is_none_or(|style| {
                                        style.get_property_value("visibility").ok().as_deref()
                                            != Some("hidden")
                                    })
                        };
                        let field = current_state
                            .context_visible
                            .then(|| document.get_element_by_id(&field_id))
                            .flatten()
                            .filter(|field| usable(field));
                        let allowed_focus = || document.active_element().is_none_or(|active| {
                            matches!(active.tag_name().as_str(), "BODY" | "HTML")
                                || origin.as_ref().is_some_and(|original| active == *original)
                                || field.as_ref().is_some_and(|original| active == *original)
                                || active.closest("#workspace-draft-warning").ok().flatten().is_some()
                        });
                        if !allowed_focus() { return; }
                        let try_focus = |element: &web_sys::Element| element.dyn_ref::<web_sys::HtmlElement>().is_some_and(|target| target.focus().is_ok())
                            && document.active_element().is_some_and(|active| active == *element);
                        if field.as_ref().is_some_and(|field| try_focus(field)) { return; }
                        if !allowed_focus() { return; }
                        if !document.get_element_by_id("evidence-return-anchor").filter(|anchor| usable(anchor)).as_ref().is_some_and(|anchor| try_focus(anchor)) {
                            warning.set(true); message.set("Could not focus an available evidence action. Your inputs and original request remain retained.".into());
                        }
                    });
                }
            })),
            discard_unsent: Callback::new(move |()| {
                inputs.set(BTreeMap::new());
                note_text.set(String::new());
                reason.set(String::new());
                input_generation.update(|generation| *generation += 1);
                if let Some(owner) = return_owner.get_value() {
                    owner.update(GuardTaskState {
                        dirty: !cleanup_debt.get_untracked().is_empty(),
                        generation: input_generation.get_untracked(),
                        in_flight: write_busy.get_untracked(),
                        uncertain: pending.get_untracked().is_some()
                            || !recovery_records.get_untracked().is_empty()
                            || matches!(recovery_inventory.get_untracked(), RecoveryInventory::Failed(_)),
                        recovery_retained: recovery_key.get_untracked().is_some()
                            || !recovery_records.get_untracked().is_empty(),
                        context_visible: context_valid.get_untracked(),
                    });
                }
                true
            }),
        },
    );
    return_owner.set_value(Some(task));
    Effect::new(move |_| {
        task.update(GuardTaskState {
            dirty: evidence_dirty(&inputs.get(), &note_text.get(), &reason.get(), false)
                || !cleanup_debt.get().is_empty(),
            generation: input_generation.get(),
            in_flight: write_busy.get(),
            uncertain: pending.get().is_some()
                || !recovery_records.get().is_empty()
                || matches!(recovery_inventory.get(), RecoveryInventory::Failed(_)),
            recovery_retained: recovery_key.get().is_some() || !recovery_records.get().is_empty(),
            context_visible: context_valid.get(),
        })
    });
    on_cleanup(move || task.unregister());

    #[cfg(target_arch = "wasm32")]
    {
        use crate::components::layout::CONTEXT_CHANGED_KEY;
        use wasm_bindgen::{JsCast, closure::Closure};
        let location = leptos_router::hooks::use_location();
        let origin = web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_default();
        editor_guard.current_url.set(format!(
            "{}{}{}{}",
            origin,
            location.pathname.get_untracked(),
            location.search.get_untracked(),
            location.hash.get_untracked()
        ));
        let window = web_sys::window().expect("browser window");
        let unload = Closure::<dyn FnMut(web_sys::BeforeUnloadEvent)>::new(
            move |event: web_sys::BeforeUnloadEvent| {
                if editor_guard.should_warn_unload(&event) {
                    event.prevent_default();
                    event.set_return_value("");
                }
            },
        );
        if window
            .add_event_listener_with_callback("beforeunload", unload.as_ref().unchecked_ref())
            .is_err()
        {
            tracing::warn!("Unable to install the evidence unsaved-changes warning");
            editor_guard.navigation_error.set(
                "The browser's unsaved-changes warning is unavailable; keep this page open.".into(),
            );
        }
        let listener = send_wrapper::SendWrapper::new((window, unload));
        on_cleanup(move || {
            let (window, unload) = &*listener;
            if window
                .remove_event_listener_with_callback(
                    "beforeunload",
                    unload.as_ref().unchecked_ref(),
                )
                .is_err()
            {
                tracing::warn!("Unable to remove the evidence unsaved-changes warning");
            }
        });

        let recheck = Callback::new(move |_| {
            reconciliation_ready.set(false);
            recovery_inventory.set(RecoveryInventory::Loading);
            context_generation.update(|generation| *generation += 1);
            let sequence = context_generation.get_untracked();
            context_valid.set(false);
            let expected = intent.get_value();
            leptos::task::spawn_local(async move {
                let valid = current_context_matches(&expected).await;
                if context_generation.try_get_untracked() != Some(sequence) {
                    return;
                }
                context_valid.set(valid);
                if valid {
                    load.run(());
                } else {
                    view.set(None);
                    readings.set(Vec::new());
                }
            });
        });
        let focus = window_event_listener(leptos::ev::focus, move |_| recheck.run(()));
        let storage = window_event_listener(leptos::ev::storage, move |event| {
            if event.key().as_deref() == Some(CONTEXT_CHANGED_KEY) {
                recheck.run(());
            }
        });
        on_cleanup(move || {
            focus.remove();
            storage.remove();
        });
    }

    let load_lifecycle_more = Callback::new(move |_| {
        if continuation_busy.get_untracked() {
            return;
        }
        let Some(cursor) = lifecycle_cursor.get_untracked() else {
            return;
        };
        continuation_attempt.update(|attempt| *attempt += 1);
        let attempt_id = continuation_attempt.get_untracked();
        let requested_cursor = cursor.clone();
        continuation_busy.set(true);
        let sequence = generation.get_untracked();
        let expected = intent.get_value();
        let expected_context = expected.clone();
        let id = brew_id.get_value();
        leptos::task::spawn_local(async move {
            macro_rules! finish_continuation {
                () => {
                    if continuation_attempt.try_get_untracked() != Some(attempt_id) {
                        return;
                    }
                    continuation_busy.set(false);
                };
            }
            let outcome = brew_evidence_view_shared(BrewEvidenceViewRequest {
                intent: expected,
                brew_id: id,
                lifecycle_cursor: Some(cursor),
            })
            .await;
            if generation.try_get_untracked() != Some(sequence) {
                finish_continuation!();
                return;
            }
            let context_sequence = context_generation.get_untracked();
            if !current_context_matches(&expected_context).await {
                finish_continuation!();
                if generation.try_get_untracked() != Some(sequence)
                    || context_generation.try_get_untracked() != Some(context_sequence)
                {
                    return;
                }
                context_valid.set(false);
                view.set(None);
                readings.set(Vec::new());
                return;
            }
            if generation.try_get_untracked() != Some(sequence)
                || context_generation.try_get_untracked() != Some(context_sequence)
            {
                finish_continuation!();
                return;
            }
            if !continuation_owned(
                continuation_attempt.try_get_untracked(),
                attempt_id,
                lifecycle_cursor.get_untracked().as_deref(),
                &requested_cursor,
            ) {
                finish_continuation!();
                return;
            }
            finish_continuation!();
            match outcome {
                Ok(BrewEvidenceViewOutcome::View(data)) => {
                    lifecycle_extra.update(|events| events.extend(data.lifecycle));
                    lifecycle_cursor.set(data.lifecycle_next_cursor);
                }
                Ok(BrewEvidenceViewOutcome::Invalid) => {
                    warning.set(true);
                    message.set("Lifecycle continuation expired; reload the brew.".into());
                }
                Ok(BrewEvidenceViewOutcome::Forbidden)
                | Ok(BrewEvidenceViewOutcome::ContextChanged) => {
                    context_valid.set(false);
                    view.set(None);
                    readings.set(Vec::new());
                    warning.set(true);
                    message.set("Account or workspace changed. Reopen the brew.".into());
                }
                _ => {
                    warning.set(true);
                    message.set("Older lifecycle events could not be loaded.".into());
                }
            }
        });
    });

    let blocked = Signal::derive(move || {
        write_busy.get()
            || pending.get().is_some()
            || blocking_records.get()
            || !cleanup_debt.get().is_empty()
            || recovery_inventory.get() != RecoveryInventory::Ready
    });
    let continuation_blocked = Signal::derive(move || blocked.get() || continuation_busy.get());

    let dispatch = Callback::new(move |(action, retained): (PendingEvidence, bool)| {
        let original = pending.get_untracked();
        let pending_matches = original
            .as_ref()
            .is_some_and(|pending| same_original_evidence(pending, &action));
        if write_busy.get_untracked()
            || (retained && !pending_matches)
            || (!retained && (original.is_some() || blocking_records.get_untracked()))
            || !cleanup_debt.get_untracked().is_empty()
            || recovery_inventory.get_untracked() != RecoveryInventory::Ready
        {
            return;
        }
        let expected = intent.get_value();
        write_attempt.update(|attempt| *attempt += 1);
        let attempt_id = write_attempt.get_untracked();
        let preflight_epoch = context_generation.get_untracked();
        recovery_status.set(RecoveryStatus::Unknown);
        pending.set(Some(action.clone()));
        let preflight_key = recovery_key.get_untracked();
        write_busy.set(true);
        leptos::task::spawn_local(async move {
            macro_rules! finish_write {
                () => {
                    if write_attempt.try_get_untracked() != Some(attempt_id) {
                        return;
                    }
                    write_busy.set(false);
                };
            }
            let verified = current_context_matches(&expected).await;
            if write_attempt.try_get_untracked() != Some(attempt_id) {
                return;
            }
            // A keyless preflight is still an owned command. Never clear or
            // reclassify a successor selected while its authority read waited.
            if recovery_key.get_untracked() != preflight_key
                || !pending
                    .get_untracked()
                    .as_ref()
                    .is_some_and(|current| same_original_evidence(current, &action))
            {
                finish_write!();
                return;
            }
            if context_generation.try_get_untracked() != Some(preflight_epoch) {
                finish_write!();
                pending.set(pending_after_preflight_failure(action, retained));
                return;
            }
            if !verified {
                finish_write!();
                // A retained command may already have committed. Keep its
                // exact identity for recovery in the original context.
                pending.set(pending_after_preflight_failure(action, retained));
                context_valid.set(false);
                view.set(None);
                readings.set(Vec::new());
                warning.set(true);
                message.set(
                    "Account or workspace changed. Return to the original context before retrying."
                        .into(),
                );
                return;
            }
            let Some(session) = task_session.get_value() else {
                finish_write!();
                pending.set(pending_after_preflight_failure(action, retained));
                warning.set(true);
                message.set(
                    "Request not sent: a secure recovery session could not be created.".into(),
                );
                return;
            };
            match evidence_recovery::retain(
                &scope.get_value(),
                &action,
                &session,
                input_generation.get_untracked(),
            ) {
                Ok(key) => recovery_key.set(Some(key)),
                Err(error) => {
                    finish_write!();
                    if retained {
                        if let Some(key) = recovery_key.get_untracked() {
                            if let Ok(Some(record)) =
                                evidence_recovery::load_selected(&scope.get_value(), &key)
                            {
                                if adopt_selected_status.run((key, record)) {
                                    return;
                                }
                            }
                        }
                    }
                    pending.set(pending_after_preflight_failure(action, retained));
                    warning.set(true);
                    message.set(if retained { format!("Original request was not resent: {error}. Its identity remains retained; review the recovery status before leaving.") } else { format!("Request not sent: {error}. No new server operation was started.") });
                    return;
                }
            }
            let sent_epoch = context_generation.get_untracked();
            let original_recovery_key = recovery_key.get_untracked();
            macro_rules! confirm_response_scope {
                ($status:expr, $denial:expr, $receipt:expr) => {
                    let original_status = $status;
                    let original_receipt = $receipt;
                    // This receipt belongs to the captured original command, not
                    // to whichever panel is currently mounted. Preserve it even
                    // if UI publication is no longer owned by this continuation.
                    let receipt_error = if original_status != RecoveryStatus::Unknown {
                        original_recovery_key.as_ref().zip(original_receipt).and_then(|(key, receipt)| evidence_recovery::set_receipt(key, receipt, original_status.clone()).err())
                    } else { None };
                    if write_attempt.try_get_untracked() != Some(attempt_id) { return; }
                    if recovery_key.get_untracked().as_deref() != original_recovery_key.as_deref() { finish_write!(); return; }
                    let response_context_generation = context_generation.get_untracked();
                    if $denial && response_context_generation != sent_epoch { finish_write!(); return; }
                    let verified = current_context_matches(&expected).await;
                    if write_attempt.try_get_untracked() != Some(attempt_id) { return; }
                    if recovery_key.get_untracked().as_deref() != original_recovery_key.as_deref() { finish_write!(); return; }
                    if context_generation.try_get_untracked() != Some(response_context_generation) { finish_write!(); return; }
                    if !verified {
                        finish_write!();
                        context_valid.set(false); view.set(None); readings.set(Vec::new());
                        warning.set(true);
                        message.set("The account or workspace context changed while the original request was sent. Its identity remains retained; return to the original context to check the result.".into());
                        return;
                    }
                    if let Some(error) = receipt_error { recovery_cleanup_error.set(format!("The original result was received but its recovery status could not be retained: {error}. The original request identity remains pinned.")); }
                };
            }
            match action {
                PendingEvidence::Measurement(command) => {
                    let outcome = brew_measurement_shared(command).await;
                    confirm_response_scope!(
                        outcome
                            .as_ref()
                            .map(measurement_recovery_status)
                            .unwrap_or(RecoveryStatus::Unknown),
                        matches!(
                            &outcome,
                            Ok(MeasurementOutcome::Forbidden | MeasurementOutcome::ContextChanged)
                        ),
                        outcome
                            .as_ref()
                            .ok()
                            .cloned()
                            .map(EvidenceReceipt::Measurement)
                    );
                    finish_write!();
                    match outcome {
                        Ok(outcome) => {
                            if matches!(
                                &outcome,
                                MeasurementOutcome::RequestIdentityConflict
                                    | MeasurementOutcome::ResultExpired { .. }
                            ) {
                                lock_recovery.run(match &outcome {
                                    MeasurementOutcome::ResultExpired { resource_id } => {
                                        RecoveryStatus::Expired {
                                            resource_id: resource_id.clone(),
                                        }
                                    }
                                    _ => RecoveryStatus::IdentityConflict,
                                });
                            }
                            let definitive = !measurement_is_uncertain(&outcome)
                                && !matches!(
                                    &outcome,
                                    MeasurementOutcome::Forbidden
                                        | MeasurementOutcome::ContextChanged
                                        | MeasurementOutcome::RequestIdentityConflict
                                        | MeasurementOutcome::ResultExpired { .. }
                                );
                            if definitive {
                                if let Some(key) = original_recovery_key.clone() {
                                    settled.run((key, measurement_message(&outcome)));
                                }
                            }
                            match outcome {
                                MeasurementOutcome::Conflict { .. }
                                | MeasurementOutcome::EntryConflict { .. } => {
                                    warning.set(true);
                                    message.set(
                                        "Changed elsewhere; current evidence refreshed. Review and reapply."
                                            .into(),
                                    );
                                    load.run(());
                                }
                                MeasurementOutcome::Forbidden
                                | MeasurementOutcome::ContextChanged => {
                                    context_valid.set(false);
                                    view.set(None);
                                    readings.set(Vec::new());
                                    warning.set(true);
                                    message.set(
                                        "Account or workspace changed. Reopen the brew.".into(),
                                    );
                                }
                                MeasurementOutcome::Retryable | MeasurementOutcome::Unavailable => {
                                    warning.set(true);
                                    message.set(
                                        "Outcome uncertain; retry the same request or abandon it."
                                            .into(),
                                    );
                                }
                                other => {
                                    let (ok, text) = measurement_message(&other);
                                    warning.set(!ok);
                                    if !text.is_empty() {
                                        message.set(text);
                                    }
                                    if ok {
                                        load.run(());
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            warning.set(true);
                            message.set(format!(
                                "Reading request uncertain: {error}. Retry the same request or abandon it."
                            ));
                        }
                    }
                }
                PendingEvidence::Note(command) => {
                    let outcome = brew_note_shared(command).await;
                    confirm_response_scope!(
                        outcome
                            .as_ref()
                            .map(note_recovery_status)
                            .unwrap_or(RecoveryStatus::Unknown),
                        matches!(
                            &outcome,
                            Ok(NoteOutcome::Forbidden | NoteOutcome::ContextChanged)
                        ),
                        outcome.as_ref().ok().cloned().map(EvidenceReceipt::Note)
                    );
                    finish_write!();
                    match outcome {
                        Ok(outcome) => {
                            if matches!(
                                &outcome,
                                NoteOutcome::RequestIdentityConflict
                                    | NoteOutcome::ResultExpired { .. }
                            ) {
                                lock_recovery.run(match &outcome {
                                    NoteOutcome::ResultExpired { resource_id } => {
                                        RecoveryStatus::Expired {
                                            resource_id: resource_id.clone(),
                                        }
                                    }
                                    _ => RecoveryStatus::IdentityConflict,
                                });
                            }
                            let definitive = !note_is_uncertain(&outcome)
                                && !matches!(
                                    &outcome,
                                    NoteOutcome::Forbidden
                                        | NoteOutcome::ContextChanged
                                        | NoteOutcome::RequestIdentityConflict
                                        | NoteOutcome::ResultExpired { .. }
                                );
                            if definitive {
                                if let Some(key) = original_recovery_key.clone() {
                                    settled.run((key, note_message(&outcome)));
                                }
                            }
                            match outcome {
                                NoteOutcome::Conflict { .. } => {
                                    warning.set(true);
                                    message.set(
                                        "Note changed elsewhere; current evidence refreshed. Review and reapply."
                                            .into(),
                                    );
                                    load.run(());
                                }
                                NoteOutcome::Forbidden | NoteOutcome::ContextChanged => {
                                    context_valid.set(false);
                                    view.set(None);
                                    readings.set(Vec::new());
                                    warning.set(true);
                                    message.set(
                                        "Account or workspace changed. Reopen the brew.".into(),
                                    );
                                }
                                NoteOutcome::Retryable | NoteOutcome::Unavailable => {
                                    warning.set(true);
                                    message.set(
                                        "Outcome uncertain; retry the same request or abandon it."
                                            .into(),
                                    );
                                }
                                other => {
                                    let (ok, text) = note_message(&other);
                                    warning.set(!ok);
                                    if !text.is_empty() {
                                        message.set(text);
                                    }
                                    if ok {
                                        load.run(());
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            warning.set(true);
                            message.set(format!(
                                "Note request uncertain: {error}. Retry the same request or abandon it."
                            ));
                        }
                    }
                }
                PendingEvidence::Lifecycle(request) => {
                    let outcome = crate::server_fns::brew::brew_lifecycle_shared(request).await;
                    confirm_response_scope!(
                        outcome
                            .as_ref()
                            .map(lifecycle_recovery_status)
                            .unwrap_or(RecoveryStatus::Unknown),
                        matches!(
                            &outcome,
                            Ok(BrewLifecycleOutcome::Forbidden
                                | BrewLifecycleOutcome::ContextChanged)
                        ),
                        outcome
                            .as_ref()
                            .ok()
                            .cloned()
                            .map(EvidenceReceipt::Lifecycle)
                    );
                    finish_write!();
                    match outcome {
                        Ok(outcome) => {
                            if matches!(
                                &outcome,
                                BrewLifecycleOutcome::RequestIdentityConflict
                                    | BrewLifecycleOutcome::ResultExpired { .. }
                            ) {
                                lock_recovery.run(match &outcome {
                                    BrewLifecycleOutcome::ResultExpired { resource_id } => {
                                        RecoveryStatus::Expired {
                                            resource_id: resource_id.clone(),
                                        }
                                    }
                                    _ => RecoveryStatus::IdentityConflict,
                                });
                            }
                            let definitive = !matches!(
                                &outcome,
                                BrewLifecycleOutcome::Retryable
                                    | BrewLifecycleOutcome::Unavailable
                                    | BrewLifecycleOutcome::Forbidden
                                    | BrewLifecycleOutcome::ContextChanged
                                    | BrewLifecycleOutcome::RequestIdentityConflict
                                    | BrewLifecycleOutcome::ResultExpired { .. }
                            );
                            if definitive {
                                if let Some(key) = original_recovery_key.clone() {
                                    settled.run((key, lifecycle_message(&outcome)));
                                }
                            }
                            match outcome {
                                BrewLifecycleOutcome::Conflict { .. } => {
                                    warning.set(true);
                                    message.set(
                                        "Brew changed elsewhere; current lifecycle refreshed. Review and reapply."
                                            .into(),
                                    );
                                    load.run(());
                                }
                                BrewLifecycleOutcome::Forbidden
                                | BrewLifecycleOutcome::ContextChanged => {
                                    context_valid.set(false);
                                    view.set(None);
                                    readings.set(Vec::new());
                                    warning.set(true);
                                    message.set(
                                        "Account or workspace changed. Reopen the brew.".into(),
                                    );
                                }
                                BrewLifecycleOutcome::Retryable
                                | BrewLifecycleOutcome::Unavailable => {
                                    warning.set(true);
                                    message.set(
                                        "Outcome uncertain; retry the same request or abandon it."
                                            .into(),
                                    );
                                }
                                other => {
                                    let (ok, text) = lifecycle_message(&other);
                                    warning.set(!ok);
                                    message.set(text);
                                    if ok {
                                        load.run(());
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            warning.set(true);
                            message.set(format!(
                                "Lifecycle request uncertain: {error}. Retry the same request or abandon it."
                            ));
                        }
                    }
                }
                PendingEvidence::Archive(request) => {
                    let outcome = brew_archive_shared(request).await;
                    confirm_response_scope!(
                        outcome
                            .as_ref()
                            .map(archive_recovery_status)
                            .unwrap_or(RecoveryStatus::Unknown),
                        matches!(
                            &outcome,
                            Ok(BrewArchiveOutcome::Forbidden | BrewArchiveOutcome::ContextChanged)
                        ),
                        outcome.as_ref().ok().cloned().map(EvidenceReceipt::Archive)
                    );
                    finish_write!();
                    match outcome {
                        Ok(outcome) => {
                            if matches!(
                                &outcome,
                                BrewArchiveOutcome::RequestIdentityConflict
                                    | BrewArchiveOutcome::ResultExpired { .. }
                            ) {
                                lock_recovery.run(match &outcome {
                                    BrewArchiveOutcome::ResultExpired { resource_id } => {
                                        RecoveryStatus::Expired {
                                            resource_id: resource_id.clone(),
                                        }
                                    }
                                    _ => RecoveryStatus::IdentityConflict,
                                });
                            }
                            let definitive = !archive_is_uncertain(&outcome)
                                && !matches!(
                                    &outcome,
                                    BrewArchiveOutcome::Forbidden
                                        | BrewArchiveOutcome::ContextChanged
                                        | BrewArchiveOutcome::RequestIdentityConflict
                                        | BrewArchiveOutcome::ResultExpired { .. }
                                );
                            if definitive {
                                if let Some(key) = original_recovery_key.clone() {
                                    settled.run((key, archive_message(&outcome)));
                                }
                            }
                            match outcome {
                                BrewArchiveOutcome::Conflict { .. } => {
                                    warning.set(true);
                                    message.set(
                                        "Archive state changed elsewhere; current evidence refreshed. Review and reapply."
                                            .into(),
                                    );
                                    load.run(());
                                }
                                BrewArchiveOutcome::Forbidden
                                | BrewArchiveOutcome::ContextChanged => {
                                    context_valid.set(false);
                                    view.set(None);
                                    readings.set(Vec::new());
                                    warning.set(true);
                                    message.set(
                                        "Account or workspace changed. Reopen the brew.".into(),
                                    );
                                }
                                BrewArchiveOutcome::Retryable | BrewArchiveOutcome::Unavailable => {
                                    warning.set(true);
                                    message.set(
                                        "Outcome uncertain; retry the same request or abandon it."
                                            .into(),
                                    );
                                }
                                other => {
                                    let (ok, text) = archive_message(&other);
                                    warning.set(!ok);
                                    message.set(text);
                                    if ok {
                                        load.run(());
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            warning.set(true);
                            message.set(format!(
                                "Archive request uncertain: {error}. Retry the same request or abandon it."
                            ));
                        }
                    }
                }
            }
        });
    });

    let run_measurement = Callback::new(move |command: MeasurementCommand| {
        dispatch.run((PendingEvidence::Measurement(command), false));
    });
    let run_note = Callback::new(move |command: NoteCommand| {
        dispatch.run((PendingEvidence::Note(command), false));
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
        dispatch.run((
            PendingEvidence::Lifecycle(TransitionBrewRequest {
                intent: intent.get_value(),
                brew_id: brew_id.get_value(),
                expected_revision: data.revision,
                transition,
                reason: (!reason_text.is_empty()).then_some(reason_text),
                request_id,
            }),
            false,
        ));
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
        dispatch.run((
            PendingEvidence::Archive(ArchiveBrewRequest {
                intent: intent.get_value(),
                brew_id: brew_id.get_value(),
                archived,
                expected_metadata_revision: data.metadata_revision,
                request_id,
            }),
            false,
        ));
    });
    let retry_pending = Callback::new(move |_| {
        if write_busy.get_untracked()
            || !cleanup_debt.get_untracked().is_empty()
            || !context_valid.get_untracked()
            || recovery_blocked.get_untracked()
            || recovery_inventory.get_untracked() != RecoveryInventory::Ready
        {
            return;
        }
        if let Some(action) = retry_action(pending.get_untracked().as_ref()) {
            if let Some(key) = recovery_key.get_untracked() {
                match evidence_recovery::load_selected(&scope.get_value(), &key) {
                    Ok(Some(record)) => {
                        if adopt_selected_status.run((key, record)) {
                            return;
                        }
                    }
                    Ok(None) => {} // retain the SAME command before any resend
                    Err(error) => {
                        recovery_inventory.set(RecoveryInventory::Failed(error));
                        return;
                    }
                }
            }
            dispatch.run((action, true));
        }
    });
    let abandon_pending = Callback::new(move |_| {
        if write_busy.get_untracked() {
            return;
        }
        warning.set(true);
        message.set(
            "The previous request may already have committed. Its original identity remains retained; retry it to check the result, or leave with recovery preserved."
                .into(),
        );
        load.run(());
    });

    let finish_recovery_review = Callback::new(move |()| {
        if !reconciliation_ready.get_untracked()
            || !context_valid.get_untracked()
            || write_busy.get_untracked()
        {
            return;
        }
        let Some(key) = recovery_key.get_untracked() else {
            return;
        };
        let reviewed = match recovery_status.get_untracked() {
            RecoveryStatus::Expired { resource_id } => {
                RecoveryStatus::ReviewedExpired { resource_id }
            }
            RecoveryStatus::IdentityConflict => RecoveryStatus::ReviewedIdentityConflict,
            _ => return,
        };
        let epoch = context_generation.get_untracked();
        let attempt = write_attempt.get_untracked();
        let expected = intent.get_value();
        leptos::task::spawn_local(async move {
            let verified = current_context_matches(&expected).await;
            if !verified
                || context_generation.try_get_untracked() != Some(epoch)
                || write_attempt.try_get_untracked() != Some(attempt)
            {
                return;
            }
            if !reconciliation_ready.get_untracked()
                || !context_valid.get_untracked()
                || recovery_key.get_untracked().as_deref() != Some(&key)
            {
                return;
            }
            match evidence_recovery::load_selected(&scope.get_value(), &key) {
                Ok(Some(current)) if current.status != recovery_status.get_untracked() => {
                    adopt_selected_status.run((key, current));
                    return;
                }
                Err(error) => {
                    warning.set(true);
                    message.set(error);
                    return;
                }
                _ => {}
            }
            match evidence_recovery::set_status(&key, reviewed.clone()) {
                Ok(()) => {
                    reviewed_records.update(|records| records.push((key, reviewed.clone())));
                    pending.set(None);
                    recovery_key.set(None);
                    recovery_blocked.set(false);
                    reconciliation_ready.set(false);
                    message.set(reviewed.message());
                    warning.set(true);
                }
                Err(error) => {
                    warning.set(true);
                    message.set(error);
                }
            }
        });
    });

    view! {
        <div class="recipe-editor" id="evidence-return-anchor" tabindex="-1" aria-label="Brew evidence task">
            <Show
                when=move || context_valid.get()
                fallback=|| view! {
                    <BreweryNotice
                        title="Account or workspace changed"
                        body="Reopen this brew in the original workspace. Evidence and drafts stay hidden until the original context is verified."
                        warning=true
                    />
                }
            >
            <BreweryPanel
                title="Brew evidence"
                description="Readings, notes, lifecycle and archive all use the same shared operations as MCP."
            >
                {move || pending.get().map(|_| view! {
                    <BreweryNotice
                        title=if recovery_status.get() == RecoveryStatus::Unknown { "Outcome uncertain" } else { "Original request retained" }
                        body="The last write may have committed. Retry the same request or abandon it before changing content."
                        warning=true
                        announcement=crate::components::NoticeAnnouncement::Settled
                    />
                    <div class="recipe-editor__actions">
                        <Button disabled=Signal::derive(move || write_busy.get() || recovery_blocked.get() || !cleanup_debt.get().is_empty() || recovery_inventory.get() != RecoveryInventory::Ready) on:click=move |_| retry_pending.run(())>"Retry same request"</Button>
                        <Button variant=ButtonVariant::Secondary disabled=write_busy on:click=move |_| abandon_pending.run(())>"Keep original request"</Button>
                    </div>
                })}
                {move || match recovery_inventory.get() {
                    RecoveryInventory::Failed(error) => view! {
                        <BreweryNotice title="Recovery discovery unavailable" body=format!("{error}. New evidence writes are blocked until original requests can be discovered.") warning=true announcement=crate::components::NoticeAnnouncement::Settled/>
                        <Button disabled=read_busy on:click=move |_| load.run(())>"Retry recovery discovery"</Button>
                    }.into_any(),
                    _ => ().into_any(),
                }}
                <For each=move || recovery_records.get() key=|(key, _)| key.clone() children=move |(key, record)| {
                    let label = format!("Check retained evidence request {}", record.command.identity().map(|(_, _, id)| id).unwrap_or("unknown"));
                    view! { <Button disabled=Signal::derive(move || pending.get().is_some() || write_busy.get()) on:click=move |_| {
                        if pending.get_untracked().is_none() && !write_busy.get_untracked() {
                            recovery_blocked.set(record.status != RecoveryStatus::Unknown);
                            recovery_status.set(record.status.clone());
                            if record.status != RecoveryStatus::Unknown { warning.set(true); message.set(record.status.message()); }
                            recovery_key.set(Some(key.clone())); pending.set(Some(record.command.clone()));
                            recovery_records.update(|records| records.retain(|(candidate, _)| candidate != &key));
                        }
                    }>{label}</Button> }
                }/>
                <Show when=move || recovery_blocked.get()>
                    {move || view! { <BreweryNotice title="Original request requires review" body=recovery_status.get().message() warning=true/> }}
                    <Button disabled=read_busy on:click=move |_| { reconciliation_ready.set(false); load.run(()); }>"Verify current evidence"</Button>
                    <Button disabled=Signal::derive(move || !reconciliation_ready.get() || !context_valid.get() || write_busy.get()) on:click=move |_| finish_recovery_review.run(())>"Keep original identity and finish review"</Button>
                    <p>"A missing item in the loaded evidence is not proof that the earlier write was absent. Finishing review does not send or replace that command."</p>
                </Show>
                <For each=move || reviewed_records.get() key=|(key, _)| key.clone() children=move |(_, status)| view! { <BreweryNotice title="Original identity retained after review" body=status.message()/> }/>
                <For each=move || cleanup_debt.get() key=|(key, _)| key.clone() children=move |(key, status)| {
                    view! { <BreweryNotice title="Confirmed result — local cleanup pending" body=status.message() warning=true announcement=crate::components::NoticeAnnouncement::Settled/>
                        <Button disabled=write_busy on:click=move |_| {
                            match evidence_recovery::cleanup_confirmed(&scope.get_value(), &key, status.clone()) {
                                Ok(()) => { cleanup_debt.update(|debts| debts.retain(|(candidate, _)| candidate != &key)); recovery_records.update(|records| records.retain(|(candidate, _)| candidate != &key)); recovery_cleanup_error.set(String::new()); }
                                Err(error) => recovery_cleanup_error.set(error),
                            }
                        }>"Retry local cleanup only"</Button>
                    }
                }/>
                {move || (!recovery_cleanup_error.get().is_empty()).then(|| view! {
                    <BreweryNotice title="Local recovery cleanup" body=recovery_cleanup_error.get() warning=true announcement=crate::components::NoticeAnnouncement::Settled/>
                })}
                {move || (!message.get().is_empty()).then(|| view! {
                    <BreweryNotice title="Brew evidence" body=message.get() warning=warning.get() announcement=crate::components::NoticeAnnouncement::Settled/>
                })}
                {move || view.get().map(|data| {
                    let status = status_label(data.status).to_owned();
                    let actual_source = if data.actual.is_some() { "Recorded actual formulation" } else { "Planned formulation" };
                    let actual_revision = data.actual.as_ref()
                        .map(|actual| format!("Revision {}", actual.revision))
                        .unwrap_or_else(|| "Planned formulation only".to_owned());
                    let boil_minutes = data.actual.as_ref()
                        .map(|actual| actual.document.boil_minutes)
                        .unwrap_or(data.plan.recipe.boil_minutes);
                    let archive_label = if data.archived { "Unarchive brew" } else { "Archive brew" };
                    let archived = data.archived;
                    let current_actor = intent.get_value().account_id;
                    let started_by = actor_text(&data.started_by, Some(current_actor.as_str()));
                    let started = format_timestamp(data.started_at);
                    view! {
                        <div class="recipe-editor__metrics">
                            <BreweryMetric label="Status" value=status unit="">{""}</BreweryMetric>
                            <BreweryMetric label="Started" value=started.clone() unit="">{""}</BreweryMetric>
                            <BreweryMetric label="Recipe revision" value=data.selected_recipe_revision.to_string() unit="">{""}</BreweryMetric>
                            <BreweryMetric label="Recorded actual" value=actual_revision unit="">{""}</BreweryMetric>
                            <BreweryMetric label="Actual source" value=actual_source.to_string() unit="">{""}</BreweryMetric>
                            <BreweryMetric
                                label="Boil duration"
                                value=boil_minutes.map(|value| format!("{value:.0}")).unwrap_or_else(|| "—".into())
                                unit="min"
                            >{""}</BreweryMetric>
                        </div>
                        <p class="note-caption">
                            {format!(
                                "Started {} by {}. The recipe revision {} is frozen for this brew.{}",
                                started,
                                started_by,
                                data.selected_recipe_revision,
                                if archived { " This brew is archived." } else { " This brew is active." }
                            )}
                        </p>
                        <BreweryDisclosure title="Record provenance">
                            <dl class="print-sheet__facts">
                                <BrewProvenanceFact label="Brew reference".to_owned() value=data.brew_id.clone()/>
                                <BrewProvenanceFact label="Brew revision".to_owned() value=data.revision.to_string()/>
                                <BrewProvenanceFact label="Record revision".to_owned() value=data.metadata_revision.to_string()/>
                            </dl>
                        </BreweryDisclosure>
                        <div class="recipe-editor__actions">
                            <Button
                                disabled=blocked
                                on:click=move |_| run_transition.run(BrewTransitionV1::Complete)
                            >"Complete"</Button>
                            <Button
                                variant=ButtonVariant::Secondary
                                disabled=blocked
                                on:click=move |_| run_transition.run(BrewTransitionV1::Cancel)
                            >"Cancel brew"</Button>
                            <Button
                                variant=ButtonVariant::Secondary
                                disabled=blocked
                                on:click=move |_| run_transition.run(BrewTransitionV1::Reopen)
                            >"Reopen"</Button>
                            <Button
                                variant=ButtonVariant::Secondary
                                disabled=blocked
                                on:click=move |_| run_archive.run(!archived)
                            >{archive_label.to_string()}</Button>
                        </div>
                        <BreweryDraftField
                            id="brew-transition-reason"
                            label="Reason for reopen or terminal correction"
                            value=Signal::from(reason)
                             on_change=Callback::new(move |text: String| { last_field.set_value("brew-transition-reason".into()); input_generation.update(|generation| *generation += 1); reason.set(text); })
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
                                 let field_id = field_id.clone();
                                 move |text: String| { last_field.set_value(field_id.clone()); input_generation.update(|generation| *generation += 1); inputs.update(|values| {
                                     values.insert(quantity_id.clone(), text);
                                 }); }
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
                            let unit = slot.unit.clone();
                            let target = slot.target
                                .map(|value| format_measurement_value(value, &unit))
                                .unwrap_or_else(|| "No planned target".into());
                            let current_value = slot.current.as_ref()
                                .and_then(|entry| entry.value)
                                .map(|value| format_measurement_value(value, &unit))
                                .unwrap_or_else(|| "No reading recorded".into());
                            view! {
                                <div class="recipe-editor__summary">
                                    <h4>{slot.label.clone()}</h4>
                                    <p class="note-caption">
                                        {format!("Planned target {} · selected actual {}", target, current_value)}
                                    </p>
                                    <BreweryDraftField
                                        id=field_id
                                        label=slot.label
                                        value=input_value
                                        on_change=input_change
                                        unit=unit.clone()
                                        numeric=true
                                    />
                                    <div class="recipe-editor__actions">
                                        <Button disabled=blocked on:click=record>"Record reading"</Button>
                                        {current.clone().map(|_| view! {
                                            <Button variant=ButtonVariant::Secondary disabled=blocked on:click=correct>"Correct selected"</Button>
                                            <Button variant=ButtonVariant::Secondary disabled=blocked on:click=retract>"Retract selected"</Button>
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
                                                        <span>
                                                            {format!(
                                                                "{} · {}",
                                                                entry.value
                                                                    .map(|value| format_measurement_value(value, &unit))
                                                                    .unwrap_or_else(|| "Retracted".into()),
                                                                format_timestamp(entry.observed_at)
                                                            )}
                                                        </span>
                                                        {selected.then(|| view! { <span class="note-caption">"Selected"</span> })}
                                                        {(!selected).then(|| view! {
                                                            <Button variant=ButtonVariant::Secondary disabled=blocked on:click=select>"Select"</Button>
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
                             on_change=Callback::new(move |text: String| { last_field.set_value("brew-note".into()); input_generation.update(|generation| *generation += 1); note_text.set(text); })
                        />
                        <div class="recipe-editor__actions">
                            <Button
                                disabled=blocked
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
                                        <p class="note-caption">{format!("Updated {} · revision {revision}", format_timestamp(note_view.current.recorded_at))}</p>
                                        <div class="recipe-editor__actions">
                                            <Button variant=ButtonVariant::Secondary disabled=blocked on:click=correct_note>"Correct with draft"</Button>
                                            <Button variant=ButtonVariant::Secondary disabled=blocked on:click=retract_note>"Retract"</Button>
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
                                        "{}: {} → {}{} · revision {}",
                                        format_timestamp(event.recorded_at),
                                        status_label(event.from_status),
                                        status_label(event.to_status),
                                        event.reason.as_ref().map(|reason| format!(" ({reason})")).unwrap_or_default(),
                                        event.brew_revision
                                    )}
                                </li>
                            }).collect_view()}
                        </ul>
                        {lifecycle_cursor.get().map(|_| view! {
                            <Button
                                variant=ButtonVariant::Secondary
                                disabled=continuation_blocked
                                on:click=move |_| load_lifecycle_more.run(())
                            >"Load older lifecycle"</Button>
                        })}
                    }
                })}
            </BreweryPanel>
            </Show>
        </div>
    }
}

#[cfg(test)]
mod evidence_ui_state_tests {
    use super::*;

    #[test]
    fn every_real_write_family_distinguishes_terminal_blockers_known_rejection_and_transport_unknown()
     {
        let id = Some("original-resource".to_owned());
        for status in [
            measurement_recovery_status(&MeasurementOutcome::ResultExpired {
                resource_id: id.clone(),
            }),
            note_recovery_status(&NoteOutcome::ResultExpired {
                resource_id: id.clone(),
            }),
            lifecycle_recovery_status(&BrewLifecycleOutcome::ResultExpired {
                resource_id: id.clone(),
            }),
            archive_recovery_status(&BrewArchiveOutcome::ResultExpired {
                resource_id: id.clone(),
            }),
        ] {
            assert_eq!(
                status,
                RecoveryStatus::Expired {
                    resource_id: id.clone()
                }
            );
        }
        for status in [
            measurement_recovery_status(&MeasurementOutcome::RequestIdentityConflict),
            note_recovery_status(&NoteOutcome::RequestIdentityConflict),
            lifecycle_recovery_status(&BrewLifecycleOutcome::RequestIdentityConflict),
            archive_recovery_status(&BrewArchiveOutcome::RequestIdentityConflict),
        ] {
            assert_eq!(status, RecoveryStatus::IdentityConflict);
        }
        for status in [
            measurement_recovery_status(&MeasurementOutcome::Retryable),
            note_recovery_status(&NoteOutcome::Unavailable),
            lifecycle_recovery_status(&BrewLifecycleOutcome::Forbidden),
            archive_recovery_status(&BrewArchiveOutcome::ContextChanged),
        ] {
            assert_eq!(status, RecoveryStatus::Unknown);
        }
        for status in [
            measurement_recovery_status(&MeasurementOutcome::Invalid),
            note_recovery_status(&NoteOutcome::Invalid),
            lifecycle_recovery_status(&BrewLifecycleOutcome::Invalid),
            archive_recovery_status(&BrewArchiveOutcome::Invalid),
        ] {
            assert!(matches!(
                status,
                RecoveryStatus::Confirmed {
                    accepted: false,
                    ..
                }
            ));
        }
    }

    #[test]
    fn expired_results_name_the_original_resource_without_claiming_absence_or_minting_a_retry() {
        let resource_id = Some("original-resource".into());
        let messages = [
            measurement_message(&MeasurementOutcome::ResultExpired {
                resource_id: resource_id.clone(),
            }),
            note_message(&NoteOutcome::ResultExpired {
                resource_id: resource_id.clone(),
            }),
            lifecycle_message(&BrewLifecycleOutcome::ResultExpired {
                resource_id: resource_id.clone(),
            }),
            archive_message(&BrewArchiveOutcome::ResultExpired { resource_id }),
        ];
        for (success, message) in messages {
            assert!(!success);
            assert!(message.contains("original-resource"));
            assert!(message.contains("does not prove the write was absent"));
            assert!(message.contains("must not be re-executed"));
            assert!(!message.contains("Start a new request"));
        }
    }

    #[test]
    fn uncertain_outcomes_keep_pending_and_definitive_outcomes_clear_it() {
        assert!(measurement_is_uncertain(&MeasurementOutcome::Retryable));
        assert!(measurement_is_uncertain(&MeasurementOutcome::Unavailable));
        assert!(!measurement_is_uncertain(&MeasurementOutcome::NotFound));
        assert!(note_is_uncertain(&NoteOutcome::Retryable));
        assert!(!note_is_uncertain(&NoteOutcome::Invalid));
        assert!(lifecycle_is_uncertain(&BrewLifecycleOutcome::Unavailable));
        assert!(!lifecycle_is_uncertain(&BrewLifecycleOutcome::Invalid));
        assert!(archive_is_uncertain(&BrewArchiveOutcome::Retryable));
        assert!(!archive_is_uncertain(&BrewArchiveOutcome::NotFound));
    }

    #[test]
    fn conflicts_request_current_evidence_refresh() {
        assert!(measurement_needs_refresh(&MeasurementOutcome::Conflict {
            expected: 1,
            actual: 2
        }));
        assert!(measurement_needs_refresh(
            &MeasurementOutcome::EntryConflict {
                expected: "old".into(),
                actual: "current".into()
            }
        ));
        assert!(!measurement_needs_refresh(&MeasurementOutcome::NotFound));
        assert!(note_needs_refresh(&NoteOutcome::Conflict {
            expected: 1,
            actual: 2
        }));
        assert!(!note_needs_refresh(&NoteOutcome::Invalid));
        assert!(lifecycle_needs_refresh(&BrewLifecycleOutcome::Conflict {
            expected: 1,
            actual: 2
        }));
        assert!(!lifecycle_needs_refresh(
            &BrewLifecycleOutcome::InvalidTransition {
                from: krausen_types::brewery::BrewStatusV1::Started,
                to: krausen_types::brewery::BrewStatusV1::Started
            }
        ));
        assert!(archive_needs_refresh(&BrewArchiveOutcome::Conflict {
            expected: 1,
            actual: 2
        }));
        assert!(!archive_needs_refresh(&BrewArchiveOutcome::NotFound));
    }

    #[test]
    fn retry_preserves_exact_request_identity() {
        let request_id = "00000000-0000-4000-8000-000000000001".to_owned();
        let pending =
            PendingEvidence::Measurement(MeasurementCommand::Record(RecordMeasurementRequest {
                intent: OperationIntent {
                    account_id: "account".into(),
                    workspace_id: "workspace".into(),
                },
                brew_id: "brew".into(),
                quantity_id: "gravity.original.v1".into(),
                value: 1.050,
                observed_at: None,
                source: "manual".into(),
                metadata: Default::default(),
                request_id: request_id.clone(),
            }));
        let retry = retry_action(Some(&pending)).expect("same request retry");
        match retry {
            PendingEvidence::Measurement(MeasurementCommand::Record(request)) => {
                assert_eq!(request.request_id, request_id);
                assert_eq!(request.value, 1.050);
            }
            _ => panic!("retry changed the command family"),
        }
        assert!(retry_action(None).is_none());
    }

    #[test]
    fn retained_preflight_failure_keeps_exact_command_identity() {
        let request_id = "00000000-0000-4000-8000-000000000002".to_owned();
        let pending =
            PendingEvidence::Measurement(MeasurementCommand::Record(RecordMeasurementRequest {
                intent: OperationIntent {
                    account_id: "account".into(),
                    workspace_id: "workspace".into(),
                },
                brew_id: "brew".into(),
                quantity_id: "gravity.final.v1".into(),
                value: 1.010,
                observed_at: None,
                source: "manual".into(),
                metadata: Default::default(),
                request_id: request_id.clone(),
            }));
        let retained =
            pending_after_preflight_failure(pending.clone(), true).expect("retained command");
        match retained {
            PendingEvidence::Measurement(MeasurementCommand::Record(request)) => {
                assert_eq!(request.request_id, request_id);
                assert_eq!(request.value, 1.010);
            }
            _ => panic!("retained command changed family"),
        }
        assert!(pending_after_preflight_failure(pending, false).is_none());
    }

    #[test]
    fn continuation_ownership_rejects_stale_attempt_or_cursor() {
        assert!(continuation_owned(Some(2), 2, Some("cursor-a"), "cursor-a"));
        assert!(!continuation_owned(
            Some(1),
            2,
            Some("cursor-a"),
            "cursor-a"
        ));
        assert!(!continuation_owned(
            Some(2),
            2,
            Some("cursor-b"),
            "cursor-a"
        ));
        assert!(!continuation_owned(None, 2, Some("cursor-a"), "cursor-a"));
    }

    #[test]
    fn dirty_tracks_drafts_and_uncertain_work() {
        let mut inputs = BTreeMap::new();
        assert!(!evidence_dirty(&inputs, "", "", false));
        inputs.insert("gravity.original.v1".into(), "  ".into());
        assert!(!evidence_dirty(&inputs, "", "", false));
        inputs.insert("gravity.original.v1".into(), "1.050".into());
        assert!(evidence_dirty(&inputs, "", "", false));
        assert!(evidence_dirty(&BTreeMap::new(), "note", "", false));
        assert!(evidence_dirty(&BTreeMap::new(), "", "reason", false));
        assert!(evidence_dirty(&BTreeMap::new(), "", "", true));
    }
}
