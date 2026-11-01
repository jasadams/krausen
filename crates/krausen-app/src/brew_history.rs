// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared bounded brew history, comparison, explicit feedback and frozen print.
//!
//! Reads project the immutable KRA-20 plan and KRA-21 evidence. Feedback appends
//! one guarded recipe revision and never mutates the brew, plan, actuals or
//! equipment profile.

use crate::{
    ActorContext, Error, RequestIdentity, authorize_workspace, boundary_error, check_intent,
    db_error, record_change, record_request, recover_request,
};
use krausen_brewery::{brew_evidence_store as evidence_store, brew_store, native_store};
use krausen_core::{DbPool, DbTx, canonical_json};
use krausen_types::{
    brew::{BrewEvidenceViewOutcome, BrewEvidenceViewRequest, MeasurementSlot},
    brew_history::{
        BrewChangeKind, BrewComparison, BrewComparisonOutcome, BrewComparisonRequest,
        BrewComparisonValue, BrewFeedbackApplyOutcome, BrewFeedbackApplyRequest,
        BrewFeedbackPreview, BrewFeedbackPreviewOutcome, BrewFeedbackPreviewRequest,
        BrewFormulationChange, BrewHistorySection, BrewMeasurementComparison, BrewPrint,
        BrewPrintOutcome, BrewPrintRequest, RecipeBrewHistoryOutcome, RecipeBrewHistoryPage,
        RecipeBrewHistoryQuery, RecipeBrewSummary,
    },
    brewery::{ActualFormulationV1, BrewPlanV1, ProcessStep, RecipeDocumentV1},
    formulation::{Preview, PreviewOutcome, PreviewRequest},
    process_summary::{
        IngredientSection, ProcessSummary, SummaryContext, SummaryEquipment, SummaryIngredient,
        SummaryProcess, SummarySource, SummaryStep,
    },
    request_context::OperationIntent,
    validation::{Diagnostic, Readiness},
};
use std::collections::{BTreeMap, BTreeSet};

async fn begin<'a>(
    pool: &'a DbPool,
    actor: &ActorContext,
    intent: &OperationIntent,
) -> Result<DbTx<'a>, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, intent)?;
    Ok(tx)
}

fn validate_id(value: &str, name: &'static str) -> Result<(), Error> {
    if value.is_empty() || value.len() > 256 {
        Err(Error::Invalid(name))
    } else {
        Ok(())
    }
}

fn validate_changes(change_ids: &[String]) -> Result<BTreeSet<String>, Error> {
    if change_ids.is_empty() || change_ids.len() > 200 {
        return Err(Error::Invalid("change selection"));
    }
    let mut set = BTreeSet::new();
    for id in change_ids {
        if id.is_empty() || id.len() > 256 || !set.insert(id.clone()) {
            return Err(Error::Invalid("change selection"));
        }
    }
    Ok(set)
}

async fn recipe_exists(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    recipe_id: &str,
) -> Result<bool, Error> {
    Ok(scalar!(
        tx,
        String,
        "SELECT recipe_id FROM recipes WHERE workspace_id = $1 AND recipe_id = $2",
        &actor.workspace_id,
        recipe_id
    )
    .map_err(db_error)?
    .is_some())
}

