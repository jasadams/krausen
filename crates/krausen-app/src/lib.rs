// SPDX-License-Identifier: AGPL-3.0-or-later
//! Application-owned transaction boundaries and shared workspace operations.

use krausen_core::{DbPool, DbTx};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub mod recipe_migration;

/// Constructed only by an authenticated adapter; workspace selection is not authority.
#[derive(Debug, Clone)]
pub struct ActorContext {
    pub actor_id: String,
    pub workspace_id: String,
    pub credential_id: String,
    /// Expiry of the verified access token, rechecked after acquiring authority locks.
    pub access_expires_at: i64,
}

/// Compare caller expectations only after current authorization; intent never grants access.
pub fn check_intent(
    actor: &ActorContext,
    intent: &krausen_types::request_context::OperationIntent,
) -> Result<(), Error> {
    if intent.account_id != actor.actor_id || intent.workspace_id != actor.workspace_id {
        return Err(Error::ContextChanged);
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("access denied")]
    Forbidden,
    #[error("operation context changed")]
    ContextChanged,
    #[error("equipment profile is archived")]
    EquipmentArchived,
    #[error("ingredient entry is archived")]
    IngredientArchived,
    #[error("duplicate ingredient entry")]
    DuplicateIngredient,
    #[error("style reference is unresolved")]
    UnresolvedStyle,
    #[error("workspace revision conflict")]
    Conflict { expected: i64, actual: i64 },
    #[error("request identity reused with a different payload")]
    RequestIdentityConflict,
    #[error("request result expired")]
    ResultExpired { resource_id: Option<String> },
    #[error("invalid input: {0}")]
    Invalid(&'static str),
    #[error("retry transaction")]
    Retryable,
    #[error("legacy recipe authoring is unavailable during native migration")]
    LegacyAuthoringUnavailable,
    #[error(transparent)]
    Database(#[from] krausen_core::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameResult {
    pub workspace_id: String,
    pub revision: i64,
}

/// Project the internal command result to the same sanitized adapter contract.
pub fn project_rename_result(
    result: Result<RenameResult, Error>,
) -> krausen_types::workspace::WorkspaceRenameOutcome {
    use krausen_types::workspace::WorkspaceRenameOutcome as Outcome;
    match result {
        Ok(result) => Outcome::Saved {
            revision: result.revision,
        },
        Err(Error::Conflict { expected, actual }) => Outcome::Conflict { expected, actual },
        Err(Error::RequestIdentityConflict) => Outcome::RequestIdentityConflict,
        Err(Error::ResultExpired { resource_id }) => Outcome::ResultExpired { resource_id },
        Err(Error::Forbidden) => Outcome::Forbidden,
        Err(Error::Invalid(_)) => Outcome::Invalid,
        Err(Error::Retryable) => Outcome::Retryable,
        Err(e) => {
            tracing::error!(error = %e, "workspace rename failed");
            Outcome::Unavailable
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkspaceSnapshot {
    pub name: Option<String>,
    pub revision: i64,
}

// Every SQL statement below executes on the borrowed connection, never the pool.
macro_rules! query {
    ($tx:expr, $sql:expr $(, $arg:expr)* ) => {
        match $tx {
            DbTx::Postgres(t) => sqlx::query($sql)$(.bind($arg))*.execute(&mut **t).await.map(|r| r.rows_affected()),
            DbTx::Sqlite(t) => sqlx::query($sql)$(.bind($arg))*.execute(&mut **t).await.map(|r| r.rows_affected()),
        }
    };
}
macro_rules! scalar {
    ($tx:expr, $ty:ty, $sql:expr $(, $arg:expr)* ) => {
        match $tx {
            DbTx::Postgres(t) => sqlx::query_scalar::<_, $ty>($sql)$(.bind($arg))*.fetch_optional(&mut **t).await,
            DbTx::Sqlite(t) => sqlx::query_scalar::<_, $ty>($sql)$(.bind($arg))*.fetch_optional(&mut **t).await,
        }
    };
}
pub mod invitation;
pub mod security_status;
pub mod account_read;
pub mod account_update;
pub mod session_read;
pub mod invitation_read;
pub mod team_invitation_read;
pub mod team_read;
fn db_error(e: sqlx::Error) -> Error {
    if let sqlx::Error::Database(ref d) = e {
        if matches!(
            d.code().as_deref(),
            Some("40001" | "40P01" | "5" | "6" | "517")
        ) {
            return Error::Retryable;
        }
    }
    Error::Database(e.into())
}
fn boundary_error(e: krausen_core::Error) -> Error {
    match e {
        krausen_core::Error::Sqlx(e) => db_error(e),
        e => Error::Database(e),
    }
}

/// Scope and semantic digest of a request authenticated by the adapter.
pub struct RequestIdentity<'a> {
    pub actor: &'a ActorContext,
    pub operation: &'a str,
    pub request_id: &'a str,
    pub payload_digest: &'a str,
}

/// Check only after current authorization. Call before revision comparisons.
pub async fn recover_request(
    tx: &mut DbTx<'_>,
    key: &RequestIdentity<'_>,
) -> Result<Option<serde_json::Value>, Error> {
    let actor = key.actor;
    let digest = scalar!(tx, String, "SELECT payload_digest FROM app_requests WHERE actor_id = $1 AND workspace_id = $2 AND operation = $3 AND request_id = $4", &actor.actor_id, &actor.workspace_id, key.operation, key.request_id).map_err(db_error)?;
    let Some(digest) = digest else {
        return Ok(None);
    };
    if digest != key.payload_digest {
        return Err(Error::RequestIdentityConflict);
    }
    let now = chrono::Utc::now().timestamp();
    let result = scalar!(tx, String, "SELECT result_json FROM app_requests WHERE actor_id = $1 AND workspace_id = $2 AND operation = $3 AND request_id = $4 AND result_expires_at > $5 AND result_json IS NOT NULL", &actor.actor_id, &actor.workspace_id, key.operation, key.request_id, now).map_err(db_error)?;
    if let Some(result) = result {
        return Ok(Some(serde_json::from_str(&result)?));
    }
    let resource_id = scalar!(tx, Option<String>, "SELECT resource_id FROM app_requests WHERE actor_id = $1 AND workspace_id = $2 AND operation = $3 AND request_id = $4", &actor.actor_id, &actor.workspace_id, key.operation, key.request_id).map_err(db_error)?.flatten();
    Err(Error::ResultExpired { resource_id })
}

/// Record a sanitized result and lifetime tombstone in the caller's transaction.
pub async fn record_request(
    tx: &mut DbTx<'_>,
    key: &RequestIdentity<'_>,
    result: &serde_json::Value,
    resource_id: Option<&str>,
) -> Result<(), Error> {
    let actor = key.actor;
    let result = serde_json::to_string(result)?;
    let expiry = chrono::Utc::now().timestamp() + 30 * 86400;
    let inserted = query!(
        tx,
        "INSERT INTO app_requests (actor_id, workspace_id, operation, request_id, payload_digest, result_json, result_expires_at, resource_id) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        &actor.actor_id,
        &actor.workspace_id,
        key.operation,
        key.request_id,
        key.payload_digest,
        &result,
        expiry,
        resource_id
    );
    match inserted {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(ref e)) if e.is_unique_violation() => Err(Error::Retryable),
        Err(e) => Err(db_error(e)),
    }
}

pub async fn record_change(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    operation: &str,
    resource_id: &str,
    revision: i64,
) -> Result<(), Error> {
    query!(tx, "INSERT INTO app_changes (change_id, workspace_id, actor_id, operation, resource_id, revision, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)", uuid::Uuid::new_v4().to_string(), &actor.workspace_id, &actor.actor_id, operation, resource_id, revision, chrono::Utc::now().timestamp()).map_err(db_error)?;
    Ok(())
}

/// Lock and recheck current authority inside the caller's write transaction.
/// The adapter is responsible for verifying the credential before constructing
/// this context; a workspace ID alone never grants authority.
pub async fn authorize_workspace(tx: &mut DbTx<'_>, actor: &ActorContext) -> Result<String, Error> {
    let pg = matches!(tx, DbTx::Postgres(_));
    let account_sql = if pg {
        "SELECT user_id FROM users WHERE user_id = $1 AND active = TRUE FOR SHARE"
    } else {
        "SELECT user_id FROM users WHERE user_id = $1 AND active = 1"
    };
    if scalar!(tx, String, account_sql, &actor.actor_id)
        .map_err(db_error)?
        .is_none()
    {
        return Err(Error::Forbidden);
    }
    let credential_sql = if pg {
        "SELECT credential_id FROM auth_credentials WHERE credential_id = $1 AND user_id = $2 AND revoked_at IS NULL AND expires_at > $3 FOR SHARE"
    } else {
        "SELECT credential_id FROM auth_credentials WHERE credential_id = $1 AND user_id = $2 AND revoked_at IS NULL AND expires_at > $3"
    };
    if scalar!(
        tx,
        String,
        credential_sql,
        &actor.credential_id,
        &actor.actor_id,
        chrono::Utc::now().timestamp()
    )
    .map_err(db_error)?
    .is_none()
    {
        return Err(Error::Forbidden);
    }
    let member_sql = if pg {
        "SELECT role FROM workspace_users WHERE workspace_id = $1 AND user_id = $2 AND active = TRUE FOR SHARE"
    } else {
        "SELECT role FROM workspace_users WHERE workspace_id = $1 AND user_id = $2 AND active = 1"
    };
    let role = scalar!(tx, String, member_sql, &actor.workspace_id, &actor.actor_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    // Lock waits can cross either expiry boundary. Re-read with the current
    // clock after acquiring authority locks.
    if actor.access_expires_at <= chrono::Utc::now().timestamp()
        || scalar!(
            tx,
            String,
            credential_sql,
            &actor.credential_id,
            &actor.actor_id,
            chrono::Utc::now().timestamp()
        )
        .map_err(db_error)?
        .is_none()
    {
        return Err(Error::Forbidden);
    }
    Ok(role)
}

/// Read the name and revision as one snapshot under current authority.
pub async fn workspace_snapshot(
    pool: &DbPool,
    actor: &ActorContext,
) -> Result<WorkspaceSnapshot, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    if authorize_workspace(&mut tx, actor).await? != "workspace_admin" {
        return Err(Error::Forbidden);
    }
    let sql = "SELECT name, revision FROM workspaces WHERE workspace_id = $1 AND status = 'active'";
    let row = match &mut tx {
        DbTx::Postgres(t) => {
            sqlx::query_as::<_, WorkspaceSnapshot>(sql)
                .bind(&actor.workspace_id)
                .fetch_optional(&mut **t)
                .await
        }
        DbTx::Sqlite(t) => {
            sqlx::query_as::<_, WorkspaceSnapshot>(sql)
                .bind(&actor.workspace_id)
                .fetch_optional(&mut **t)
                .await
        }
    }
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(row)
}

/// Rename an existing workspace with current authorization and durable retry.
/// The adapter supplies a stable request UUID and the revision it observed.
pub async fn rename_workspace(
    pool: &DbPool,
    actor: &ActorContext,
    name: &str,
    expected_revision: i64,
    request_id: uuid::Uuid,
) -> Result<RenameResult, Error> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 255 {
        return Err(Error::Invalid("workspace name"));
    }
    let payload = serde_json::to_vec(&(name, expected_revision))?;
    let digest = format!("{:x}", Sha256::digest(payload));
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let result = rename_in_tx(
        &mut tx,
        actor,
        name,
        expected_revision,
        &request_id.to_string(),
        &digest,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(result)
}

async fn rename_in_tx(
    tx: &mut DbTx<'_>,
    actor: &ActorContext,
    name: &str,
    expected: i64,
    request_id: &str,
    digest: &str,
) -> Result<RenameResult, Error> {
    let pg = matches!(tx, DbTx::Postgres(_));
    // The account and membership locks serialize with legacy UPDATE/DELETE revocations.
    if authorize_workspace(tx, actor).await? != "workspace_admin" {
        return Err(Error::Forbidden);
    }
    // Lock the resource before checking retries, serializing concurrent identical keys.
    let ws_sql = if pg {
        "SELECT revision FROM workspaces WHERE workspace_id = $1 AND status = 'active' FOR UPDATE"
    } else {
        "SELECT revision FROM workspaces WHERE workspace_id = $1 AND status = 'active'"
    };
    let current = scalar!(tx, i64, ws_sql, &actor.workspace_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    // The resource lock may have waited past token expiry.
    authorize_workspace(tx, actor).await?;
    let key = RequestIdentity {
        actor,
        operation: "workspace.rename.v1",
        request_id,
        payload_digest: digest,
    };
    if let Some(result) = recover_request(tx, &key).await? {
        return Ok(serde_json::from_value(result)?);
    }
    if current != expected {
        return Err(Error::Conflict {
            expected,
            actual: current,
        });
    }
    let revision = current + 1;
    let updated = query!(tx, "UPDATE workspaces SET name = $1, revision = $2, updated_at = CURRENT_TIMESTAMP WHERE workspace_id = $3 AND revision = $4", name, revision, &actor.workspace_id, current).map_err(db_error)?;
    if updated != 1 {
        return Err(Error::Conflict {
            expected,
            actual: current,
        });
    }
    let result = RenameResult {
        workspace_id: actor.workspace_id.clone(),
        revision,
    };
    // Preserve the existing sync protocol while making its record atomic.
    let settings = scalar!(
        tx,
        String,
        "SELECT COALESCE(CAST(settings AS TEXT), 'null') FROM workspaces WHERE workspace_id = $1",
        &actor.workspace_id
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    let updated_at = scalar!(
        tx,
        String,
        "SELECT CAST(updated_at AS TEXT) FROM workspaces WHERE workspace_id = $1",
        &actor.workspace_id
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
    let snapshot = serde_json::json!({"workspace_id": actor.workspace_id, "name": name, "settings": serde_json::from_str::<serde_json::Value>(&settings)?, "updated_at": updated_at}).to_string();
    let sync_sql = if pg {
        "INSERT INTO sync_log (entity_type, entity_id, workspace_id, action, data) VALUES ('workspace_settings', $1, $1, 'update', CAST($2 AS JSONB))"
    } else {
        "INSERT INTO sync_log (entity_type, entity_id, workspace_id, action, data) VALUES ('workspace_settings', $1, $1, 'update', $2)"
    };
    query!(tx, sync_sql, &actor.workspace_id, &snapshot).map_err(db_error)?;
    record_request(
        tx,
        &key,
        &serde_json::to_value(&result)?,
        Some(&actor.workspace_id),
    )
    .await?;
    record_change(
        tx,
        actor,
        "workspace.rename.v1",
        &actor.workspace_id,
        revision,
    )
    .await?;
    Ok(result)
}

/// Add a required external effect to the caller's transaction. A worker must
/// deliver it only after commit, and mark delivery separately.
pub async fn enqueue_outbox(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    operation: &str,
    payload: &serde_json::Value,
) -> Result<String, Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let payload = serde_json::to_string(payload)?;
    let now = chrono::Utc::now().timestamp();
    query!(tx, "INSERT INTO app_outbox (outbox_id, workspace_id, operation, payload_json, created_at) VALUES ($1, $2, $3, $4, $5)", &id, workspace_id, operation, &payload, now).map_err(db_error)?;
    Ok(id)
}

pub mod formulation;
pub mod interchange;

pub mod recipe_editor;
mod style_reference;

pub mod equipment;

pub mod recipe_library;
pub mod process_summary;

pub mod catalogue;

pub mod scaling;
