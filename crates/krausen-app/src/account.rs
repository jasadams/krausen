// SPDX-License-Identifier: AGPL-3.0-or-later
//! Account operations shared by browser and MCP adapters.
use crate::{ActorContext, Error, authorize_account, boundary_error, db_error};
use krausen_core::{DbPool, DbTx};
use krausen_types::account::Profile;

mod write;
pub use write::save;

/// Shared, sanitized command outcome for all adapters.
pub fn project_save(result: Result<Profile, Error>) -> krausen_types::account::ProfileSaveOutcome {
    use krausen_types::account::ProfileSaveOutcome as Outcome;
    match result {
        Ok(profile) => Outcome::Saved(profile),
        Err(Error::Conflict { expected, actual }) => Outcome::Conflict { expected, actual },
        Err(Error::RequestIdentityConflict) => Outcome::RequestIdentityConflict,
        Err(Error::ResultExpired { .. }) => Outcome::ResultExpired,
        Err(Error::Forbidden) => Outcome::Forbidden,
        Err(Error::Invalid(_)) => Outcome::Invalid,
        Err(Error::Retryable) => Outcome::Retryable,
        Err(error) => {
            tracing::error!(%error, "profile save failed");
            Outcome::Unavailable
        }
    }
}

#[derive(sqlx::FromRow)]
struct MembershipRow {
    workspace_id: String,
    name: Option<String>,
    role: String,
}

/// List only active memberships of the authenticated account.
pub async fn workspaces(
    pool: &DbPool,
    actor: &ActorContext,
) -> Result<Vec<krausen_types::account::WorkspaceMembership>, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_account(&mut tx, actor).await?;
    let sql = "SELECT w.workspace_id, w.name, m.role FROM workspaces w JOIN workspace_users m ON m.workspace_id = w.workspace_id WHERE m.user_id = $1 AND m.active = TRUE AND w.status = 'active' ORDER BY w.workspace_id";
    let rows = match &mut tx {
        krausen_core::DbTx::Postgres(t) => {
            sqlx::query_as::<_, MembershipRow>(sql)
                .bind(&actor.actor_id)
                .fetch_all(&mut **t)
                .await
        }
        krausen_core::DbTx::Sqlite(t) => {
            sqlx::query_as::<_, MembershipRow>(sql)
                .bind(&actor.actor_id)
                .fetch_all(&mut **t)
                .await
        }
    }
    .map_err(db_error)?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(rows
        .into_iter()
        .map(|r| krausen_types::account::WorkspaceMembership {
            workspace_id: r.workspace_id,
            name: r.name,
            role: r.role,
        })
        .collect())
}

/// Select a currently authorized membership. No credential or token is stored in the retry result.
pub async fn select_workspace(
    pool: &DbPool,
    actor: &ActorContext,
    workspace_id: &str,
    request_id: uuid::Uuid,
) -> Result<krausen_types::account::WorkspaceMembership, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    // Lock the account exclusively first: updating its preference after taking a
    // shared lock would deadlock concurrent PostgreSQL selectors.
    let sql = if matches!(tx, krausen_core::DbTx::Postgres(_)) {
        "SELECT user_id FROM users WHERE user_id = $1 AND active = TRUE FOR UPDATE"
    } else {
        "SELECT user_id FROM users WHERE user_id = $1 AND active = TRUE"
    };
    if scalar!(&mut tx, String, sql, &actor.actor_id)
        .map_err(db_error)?
        .is_none()
    {
        return Err(Error::Forbidden);
    }
    let scoped = ActorContext {
        workspace_id: workspace_id.to_owned(),
        ..actor.clone()
    };
    let role = crate::authorize_workspace(&mut tx, &scoped).await?;
    let sql = if matches!(tx, krausen_core::DbTx::Postgres(_)) {
        "SELECT name FROM workspaces WHERE workspace_id = $1 AND status = 'active' FOR SHARE"
    } else {
        "SELECT name FROM workspaces WHERE workspace_id = $1 AND status = 'active'"
    };
    let name = scalar!(&mut tx, Option<String>, sql, workspace_id)
        .map_err(db_error)?
        .ok_or(Error::Forbidden)?;
    crate::authorize_workspace(&mut tx, &scoped).await?;
    let id = request_id.to_string();
    let digest =
        krausen_core::canonical_json::sha256(&serde_json::json!({"workspace_id": workspace_id}))
            .map_err(|_| Error::Invalid("invalid workspace identity"))?;
    let key = crate::RequestIdentity {
        actor: &scoped,
        operation: "account.workspace.select.v1",
        request_id: &id,
        payload_digest: &digest,
    };
    if let Some(result) = crate::recover_request(&mut tx, &key).await? {
        let result = serde_json::from_value(result)?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(result);
    }
    query!(
        &mut tx,
        "UPDATE users SET last_workspace_id = $1 WHERE user_id = $2",
        workspace_id,
        &actor.actor_id
    )
    .map_err(db_error)?;
    let result = krausen_types::account::WorkspaceMembership {
        workspace_id: workspace_id.to_owned(),
        name,
        role,
    };
    crate::record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&result)?,
        Some(workspace_id),
    )
    .await?;
    crate::record_change(
        &mut tx,
        &scoped,
        "account.workspace.select.v1",
        workspace_id,
        0,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(result)
}

