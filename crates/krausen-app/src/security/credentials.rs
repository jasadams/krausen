// SPDX-License-Identifier: AGPL-3.0-or-later
//! Transactional authenticator (TOTP) and passkey management.
//!
//! Browser and MCP adapters share these commands. Pending enrollment material
//! lives only in the KV store with a short TTL; it is never written to the
//! durable request/audit tables.
use super::{
    ActorContext, DbPool, DbTx, Error, RequestIdentity, authorize_account, boundary_error, db_error,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use krausen_core::KVPool;
use webauthn_rs::prelude::{CredentialID, RegisterPublicKeyCredential, Webauthn};

fn digest(value: &serde_json::Value) -> Result<String, Error> {
    krausen_core::canonical_json::sha256(value).map_err(|_| Error::Invalid("request identity"))
}

fn account_locked_sql(pg: bool) -> &'static str {
    if pg {
        "SELECT account_revision FROM users WHERE user_id=$1 AND active=TRUE FOR UPDATE"
    } else {
        "SELECT account_revision FROM users WHERE user_id=$1 AND active=TRUE"
    }
}

async fn lock_account(tx: &mut DbTx<'_>, actor: &ActorContext) -> Result<i64, Error> {
    let pg = matches!(tx, DbTx::Postgres(_));
    let revision = scalar!(tx, i64, account_locked_sql(pg), &actor.actor_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    authorize_account(tx, actor).await?;
    Ok(revision)
}

async fn method_json(
    tx: &mut DbTx<'_>,
    user_id: &str,
    auth_type: &str,
    exclusive: bool,
) -> Result<Option<serde_json::Value>, Error> {
    let sql = match (matches!(tx, DbTx::Postgres(_)), exclusive) {
        (true, true) => {
            "SELECT CAST(auth_data AS TEXT) FROM user_auth_methods WHERE user_id=$1 AND auth_type=$2 AND active=TRUE FOR UPDATE"
        }
        _ => {
            "SELECT CAST(auth_data AS TEXT) FROM user_auth_methods WHERE user_id=$1 AND auth_type=$2 AND active=TRUE"
        }
    };
    let value = scalar!(tx, String, sql, user_id, auth_type).map_err(db_error)?;
    match value {
        Some(value) => Ok(Some(serde_json::from_str(&value)?)),
        None => Ok(None),
    }
}

async fn save_method(
    tx: &mut DbTx<'_>,
    user_id: &str,
    auth_type: &str,
    data: &serde_json::Value,
) -> Result<(), Error> {
    let json = serde_json::to_string(data)?;
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "INSERT INTO user_auth_methods (user_id,auth_type,auth_data,active) VALUES ($1,$2,CAST($3 AS JSON),TRUE) ON CONFLICT (user_id,auth_type) DO UPDATE SET auth_data=CAST($3 AS JSON), active=TRUE"
    } else {
        "INSERT INTO user_auth_methods (user_id,auth_type,auth_data,active) VALUES ($1,$2,$3,1) ON CONFLICT (user_id,auth_type) DO UPDATE SET auth_data=$3, active=1"
    };
    query!(tx, sql, user_id, auth_type, &json).map_err(db_error)?;
    Ok(())
}

async fn deactivate_method(
    tx: &mut DbTx<'_>,
    user_id: &str,
    auth_type: &str,
) -> Result<bool, Error> {
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "UPDATE user_auth_methods SET active=FALSE WHERE user_id=$1 AND auth_type=$2 AND active=TRUE"
    } else {
        "UPDATE user_auth_methods SET active=0 WHERE user_id=$1 AND auth_type=$2 AND active=1"
    };
    Ok(query!(tx, sql, user_id, auth_type).map_err(db_error)? > 0)
}

async fn bump_revision(tx: &mut DbTx<'_>, user_id: &str, revision: i64) -> Result<i64, Error> {
    let next = revision + 1;
    let updated = query!(
        tx,
        "UPDATE users SET account_revision=$1 WHERE user_id=$2 AND account_revision=$3",
        next,
        user_id,
        revision
    )
    .map_err(db_error)?;
    if updated != 1 {
        return Err(Error::Retryable);
    }
    Ok(next)
}

