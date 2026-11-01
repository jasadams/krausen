// SPDX-License-Identifier: AGPL-3.0-or-later
//! Account profile read shared by UI and MCP.
use crate::{
    Error, boundary_error, db_error,
    invitation::{AccountActor, authorize_account, recheck_credential},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use krausen_core::{DbPool, DbTx};
use krausen_types::account_read::{
    AccountProfile, AccountProfileOutcome, AccountWorkspace, AccountWorkspaceListOutcome,
    AccountWorkspacePage,
};
use serde::{Deserialize, Serialize};

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
    let settings_revision = scalar!(&mut tx, i64, "SELECT settings_revision FROM users WHERE user_id=$1", &actor.actor_id)
        .map_err(db_error)?.ok_or(Error::Forbidden)?;
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
        settings_revision,
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

#[derive(Serialize, Deserialize)]
struct WorkspaceCursor {
    version: u8,
    actor_id: String,
    order: String,
    created_at: chrono::DateTime<chrono::Utc>,
    workspace_id: String,
}

#[derive(sqlx::FromRow)]
struct WorkspaceRow {
    workspace_id: String,
    name: Option<String>,
    role: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// List a bounded page of this account's active memberships, independent of
/// any other client's workspace selection.
pub async fn workspaces(
    pool: &DbPool,
    actor: &AccountActor,
    cursor: Option<&str>,
    limit: i64,
) -> Result<AccountWorkspacePage, Error> {
    if !(1..=100).contains(&limit) {
        return Err(Error::Invalid("workspace list limit"));
    }
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_account(&mut tx, actor).await?;
    let after = match cursor {
        None => None,
        Some(encoded) => {
            if encoded.len() > 1024 {
                return Err(Error::Invalid("cursor"));
            }
            let bytes = URL_SAFE_NO_PAD
                .decode(encoded)
                .map_err(|_| Error::Invalid("cursor"))?;
            let value: WorkspaceCursor =
                serde_json::from_slice(&bytes).map_err(|_| Error::Invalid("cursor"))?;
            if value.version != 1
                || value.actor_id != actor.actor_id
                || value.order != "created_at,workspace_id"
                || value.workspace_id.is_empty()
            {
                return Err(Error::Invalid("cursor"));
            }
            Some(value)
        }
    };
    let pg_initial = "SELECT w.workspace_id,w.name,m.role,m.created_at FROM workspace_users m JOIN workspaces w ON w.workspace_id=m.workspace_id WHERE m.user_id=$1 AND m.active=TRUE ORDER BY m.created_at,w.workspace_id LIMIT $2";
    let sqlite_initial = "SELECT w.workspace_id,w.name,m.role,m.created_at FROM workspace_users m JOIN workspaces w ON w.workspace_id=m.workspace_id WHERE m.user_id=$1 AND m.active=TRUE ORDER BY julianday(m.created_at),w.workspace_id LIMIT $2";
    let pg_next = "SELECT w.workspace_id,w.name,m.role,m.created_at FROM workspace_users m JOIN workspaces w ON w.workspace_id=m.workspace_id WHERE m.user_id=$1 AND m.active=TRUE AND (m.created_at>$2 OR (m.created_at=$2 AND w.workspace_id>$3)) ORDER BY m.created_at,w.workspace_id LIMIT $4";
    let sqlite_next = "SELECT w.workspace_id,w.name,m.role,m.created_at FROM workspace_users m JOIN workspaces w ON w.workspace_id=m.workspace_id WHERE m.user_id=$1 AND m.active=TRUE AND (julianday(m.created_at)>julianday($2) OR (julianday(m.created_at)=julianday($2) AND w.workspace_id>$3)) ORDER BY julianday(m.created_at),w.workspace_id LIMIT $4";
    let mut rows = match (&mut tx, &after) {
        (DbTx::Postgres(t), None) => {
            sqlx::query_as::<_, WorkspaceRow>(pg_initial)
                .bind(&actor.actor_id)
                .bind(limit + 1)
                .fetch_all(&mut **t)
                .await
        }
        (DbTx::Sqlite(t), None) => {
            sqlx::query_as::<_, WorkspaceRow>(sqlite_initial)
                .bind(&actor.actor_id)
                .bind(limit + 1)
                .fetch_all(&mut **t)
                .await
        }
        (DbTx::Postgres(t), Some(c)) => {
            sqlx::query_as::<_, WorkspaceRow>(pg_next)
                .bind(&actor.actor_id)
                .bind(c.created_at)
                .bind(&c.workspace_id)
                .bind(limit + 1)
                .fetch_all(&mut **t)
                .await
        }
        (DbTx::Sqlite(t), Some(c)) => {
            sqlx::query_as::<_, WorkspaceRow>(sqlite_next)
                .bind(&actor.actor_id)
                .bind(c.created_at)
                .bind(&c.workspace_id)
                .bind(limit + 1)
                .fetch_all(&mut **t)
                .await
        }
    }
    .map_err(db_error)?;
    let more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let next_cursor = if more {
        let last = rows.last().ok_or(Error::Invalid("cursor"))?;
        Some(URL_SAFE_NO_PAD.encode(serde_json::to_vec(&WorkspaceCursor {
            version: 1,
            actor_id: actor.actor_id.clone(),
            order: "created_at,workspace_id".into(),
            created_at: last.created_at,
            workspace_id: last.workspace_id.clone(),
        })?))
    } else {
        None
    };
    recheck_credential(&mut tx, actor).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(AccountWorkspacePage {
        items: rows
            .into_iter()
            .map(|row| AccountWorkspace {
                workspace_id: row.workspace_id,
                name: row.name,
                role: row.role,
            })
            .collect(),
        next_cursor,
    })
}

pub fn project_workspaces(
    result: Result<AccountWorkspacePage, Error>,
) -> AccountWorkspaceListOutcome {
    match result {
        Ok(page) => AccountWorkspaceListOutcome::Available { page },
        Err(Error::Forbidden) => AccountWorkspaceListOutcome::Forbidden,
        Err(Error::Invalid(_)) => AccountWorkspaceListOutcome::Invalid,
        Err(Error::Retryable) => AccountWorkspaceListOutcome::Retryable,
        Err(error) => {
            tracing::error!(%error, "workspace inventory failed");
            AccountWorkspaceListOutcome::Unavailable
        }
    }
}
