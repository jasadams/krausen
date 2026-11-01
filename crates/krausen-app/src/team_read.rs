// SPDX-License-Identifier: AGPL-3.0-or-later
//! Workspace member inventory under the shared membership boundary.
use crate::{ActorContext, Error, authorize_workspace, boundary_error, db_error};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use krausen_core::{DbPool, DbTx};
use krausen_types::team_read::{Member, MemberListOutcome, MemberPage};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Cursor {
    version: u8,
    workspace_id: String,
    order: String,
    created_at: chrono::DateTime<chrono::Utc>,
    user_id: String,
}

#[derive(sqlx::FromRow)]
struct Row {
    user_id: String,
    email: String,
    name: Option<String>,
    role: String,
    is_owner: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// Ordinary active members can read the member inventory, not only admins.
pub async fn members(
    pool: &DbPool,
    actor: &ActorContext,
    cursor: Option<&str>,
    limit: i64,
) -> Result<MemberPage, Error> {
    if !(1..=100).contains(&limit) {
        return Err(Error::Invalid("member list limit"));
    }
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_workspace(&mut tx, actor).await?;
    let after = match cursor {
        None => None,
        Some(encoded) => {
            if encoded.len() > 1024 {
                return Err(Error::Invalid("cursor"));
            }
            let bytes = URL_SAFE_NO_PAD
                .decode(encoded)
                .map_err(|_| Error::Invalid("cursor"))?;
            let value: Cursor =
                serde_json::from_slice(&bytes).map_err(|_| Error::Invalid("cursor"))?;
            if value.version != 1
                || value.workspace_id != actor.workspace_id
                || value.order != "created_at,user_id"
                || value.user_id.is_empty()
            {
                return Err(Error::Invalid("cursor"));
            }
            Some(value)
        }
    };
    let pg_first = "SELECT m.user_id,u.email,u.name,m.role,(w.owner_user_id=m.user_id) AS is_owner,m.created_at FROM workspace_users m JOIN users u ON u.user_id=m.user_id JOIN workspaces w ON w.workspace_id=m.workspace_id WHERE m.workspace_id=$1 AND m.active=TRUE AND w.status='active' ORDER BY m.created_at,m.user_id LIMIT $2";
    let sqlite_first = "SELECT m.user_id,u.email,u.name,m.role,(w.owner_user_id=m.user_id) AS is_owner,m.created_at FROM workspace_users m JOIN users u ON u.user_id=m.user_id JOIN workspaces w ON w.workspace_id=m.workspace_id WHERE m.workspace_id=$1 AND m.active=TRUE AND w.status='active' ORDER BY julianday(m.created_at),m.user_id LIMIT $2";
    let pg_next = "SELECT m.user_id,u.email,u.name,m.role,(w.owner_user_id=m.user_id) AS is_owner,m.created_at FROM workspace_users m JOIN users u ON u.user_id=m.user_id JOIN workspaces w ON w.workspace_id=m.workspace_id WHERE m.workspace_id=$1 AND m.active=TRUE AND w.status='active' AND (m.created_at>$2 OR (m.created_at=$2 AND m.user_id>$3)) ORDER BY m.created_at,m.user_id LIMIT $4";
    let sqlite_next = "SELECT m.user_id,u.email,u.name,m.role,(w.owner_user_id=m.user_id) AS is_owner,m.created_at FROM workspace_users m JOIN users u ON u.user_id=m.user_id JOIN workspaces w ON w.workspace_id=m.workspace_id WHERE m.workspace_id=$1 AND m.active=TRUE AND w.status='active' AND (julianday(m.created_at)>julianday($2) OR (julianday(m.created_at)=julianday($2) AND m.user_id>$3)) ORDER BY julianday(m.created_at),m.user_id LIMIT $4";
    let mut rows = match (&mut tx, &after) {
        (DbTx::Postgres(t), None) => {
            sqlx::query_as::<_, Row>(pg_first)
                .bind(&actor.workspace_id)
                .bind(limit + 1)
                .fetch_all(&mut **t)
                .await
        }
        (DbTx::Sqlite(t), None) => {
            sqlx::query_as::<_, Row>(sqlite_first)
                .bind(&actor.workspace_id)
                .bind(limit + 1)
                .fetch_all(&mut **t)
                .await
        }
        (DbTx::Postgres(t), Some(c)) => {
            sqlx::query_as::<_, Row>(pg_next)
                .bind(&actor.workspace_id)
                .bind(c.created_at)
                .bind(&c.user_id)
                .bind(limit + 1)
                .fetch_all(&mut **t)
                .await
        }
        (DbTx::Sqlite(t), Some(c)) => {
            sqlx::query_as::<_, Row>(sqlite_next)
                .bind(&actor.workspace_id)
                .bind(c.created_at)
                .bind(&c.user_id)
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
        Some(URL_SAFE_NO_PAD.encode(serde_json::to_vec(&Cursor {
            version: 1,
            workspace_id: actor.workspace_id.clone(),
            order: "created_at,user_id".into(),
            created_at: last.created_at,
            user_id: last.user_id.clone(),
        })?))
    } else {
        None
    };
    authorize_workspace(&mut tx, actor).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(MemberPage {
        items: rows
            .into_iter()
            .map(|row| Member {
                user_id: row.user_id,
                email: row.email,
                name: row.name,
                role: row.role,
                is_owner: row.is_owner,
                joined_at: row.created_at.to_rfc3339(),
            })
            .collect(),
        next_cursor,
    })
}

pub fn project_members(result: Result<MemberPage, Error>) -> MemberListOutcome {
    match result {
        Ok(page) => MemberListOutcome::Available { page },
        Err(Error::Forbidden) => MemberListOutcome::Forbidden,
        Err(Error::Invalid(_)) => MemberListOutcome::Invalid,
        Err(Error::Retryable) => MemberListOutcome::Retryable,
        Err(error) => {
            tracing::error!(%error, "member inventory failed");
            MemberListOutcome::Unavailable
        }
    }
}
