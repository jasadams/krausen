// SPDX-License-Identifier: AGPL-3.0-or-later
//! Account-owned optimistic settings mutations and atomic audit.
use crate::{
    ActorContext, Error, authorize_workspace, boundary_error, db_error,
    invitation::{AccountActor, lock_account_settings, recheck_credential},
};
use krausen_core::{DbPool, DbTx};
use krausen_types::account_read::AccountUpdateOutcome;

enum Change<'a> {
    Name(&'a str),
    Theme(&'a str),
    Workspace(&'a ActorContext),
}

pub async fn name(
    pool: &DbPool,
    actor: &AccountActor,
    expected: i64,
    name: &str,
) -> Result<i64, Error> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Invalid("name"));
    }
    apply(pool, actor, expected, Change::Name(name)).await
}

pub async fn theme(
    pool: &DbPool,
    actor: &AccountActor,
    expected: i64,
    theme: &str,
) -> Result<i64, Error> {
    if !["light", "dark", "system"].contains(&theme) {
        return Err(Error::Invalid("theme"));
    }
    apply(pool, actor, expected, Change::Theme(theme)).await
}

pub async fn workspace(pool: &DbPool, actor: &ActorContext, expected: i64) -> Result<i64, Error> {
    let account = AccountActor {
        actor_id: actor.actor_id.clone(),
        credential_id: actor.credential_id.clone(),
        access_expires_at: actor.access_expires_at,
    };
    apply(pool, &account, expected, Change::Workspace(actor)).await
}

async fn apply(
    pool: &DbPool,
    actor: &AccountActor,
    expected: i64,
    change: Change<'_>,
) -> Result<i64, Error> {
    if expected < 0 {
        return Err(Error::Invalid("settings revision"));
    }
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let current = lock_account_settings(&mut tx, actor).await?;
    if let Change::Workspace(workspace) = &change {
        authorize_workspace(&mut tx, workspace).await?;
        let sql = if matches!(&tx, DbTx::Postgres(_)) {
            "SELECT workspace_id FROM workspaces WHERE workspace_id=$1 AND status='active' FOR SHARE"
        } else {
            "SELECT workspace_id FROM workspaces WHERE workspace_id=$1 AND status='active'"
        };
        if scalar!(&mut tx, String, sql, &workspace.workspace_id)
            .map_err(db_error)?
            .is_none()
        {
            return Err(Error::Forbidden);
        }
    }
    if current != expected {
        return Err(Error::Conflict {
            expected,
            actual: current,
        });
    }
    let next = current
        .checked_add(1)
        .ok_or(Error::Invalid("settings revision"))?;
    let (pg, sqlite, value, operation) = match change {
        Change::Name(name) => (
            "UPDATE users SET name=$1,settings_revision=$2,updated_at=NOW() WHERE user_id=$3",
            "UPDATE users SET name=$1,settings_revision=$2,updated_at=CURRENT_TIMESTAMP WHERE user_id=$3",
            name.to_owned(),
            "account.name.v1",
        ),
        Change::Theme(theme) => (
            "UPDATE users SET extra_metadata=(COALESCE(extra_metadata::jsonb,'{}'::jsonb) || $1::jsonb)::json,settings_revision=$2,updated_at=NOW() WHERE user_id=$3",
            "UPDATE users SET extra_metadata=json_patch(COALESCE(extra_metadata,'{}'),$1),settings_revision=$2,updated_at=CURRENT_TIMESTAMP WHERE user_id=$3",
            serde_json::json!({"theme":theme}).to_string(),
            "account.theme.v1",
        ),
        Change::Workspace(workspace) => (
            "UPDATE users SET last_workspace_id=$1,settings_revision=$2,updated_at=NOW() WHERE user_id=$3",
            "UPDATE users SET last_workspace_id=$1,settings_revision=$2,updated_at=CURRENT_TIMESTAMP WHERE user_id=$3",
            workspace.workspace_id.clone(),
            "account.workspace-preference.v1",
        ),
    };
    let affected = match &mut tx {
        DbTx::Postgres(t) => sqlx::query(pg)
            .bind(&value)
            .bind(next)
            .bind(&actor.actor_id)
            .execute(&mut **t)
            .await
            .map(|r| r.rows_affected()),
        DbTx::Sqlite(t) => sqlx::query(sqlite)
            .bind(&value)
            .bind(next)
            .bind(&actor.actor_id)
            .execute(&mut **t)
            .await
            .map(|r| r.rows_affected()),
    }
    .map_err(db_error)?;
    if affected != 1 {
        return Err(Error::Forbidden);
    }
    query!(&mut tx, "INSERT INTO account_settings_changes (change_id,account_id,actor_id,operation,revision,recorded_at) VALUES ($1,$2,$3,$4,$5,$6)", uuid::Uuid::new_v4().to_string(), &actor.actor_id, &actor.actor_id, operation, next, chrono::Utc::now().timestamp()).map_err(db_error)?;
    recheck_credential(&mut tx, actor).await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(next)
}

pub fn project(result: Result<i64, Error>) -> AccountUpdateOutcome {
    match result {
        Ok(settings_revision) => AccountUpdateOutcome::Updated { settings_revision },
        Err(Error::Conflict { expected, actual }) => AccountUpdateOutcome::Conflict {
            expected,
            current: actual,
        },
        Err(Error::Forbidden) => AccountUpdateOutcome::Forbidden,
        Err(Error::Invalid(_)) => AccountUpdateOutcome::Invalid,
        Err(Error::Retryable) => AccountUpdateOutcome::Retryable,
        Err(error) => {
            tracing::error!(%error, "account update failed");
            AccountUpdateOutcome::Unavailable
        }
    }
}
