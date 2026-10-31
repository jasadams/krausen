// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit BeerXML import preview and commit.
use crate::{
    components::{
        BreweryDraftArea, BreweryDraftField, BreweryDraftSelect, BreweryNotice, BreweryPanel,
        Button, ButtonLink,
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
                            None => {
                                context.equipment_profile.is_none()
                                    && context.equipment_draft == request.equipment_draft
                            }
                        }
                })
        }
        PreviewOutcome::UnresolvedProfile | PreviewOutcome::UnresolvedStyle => true,
        _ => false,
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
    let pending = RwSignal::new(None::<(ImportRequest, String)>);
    let status = RwSignal::new(String::new());
    let imported_id = RwSignal::new(None::<String>);
    let import_diagnostics =
        RwSignal::new(Vec::<krausen_types::interchange::MappingDiagnostic>::new());
    let visible = RwSignal::new(true);
    let busy = RwSignal::new(false);
    let sequence = RwSignal::new(0_u64);
    let invalidate = Callback::new(move |()| {
        sequence.update(|n| *n = n.checked_add(1).expect("import generation exhausted"));
        preview.set(None);
        pending.set(None);
        imported_id.set(None);
        import_diagnostics.set(Vec::new());
        status.set(String::new());
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
    let preview_action = move |_| {
        if busy.get_untracked() {
            return;
        }
        if source.with_untracked(|value| value.len())
            > krausen_types::interchange::MAX_BEERXML_BYTES
        {
            status.set("The source is larger than 2 MiB.".into());
            return;
        }
        let Some(generation) = crate::utils::request_id::new_request_id() else {
            status.set("Unable to prepare a preview.".into());
            return;
        };
        let request = ImportRequest {
            intent: identity.get_value(),
            xml: source.get_untracked(),
            recipe_index: match recipe_number
                .get_untracked()
                .parse::<usize>()
                .ok()
                .and_then(|n| n.checked_sub(1))
            {
                Some(index) => index,
                None => {
                    status.set("Enter a recipe number starting at 1.".into());
                    return;
                }
            },
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
            if sequence.try_get_untracked() != Some(current) {
                return;
            }
            match outcome {
                Ok(ImportPreviewOutcome::Preview(result)) => {
                    if !matches_import_context(&request, &result) {
                        status.set("Preview context changed. Try again.".into());
                        return;
                    }
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
                _ => status
                    .set("Unable to preview this BeerXML. Check the source and try again.".into()),
            }
        });
    };
    let commit_action = move |_| {
        if busy.get_untracked() {
            return;
        }
        let Some((request, _)) = preview.get_untracked() else {
            return;
        };
        let attempt = match pending.get_untracked() {
            Some(attempt) => attempt,
            None => {
                let Some(id) = crate::utils::request_id::new_request_id() else {
                    status.set("Unable to prepare import.".into());
                    return;
                };
                let attempt = (request, id);
                pending.set(Some(attempt.clone()));
                attempt
            }
        };
        busy.set(true);
        leptos::task::spawn_local(async move {
            if !current_context_matches(&attempt.0.intent).await {
                visible.set(false);
                busy.set(false);
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
                visible.set(false);
                busy.set(false);
                return;
            }
            busy.set(false);
            match outcome {
                Ok(ImportCommitOutcome::Imported {
                    recipe_id,
                    diagnostics,
                    ..
                }) => {
                    pending.set(None);
                    preview.set(None);
                    imported_id.set(Some(recipe_id));
                    import_diagnostics.set(diagnostics);
                    status.set("Recipe imported.".into());
                }
                Ok(ImportCommitOutcome::ChoicesRequired(_)) => {
                    pending.set(None);
                    status.set(
                        "Choose how to interpret source colour and efficiency, then preview again."
                            .into(),
                    );
                }
                Ok(ImportCommitOutcome::Invalid(_)) => {
                    pending.set(None);
                    status.set(
                        "The source contains invalid values that must be resolved before import."
                            .into(),
                    );
                }
                Ok(ImportCommitOutcome::ProfileChanged) => {
                    pending.set(None);
                    preview.set(None);
                    status.set(
                        "The equipment profile changed. Preview again before importing.".into(),
                    );
                }
                Ok(ImportCommitOutcome::UnresolvedProfile) => {
                    pending.set(None);
                    preview.set(None);
                    status.set("The selected equipment is unavailable. Choose equipment and preview again.".into());
                }
                Ok(
                    ImportCommitOutcome::InvalidSource
                    | ImportCommitOutcome::InvalidRequest
                    | ImportCommitOutcome::RequestIdentityConflict,
                ) => {
                    pending.set(None);
                    preview.set(None);
                    status.set(
                        "This import request is invalid. Review the source and preview again."
                            .into(),
                    );
                }
                Ok(ImportCommitOutcome::Forbidden | ImportCommitOutcome::ContextChanged) => {
                    visible.set(false);
                }
                Ok(ImportCommitOutcome::ResultExpired { .. }) => {
                    status.set("The import result expired. Check your recipe library before making another import.".into());
                }
                _ => {
                    status.set("Import did not complete. Retry to recover the same request.".into())
                }
            }
        });
    };
    view! { <Show when=move || visible.get() fallback=|| view! { <BreweryNotice title="Account or workspace changed" body="Return to the original context to continue this import." warning=true/> }>
        <BreweryPanel title="Import BeerXML" description="Preview before creating a recipe. The original file is retained separately.">
            <fieldset disabled=move || pending.get().is_some()>
            <label for="beerxml-file">"Choose a UTF-8 BeerXML file"</label>
            <input id="beerxml-file" type="file" class=crate::components::INPUT_CLASS accept=".xml,.beerxml,text/xml,application/xml" on:change=move |event| {
                #[cfg(target_arch = "wasm32")]
                {
                    use wasm_bindgen::JsCast;
                    if pending.get_untracked().is_some() { return; }
                    let file = event.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok()).and_then(|i| i.files()).and_then(|f| f.get(0));
                    if let Some(file) = file {
                        if file.size() > krausen_types::interchange::MAX_BEERXML_BYTES as f64 { status.set("The file is larger than 2 MiB.".into()); return; }
                        invalidate.run(());
                        let generation = sequence.get_untracked();
                        leptos::task::spawn_local(async move {
                            match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                                Ok(buffer) => match String::from_utf8(js_sys::Uint8Array::new(&buffer).to_vec()) {
                                    Ok(text) if sequence.try_get_untracked() == Some(generation) && pending.get_untracked().is_none() => { source.set(text); invalidate.run(()); }
                                    Ok(_) => {}
                                    Err(_) => status.set("The file must be UTF-8.".into()),
                                },
                                Err(_) => status.set("Unable to read the selected file.".into()),
                            }
                        });
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                let _ = event;
            }/>
            <BreweryDraftArea id="beerxml-source" label="Paste BeerXML" value=Signal::from(source) on_change=Callback::new(move |v| { if pending.get_untracked().is_none() { source.set(v); invalidate.run(()); } })/>
            <BreweryDraftField id="beerxml-recipe-number" label="Recipe number in file" value=Signal::from(recipe_number) numeric=true on_change=Callback::new(move |v| { if pending.get_untracked().is_none() { recipe_number.set(v); invalidate.run(()); } })/>
            <BreweryDraftSelect id="beerxml-colour" label="Source colour" value=Signal::from(colour) on_change=Callback::new(move |v| { if pending.get_untracked().is_none() { colour.set(v); invalidate.run(()); } }) options=vec![("".into(), "Choose when present".into()), ("srm".into(), "SRM".into()), ("lovibond".into(), "Lovibond".into())]/>
            <BreweryDraftSelect id="beerxml-efficiency" label="Source efficiency" value=Signal::from(efficiency) on_change=Callback::new(move |v| { if pending.get_untracked().is_none() { efficiency.set(v); invalidate.run(()); } }) options=vec![("".into(), "Choose when present".into()), ("override".into(), "Use as recipe override".into()), ("equipment".into(), "Use selected equipment".into())]/>
            {move || { let mut options = vec![(String::new(), "No equipment profile".to_string())]; options.extend(profiles.get().into_iter().map(|p| (p.profile_id, p.name))); view! {
                <BreweryDraftSelect id="beerxml-equipment" label="Equipment profile" value=Signal::from(selected_profile) on_change=Callback::new(move |v| { if pending.get_untracked().is_none() { selected_profile.set(v); invalidate.run(()); } }) options=options/>
            } }}
            <Show when=move || profile_cursor.get().is_some()><Button on:click=move |_| { if !profile_loading.get_untracked() { load_profiles.run(profile_cursor.get_untracked()); } }>"Load more equipment profiles"</Button></Show>
            </fieldset>
            <Button on:click=preview_action>"Preview"</Button>
            {move || preview.get().map(|(_, result)| view! {
                <section aria-label="Import preview"><h3>{result.document.title.clone()}</h3>
                    <p>{format!("Original source SHA-256: {}", result.source_sha256)}</p>
                    <p>{format!("{} fermentables, {} hop additions", result.document.fermentables.len(), result.document.hops.len())}</p>
                    <p>{format!("Source estimates (not calculated by Krausen): OG {}, FG {}, bitterness {}, colour {}, alcohol {}", result.source_estimates.original_gravity.unwrap_or_else(|| "not supplied".into()), result.source_estimates.final_gravity.unwrap_or_else(|| "not supplied".into()), result.source_estimates.bitterness.unwrap_or_else(|| "not supplied".into()), result.source_estimates.colour.unwrap_or_else(|| "not supplied".into()), result.source_estimates.alcohol.unwrap_or_else(|| "not supplied".into()))}</p>
                    <ul>{result.diagnostics.into_iter().map(|d| view! { <li>{format!("{}: {}", d.path, d.message)}</li> }).collect_view()}</ul>
                    <p>{calculation_summary(result.calculation)}</p>
                </section>
            })}
            <Show when=move || preview.get().is_some()><Button on:click=commit_action>"Import recipe"</Button></Show>
            <p role="status">{move || status.get()}</p>
            <ul>{move || import_diagnostics.get().into_iter().map(|d| view! { <li>{d.message}</li> }).collect_view()}</ul>
            {move || imported_id.get().map(|id| view! { <ButtonLink href=format!("/recipe-editor/{id}")>"Open imported recipe"</ButtonLink> })}
            <ButtonLink href="/recipes">"Cancel"</ButtonLink>
        </BreweryPanel>
    </Show> }
}

/// Export a saved revision or the explicitly labelled current local draft.
#[component]
pub(super) fn BeerXmlExport(
    identity: RwSignal<OperationIntent>,
    context_visible: RwSignal<bool>,
    recipe_id: RwSignal<Option<String>>,
    revision: RwSignal<Option<i64>>,
    draft: RwSignal<crate::pages::recipe_draft::RecipeDraft>,
) -> impl IntoView {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use krausen_types::interchange::{
        ExportOutcome, ExportRequest, ExportSelection, OriginalOutcome,
    };
    let revision_text = RwSignal::new(
        revision
            .get_untracked()
            .map(|n| n.to_string())
            .unwrap_or_default(),
    );
    Effect::new(move |_| {
        if let Some(current) = revision.get() {
            if revision_text.get_untracked().is_empty() {
                revision_text.set(current.to_string());
            }
        }
    });
    let output = RwSignal::new(None::<String>);
    let original = RwSignal::new(None::<String>);
    let warnings = RwSignal::new(Vec::<krausen_types::interchange::MappingDiagnostic>::new());
    let pending = RwSignal::new(None::<ExportRequest>);
    let draft_export_generation = RwSignal::new(None::<u64>);
    let export_sequence = RwSignal::new(0_u64);
    let status = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    Effect::new(move |_| {
        if !context_visible.get() {
            export_sequence.update(|n| *n = n.checked_add(1).expect("export sequence exhausted"));
            output.set(None);
            original.set(None);
            pending.set(None);
            warnings.set(Vec::new());
            draft_export_generation.set(None);
        }
    });
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
            status.set("The draft changed. Prepare the export again.".into());
        }
    });
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
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                busy.set(false);
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
                busy.set(false);
                return;
            }
            busy.set(false);
            if export_sequence.try_get_untracked() != Some(sequence) {
                return;
            }
            if expected_generation
                .is_some_and(|generation| draft.with_untracked(|d| d.generation) != generation)
            {
                status.set("The draft changed. Prepare the export again.".into());
                return;
            }
            match result {
                Ok(ExportOutcome::LossAcknowledgementRequired(diagnostics)) => {
                    warnings.set(diagnostics);
                    pending.set(Some(request));
                    status.set("Review the omitted information before downloading.".into());
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
                }
                _ => {
                    output.set(None);
                    status.set("Unable to export this revision or draft.".into());
                }
            }
        });
    });
    let saved = move |_| {
        let (Some(id), Ok(number)) = (
            recipe_id.get_untracked(),
            revision_text.get_untracked().parse::<i64>(),
        ) else {
            status.set("Enter an available saved revision.".into());
            return;
        };
        output.set(None);
        warnings.set(Vec::new());
        pending.set(None);
        draft_export_generation.set(None);
        run_export.run(ExportRequest {
            intent: identity.get_untracked(),
            selection: ExportSelection::Saved {
                recipe_id: id,
                revision: number,
            },
            acknowledge_loss: false,
        });
    };
    let unsaved = move |_| {
        if !draft.with_untracked(|d| d.unfinished_inputs().is_empty()) {
            status.set("Complete unfinished fields before exporting the draft.".into());
            return;
        }
        output.set(None);
        warnings.set(Vec::new());
        pending.set(None);
        draft_export_generation.set(Some(draft.with_untracked(|d| d.generation)));
        run_export.run(ExportRequest {
            intent: identity.get_untracked(),
            selection: ExportSelection::Draft {
                recipe_id: recipe_id.get_untracked(),
                document: draft.with_untracked(|d| d.document().clone()),
            },
            acknowledge_loss: false,
        });
    };
    let fetch_original = move |_| {
        if busy.get_untracked() {
            return;
        }
        let Some(id) = recipe_id.get_untracked() else {
            return;
        };
        let expected = identity.get_untracked();
        busy.set(true);
        original.set(None);
        leptos::task::spawn_local(async move {
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                busy.set(false);
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
                busy.set(false);
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
                }
                _ => status.set("No original import is available for this recipe.".into()),
            }
        });
    };
    view! { <BreweryPanel title="BeerXML export" description="Export a saved revision or this unsaved draft. The unedited original is separate.">
        <crate::components::BreweryDraftField id="beerxml-export-revision" label="Saved revision" numeric=true value=Signal::from(revision_text) on_change=Callback::new(move |v| { export_sequence.update(|n| *n = n.checked_add(1).expect("export sequence exhausted")); revision_text.set(v); output.set(None); pending.set(None); warnings.set(Vec::new()); })/>
        <Button on:click=saved>"Prepare saved revision"</Button>
        <Button on:click=unsaved>"Prepare unsaved draft"</Button>
        <Show when=move || recipe_id.get().is_some()><Button on:click=fetch_original>"Prepare unedited original"</Button></Show>
        <ul>{move || warnings.get().into_iter().map(|d| view! { <li>{d.message}</li> }).collect_view()}</ul>
        <Show when=move || pending.get().is_some()><Button on:click=move |_| { if let Some(mut request) = pending.get_untracked() { request.acknowledge_loss = true; run_export.run(request); } }>"Acknowledge omissions and prepare download"</Button></Show>
        {move || output.get().map(|href| view! { <crate::components::DownloadLink href=Signal::derive(move || href.clone()) filename="recipe.xml">"Download BeerXML"</crate::components::DownloadLink> })}
        {move || original.get().map(|href| view! { <crate::components::DownloadLink href=Signal::derive(move || href.clone()) filename="original-unedited.xml">"Download unedited original"</crate::components::DownloadLink> })}
        <p role="status">{move || status.get()}</p>
    </BreweryPanel> }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krausen_types::{
        formulation::{Preview, PreviewOutcome, Targets},
        interchange::SourceEstimates,
        validation::{Readiness, Validation},
    };

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
