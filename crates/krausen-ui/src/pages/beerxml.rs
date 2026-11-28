// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit BeerXML import preview and commit.
use super::beerxml_recovery::{self, ImportRecovery, RecoveryStatus};
use crate::{
    components::{
        BreweryActionGroup, BreweryDraftArea, BreweryDraftField, BreweryDraftSelect, BreweryNotice,
        BreweryPanel, BrewerySection, Button, ButtonLink, ButtonVariant,
        layout::{EditorGuard, GuardScope, GuardTaskCallbacks, GuardTaskKind, GuardTaskState},
    },
    pages::recipe_editor::current_context_matches,
    server_fns::interchange::{commit_beerxml, preview_beerxml},
};
use krausen_types::{
    brewery::EquipmentSettingsV1,
    interchange::{
        ColourUnit, EfficiencyChoice, ImportChoices, ImportCommitOutcome, ImportPreview,
        ImportPreviewOutcome, ImportRequest,
    },
    request_context::OperationIntent,
};
use leptos::prelude::*;

fn calculation_summary(outcome: krausen_types::formulation::PreviewOutcome) -> String {
    let krausen_types::formulation::PreviewOutcome::Preview(value) = outcome else {
        return "Targets are unavailable for this equipment or style.".into();
    };
    let number = |value: Option<f64>, places: usize| {
        value
            .map(|n| format!("{n:.places$}"))
            .unwrap_or_else(|| "Incomplete".into())
    };
    let diagnostics = value
        .validation
        .diagnostics
        .iter()
        .map(|d| format!("{}: {}", d.path, d.code.replace('_', " ")))
        .collect::<Vec<_>>()
        .join("; ");
    let style = value.style.as_ref().map(|s| format!(
        "Style {} ({}): OG {:.3}–{:.3}, FG {:.3}–{:.3}, IBU {:.1}–{:.1}, ABV {:.1}–{:.1}%, colour {:.1}–{:.1} EBC.",
        s.name, s.edition, s.original_sg.min, s.original_sg.max, s.final_sg.min, s.final_sg.max,
        s.ibu.min, s.ibu.max, s.abv_percent.min, s.abv_percent.max, s.colour_ebc.min, s.colour_ebc.max
    )).unwrap_or_default();
    format!(
        "Readiness: {:?}. OG: {}; FG: {}; ABV: {}%; IBU: {}; colour: {} EBC. {} {}",
        value.validation.state,
        number(value.targets.original_sg, 3),
        number(value.targets.final_sg, 3),
        number(value.targets.abv_percent, 1),
        number(value.targets.ibu, 1),
        number(value.targets.colour_ebc, 1),
        style,
        diagnostics
    )
}

fn matches_import_context(request: &ImportRequest, result: &ImportPreview) -> bool {
    use krausen_types::{formulation::PreviewOutcome, validation::Readiness};
    if result.draft_generation != request.draft_generation {
        return false;
    }
    match &result.calculation {
        PreviewOutcome::Preview(preview) => {
            if preview.draft_generation != request.draft_generation {
                return false;
            }
            if preview.validation.state == Readiness::Invalid
                && preview.context.is_none()
                && preview.context_digest.is_none()
            {
                return true;
            }
            !preview.context_digest.as_deref().is_none_or(str::is_empty)
                && preview.context.as_ref().is_some_and(|context| {
                    context.workspace_id == request.intent.workspace_id
                        && context.draft == result.document
                        && match &request.equipment_profile_id {
                            Some(id) => context.equipment_profile.as_ref().is_some_and(|profile| {
                                profile.workspace_id == request.intent.workspace_id
                                    && profile.profile_id == *id
                                    && !profile.archived
                                    && context.equipment_draft == profile.settings
                                    && context.equipment_provenance == "saved_profile"
                            }),
                            // No profile named: the server used the workspace default if one is set.
                            None => match &context.equipment_profile {
                                Some(profile) => {
                                    profile.workspace_id == request.intent.workspace_id
                                        && !profile.archived
                                        && context.equipment_draft == profile.settings
                                        && context.equipment_provenance == "saved_profile"
                                }
                                None => context.equipment_draft == request.equipment_draft,
                            },
                        }
                })
        }
        PreviewOutcome::UnresolvedProfile | PreviewOutcome::UnresolvedStyle => true,
        _ => false,
    }
}

#[cfg(target_arch = "wasm32")]
fn hand_off_import_action_focus(event: &leptos::ev::MouseEvent) {
    use wasm_bindgen::JsCast;
    // Native disabling blurs an owned action. Reuse the same existing task
    // successor synchronously; focus alone neither activates Cancel nor navigates.
    let active = web_sys::window().and_then(|w| w.document()).and_then(|d| d.active_element());
    let button = event.target().and_then(|target| target.dyn_into::<web_sys::Element>().ok())
        .and_then(|target| target.closest("button").ok().flatten());
    if let Some(button) = button.filter(|button| active.as_ref() == Some(button)) {
        let cancel = button.closest("[role='group'][aria-label='Import actions']").ok().flatten()
            .and_then(|group| group.query_selector("a[href='/recipes']").ok().flatten());
        if let Some(cancel) = cancel.filter(|element| {
            element.is_connected() && element.get_client_rects().length() > 0
                && element.closest("[hidden], [inert], :disabled").ok().flatten().is_none()
                && web_sys::window().and_then(|w| w.get_computed_style(element).ok().flatten())
                    .and_then(|style| style.get_property_value("visibility").ok()).is_some_and(|v| v != "hidden")
        }) {
            if let Some(cancel) = cancel.dyn_ref::<web_sys::HtmlElement>() {
                if cancel.focus().is_err() { tracing::warn!("Unable to hand off import action focus"); }
            }
        }
    }
}

const UNCONFIRMED_TITLE: &str = "Import result unconfirmed";
const UNCONFIRMED_COPY: &str = "The import was sent and its result is unconfirmed. A recipe may already have been created. Check the previous result to confirm it; the original request is retained and is reused, so no duplicate is created.";
const EXPIRED_COPY: &str = "The server no longer holds this import's result, so Krausen cannot confirm whether a recipe was created. Check your recipe library before importing again. The original request stays retained until you acknowledge it.";
const CONFLICT_COPY: &str = "This request identity is bound to different content and cannot be resent. Check your recipe library before importing again. The original request stays retained until you acknowledge it.";
const MIN_RECIPE_NUMBER: &str = "Enter a recipe number starting at 1.";

fn recovery_copy(status: &RecoveryStatus) -> &'static str {
    match status {
        RecoveryStatus::Unknown => UNCONFIRMED_COPY,
        RecoveryStatus::Expired { .. } => EXPIRED_COPY,
        RecoveryStatus::IdentityConflict => CONFLICT_COPY,
    }
}

