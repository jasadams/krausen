// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recipient-owned pending invitations, independent of workspace membership.
use crate::{
    Error, boundary_error, db_error,
    invitation::{AccountActor, authorize_account, recheck_credential},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use krausen_core::{DbPool, DbTx};
use krausen_types::invitation_read::{
    RecipientInvitation, RecipientInvitationListOutcome, RecipientInvitationPage,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Cursor {
    version: u8,
    actor_id: String,
    order: String,
    created_at: chrono::DateTime<chrono::Utc>,
    invitation_id: String,
}
#[derive(sqlx::FromRow)]
struct Row {
    invitation_id: String,
    workspace_id: String,
    email: String,
    role: String,
    created_at: chrono::DateTime<chrono::Utc>,
    expires_at: chrono::DateTime<chrono::Utc>,
}

pub async fn recipient_invitations(
    pool: &DbPool,
    actor: &AccountActor,
    cursor: Option<&str>,
    limit: i64,
) -> Result<RecipientInvitationPage, Error> {
    if !(1..=100).contains(&limit) {
        return Err(Error::Invalid("invitation list limit"));
    }
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let email = authorize_account(&mut tx, actor).await?;
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
                || value.actor_id != actor.actor_id
                || value.order != "created_at_desc,invitation_id_desc"
                || value.invitation_id.is_empty()
            {
                return Err(Error::Invalid("cursor"));
            }
            Some(value)
        }
    };
    let common = "SELECT invitation_id,workspace_id,email,role,created_at,expires_at FROM workspace_invitations WHERE LOWER(email)=LOWER($1) AND status='pending'";
    let pg_first = format!(
        "{common} AND expires_at>clock_timestamp() ORDER BY created_at DESC,invitation_id DESC LIMIT $2"
    );
    let sq_first = format!(
        "{common} AND julianday(expires_at)>julianday('now') ORDER BY julianday(created_at) DESC,invitation_id DESC LIMIT $2"
    );
    let pg_next = format!(
        "{common} AND expires_at>clock_timestamp() AND (created_at<$2 OR (created_at=$2 AND invitation_id<$3)) ORDER BY created_at DESC,invitation_id DESC LIMIT $4"
    );
    let sq_next = format!(
        "{common} AND julianday(expires_at)>julianday('now') AND (julianday(created_at)<julianday($2) OR (julianday(created_at)=julianday($2) AND invitation_id<$3)) ORDER BY julianday(created_at) DESC,invitation_id DESC LIMIT $4"
    );
    let mut rows = match (&mut tx, &after) {
        (DbTx::Postgres(t), None) => {
            sqlx::query_as::<_, Row>(&pg_first)
                .bind(&email)
                .bind(limit + 1)
                .fetch_all(&mut **t)
                .await
        }
        (DbTx::Sqlite(t), None) => {
            sqlx::query_as::<_, Row>(&sq_first)
                .bind(&email)
                .bind(limit + 1)
                .fetch_all(&mut **t)
                .await
        }
        (DbTx::Postgres(t), Some(c)) => {
            sqlx::query_as::<_, Row>(&pg_next)
                .bind(&email)
                .bind(c.created_at)
                .bind(&c.invitation_id)
                .bind(limit + 1)
                .fetch_all(&mut **t)
                .await
        }
        (DbTx::Sqlite(t), Some(c)) => {
            sqlx::query_as::<_, Row>(&sq_next)
                .bind(&email)
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
            actor_id: actor.actor_id.clone(),
            order: "created_at_desc,invitation_id_desc".into(),
            created_at: last.created_at,
            invitation_id: last.invitation_id.clone(),
        })?))
    } else {
        None
    };
    recheck_credential(&mut tx, actor).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(RecipientInvitationPage {
        items: rows
            .into_iter()
            .map(|r| RecipientInvitation {
                invitation_id: r.invitation_id,
                workspace_id: r.workspace_id,
                email: r.email,
                role: r.role,
                created_at: r.created_at.to_rfc3339(),
                expires_at: r.expires_at.to_rfc3339(),
            })
            .collect(),
        next_cursor,
    })
}

pub fn project(result: Result<RecipientInvitationPage, Error>) -> RecipientInvitationListOutcome {
    match result {
        Ok(page) => RecipientInvitationListOutcome::Available { page },
        Err(Error::Forbidden) => RecipientInvitationListOutcome::Forbidden,
        Err(Error::Invalid(_)) => RecipientInvitationListOutcome::Invalid,
        Err(Error::Retryable) => RecipientInvitationListOutcome::Retryable,
        Err(error) => {
            tracing::error!(%error, "recipient invitation inventory failed");
            RecipientInvitationListOutcome::Unavailable
        }
    }
}
