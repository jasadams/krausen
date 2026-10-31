// SPDX-License-Identifier: AGPL-3.0-or-later
//! Authorized linear-ingredients-v1 scaling preview/apply and recipe duplication.
//!
//! The preview is read-only and produces the complete accepted context. Apply
//! owns its write transaction, re-resolves the active profile and effective
//! equipment under the recipe lock, compares every accepted context component
//! plus recomputed targets, and appends one revision (or one new recipe for a
//! duplicate). No formulation source or digest producer is edited here.

use crate::{
    ActorContext, Error, RequestIdentity, authorize_workspace, boundary_error, check_intent,
    db_error, record_change, record_request, recover_request,
};
use krausen_brewery::{
    equipment_store, mapping::calculation, native_store, validation::validate_draft,
};
use krausen_core::{DbPool, DbTx, canonical_json};
use krausen_types::{
    brewery::{EquipmentProfile, EquipmentSettingsV1, RecipeDocumentV1},
    formulation::{CalculationContext, PreviewOutcome, PreviewRequest, Targets},
    scaling::{
        AcceptedScaleContext, DuplicateOutcome, DuplicateRecipe,
        SCALE_POLICY_LINEAR_INGREDIENTS_V1, ScaleApplyOutcome, ScaleApplyRequest, ScaleChange,
        ScaleOutcome, ScalePreview, ScalePreviewRequest, ScaleProblem, ScaleSection, ScaleWarning,
    },
    validation::Validation,
};
use sha2::{Digest, Sha256};

const ENGINE_VERSION: &str = krausen_calc::formulation::ENGINE_VERSION;
const ENGINE_BUILD: &str = krausen_calc::formulation::ENGINE_BUILD;
const POLICY_VERSION: &str = "krausen-formulation-v1";

#[derive(Debug, PartialEq)]
struct ScaledDocument {
    document: RecipeDocumentV1,
    source_batch_l: f64,
    factor: f64,
    changes: Vec<ScaleChange>,
}

fn multiplied(value: Option<f64>, factor: f64) -> Result<Option<f64>, ScaleProblem> {
    let Some(value) = value else {
        return Ok(None);
    };
    if !value.is_finite() {
        return Err(ScaleProblem::InvalidSourceVolume);
    }
    let scaled = value * factor;
    if !scaled.is_finite() {
        return Err(ScaleProblem::InvalidTargetVolume);
    }
    Ok(Some(scaled))
}

fn record(
    changes: &mut Vec<ScaleChange>,
    section: ScaleSection,
    row_id: Option<String>,
    field: &str,
    before: Option<f64>,
    after: Option<f64>,
) {
    if let (Some(before), Some(after)) = (before, after) {
        if before != after {
            changes.push(ScaleChange {
                section,
                row_id,
                field: field.to_owned(),
                before,
                after,
            });
        }
    }
}

/// Pure `linear-ingredients-v1` transformation.
fn scale_document(
    source: &RecipeDocumentV1,
    target_batch_size_l: f64,
) -> Result<ScaledDocument, ScaleProblem> {
    if !target_batch_size_l.is_finite() || target_batch_size_l <= 0.0 {
        return Err(ScaleProblem::InvalidTargetVolume);
    }
    let source_batch_l = source
        .batch_size_l
        .ok_or(ScaleProblem::MissingSourceVolume)?;
    if !source_batch_l.is_finite() || source_batch_l <= 0.0 {
        return Err(ScaleProblem::InvalidSourceVolume);
    }
    let factor = target_batch_size_l / source_batch_l;
    if !factor.is_finite() {
        return Err(ScaleProblem::InvalidTargetVolume);
    }

    let mut document = source.clone();
    document.batch_size_l = Some(target_batch_size_l);
    let mut changes = Vec::new();

    for row in &mut document.fermentables {
        let before = row.amount_kg;
        row.amount_kg = multiplied(before, factor)?;
        record(
            &mut changes,
            ScaleSection::Fermentable,
            Some(row.row_id.clone()),
            "amount_kg",
            before,
            row.amount_kg,
        );
    }
    for row in &mut document.hops {
        let before = row.amount_g;
        row.amount_g = multiplied(before, factor)?;
        record(
            &mut changes,
            ScaleSection::Hop,
            Some(row.row_id.clone()),
            "amount_g",
            before,
            row.amount_g,
        );
    }
    if source.process.mash_water_method.as_deref() == Some("manual") {
        let before = document.process.mash_water_l;
        document.process.mash_water_l = multiplied(before, factor)?;
        record(
            &mut changes,
            ScaleSection::MashWater,
            None,
            "process.mash_water_l",
            before,
            document.process.mash_water_l,
        );
    }
    let before = document.process.kettle_top_up_l;
    document.process.kettle_top_up_l = multiplied(before, factor)?;
    record(
        &mut changes,
        ScaleSection::KettleTopUp,
        None,
        "process.kettle_top_up_l",
        before,
        document.process.kettle_top_up_l,
    );

    Ok(ScaledDocument {
        document,
        source_batch_l,
        factor,
        changes,
    })
}

/// Mirror of the formulation resolver's effective-setting merge. Kept local
/// because formulation.rs participates in its own policy-source hash and must
/// not be refactored.
/// Mirrors formulation.rs, which is hash-participating; keep byte-for-byte
/// semantics. Drift is caught by `mirrored_context_components_match_real_preview`.
fn effective(base: &EquipmentSettingsV1, overrides: &EquipmentSettingsV1) -> EquipmentSettingsV1 {
    let mut e = base.clone();
    macro_rules! apply { ($($field:ident),*) => { $(if overrides.$field.is_some() { e.$field = overrides.$field.clone(); })* }; }
    apply!(
        efficiency_override_fraction,
        mash_efficiency_fraction,
        boil_off_l_per_hour,
        transfer_loss_l,
        pre_boil_deadspace_l,
        grain_absorption_l_per_kg,
        mash_thickness_l_per_kg,
        kettle_capacity_l,
        kettle_diameter_cm,
        kettle_opening_cm,
        boil_temperature_c,
        chilling
    );
    e.extensions.extend(overrides.extensions.clone());
    e
}