async fn recipe_head(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    recipe_id: &str,
    lock: bool,
) -> Result<Option<(i64, RecipeDocumentV1)>, Error> {
    let sql = format!(
        "SELECT native_head_revision, CAST(CASE WHEN deleted_at IS NULL THEN 0 ELSE 1 END AS BIGINT) AS archived FROM recipes WHERE workspace_id = $1 AND recipe_id = $2 AND native_head_revision > 0{}",
        if lock && matches!(tx, DbTx::Postgres(_)) {
            " FOR UPDATE"
        } else {
            ""
        }
    );
    #[derive(sqlx::FromRow)]
    struct Head {
        native_head_revision: i64,
        archived: i64,
    }
    let row = match &mut *tx {
        DbTx::Postgres(inner) => {
            sqlx::query_as::<_, Head>(&sql)
                .bind(&actor.workspace_id)
                .bind(recipe_id)
                .fetch_optional(&mut **inner)
                .await
        }
        DbTx::Sqlite(inner) => {
            sqlx::query_as::<_, Head>(&sql)
                .bind(&actor.workspace_id)
                .bind(recipe_id)
                .fetch_optional(&mut **inner)
                .await
        }
    }
    .map_err(db_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.archived != 0 {
        return Ok(None);
    }
    let document =
        native_store::load_document(tx, &actor.workspace_id, recipe_id, row.native_head_revision)
            .await
            .map_err(|error| match error {
                native_store::StoredReadError::Storage(error) => boundary_error(error),
                native_store::StoredReadError::UnsupportedVersion(version) => {
                    tracing::error!(version, "stored recipe revision has an unsupported version");
                    Error::Database(krausen_core::Error::Internal(
                        "unsupported recipe revision".into(),
                    ))
                }
                native_store::StoredReadError::Document(error) => {
                    tracing::error!(error = ?error, "stored recipe revision cannot be decoded");
                    Error::Database(krausen_core::Error::Internal(
                        "stored recipe revision cannot be decoded".into(),
                    ))
                }
            })?
            .ok_or(Error::Invalid("missing recipe revision"))?;
    Ok(Some((row.native_head_revision, document)))
}

pub async fn history(
    pool: &DbPool,
    actor: &ActorContext,
    query: RecipeBrewHistoryQuery,
) -> RecipeBrewHistoryOutcome {
    match history_inner(pool, actor, query).await {
        Ok(outcome) => outcome,
        Err(Error::Forbidden) => RecipeBrewHistoryOutcome::Forbidden,
        Err(Error::ContextChanged) => RecipeBrewHistoryOutcome::ContextChanged,
        Err(Error::Invalid(_)) => RecipeBrewHistoryOutcome::InvalidRequest,
        Err(error) => {
            tracing::warn!(%error, "brew history unavailable");
            RecipeBrewHistoryOutcome::Unavailable
        }
    }
}

async fn history_inner(
    pool: &DbPool,
    actor: &ActorContext,
    query: RecipeBrewHistoryQuery,
) -> Result<RecipeBrewHistoryOutcome, Error> {
    validate_id(&query.recipe_id, "recipe")?;
    crate::recipe_library::limit(query.limit)?;
    let mut tx = begin(pool, actor, &query.intent).await?;
    if !recipe_exists(&mut tx, actor, &query.recipe_id).await? {
        return Ok(RecipeBrewHistoryOutcome::NotFound);
    }
    let filter = format!(
        "recipe:{}:{}",
        query.recipe_id,
        if query.include_archived {
            "all"
        } else {
            "active"
        }
    );
    let (before_started, before_id) = match query.cursor.as_deref() {
        Some(value) => {
            let cursor = crate::recipe_library::decode(value, actor, "brew", &filter, "started")?;
            let started = cursor
                .position
                .parse::<i64>()
                .map_err(|_| Error::Invalid("cursor"))?;
            (Some(started), Some(cursor.id.clone()))
        }
        None => (None, None),
    };
    let mut rows = brew_store::list_for_recipe(
        &mut tx,
        &actor.workspace_id,
        &query.recipe_id,
        query.include_archived,
        before_started,
        before_id.as_deref(),
        query.limit + 1,
    )
    .await
    .map_err(boundary_error)?;
    let next_cursor = if rows.len() as i64 > query.limit {
        rows.truncate(query.limit as usize);
        rows.last()
            .map(|row| {
                crate::recipe_library::cursor(
                    actor,
                    "brew",
                    &filter,
                    "started",
                    row.started_at.to_string(),
                    row.brew_id.clone(),
                )
            })
            .transpose()?
    } else {
        None
    };
    tx.commit().await.map_err(boundary_error)?;
    Ok(RecipeBrewHistoryOutcome::History(RecipeBrewHistoryPage {
        items: rows
            .into_iter()
            .map(|row| RecipeBrewSummary {
                brew_id: row.brew_id,
                recipe_id: row.recipe_id,
                selected_recipe_revision: row.selected_recipe_revision,
                status: row.status,
                revision: row.revision,
                started_at: row.started_at,
                started_by: row.started_by,
                archived: row.archived,
                metadata_revision: row.metadata_revision,
                actual_revision: row.actual_revision,
                plan_content_digest: row.plan_content_digest,
            })
            .collect(),
        next_cursor,
    }))
}

pub async fn comparison(
    pool: &DbPool,
    actor: &ActorContext,
    request: BrewComparisonRequest,
) -> BrewComparisonOutcome {
    match comparison_inner(pool, actor, request).await {
        Ok(outcome) => outcome,
        Err(Error::Forbidden) => BrewComparisonOutcome::Forbidden,
        Err(Error::ContextChanged) => BrewComparisonOutcome::ContextChanged,
        Err(error) => {
            tracing::warn!(%error, "brew comparison unavailable");
            BrewComparisonOutcome::Unavailable
        }
    }
}

async fn comparison_inner(
    pool: &DbPool,
    actor: &ActorContext,
    request: BrewComparisonRequest,
) -> Result<BrewComparisonOutcome, Error> {
    validate_id(&request.brew_id, "brew")?;
    let view = crate::brew_evidence::view(
        pool,
        actor,
        BrewEvidenceViewRequest {
            intent: request.intent,
            brew_id: request.brew_id,
            lifecycle_cursor: None,
        },
    )
    .await;
    let view = match view {
        BrewEvidenceViewOutcome::View(view) => view,
        BrewEvidenceViewOutcome::NotFound => return Ok(BrewComparisonOutcome::NotFound),
        BrewEvidenceViewOutcome::Forbidden => return Ok(BrewComparisonOutcome::Forbidden),
        BrewEvidenceViewOutcome::ContextChanged => {
            return Ok(BrewComparisonOutcome::ContextChanged);
        }
        BrewEvidenceViewOutcome::Invalid => return Ok(BrewComparisonOutcome::Unavailable),
        BrewEvidenceViewOutcome::Retryable | BrewEvidenceViewOutcome::Unavailable => {
            return Ok(BrewComparisonOutcome::Unavailable);
        }
    };
    let plan = view.plan.clone();
    let measurements = view
        .measurements
        .into_iter()
        .map(|slot| measurement_comparison(slot, &plan))
        .collect();
    let changes = comparison_changes(
        &view.plan,
        view.actual.as_ref().map(|actual| &actual.document),
    );
    Ok(BrewComparisonOutcome::Comparison(BrewComparison {
        brew_id: view.brew_id,
        recipe_id: view.recipe_id,
        selected_recipe_revision: view.selected_recipe_revision,
        status: view.status,
        revision: view.revision,
        metadata_revision: view.metadata_revision,
        archived: view.archived,
        plan: view.plan,
        actual: view.actual,
        measurements,
        changes,
    }))
}

fn canonical_digest<T: serde::Serialize>(value: &T) -> Option<String> {
    canonical_json::from_serializable(value)
        .ok()
        .and_then(|value| canonical_json::sha256(&value).ok())
}

fn value_for_fermentable(row: &krausen_types::brewery::FermentableAddition) -> BrewComparisonValue {
    BrewComparisonValue {
        canonical_digest: canonical_digest(row),
        name: row.specification.name.clone(),
        amount: row.amount_kg,
        unit: Some("kg".into()),
        stage: Some(row.addition.stage.clone()),
        time_minutes: row.addition.time_minutes,
        ..Default::default()
    }
}

fn value_for_hop(row: &krausen_types::brewery::HopAddition) -> BrewComparisonValue {
    BrewComparisonValue {
        canonical_digest: canonical_digest(row),
        name: row.specification.name.clone(),
        amount: row.amount_g,
        unit: Some("g".into()),
        stage: Some(row.stage.clone()),
        time_minutes: row.contact_minutes,
        temperature_c: row.temperature_c,
        ..Default::default()
    }
}

fn value_for_other(row: &krausen_types::brewery::IngredientAddition) -> BrewComparisonValue {
    BrewComparisonValue {
        canonical_digest: canonical_digest(row),
        name: row.specification.name.clone(),
        amount: row.amount,
        unit: row.unit.clone(),
        ..Default::default()
    }
}

fn value_for_step(row: &ProcessStep) -> BrewComparisonValue {
    BrewComparisonValue {
        canonical_digest: canonical_digest(row),
        name: row.name.clone(),
        time_minutes: row.time_minutes,
        temperature_c: row.temperature_c,
        volume_l: row.volume_l,
        ..Default::default()
    }
}

fn change(
    id: String,
    section: BrewHistorySection,
    row_id: Option<String>,
    field: Option<String>,
    kind: BrewChangeKind,
    planned: Option<BrewComparisonValue>,
    actual: Option<BrewComparisonValue>,
) -> BrewFormulationChange {
    BrewFormulationChange {
        id,
        section,
        row_id,
        field,
        kind,
        planned,
        actual,
    }
}

fn ingredient_changes(
    section: BrewHistorySection,
    prefix: &str,
    planned: &[(String, BrewComparisonValue)],
    actual: &[(String, BrewComparisonValue)],
    out: &mut Vec<BrewFormulationChange>,
) {
    let planned_map: BTreeMap<&str, &BrewComparisonValue> = planned
        .iter()
        .map(|(id, value)| (id.as_str(), value))
        .collect();
    let actual_map: BTreeMap<&str, &BrewComparisonValue> = actual
        .iter()
        .map(|(id, value)| (id.as_str(), value))
        .collect();
    for (id, value) in planned {
        match actual_map.get(id.as_str()) {
            Some(actual_value) if *actual_value != value => out.push(change(
                format!("{prefix}:{id}"),
                section,
                Some(id.clone()),
                None,
                BrewChangeKind::Changed,
                Some(value.clone()),
                Some((*actual_value).clone()),
            )),
            None => out.push(change(
                format!("{prefix}:{id}"),
                section,
                Some(id.clone()),
                None,
                BrewChangeKind::Removed,
                Some(value.clone()),
                None,
            )),
            _ => {}
        }
    }
    for (id, value) in actual {
        if !planned_map.contains_key(id.as_str()) {
            out.push(change(
                format!("{prefix}:{id}"),
                section,
                Some(id.clone()),
                None,
                BrewChangeKind::Added,
                None,
                Some(value.clone()),
            ));
        }
    }
}

fn measurement_comparison(slot: MeasurementSlot, plan: &BrewPlanV1) -> BrewMeasurementComparison {
    let target = slot.target;
    let selected = slot.current;
    let (comparable, warning) = if target.is_none() {
        (false, Some("no_planned_target_for_basis".to_owned()))
    } else if plan
        .assumptions
        .iter()
        .all(|assumption| assumption != "volume_reference_20c")
    {
        (
            false,
            Some("planned_reference_temperature_unknown".to_owned()),
        )
    } else if slot.basis == "volume" {
        match selected
            .as_ref()
            .and_then(|entry| entry.metadata.sample_temperature_c)
        {
            Some(temperature) if (temperature - 20.0).abs() > 1e-9 => {
                (false, Some("unlike_sample_temperature".to_owned()))
            }
            Some(_) => (true, None),
            None => (false, Some("measurement_temperature_unknown".to_owned())),
        }
    } else if slot.basis == "specific_gravity" {
        match selected.as_ref() {
            None => (
                false,
                Some("measurement_reference_temperature_unknown".to_owned()),
            ),
            Some(entry) => {
                let reference = entry.metadata.reference_temperature_c;
                let sample = entry.metadata.sample_temperature_c;
                match (reference, sample) {
                    (Some(reference), _) if (reference - 20.0).abs() > 1e-9 => {
                        (false, Some("unlike_reference_temperature".to_owned()))
                    }
                    (None, _) => (
                        false,
                        Some("measurement_reference_temperature_unknown".to_owned()),
                    ),
                    (Some(_), None) => (false, Some("measurement_temperature_unknown".to_owned())),
                    (Some(_), Some(sample)) if (sample - 20.0).abs() > 1e-9 => {
                        (false, Some("unlike_sample_temperature".to_owned()))
                    }
                    (Some(_), Some(_)) => (true, None),
                }
            }
        }
    } else {
        (true, None)
    };
    let delta = if comparable {
        match (target, selected.as_ref().and_then(|entry| entry.value)) {
            (Some(target), Some(value)) => Some(value - target),
            _ => None,
        }
    } else {
        None
    };
    BrewMeasurementComparison {
        quantity_id: slot.quantity_id,
        label: slot.label,
        unit: slot.unit,
        stage: slot.stage,
        basis: slot.basis,
        target,
        selected,
        comparable,
        delta,
        warning,
    }
}

fn comparison_changes(
    plan: &BrewPlanV1,
    actual: Option<&ActualFormulationV1>,
) -> Vec<BrewFormulationChange> {
    let Some(actual) = actual else {
        return Vec::new();
    };
    let recipe = &plan.recipe;
    let mut out = Vec::new();
    let planned: Vec<_> = recipe
        .fermentables
        .iter()
        .map(|row| (row.row_id.clone(), value_for_fermentable(row)))
        .collect();
    let current: Vec<_> = actual
        .fermentables
        .iter()
        .map(|row| (row.row_id.clone(), value_for_fermentable(row)))
        .collect();
    ingredient_changes(
        BrewHistorySection::Fermentable,
        "fermentables",
        &planned,
        &current,
        &mut out,
    );
    let planned: Vec<_> = recipe
        .hops
        .iter()
        .map(|row| (row.row_id.clone(), value_for_hop(row)))
        .collect();
    let current: Vec<_> = actual
        .hops
        .iter()
        .map(|row| (row.row_id.clone(), value_for_hop(row)))
        .collect();
    ingredient_changes(
        BrewHistorySection::Hop,
        "hops",
        &planned,
        &current,
        &mut out,
    );
    let planned: Vec<_> = recipe
        .yeast
        .iter()
        .map(|row| (row.row_id.clone(), value_for_other(row)))
        .collect();
    let current: Vec<_> = actual
        .yeast
        .iter()
        .map(|row| (row.row_id.clone(), value_for_other(row)))
        .collect();
    ingredient_changes(
        BrewHistorySection::Yeast,
        "yeast",
        &planned,
        &current,
        &mut out,
    );
    let planned: Vec<_> = recipe
        .other_ingredients
        .iter()
        .map(|row| (row.row_id.clone(), value_for_other(row)))
        .collect();
    let current: Vec<_> = actual
        .other_ingredients
        .iter()
        .map(|row| (row.row_id.clone(), value_for_other(row)))
        .collect();
    ingredient_changes(
        BrewHistorySection::OtherIngredient,
        "other_ingredients",
        &planned,
        &current,
        &mut out,
    );

    fn scalar_change(
        out: &mut Vec<BrewFormulationChange>,
        field: &str,
        planned: Option<f64>,
        current: Option<f64>,
        text: bool,
    ) {
        if planned != current {
            out.push(change(
                format!("process:{field}"),
                BrewHistorySection::Process,
                None,
                Some(field.into()),
                BrewChangeKind::Changed,
                Some(BrewComparisonValue {
                    amount: planned,
                    text: if text {
                        planned.map(|value| value.to_string())
                    } else {
                        None
                    },
                    ..Default::default()
                }),
                Some(BrewComparisonValue {
                    amount: current,
                    text: if text {
                        current.map(|value| value.to_string())
                    } else {
                        None
                    },
                    ..Default::default()
                }),
            ));
        }
    }
    scalar_change(
        &mut out,
        "boil_minutes",
        recipe.boil_minutes,
        actual.boil_minutes,
        false,
    );
    if recipe.process.apparent_attenuation_fraction != actual.process.apparent_attenuation_fraction
    {
        out.push(change(
            "process:apparent_attenuation_fraction".into(),
            BrewHistorySection::Process,
            None,
            Some("apparent_attenuation_fraction".into()),
            BrewChangeKind::Changed,
            Some(BrewComparisonValue {
                amount: recipe.process.apparent_attenuation_fraction,
                ..Default::default()
            }),
            Some(BrewComparisonValue {
                amount: actual.process.apparent_attenuation_fraction,
                ..Default::default()
            }),
        ));
    }
    scalar_change(
        &mut out,
        "whirlpool_minutes",
        recipe.process.whirlpool_minutes,
        actual.process.whirlpool_minutes,
        false,
    );
    scalar_change(
        &mut out,
        "whirlpool_temperature_c",
        recipe.process.whirlpool_temperature_c,
        actual.process.whirlpool_temperature_c,
        false,
    );
    scalar_change(
        &mut out,
        "mash_water_l",
        recipe.process.mash_water_l,
        actual.process.mash_water_l,
        false,
    );
    if recipe.process.mash_water_method != actual.process.mash_water_method {
        out.push(change(
            "process:mash_water_method".into(),
            BrewHistorySection::Process,
            None,
            Some("mash_water_method".into()),
            BrewChangeKind::Changed,
            Some(BrewComparisonValue {
                text: recipe.process.mash_water_method.clone(),
                ..Default::default()
            }),
            Some(BrewComparisonValue {
                text: actual.process.mash_water_method.clone(),
                ..Default::default()
            }),
        ));
    }
    scalar_change(
        &mut out,
        "kettle_top_up_l",
        recipe.process.kettle_top_up_l,
        actual.process.kettle_top_up_l,
        false,
    );

    fn steps(
        section: BrewHistorySection,
        prefix: &str,
        planned: &[ProcessStep],
        actual: &[ProcessStep],
        out: &mut Vec<BrewFormulationChange>,
    ) {
        let planned: Vec<_> = planned
            .iter()
            .map(|row| (row.row_id.clone(), value_for_step(row)))
            .collect();
        let actual: Vec<_> = actual
            .iter()
            .map(|row| (row.row_id.clone(), value_for_step(row)))
            .collect();
        ingredient_changes(section, prefix, &planned, &actual, out);
    }
    steps(
        BrewHistorySection::MashStep,
        "mash_steps",
        &recipe.process.mash_steps,
        &actual.process.mash_steps,
        &mut out,
    );
    steps(
        BrewHistorySection::FermentationStep,
        "fermentation_steps",
        &recipe.process.fermentation_steps,
        &actual.process.fermentation_steps,
        &mut out,
    );
    if recipe.extensions != actual.extensions {
        out.push(change(
            "extensions".into(),
            BrewHistorySection::Extensions,
            None,
            None,
            BrewChangeKind::Changed,
            Some(BrewComparisonValue {
                canonical_digest: canonical_digest(&recipe.extensions),
                text: Some("document extensions differ".into()),
                ..Default::default()
            }),
            Some(BrewComparisonValue {
                canonical_digest: canonical_digest(&actual.extensions),
                text: Some("document extensions differ".into()),
                ..Default::default()
            }),
        ));
    }
    if recipe.process.unsupported != actual.process.unsupported {
        out.push(change(
            "process:unsupported".into(),
            BrewHistorySection::Process,
            None,
            Some("unsupported".into()),
            BrewChangeKind::Changed,
            Some(BrewComparisonValue {
                canonical_digest: canonical_digest(&recipe.process.unsupported),
                text: Some("unsupported process data differs".into()),
                ..Default::default()
            }),
            Some(BrewComparisonValue {
                canonical_digest: canonical_digest(&actual.process.unsupported),
                text: Some("unsupported process data differs".into()),
                ..Default::default()
            }),
        ));
    }
    if recipe.notes != actual.notes {
        out.push(change(
            "notes".into(),
            BrewHistorySection::Notes,
            None,
            None,
            BrewChangeKind::Changed,
            Some(BrewComparisonValue {
                text: recipe.notes.clone(),
                ..Default::default()
            }),
            Some(BrewComparisonValue {
                text: actual.notes.clone(),
                ..Default::default()
            }),
        ));
    }
    out
}

fn selected_row_ids<'a>(selected: &'a BTreeSet<String>, prefix: &str) -> BTreeSet<&'a str> {
    selected
        .iter()
        .filter_map(|id| id.strip_prefix(prefix))
        .collect()
}

