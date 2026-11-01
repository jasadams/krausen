// SPDX-License-Identifier: AGPL-3.0-or-later
//! Account-owned security status queries shared by adapters.
use crate::{
    Error, boundary_error, db_error,
    invitation::{AccountActor, authorize_account, recheck_credential},
};
use krausen_core::{DbPool, DbTx};
use krausen_types::security_status::{PasswordStatusOutcome as Outcome, TotpStatusOutcome};

pub async fn password_status(pool: &DbPool, actor: &AccountActor) -> Result<bool, Error> {
    method_status(pool, actor, "password").await
}

pub async fn totp_status(pool: &DbPool, actor: &AccountActor) -> Result<bool, Error> {
    method_status(pool, actor, "totp").await
}

async fn method_status(pool: &DbPool, actor: &AccountActor, method: &str) -> Result<bool, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_account(&mut tx, actor).await?;
    let count = scalar!(
        &mut tx,
        i64,
        "SELECT COUNT(*) FROM user_auth_methods WHERE user_id=$1 AND auth_type=$2 AND active=TRUE",
        &actor.actor_id,
        method
    )
    .map_err(db_error)?
    .unwrap_or_default();
    recheck_credential(&mut tx, actor).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(count > 0)
}

pub fn project_totp_status(result: Result<bool, Error>) -> TotpStatusOutcome {
    match result {
        Ok(enabled) => TotpStatusOutcome::Available { enabled },
        Err(Error::Forbidden) => TotpStatusOutcome::Forbidden,
        Err(Error::Retryable) => TotpStatusOutcome::Retryable,
        Err(error) => {
            tracing::error!(%error, "TOTP status failed");
            TotpStatusOutcome::Unavailable
        }
    }
}

pub fn project_password_status(result: Result<bool, Error>) -> Outcome {
    match result {
        Ok(has_password) => Outcome::Available { has_password },
        Err(Error::Forbidden) => Outcome::Forbidden,
        Err(Error::Retryable) => Outcome::Retryable,
        Err(error) => {
            tracing::error!(%error, "password status failed");
            Outcome::Unavailable
        }
    }
}