/// Mirror of the formulation resolver's disclosed assumptions.
/// Mirrors formulation.rs, which is hash-participating; keep byte-for-byte
/// semantics. Drift is caught by `mirrored_context_components_match_real_preview`.
fn assumptions(document: &RecipeDocumentV1, profile: Option<&EquipmentProfile>) -> Vec<String> {
    let mut assumptions = vec![
        "volume_reference_20c".to_owned(),
        "solid_addition_volume_negligible".to_owned(),
    ];
    if let Some(profile) = profile {
        assumptions.extend(
            profile
                .provenance
                .assumptions
                .iter()
                .map(|item| format!("profile:{item}")),
        );
    }
    if document.process.kettle_top_up_l.is_none() {
        assumptions.push("kettle_top_up_default_zero".to_owned());
    }
    if document.process.whirlpool_minutes.is_none() && document.hops.is_empty() {
        assumptions.push("whirlpool_duration_default_zero".to_owned());
    }
    assumptions
}

/// Current process build identity, used only for context-component comparison.
/// The policy sources are the same bytes hashed by krausen-app::formulation.
/// Mirrors formulation.rs, which is hash-participating; keep byte-for-byte
/// semantics. Drift is caught by `mirrored_context_components_match_real_preview`.
fn engine_source_hash() -> String {
    format!(
        "{:x}",
        Sha256::digest(krausen_calc::formulation::ENGINE_SOURCE.as_bytes())
    )
}

/// Policy source identity mirror. Mirrors formulation.rs, which is
/// hash-participating; keep byte-for-byte semantics. Drift is caught by
/// `mirrored_context_components_match_real_preview`.
fn policy_source_hash() -> String {
    format!(
        "{:x}",
        Sha256::digest(
            concat!(
                include_str!("formulation.rs"),
                include_str!("style_reference.rs"),
                include_str!("../../krausen-brewery/src/mapping/calculation.rs"),
                include_str!("../../krausen-brewery/src/validation.rs"),
                include_str!("../../krausen-types/src/brewery.rs"),
                include_str!("../../krausen-core/src/canonical_json.rs")
            )
            .as_bytes()
        )
    )
}

fn warnings(
    document: &RecipeDocumentV1,
    targets: &Targets,
    effective_equipment: &EquipmentSettingsV1,
) -> Vec<ScaleWarning> {
    let mut warnings = vec![ScaleWarning::FixedLossesRetained {
        transfer_loss_l: effective_equipment.transfer_loss_l,
        boil_off_l_per_hour: effective_equipment.boil_off_l_per_hour,
        pre_boil_deadspace_l: effective_equipment.pre_boil_deadspace_l,
        grain_absorption_l_per_kg: effective_equipment.grain_absorption_l_per_kg,
    }];
    if let Some(post_boil_l) = targets.post_boil_l {
        match effective_equipment.kettle_capacity_l {
            Some(capacity_l) if post_boil_l > capacity_l => {
                warnings.push(ScaleWarning::KettleCapacityExceeded {
                    capacity_l,
                    post_boil_l,
                    pre_boil_l: targets.pre_boil_l,
                });
            }
            None => warnings.push(ScaleWarning::CapacityUnknown { post_boil_l }),
            _ => {}
        }
    }
    let yeast_rows = document.yeast.len();
    let other_ingredient_rows = document.other_ingredients.len();
    if yeast_rows > 0 || other_ingredient_rows > 0 {
        warnings.push(ScaleWarning::UnscaledIngredients {
            yeast_rows,
            other_ingredient_rows,
        });
    }
    if document.process.mash_water_method.as_deref() == Some("profile_thickness") {
        if let Some(thickness_l_per_kg) = effective_equipment.mash_thickness_l_per_kg {
            warnings.push(ScaleWarning::DerivedMashWater { thickness_l_per_kg });
        }
    }
    warnings
}

fn accepted_digest(
    recipe_id: &str,
    source_revision: i64,
    target_batch_size_l: f64,
    policy_version: &str,
    calculation_context_digest: &str,
) -> Result<String, Error> {
    let value = canonical_json::from_serializable(&(
        recipe_id,
        source_revision,
        target_batch_size_l,
        policy_version,
        calculation_context_digest,
    ))
    .map_err(|_| Error::Invalid("scaling context"))?;
    canonical_json::sha256(&value).map_err(|_| Error::Invalid("scaling context"))
}

fn preview_from_formulation(
    preview: PreviewOutcome,
) -> Result<(Targets, Validation, CalculationContext, String), Error> {
    match preview {
        PreviewOutcome::Preview(preview) => {
            let context = preview
                .context
                .ok_or(Error::Invalid("calculation context cannot be encoded"))?;
            let digest = preview
                .context_digest
                .ok_or(Error::Invalid("calculation context cannot be encoded"))?;
            Ok((preview.targets, preview.validation, context, digest))
        }
        PreviewOutcome::UnresolvedProfile => Err(Error::EquipmentArchived),
        PreviewOutcome::UnresolvedStyle => Err(Error::UnresolvedStyle),
        PreviewOutcome::Forbidden => Err(Error::Forbidden),
        PreviewOutcome::ContextChanged => Err(Error::ContextChanged),
        PreviewOutcome::Invalid | PreviewOutcome::Unavailable => {
            Err(Error::Invalid("calculation preview unavailable"))
        }
    }
}

