// SPDX-License-Identifier: AGPL-3.0-or-later
//! Transactional password change and secret-safe retry identity.
use super::{
    ActorContext, DbPool, DbTx, Error, RequestIdentity, authorize_account, boundary_error, db_error,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub(super) fn command_tag(
    key: &[u8],
    operation: &str,
    actor: &ActorContext,
    request_id: uuid::Uuid,
    expected: i64,
    parts: &[&str],
) -> Result<(String, String), Error> {
    if key.len() < 32 {
        return Err(Error::Invalid("invalid installation key"));
    }
    let mut derive = Hmac::<Sha256>::new_from_slice(key)
        .map_err(|_| Error::Invalid("invalid installation key"))?;
    derive.update(b"krausen.password-command.subkey.v1\0");
    let subkey = derive.finalize().into_bytes();
    let key_id = URL_SAFE_NO_PAD.encode(Sha256::digest(&subkey));
    let mut mac = Hmac::<Sha256>::new_from_slice(&subkey)
        .map_err(|_| Error::Invalid("invalid password key"))?;
    let request = request_id.to_string();
    for part in std::iter::once(operation)
        .chain(std::iter::once(actor.actor_id.as_str()))
        .chain(std::iter::once(actor.credential_id.as_str()))
        .chain(std::iter::once(request.as_str()))
        .chain(parts.iter().copied())
    {
        mac.update(&(part.len() as u64).to_be_bytes());
        mac.update(part.as_bytes());
    }
    mac.update(&expected.to_be_bytes());
    let tag = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    Ok((key_id.clone(), format!("password-hmac:v1:{key_id}:{tag}")))
}

pub async fn change_password(
    pool: &DbPool,
    actor: &ActorContext,
    current: String,
    new: String,
    expected: i64,
    request_id: uuid::Uuid,
    key: &[u8],
) -> Result<i64, Error> {
    if new.len() < 8 || new.len() > 1024 || current.len() > 1024 {
        return Err(Error::Invalid("password"));
    }
    let (key_id, digest) = command_tag(
        key,
        "account.password.change.v1",
        actor,
        request_id,
        expected,
        &[&current, &new],
    )?;
    let to_hash = new;
    let new_hash =
        tokio::task::spawn_blocking(move || krausen_auth::password::hash_password(&to_hash))
            .await
            .map_err(|_| Error::Retryable)??;
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT account_revision FROM users WHERE user_id=$1 AND active=TRUE FOR UPDATE"
    } else {
        "SELECT account_revision FROM users WHERE user_id=$1 AND active=TRUE"
    };
    let revision = scalar!(&mut tx, i64, sql, &actor.actor_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    authorize_account(&mut tx, actor).await?;
    let scoped = ActorContext {
        workspace_id: String::new(),
        ..actor.clone()
    };
    let request = request_id.to_string();
    let request_key = RequestIdentity {
        actor: &scoped,
        operation: "account.password.change.v1",
        request_id: &request,
        payload_digest: &digest,
    };
    let stored=scalar!(&mut tx,String,"SELECT payload_digest FROM app_requests WHERE actor_id=$1 AND workspace_id=$2 AND operation=$3 AND request_id=$4",&scoped.actor_id,&scoped.workspace_id,request_key.operation,&request).map_err(db_error)?;
    let bound=scalar!(&mut tx,String,"SELECT credential_id FROM account_password_request_bindings WHERE actor_id=$1 AND operation=$2 AND request_id=$3",&actor.actor_id,request_key.operation,&request).map_err(db_error)?;
    match (&stored, &bound) {
        (Some(_), Some(credential)) if credential != &actor.credential_id => {
            return Err(Error::Forbidden);
        }
        (Some(_), None) | (None, Some(_)) => return Err(Error::RetryKeyUnavailable),
        _ => {}
    }
    if let Some(stored) = stored {
        let mut parts = stored.split(':');
        if parts.next() != Some("password-hmac")
            || parts.next() != Some("v1")
            || parts.next() != Some(key_id.as_str())
            || parts.next().is_none()
            || parts.next().is_some()
        {
            return Err(Error::RetryKeyUnavailable);
        }
        if !bool::from(stored.as_bytes().ct_eq(digest.as_bytes())) {
            return Err(Error::RequestIdentityConflict);
        }
        let value = crate::recover_request(&mut tx, &request_key)
            .await?
            .ok_or(Error::Retryable)?;
        let result = serde_json::from_value(value)?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(result);
    }
    if revision != expected {
        return Err(Error::Conflict {
            expected,
            actual: revision,
        });
    }
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT CAST(auth_data AS TEXT) FROM user_auth_methods WHERE user_id=$1 AND auth_type='password' AND active=TRUE FOR UPDATE"
    } else {
        "SELECT CAST(auth_data AS TEXT) FROM user_auth_methods WHERE user_id=$1 AND auth_type='password' AND active=TRUE"
    };
    let data = scalar!(&mut tx, String, sql, &actor.actor_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    let data: serde_json::Value = serde_json::from_str(&data)?;
    let old_hash = data
        .get("hash")
        .and_then(|value| value.as_str())
        .ok_or(Error::Forbidden)?
        .to_owned();
    let valid = tokio::task::spawn_blocking(move || {
        krausen_auth::password::verify_password(&current, &old_hash)
    })
    .await
    .map_err(|_| Error::Retryable)??;
    if !valid {
        return Err(Error::InvalidPassword);
    }
    authorize_account(&mut tx, actor).await?;
    let data = serde_json::json!({"hash":new_hash}).to_string();
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "UPDATE user_auth_methods SET auth_data=CAST($1 AS JSON) WHERE user_id=$2 AND auth_type='password' AND active=TRUE"
    } else {
        "UPDATE user_auth_methods SET auth_data=$1 WHERE user_id=$2 AND auth_type='password' AND active=TRUE"
    };
    let affected = query!(&mut tx, sql, &data, &actor.actor_id).map_err(db_error)?;
    if affected != 1 {
        return Err(Error::Retryable);
    }
    let next = revision + 1;
    query!(
        &mut tx,
        "UPDATE users SET account_revision=$1 WHERE user_id=$2 AND account_revision=$3",
        next,
        &actor.actor_id,
        revision
    )
    .map_err(db_error)?;
    query!(&mut tx,"INSERT INTO account_password_request_bindings (actor_id,operation,request_id,credential_id) VALUES ($1,$2,$3,$4)",&actor.actor_id,request_key.operation,&request,&actor.credential_id).map_err(db_error)?;
    crate::record_request(
        &mut tx,
        &request_key,
        &serde_json::json!(next),
        Some(&actor.actor_id),
    )
    .await?;
    crate::record_change(
        &mut tx,
        &scoped,
        "account.password.change.v1",
        &actor.actor_id,
        next,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(next)
}

/// Add a first password to an account that authenticates another way.
///
/// Runs with the same durable retry, credential binding and revision checks as
/// a password change, but refuses to overwrite an existing password verifier.
pub async fn set_password(
    pool: &DbPool,
    actor: &ActorContext,
    new: String,
    expected: i64,
    request_id: uuid::Uuid,
    key: &[u8],
) -> Result<i64, Error> {
    if new.len() < 8 || new.len() > 1024 {
        return Err(Error::Invalid("password"));
    }
    let (key_id, digest) = command_tag(
        key,
        "account.password.set.v1",
        actor,
        request_id,
        expected,
        &[&new],
    )?;
    let to_hash = new;
    let new_hash =
        tokio::task::spawn_blocking(move || krausen_auth::password::hash_password(&to_hash))
            .await
            .map_err(|_| Error::Retryable)??;
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT account_revision FROM users WHERE user_id=$1 AND active=TRUE FOR UPDATE"
    } else {
        "SELECT account_revision FROM users WHERE user_id=$1 AND active=TRUE"
    };
    let revision = scalar!(&mut tx, i64, sql, &actor.actor_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    authorize_account(&mut tx, actor).await?;
    let scoped = ActorContext {
        workspace_id: String::new(),
        ..actor.clone()
    };
    let request = request_id.to_string();
    let request_key = RequestIdentity {
        actor: &scoped,
        operation: "account.password.set.v1",
        request_id: &request,
        payload_digest: &digest,
    };
    let stored=scalar!(&mut tx,String,"SELECT payload_digest FROM app_requests WHERE actor_id=$1 AND workspace_id=$2 AND operation=$3 AND request_id=$4",&scoped.actor_id,&scoped.workspace_id,request_key.operation,&request).map_err(db_error)?;
    let bound=scalar!(&mut tx,String,"SELECT credential_id FROM account_password_request_bindings WHERE actor_id=$1 AND operation=$2 AND request_id=$3",&actor.actor_id,request_key.operation,&request).map_err(db_error)?;
    match (&stored, &bound) {
        (Some(_), Some(credential)) if credential != &actor.credential_id => {
            return Err(Error::Forbidden);
        }
        (Some(_), None) | (None, Some(_)) => return Err(Error::RetryKeyUnavailable),
        _ => {}
    }
    if let Some(stored) = stored {
        let mut parts = stored.split(':');
        if parts.next() != Some("password-hmac")
            || parts.next() != Some("v1")
            || parts.next() != Some(key_id.as_str())
            || parts.next().is_none()
            || parts.next().is_some()
        {
            return Err(Error::RetryKeyUnavailable);
        }
        if !bool::from(stored.as_bytes().ct_eq(digest.as_bytes())) {
            return Err(Error::RequestIdentityConflict);
        }
        let value = crate::recover_request(&mut tx, &request_key)
            .await?
            .ok_or(Error::Retryable)?;
        let result = serde_json::from_value(value)?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(result);
    }
    if revision != expected {
        return Err(Error::Conflict {
            expected,
            actual: revision,
        });
    }
    let existing = scalar!(
        &mut tx,
        Option<String>,
        "SELECT CAST(auth_data AS TEXT) FROM user_auth_methods WHERE user_id=$1 AND auth_type='password' AND active=TRUE",
        &actor.actor_id
    )
    .map_err(db_error)?;
    if existing.is_some() {
        return Err(Error::PasswordAlreadySet);
    }
    let data = serde_json::json!({"hash":new_hash}).to_string();
    let insert = if matches!(tx, DbTx::Postgres(_)) {
        "INSERT INTO user_auth_methods (user_id,auth_type,auth_data,active) VALUES ($1,'password',CAST($2 AS JSON),TRUE) ON CONFLICT (user_id,auth_type) DO UPDATE SET auth_data=CAST($2 AS JSON), active=TRUE"
    } else {
        "INSERT INTO user_auth_methods (user_id,auth_type,auth_data,active) VALUES ($1,'password',$2,1) ON CONFLICT (user_id,auth_type) DO UPDATE SET auth_data=$2, active=1"
    };
    query!(&mut tx, insert, &actor.actor_id, &data).map_err(db_error)?;
    let next = revision + 1;
    query!(
        &mut tx,
        "UPDATE users SET account_revision=$1 WHERE user_id=$2 AND account_revision=$3",
        next,
        &actor.actor_id,
        revision
    )
    .map_err(db_error)?;
    query!(&mut tx,"INSERT INTO account_password_request_bindings (actor_id,operation,request_id,credential_id) VALUES ($1,$2,$3,$4)",&actor.actor_id,request_key.operation,&request,&actor.credential_id).map_err(db_error)?;
    crate::record_request(
        &mut tx,
        &request_key,
        &serde_json::json!(next),
        Some(&actor.actor_id),
    )
    .await?;
    crate::record_change(
        &mut tx,
        &scoped,
        "account.password.set.v1",
        &actor.actor_id,
        next,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(next)
}