/// Only a returned, typed outcome is a known result. A transport failure or a
/// busy/unavailable answer leaves the sent command unconfirmed (INT-004).
fn commit_is_unconfirmed(outcome: &Result<ImportCommitOutcome, ServerFnError>) -> bool {
    matches!(
        outcome,
        Err(_) | Ok(ImportCommitOutcome::Retryable | ImportCommitOutcome::Unavailable)
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputField {
    Source,
    RecipeNumber,
}

/// Validate unsent source facts locally; the shared service remains the authority.
fn check_inputs(source: &str, number: &str) -> Result<usize, (InputField, &'static str)> {
    if source.trim().is_empty() {
        return Err((
            InputField::Source,
            "Paste BeerXML or choose a file before previewing.",
        ));
    }
    if source.len() > krausen_types::interchange::MAX_BEERXML_BYTES {
        return Err((InputField::Source, "The source is larger than 2 MiB."));
    }
    number
        .trim()
        .parse::<usize>()
        .ok()
        .and_then(|n| n.checked_sub(1))
        .ok_or((InputField::RecipeNumber, MIN_RECIPE_NUMBER))
}

/// Associate required interpretation choices with their own controls.
fn choice_errors(
    diagnostics: &[krausen_types::interchange::MappingDiagnostic],
) -> (Option<String>, Option<String>) {
    let has = |code: &str| diagnostics.iter().any(|d| d.code == code);
    (
        has("colour_interpretation_required")
            .then(|| "Choose how to interpret the source colour.".to_owned()),
        has("efficiency_choice_required")
            .then(|| "Choose how to use the source efficiency.".to_owned()),
    )
}

fn inputs_changed(source: &str, number: &str, colour: &str, efficiency: &str, profile: &str) -> bool {
    !source.is_empty()
        || number != "1"
        || !colour.is_empty()
        || !efficiency.is_empty()
        || !profile.is_empty()
}

/// The page owns the task; the shared guard only reads this summary. Retained
/// unknown work is not a dirty draft and is never described as cancellable.
fn import_task_state(
    changed: bool,
    imported: bool,
    retained: bool,
    committing: bool,
    generation: u64,
    context_visible: bool,
) -> GuardTaskState {
    GuardTaskState {
        dirty: changed && !imported && !retained,
        generation,
        in_flight: committing,
        uncertain: retained,
        recovery_retained: retained,
        context_visible,
    }
}

fn colour_value(choice: Option<ColourUnit>) -> &'static str {
    match choice {
        Some(ColourUnit::Srm) => "srm",
        Some(ColourUnit::Lovibond) => "lovibond",
        None => "",
    }
}

fn efficiency_value(choice: Option<EfficiencyChoice>) -> &'static str {
    match choice {
        Some(EfficiencyChoice::RecipeOverride) => "override",
        Some(EfficiencyChoice::SelectedEquipment) => "equipment",
        None => "",
    }
}

#[component]
pub fn BeerXmlPage() -> impl IntoView {
    let context = LocalResource::new(|| async {
        let context = crate::server_fns::context::get_user_context().await?;
        Ok::<_, ServerFnError>(OperationIntent {
            account_id: context.user_id,
            workspace_id: context
                .workspace_id
                .ok_or_else(|| ServerFnError::new("Workspace required"))?,
        })
    });
    view! { <Suspense fallback=|| view! { <p>"Loading import…"</p> }>
        {move || Suspend::new(async move { match context.await {
            Ok(identity) => view! { <BeerXmlForm identity=identity/> }.into_any(),
            Err(_) => view! { <BreweryNotice title="Unable to load import" body="Sign in and select a workspace." warning=true/> }.into_any(),
        } })}
    </Suspense> }
}