fn apply_changes(
    base: &RecipeDocumentV1,
    plan: &BrewPlanV1,
    actual: &ActualFormulationV1,
    selected: &BTreeSet<String>,
) -> Result<(RecipeDocumentV1, Vec<String>), Error> {
    let all = comparison_changes(plan, Some(actual));
    let known: BTreeSet<&str> = all.iter().map(|change| change.id.as_str()).collect();
    if selected.iter().any(|id| !known.contains(id.as_str())) {
        return Err(Error::Invalid("change selection"));
    }
    let mut document = base.clone();
    let mut diagnostics = Vec::new();
    let selected_prefix = |prefix: &str| selected_row_ids(selected, prefix);

    let grain_rows = selected_prefix("fermentables:");
    if !grain_rows.is_empty() {
        for row in &actual.fermentables {
            if !grain_rows.contains(row.row_id.as_str()) {
                continue;
            }
            if let Some(current) = document
                .fermentables
                .iter_mut()
                .find(|current| current.row_id == row.row_id)
            {
                if current.specification != row.specification {
                    diagnostics.push(format!(
                        "fermentables:{}: recipe specification preserved; actual specification not copied",
                        row.row_id
                    ));
                }
                if current.source != row.source {
                    diagnostics.push(format!(
                        "fermentables:{}: source provenance retained as evidence and not applied",
                        row.row_id
                    ));
                }
                current.amount_kg = row.amount_kg;
                current.addition = row.addition.clone();
            } else {
                if row.source.is_some() {
                    diagnostics.push(format!(
                        "fermentables:{}: source provenance cannot be introduced by feedback",
                        row.row_id
                    ));
                }
                document.fermentables.push(row.clone());
            }
        }
        document.fermentables.retain(|row| {
            !grain_rows.contains(row.row_id.as_str())
                || actual
                    .fermentables
                    .iter()
                    .any(|actual_row| actual_row.row_id == row.row_id)
        });
    }
    let hop_rows = selected_prefix("hops:");
    if !hop_rows.is_empty() {
        for row in &actual.hops {
            if !hop_rows.contains(row.row_id.as_str()) {
                continue;
            }
            if let Some(current) = document
                .hops
                .iter_mut()
                .find(|current| current.row_id == row.row_id)
            {
                if current.specification != row.specification {
                    diagnostics.push(format!(
                        "hops:{}: recipe specification preserved; actual specification not copied",
                        row.row_id
                    ));
                }
                if current.source != row.source {
                    diagnostics.push(format!(
                        "hops:{}: source provenance retained as evidence and not applied",
                        row.row_id
                    ));
                }
                current.amount_g = row.amount_g;
                current.stage = row.stage.clone();
                current.contact_minutes = row.contact_minutes;
                current.temperature_c = row.temperature_c;
            } else {
                if row.source.is_some() {
                    diagnostics.push(format!(
                        "hops:{}: source provenance cannot be introduced by feedback",
                        row.row_id
                    ));
                }
                document.hops.push(row.clone());
            }
        }
        document.hops.retain(|row| {
            !hop_rows.contains(row.row_id.as_str())
                || actual
                    .hops
                    .iter()
                    .any(|actual_row| actual_row.row_id == row.row_id)
        });
    }
    let yeast_rows = selected_prefix("yeast:");
    if !yeast_rows.is_empty() {
        for row in &actual.yeast {
            if !yeast_rows.contains(row.row_id.as_str()) {
                continue;
            }
            if let Some(current) = document
                .yeast
                .iter_mut()
                .find(|current| current.row_id == row.row_id)
            {
                if current.specification != row.specification {
                    diagnostics.push(format!(
                        "yeast:{}: recipe specification preserved; actual specification not copied",
                        row.row_id
                    ));
                }
                if current.source != row.source {
                    diagnostics.push(format!(
                        "yeast:{}: source provenance retained as evidence and not applied",
                        row.row_id
                    ));
                }
                current.amount = row.amount;
                current.unit = row.unit.clone();
            } else {
                if row.source.is_some() {
                    diagnostics.push(format!(
                        "yeast:{}: source provenance cannot be introduced by feedback",
                        row.row_id
                    ));
                }
                document.yeast.push(row.clone());
            }
        }
        document.yeast.retain(|row| {
            !yeast_rows.contains(row.row_id.as_str())
                || actual
                    .yeast
                    .iter()
                    .any(|actual_row| actual_row.row_id == row.row_id)
        });
    }
    let other_rows = selected_prefix("other_ingredients:");
    if !other_rows.is_empty() {
        for row in &actual.other_ingredients {
            if !other_rows.contains(row.row_id.as_str()) {
                continue;
            }
            if let Some(current) = document
                .other_ingredients
                .iter_mut()
                .find(|current| current.row_id == row.row_id)
            {
                if current.specification != row.specification {
                    diagnostics.push(format!(
                        "other_ingredients:{}: recipe specification preserved; actual specification not copied",
                        row.row_id
                    ));
                }
                if current.source != row.source {
                    diagnostics.push(format!(
                        "other_ingredients:{}: source provenance retained as evidence and not applied",
                        row.row_id
                    ));
                }
                current.amount = row.amount;
                current.unit = row.unit.clone();
            } else {
                if row.source.is_some() {
                    diagnostics.push(format!(
                        "other_ingredients:{}: source provenance cannot be introduced by feedback",
                        row.row_id
                    ));
                }
                document.other_ingredients.push(row.clone());
            }
        }
        document.other_ingredients.retain(|row| {
            !other_rows.contains(row.row_id.as_str())
                || actual
                    .other_ingredients
                    .iter()
                    .any(|actual_row| actual_row.row_id == row.row_id)
        });
    }
    if selected.contains("process:boil_minutes") {
        document.boil_minutes = actual.boil_minutes;
    }
    if selected.contains("process:apparent_attenuation_fraction") {
        document.process.apparent_attenuation_fraction =
            actual.process.apparent_attenuation_fraction;
    }
    if selected.contains("process:whirlpool_minutes") {
        document.process.whirlpool_minutes = actual.process.whirlpool_minutes;
    }
    if selected.contains("process:whirlpool_temperature_c") {
        document.process.whirlpool_temperature_c = actual.process.whirlpool_temperature_c;
    }
    if selected.contains("process:mash_water_l") {
        document.process.mash_water_l = actual.process.mash_water_l;
    }
    if selected.contains("process:mash_water_method") {
        document.process.mash_water_method = actual.process.mash_water_method.clone();
    }
    if selected.contains("process:kettle_top_up_l") {
        document.process.kettle_top_up_l = actual.process.kettle_top_up_l;
    }
    let mash_rows = selected_prefix("mash_steps:");
    if !mash_rows.is_empty() {
        for row in &actual.process.mash_steps {
            if !mash_rows.contains(row.row_id.as_str()) {
                continue;
            }
            if let Some(current) = document
                .process
                .mash_steps
                .iter_mut()
                .find(|current| current.row_id == row.row_id)
            {
                *current = row.clone();
            } else {
                document.process.mash_steps.push(row.clone());
            }
        }
        document.process.mash_steps.retain(|row| {
            !mash_rows.contains(row.row_id.as_str())
                || actual
                    .process
                    .mash_steps
                    .iter()
                    .any(|actual_row| actual_row.row_id == row.row_id)
        });
    }
    let ferm_rows = selected_prefix("fermentation_steps:");
    if !ferm_rows.is_empty() {
        for row in &actual.process.fermentation_steps {
            if !ferm_rows.contains(row.row_id.as_str()) {
                continue;
            }
            if let Some(current) = document
                .process
                .fermentation_steps
                .iter_mut()
                .find(|current| current.row_id == row.row_id)
            {
                *current = row.clone();
            } else {
                document.process.fermentation_steps.push(row.clone());
            }
        }
        document.process.fermentation_steps.retain(|row| {
            !ferm_rows.contains(row.row_id.as_str())
                || actual
                    .process
                    .fermentation_steps
                    .iter()
                    .any(|actual_row| actual_row.row_id == row.row_id)
        });
    }
    if selected.contains("notes") {
        document.notes = actual.notes.clone();
    }
    if selected.contains("extensions") {
        diagnostics.push(
            "document extensions are retained as plan evidence and are not applied to the recipe"
                .into(),
        );
    }
    if selected.contains("process:unsupported") {
        diagnostics.push(
            "unsupported process data is retained as evidence and is not applied to the recipe"
                .into(),
        );
    }
    Ok((document, diagnostics))
}

