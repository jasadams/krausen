// SPDX-License-Identifier: AGPL-3.0-or-later
//! Account-owned browser session revocation; token IDs are opaque expected versions.
use crate::{
    Error, boundary_error, db_error,
    invitation::{AccountActor, recheck_credential},
};
use krausen_core::{DbPool, DbTx};
use krausen_types::session_revoke::{SessionRevokeCommand, SessionRevokeOutcome as Outcome};

#[derive(sqlx::FromRow)]
struct Recorded {
    payload_digest: String,
    expected_token_id: String,
    revoked_current: bool,
    result_expires_at: i64,
}

pub async fn revoke(
    pool: &DbPool,
    actor: &AccountActor,
    command: &SessionRevokeCommand,
) -> Result<Outcome, Error> {
    let request_id = uuid::Uuid::parse_str(&command.request_id)
        .map_err(|_| Error::Invalid("request id"))?
        .to_string();
    let token = &command.expected_current_token_id;
    if token.is_empty() || token.len() > 512 {
        return Err(Error::Invalid("session token id"));
    }
    let digest = krausen_core::canonical_json::sha256(
        &serde_json::json!({"expected_current_token_id":token}),
    )
    .map_err(|_| Error::Invalid("session revoke"))?;
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let account_sql = if matches!(&tx, DbTx::Postgres(_)) {
        "SELECT user_id FROM users WHERE user_id=$1 AND active=TRUE FOR UPDATE"
    } else {
        "SELECT user_id FROM users WHERE user_id=$1 AND active=TRUE"
    };
    if scalar!(&mut tx, String, account_sql, &actor.actor_id)
        .map_err(db_error)?
        .is_none()
    {
        return Err(Error::Forbidden);
    }
    recheck_credential(&mut tx, actor).await?;
    let prior = krausen_core::dbtx_fetch_optional!(&mut tx, Recorded, "SELECT payload_digest,expected_token_id,revoked_current,result_expires_at FROM account_session_revocations WHERE account_id=$1 AND request_id=$2", &actor.actor_id, &request_id).map_err(db_error)?;
    if let Some(prior) = prior {
        if prior.payload_digest != digest {
            return Err(Error::RequestIdentityConflict);
        }
        if prior.result_expires_at <= chrono::Utc::now().timestamp() {
            return Err(Error::ResultExpired {
                resource_id: Some(prior.expected_token_id),
            });
        }
        tx.commit().await.map_err(boundary_error)?;
        return Ok(Outcome::Revoked {
            token_id: prior.expected_token_id,
            revoked_current: prior.revoked_current,
        });
    }
    let family = scalar!(
        &mut tx,
        String,
        "SELECT family_id FROM refresh_tokens WHERE token_id=$1 AND user_id=$2",
        token,
        &actor.actor_id
    )
    .map_err(db_error)?;
    let Some(family) = family else {
        return Ok(Outcome::NotFound);
    };
    let lock_sql = if matches!(&tx, DbTx::Postgres(_)) {
        "SELECT credential_id FROM auth_credentials WHERE credential_id=$1 AND user_id=$2 AND kind='browser' FOR UPDATE"
    } else {
        "SELECT credential_id FROM auth_credentials WHERE credential_id=$1 AND user_id=$2 AND kind='browser'"
    };
    if scalar!(&mut tx, String, lock_sql, &family, &actor.actor_id)
        .map_err(db_error)?
        .is_none()
    {
        return Ok(Outcome::NotFound);
    }
    let now = chrono::Utc::now().timestamp();
    let live = scalar!(&mut tx, String, "SELECT credential_id FROM auth_credentials WHERE credential_id=$1 AND user_id=$2 AND kind='browser' AND revoked_at IS NULL AND expires_at>$3", &family, &actor.actor_id, now).map_err(db_error)?;
    if live.is_none() {
        return Ok(Outcome::Conflict);
    }
    let count = scalar!(&mut tx, i64, "SELECT COUNT(*) FROM refresh_tokens WHERE user_id=$1 AND family_id=$2 AND is_active=TRUE AND replaced_at IS NULL", &actor.actor_id, &family).map_err(db_error)?.unwrap_or(0);
    if count != 1 {
        return Ok(Outcome::Conflict);
    }
    let leaf_sql = if matches!(&tx, DbTx::Postgres(_)) {
        "SELECT token_id FROM refresh_tokens WHERE user_id=$1 AND family_id=$2 AND token_id=$3 AND is_active=TRUE AND replaced_at IS NULL AND expires_at>clock_timestamp()"
    } else {
        "SELECT token_id FROM refresh_tokens WHERE user_id=$1 AND family_id=$2 AND token_id=$3 AND is_active=TRUE AND replaced_at IS NULL AND julianday(expires_at)>julianday('now')"
    };
    if scalar!(&mut tx, String, leaf_sql, &actor.actor_id, &family, token)
        .map_err(db_error)?
        .is_none()
    {
        return Ok(Outcome::Conflict);
    }
    recheck_credential(&mut tx, actor).await?;
    let kind = scalar!(
        &mut tx,
        String,
        "SELECT kind FROM auth_credentials WHERE credential_id=$1 AND user_id=$2",
        &actor.credential_id,
        &actor.actor_id
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    let revoked_current = kind == "browser" && family == actor.credential_id;
    let revoked_at = chrono::Utc::now().timestamp();
    let changed = query!(&mut tx, "UPDATE auth_credentials SET revoked_at=$1 WHERE credential_id=$2 AND user_id=$3 AND kind='browser' AND revoked_at IS NULL AND expires_at>$1", revoked_at, &family, &actor.actor_id).map_err(db_error)?;
    if changed != 1 {
        return Ok(Outcome::Conflict);
    }
    query!(&mut tx, "UPDATE refresh_tokens SET is_active=FALSE,revoked_at=CURRENT_TIMESTAMP WHERE user_id=$1 AND family_id=$2 AND is_active=TRUE", &actor.actor_id, &family).map_err(db_error)?;
    let expires = revoked_at
        .checked_add(30 * 24 * 60 * 60)
        .ok_or(Error::Invalid("time"))?;
    query!(&mut tx, "INSERT INTO account_session_revocations (account_id,request_id,payload_digest,expected_token_id,family_id,revoked_current,recorded_at,result_expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)", &actor.actor_id, &request_id, &digest, token, &family, revoked_current, revoked_at, expires).map_err(db_error)?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(Outcome::Revoked {
        token_id: token.clone(),
        revoked_current,
    })
}

pub fn project(result: Result<Outcome, Error>) -> Outcome {
    match result {
        Ok(outcome) => outcome,
        Err(Error::Forbidden) => Outcome::Forbidden,
        Err(Error::Invalid(_)) => Outcome::Invalid,
        Err(Error::RequestIdentityConflict) => Outcome::RequestIdentityConflict,
        Err(Error::ResultExpired { resource_id }) => Outcome::ResultExpired { resource_id },
        Err(Error::Retryable) => Outcome::Retryable,
        Err(error) => {
            tracing::error!(%error, "session revocation failed");
            Outcome::Unavailable
        }
    }
}
