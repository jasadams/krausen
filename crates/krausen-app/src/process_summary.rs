// SPDX-License-Identifier: AGPL-3.0-or-later
//! Authorized structured recipe summary for printing and export.
//!
//! The summary is derived from exactly one saved native revision or one unsaved
//! native draft. It reuses the shared formulation resolver for equipment/style
//! context and never persists a draft or creates recipe history.
use crate::{ActorContext, Error, authorize_workspace, boundary_error, check_intent, db_error};
use krausen_brewery::native_store::{self, StoredReadError};
use krausen_core::{DbPool, DbTx};
use krausen_types::{
    brewery::RecipeDocumentV1,
    formulation::{Preview, PreviewOutcome, PreviewRequest},
    process_summary::{
        IngredientSection, ProcessSummary, ProcessSummaryOutcome, ProcessSummaryRequest,
        ProcessSummarySelection, SummaryContext, SummaryEquipment, SummaryIngredient,
        SummaryProcess, SummarySource, SummaryStep,
    },
    request_context::OperationIntent,
    validation::Validation,
};

pub async fn read(
    pool: &DbPool,
    actor: &ActorContext,
    request: ProcessSummaryRequest,
) -> ProcessSummaryOutcome {
    match read_inner(pool, actor, request).await {
        Ok(outcome) => outcome,
        Err(Error::Forbidden) => ProcessSummaryOutcome::Forbidden,
        Err(Error::ContextChanged) => ProcessSummaryOutcome::ContextChanged,
        Err(Error::Invalid(_)) => ProcessSummaryOutcome::InvalidRequest,
        Err(error) => {
            tracing::warn!(%error, "recipe process summary unavailable");
            ProcessSummaryOutcome::Unavailable
        }
    }
}

async fn read_inner(
    pool: &DbPool,
    actor: &ActorContext,
    request: ProcessSummaryRequest,
) -> Result<ProcessSummaryOutcome, Error> {
    let intent = request.intent;
    let (source, document) = match request.selection {
        ProcessSummarySelection::Saved {
            recipe_id,
            revision,
        } => {
            if revision <= 0 {
                return Ok(ProcessSummaryOutcome::InvalidRequest);
            }
            match load_saved(pool, actor, &intent, &recipe_id, revision).await? {
                SavedRead::Found(document) => (
                    SummarySource::Saved {
                        recipe_id,
                        revision,
                    },
                    document,
                ),
                SavedRead::Missing => return Ok(ProcessSummaryOutcome::NotFound),
                SavedRead::UnsupportedVersion(schema_version) => {
                    return Ok(ProcessSummaryOutcome::UnsupportedRevision { schema_version });
                }
            }
        }
        ProcessSummarySelection::Draft {
            recipe_id,
            base_revision,
            document,
        } => {
            if document.schema_version != 1 {
                return Ok(ProcessSummaryOutcome::InvalidRequest);
            }
            // A caller-supplied source identity is a trusted label only after
            // it has been scoped to this workspace and parent. New drafts may
            // omit it; partial or non-positive pairs are impossible.
            let (recipe_id, base_revision) = match (recipe_id, base_revision) {
                (None, None) => (None, None),
                (Some(recipe_id), Some(revision)) if revision > 0 => {
                    if !base_revision_exists(pool, actor, &intent, &recipe_id, revision).await? {
                        return Ok(ProcessSummaryOutcome::NotFound);
                    }
                    (Some(recipe_id), Some(revision))
                }
                _ => return Ok(ProcessSummaryOutcome::InvalidRequest),
            };
            (
                SummarySource::UnsavedDraft {
                    recipe_id,
                    base_revision,
                },
                document,
            )
        }
    };
    // Deliberately call the existing shared preview instead of extracting an
    // in-transaction helper: formulation.rs is part of its own self-hashed
    // policy source, so editing it would change every calculation-context
    // digest for a no-op refactor. The selected revision is immutable and
    // preview performs current authorization, intent checking and the
    // post-profile-lock recheck.
    let preview = crate::formulation::preview(
        pool,
        actor,
        PreviewRequest {
            draft_generation: "process-summary".into(),
            intent,
            draft: document.clone(),
            equipment_draft: Default::default(),
        },
    )
    .await?;
    let preview = match preview {
        PreviewOutcome::Preview(preview) => preview,
        PreviewOutcome::UnresolvedProfile => {
            return Ok(ProcessSummaryOutcome::UnresolvedEquipmentProfile);
        }
        PreviewOutcome::UnresolvedStyle => return Ok(ProcessSummaryOutcome::UnresolvedStyle),
        PreviewOutcome::Forbidden => return Ok(ProcessSummaryOutcome::Forbidden),
        PreviewOutcome::ContextChanged => return Ok(ProcessSummaryOutcome::ContextChanged),
        PreviewOutcome::Invalid => return Ok(ProcessSummaryOutcome::InvalidRequest),
        PreviewOutcome::Unavailable => return Ok(ProcessSummaryOutcome::Unavailable),
    };
    let Preview {
        validation,
        targets,
        context_digest,
        context,
        style,
        style_comparison,
        ..
    } = preview;
    // The formulation resolver always produces an encoded context for an Invalid
    // draft as well. Without one, no resolved equipment or context can be claimed.
    let Some(context) = context else {
        return Ok(ProcessSummaryOutcome::Invalid(validation));
    };
    let summary = project(ResolvedSummary {
        source,
        document: &document,
        validation,
        targets,
        style,
        style_comparison,
        context_digest,
        context: &context,
    });
    Ok(ProcessSummaryOutcome::Summary(summary))
}