#[derive(sqlx::FromRow)]
struct ProfileRow {
    user_id: String,
    email: String,
    name: Option<String>,
    metadata: Option<String>,
    revision: i64,
}

/// Read only the current authenticated account, independently of workspace selection.
pub async fn profile(pool: &DbPool, actor: &ActorContext) -> Result<Profile, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_account(&mut tx, actor).await?;
    let profile = profile_in_tx(&mut tx, actor).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(profile)
}

async fn profile_in_tx(tx: &mut DbTx<'_>, actor: &ActorContext) -> Result<Profile, Error> {
    let row = krausen_core::dbtx_fetch_optional!(tx, ProfileRow, "SELECT user_id, email, name, CAST(extra_metadata AS TEXT) AS metadata, account_revision AS revision FROM users WHERE user_id = $1", &actor.actor_id).map_err(db_error)?.ok_or(Error::Forbidden)?;
    let metadata = match row.metadata {
        Some(value) => serde_json::from_str::<serde_json::Value>(&value)?,
        None => serde_json::Value::Null,
    };
    let theme = metadata
        .get("theme")
        .and_then(|v| v.as_str())
        .unwrap_or("system")
        .to_owned();
    Ok(Profile {
        user_id: row.user_id,
        email: row.email,
        name: row.name,
        theme,
        revision: row.revision,
    })
}

/// Return current account and, when supplied, an independently authorized workspace.
pub async fn context(
    pool: &DbPool,
    actor: &ActorContext,
) -> Result<krausen_types::account::AccountContext, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_account(&mut tx, actor).await?;
    let profile = profile_in_tx(&mut tx, actor).await?;
    let (workspace, is_owner) = if actor.workspace_id.is_empty() {
        (None, false)
    } else {
        let role = crate::authorize_workspace(&mut tx, actor).await?;
        #[derive(sqlx::FromRow)]
        struct Workspace {
            name: Option<String>,
            owner_user_id: String,
        }
        let sql = if matches!(tx, DbTx::Postgres(_)) {
            "SELECT name,owner_user_id FROM workspaces WHERE workspace_id=$1 AND status='active' FOR SHARE"
        } else {
            "SELECT name,owner_user_id FROM workspaces WHERE workspace_id=$1 AND status='active'"
        };
        let row = krausen_core::dbtx_fetch_optional!(&mut tx, Workspace, sql, &actor.workspace_id)
            .map_err(db_error)?
            .ok_or(Error::Forbidden)?;
        crate::authorize_workspace(&mut tx, actor).await?;
        (
            Some(krausen_types::account::WorkspaceMembership {
                workspace_id: actor.workspace_id.clone(),
                name: row.name,
                role,
            }),
            row.owner_user_id == actor.actor_id,
        )
    };
    tx.commit().await.map_err(boundary_error)?;
    Ok(krausen_types::account::AccountContext {
        profile,
        workspace,
        is_owner,
    })
}