fn preview_request(intent: OperationIntent, draft: RecipeDocumentV1) -> PreviewRequest {
    PreviewRequest {
        draft_generation: "brew-feedback".into(),
        intent,
        draft,
        equipment_draft: Default::default(),
    }
}

fn map_preview(preview: PreviewOutcome) -> Result<Preview, BrewFeedbackPreviewOutcome> {
    match preview {
        PreviewOutcome::Preview(preview) => Ok(preview),
        PreviewOutcome::UnresolvedProfile => Err(BrewFeedbackPreviewOutcome::UnresolvedProfile),
        PreviewOutcome::UnresolvedStyle => Err(BrewFeedbackPreviewOutcome::UnresolvedStyle),
        PreviewOutcome::Forbidden => Err(BrewFeedbackPreviewOutcome::Forbidden),
        PreviewOutcome::ContextChanged => Err(BrewFeedbackPreviewOutcome::ContextChanged),
        PreviewOutcome::Invalid | PreviewOutcome::Unavailable => {
            Err(BrewFeedbackPreviewOutcome::Unavailable)
        }
    }
}

pub async fn feedback_preview(
    pool: &DbPool,
    actor: &ActorContext,
    request: BrewFeedbackPreviewRequest,
) -> BrewFeedbackPreviewOutcome {
    match feedback_preview_inner(pool, actor, request).await {
        Ok(outcome) => outcome,
        Err(Error::Forbidden) => BrewFeedbackPreviewOutcome::Forbidden,
        Err(Error::ContextChanged) => BrewFeedbackPreviewOutcome::ContextChanged,
        Err(Error::Invalid(_)) => BrewFeedbackPreviewOutcome::InvalidRequest,
        Err(error) => {
            tracing::warn!(%error, "brew feedback preview unavailable");
            BrewFeedbackPreviewOutcome::Unavailable
        }
    }
}

