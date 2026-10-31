// SPDX-License-Identifier: AGPL-3.0-or-later
//! Authorized formulation previews and context identities.
use crate::{ActorContext, Error, authorize_workspace};
use krausen_core::{DbPool, canonical_json};
use krausen_types::{
    brewery::EquipmentSettingsV1,
    formulation::{Preview, PreviewOutcome, PreviewRequest},
};
use sha2::{Digest, Sha256};

fn effective(base: &EquipmentSettingsV1, overrides: &EquipmentSettingsV1) -> EquipmentSettingsV1 {
    let mut e = base.clone();
    macro_rules! apply { ($($field:ident),*) => { $(if overrides.$field.is_some() { e.$field=overrides.$field.clone(); })* }; }
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

/// Run the shared preview after verifying current workspace membership. The
/// equipment draft has explicit caller-supplied provenance and is not a profile.
pub async fn preview(
    pool: &DbPool,
    actor: &ActorContext,
    request: PreviewRequest,
) -> Result<PreviewOutcome, Error> {
    let mut tx = pool.begin_write().await?;
    authorize_workspace(&mut tx, actor).await?;
    crate::check_intent(actor, &request.intent)?;
    if request.draft.equipment_profile_id.is_some() {
        return Ok(PreviewOutcome::UnresolvedProfile);
    }
    let style = match &request.draft.style {
        Some(reference) => match crate::style_reference::resolve(reference) {
            Some(style) => Some(style),
            None => return Ok(PreviewOutcome::UnresolvedStyle),
        },
        None => None,
    };
    if request.draft.equipment_overrides.schema_version != 1 {
        return Err(Error::Invalid("unsupported equipment override schema"));
    }
    let e = effective(&request.equipment_draft, &request.draft.equipment_overrides);
    let mut validation = krausen_brewery::validation::validate_draft(&request.draft, &e);
    let targets =
        krausen_brewery::mapping::calculation::targets(&request.draft, &e, &mut validation);
    let engine_source_hash = format!(
        "{:x}",
        Sha256::digest(krausen_calc::formulation::ENGINE_SOURCE.as_bytes())
    );
    let policy_source_hash = format!(
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
    );
    let mut assumptions = vec![
        "volume_reference_20c".into(),
        "solid_addition_volume_negligible".into(),
    ];
    if request.draft.process.kettle_top_up_l.is_none() {
        assumptions.push("kettle_top_up_default_zero".into());
    }
    if request.draft.process.whirlpool_minutes.is_none() && request.draft.hops.is_empty() {
        assumptions.push("whirlpool_duration_default_zero".into());
    }
    let envelope = krausen_types::formulation::CalculationContext {
        workspace_id: actor.workspace_id.clone(),
        draft: request.draft.clone(),
        equipment_draft: request.equipment_draft.clone(),
        effective_equipment: e,
        equipment_provenance: "caller_supplied_equipment_draft".into(),
        assumptions,
        engine_version: krausen_calc::formulation::ENGINE_VERSION.into(),
        engine_build: krausen_calc::formulation::ENGINE_BUILD.into(),
        engine_source_hash,
        policy_source_hash,
        policy_version: "krausen-formulation-v1".into(),
        native_schema_version: 1,
        result_schema_version: 1,
        style: style.clone(),
    };
    let digest = canonical_json::from_serializable(&envelope)
        .ok()
        .and_then(|v| canonical_json::sha256(&v).ok());
    if digest.is_none() && validation.state != krausen_types::validation::Readiness::Invalid {
        return Err(Error::Invalid("calculation context cannot be encoded"));
    }
    let style_comparison = style
        .as_ref()
        .map(|s| crate::style_reference::compare(s, &targets));
    tx.commit().await?;
    Ok(PreviewOutcome::Preview(Preview {
        draft_generation: request.draft_generation,
        validation,
        targets,
        context: digest.as_ref().map(|_| envelope),
        context_digest: digest,
        style,
        style_comparison,
    }))
}

/// Bounded list of supported style identities under the same workspace policy.
pub async fn list_styles(
    pool: &DbPool,
    actor: &ActorContext,
    intent: &krausen_types::request_context::OperationIntent,
) -> Result<Vec<krausen_types::brewery::StyleReference>, Error> {
    let mut tx = pool.begin_write().await?;
    authorize_workspace(&mut tx, actor).await?;
    crate::check_intent(actor, intent)?;
    let styles = crate::style_reference::identities();
    tx.commit().await?;
    Ok(styles)
}

/// Shared sanitized result mapping for both adapters.
pub fn project_preview(result: Result<PreviewOutcome, Error>) -> PreviewOutcome {
    match result {
        Ok(outcome) => outcome,
        Err(Error::Forbidden) => PreviewOutcome::Forbidden,
        Err(Error::ContextChanged) => PreviewOutcome::ContextChanged,
        Err(Error::Invalid(_)) => PreviewOutcome::Invalid,
        Err(error) => {
            tracing::warn!(%error, "formulation preview unavailable");
            PreviewOutcome::Unavailable
        }
    }
}
pub fn project_styles(
    result: Result<Vec<krausen_types::brewery::StyleReference>, Error>,
) -> krausen_types::formulation::StyleListOutcome {
    use krausen_types::formulation::StyleListOutcome;
    match result {
        Ok(styles) => StyleListOutcome::Styles(styles),
        Err(Error::Forbidden) => StyleListOutcome::Forbidden,
        Err(Error::ContextChanged) => StyleListOutcome::ContextChanged,
        Err(error) => {
            tracing::warn!(%error, "style list unavailable");
            StyleListOutcome::Unavailable
        }
    }
}
