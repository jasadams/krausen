// SPDX-License-Identifier: AGPL-3.0-or-later
//! Account profile read shared by UI and MCP.
use crate::{
    Error, boundary_error, db_error,
    invitation::{AccountActor, authorize_account, recheck_credential},
};
use krausen_core::DbPool;
use krausen_types::account_read::{AccountProfile, AccountProfileOutcome};

pub async fn profile(pool: &DbPool, actor: &AccountActor) -> Result<AccountProfile, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_account(&mut tx, actor).await?;
    let user = krausen_core::dbtx_fetch_optional!(
        &mut tx,
        krausen_core::models::User,
        "SELECT * FROM users WHERE user_id=$1",
        &actor.actor_id
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    recheck_credential(&mut tx, actor).await?;
    let theme = user
        .extra_metadata
        .as_ref()
        .and_then(|m| m.get("theme"))
        .and_then(|v| v.as_str())
        .unwrap_or("system")
        .to_owned();
    let profile = AccountProfile {
        user_id: user.user_id,
        email: user.email,
        name: user.name,
        theme,
    };
    tx.commit().await.map_err(boundary_error)?;
    Ok(profile)
}

pub fn project_profile(result: Result<AccountProfile, Error>) -> AccountProfileOutcome {
    match result {
        Ok(profile) => AccountProfileOutcome::Available { profile },
        Err(Error::Forbidden) => AccountProfileOutcome::Forbidden,
        Err(Error::Retryable) => AccountProfileOutcome::Retryable,
        Err(error) => {
            tracing::error!(%error, "account profile failed");
            AccountProfileOutcome::Unavailable
        }
    }
}
