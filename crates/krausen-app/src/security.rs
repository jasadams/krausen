// SPDX-License-Identifier: AGPL-3.0-or-later
//! Credential-owned session queries and revocation.
use crate::{ActorContext, Error, RequestIdentity, authorize_account, boundary_error, db_error};
use krausen_core::{DbPool, DbTx};
use krausen_types::account::Session;

#[derive(sqlx::FromRow)]
struct SessionRow {
    credential_id: String,
    kind: String,
    revision: i64,
    expires_at: i64,
    created_at: Option<String>,
    last_used: Option<String>,
    user_agent: Option<String>,
    ip_address: Option<String>,
    country_code: Option<String>,
    client_name: Option<String>,
}

/// A credential holder can inspect only credentials belonging to that account.
pub async fn sessions(pool: &DbPool, actor: &ActorContext) -> Result<Vec<Session>, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_account(&mut tx, actor).await?;
    let sql = "SELECT c.credential_id,c.kind,c.revision,c.expires_at, (SELECT CAST(r.created_at AS TEXT) FROM refresh_tokens r WHERE r.family_id=c.credential_id ORDER BY r.created_at DESC LIMIT 1) AS created_at, (SELECT CAST(r.last_used AS TEXT) FROM refresh_tokens r WHERE r.family_id=c.credential_id ORDER BY r.created_at DESC LIMIT 1) AS last_used, (SELECT r.user_agent FROM refresh_tokens r WHERE r.family_id=c.credential_id ORDER BY r.created_at DESC LIMIT 1) AS user_agent, (SELECT r.ip_address FROM refresh_tokens r WHERE r.family_id=c.credential_id ORDER BY r.created_at DESC LIMIT 1) AS ip_address, (SELECT r.country_code FROM refresh_tokens r WHERE r.family_id=c.credential_id ORDER BY r.created_at DESC LIMIT 1) AS country_code, (SELECT cl.client_name FROM mcp_oauth_refresh_tokens r JOIN mcp_oauth_clients cl ON cl.client_id=r.client_id WHERE r.credential_id=c.credential_id LIMIT 1) AS client_name FROM auth_credentials c WHERE c.user_id=$1 AND c.revoked_at IS NULL AND c.expires_at>$2 ORDER BY c.credential_id LIMIT 100";
    let rows = match &mut tx {
        DbTx::Postgres(t) => {
            sqlx::query_as::<_, SessionRow>(sql)
                .bind(&actor.actor_id)
                .bind(chrono::Utc::now().timestamp())
                .fetch_all(&mut **t)
                .await
        }
        DbTx::Sqlite(t) => {
            sqlx::query_as::<_, SessionRow>(sql)
                .bind(&actor.actor_id)
                .bind(chrono::Utc::now().timestamp())
                .fetch_all(&mut **t)
                .await
        }
    }
    .map_err(db_error)?;
    authorize_account(&mut tx, actor).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(rows
        .into_iter()
        .map(|r| Session {
            is_current: r.credential_id == actor.credential_id,
            credential_id: r.credential_id,
            kind: r.kind,
            revision: r.revision,
            expires_at: r.expires_at,
            created_at: r.created_at,
            last_used: r.last_used,
            user_agent: r.user_agent,
            ip_address: r.ip_address,
            country_code: r.country_code,
            client_name: r.client_name,
        })
        .collect())
}

#[derive(sqlx::FromRow)]
struct Credential {
    credential_id: String,
    revision: i64,
    revoked_at: Option<i64>,
}