#[component]
fn BeerXmlForm(identity: OperationIntent) -> impl IntoView {
    let installation = StoredValue::new(GuardScope::installation());
    let scope = GuardScope {
        installation: installation.get_value(),
        account_id: identity.account_id.clone(),
        workspace_id: identity.workspace_id.clone(),
        resource_id: None,
        base_revision: None,
    };
    let identity = StoredValue::new(identity);
    let source = RwSignal::new(String::new());
    let colour = RwSignal::new(String::new());
    let efficiency = RwSignal::new(String::new());
    let recipe_number = RwSignal::new("1".to_string());
    let selected_profile = RwSignal::new(String::new());
    let profiles = RwSignal::new(Vec::<krausen_types::brewery::EquipmentProfile>::new());
    let profile_cursor = RwSignal::new(None::<String>);
    let profile_generation = RwSignal::new(0_u64);
    let profile_loading = RwSignal::new(false);
    let preview = RwSignal::new(None::<(ImportRequest, ImportPreview)>);
    // The exact sent (or about-to-be-sent) request and its identity.
    let pending = RwSignal::new(None::<(ImportRequest, String)>);
    // True once a sent request is unconfirmed (or recovered): the action becomes
    // 'Check previous result'. During the first in-flight send the original named
    // 'Import recipe' action stays visible and pending.
    let unconfirmed = RwSignal::new(false);
    let recovery_status = RwSignal::new(RecoveryStatus::Unknown);
    let recovery_checked = RwSignal::new(false);
    let recovery_error = RwSignal::new(None::<String>);
    let status = RwSignal::new(String::new());
    let source_error = RwSignal::new(None::<String>);
    let file_error = RwSignal::new(None::<String>);
    let number_error = RwSignal::new(None::<String>);
    let colour_error = RwSignal::new(None::<String>);
    let efficiency_error = RwSignal::new(None::<String>);
    let imported_id = RwSignal::new(None::<String>);
    let import_diagnostics =
        RwSignal::new(Vec::<krausen_types::interchange::MappingDiagnostic>::new());
    let visible = RwSignal::new(true);
    let busy = RwSignal::new(false);
    let committing = RwSignal::new(false);
    let sequence = RwSignal::new(0_u64);
    // Fences late previews after a discard without changing the guard generation.
    let discard_epoch = RwSignal::new(0_u64);
    let clear_errors = move || {
        source_error.set(None);
        file_error.set(None);
        number_error.set(None);
        colour_error.set(None);
        efficiency_error.set(None);
    };
    let invalidate = Callback::new(move |()| {
        sequence.update(|n| *n = n.checked_add(1).expect("import generation exhausted"));
        preview.set(None);
        pending.set(None);
        unconfirmed.set(false);
        imported_id.set(None);
        import_diagnostics.set(Vec::new());
        status.set(String::new());
        clear_errors();
    });
    // Reports whether the retained record is really gone. Callers pass the
    // installation captured while the page was live, so a late completion after
    // disposal never reads disposed state.
    let forget_retained = move |installation: &str, intent: &OperationIntent, request_id: &str| -> bool {
        match beerxml_recovery::remove(installation, intent, request_id) {
            Ok(()) => true,
            Err(message) => {
                tracing::warn!("Unable to remove resolved import recovery: {message}");
                false
            }
        }
    };
    let editor_guard = expect_context::<EditorGuard>();
    let task = editor_guard.register_task(
        GuardTaskKind::Import,
        scope,
        GuardTaskCallbacks {
            save_and_leave: None,
            return_to_actions: None,
            // Leaving never cancels a sent request; only unsent inputs are dropped.
            discard_unsent: Callback::new(move |()| {
                if pending.get_untracked().is_none() {
                    source.set(String::new());
                    colour.set(String::new());
                    efficiency.set(String::new());
                    recipe_number.set("1".into());
                    selected_profile.set(String::new());
                    // The shared guard fences on the task generation; discarding
                    // must not look like a newer edit, so `sequence` is untouched.
                    // The task effect later clears `resolved_navigation` when `dirty`
                    // flips; that is harmless because the leave already captured
                    // its owners and lease compares only generation/in-flight.
                    discard_epoch.update(|n| *n = n.wrapping_add(1));
                    preview.set(None);
                    imported_id.set(None);
                    import_diagnostics.set(Vec::new());
                    status.set(String::new());
                    clear_errors();
                }
                true
            }),
        },
    );
    Effect::new(move |_| {
        let changed = inputs_changed(
            &source.get(),
            &recipe_number.get(),
            &colour.get(),
            &efficiency.get(),
            &selected_profile.get(),
        );
        task.update(import_task_state(
            changed,
            imported_id.get().is_some(),
            pending.get().is_some(),
            committing.get(),
            sequence.get(),
            visible.get(),
        ));
    });
    on_cleanup(move || task.unregister());
    // Reopen retained unknown work for this exact installation/account/workspace.
    Effect::new(move |_| {
        if recovery_checked.get_untracked() {
            return;
        }
        match beerxml_recovery::load(&installation.get_value(), &identity.get_value()) {
            Ok(Some(record)) => {
                source.set(record.request.xml.clone());
                recipe_number.set((record.request.recipe_index + 1).to_string());
                colour.set(colour_value(record.request.choices.colour).into());
                efficiency.set(efficiency_value(record.request.choices.efficiency).into());
                selected_profile.set(record.request.equipment_profile_id.clone().unwrap_or_default());
                recovery_status.set(record.status);
                pending.set(Some((record.request, record.request_id)));
                unconfirmed.set(true);
            }
            Ok(None) => {}
            Err(message) => recovery_error.set(Some(message)),
        }
        recovery_checked.set(true);
    });
    let load_profiles = Callback::new(move |cursor: Option<String>| {
        if cursor.is_none() {
            profile_generation
                .update(|n| *n = n.checked_add(1).expect("profile generation exhausted"));
        }
        let generation = profile_generation.get_untracked();
        let append = cursor.is_some();
        profile_loading.set(true);
        let expected = identity.get_value();
        leptos::task::spawn_local(async move {
            if !current_context_matches(&expected).await {
                visible.set(false);
                profile_loading.set(false);
                return;
            }
            let outcome = crate::server_fns::equipment::equipment_command_shared(
                krausen_types::equipment::EquipmentCommand::List(
                    krausen_types::equipment::EquipmentListQuery {
                        intent: expected.clone(),
                        cursor,
                        limit: 100,
                        include_archived: false,
                    },
                ),
            )
            .await;
            if !current_context_matches(&expected).await {
                visible.set(false);
                profile_loading.set(false);
                return;
            }
            if profile_generation.try_get_untracked() != Some(generation) {
                return;
            }
            profile_loading.set(false);
            if let Ok(krausen_types::equipment::EquipmentOutcome::Profiles(page)) = outcome {
                if append {
                    profiles.update(|items| items.extend(page.items));
                } else {
                    // An import with no chosen profile uses the workspace default, so show it chosen.
                    if let Some(default) = &page.default_profile {
                        if selected_profile.get_untracked().is_empty()
                            && preview.get_untracked().is_none()
                            && pending.get_untracked().is_none()
                        {
                            selected_profile.set(default.profile_id.clone());
                        }
                    }
                    profiles.set(page.items);
                }
                profile_cursor.set(page.next_cursor);
            } else {
                status.set("Unable to load equipment profiles.".into());
            }
        });
    });
    Effect::new(move |_| {
        if visible.get() {
            load_profiles.run(None);
        } else {
            profile_generation
                .update(|n| *n = n.checked_add(1).expect("profile generation exhausted"));
        }
    });
    #[cfg(target_arch = "wasm32")]
    {
        let recheck = Callback::new(move |()| {
            visible.set(false);
            sequence.update(|n| *n = n.checked_add(1).expect("import generation exhausted"));
            let generation = sequence.get_untracked();
            let expected = identity.get_value();
            leptos::task::spawn_local(async move {
                let matches = current_context_matches(&expected).await;
                if sequence.try_get_untracked() == Some(generation) {
                    visible.set(matches);
                }
            });
        });
        let focus = window_event_listener(leptos::ev::focus, move |_| recheck.run(()));
        let storage = window_event_listener(leptos::ev::storage, move |event| {
            if event.key().as_deref() == Some(crate::components::layout::CONTEXT_CHANGED_KEY) {
                recheck.run(());
            }
        });
        on_cleanup(move || {
            focus.remove();
            storage.remove();
        });
    }
    let preview_action = move |_event: leptos::ev::MouseEvent| {
        if busy.get_untracked() || pending.get_untracked().is_some() {
            return;
        }
        clear_errors();
        let recipe_index = match check_inputs(
            &source.get_untracked(),
            &recipe_number.get_untracked(),
        ) {
            Ok(index) => index,
            Err((field, message)) => {
                match field {
                    InputField::Source => source_error.set(Some(message.into())),
                    InputField::RecipeNumber => number_error.set(Some(message.into())),
                }
                return;
            }
        };
        let Some(generation) = crate::utils::request_id::new_request_id() else {
            status.set("Unable to prepare a preview.".into());
            return;
        };
        let request = ImportRequest {
            intent: identity.get_value(),
            xml: source.get_untracked(),
            recipe_index,
            choices: ImportChoices {
                colour: match colour.get_untracked().as_str() {
                    "srm" => Some(ColourUnit::Srm),
                    "lovibond" => Some(ColourUnit::Lovibond),
                    _ => None,
                },
                efficiency: match efficiency.get_untracked().as_str() {
                    "override" => Some(EfficiencyChoice::RecipeOverride),
                    "equipment" => Some(EfficiencyChoice::SelectedEquipment),
                    _ => None,
                },
            },
            equipment_profile_id: (!selected_profile.get_untracked().is_empty())
                .then(|| selected_profile.get_untracked()),
            expected_profile_revision: None,
            expected_profile_metadata_revision: None,
            equipment_draft: EquipmentSettingsV1::default(),
            draft_generation: generation,
        };
        sequence.update(|n| *n = n.checked_add(1).expect("import generation exhausted"));
        let current = sequence.get_untracked();
        let epoch = discard_epoch.get_untracked();
        // Rejected source/index/generation paths above never move focus.
        #[cfg(target_arch = "wasm32")]
        hand_off_import_action_focus(&_event);
        busy.set(true);
        leptos::task::spawn_local(async move {
            if !current_context_matches(&request.intent).await {
                visible.set(false);
                busy.set(false);
                return;
            }
            let outcome = preview_beerxml(request.clone()).await;
            #[cfg(target_arch = "wasm32")]
            let outcome = if outcome
                .as_ref()
                .err()
                .is_some_and(|e| crate::utils::auth_refresh::is_auth_error(&e.to_string()))
                && crate::utils::auth_refresh::try_refresh().await
                && current_context_matches(&request.intent).await
            {
                preview_beerxml(request.clone()).await
            } else {
                outcome
            };
            if !current_context_matches(&request.intent).await {
                visible.set(false);
                busy.set(false);
                return;
            }
            busy.set(false);
            if sequence.try_get_untracked() != Some(current)
                || discard_epoch.try_get_untracked() != Some(epoch)
            {
                return;
            }
            match outcome {
                Ok(ImportPreviewOutcome::Preview(result)) => {
                    if !matches_import_context(&request, &result) {
                        status.set("Preview context changed. Try again.".into());
                        return;
                    }
                    let (colour_problem, efficiency_problem) = choice_errors(&result.diagnostics);
                    colour_error.set(colour_problem);
                    efficiency_error.set(efficiency_problem);
                    let mut request = request;
                    request.expected_profile_revision = result.selected_profile_revision;
                    request.expected_profile_metadata_revision =
                        result.selected_profile_metadata_revision;
                    if let krausen_types::formulation::PreviewOutcome::Preview(calculation) =
                        &result.calculation
                    {
                        request.expected_profile_revision = calculation
                            .context
                            .as_ref()
                            .and_then(|context| context.equipment_profile.as_ref())
                            .map(|profile| profile.revision);
                        request.expected_profile_metadata_revision = calculation
                            .context
                            .as_ref()
                            .and_then(|context| context.equipment_profile.as_ref())
                            .map(|profile| profile.metadata_revision);
                    }
                    preview.set(Some((request, result)));
                    status.set(String::new());
                }
                Ok(ImportPreviewOutcome::InvalidSource) => {
                    source_error.set(Some("This is not a BeerXML recipe file the importer can read. Check the source and try again.".into()));
                }
                _ => status
                    .set("Unable to preview this BeerXML. Check the source and try again.".into()),
            }
        });
    };
    let commit_action = move |_| {
        if busy.get_untracked() {
            return;
        }
        let attempt = match pending.get_untracked() {
            Some(attempt) => attempt,
            None => {
                let Some((request, _)) = preview.get_untracked() else {
                    return;
                };
                let Some(id) = crate::utils::request_id::new_request_id() else {
                    status.set("Unable to prepare import.".into());
                    return;
                };
                // The original request is retained before it can be sent.
                if let Err(message) = beerxml_recovery::retain(&ImportRecovery::new(
                    &installation.get_value(),
                    &request,
                    &id,
                )) {
                    status.set(format!("{message} Nothing was sent."));
                    return;
                }
                let attempt = (request, id);
                pending.set(Some(attempt.clone()));
                recovery_status.set(RecoveryStatus::Unknown);
                attempt
            }
        };
        busy.set(true);
        committing.set(true);
        let inst = installation.get_value();
        leptos::task::spawn_local(async move {
            if !current_context_matches(&attempt.0.intent).await {
                visible.set(false);
                busy.set(false);
                committing.set(false);
                return;
            }
            let outcome = commit_beerxml(attempt.0.clone(), attempt.1.clone()).await;
            #[cfg(target_arch = "wasm32")]
            let outcome = if outcome
                .as_ref()
                .err()
                .is_some_and(|e| crate::utils::auth_refresh::is_auth_error(&e.to_string()))
                && crate::utils::auth_refresh::try_refresh().await
                && current_context_matches(&attempt.0.intent).await
            {
                commit_beerxml(attempt.0.clone(), attempt.1.clone()).await
            } else {
                outcome
            };
            if !current_context_matches(&attempt.0.intent).await {
                // The request was already sent: the result is unknown.
                unconfirmed.set(true);
                visible.set(false);
                busy.set(false);
                committing.set(false);
                return;
            }
            busy.set(false);
            committing.set(false);
            if commit_is_unconfirmed(&outcome) {
                unconfirmed.set(true);
                recovery_status.set(RecoveryStatus::Unknown);
                status.set(UNCONFIRMED_TITLE.into());
                return;
            }
            match outcome {
                Ok(ImportCommitOutcome::Forbidden | ImportCommitOutcome::ContextChanged) => {
                    unconfirmed.set(true);
                    // Not confirmed either way: the retained request stays.
                    visible.set(false);
                    return;
                }
                Ok(ImportCommitOutcome::ResultExpired { resource_id }) => {
                    let next = RecoveryStatus::Expired { resource_id };
                    if let Err(message) = beerxml_recovery::set_status(&inst, &attempt.0.intent, &attempt.1, next.clone()) {
                        tracing::warn!("Unable to retain expired import status: {message}");
                    }
                    unconfirmed.set(true);
                    recovery_status.set(next);
                    status.set(UNCONFIRMED_TITLE.into());
                    return;
                }
                Ok(ImportCommitOutcome::RequestIdentityConflict) => {
                    if let Err(message) = beerxml_recovery::set_status(&inst, &attempt.0.intent, &attempt.1, RecoveryStatus::IdentityConflict) {
                        tracing::warn!("Unable to retain identity-conflict status: {message}");
                    }
                    unconfirmed.set(true);
                    recovery_status.set(RecoveryStatus::IdentityConflict);
                    status.set(UNCONFIRMED_TITLE.into());
                    return;
                }
                _ => {}
            }
            // Every remaining outcome is a returned, known result.
            let cleaned = forget_retained(&inst, &attempt.0.intent, &attempt.1);
            pending.set(None);
            unconfirmed.set(false);
            match outcome {
                Ok(ImportCommitOutcome::Imported {
                    recipe_id,
                    diagnostics,
                    ..
                }) => {
                    preview.set(None);
                    imported_id.set(Some(recipe_id));
                    import_diagnostics.set(diagnostics);
                    status.set(if cleaned { "Recipe imported." } else { "Recipe imported. The local recovery record could not be removed; Check previous result may be offered once more." }.into());
                }
                Ok(ImportCommitOutcome::ChoicesRequired(diagnostics)) => {
                    let (colour_problem, efficiency_problem) = choice_errors(&diagnostics);
                    colour_error.set(colour_problem);
                    efficiency_error.set(efficiency_problem);
                    status.set(
                        "Choose how to interpret source colour and efficiency, then preview again."
                            .into(),
                    );
                }
                Ok(ImportCommitOutcome::Invalid(_)) => {
                    status.set(
                        "The source contains invalid values that must be resolved before import."
                            .into(),
                    );
                }
                Ok(ImportCommitOutcome::ProfileChanged) => {
                    preview.set(None);
                    status.set(
                        "The equipment profile changed. Preview again before importing.".into(),
                    );
                }
                Ok(ImportCommitOutcome::UnresolvedProfile) => {
                    preview.set(None);
                    status.set("The selected equipment is unavailable. Choose equipment and preview again.".into());
                }
                _ => {
                    preview.set(None);
                    status.set(
                        "This import request is invalid. Review the source and preview again."
                            .into(),
                    );
                }
            }
        });
    };
    // A resolved-but-unconfirmable request is acknowledged by name, never by a
    // fresh "retry"; the user must then re-preview deliberately.
    let acknowledge = move |_| {
        if busy.get_untracked() {
            return;
        }
        let Some((request, id)) = pending.get_untracked() else {
            return;
        };
        if recovery_status.get_untracked() == RecoveryStatus::Unknown {
            return;
        }
        if !forget_retained(&installation.get_value(), &request.intent, &id) {
            status.set("The retained request could not be cleared. Try again.".into());
            return;
        }
        invalidate.run(());
        status.set("The retained import request was cleared. No new import was sent.".into());
    };
    let status_node = NodeRef::<leptos::html::P>::new();
    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        if imported_id.get().is_none() {
            return;
        }
        // Move focus to the result action only from the task's own actions or the
        // document body; never steal it from somewhere the user has moved.
        let _ = request_animation_frame_with_handle(move || {
            use wasm_bindgen::JsCast;
            let Some(document) = web_sys::window().and_then(|w| w.document()) else { return; };
            let in_actions = |element: &web_sys::Element| element.closest("[role='group'][aria-label='Import actions']").ok().flatten().is_some();
            let allowed = document.active_element().is_none_or(|active| {
                matches!(active.tag_name().as_str(), "BODY" | "HTML") || in_actions(&active)
            });
            if !allowed { return; }
            let Some(link) = document.query_selector("[role='group'][aria-label='Import actions'] a[href^='/recipe-editor/']").ok().flatten() else { return; };
            if let Some(link) = link.dyn_ref::<web_sys::HtmlElement>() {
                if link.focus().is_err() { tracing::warn!("Unable to focus the imported recipe action"); }
            }
        });
    });
    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        let _ = status.get();
        let Some(marker) = status_node.get() else { return; };
        let Some(document) = web_sys::window().and_then(|w| w.document()) else { return; };
        let Some(active) = document.active_element() else { return; };
        let Some(panel) = marker.closest(".k-panel").ok().flatten() else { return; };
        if active.closest("[role='group'][aria-label='Import actions']").ok().flatten().is_none()
            || active.closest(".k-panel").ok().flatten().as_ref() != Some(&panel) { return; }
        // Status growth can move a retained focused action below the viewport.
        // Wait for its real layout, then reveal only that same current task owner.
        if request_animation_frame_with_handle(move || {
            use wasm_bindgen::JsCast;
            if visible.try_get_untracked() != Some(true) || !marker.is_connected() || !active.is_connected()
                || document.active_element().as_ref() != Some(&active) || document.has_focus().ok() != Some(true)
                || !panel.contains(Some(&active))
                || marker.closest(".k-panel").ok().flatten().as_ref() != Some(&panel)
                || active.closest(".k-panel").ok().flatten().as_ref() != Some(&panel)
                || active.closest("[role='group'][aria-label='Import actions']").ok().flatten().is_none()
                || !active.matches("button, a").unwrap_or(false)
                || active.closest("[hidden], [inert], :disabled").ok().flatten().is_some()
                || active.get_client_rects().length() == 0 { return; }
            let Some(window) = web_sys::window() else { return; };
            if window.get_computed_style(&active).ok().flatten()
                .and_then(|style| style.get_property_value("visibility").ok()).is_none_or(|v| v == "hidden" || v == "collapse") { return; }
            let Some(width) = window.inner_width().ok().and_then(|v| v.as_f64()) else { return; };
            let Some(height) = window.inner_height().ok().and_then(|v| v.as_f64()) else { return; };
            let rect = active.get_bounding_client_rect();
            if rect.top() >= 0.0 && rect.bottom() <= height && rect.left() >= 0.0 && rect.right() <= width { return; }
            // Native element-scoped scrolling chooses the actual ancestor chain;
            // no window offsets, focus call, global listener or smooth motion.
            let reveal = js_sys::Reflect::get(active.as_ref(), &"scrollIntoView".into())
                .and_then(|method| method.dyn_into::<js_sys::Function>())
                .and_then(|method| {
                    let options = js_sys::JSON::parse(r#"{"block":"center","inline":"nearest","behavior":"instant"}"#)?;
                    method.call1(active.as_ref(), &options)
                });
            if reveal.is_err() { tracing::warn!("Unable to reveal focused import action"); }
        }).is_err() { tracing::warn!("Unable to schedule import action reveal"); }
    });
    let busy_label = Signal::derive(move || busy.get() || pending.get().is_some() || !recovery_checked.get() || recovery_error.get().is_some());
    view! { <Show when=move || visible.get() fallback=|| view! { <BreweryNotice title="Account or workspace changed" body="Return to the original context to continue this import." warning=true/> }>
        <crate::components::wayfinding::PageTitle text=Signal::derive(|| "Import recipes".to_owned())/>
        <crate::components::wayfinding::Breadcrumb items=vec![crate::components::wayfinding::Crumb::link("Recipes", "/recipes"), crate::components::wayfinding::Crumb::current("Import recipes")]/>
        <h1 id="beerxml-heading" tabindex="-1">"Import recipes"</h1>
        <BreweryPanel title="Import BeerXML" description="Preview before creating a recipe. The original file is retained separately."
            footer=std::sync::Arc::new(move || view! {
                {move || {
                    let has_preview = preview.get().is_some();
                    let imported = imported_id.get();
                    let preview_variant = if has_preview || imported.is_some() { ButtonVariant::Secondary } else { ButtonVariant::Default };
                    view! {
                        <BreweryActionGroup label="Import actions">
                            <Show when=move || !unconfirmed.get()>
                                <Button
                                    variant=preview_variant
                                    disabled=busy_label
                                    pending=Signal::derive(move || busy.get() && pending.get().is_none())
                                    on:click=preview_action
                                >"Preview"</Button>
                            </Show>
                            <Show when=move || preview.get().is_some() && !unconfirmed.get()>
                                <Button disabled=Signal::derive(move || busy.get() || colour_error.get().is_some() || efficiency_error.get().is_some()) pending=Signal::derive(move || committing.get()) on:click=move |event: leptos::ev::MouseEvent| {
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        if !busy.get_untracked() && preview.get_untracked().is_some() {
                                            hand_off_import_action_focus(&event);
                                        }
                                    }
                                    commit_action(event);
                                }>"Import recipe"</Button>
                            </Show>
                            <Show when=move || unconfirmed.get()>
                                <Button disabled=Signal::derive(move || busy.get()) pending=Signal::derive(move || committing.get()) on:click=move |event: leptos::ev::MouseEvent| {
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        if !busy.get_untracked() { hand_off_import_action_focus(&event); }
                                    }
                                    commit_action(event);
                                }>"Check previous result"</Button>
                            </Show>
                            <Show when=move || unconfirmed.get() && recovery_status.get() != RecoveryStatus::Unknown>
                                <Button variant=ButtonVariant::Secondary disabled=Signal::derive(move || busy.get()) on:click=acknowledge>"Acknowledge and clear retained request"</Button>
                            </Show>
                            {imported.map(|id| view! { <ButtonLink variant=if has_preview { ButtonVariant::Secondary } else { ButtonVariant::Default } href=format!("/recipe-editor/{id}")>"Open imported recipe"</ButtonLink> })}
                            <ButtonLink variant=ButtonVariant::Ghost href="/recipes">{move || if unconfirmed.get() { "Back to recipes" } else { "Cancel" }}</ButtonLink>
                        </BreweryActionGroup>
                    }
                }}
            }.into_any())
        >
            {move || recovery_error.get().map(|message| view! { <BreweryNotice title="Import recovery unavailable" body=message warning=true/> })}
            <Show when=move || recovery_error.get().is_some()>
                <BreweryActionGroup label="Recovery actions">
                    <Button variant=ButtonVariant::Secondary on:click=move |_| {
                        match beerxml_recovery::discard_unreadable(&installation.get_value(), &identity.get_value()) {
                            Ok(()) => { recovery_error.set(None); status.set("The unreadable retained import record was discarded. Check your recipe library before importing again.".into()); }
                            Err(message) => status.set(message),
                        }
                    }>"Discard unreadable retained request"</Button>
                </BreweryActionGroup>
            </Show>
            {move || pending.get().map(|(request, _)| {
                let body = recovery_copy(&recovery_status.get());
                let pinned = format!("Pinned source: recipe {} of the file, {} bytes. It cannot be changed while this request is retained.", request.recipe_index + 1, request.xml.len());
                view! { <div id="beerxml-recovery"><BreweryNotice title=UNCONFIRMED_TITLE body=body warning=true/><p>{pinned}</p></div> }
            })}
            <fieldset disabled=move || pending.get().is_some()>
            <BrewerySection title="Source">
            <label for="beerxml-file">"Choose a UTF-8 BeerXML file"</label>
            <input id="beerxml-file" type="file" class=crate::components::INPUT_CLASS accept=".xml,.beerxml,text/xml,application/xml"
                aria-invalid=move || file_error.get().is_some().to_string()
                aria-describedby=move || file_error.get().map(|_| "beerxml-file-error")
                on:change=move |event| {
                #[cfg(target_arch = "wasm32")]
                {
                    use wasm_bindgen::JsCast;
                    if pending.get_untracked().is_some() { return; }
                    let file = event.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok()).and_then(|i| i.files()).and_then(|f| f.get(0));
                    if let Some(file) = file {
                        if file.size() > krausen_types::interchange::MAX_BEERXML_BYTES as f64 { invalidate.run(()); file_error.set(Some("The file is larger than 2 MiB.".into())); return; }
                        invalidate.run(());
                        let generation = sequence.get_untracked();
                        leptos::task::spawn_local(async move {
                            match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                                Ok(buffer) => match String::from_utf8(js_sys::Uint8Array::new(&buffer).to_vec()) {
                                    Ok(text) if sequence.try_get_untracked() == Some(generation) && pending.get_untracked().is_none() => { source.set(text); invalidate.run(()); }
                                    Ok(_) => {}
                                    Err(_) => { invalidate.run(()); file_error.set(Some("The file must be UTF-8.".into())); }
                                },
                                Err(_) => { invalidate.run(()); file_error.set(Some("Unable to read the selected file.".into())); }
                            }
                        });
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                let _ = event;
            }/>
            {move || file_error.get().map(|message| view! { <span id="beerxml-file-error" class="k-field__error">{message}</span> })}
            <BreweryDraftArea id="beerxml-source" label="Paste BeerXML" value=Signal::from(source) error=source_error on_change=Callback::new(move |v| { if pending.get_untracked().is_none() { source.set(v); invalidate.run(()); } })/>
            <BreweryDraftField id="beerxml-recipe-number" label="Recipe number in file" value=Signal::from(recipe_number) error=number_error numeric=true on_change=Callback::new(move |v| { if pending.get_untracked().is_none() { recipe_number.set(v); invalidate.run(()); } })/>
            </BrewerySection>
            <BrewerySection title="Interpretation and equipment">
            <BreweryDraftSelect id="beerxml-colour" label="Source colour" value=Signal::from(colour) error=colour_error on_change=Callback::new(move |v| { if pending.get_untracked().is_none() { colour.set(v); invalidate.run(()); } }) options=vec![("".into(), "Choose when present".into()), ("srm".into(), "SRM".into()), ("lovibond".into(), "Lovibond".into())]/>
            <BreweryDraftSelect id="beerxml-efficiency" label="Source efficiency" value=Signal::from(efficiency) error=efficiency_error on_change=Callback::new(move |v| { if pending.get_untracked().is_none() { efficiency.set(v); invalidate.run(()); } }) options=vec![("".into(), "Choose when present".into()), ("override".into(), "Use as recipe override".into()), ("equipment".into(), "Use selected equipment".into())]/>
            {move || { let mut options = vec![(String::new(), "No profile chosen (the workspace default is used if one is set)".to_string())]; options.extend(profiles.get().into_iter().map(|p| (p.profile_id, p.name))); view! {
                <BreweryDraftSelect id="beerxml-equipment" label="Equipment profile" value=Signal::from(selected_profile) on_change=Callback::new(move |v| { if pending.get_untracked().is_none() { selected_profile.set(v); invalidate.run(()); } }) options=options/>
            } }}
            <Show when=move || profile_cursor.get().is_some()><Button variant=ButtonVariant::Secondary on:click=move |_| { if !profile_loading.get_untracked() { load_profiles.run(profile_cursor.get_untracked()); } }>"Load more equipment profiles"</Button></Show>
            </BrewerySection>
            </fieldset>
            {move || preview.get().map(|(_, result)| {
                let estimate = |value: Option<String>| value.unwrap_or_else(|| "not supplied".into());
                let estimates = result.source_estimates.clone();
                view! {
                <section aria-label="Import preview"><h3>{result.document.title.clone()}</h3>
                    <p>{format!("{} fermentables, {} hop additions", result.document.fermentables.len(), result.document.hops.len())}</p>
                    <p>{calculation_summary(result.calculation)}</p>
                    <h4>"Missing or unsupported information"</h4>
                    {if result.diagnostics.is_empty() {
                        view! { <p>"None reported."</p> }.into_any()
                    } else {
                        view! { <ul>{result.diagnostics.into_iter().map(|d| view! { <li>{format!("{}: {}", d.path, d.message)}</li> }).collect_view()}</ul> }.into_any()
                    }}
                    <p>{format!("Source estimates (not calculated by Krausen): OG {}, FG {}, bitterness {}, colour {}, alcohol {}", estimate(estimates.original_gravity), estimate(estimates.final_gravity), estimate(estimates.bitterness), estimate(estimates.colour), estimate(estimates.alcohol))}</p>
                    <details><summary>"Source details"</summary>
                        <p>{format!("Original source SHA-256: {}", result.source_sha256)}</p>
                        <p>"The original bytes are retained unchanged and stay separate from the recipe's current values."</p>
                    </details>
                </section>
            } })}
            <p node_ref=status_node role="status">{move || status.get()}</p>
            <ul>{move || import_diagnostics.get().into_iter().map(|d| view! { <li>{d.message}</li> }).collect_view()}</ul>
        </BreweryPanel>
    </Show> }
}