async fn feedback_preview_inner(
    pool: &DbPool,
    actor: &ActorContext,
    request: BrewFeedbackPreviewRequest,
) -> Result<BrewFeedbackPreviewOutcome, Error> {
    validate_id(&request.brew_id, "brew")?;
    let selected = validate_changes(&request.change_ids)?;
    let mut tx = begin(pool, actor, &request.intent).await?;
    let Some(brew) = brew_store::load(&mut tx, &actor.workspace_id, &request.brew_id)
        .await
        .map_err(boundary_error)?
    else {
        return Ok(BrewFeedbackPreviewOutcome::NotFound);
    };
    let Some(state) = evidence_store::load_state(&mut tx, &actor.workspace_id, &request.brew_id)
        .await
        .map_err(boundary_error)?
    else {
        return Ok(BrewFeedbackPreviewOutcome::NotFound);
    };
    let Some(actual_revision) = state.actual_revision else {
        return Ok(BrewFeedbackPreviewOutcome::NoActual);
    };
    if request.expected_actual_revision != Some(actual_revision) {
        return Ok(BrewFeedbackPreviewOutcome::Conflict {
            expected: request.expected_actual_revision,
            actual: Some(actual_revision),
        });
    }
    let Some(actual) = evidence_store::load_actual_formulation(
        &mut tx,
        &actor.workspace_id,
        &request.brew_id,
        actual_revision,
    )
    .await
    .map_err(boundary_error)?
    else {
        return Ok(BrewFeedbackPreviewOutcome::NoActual);
    };
    let Some((head, document)) = recipe_head(&mut tx, actor, &brew.recipe_id, false).await? else {
        return Ok(BrewFeedbackPreviewOutcome::NotFound);
    };
    let (proposed, diagnostics) =
        match apply_changes(&document, &brew.plan, &actual.document, &selected) {
            Ok(result) => result,
            Err(Error::Invalid(_)) => return Ok(BrewFeedbackPreviewOutcome::InvalidRequest),
            Err(error) => return Err(error),
        };
    if proposed == document {
        return Ok(BrewFeedbackPreviewOutcome::NoChanges { diagnostics });
    }
    let preview = crate::formulation_kernel::resolve_in_tx(
        &mut tx,
        actor,
        preview_request(request.intent.clone(), proposed.clone()),
    )
    .await?;
    let preview = match map_preview(preview) {
        Ok(preview) => preview,
        Err(outcome) => return Ok(outcome),
    };
    tx.commit().await.map_err(boundary_error)?;
    Ok(BrewFeedbackPreviewOutcome::Preview(BrewFeedbackPreview {
        brew_id: brew.brew_id,
        recipe_id: brew.recipe_id,
        source_actual_revision: actual_revision,
        base_recipe_head: head,
        proposed_recipe: proposed,
        applied_change_ids: request.change_ids,
        preview,
        diagnostics,
    }))
}