/// Revoke one owned browser or MCP credential, atomically invalidating its family.
pub async fn revoke(
    pool: &DbPool,
    actor: &ActorContext,
    credential_id: &str,
    expected: i64,
    request_id: uuid::Uuid,
) -> Result<i64, Error> {
    let digest = krausen_core::canonical_json::sha256(
        &serde_json::json!({"credential_id":credential_id,"expected_revision":expected}),
    )
    .map_err(|_| Error::Invalid("credential identity"))?;
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let account_sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT user_id FROM users WHERE user_id=$1 AND active=TRUE FOR SHARE"
    } else {
        "SELECT user_id FROM users WHERE user_id=$1 AND active=TRUE"
    };
    if scalar!(&mut tx, String, account_sql, &actor.actor_id)
        .map_err(db_error)?
        .is_none()
    {
        return Err(Error::Forbidden);
    }
    // Lock both credentials in deterministic order, preventing cross-revocation deadlocks.
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT credential_id FROM auth_credentials WHERE user_id=$1 AND credential_id IN ($2,$3) ORDER BY credential_id FOR UPDATE"
    } else {
        "SELECT credential_id FROM auth_credentials WHERE user_id=$1 AND credential_id IN ($2,$3) ORDER BY credential_id"
    };
    let ids = match &mut tx {
        DbTx::Postgres(t) => {
            sqlx::query_scalar::<_, String>(sql)
                .bind(&actor.actor_id)
                .bind(&actor.credential_id)
                .bind(credential_id)
                .fetch_all(&mut **t)
                .await
        }
        DbTx::Sqlite(t) => {
            sqlx::query_scalar::<_, String>(sql)
                .bind(&actor.actor_id)
                .bind(&actor.credential_id)
                .bind(credential_id)
                .fetch_all(&mut **t)
                .await
        }
    }
    .map_err(db_error)?;
    if !ids.iter().any(|id| id == &actor.credential_id) || !ids.iter().any(|id| id == credential_id)
    {
        return Err(Error::Forbidden);
    }
    authorize_account(&mut tx, actor).await?;
    let target=krausen_core::dbtx_fetch_optional!(&mut tx,Credential,"SELECT credential_id,revision,revoked_at FROM auth_credentials WHERE credential_id=$1 AND user_id=$2",credential_id,&actor.actor_id).map_err(db_error)?.ok_or(Error::Forbidden)?;
    if target.credential_id != credential_id {
        return Err(Error::Forbidden);
    }
    let account_actor = ActorContext {
        workspace_id: String::new(),
        ..actor.clone()
    };
    let request = request_id.to_string();
    let key = RequestIdentity {
        actor: &account_actor,
        operation: "account.credential.revoke.v1",
        request_id: &request,
        payload_digest: &digest,
    };
    if let Some(result) = crate::recover_request(&mut tx, &key).await? {
        let revision = serde_json::from_value(result)?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(revision);
    }
    if target.revoked_at.is_some() {
        return Err(Error::Forbidden);
    }
    if target.revision != expected {
        return Err(Error::Conflict {
            expected,
            actual: target.revision,
        });
    }
    let now = chrono::Utc::now().timestamp();
    let revision = target.revision + 1;
    query!(&mut tx,"UPDATE auth_credentials SET revoked_at=$1,revision=$2 WHERE credential_id=$3 AND user_id=$4 AND revoked_at IS NULL",now,revision,credential_id,&actor.actor_id).map_err(db_error)?;
    query!(&mut tx,"UPDATE refresh_tokens SET is_active=FALSE,revoked_at=CURRENT_TIMESTAMP WHERE family_id=$1 AND user_id=$2 AND is_active=TRUE",credential_id,&actor.actor_id).map_err(db_error)?;
    query!(&mut tx,"UPDATE mcp_oauth_refresh_tokens SET consumed_at=COALESCE(consumed_at,$1) WHERE credential_id=$2",now,credential_id).map_err(db_error)?;
    crate::record_request(
        &mut tx,
        &key,
        &serde_json::json!(revision),
        Some(credential_id),
    )
    .await?;
    crate::record_change(
        &mut tx,
        &account_actor,
        "account.credential.revoke.v1",
        credential_id,
        revision,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(revision)
}