/// Verify a caller-declared draft base revision exists under the current
/// workspace and recipe. Historical bases are valid; no head check is applied.
async fn base_revision_exists(
    pool: &DbPool,
    actor: &ActorContext,
    intent: &OperationIntent,
    recipe_id: &str,
    revision: i64,
) -> Result<bool, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, intent)?;
    let count = scalar!(
        &mut tx,
        i64,
        "SELECT COUNT(*) FROM recipe_revisions WHERE workspace_id = $1 AND recipe_id = $2 AND revision = $3",
        &actor.workspace_id,
        recipe_id,
        revision
    )
    .map_err(db_error)?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(count.unwrap_or(0) > 0)
}

enum SavedRead {
    Found(RecipeDocumentV1),
    Missing,
    UnsupportedVersion(i32),
}

/// Read one explicitly selected immutable revision in the current workspace.
/// Archived recipes remain readable for explicit export/print; a missing or
/// foreign revision is reported as absent without disclosing another workspace.
async fn load_saved(
    pool: &DbPool,
    actor: &ActorContext,
    intent: &OperationIntent,
    recipe_id: &str,
    revision: i64,
) -> Result<SavedRead, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, intent)?;
    let exists = scalar!(
        &mut tx,
        i64,
        "SELECT COUNT(*) FROM recipes WHERE workspace_id = $1 AND recipe_id = $2 AND native_head_revision > 0",
        &actor.workspace_id,
        recipe_id
    )
    .map_err(db_error)?;
    if exists.unwrap_or(0) == 0 {
        tx.commit().await.map_err(boundary_error)?;
        return Ok(SavedRead::Missing);
    }
    let document = match native_store::load_document(
        &mut tx,
        &actor.workspace_id,
        recipe_id,
        revision,
    )
    .await
    {
        Ok(Some(document)) => SavedRead::Found(document),
        Ok(None) => SavedRead::Missing,
        Err(StoredReadError::UnsupportedVersion(schema_version)) => {
            SavedRead::UnsupportedVersion(schema_version)
        }
        Err(StoredReadError::Storage(error)) => return Err(Error::Database(error)),
        Err(StoredReadError::Document(error)) => {
            tracing::error!(error = ?error, "stored recipe revision cannot be decoded");
            return Err(Error::Database(krausen_core::Error::Internal(
                "stored recipe revision cannot be decoded".into(),
            )));
        }
    };
    tx.commit().await.map_err(boundary_error)?;
    Ok(document)
}

struct ResolvedSummary<'a> {
    source: SummarySource,
    document: &'a RecipeDocumentV1,
    validation: Validation,
    targets: krausen_types::formulation::Targets,
    style: Option<krausen_types::formulation::StyleRanges>,
    style_comparison: Option<krausen_types::formulation::StyleComparison>,
    context_digest: Option<String>,
    context: &'a krausen_types::formulation::CalculationContext,
}