async fn head_row(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    recipe_id: &str,
) -> Result<Option<(i64, bool)>, Error> {
    #[derive(sqlx::FromRow)]
    struct HeadRow {
        head: i64,
        archived: i64,
    }
    let sql = format!(
        "SELECT native_head_revision AS head, CAST(CASE WHEN deleted_at IS NULL THEN 0 ELSE 1 END AS BIGINT) AS archived FROM recipes WHERE workspace_id=$1 AND recipe_id=$2 AND native_head_revision>0{}",
        if matches!(tx, DbTx::Postgres(_)) {
            " FOR UPDATE"
        } else {
            ""
        }
    );
    let row = match &mut *tx {
        DbTx::Postgres(t) => {
            sqlx::query_as::<_, HeadRow>(&sql)
                .bind(&actor.workspace_id)
                .bind(recipe_id)
                .fetch_optional(&mut **t)
                .await
        }
        DbTx::Sqlite(t) => {
            sqlx::query_as::<_, HeadRow>(&sql)
                .bind(&actor.workspace_id)
                .bind(recipe_id)
                .fetch_optional(&mut **t)
                .await
        }
    }
    .map_err(db_error)?;
    Ok(row.map(|row| (row.head, row.archived != 0)))
}

fn duplicate_document(source: &RecipeDocumentV1, title: Option<String>) -> RecipeDocumentV1 {
    let mut document = source.clone();
    if let Some(title) = title {
        document.title = title;
    }
    // The import sidecar is scoped to the source recipe/revision. A duplicate
    // copies ingredient specifications and their source provenance, but not a
    // false claim that the copy owns the original imported file.
    document.import_provenance_id = None;
    document
}

async fn load_head_document(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    recipe_id: &str,
    revision: i64,
) -> Result<RecipeDocumentV1, Error> {
    native_store::load_document(tx, &actor.workspace_id, recipe_id, revision)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "native recipe revision cannot be read");
            Error::Invalid("unsupported recipe revision")
        })?
        .ok_or(Error::Invalid("missing recipe revision"))
}

async fn resolve_profile(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    document: &RecipeDocumentV1,
) -> Result<Option<EquipmentProfile>, Error> {
    let Some(profile_id) = &document.equipment_profile_id else {
        return Ok(None);
    };
    let profile = equipment_store::load(tx, &actor.workspace_id, profile_id, true)
        .await
        .map_err(boundary_error)?;
    // The profile row lock may have waited past the credential expiry boundary.
    authorize_workspace(tx, actor).await?;
    match profile {
        Some(profile) if !profile.archived => Ok(Some(profile)),
        _ => Err(Error::EquipmentArchived),
    }
}

async fn build_preview(
    pool: &DbPool,
    actor: &ActorContext,
    intent: &krausen_types::request_context::OperationIntent,
    recipe_id: &str,
    source_revision: i64,
    source_document: RecipeDocumentV1,
    scaled: ScaledDocument,
) -> Result<ScalePreview, Error> {
    let source_preview = crate::formulation::preview(
        pool,
        actor,
        PreviewRequest {
            draft_generation: "scale-source".to_owned(),
            intent: intent.clone(),
            draft: source_document.clone(),
            equipment_draft: EquipmentSettingsV1::default(),
        },
    )
    .await?;
    let (source_targets, _, _, _) = preview_from_formulation(source_preview)?;

    let scaled_preview = crate::formulation::preview(
        pool,
        actor,
        PreviewRequest {
            draft_generation: "scale-target".to_owned(),
            intent: intent.clone(),
            draft: scaled.document.clone(),
            equipment_draft: EquipmentSettingsV1::default(),
        },
    )
    .await?;
    let (targets, validation, context, context_digest) = preview_from_formulation(scaled_preview)?;
    let digest = accepted_digest(
        recipe_id,
        source_revision,
        scaled.document.batch_size_l.unwrap_or_default(),
        SCALE_POLICY_LINEAR_INGREDIENTS_V1,
        &context_digest,
    )?;
    let accepted = AcceptedScaleContext {
        source_revision,
        target_batch_size_l: scaled.document.batch_size_l.unwrap_or_default(),
        policy_version: SCALE_POLICY_LINEAR_INGREDIENTS_V1.to_owned(),
        calculation_context_digest: context_digest,
        context: context.clone(),
        targets: targets.clone(),
        digest,
    };
    let warnings = warnings(&scaled.document, &targets, &context.effective_equipment);
    Ok(ScalePreview {
        recipe_id: recipe_id.to_owned(),
        source_revision,
        source_batch_size_l: scaled.source_batch_l,
        target_batch_size_l: scaled.document.batch_size_l.unwrap_or_default(),
        factor: scaled.factor,
        source_document,
        scaled_document: scaled.document,
        changes: scaled.changes,
        source_targets,
        targets,
        validation,
        warnings,
        accepted,
    })
}

pub async fn preview(
    pool: &DbPool,
    actor: &ActorContext,
    request: ScalePreviewRequest,
) -> ScaleOutcome {
    match preview_inner(pool, actor, request).await {
        Ok(outcome) => outcome,
        Err(Error::Forbidden) => ScaleOutcome::Forbidden,
        Err(Error::ContextChanged) => ScaleOutcome::ContextChanged,
        Err(Error::EquipmentArchived) => ScaleOutcome::UnresolvedProfile,
        Err(Error::UnresolvedStyle) => ScaleOutcome::UnresolvedStyle,
        Err(Error::Invalid(_)) => ScaleOutcome::Unavailable,
        Err(error) => {
            tracing::error!(%error, "scaling preview failed");
            ScaleOutcome::Unavailable
        }
    }
}