async fn finish_request(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    operation: &str,
    request_id: &str,
    payload_digest: &str,
    revision: i64,
) -> Result<(), Error> {
    let scoped = ActorContext {
        workspace_id: String::new(),
        ..actor.clone()
    };
    let key = RequestIdentity {
        actor: &scoped,
        operation,
        request_id,
        payload_digest,
    };
    crate::record_request(
        tx,
        &key,
        &serde_json::json!(revision),
        Some(&actor.actor_id),
    )
    .await?;
    crate::record_change(tx, &scoped, operation, &actor.actor_id, revision).await
}

async fn recover(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    operation: &str,
    request_id: &str,
    payload_digest: &str,
) -> Result<Option<i64>, Error> {
    let scoped = ActorContext {
        workspace_id: String::new(),
        ..actor.clone()
    };
    let key = RequestIdentity {
        actor: &scoped,
        operation,
        request_id,
        payload_digest,
    };
    match crate::recover_request(tx, &key).await? {
        Some(value) => Ok(Some(serde_json::from_value(value)?)),
        None => Ok(None),
    }
}

async fn recover_value(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    operation: &str,
    request_id: &str,
    payload_digest: &str,
) -> Result<Option<(String, i64)>, Error> {
    let scoped = ActorContext {
        workspace_id: String::new(),
        ..actor.clone()
    };
    let key = RequestIdentity {
        actor: &scoped,
        operation,
        request_id,
        payload_digest,
    };
    let Some(value) = crate::recover_request(tx, &key).await? else {
        return Ok(None);
    };
    let device = value
        .get("device_name")
        .and_then(|name| name.as_str())
        .unwrap_or("Passkey")
        .to_owned();
    let revision = value
        .get("revision")
        .and_then(|rev| rev.as_i64())
        .unwrap_or_default();
    Ok(Some((device, revision)))
}

/// Begin TOTP enrollment. The generated secret is returned to the caller and
/// kept only in the short-lived KV pending slot.
pub async fn setup_totp(
    pool: &DbPool,
    kv: &KVPool,
    actor: &ActorContext,
    expected: i64,
) -> Result<krausen_types::account::TotpSetup, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let pg = matches!(tx, DbTx::Postgres(_));
    let sql = if pg {
        "SELECT account_revision FROM users WHERE user_id=$1 AND active=TRUE FOR SHARE"
    } else {
        "SELECT account_revision FROM users WHERE user_id=$1 AND active=TRUE"
    };
    let revision = scalar!(&mut tx, i64, sql, &actor.actor_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    authorize_account(&mut tx, actor).await?;
    let email = scalar!(
        &mut tx,
        String,
        "SELECT email FROM users WHERE user_id=$1 AND active=TRUE",
        &actor.actor_id
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    if method_json(&mut tx, &actor.actor_id, "totp", false)
        .await?
        .is_some()
    {
        return Err(Error::TotpAlreadyEnabled);
    }
    if revision != expected {
        return Err(Error::Conflict {
            expected,
            actual: revision,
        });
    }
    let secret = krausen_auth::totp::generate_secret();
    let qr_uri = krausen_auth::totp::generate_qr_code(&secret, &email)
        .map_err(|_| Error::Invalid("authenticator setup"))?;
    tx.commit().await.map_err(boundary_error)?;
    krausen_auth::redis_ops::store_pending_totp(kv, &actor.actor_id, &secret)
        .await
        .map_err(|_| Error::Retryable)?;
    Ok(krausen_types::account::TotpSetup { secret, qr_uri })
}

/// Confirm TOTP enrollment and persist the verified secret atomically.
pub async fn enable_totp(
    pool: &DbPool,
    kv: &KVPool,
    actor: &ActorContext,
    code: &str,
    expected: i64,
    request_id: uuid::Uuid,
    key: &[u8],
) -> Result<i64, Error> {
    if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::InvalidCode);
    }
    let request = request_id.to_string();
    // A six-digit code is too small to store as a bare digest; use the
    // installation-key HMAC commitment shared with password commands.
    let (_, payload) = super::password::command_tag(
        key,
        "account.totp.enable.v1",
        actor,
        request_id,
        expected,
        &[code],
    )?;
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let revision = lock_account(&mut tx, actor).await?;
    if let Some(recovered) =
        recover(&mut tx, actor, "account.totp.enable.v1", &request, &payload).await?
    {
        tx.commit().await.map_err(boundary_error)?;
        return Ok(recovered);
    }
    if revision != expected {
        return Err(Error::Conflict {
            expected,
            actual: revision,
        });
    }
    let Some(secret) = krausen_auth::redis_ops::peek_pending_totp(kv, &actor.actor_id)
        .await
        .map_err(|_| Error::Retryable)?
    else {
        return Err(Error::NoPendingSetup);
    };
    if !krausen_auth::totp::verify_code(&secret, code) {
        // The pending secret is left untouched so the caller can retry.
        return Err(Error::InvalidCode);
    }
    let data = serde_json::json!({
        "secret": secret,
        "enabled_at": chrono::Utc::now().to_rfc3339(),
    });
    save_method(&mut tx, &actor.actor_id, "totp", &data).await?;
    let next = bump_revision(&mut tx, &actor.actor_id, revision).await?;
    finish_request(
        &mut tx,
        actor,
        "account.totp.enable.v1",
        &request,
        &payload,
        next,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    if let Err(error) = krausen_auth::redis_ops::delete_pending_totp(kv, &actor.actor_id).await {
        tracing::warn!(%error, "failed to clear consumed authenticator setup");
    }
    Ok(next)
}