pub async fn feedback_apply(
    pool: &DbPool,
    actor: &ActorContext,
    request: BrewFeedbackApplyRequest,
) -> BrewFeedbackApplyOutcome {
    match feedback_apply_inner(pool, actor, request).await {
        Ok(outcome) => outcome,
        Err(Error::Forbidden) => BrewFeedbackApplyOutcome::Forbidden,
        Err(Error::ContextChanged) => BrewFeedbackApplyOutcome::ContextChanged,
        Err(Error::RequestIdentityConflict) => BrewFeedbackApplyOutcome::RequestIdentityConflict,
        Err(Error::ResultExpired { resource_id }) => {
            BrewFeedbackApplyOutcome::ResultExpired { resource_id }
        }
        Err(Error::Retryable) => BrewFeedbackApplyOutcome::Retryable,
        Err(Error::Invalid(_)) => BrewFeedbackApplyOutcome::InvalidRequest,
        Err(error) => {
            tracing::warn!(%error, "brew feedback apply unavailable");
            BrewFeedbackApplyOutcome::Unavailable
        }
    }
}

async fn feedback_apply_inner(
    pool: &DbPool,
    actor: &ActorContext,
    request: BrewFeedbackApplyRequest,
) -> Result<BrewFeedbackApplyOutcome, Error> {
    validate_id(&request.brew_id, "brew")?;
    let selected = validate_changes(&request.change_ids)?;
    if request.expected_recipe_head < 1 || request.expected_calculation_context.is_empty() {
        return Err(Error::Invalid("feedback request"));
    }
    uuid::Uuid::parse_str(&request.request_id).map_err(|_| Error::Invalid("request id"))?;
    let payload = canonical_json::from_serializable(&(
        &request.brew_id,
        request.expected_actual_revision,
        &request.change_ids,
        request.expected_recipe_head,
        &request.expected_calculation_context,
    ))
    .map_err(|_| Error::Invalid("feedback request"))?;
    let digest =
        canonical_json::sha256(&payload).map_err(|_| Error::Invalid("feedback request"))?;

    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, &request.intent)?;
    let ws_sql = if matches!(&tx, DbTx::Postgres(_)) {
        "SELECT revision FROM workspaces WHERE workspace_id = $1 AND status = 'active' FOR UPDATE"
    } else {
        "SELECT revision FROM workspaces WHERE workspace_id = $1 AND status = 'active'"
    };
    if scalar!(&mut tx, i64, ws_sql, &actor.workspace_id)
        .map_err(db_error)?
        .is_none()
    {
        return Err(Error::Forbidden);
    }
    authorize_workspace(&mut tx, actor).await?;
    let key = RequestIdentity {
        actor,
        operation: "brew.feedback.apply.v1",
        request_id: &request.request_id,
        payload_digest: &digest,
    };
    if let Some(result) = recover_request(&mut tx, &key).await? {
        let outcome = serde_json::from_value(result)?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(outcome);
    }

    let Some(brew) = brew_store::load(&mut tx, &actor.workspace_id, &request.brew_id)
        .await
        .map_err(boundary_error)?
    else {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(BrewFeedbackApplyOutcome::NotFound);
    };
    let Some(state) = evidence_store::load_state(&mut tx, &actor.workspace_id, &request.brew_id)
        .await
        .map_err(boundary_error)?
    else {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(BrewFeedbackApplyOutcome::NotFound);
    };
    let actual_revision = state.actual_revision;
    if actual_revision != request.expected_actual_revision {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(BrewFeedbackApplyOutcome::ActualConflict {
            expected: request.expected_actual_revision,
            actual: actual_revision,
        });
    }
    let Some(actual_revision) = actual_revision else {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(BrewFeedbackApplyOutcome::NoActual);
    };
    let Some(actual) = evidence_store::load_actual_formulation(
        &mut tx,
        &actor.workspace_id,
        &request.brew_id,
        actual_revision,
    )
    .await
    .map_err(boundary_error)?
    else {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(BrewFeedbackApplyOutcome::NoActual);
    };
    let Some((head, document)) = recipe_head(&mut tx, actor, &brew.recipe_id, false).await? else {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(BrewFeedbackApplyOutcome::NotFound);
    };
    if head != request.expected_recipe_head {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(BrewFeedbackApplyOutcome::HeadConflict {
            expected: request.expected_recipe_head,
            actual: head,
        });
    }
    let (proposed, diagnostics) =
        match apply_changes(&document, &brew.plan, &actual.document, &selected) {
            Ok(result) => result,
            Err(Error::Invalid(_)) => {
                tx.rollback().await.map_err(boundary_error)?;
                return Ok(BrewFeedbackApplyOutcome::InvalidRequest);
            }
            Err(error) => return Err(error),
        };
    if proposed == document {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(BrewFeedbackApplyOutcome::NoChanges { diagnostics });
    }
    let preview = crate::formulation_kernel::resolve_in_tx(
        &mut tx,
        actor,
        preview_request(request.intent.clone(), proposed.clone()),
    )
    .await?;
    let preview = match preview {
        PreviewOutcome::Preview(preview) => preview,
        PreviewOutcome::UnresolvedProfile => {
            tx.rollback().await.map_err(boundary_error)?;
            return Ok(BrewFeedbackApplyOutcome::UnresolvedProfile);
        }
        PreviewOutcome::UnresolvedStyle => {
            tx.rollback().await.map_err(boundary_error)?;
            return Ok(BrewFeedbackApplyOutcome::UnresolvedStyle);
        }
        PreviewOutcome::Forbidden => {
            tx.rollback().await.map_err(boundary_error)?;
            return Ok(BrewFeedbackApplyOutcome::Forbidden);
        }
        PreviewOutcome::ContextChanged => {
            tx.rollback().await.map_err(boundary_error)?;
            return Ok(BrewFeedbackApplyOutcome::ContextChanged);
        }
        PreviewOutcome::Invalid | PreviewOutcome::Unavailable => {
            tx.rollback().await.map_err(boundary_error)?;
            return Ok(BrewFeedbackApplyOutcome::Unavailable);
        }
    };
    if preview.validation.state == Readiness::Invalid {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(BrewFeedbackApplyOutcome::Invalid(preview.validation));
    }
    if preview.context_digest.as_deref() != Some(request.expected_calculation_context.as_str()) {
        let refreshed = preview;
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(BrewFeedbackApplyOutcome::ContextConflict { refreshed });
    }
    // Lock order for a recipe append: workspace (held), profile (taken by the
    // resolver), brew evidence aggregate, then the recipe head.
    if evidence_store::lock(&mut tx, &actor.workspace_id, &request.brew_id)
        .await
        .map_err(boundary_error)?
        .is_none()
    {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(BrewFeedbackApplyOutcome::NotFound);
    }
    let Some((locked_head, _)) = recipe_head(&mut tx, actor, &brew.recipe_id, true).await? else {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(BrewFeedbackApplyOutcome::NotFound);
    };
    authorize_workspace(&mut tx, actor).await?;
    if locked_head != request.expected_recipe_head {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(BrewFeedbackApplyOutcome::HeadConflict {
            expected: request.expected_recipe_head,
            actual: locked_head,
        });
    }
    let current_actual =
        evidence_store::load_current_actual(&mut tx, &actor.workspace_id, &request.brew_id)
            .await
            .map_err(boundary_error)?;
    if current_actual.as_ref().map(|actual| actual.revision) != Some(actual_revision) {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(BrewFeedbackApplyOutcome::ActualConflict {
            expected: Some(actual_revision),
            actual: current_actual.map(|actual| actual.revision),
        });
    }
    // A new source reference cannot be introduced by feedback; copied plan
    // sources must already exist on the current head row.
    if !source_refs_preserved(&proposed, &document) {
        tx.rollback().await.map_err(boundary_error)?;
        return Ok(BrewFeedbackApplyOutcome::UnresolvedIngredientSource);
    }
    let saved = native_store::save(
        &mut tx,
        &actor.workspace_id,
        &brew.recipe_id,
        locked_head,
        &proposed,
        &actor.actor_id,
        chrono::Utc::now(),
        None,
    )
    .await
    .map_err(boundary_error)?;
    let recipe_revision = match saved {
        native_store::SaveResult::Unchanged(revision) => {
            tx.commit().await.map_err(boundary_error)?;
            return Ok(BrewFeedbackApplyOutcome::Unchanged {
                brew_id: brew.brew_id,
                recipe_id: brew.recipe_id,
                recipe_revision: revision,
            });
        }
        native_store::SaveResult::Appended(revision) => revision,
    };
    let outcome = BrewFeedbackApplyOutcome::Applied {
        brew_id: brew.brew_id.clone(),
        recipe_id: brew.recipe_id.clone(),
        source_actual_revision: actual_revision,
        recipe_revision,
    };
    record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&outcome)?,
        Some(&brew.brew_id),
    )
    .await?;
    record_change(
        &mut tx,
        actor,
        "brew.feedback.apply.v1",
        &brew.recipe_id,
        recipe_revision,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(outcome)
}

