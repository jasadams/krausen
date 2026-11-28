// SPDX-License-Identifier: AGPL-3.0-or-later
//! Authorized per-user settings. They live in the user's `extra_metadata` JSON beside the theme
//! preference; every statement here runs on the caller's `DbTx`, so a write owns one transaction
//! and a new recipe reads the default inside its own save transaction.
use crate::{ActorContext, Error, authorize_workspace, boundary_error, check_intent, db_error};
use krausen_core::{DbPool, DbTx};
use krausen_types::{
    brewery::RecipeDocumentV1,
    request_context::OperationIntent,
    user_settings::{UserSettings, UserSettingsOutcome},
    validation::Readiness,
};

/// The user's default batch size, if one is set. Runs on the caller's transaction.
pub(crate) async fn default_batch_size(
    tx: &mut DbTx<'_>,
    user_id: &str,
) -> Result<Option<f64>, Error> {
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT CAST(extra_metadata::jsonb ->> 'default_batch_size_l' AS DOUBLE PRECISION) FROM users WHERE user_id = $1"
    } else {
        "SELECT json_extract(extra_metadata, '$.default_batch_size_l') FROM users WHERE user_id = $1"
    };
    Ok(scalar!(tx, Option<f64>, sql, user_id)
        .map_err(db_error)?
        .flatten())
}

/// A default batch size is valid when a recipe may carry it as its own batch size.
fn validate(value: f64) -> Result<(), Error> {
    let document = RecipeDocumentV1 {
        batch_size_l: Some(value),
        ..RecipeDocumentV1::default()
    };
    let result =
        krausen_brewery::validation::validate_draft(&document, &document.equipment_overrides);
    if result
        .diagnostics
        .iter()
        .any(|d| d.path == "batch_size_l" && d.severity == Readiness::Invalid)
    {
        return Err(Error::Invalid("default batch size"));
    }
    Ok(())
}

/// Read the actor's settings under current authority.
pub async fn get(
    pool: &DbPool,
    actor: &ActorContext,
    intent: &OperationIntent,
) -> Result<UserSettings, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, intent)?;
    let settings = UserSettings {
        default_batch_size_l: default_batch_size(&mut tx, &actor.actor_id).await?,
    };
    tx.commit().await.map_err(boundary_error)?;
    Ok(settings)
}

/// Replace the actor's settings: a `None` value unsets that setting.
pub async fn save(
    pool: &DbPool,
    actor: &ActorContext,
    intent: &OperationIntent,
    settings: &UserSettings,
) -> Result<UserSettings, Error> {
    if let Some(value) = settings.default_batch_size_l {
        validate(value)?;
    }
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    check_intent(actor, intent)?;
    let pg = matches!(tx, DbTx::Postgres(_));
    let now = krausen_core::sql_compat::now(pg);
    let changed = match settings.default_batch_size_l {
        Some(value) => {
            let sql = if pg {
                format!("UPDATE users SET extra_metadata = (COALESCE(extra_metadata::jsonb, '{{}}'::jsonb) || jsonb_build_object('default_batch_size_l', CAST($1 AS DOUBLE PRECISION)))::json, updated_at = {now} WHERE user_id = $2")
            } else {
                format!("UPDATE users SET extra_metadata = json_set(COALESCE(extra_metadata, '{{}}'), '$.default_batch_size_l', $1), updated_at = {now} WHERE user_id = $2")
            };
            query!(&mut tx, &sql, value, &actor.actor_id)
        }
        None => {
            let sql = if pg {
                format!("UPDATE users SET extra_metadata = (COALESCE(extra_metadata::jsonb, '{{}}'::jsonb) - 'default_batch_size_l')::json, updated_at = {now} WHERE user_id = $1")
            } else {
                format!("UPDATE users SET extra_metadata = json_remove(COALESCE(extra_metadata, '{{}}'), '$.default_batch_size_l'), updated_at = {now} WHERE user_id = $1")
            };
            query!(&mut tx, &sql, &actor.actor_id)
        }
    }
    .map_err(db_error)?;
    if changed != 1 {
        return Err(Error::Forbidden);
    }
    tx.commit().await.map_err(boundary_error)?;
    Ok(UserSettings {
        default_batch_size_l: settings.default_batch_size_l,
    })
}

/// Project an application result to the contract both adapters return.
pub fn project(result: Result<UserSettings, Error>) -> UserSettingsOutcome {
    match result {
        Ok(settings) => UserSettingsOutcome::Settings(settings),
        Err(Error::Forbidden) => UserSettingsOutcome::Forbidden,
        Err(Error::ContextChanged) => UserSettingsOutcome::ContextChanged,
        Err(Error::Invalid(_)) => UserSettingsOutcome::Invalid,
        Err(e) => {
            tracing::warn!(error = %e, "user settings operation unavailable");
            UserSettingsOutcome::Unavailable
        }
    }
}