async fn preview_inner(
    pool: &DbPool,
    actor: &ActorContext,
    request: ScalePreviewRequest,
) -> Result<ScaleOutcome, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, &request.intent)?;
    let Some((revision, archived)) = head_row(&mut tx, actor, &request.recipe_id).await? else {
        tx.commit().await.map_err(boundary_error)?;
        return Ok(ScaleOutcome::NotFound);
    };
    if archived {
        tx.commit().await.map_err(boundary_error)?;
        return Ok(ScaleOutcome::Archived);
    }
    let document = load_head_document(&mut tx, actor, &request.recipe_id, revision).await?;
    tx.commit().await.map_err(boundary_error)?;

    let scaled = match scale_document(&document, request.target_batch_size_l) {
        Ok(scaled) => scaled,
        Err(problem) => return Ok(ScaleOutcome::Unscalable(problem)),
    };
    match build_preview(
        pool,
        actor,
        &request.intent,
        &request.recipe_id,
        revision,
        document,
        scaled,
    )
    .await
    {
        Ok(preview) => Ok(ScaleOutcome::Preview(preview)),
        Err(Error::EquipmentArchived) => Ok(ScaleOutcome::UnresolvedProfile),
        Err(error) => Err(error),
    }
}

pub async fn apply(
    pool: &DbPool,
    actor: &ActorContext,
    request: ScaleApplyRequest,
) -> ScaleApplyOutcome {
    match apply_inner(pool, actor, request).await {
        Ok(outcome) => outcome,
        Err(Error::Forbidden) => ScaleApplyOutcome::Forbidden,
        Err(Error::ContextChanged) => ScaleApplyOutcome::ContextChanged,
        Err(Error::EquipmentArchived) => ScaleApplyOutcome::UnresolvedProfile,
        Err(Error::UnresolvedStyle) => ScaleApplyOutcome::UnresolvedStyle,
        Err(Error::RequestIdentityConflict) => ScaleApplyOutcome::RequestIdentityConflict,
        Err(Error::ResultExpired { resource_id }) => {
            ScaleApplyOutcome::ResultExpired { resource_id }
        }
        Err(Error::Retryable) => ScaleApplyOutcome::Retryable,
        Err(Error::Invalid(_)) => ScaleApplyOutcome::InvalidRequest,
        Err(error) => {
            tracing::error!(%error, "scaling apply failed");
            ScaleApplyOutcome::Unavailable
        }
    }
}

async fn context_conflict(
    pool: &DbPool,
    actor: &ActorContext,
    request: &ScaleApplyRequest,
) -> ScaleApplyOutcome {
    match preview(
        pool,
        actor,
        ScalePreviewRequest {
            intent: request.intent.clone(),
            recipe_id: request.recipe_id.clone(),
            target_batch_size_l: request.target_batch_size_l,
        },
    )
    .await
    {
        ScaleOutcome::Preview(fresh) => ScaleApplyOutcome::ContextConflict {
            fresh: Box::new(fresh),
        },
        ScaleOutcome::Archived => ScaleApplyOutcome::Archived,
        ScaleOutcome::NotFound => ScaleApplyOutcome::NotFound,
        ScaleOutcome::Forbidden => ScaleApplyOutcome::Forbidden,
        ScaleOutcome::ContextChanged => ScaleApplyOutcome::ContextChanged,
        ScaleOutcome::UnresolvedProfile => ScaleApplyOutcome::UnresolvedProfile,
        ScaleOutcome::UnresolvedStyle => ScaleApplyOutcome::UnresolvedStyle,
        ScaleOutcome::Unscalable(problem) => ScaleApplyOutcome::Unscalable(problem),
        ScaleOutcome::Invalid(validation) => ScaleApplyOutcome::Invalid(validation),
        ScaleOutcome::Unavailable => ScaleApplyOutcome::Unavailable,
    }
}

fn context_matches(
    accepted: &AcceptedScaleContext,
    actor: &ActorContext,
    document: &RecipeDocumentV1,
    profile: Option<&EquipmentProfile>,
    equipment_base: &EquipmentSettingsV1,
    effective_equipment: &EquipmentSettingsV1,
    style: Option<&krausen_types::formulation::StyleRanges>,
    targets: &Targets,
) -> bool {
    let context = &accepted.context;
    let provenance = if profile.is_some() {
        "saved_profile"
    } else {
        "caller_supplied_equipment_draft"
    };
    context.workspace_id == actor.workspace_id
        && context.draft == *document
        && context.equipment_draft == *equipment_base
        && context.effective_equipment == *effective_equipment
        && context.equipment_provenance == provenance
        && context.equipment_profile.as_ref() == profile
        && context.assumptions == assumptions(document, profile)
        && context.engine_version == ENGINE_VERSION
        && context.engine_build == ENGINE_BUILD
        && context.engine_source_hash == engine_source_hash()
        && context.policy_source_hash == policy_source_hash()
        && context.policy_version == POLICY_VERSION
        && context.native_schema_version == 1
        && context.result_schema_version == 1
        && context.style.as_ref() == style
        && accepted.targets == *targets
}