fn source_refs_preserved(proposed: &RecipeDocumentV1, previous: &RecipeDocumentV1) -> bool {
    macro_rules! preserved {
        ($field:ident) => {
            proposed.$field.iter().all(|row| {
                row.source.is_none()
                    || previous
                        .$field
                        .iter()
                        .any(|prior| prior.row_id == row.row_id && prior.source == row.source)
            })
        };
    }
    preserved!(fermentables)
        && preserved!(hops)
        && preserved!(yeast)
        && preserved!(other_ingredients)
}

pub async fn historical_print(
    pool: &DbPool,
    actor: &ActorContext,
    request: BrewPrintRequest,
) -> BrewPrintOutcome {
    match historical_print_inner(pool, actor, request).await {
        Ok(outcome) => outcome,
        Err(Error::Forbidden) => BrewPrintOutcome::Forbidden,
        Err(Error::ContextChanged) => BrewPrintOutcome::ContextChanged,
        Err(Error::Invalid(_)) => BrewPrintOutcome::InvalidRequest,
        Err(error) => {
            tracing::warn!(%error, "brew print unavailable");
            BrewPrintOutcome::Unavailable
        }
    }
}

async fn historical_print_inner(
    pool: &DbPool,
    actor: &ActorContext,
    request: BrewPrintRequest,
) -> Result<BrewPrintOutcome, Error> {
    validate_id(&request.brew_id, "brew")?;
    let view = crate::brew_evidence::view(
        pool,
        actor,
        BrewEvidenceViewRequest {
            intent: request.intent,
            brew_id: request.brew_id,
            lifecycle_cursor: None,
        },
    )
    .await;
    let view = match view {
        BrewEvidenceViewOutcome::View(view) => view,
        BrewEvidenceViewOutcome::NotFound => return Ok(BrewPrintOutcome::NotFound),
        BrewEvidenceViewOutcome::Forbidden => return Ok(BrewPrintOutcome::Forbidden),
        BrewEvidenceViewOutcome::ContextChanged => return Ok(BrewPrintOutcome::ContextChanged),
        BrewEvidenceViewOutcome::Invalid => return Ok(BrewPrintOutcome::InvalidRequest),
        BrewEvidenceViewOutcome::Retryable | BrewEvidenceViewOutcome::Unavailable => {
            return Ok(BrewPrintOutcome::Unavailable);
        }
    };
    let summary = summary_from_plan(&view);
    Ok(BrewPrintOutcome::Print(BrewPrint {
        brew_id: view.brew_id,
        recipe_id: view.recipe_id,
        selected_recipe_revision: view.selected_recipe_revision,
        applied_defaults: view.plan.applied_defaults.clone(),
        actual: view.actual,
        measurements: view.measurements,
        summary,
    }))
}

