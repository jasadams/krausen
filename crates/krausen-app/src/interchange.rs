// SPDX-License-Identifier: AGPL-3.0-or-later
//! Authorized BeerXML operations. Source bytes and native revision are committed together.
use crate::{
    ActorContext, Error, RequestIdentity, authorize_workspace, boundary_error, check_intent,
    db_error, record_change, record_request, recover_request,
};
use krausen_core::{DbPool, DbTx, canonical_json};
use krausen_types::interchange::{
    ExportOutcome, ExportRequest, ExportSelection, ImportCommitOutcome, ImportPreview,
    ImportPreviewOutcome, ImportRequest, OriginalOutcome,
};

fn project(request: &ImportRequest) -> Option<krausen_beerxml::native::NativeMigration> {
    if request.xml.len() > krausen_types::interchange::MAX_BEERXML_BYTES {
        return None;
    }
    let mut result =
        krausen_beerxml::native::from_xml_at(&request.xml, &request.choices, request.recipe_index)
            .ok()??;
    result.document.equipment_profile_id = request.equipment_profile_id.clone();
    if request.equipment_profile_id.is_none() {
        let efficiency = result
            .document
            .equipment_overrides
            .efficiency_override_fraction;
        result.document.equipment_overrides = request.equipment_draft.clone();
        if efficiency.is_some() {
            result
                .document
                .equipment_overrides
                .efficiency_override_fraction = efficiency;
        }
    }
    if request.choices.efficiency
        == Some(krausen_types::interchange::EfficiencyChoice::SelectedEquipment)
        && result
            .legacy_record
            .get("header")
            .and_then(|v| v.get("efficiency"))
            .is_some_and(|v| !v.is_null())
        && request.equipment_profile_id.is_none()
        && request.equipment_draft.mash_efficiency_fraction.is_none()
        && request
            .equipment_draft
            .efficiency_override_fraction
            .is_none()
    {
        result
            .warnings
            .push(krausen_types::interchange::MappingDiagnostic {
                code: "equipment_efficiency_required".into(),
                path: "equipment".into(),
                message:
                    "Choose equipment with an efficiency or retain the source as a recipe override"
                        .into(),
            });
    }
    Some(result)
}

pub async fn preview(
    pool: &DbPool,
    actor: &ActorContext,
    request: &ImportRequest,
) -> ImportPreviewOutcome {
    match preview_inner(pool, actor, request).await {
        Ok(result) => result,
        Err(Error::Forbidden) => ImportPreviewOutcome::Forbidden,
        Err(Error::ContextChanged) => ImportPreviewOutcome::ContextChanged,
        Err(e) => {
            tracing::error!(error = %e, "BeerXML preview failed");
            ImportPreviewOutcome::Unavailable
        }
    }
}

async fn preview_inner(
    pool: &DbPool,
    actor: &ActorContext,
    request: &ImportRequest,
) -> Result<ImportPreviewOutcome, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, &request.intent)?;
    let profile = if let Some(profile_id) = &request.equipment_profile_id {
        let profile =
            krausen_brewery::equipment_store::load(&mut tx, &actor.workspace_id, profile_id, true)
                .await
                .map_err(boundary_error)?;
        authorize_workspace(&mut tx, actor).await?;
        profile.filter(|profile| !profile.archived)
    } else {
        None
    };
    tx.commit().await.map_err(boundary_error)?;
    let Some(imported) = project(request) else {
        return Ok(ImportPreviewOutcome::InvalidSource);
    };
    let calculation = crate::formulation::preview(
        pool,
        actor,
        krausen_types::formulation::PreviewRequest {
            intent: request.intent.clone(),
            draft_generation: request.draft_generation.clone(),
            draft: imported.document.clone(),
            equipment_draft: request.equipment_draft.clone(),
        },
    )
    .await?;
    let preview_profile = match &calculation {
        krausen_types::formulation::PreviewOutcome::Preview(preview) => preview
            .context
            .as_ref()
            .and_then(|context| context.equipment_profile.as_ref()),
        _ => None,
    }
    .or(profile.as_ref());
    Ok(ImportPreviewOutcome::Preview(ImportPreview {
        draft_generation: request.draft_generation.clone(),
        document: imported.document,
        source_sha256: imported
            .original_source
            .as_ref()
            .ok_or(Error::Invalid("missing original source"))?
            .sha256
            .clone(),
        selected_profile_revision: preview_profile.map(|profile| profile.revision),
        selected_profile_metadata_revision: preview_profile
            .map(|profile| profile.metadata_revision),
        source_estimates: imported.source_estimates,
        diagnostics: imported.warnings,
        calculation,
    }))
}

