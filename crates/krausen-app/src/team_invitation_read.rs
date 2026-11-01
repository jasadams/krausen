// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pending workspace invitation inventory under current administrator authority.
use crate::{ActorContext, Error, authorize_workspace, boundary_error, db_error};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use krausen_core::{DbPool, DbTx};
use krausen_types::team_invitation_read::{
    TeamInvitation, TeamInvitationListOutcome, TeamInvitationPage,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Cursor {
    version: u8,
    workspace_id: String,
    order: String,
    created_at: chrono::DateTime<chrono::Utc>,
    invitation_id: String,
}
#[derive(sqlx::FromRow)]
struct Row {
    invitation_id: String,
    email: String,
    role: String,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
    expires_at: chrono::DateTime<chrono::Utc>,
}

pub async fn invitations(
    pool: &DbPool,
    actor: &ActorContext,
    cursor: Option<&str>,
    limit: i64,
) -> Result<TeamInvitationPage, Error> {
    if !(1..=100).contains(&limit) {
        return Err(Error::Invalid("invitation list limit"));
    }
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    if authorize_workspace(&mut tx, actor).await? != "workspace_admin" {
        return Err(Error::Forbidden);
    }
    let workspace_sql = if matches!(&tx, DbTx::Postgres(_)) {
        "SELECT workspace_id FROM workspaces WHERE workspace_id=$1 AND status='active' FOR SHARE"
    } else {
        "SELECT workspace_id FROM workspaces WHERE workspace_id=$1 AND status='active'"
    };
    if scalar!(&mut tx, String, workspace_sql, &actor.workspace_id)
        .map_err(db_error)?
        .is_none()
    {
        return Err(Error::Forbidden);
    }
    let after = match cursor {
        None => None,
        Some(encoded) => {
            if encoded.len() > 1024 {
                return Err(Error::Invalid("cursor"));
            }
            let value: Cursor = serde_json::from_slice(
                &URL_SAFE_NO_PAD
                    .decode(encoded)
                    .map_err(|_| Error::Invalid("cursor"))?,
            )
            .map_err(|_| Error::Invalid("cursor"))?;
            if value.version != 1
                || value.workspace_id != actor.workspace_id
                || value.order != "created_at_desc,invitation_id_desc"
                || value.invitation_id.is_empty()
            {
                return Err(Error::Invalid("cursor"));
            }
            Some(value)
        }
    };
    let common = "SELECT i.invitation_id,i.email,i.role,i.status,i.created_at,i.expires_at FROM workspace_invitations i JOIN workspaces w ON w.workspace_id=i.workspace_id WHERE i.workspace_id=$1 AND w.status='active' AND i.status='pending'";
    let pg_first = format!("{common} ORDER BY i.created_at DESC,i.invitation_id DESC LIMIT $2");
    let sq_first =
        format!("{common} ORDER BY julianday(i.created_at) DESC,i.invitation_id DESC LIMIT $2");
    let pg_next = format!(
        "{common} AND (i.created_at<$2 OR (i.created_at=$2 AND i.invitation_id<$3)) ORDER BY i.created_at DESC,i.invitation_id DESC LIMIT $4"
    );
    let sq_next = format!(
        "{common} AND (julianday(i.created_at)<julianday($2) OR (julianday(i.created_at)=julianday($2) AND i.invitation_id<$3)) ORDER BY julianday(i.created_at) DESC,i.invitation_id DESC LIMIT $4"
    );
    let mut rows = match (&mut tx, &after) {
        (DbTx::Postgres(t), None) => {
            sqlx::query_as::<_, Row>(&pg_first)
                .bind(&actor.workspace_id)
                .bind(limit + 1)
                .fetch_all(&mut **t)
                .await
        }
        (DbTx::Sqlite(t), None) => {
            sqlx::query_as::<_, Row>(&sq_first)
                .bind(&actor.workspace_id)
                .bind(limit + 1)
                .fetch_all(&mut **t)
                .await
        }
        (DbTx::Postgres(t), Some(c)) => {
            sqlx::query_as::<_, Row>(&pg_next)
                .bind(&actor.workspace_id)
                .bind(c.created_at)
                .bind(&c.invitation_id)
                .bind(limit + 1)
                .fetch_all(&mut **t)
                .await
        }
        (DbTx::Sqlite(t), Some(c)) => {
            sqlx::query_as::<_, Row>(&sq_next)
                .bind(&actor.workspace_id)
                .bind(c.created_at)
                .bind(&c.invitation_id)
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
            order: "created_at_desc,invitation_id_desc".into(),
            created_at: last.created_at,
            invitation_id: last.invitation_id.clone(),
        })?))
    } else {
        None
    };
    if authorize_workspace(&mut tx, actor).await? != "workspace_admin" {
        return Err(Error::Forbidden);
    }
    tx.commit().await.map_err(boundary_error)?;
    Ok(TeamInvitationPage {
        items: rows
            .into_iter()
            .map(|r| TeamInvitation {
                invitation_id: r.invitation_id,
                email: r.email,
                role: r.role,
                status: r.status,
                created_at: r.created_at.to_rfc3339(),
                expires_at: r.expires_at.to_rfc3339(),
            })
            .collect(),
        next_cursor,
    })
}

pub fn project(result: Result<TeamInvitationPage, Error>) -> TeamInvitationListOutcome {
    match result {
        Ok(page) => TeamInvitationListOutcome::Available { page },
        Err(Error::Forbidden) => TeamInvitationListOutcome::Forbidden,
        Err(Error::Invalid(_)) => TeamInvitationListOutcome::Invalid,
        Err(Error::Retryable) => TeamInvitationListOutcome::Retryable,
        Err(error) => {
            tracing::error!(%error, "team invitation inventory failed");
            TeamInvitationListOutcome::Unavailable
        }
    }
}