fn readiness(diagnostics: &[Diagnostic]) -> Readiness {
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Readiness::Invalid)
    {
        Readiness::Invalid
    } else if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Readiness::Incomplete)
    {
        Readiness::Incomplete
    } else {
        Readiness::Ready
    }
}

fn summary_from_plan(view: &krausen_types::brew::BrewEvidenceView) -> ProcessSummary {
    let plan = &view.plan;
    let recipe = &plan.recipe;
    let mut fermentables = Vec::new();
    for row in &recipe.fermentables {
        fermentables.push(SummaryIngredient {
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
        });
    }
    let hops = recipe
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
        .collect();
    let yeast = recipe
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
        .collect();
    let other_ingredients = recipe
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
        .collect();
    let steps = |rows: &[ProcessStep]| {
        rows.iter()
            .map(|row| SummaryStep {
                row_id: row.row_id.clone(),
                name: row.name.clone(),
                time_minutes: row.time_minutes,
                temperature_c: row.temperature_c,
                volume_l: row.volume_l,
            })
            .collect()
    };
    ProcessSummary {
        source: SummarySource::HistoricalBrew {
            brew_id: view.brew_id.clone(),
            recipe_id: view.recipe_id.clone(),
            selected_recipe_revision: view.selected_recipe_revision,
        },
        title: recipe.title.clone(),
        description: recipe.description.clone(),
        style: recipe.style.clone(),
        batch_size_l: recipe.batch_size_l,
        boil_minutes: recipe.boil_minutes,
        notes: recipe.notes.clone(),
        readiness: readiness(&plan.diagnostics),
        diagnostics: plan.diagnostics.clone(),
        targets: plan.original_targets.clone(),
        style_ranges: plan.style_reference.clone(),
        style_comparison: plan.style_comparison.clone(),
        assumptions: plan.assumptions.clone(),
        equipment: SummaryEquipment {
            profile_id: plan.equipment_profile_id.clone(),
            profile_name: None,
            profile_revision: plan.equipment_profile_revision,
            provenance: plan.calculation_context.equipment_provenance.clone(),
            settings: plan.equipment.clone(),
        },
        context: SummaryContext {
            digest: Some(plan.context_id.clone()),
            engine_version: plan.engine_version.clone(),
            engine_build: plan.engine_build.clone(),
            policy_version: plan.policy_version.clone(),
            native_schema_version: plan.native_schema_version,
            result_schema_version: plan.result_schema_version,
        },
        fermentables,
        hops,
        yeast,
        other_ingredients,
        process: SummaryProcess {
            apparent_attenuation_fraction: recipe.process.apparent_attenuation_fraction,
            mash_steps: steps(&recipe.process.mash_steps),
            fermentation_steps: steps(&recipe.process.fermentation_steps),
            whirlpool_minutes: recipe.process.whirlpool_minutes,
            whirlpool_temperature_c: recipe.process.whirlpool_temperature_c,
            mash_water_l: recipe.process.mash_water_l,
            mash_water_method: recipe.process.mash_water_method.clone(),
            kettle_top_up_l: recipe.process.kettle_top_up_l,
            chilling: plan.equipment.chilling.clone(),
            unsupported: recipe.process.unsupported.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_row_ids_strip_exactly_one_namespace_prefix() {
        let mut selected = BTreeSet::new();
        selected.insert("fermentables:fermentables:x".to_owned());
        selected.insert("fermentables:y".to_owned());
        let rows = selected_row_ids(&selected, "fermentables:");
        assert!(rows.contains("fermentables:x"));
        assert!(!rows.contains("x"));
        assert!(rows.contains("y"));
    }
}