pub async fn commit(
    pool: &DbPool,
    actor: &ActorContext,
    request: &ImportRequest,
    request_id: uuid::Uuid,
) -> ImportCommitOutcome {
    match commit_inner(pool, actor, request, request_id).await {
        Ok(result) => result,
        Err(Error::Forbidden) => ImportCommitOutcome::Forbidden,
        Err(Error::ContextChanged) => ImportCommitOutcome::ContextChanged,
        Err(Error::RequestIdentityConflict) => ImportCommitOutcome::RequestIdentityConflict,
        Err(Error::ResultExpired { resource_id }) => {
            ImportCommitOutcome::ResultExpired { resource_id }
        }
        Err(Error::Retryable) => ImportCommitOutcome::Retryable,
        Err(e) => {
            tracing::error!(error = %e, "BeerXML commit failed");
            ImportCommitOutcome::Unavailable
        }
    }
}

async fn commit_inner(
    pool: &DbPool,
    actor: &ActorContext,
    request: &ImportRequest,
    request_id: uuid::Uuid,
) -> Result<ImportCommitOutcome, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, &request.intent)?;
    let sql = if matches!(&tx, DbTx::Postgres(_)) {
        "SELECT revision FROM workspaces WHERE workspace_id = $1 AND status = 'active' FOR UPDATE"
    } else {
        "SELECT revision FROM workspaces WHERE workspace_id = $1 AND status = 'active'"
    };
    let workspace = scalar!(&mut tx, i64, sql, &actor.workspace_id).map_err(db_error)?;
    authorize_workspace(&mut tx, actor).await?;
    if workspace.is_none() {
        return Err(Error::Forbidden);
    }
    if request.xml.len() > krausen_types::interchange::MAX_BEERXML_BYTES {
        return Ok(ImportCommitOutcome::InvalidSource);
    }
    let value =
        canonical_json::from_serializable(request).map_err(|_| Error::Invalid("import request"))?;
    let digest = canonical_json::sha256(&value).map_err(|_| Error::Invalid("import request"))?;
    let request_id = request_id.to_string();
    let key = RequestIdentity {
        actor,
        operation: "beerxml.import.v1",
        request_id: &request_id,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        return Ok(serde_json::from_value(result)?);
    }
    let Some(imported) = project(request) else {
        return Ok(ImportCommitOutcome::InvalidSource);
    };
    let pending: Vec<_> = imported
        .warnings
        .iter()
        .filter(|w| {
            matches!(
                w.code.as_str(),
                "colour_interpretation_required"
                    | "efficiency_choice_required"
                    | "equipment_efficiency_required"
            )
        })
        .cloned()
        .collect();
    if !pending.is_empty() {
        return Ok(ImportCommitOutcome::ChoicesRequired(pending));
    }
    if let Some(profile_id) = &imported.document.equipment_profile_id {
        let profile =
            krausen_brewery::equipment_store::load(&mut tx, &actor.workspace_id, profile_id, true)
                .await
                .map_err(boundary_error)?;
        authorize_workspace(&mut tx, actor).await?;
        let Some(profile) = profile.filter(|p| !p.archived) else {
            return Ok(ImportCommitOutcome::UnresolvedProfile);
        };
        if request.expected_profile_revision != Some(profile.revision)
            || request.expected_profile_metadata_revision != Some(profile.metadata_revision)
        {
            return Ok(ImportCommitOutcome::ProfileChanged);
        }
    }
    let validation = krausen_brewery::validation::validate_draft(
        &imported.document,
        &imported.document.equipment_overrides,
    );
    if !validation.saveable() {
        return Ok(ImportCommitOutcome::Invalid(validation));
    }
    let recipe_id = uuid::Uuid::new_v4().to_string();
    let diagnostics = imported.warnings.clone();
    let source_sha256 = imported
        .original_source
        .as_ref()
        .ok_or(Error::Invalid("missing original source"))?
        .sha256
        .clone();
    krausen_brewery::native_store::create_imported(
        &mut tx,
        &actor.workspace_id,
        &recipe_id,
        imported,
        &actor.actor_id,
        chrono::Utc::now(),
    )
    .await
    .map_err(boundary_error)?;
    let result = ImportCommitOutcome::Imported {
        recipe_id: recipe_id.clone(),
        revision: 1,
        source_sha256,
        diagnostics,
    };
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&result)?,
        Some(&recipe_id),
    )
    .await?;
    record_change(&mut tx, actor, "beerxml.import.v1", &recipe_id, 1).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(result)
}

/// Export one saved revision or an explicitly identified local draft.
pub async fn export(pool: &DbPool, actor: &ActorContext, request: &ExportRequest) -> ExportOutcome {
    match export_inner(pool, actor, request).await {
        Ok(result) => result,
        Err(Error::Forbidden) => ExportOutcome::Forbidden,
        Err(Error::ContextChanged) => ExportOutcome::ContextChanged,
        Err(e) => {
            tracing::error!(error = %e, "BeerXML export failed");
            ExportOutcome::Unavailable
        }
    }
}

