// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared transaction-aware formulation resolver.
//!
//! This is the single producer of calculation targets, assumptions and the
//! accepted context digest. `formulation::preview` owns a pool transaction;
//! commands such as brew start call `resolve_in_tx` on their held `DbTx`.
use crate::{ActorContext, Error, authorize_workspace};
use krausen_core::{DbTx, canonical_json};
use krausen_types::{
    brewery::EquipmentSettingsV1,
    formulation::{Preview, PreviewOutcome, PreviewRequest},
};
use sha2::{Digest, Sha256};

pub(crate) const POLICY_VERSION: &str = "krausen-formulation-v2";

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

pub(crate) fn engine_source_hash() -> String {
    format!(
        "{:x}",
        Sha256::digest(krausen_calc::formulation::ENGINE_SOURCE.as_bytes())
    )
}

/// Source identity for policy V2. The resolver module is included; the thin
/// preview wrapper is not, so moving orchestration cannot change the identity.
pub(crate) fn policy_source_hash() -> String {
    format!(
        "{:x}",
        Sha256::digest(
            concat!(
                include_str!("formulation_kernel.rs"),
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

/// Resolve and calculate on the caller's transaction. Authorization and intent
/// are checked here; the profile lock is followed by a second authorization
/// because it may wait across the credential expiry boundary. This function
/// never begins, commits or rolls back a transaction.
pub(crate) async fn resolve_in_tx(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    request: PreviewRequest,
) -> Result<PreviewOutcome, Error> {
    authorize_workspace(tx, actor).await?;
    crate::check_intent(actor, &request.intent)?;
    let profile = if let Some(id) = &request.draft.equipment_profile_id {
        match krausen_brewery::equipment_store::load(tx, &actor.workspace_id, id, true)
            .await
            .map_err(crate::boundary_error)?
        {
            Some(profile) if !profile.archived => Some(profile),
            _ => return Ok(PreviewOutcome::UnresolvedProfile),
        }
    } else {
        None
    };
    // A profile row lock may have waited across the credential expiry boundary.
    authorize_workspace(tx, actor).await?;
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
    let base = profile
        .as_ref()
        .map(|p| &p.settings)
        .unwrap_or(&request.equipment_draft);
    let e = effective(base, &request.draft.equipment_overrides);
    let mut validation = krausen_brewery::validation::validate_draft(&request.draft, &e);
    let mut applied_defaults = Vec::new();
    let targets = krausen_brewery::mapping::calculation::targets_with_defaults(
        &request.draft,
        &e,
        &mut validation,
        &mut applied_defaults,
    );
    let engine_source_hash = engine_source_hash();
    let policy_source_hash = policy_source_hash();
    let mut assumptions = vec![
        "volume_reference_20c".into(),
        "solid_addition_volume_negligible".into(),
    ];
    if let Some(profile) = &profile {
        assumptions.extend(
            profile
                .provenance
                .assumptions
                .iter()
                .map(|item| format!("profile:{item}")),
        );
    }
    if request.draft.process.kettle_top_up_l.is_none() {
        assumptions.push("kettle_top_up_default_zero".into());
    }
    if request.draft.process.whirlpool_minutes.is_none() && request.draft.hops.is_empty() {
        assumptions.push("whirlpool_duration_default_zero".into());
    }
    let envelope = krausen_types::formulation::CalculationContext {
        workspace_id: actor.workspace_id.clone(),
        draft: request.draft.clone(),
        equipment_draft: base.clone(),
        effective_equipment: e,
        equipment_provenance: if profile.is_some() {
            "saved_profile"
        } else {
            "caller_supplied_equipment_draft"
        }
        .into(),
        equipment_profile: profile,
        assumptions,
        applied_defaults,
        engine_version: krausen_calc::formulation::ENGINE_VERSION.into(),
        engine_build: krausen_calc::formulation::ENGINE_BUILD.into(),
        engine_source_hash,
        policy_source_hash,
        policy_version: POLICY_VERSION.into(),
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
