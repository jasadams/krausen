// SPDX-License-Identifier: AGPL-3.0-or-later
//! Account-owned session inventory. No refresh-token secrets cross the boundary.
use crate::{
    Error, boundary_error, db_error,
    invitation::{AccountActor, authorize_account, recheck_credential},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use krausen_core::{DbPool, DbTx};
use krausen_types::session_read::{Session, SessionListOutcome, SessionPage};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Cursor {
    version: u8,
    actor_id: String,
    order: String,
    created_at: chrono::DateTime<chrono::Utc>,
    token_id: String,
}
#[derive(sqlx::FromRow)]
struct Row {
    token_id: String,
    family_id: String,
    created_at: chrono::DateTime<chrono::Utc>,
    last_used: Option<chrono::DateTime<chrono::Utc>>,
    expires_at: chrono::DateTime<chrono::Utc>,
    user_agent: Option<String>,
    ip_address: Option<String>,
    country_code: Option<String>,
    oauth_client_name: Option<String>,
}

pub async fn sessions(
    pool: &DbPool,
    actor: &AccountActor,
    cursor: Option<&str>,
    limit: i64,
) -> Result<SessionPage, Error> {
    if !(1..=100).contains(&limit) {
        return Err(Error::Invalid("session list limit"));
    }
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_account(&mut tx, actor).await?;
    let credential_kind = scalar!(
        &mut tx,
        String,
        "SELECT kind FROM auth_credentials WHERE credential_id=$1 AND user_id=$2",
        &actor.credential_id,
        &actor.actor_id
    )
    .map_err(db_error)?
    .ok_or(Error::Forbidden)?;
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
                || value.actor_id != actor.actor_id
                || value.order != "created_at,token_id"
                || value.token_id.is_empty()
            {
                return Err(Error::Invalid("cursor"));
            }
            Some(value)
        }
    };
    let columns = "SELECT rt.token_id,rt.family_id,rt.created_at,rt.last_used,rt.expires_at,rt.user_agent,rt.ip_address,rt.country_code,oc.name AS oauth_client_name FROM refresh_tokens rt LEFT JOIN oauth_clients oc ON rt.oauth_client_id=oc.client_id WHERE rt.user_id=$1 AND rt.is_active=TRUE AND rt.replaced_at IS NULL";
    let pg_first =
        format!("{columns} AND rt.expires_at>NOW() ORDER BY rt.created_at,rt.token_id LIMIT $2");
    let sq_first = format!(
        "{columns} AND julianday(rt.expires_at)>julianday('now') ORDER BY julianday(rt.created_at),rt.token_id LIMIT $2"
    );
    let pg_next = format!(
        "{columns} AND rt.expires_at>NOW() AND (rt.created_at>$2 OR (rt.created_at=$2 AND rt.token_id>$3)) ORDER BY rt.created_at,rt.token_id LIMIT $4"
    );
    let sq_next = format!(
        "{columns} AND julianday(rt.expires_at)>julianday('now') AND (julianday(rt.created_at)>julianday($2) OR (julianday(rt.created_at)=julianday($2) AND rt.token_id>$3)) ORDER BY julianday(rt.created_at),rt.token_id LIMIT $4"
    );
    let mut rows = match (&mut tx, &after) {
        (DbTx::Postgres(t), None) => {
            sqlx::query_as::<_, Row>(&pg_first)
                .bind(&actor.actor_id)
                .bind(limit + 1)
                .fetch_all(&mut **t)
                .await
        }
        (DbTx::Sqlite(t), None) => {
            sqlx::query_as::<_, Row>(&sq_first)
                .bind(&actor.actor_id)
                .bind(limit + 1)
                .fetch_all(&mut **t)
                .await
        }
        (DbTx::Postgres(t), Some(c)) => {
            sqlx::query_as::<_, Row>(&pg_next)
                .bind(&actor.actor_id)
                .bind(c.created_at)
                .bind(&c.token_id)
                .bind(limit + 1)
                .fetch_all(&mut **t)
                .await
        }
        (DbTx::Sqlite(t), Some(c)) => {
            sqlx::query_as::<_, Row>(&sq_next)
                .bind(&actor.actor_id)
                .bind(c.created_at)
                .bind(&c.token_id)
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
            order: "created_at,token_id".into(),
            created_at: last.created_at,
            token_id: last.token_id.clone(),
        })?))
    } else {
        None
    };
    recheck_credential(&mut tx, actor).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(SessionPage {
        items: rows
            .into_iter()
            .map(|r| Session {
                token_id: r.token_id,
                is_current: credential_kind == "browser" && r.family_id == actor.credential_id,
                created_at: r.created_at.to_rfc3339(),
                last_used: r.last_used.map(|v| v.to_rfc3339()),
                expires_at: r.expires_at.to_rfc3339(),
                user_agent: r.user_agent,
                ip_address: r.ip_address,
                country_code: r.country_code,
                oauth_client_name: r.oauth_client_name,
            })
            .collect(),
        next_cursor,
    })
}

pub fn project(result: Result<SessionPage, Error>) -> SessionListOutcome {
    match result {
        Ok(page) => SessionListOutcome::Available { page },
        Err(Error::Forbidden) => SessionListOutcome::Forbidden,
        Err(Error::Invalid(_)) => SessionListOutcome::Invalid,
        Err(Error::Retryable) => SessionListOutcome::Retryable,
        Err(error) => {
            tracing::error!(%error, "session inventory failed");
            SessionListOutcome::Unavailable
        }
    }
}