/// Disable TOTP for the current account. This does not remove a sign-in method.
pub async fn disable_totp(
    pool: &DbPool,
    actor: &ActorContext,
    expected: i64,
    request_id: uuid::Uuid,
) -> Result<i64, Error> {
    let request = request_id.to_string();
    let payload = digest(&serde_json::json!({"expected_revision":expected}))?;
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let revision = lock_account(&mut tx, actor).await?;
    if let Some(recovered) = recover(
        &mut tx,
        actor,
        "account.totp.disable.v1",
        &request,
        &payload,
    )
    .await?
    {
        tx.commit().await.map_err(boundary_error)?;
        return Ok(recovered);
    }
    if revision != expected {
        return Err(Error::Conflict {
            expected,
            actual: revision,
        });
    }
    if !deactivate_method(&mut tx, &actor.actor_id, "totp").await? {
        return Err(Error::TotpNotEnabled);
    }
    let next = bump_revision(&mut tx, &actor.actor_id, revision).await?;
    finish_request(
        &mut tx,
        actor,
        "account.totp.disable.v1",
        &request,
        &payload,
        next,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(next)
}

/// Begin passkey registration, storing the one-time challenge in KV.
pub async fn start_passkey_registration(
    pool: &DbPool,
    kv: &KVPool,
    webauthn: &Webauthn,
    actor: &ActorContext,
    device_name: &str,
    expected: i64,
) -> Result<krausen_types::account::PasskeyRegistration, Error> {
    let device_name = device_name.trim();
    if device_name.is_empty() || device_name.chars().count() > 100 {
        return Err(Error::Invalid("device name"));
    }
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT account_revision FROM users WHERE user_id=$1 AND active=TRUE FOR SHARE"
    } else {
        "SELECT account_revision FROM users WHERE user_id=$1 AND active=TRUE"
    };
    let revision = scalar!(&mut tx, i64, sql, &actor.actor_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    authorize_account(&mut tx, actor).await?;
    if revision != expected {
        return Err(Error::Conflict {
            expected,
            actual: revision,
        });
    }
    let row = krausen_core::dbtx_fetch_optional!(
        &mut tx,
        UserIdentity,
        "SELECT email,name FROM users WHERE user_id=$1 AND active=TRUE",
        &actor.actor_id
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    let credentials = method_json(&mut tx, &actor.actor_id, "webauthn", false)
        .await?
        .and_then(|data| {
            data.get("credentials")
                .and_then(|value| value.as_object())
                .map(|map| map.keys().cloned().collect::<Vec<_>>())
        })
        .unwrap_or_default();
    tx.commit().await.map_err(boundary_error)?;
    let mut exclude = Vec::new();
    for id in credentials {
        if let Ok(bytes) = URL_SAFE_NO_PAD.decode(id) {
            exclude.push(CredentialID::from(bytes));
        }
    }
    let (options, state) = krausen_auth::webauthn::start_registration(
        webauthn,
        krausen_auth::auth_service::webauthn_user_id(&row.email),
        &row.email,
        row.name.as_deref().unwrap_or(&row.email),
        (!exclude.is_empty()).then_some(exclude),
    )
    .map_err(|_| Error::Invalid("passkey registration"))?;
    let challenge_id = krausen_auth::redis_ops::generate_token();
    let challenge = serde_json::json!({
        "registration_state": serde_json::to_value(&state)?,
        "email": row.email,
        "user_name": row.name.unwrap_or_default(),
        "user_id": actor.actor_id,
        "device_name": device_name,
    });
    krausen_auth::redis_ops::store_webauthn_challenge(kv, &challenge_id, &challenge)
        .await
        .map_err(|_| Error::Retryable)?;
    Ok(krausen_types::account::PasskeyRegistration {
        challenge_id,
        options: serde_json::to_value(options)?,
    })
}

#[derive(sqlx::FromRow)]
struct UserIdentity {
    email: String,
    name: Option<String>,
}

/// Complete passkey registration and persist the verified credential.
pub async fn complete_passkey_registration(
    pool: &DbPool,
    kv: &KVPool,
    webauthn: &Webauthn,
    actor: &ActorContext,
    challenge_id: &str,
    credential: &RegisterPublicKeyCredential,
    expected: i64,
    request_id: uuid::Uuid,
) -> Result<(String, i64), Error> {
    let request = request_id.to_string();
    let payload = digest(&serde_json::json!({
        "challenge_id": challenge_id,
        "expected_revision": expected
    }))?;
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let revision = lock_account(&mut tx, actor).await?;
    if let Some(value) = recover_value(
        &mut tx,
        actor,
        "account.passkey.register.complete.v1",
        &request,
        &payload,
    )
    .await?
    {
        tx.commit().await.map_err(boundary_error)?;
        return Ok((value.0, value.1));
    }
    if revision != expected {
        return Err(Error::Conflict {
            expected,
            actual: revision,
        });
    }
    let Some(challenge) = krausen_auth::redis_ops::get_webauthn_challenge(kv, challenge_id)
        .await
        .map_err(|_| Error::Retryable)?
    else {
        return Err(Error::Invalid("passkey challenge"));
    };
    if challenge["user_id"].as_str() != Some(actor.actor_id.as_str()) {
        return Err(Error::Forbidden);
    }
    let state: webauthn_rs::prelude::PasskeyRegistration =
        serde_json::from_value(challenge["registration_state"].clone())
            .map_err(|_| Error::Invalid("passkey challenge"))?;
    let device_name = challenge["device_name"]
        .as_str()
        .unwrap_or("Passkey")
        .to_owned();
    let passkey = krausen_auth::webauthn::finish_registration(webauthn, credential, &state)
        .map_err(|_| Error::Invalid("passkey credential"))?;
    let credential_id = URL_SAFE_NO_PAD.encode(passkey.cred_id().as_ref());
    let passkey_json = serde_json::to_value(&passkey)?;
    let public_key = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&passkey)?);
    let counter = passkey_json
        .get("cred")
        .and_then(|cred| cred.get("counter"))
        .and_then(|counter| counter.as_u64())
        .unwrap_or(0) as u32;
    let entry = serde_json::json!({
        "public_key": public_key,
        "sign_count": counter,
        "device_name": device_name,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "last_used": null,
        "passkey": passkey_json,
    });
    let mut data = method_json(&mut tx, &actor.actor_id, "webauthn", true)
        .await?
        .unwrap_or_else(|| serde_json::json!({"credentials":{}}));
    if !data.is_object() {
        return Err(Error::Invalid("passkey store"));
    }
    if !data
        .get("credentials")
        .is_some_and(|value| value.is_object())
    {
        data["credentials"] = serde_json::json!({});
    }
    data["credentials"]
        .as_object_mut()
        .ok_or(Error::Invalid("passkey store"))?
        .insert(credential_id, entry);
    save_method(&mut tx, &actor.actor_id, "webauthn", &data).await?;
    let next = bump_revision(&mut tx, &actor.actor_id, revision).await?;
    let scoped = ActorContext {
        workspace_id: String::new(),
        ..actor.clone()
    };
    let key = RequestIdentity {
        actor: &scoped,
        operation: "account.passkey.register.complete.v1",
        request_id: &request,
        payload_digest: &payload,
    };
    crate::record_request(
        &mut tx,
        &key,
        &serde_json::json!({"device_name":device_name,"revision":next}),
        Some(&actor.actor_id),
    )
    .await?;
    crate::record_change(
        &mut tx,
        &scoped,
        "account.passkey.register.complete.v1",
        &actor.actor_id,
        next,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    if let Err(error) = krausen_auth::redis_ops::delete_webauthn_challenge(kv, challenge_id).await {
        tracing::warn!(%error, "passkey challenge cleanup failed");
    }
    Ok((device_name, next))
}

/// Rename a passkey owned by the current account.
pub async fn rename_passkey(
    pool: &DbPool,
    actor: &ActorContext,
    credential_id: &str,
    name: &str,
    expected: i64,
    request_id: uuid::Uuid,
) -> Result<i64, Error> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(Error::Invalid("device name"));
    }
    let request = request_id.to_string();
    let payload = digest(&serde_json::json!({
        "credential_id": credential_id,
        "name": name,
        "expected_revision": expected
    }))?;
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let revision = lock_account(&mut tx, actor).await?;
    if let Some(recovered) = recover(
        &mut tx,
        actor,
        "account.passkey.rename.v1",
        &request,
        &payload,
    )
    .await?
    {
        tx.commit().await.map_err(boundary_error)?;
        return Ok(recovered);
    }
    if revision != expected {
        return Err(Error::Conflict {
            expected,
            actual: revision,
        });
    }
    let mut data = method_json(&mut tx, &actor.actor_id, "webauthn", true)
        .await?
        .ok_or(Error::NotFound)?;
    let entry = data
        .get_mut("credentials")
        .and_then(|value| value.as_object_mut())
        .and_then(|map| map.get_mut(credential_id))
        .and_then(|value| value.as_object_mut())
        .ok_or(Error::NotFound)?;
    entry.insert(
        "device_name".into(),
        serde_json::Value::String(name.to_owned()),
    );
    save_method(&mut tx, &actor.actor_id, "webauthn", &data).await?;
    let next = bump_revision(&mut tx, &actor.actor_id, revision).await?;
    finish_request(
        &mut tx,
        actor,
        "account.passkey.rename.v1",
        &request,
        &payload,
        next,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(next)
}