async fn export_inner(
    pool: &DbPool,
    actor: &ActorContext,
    request: &ExportRequest,
) -> Result<ExportOutcome, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, &request.intent)?;
    let (document, unsaved_draft, source_recipe) = match &request.selection {
        ExportSelection::Saved {
            recipe_id,
            revision,
        } => {
            if *revision < 1 {
                return Ok(ExportOutcome::NotFound);
            }
            let document = krausen_brewery::native_store::load_document(
                &mut tx,
                &actor.workspace_id,
                recipe_id,
                *revision,
            )
            .await
            .map_err(|_| Error::Invalid("recipe revision"))?;
            let Some(document) = document else {
                return Ok(ExportOutcome::NotFound);
            };
            (document, false, Some(recipe_id.as_str()))
        }
        ExportSelection::Draft {
            recipe_id,
            document,
        } => (document.clone(), true, recipe_id.as_deref()),
    };
    let validation =
        krausen_brewery::validation::validate_draft(&document, &document.equipment_overrides);
    if !validation.saveable() {
        return Ok(ExportOutcome::Invalid);
    }
    let mut output = krausen_beerxml::native::to_xml(&document);
    if let (Some(recipe_id), Some(provenance_id)) =
        (source_recipe, document.import_provenance_id.as_ref())
    {
        if let Some(provenance) = krausen_brewery::native_store::load_provenance(
            &mut tx,
            &actor.workspace_id,
            recipe_id,
            1,
        )
        .await
        .map_err(boundary_error)?
        {
            if &provenance.provenance_id == provenance_id {
                let mapping_matches =
                    provenance.mapping_version == krausen_beerxml::native::MAPPING_VERSION;
                if let (Some(bytes), Some(sha256), Some(record)) = (
                    provenance.original_bytes,
                    provenance.original_hash,
                    provenance.source_record,
                ) {
                    let source = krausen_beerxml::native::OriginalSource { bytes, sha256 };
                    let metadata: serde_json::Value = serde_json::from_str(&record)?;
                    let choices = metadata
                        .get("choices")
                        .cloned()
                        .and_then(|v| serde_json::from_value(v).ok());
                    let index = metadata
                        .get("source_recipe_index")
                        .and_then(|v| v.as_u64())
                        .and_then(|v| usize::try_from(v).ok());
                    if source.has_valid_hash() {
                        if let (true, Ok(xml), Some(index), Some(choices)) = (
                            mapping_matches,
                            std::str::from_utf8(&source.bytes),
                            index,
                            choices,
                        ) {
                            output = krausen_beerxml::native::to_xml_with_source(
                                &document, xml, index, &choices,
                            );
                        }
                    } else {
                        return Err(Error::Invalid("original source checksum"));
                    }
                }
            } else {
                return Ok(ExportOutcome::Invalid);
            }
        } else {
            return Ok(ExportOutcome::NotFound);
        }
    }
    if !krausen_beerxml::native::valid_xml_characters(&output.xml) {
        return Ok(ExportOutcome::Invalid);
    }
    tx.commit().await.map_err(boundary_error)?;
    if !output.diagnostics.is_empty() && !request.acknowledge_loss {
        return Ok(ExportOutcome::LossAcknowledgementRequired(
            output.diagnostics,
        ));
    }
    Ok(ExportOutcome::Exported {
        xml: output.xml,
        diagnostics: output.diagnostics,
        unsaved_draft,
    })
}

/// Recover the unedited import attachment only from the current workspace.
pub async fn original(
    pool: &DbPool,
    actor: &ActorContext,
    intent: &krausen_types::request_context::OperationIntent,
    recipe_id: &str,
) -> OriginalOutcome {
    match original_inner(pool, actor, intent, recipe_id).await {
        Ok(result) => result,
        Err(Error::Forbidden) => OriginalOutcome::Forbidden,
        Err(Error::ContextChanged) => OriginalOutcome::ContextChanged,
        Err(e) => {
            tracing::error!(error = %e, "BeerXML original retrieval failed");
            OriginalOutcome::Unavailable
        }
    }
}

async fn original_inner(
    pool: &DbPool,
    actor: &ActorContext,
    intent: &krausen_types::request_context::OperationIntent,
    recipe_id: &str,
) -> Result<OriginalOutcome, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, intent)?;
    let provenance =
        krausen_brewery::native_store::load_provenance(&mut tx, &actor.workspace_id, recipe_id, 1)
            .await
            .map_err(boundary_error)?;
    let Some((bytes, sha256)) = provenance.and_then(|p| p.original_bytes.zip(p.original_hash))
    else {
        return Ok(OriginalOutcome::NotFound);
    };
    let source = krausen_beerxml::native::OriginalSource { bytes, sha256 };
    if !source.has_valid_hash() {
        return Err(Error::Invalid("original source checksum"));
    }
    tx.commit().await.map_err(boundary_error)?;
    Ok(OriginalOutcome::Original {
        bytes: source.bytes,
        sha256: source.sha256,
    })
}