async fn apply_inner(
    pool: &DbPool,
    actor: &ActorContext,
    request: ScaleApplyRequest,
) -> Result<ScaleApplyOutcome, Error> {
    uuid::Uuid::parse_str(&request.request_id).map_err(|_| Error::Invalid("request id"))?;
    if request.recipe_id.is_empty()
        || request.recipe_id.len() > 256
        || request.expected_revision < 1
        || !request.target_batch_size_l.is_finite()
        || request.target_batch_size_l <= 0.0
    {
        return Err(Error::Invalid("scale request"));
    }
    let payload = canonical_json::from_serializable(&(
        &request.recipe_id,
        request.expected_revision,
        request.target_batch_size_l,
        &request.accepted.digest,
    ))
    .map_err(|_| Error::Invalid("scale request"))?;
    let digest = canonical_json::sha256(&payload).map_err(|_| Error::Invalid("scale request"))?;

    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, &request.intent)?;
    let ws_sql = if matches!(&tx, DbTx::Postgres(_)) {
        "SELECT revision FROM workspaces WHERE workspace_id = $1 AND status = 'active' FOR UPDATE"
    } else {
        "SELECT revision FROM workspaces WHERE workspace_id = $1 AND status = 'active'"
    };
    let workspace = scalar!(&mut tx, i64, ws_sql, &actor.workspace_id).map_err(db_error)?;
    authorize_workspace(&mut tx, actor).await?;
    if workspace.is_none() {
        return Err(Error::Forbidden);
    }
    let key = RequestIdentity {
        actor,
        operation: "recipe.scale-apply.v1",
        request_id: &request.request_id,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        let outcome = serde_json::from_value(result)?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(outcome);
    }

    let Some((actual, archived)) = head_row(&mut tx, actor, &request.recipe_id).await? else {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(ScaleApplyOutcome::NotFound);
    };
    if archived {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(ScaleApplyOutcome::Archived);
    }
    if actual != request.expected_revision {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(ScaleApplyOutcome::Conflict {
            expected: request.expected_revision,
            actual,
        });
    }
    let source_document = load_head_document(&mut tx, actor, &request.recipe_id, actual).await?;
    let scaled = match scale_document(&source_document, request.target_batch_size_l) {
        Ok(scaled) => scaled,
        Err(problem) => {
            tx.rollback().await.map_err(boundary_error)?;
            return Ok(ScaleApplyOutcome::Unscalable(problem));
        }
    };
    let profile = resolve_profile(&mut tx, actor, &scaled.document).await?;
    let base = profile
        .as_ref()
        .map(|profile| profile.settings.clone())
        .unwrap_or_default();
    let effective_equipment = effective(&base, &scaled.document.equipment_overrides);
    let style = match &scaled.document.style {
        Some(reference) => match crate::style_reference::resolve(reference) {
            Some(style) => Some(style),
            None => {
                tx.rollback().await.map_err(boundary_error)?;
                return Ok(ScaleApplyOutcome::UnresolvedStyle);
            }
        },
        None => None,
    };
    let mut validation = validate_draft(&scaled.document, &effective_equipment);
    let targets = calculation::targets(&scaled.document, &effective_equipment, &mut validation);

    let accepted = &request.accepted;
    let digest_matches = accepted.source_revision == actual
        && accepted.target_batch_size_l == request.target_batch_size_l
        && accepted.policy_version == SCALE_POLICY_LINEAR_INGREDIENTS_V1
        && accepted_digest(
            &request.recipe_id,
            accepted.source_revision,
            accepted.target_batch_size_l,
            &accepted.policy_version,
            &accepted.calculation_context_digest,
        )?
        .eq(&accepted.digest);
    let components_match = digest_matches
        && context_matches(
            accepted,
            actor,
            &scaled.document,
            profile.as_ref(),
            &base,
            &effective_equipment,
            style.as_ref(),
            &targets,
        );
    if !components_match {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(context_conflict(pool, actor, &request).await);
    }
    if validation.state == krausen_types::validation::Readiness::Invalid {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(ScaleApplyOutcome::Invalid(validation));
    }
    if scaled.factor == 1.0 {
        tx.commit().await.map_err(boundary_error)?;
        return Ok(ScaleApplyOutcome::Unchanged {
            recipe_id: request.recipe_id,
            revision: actual,
        });
    }

    let saved = native_store::save(
        &mut tx,
        &actor.workspace_id,
        &request.recipe_id,
        actual,
        &scaled.document,
        &actor.actor_id,
        chrono::Utc::now(),
        None,
    )
    .await
    .map_err(boundary_error)?;
    let revision = match saved {
        native_store::SaveResult::Unchanged(revision) => {
            tx.commit().await.map_err(boundary_error)?;
            return Ok(ScaleApplyOutcome::Unchanged {
                recipe_id: request.recipe_id,
                revision,
            });
        }
        native_store::SaveResult::Appended(revision) => revision,
    };
    let outcome = ScaleApplyOutcome::Applied {
        recipe_id: request.recipe_id.clone(),
        revision,
        source_revision: actual,
    };
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&outcome)?,
        Some(&request.recipe_id),
    )
    .await?;
    record_change(
        &mut tx,
        actor,
        "recipe.scale-apply.v1",
        &request.recipe_id,
        revision,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(outcome)
}

pub async fn duplicate(
    pool: &DbPool,
    actor: &ActorContext,
    request: DuplicateRecipe,
) -> DuplicateOutcome {
    match duplicate_inner(pool, actor, request).await {
        Ok(outcome) => outcome,
        Err(Error::Forbidden) => DuplicateOutcome::Forbidden,
        Err(Error::ContextChanged) => DuplicateOutcome::ContextChanged,
        Err(Error::RequestIdentityConflict) => DuplicateOutcome::RequestIdentityConflict,
        Err(Error::ResultExpired { resource_id }) => {
            DuplicateOutcome::ResultExpired { resource_id }
        }
        Err(Error::Retryable) => DuplicateOutcome::Retryable,
        Err(Error::Invalid(_)) => DuplicateOutcome::InvalidRequest,
        Err(error) => {
            tracing::error!(%error, "recipe duplicate failed");
            DuplicateOutcome::Unavailable
        }
    }
}