fn project(input: ResolvedSummary<'_>) -> ProcessSummary {
    let ResolvedSummary {
        source,
        document,
        validation,
        targets,
        style,
        style_comparison,
        context_digest,
        context,
    } = input;
    let profile = context.equipment_profile.as_ref();
    ProcessSummary {
        source,
        title: document.title.clone(),
        description: document.description.clone(),
        style: document.style.clone(),
        batch_size_l: document.batch_size_l,
        boil_minutes: document.boil_minutes,
        notes: document.notes.clone(),
        readiness: validation.state,
        diagnostics: validation.diagnostics,
        targets,
        style_ranges: style,
        style_comparison,
        assumptions: context.assumptions.clone(),
        equipment: SummaryEquipment {
            profile_id: profile.map(|profile| profile.profile_id.clone()),
            profile_name: profile.map(|profile| profile.name.clone()),
            profile_revision: profile.map(|profile| profile.revision),
            provenance: context.equipment_provenance.clone(),
            settings: context.effective_equipment.clone(),
        },
        context: SummaryContext {
            digest: context_digest,
            engine_version: context.engine_version.clone(),
            engine_build: context.engine_build.clone(),
            policy_version: context.policy_version.clone(),
            native_schema_version: context.native_schema_version,
            result_schema_version: context.result_schema_version,
        },
        fermentables: document
            .fermentables
            .iter()
            .map(|row| SummaryIngredient {
                row_id: row.row_id.clone(),
                section: IngredientSection::Fermentable,
                name: row.specification.name.clone(),
                producer: row.specification.producer.clone(),
                amount: row.amount_kg,
                unit: Some("kg".into()),
                stage: Some(row.addition.stage.clone()),
                time_minutes: row.addition.time_minutes,
                temperature_c: None,
                fermentable_type: row.specification.kind.clone(),
                yield_fraction: row.specification.yield_fraction,
                colour_ebc: row.specification.colour_ebc,
                alpha_fraction: None,
                attenuation_fraction: None,
                source: row.source.clone(),
            })
            .collect(),
        hops: document
            .hops
            .iter()
            .map(|row| SummaryIngredient {
                row_id: row.row_id.clone(),
                section: IngredientSection::Hop,
                name: row.specification.name.clone(),
                producer: row.specification.producer.clone(),
                amount: row.amount_g,
                unit: Some("g".into()),
                stage: Some(row.stage.clone()),
                time_minutes: row.contact_minutes,
                temperature_c: row.temperature_c,
                fermentable_type: None,
                yield_fraction: None,
                colour_ebc: None,
                alpha_fraction: row.specification.alpha_fraction,
                attenuation_fraction: None,
                source: row.source.clone(),
            })
            .collect(),
        yeast: document
            .yeast
            .iter()
            .map(|row| SummaryIngredient {
                row_id: row.row_id.clone(),
                section: IngredientSection::Yeast,
                name: row.specification.name.clone(),
                producer: row.specification.producer.clone(),
                amount: row.amount,
                unit: row.unit.clone(),
                stage: None,
                time_minutes: None,
                temperature_c: None,
                fermentable_type: None,
                yield_fraction: None,
                colour_ebc: None,
                alpha_fraction: None,
                attenuation_fraction: row.specification.attenuation_fraction,
                source: row.source.clone(),
            })
            .collect(),
        other_ingredients: document
            .other_ingredients
            .iter()
            .map(|row| SummaryIngredient {
                row_id: row.row_id.clone(),
                section: IngredientSection::Other,
                name: row.specification.name.clone(),
                producer: row.specification.producer.clone(),
                amount: row.amount,
                unit: row.unit.clone(),
                stage: None,
                time_minutes: None,
                temperature_c: None,
                fermentable_type: None,
                yield_fraction: None,
                colour_ebc: None,
                alpha_fraction: None,
                attenuation_fraction: row.specification.attenuation_fraction,
                source: row.source.clone(),
            })
            .collect(),
        process: SummaryProcess {
            apparent_attenuation_fraction: document.process.apparent_attenuation_fraction,
            mash_steps: document
                .process
                .mash_steps
                .iter()
                .map(project_step)
                .collect(),
            fermentation_steps: document
                .process
                .fermentation_steps
                .iter()
                .map(project_step)
                .collect(),
            whirlpool_minutes: document.process.whirlpool_minutes,
            whirlpool_temperature_c: document.process.whirlpool_temperature_c,
            mash_water_l: document.process.mash_water_l,
            mash_water_method: document.process.mash_water_method.clone(),
            kettle_top_up_l: document.process.kettle_top_up_l,
            chilling: context.effective_equipment.chilling.clone(),
            unsupported: document.process.unsupported.clone(),
        },
    }
}

fn project_step(step: &krausen_types::brewery::ProcessStep) -> SummaryStep {
    SummaryStep {
        row_id: step.row_id.clone(),
        name: step.name.clone(),
        time_minutes: step.time_minutes,
        temperature_c: step.temperature_c,
        volume_l: step.volume_l,
    }
}