/// Delete a passkey, refusing to remove the account's final sign-in method.
pub async fn delete_passkey(
    pool: &DbPool,
    actor: &ActorContext,
    credential_id: &str,
    expected: i64,
    request_id: uuid::Uuid,
) -> Result<i64, Error> {
    let request = request_id.to_string();
    let payload = digest(&serde_json::json!({
        "credential_id": credential_id,
        "expected_revision": expected
    }))?;
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let revision = lock_account(&mut tx, actor).await?;
    if let Some(recovered) = recover(
        &mut tx,
        actor,
        "account.passkey.delete.v1",
        &request,
        &payload,
    )
    .await?
    {
        tx.commit().await.map_err(boundary_error)?;
        return Ok(recovered);
    }
    if revision != expected {
        return Err(Error::Conflict {
            expected,
            actual: revision,
        });
    }
    let mut data = method_json(&mut tx, &actor.actor_id, "webauthn", true)
        .await?
        .ok_or(Error::NotFound)?;
    let credentials = data
        .get_mut("credentials")
        .and_then(|value| value.as_object_mut())
        .ok_or(Error::NotFound)?;
    if !credentials.contains_key(credential_id) {
        return Err(Error::NotFound);
    }
    if credentials.len() <= 1 {
        let sql = if matches!(tx, DbTx::Postgres(_)) {
            "SELECT COUNT(*) FROM user_auth_methods WHERE user_id=$1 AND active=TRUE AND auth_type IN ('password','google_oauth')"
        } else {
            "SELECT COUNT(*) FROM user_auth_methods WHERE user_id=$1 AND active=1 AND auth_type IN ('password','google_oauth')"
        };
        let alternatives = scalar!(&mut tx, i64, sql, &actor.actor_id)
            .map_err(db_error)?
            .unwrap_or(0);
        if alternatives == 0 {
            return Err(Error::LastSignInMethod);
        }
    }
    credentials.remove(credential_id);
    save_method(&mut tx, &actor.actor_id, "webauthn", &data).await?;
    let next = bump_revision(&mut tx, &actor.actor_id, revision).await?;
    finish_request(
        &mut tx,
        actor,
        "account.passkey.delete.v1",
        &request,
        &payload,
        next,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(next)
}