/// Revoke every current browser and MCP credential belonging to the account.
pub async fn revoke_all(
    pool: &DbPool,
    actor: &ActorContext,
    request_id: uuid::Uuid,
) -> Result<i64, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT user_id FROM users WHERE user_id=$1 AND active=TRUE FOR UPDATE"
    } else {
        "SELECT user_id FROM users WHERE user_id=$1 AND active=TRUE"
    };
    if scalar!(&mut tx, String, sql, &actor.actor_id)
        .map_err(db_error)?
        .is_none()
    {
        return Err(Error::Forbidden);
    }
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT credential_id FROM auth_credentials WHERE user_id=$1 ORDER BY credential_id FOR UPDATE"
    } else {
        "SELECT credential_id FROM auth_credentials WHERE user_id=$1 ORDER BY credential_id"
    };
    let _ids = match &mut tx {
        DbTx::Postgres(t) => {
            sqlx::query_scalar::<_, String>(sql)
                .bind(&actor.actor_id)
                .fetch_all(&mut **t)
                .await
        }
        DbTx::Sqlite(t) => {
            sqlx::query_scalar::<_, String>(sql)
                .bind(&actor.actor_id)
                .fetch_all(&mut **t)
                .await
        }
    }
    .map_err(db_error)?;
    authorize_account(&mut tx, actor).await?;
    let scoped = ActorContext {
        workspace_id: String::new(),
        ..actor.clone()
    };
    let request = request_id.to_string();
    let key = RequestIdentity {
        actor: &scoped,
        operation: "account.credentials.revoke_all.v1",
        request_id: &request,
        payload_digest: "revoke-all.v1",
    };
    if let Some(value) = crate::recover_request(&mut tx, &key).await? {
        let count = serde_json::from_value(value)?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(count);
    }
    let now = chrono::Utc::now().timestamp();
    let count=query!(&mut tx,"UPDATE auth_credentials SET revoked_at=$1,revision=revision+1 WHERE user_id=$2 AND revoked_at IS NULL",now,&actor.actor_id).map_err(db_error)? as i64;
    query!(&mut tx,"UPDATE refresh_tokens SET is_active=FALSE,revoked_at=CURRENT_TIMESTAMP WHERE user_id=$1 AND is_active=TRUE",&actor.actor_id).map_err(db_error)?;
    query!(&mut tx,"UPDATE mcp_oauth_refresh_tokens SET consumed_at=COALESCE(consumed_at,$1) WHERE credential_id IN (SELECT credential_id FROM auth_credentials WHERE user_id=$2)",now,&actor.actor_id).map_err(db_error)?;
    crate::record_request(&mut tx, &key, &serde_json::json!(count), None).await?;
    crate::record_change(
        &mut tx,
        &scoped,
        "account.credentials.revoke_all.v1",
        &actor.actor_id,
        0,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(count)
}

pub fn project_revoke_all(result: Result<i64, Error>) -> krausen_types::account::RevokeAllOutcome {
    use krausen_types::account::RevokeAllOutcome as Outcome;
    match result {
        Ok(count) => Outcome::Revoked { count },
        Err(Error::Forbidden) => Outcome::Forbidden,
        Err(Error::Retryable) => Outcome::Retryable,
        Err(error) => {
            tracing::error!(%error,"all-session revocation failed");
            Outcome::Unavailable
        }
    }
}

/// Return only the current account's enabled password and authenticator status.
pub async fn status(
    pool: &DbPool,
    actor: &ActorContext,
) -> Result<krausen_types::account::SecurityStatus, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_account(&mut tx, actor).await?;
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT auth_type FROM user_auth_methods WHERE user_id=$1 AND active=TRUE ORDER BY auth_type FOR SHARE"
    } else {
        "SELECT auth_type FROM user_auth_methods WHERE user_id=$1 AND active=TRUE ORDER BY auth_type"
    };
    let methods = match &mut tx {
        DbTx::Postgres(t) => {
            sqlx::query_scalar::<_, String>(sql)
                .bind(&actor.actor_id)
                .fetch_all(&mut **t)
                .await
        }
        DbTx::Sqlite(t) => {
            sqlx::query_scalar::<_, String>(sql)
                .bind(&actor.actor_id)
                .fetch_all(&mut **t)
                .await
        }
    }
    .map_err(db_error)?;
    authorize_account(&mut tx, actor).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(krausen_types::account::SecurityStatus {
        has_password: methods.iter().any(|method| method == "password"),
        totp_enabled: methods.iter().any(|method| method == "totp"),
    })
}

/// Return passkey display metadata without public-key or authenticator material.
pub async fn passkeys(
    pool: &DbPool,
    actor: &ActorContext,
) -> Result<Vec<krausen_types::account::PasskeySummary>, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_account(&mut tx, actor).await?;
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT CAST(auth_data AS TEXT) FROM user_auth_methods WHERE user_id=$1 AND auth_type='webauthn' AND active=TRUE FOR SHARE"
    } else {
        "SELECT CAST(auth_data AS TEXT) FROM user_auth_methods WHERE user_id=$1 AND auth_type='webauthn' AND active=TRUE"
    };
    let value = scalar!(&mut tx, String, sql, &actor.actor_id).map_err(db_error)?;
    let data = match value {
        Some(value) => serde_json::from_str::<serde_json::Value>(&value)?,
        None => serde_json::Value::Null,
    };
    let mut result = Vec::new();
    if let Some(credentials) = data.get("credentials").and_then(|value| value.as_object()) {
        for (id, value) in credentials {
            result.push(krausen_types::account::PasskeySummary {
                credential_id: id.clone(),
                name: value
                    .get("device_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unnamed Device")
                    .to_owned(),
                created_at: value
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
                last_used: value
                    .get("last_used")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
            });
        }
    }
    authorize_account(&mut tx, actor).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(result)
}