async fn duplicate_inner(
    pool: &DbPool,
    actor: &ActorContext,
    request: DuplicateRecipe,
) -> Result<DuplicateOutcome, Error> {
    uuid::Uuid::parse_str(&request.request_id).map_err(|_| Error::Invalid("request id"))?;
    if request.source_recipe_id.is_empty()
        || request.source_recipe_id.len() > 256
        || request.expected_revision < 1
        || request
            .title
            .as_ref()
            .is_some_and(|title| title.len() > 512)
    {
        return Err(Error::Invalid("duplicate request"));
    }
    let payload = canonical_json::from_serializable(&(
        &request.source_recipe_id,
        request.expected_revision,
        &request.title,
    ))
    .map_err(|_| Error::Invalid("duplicate request"))?;
    let digest =
        canonical_json::sha256(&payload).map_err(|_| Error::Invalid("duplicate request"))?;

    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, &request.intent)?;
    let ws_sql = if matches!(&tx, DbTx::Postgres(_)) {
        "SELECT revision FROM workspaces WHERE workspace_id = $1 AND status = 'active' FOR UPDATE"
    } else {
        "SELECT revision FROM workspaces WHERE workspace_id = $1 AND status = 'active'"
    };
    let workspace = scalar!(&mut tx, i64, ws_sql, &actor.workspace_id).map_err(db_error)?;
    authorize_workspace(&mut tx, actor).await?;
    if workspace.is_none() {
        return Err(Error::Forbidden);
    }
    let key = RequestIdentity {
        actor,
        operation: "recipe.duplicate.v1",
        request_id: &request.request_id,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        let outcome = serde_json::from_value(result)?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(outcome);
    }

    let Some((actual, archived)) = head_row(&mut tx, actor, &request.source_recipe_id).await?
    else {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(DuplicateOutcome::NotFound);
    };
    if archived {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(DuplicateOutcome::Archived);
    }
    if actual != request.expected_revision {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(DuplicateOutcome::Conflict {
            expected: request.expected_revision,
            actual,
        });
    }
    let source_document =
        load_head_document(&mut tx, actor, &request.source_recipe_id, actual).await?;
    let document = duplicate_document(&source_document, request.title);

    let recipe_id = uuid::Uuid::new_v4().to_string();
    native_store::create(
        &mut tx,
        &actor.workspace_id,
        &recipe_id,
        &document,
        &actor.actor_id,
        chrono::Utc::now(),
    )
    .await
    .map_err(boundary_error)?;
    let outcome = DuplicateOutcome::Duplicated {
        recipe_id: recipe_id.clone(),
        revision: 1,
        source_revision: actual,
    };
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&outcome)?,
        Some(&recipe_id),
    )
    .await?;
    record_change(&mut tx, actor, "recipe.duplicate.v1", &recipe_id, 1).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use krausen_types::brewery::{
        AdditionStage, FermentableAddition, FermentableSpecification, HopAddition,
        HopSpecification, IngredientAddition, IngredientSource, IngredientSpecification,
        ProcessStep, StyleReference,
    };

    fn document() -> RecipeDocumentV1 {
        RecipeDocumentV1 {
            title: "House ale".into(),
            style: Some(StyleReference {
                name: "American Pale Ale".into(),
                edition: Some("2021".into()),
                style_id: Some("18B".into()),
            }),
            batch_size_l: Some(20.0),
            boil_minutes: Some(60.0),
            equipment_profile_id: Some("profile-1".into()),
            equipment_overrides: EquipmentSettingsV1 {
                efficiency_override_fraction: Some(0.72),
                transfer_loss_l: Some(2.0),
                kettle_capacity_l: Some(30.0),
                ..Default::default()
            },
            fermentables: vec![FermentableAddition {
                row_id: "f1".into(),
                amount_kg: Some(4.2),
                addition: AdditionStage {
                    stage: "mash".into(),
                    time_minutes: Some(60.0),
                },
                specification: FermentableSpecification {
                    name: Some("Pale malt".into()),
                    producer: Some("Maltster".into()),
                    kind: Some("grain".into()),
                    yield_fraction: Some(0.8),
                    colour_ebc: Some(7.0),
                },
                source: Some(IngredientSource {
                    scope: "workspace".into(),
                    catalogue_id: None,
                    entry_id: "entry-1".into(),
                    version: Some(3),
                }),
            }],
            hops: vec![HopAddition {
                row_id: "h1".into(),
                amount_g: Some(30.0),
                stage: "boil".into(),
                contact_minutes: Some(60.0),
                temperature_c: Some(100.0),
                specification: HopSpecification {
                    name: Some("Cascade".into()),
                    producer: None,
                    alpha_fraction: Some(0.055),
                },
                source: None,
            }],
            yeast: vec![IngredientAddition {
                row_id: "y1".into(),
                amount: Some(1.0),
                unit: Some("pkg".into()),
                specification: IngredientSpecification {
                    name: Some("Yeast".into()),
                    producer: None,
                    attenuation_fraction: Some(0.78),
                    extensions: Default::default(),
                },
                source: None,
            }],
            other_ingredients: vec![IngredientAddition {
                row_id: "o1".into(),
                amount: Some(5.0),
                unit: Some("g".into()),
                specification: IngredientSpecification {
                    name: Some("Whirlfloc".into()),
                    producer: None,
                    attenuation_fraction: None,
                    extensions: Default::default(),
                },
                source: None,
            }],
            process: krausen_types::brewery::ProcessV1 {
                apparent_attenuation_fraction: Some(0.8),
                mash_steps: vec![ProcessStep {
                    row_id: "s1".into(),
                    name: Some("Saccharification".into()),
                    time_minutes: Some(45.0),
                    temperature_c: Some(66.0),
                    volume_l: Some(15.0),
                    extensions: Default::default(),
                }],
                mash_water_l: Some(15.0),
                mash_water_method: Some("manual".into()),
                kettle_top_up_l: Some(2.0),
                ..Default::default()
            },
            import_provenance_id: Some("provenance-1".into()),
            ..Default::default()
        }
    }

    #[test]
    fn scales_only_linear_inputs_and_preserves_specifications() {
        let source = document();
        let scaled = scale_document(&source, 40.0).unwrap();
        assert_eq!(scaled.factor, 2.0);
        assert_eq!(scaled.source_batch_l, 20.0);
        assert_eq!(scaled.document.batch_size_l, Some(40.0));
        assert_eq!(scaled.document.fermentables[0].amount_kg, Some(8.4));
        assert_eq!(scaled.document.hops[0].amount_g, Some(60.0));
        assert_eq!(scaled.document.process.mash_water_l, Some(30.0));
        assert_eq!(scaled.document.process.kettle_top_up_l, Some(4.0));
        assert_eq!(scaled.document.yeast[0].amount, Some(1.0));
        assert_eq!(scaled.document.other_ingredients[0].amount, Some(5.0));
        assert_eq!(scaled.document.hops[0].contact_minutes, Some(60.0));
        assert_eq!(scaled.document.hops[0].temperature_c, Some(100.0));
        assert_eq!(
            scaled.document.process.mash_steps[0].time_minutes,
            Some(45.0)
        );
        assert_eq!(
            scaled.document.process.apparent_attenuation_fraction,
            Some(0.8)
        );
        assert_eq!(scaled.document.style, source.style);
        assert_eq!(
            scaled.document.equipment_overrides,
            source.equipment_overrides
        );
        assert_eq!(
            scaled.document.fermentables[0].specification,
            source.fermentables[0].specification
        );
        assert_eq!(
            scaled.document.fermentables[0].source,
            source.fermentables[0].source
        );
        assert_eq!(scaled.changes.len(), 4);
    }

    #[test]
    fn profile_thickness_mash_water_is_derived_not_multiplied() {
        let mut source = document();
        source.process.mash_water_method = Some("profile_thickness".into());
        source.process.mash_water_l = Some(15.0);
        let scaled = scale_document(&source, 40.0).unwrap();
        assert_eq!(scaled.document.process.mash_water_l, Some(15.0));
        assert!(
            !scaled
                .changes
                .iter()
                .any(|change| change.section == ScaleSection::MashWater)
        );
    }

    #[test]
    fn absent_values_stay_absent() {
        let mut source = document();
        source.fermentables[0].amount_kg = None;
        source.hops[0].amount_g = None;
        source.process.kettle_top_up_l = None;
        let scaled = scale_document(&source, 40.0).unwrap();
        assert_eq!(scaled.document.fermentables[0].amount_kg, None);
        assert_eq!(scaled.document.hops[0].amount_g, None);
        assert_eq!(scaled.document.process.kettle_top_up_l, None);
    }

    #[test]
    fn equal_volume_produces_no_changes() {
        let scaled = scale_document(&document(), 20.0).unwrap();
        assert_eq!(scaled.factor, 1.0);
        assert!(scaled.changes.is_empty());
    }

    #[test]
    fn missing_zero_and_nonfinite_volumes_are_not_silently_one() {
        let mut source = document();
        source.batch_size_l = None;
        assert_eq!(
            scale_document(&source, 40.0),
            Err(ScaleProblem::MissingSourceVolume)
        );
        source.batch_size_l = Some(0.0);
        assert_eq!(
            scale_document(&source, 40.0),
            Err(ScaleProblem::InvalidSourceVolume)
        );
        assert_eq!(
            scale_document(&document(), 0.0),
            Err(ScaleProblem::InvalidTargetVolume)
        );
        assert_eq!(
            scale_document(&document(), f64::NAN),
            Err(ScaleProblem::InvalidTargetVolume)
        );
    }

    #[test]
    fn duplicate_clears_import_provenance_only() {
        let source = document();
        let duplicate = duplicate_document(&source, Some("Copy".into()));
        assert_eq!(duplicate.title, "Copy");
        assert_eq!(duplicate.import_provenance_id, None);
        assert_eq!(duplicate.fermentables, source.fermentables);
        assert_eq!(duplicate.hops, source.hops);
        assert_eq!(duplicate.process, source.process);
        assert_eq!(duplicate.equipment_profile_id, source.equipment_profile_id);
        let same_title = duplicate_document(&source, None);
        assert_eq!(same_title.title, source.title);
    }

    #[test]
    fn capacity_and_fixed_losses_are_explicit() {
        let targets = Targets {
            post_boil_l: Some(42.0),
            pre_boil_l: Some(45.0),
            ..Default::default()
        };
        let equipment = EquipmentSettingsV1 {
            kettle_capacity_l: Some(30.0),
            transfer_loss_l: Some(2.0),
            ..Default::default()
        };
        let warnings = warnings(&document(), &targets, &equipment);
        assert!(warnings.iter().any(|warning| matches!(
            warning,
            ScaleWarning::KettleCapacityExceeded {
                capacity_l: 30.0,
                post_boil_l: 42.0,
                ..
            }
        )));
        assert!(warnings.iter().any(|warning| matches!(
            warning,
            ScaleWarning::FixedLossesRetained {
                transfer_loss_l: Some(2.0),
                ..
            }
        )));
        assert!(warnings.iter().any(|warning| matches!(
            warning,
            ScaleWarning::UnscaledIngredients {
                yeast_rows: 1,
                other_ingredient_rows: 1,
            }
        )));
    }

    #[tokio::test]
    async fn mirrored_context_components_match_real_preview() {
        use crate::{ActorContext, equipment};
        use krausen_types::equipment::SaveEquipment;
        use krausen_types::formulation::{PreviewOutcome, PreviewRequest};
        use krausen_types::request_context::OperationIntent;

        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../apps/server/migrations-sqlite")
            .run(&pool)
            .await
            .unwrap();
        let suffix = uuid::Uuid::new_v4();
        let user = format!("u-{suffix}");
        let workspace = format!("a-{suffix}");
        let credential = format!("c-{suffix}");
        sqlx::query("INSERT INTO users (user_id,email) VALUES ($1,$2)")
            .bind(&user)
            .bind(format!("{suffix}@example.test"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ($1,$2)")
            .bind(&workspace)
            .bind(&user)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO workspace_users (workspace_id,user_id,role) VALUES ($1,$2,'member')",
        )
        .bind(&workspace)
        .bind(&user)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)")
            .bind(&credential)
            .bind(&user)
            .bind(4102444800_i64)
            .execute(&pool)
            .await
            .unwrap();
        let db = DbPool::Sqlite(pool);
        let actor = ActorContext {
            actor_id: user.clone(),
            workspace_id: workspace.clone(),
            credential_id: credential,
            access_expires_at: 4102444800,
        };
        let intent = OperationIntent {
            account_id: user,
            workspace_id: workspace,
        };
        let profile = equipment::save(
            &db,
            &actor,
            SaveEquipment {
                intent: intent.clone(),
                profile_id: uuid::Uuid::new_v4().to_string(),
                name: "Mirror profile".into(),
                settings: EquipmentSettingsV1 {
                    mash_efficiency_fraction: Some(0.75),
                    boil_off_l_per_hour: Some(3.0),
                    transfer_loss_l: Some(2.0),
                    pre_boil_deadspace_l: Some(0.0),
                    grain_absorption_l_per_kg: Some(0.8),
                    kettle_capacity_l: Some(30.0),
                    mash_thickness_l_per_kg: Some(3.0),
                    ..Default::default()
                },
                provenance: krausen_types::brewery::EquipmentProvenance {
                    source: "test".into(),
                    assumptions: vec!["mirror assumption".into()],
                },
                expected_revision: 0,
                expected_metadata_revision: 0,
                request_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .await
        .unwrap();

        let mut draft = document();
        draft.equipment_profile_id = Some(profile.profile_id.clone());
        draft.equipment_overrides.efficiency_override_fraction = Some(0.7);
        draft.equipment_overrides.kettle_capacity_l = Some(31.0);
        draft.process.mash_water_method = Some("profile_thickness".into());
        draft.process.mash_water_l = None;
        draft.style = Some(StyleReference {
            name: "Munich Helles".into(),
            edition: Some("BJCP 2021".into()),
            style_id: Some("4A".into()),
        });
        let preview = crate::formulation::preview(
            &db,
            &actor,
            PreviewRequest {
                draft_generation: "mirror".into(),
                intent,
                draft: draft.clone(),
                equipment_draft: EquipmentSettingsV1::default(),
            },
        )
        .await
        .unwrap();
        let PreviewOutcome::Preview(preview) = preview else {
            panic!("real preview required");
        };
        let context = preview.context.expect("preview context");
        let base = profile.settings.clone();
        assert_eq!(context.workspace_id, actor.workspace_id);
        assert_eq!(context.draft, draft);
        assert_eq!(context.equipment_draft, base);
        assert_eq!(
            context.effective_equipment,
            effective(&base, &draft.equipment_overrides)
        );
        assert_eq!(context.equipment_provenance, "saved_profile");
        assert_eq!(context.equipment_profile, Some(profile.clone()));
        assert_eq!(context.assumptions, assumptions(&draft, Some(&profile)));
        assert_eq!(context.engine_version, ENGINE_VERSION);
        assert_eq!(context.engine_build, ENGINE_BUILD);
        assert_eq!(context.engine_source_hash, engine_source_hash());
        assert_eq!(context.policy_source_hash, policy_source_hash());
        assert_eq!(context.policy_version, POLICY_VERSION);
        assert_eq!(context.native_schema_version, 1);
        assert_eq!(context.result_schema_version, 1);
        assert_eq!(
            context.style,
            crate::style_reference::resolve(draft.style.as_ref().unwrap())
        );
    }

    #[test]
    fn accepted_digest_binds_revision_target_policy_and_context() {
        let base = accepted_digest("r1", 3, 40.0, "linear-ingredients-v1", "ctx").unwrap();
        assert_eq!(
            base,
            accepted_digest("r1", 3, 40.0, "linear-ingredients-v1", "ctx").unwrap()
        );
        assert_ne!(
            base,
            accepted_digest("r1", 4, 40.0, "linear-ingredients-v1", "ctx").unwrap()
        );
        assert_ne!(
            base,
            accepted_digest("r1", 3, 20.0, "linear-ingredients-v1", "ctx").unwrap()
        );
        assert_ne!(
            base,
            accepted_digest("r1", 3, 40.0, "linear-ingredients-v2", "ctx").unwrap()
        );
        assert_ne!(
            base,
            accepted_digest("r1", 3, 40.0, "linear-ingredients-v1", "other").unwrap()
        );
    }
}