/// BeerXML export as an on-demand, source-bound task (INT-008). The source is
/// known from the editor state or chosen once when the saved head and the
/// unsaved draft differ; a saved version selected in Saved versions supplies its
/// own revision, so no revision number is typed. The unedited original import is
/// a separate, labelled download and never replaces current native values.
#[component]
pub(super) fn BeerXmlExport(
    identity: RwSignal<OperationIntent>,
    context_visible: RwSignal<bool>,
    recipe_id: RwSignal<Option<String>>,
    revision: RwSignal<Option<i64>>,
    draft: RwSignal<crate::pages::recipe_draft::RecipeDraft>,
    open: RwSignal<bool>,
    selected: RwSignal<Option<i64>>,
) -> impl IntoView {
    use crate::pages::recipe_output::{OutputSource, SourceChoice, resolve_source};
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use krausen_types::interchange::{
        ExportOutcome, ExportRequest, ExportSelection, OriginalOutcome,
    };
    let source = Signal::derive(move || {
        resolve_source(revision.get(), draft.with(|d| d.dirty), selected.get())
    });
    let has_original = Signal::derive(move || {
        recipe_id.get().is_some() && draft.with(|d| d.document().import_provenance_id.is_some())
    });
    let output = RwSignal::new(None::<String>);
    let original = RwSignal::new(None::<String>);
    let warnings = RwSignal::new(Vec::<krausen_types::interchange::MappingDiagnostic>::new());
    let pending = RwSignal::new(None::<ExportRequest>);
    let draft_export_generation = RwSignal::new(None::<u64>);
    let export_sequence = RwSignal::new(0_u64);
    let status = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let clear = move || {
        output.set(None);
        original.set(None);
        pending.set(None);
        warnings.set(Vec::new());
        draft_export_generation.set(None);
    };
    // Retires any request in flight: its late result is dropped by the sequence
    // fence, and `busy` is released here so the task can be started again.
    let retire = move || {
        export_sequence.update(|n| *n = n.checked_add(1).expect("export sequence exhausted"));
        busy.set(false);
        clear();
    };
    Effect::new(move |_| {
        let generation = draft.with(|d| d.generation);
        if draft_export_generation
            .get()
            .is_some_and(|expected| expected != generation)
        {
            output.set(None);
            pending.set(None);
            warnings.set(Vec::new());
            draft_export_generation.set(None);
            status.set("The draft changed. Open Export again.".into());
        }
    });
    let focus_result = move |id: &'static str| {
        #[cfg(target_arch = "wasm32")]
        request_animation_frame(move || crate::utils::focus::focus_element(id));
        #[cfg(not(target_arch = "wasm32"))]
        let _ = id;
    };
    let run_export = Callback::new(move |request: ExportRequest| {
        if busy.get_untracked() {
            return;
        }
        export_sequence.update(|n| *n = n.checked_add(1).expect("export sequence exhausted"));
        let sequence = export_sequence.get_untracked();
        busy.set(true);
        let expected = request.intent.clone();
        let expected_generation = if matches!(&request.selection, ExportSelection::Draft { .. }) {
            draft_export_generation.get_untracked()
        } else {
            None
        };
        leptos::task::spawn_local(async move {
            let stale = move || export_sequence.try_get_untracked() != Some(sequence);
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                return;
            }
            if stale() {
                return;
            }
            let result = crate::server_fns::interchange::export_beerxml(request.clone()).await;
            #[cfg(target_arch = "wasm32")]
            let result = if result
                .as_ref()
                .err()
                .is_some_and(|e| crate::utils::auth_refresh::is_auth_error(&e.to_string()))
                && crate::utils::auth_refresh::try_refresh().await
                && current_context_matches(&expected).await
            {
                crate::server_fns::interchange::export_beerxml(request.clone()).await
            } else {
                result
            };
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                return;
            }
            if stale() {
                return;
            }
            busy.set(false);
            if expected_generation
                .is_some_and(|generation| draft.with_untracked(|d| d.generation) != generation)
            {
                status.set("The draft changed. Open Export again.".into());
                return;
            }
            match result {
                Ok(ExportOutcome::LossAcknowledgementRequired(diagnostics)) => {
                    warnings.set(diagnostics);
                    pending.set(Some(request));
                    status.set("Review the omitted information before downloading.".into());
                    focus_result("export-acknowledge");
                }
                Ok(ExportOutcome::Exported {
                    xml,
                    diagnostics,
                    unsaved_draft,
                }) => {
                    warnings.set(diagnostics);
                    pending.set(None);
                    output.set(Some(format!(
                        "data:application/xml;base64,{}",
                        STANDARD.encode(xml.as_bytes())
                    )));
                    status.set(
                        if unsaved_draft {
                            "Unsaved draft export is ready."
                        } else {
                            "Saved revision export is ready."
                        }
                        .into(),
                    );
                    focus_result("export-result");
                }
                _ => {
                    output.set(None);
                    status.set("Unable to export this revision or draft.".into());
                }
            }
        });
    });
    let prepare = Callback::new(move |chosen: OutputSource| {
        clear();
        status.set(String::new());
        match chosen {
            OutputSource::Saved { revision: number } => {
                let Some(id) = recipe_id.get_untracked() else {
                    status.set("Save the recipe before exporting a saved revision.".into());
                    return;
                };
                run_export.run(ExportRequest {
                    intent: identity.get_untracked(),
                    selection: ExportSelection::Saved {
                        recipe_id: id,
                        revision: number,
                    },
                    acknowledge_loss: false,
                });
            }
            OutputSource::Draft => {
                if !draft.with_untracked(|d| d.unfinished_inputs().is_empty()) {
                    status.set("Complete unfinished fields before exporting the draft.".into());
                    return;
                }
                draft_export_generation.set(Some(draft.with_untracked(|d| d.generation)));
                run_export.run(ExportRequest {
                    intent: identity.get_untracked(),
                    selection: ExportSelection::Draft {
                        recipe_id: recipe_id.get_untracked(),
                        document: draft.with_untracked(|d| d.document().clone()),
                    },
                    acknowledge_loss: false,
                });
            }
        }
    });
    // Opening resets the task; a known source is prepared at once, an ambiguous
    // one waits for the short choice. Closing retires any in-flight request.
    // It also runs again when a revalidation hides and restores the editor, so a
    // result dropped by the check is prepared afresh instead of being stranded.
    Effect::new(move |_| {
        if open.get() && context_visible.get() {
            clear();
            status.set(String::new());
            if let SourceChoice::Known(chosen) = source.get_untracked() {
                prepare.run(chosen);
            }
        } else {
            retire();
        }
    });
    let fetch_original = move |_| {
        if busy.get_untracked() {
            return;
        }
        let Some(id) = recipe_id.get_untracked() else {
            return;
        };
        let expected = identity.get_untracked();
        export_sequence.update(|n| *n = n.checked_add(1).expect("export sequence exhausted"));
        let sequence = export_sequence.get_untracked();
        busy.set(true);
        original.set(None);
        leptos::task::spawn_local(async move {
            let stale = move || export_sequence.try_get_untracked() != Some(sequence);
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                return;
            }
            if stale() {
                return;
            }
            let result =
                crate::server_fns::interchange::original_beerxml(expected.clone(), id.clone())
                    .await;
            #[cfg(target_arch = "wasm32")]
            let result = if result
                .as_ref()
                .err()
                .is_some_and(|e| crate::utils::auth_refresh::is_auth_error(&e.to_string()))
                && crate::utils::auth_refresh::try_refresh().await
                && current_context_matches(&expected).await
            {
                crate::server_fns::interchange::original_beerxml(expected.clone(), id).await
            } else {
                result
            };
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                return;
            }
            if stale() {
                return;
            }
            busy.set(false);
            match result {
                Ok(OriginalOutcome::Original { bytes, sha256 }) => {
                    original.set(Some(format!(
                        "data:application/xml;base64,{}",
                        STANDARD.encode(bytes)
                    )));
                    status.set(format!(
                        "Unedited original import is ready. SHA-256: {sha256}"
                    ));
                    focus_result("export-original-result");
                }
                _ => status.set("No original import is available for this recipe.".into()),
            }
        });
    };
    let description = Signal::derive(move || match source.get() {
        SourceChoice::Known(chosen) => format!(
            "Source: {}. Edited export writes the current native values; the unedited original import is separate.",
            chosen.label()
        ),
        SourceChoice::Choose { .. } => {
            "Your draft has changes that are not saved. Choose which source to export.".to_owned()
        }
    });
    view! {
        <crate::components::BreweryDialog
            id="export-task"
            title="Export BeerXML"
            description=description
            open=open
            wide=true
            return_focus_fallback="recipe-actions-trigger".to_owned()
            actions=std::sync::Arc::new(move || view! {
                <Button variant=ButtonVariant::Ghost on:click=move |_| open.set(false)>"Close"</Button>
                {move || match source.get() {
                    SourceChoice::Choose { saved, draft: draft_source } => view! {
                        <Button variant=ButtonVariant::Secondary disabled=busy on:click=move |_| prepare.run(saved)>{format!("Export {}", saved.label().to_lowercase())}</Button>
                        <Button disabled=busy on:click=move |_| prepare.run(draft_source)>"Export unsaved draft"</Button>
                    }.into_any(),
                    SourceChoice::Known(_) => ().into_any(),
                }}
            }.into_any())
        >
            <div class="space-y-4">
                <ul>{move || warnings.get().into_iter().map(|d| view! { <li>{d.message}</li> }).collect_view()}</ul>
                <Show when=move || pending.get().is_some()>
                    <Button id="export-acknowledge" on:click=move |_| { if let Some(mut request) = pending.get_untracked() { request.acknowledge_loss = true; run_export.run(request); } }>"Acknowledge omissions and prepare download"</Button>
                </Show>
                <div id="export-result" tabindex="-1">
                    {move || output.get().map(|href| view! { <crate::components::DownloadLink href=Signal::derive(move || href.clone()) filename="recipe.xml">"Download BeerXML"</crate::components::DownloadLink> })}
                </div>
                <Show when=move || has_original.get()>
                    <div class="k-field">
                        <p class="k-field__label">"Unedited original import"</p>
                        <p class="k-field__hint">"The file as it was imported, separate from this recipe's current values."</p>
                        <Button variant=ButtonVariant::Secondary disabled=busy on:click=fetch_original>"Prepare unedited original"</Button>
                        <div id="export-original-result" tabindex="-1">
                            {move || original.get().map(|href| view! { <crate::components::DownloadLink href=Signal::derive(move || href.clone()) filename="original-unedited.xml">"Download unedited original"</crate::components::DownloadLink> })}
                        </div>
                    </div>
                </Show>
                <p role="status">{move || status.get()}</p>
            </div>
        </crate::components::BreweryDialog>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krausen_types::{
        formulation::{Preview, PreviewOutcome, Targets},
        interchange::SourceEstimates,
        validation::{Readiness, Validation},
    };

    fn diagnostic(code: &str) -> krausen_types::interchange::MappingDiagnostic {
        krausen_types::interchange::MappingDiagnostic {
            code: code.into(),
            path: "p".into(),
            message: "m".into(),
        }
    }

    #[test]
    fn unsent_source_problems_are_tied_to_their_own_field() {
        assert_eq!(check_inputs("<R/>", "2"), Ok(1));
        assert_eq!(check_inputs(" \n", "1").unwrap_err().0, InputField::Source);
        let oversize = "x".repeat(krausen_types::interchange::MAX_BEERXML_BYTES + 1);
        assert_eq!(check_inputs(&oversize, "1").unwrap_err().0, InputField::Source);
        for bad in ["0", "", "x", "-1", "1.5"] {
            assert_eq!(check_inputs("<R/>", bad).unwrap_err().0, InputField::RecipeNumber, "{bad}");
        }
    }

    #[test]
    fn required_interpretations_are_attributed_to_their_own_select() {
        let both = [diagnostic("colour_interpretation_required"), diagnostic("efficiency_choice_required")];
        let (colour, efficiency) = choice_errors(&both);
        assert!(colour.is_some() && efficiency.is_some());
        let (colour, efficiency) = choice_errors(&both[..1]);
        assert!(colour.is_some() && efficiency.is_none());
        assert_eq!(choice_errors(&[diagnostic("other")]), (None, None));
    }

    #[test]
    fn only_a_returned_typed_outcome_is_a_known_import_result() {
        use krausen_types::interchange::ImportCommitOutcome as O;
        assert!(commit_is_unconfirmed(&Err(ServerFnError::new("lost"))));
        assert!(commit_is_unconfirmed(&Ok(O::Retryable)));
        assert!(commit_is_unconfirmed(&Ok(O::Unavailable)));
        for known in [O::InvalidSource, O::InvalidRequest, O::ProfileChanged, O::UnresolvedProfile] {
            assert!(!commit_is_unconfirmed(&Ok(known)));
        }
    }

    #[test]
    fn unconfirmed_copy_never_claims_nothing_was_created_or_cancelled() {
        for copy in [UNCONFIRMED_COPY, EXPIRED_COPY, CONFLICT_COPY] {
            let lower = copy.to_lowercase();
            for forbidden in ["did not complete", "not created", "nothing was", "cancelled", "canceled", "try again"] {
                assert!(!lower.contains(forbidden), "{copy}: {forbidden}");
            }
        }
        assert!(UNCONFIRMED_COPY.contains("may already have been created"));
    }

    #[test]
    fn guard_state_separates_unsent_edits_from_retained_unknown_work() {
        let draft = import_task_state(true, false, false, false, 3, true);
        assert!(draft.dirty && !draft.uncertain && draft.blocks());
        assert!(draft.may_discard(GuardTaskKind::Import));
        let sent = import_task_state(true, false, true, true, 3, true);
        assert!(!sent.dirty && sent.uncertain && sent.in_flight && sent.recovery_retained);
        assert!(!sent.may_discard(GuardTaskKind::Import), "an in-flight send cannot be abandoned");
        let unknown = import_task_state(true, false, true, false, 3, true);
        assert!(unknown.blocks() && unknown.may_discard(GuardTaskKind::Import));
        let done = import_task_state(true, true, false, false, 3, true);
        assert!(!done.blocks());
        assert!(!import_task_state(false, false, false, false, 0, true).blocks());
        assert!(!inputs_changed("", "1", "", "", ""));
        assert!(inputs_changed("", "2", "", "", ""));
        assert!(inputs_changed("", "1", "srm", "", ""));
    }

    #[test]
    fn retained_choices_round_trip_to_their_select_values() {
        assert_eq!(colour_value(Some(ColourUnit::Srm)), "srm");
        assert_eq!(colour_value(Some(ColourUnit::Lovibond)), "lovibond");
        assert_eq!(colour_value(None), "");
        assert_eq!(efficiency_value(Some(EfficiencyChoice::RecipeOverride)), "override");
        assert_eq!(efficiency_value(Some(EfficiencyChoice::SelectedEquipment)), "equipment");
        assert_eq!(efficiency_value(None), "");
    }

    #[test]
    fn import_preview_rejects_a_mismatched_generation() {
        let request = ImportRequest {
            intent: OperationIntent {
                account_id: "a".into(),
                workspace_id: "w".into(),
            },
            xml: String::new(),
            recipe_index: 0,
            choices: ImportChoices::default(),
            equipment_profile_id: None,
            expected_profile_revision: None,
            expected_profile_metadata_revision: None,
            equipment_draft: EquipmentSettingsV1::default(),
            draft_generation: "current".into(),
        };
        let preview = Preview {
            draft_generation: "old".into(),
            validation: Validation {
                state: Readiness::Invalid,
                diagnostics: vec![],
            },
            targets: Targets::default(),
            context_digest: None,
            context: None,
            style: None,
            style_comparison: None,
        };
        let mut result = ImportPreview {
            draft_generation: "current".into(),
            document: Default::default(),
            source_sha256: String::new(),
            selected_profile_revision: None,
            selected_profile_metadata_revision: None,
            source_estimates: SourceEstimates::default(),
            diagnostics: vec![],
            calculation: PreviewOutcome::Preview(preview),
        };
        assert!(!matches_import_context(&request, &result));
        if let PreviewOutcome::Preview(preview) = &mut result.calculation {
            preview.draft_generation = "current".into();
        }
        assert!(matches_import_context(&request, &result));
        result.draft_generation = "old".into();
        assert!(!matches_import_context(&request, &result));
    }
}
